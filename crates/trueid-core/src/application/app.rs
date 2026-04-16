use std::sync::Arc;
use std::time::Instant;

use crate::domain::{Embedding, Frame, TemplateBundle, UserId};
use crate::ports::{
    CaptureSpec, EmbeddingMatcher, FaceAligner, FaceDetector, FaceEmbedder, Health, HealthStatus,
    LivenessChecker, LivenessError, TemplateStore, VideoSource,
};

use super::error::AppError;
use super::verification_decision::{VerificationDecider, template_quorum_required};

/// Enroll vs verify capture lengths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiFramePolicy {
    pub enroll: CaptureSpec,
    pub verify: CaptureSpec,
}

impl Default for MultiFramePolicy {
    fn default() -> Self {
        Self {
            enroll: CaptureSpec::new(2, 5),
            verify: CaptureSpec::new(2, 3),
        }
    }
}

/// [`TrueIdApp`] dependencies.
pub struct TrueIdAppDeps {
    pub health: Arc<dyn Health>,
    pub video: Arc<dyn VideoSource>,
    pub detector: Arc<dyn FaceDetector>,
    pub aligner: Arc<dyn FaceAligner>,
    pub liveness: Arc<dyn LivenessChecker>,
    pub face_embedder: Arc<dyn FaceEmbedder>,
    pub template_store: Arc<dyn TemplateStore>,
    pub matcher: Arc<dyn EmbeddingMatcher>,
    pub capture: MultiFramePolicy,
}

pub struct TrueIdApp {
    health: Arc<dyn Health>,
    video: Arc<dyn VideoSource>,
    detector: Arc<dyn FaceDetector>,
    aligner: Arc<dyn FaceAligner>,
    liveness: Arc<dyn LivenessChecker>,
    face_embedder: Arc<dyn FaceEmbedder>,
    template_store: Arc<dyn TemplateStore>,
    verification: VerificationDecider,
    capture: MultiFramePolicy,
}

impl TrueIdApp {
    pub fn new(deps: TrueIdAppDeps) -> Self {
        Self {
            health: deps.health,
            video: deps.video,
            detector: deps.detector,
            aligner: deps.aligner,
            liveness: deps.liveness,
            face_embedder: deps.face_embedder,
            template_store: deps.template_store,
            verification: VerificationDecider::new(deps.matcher.clone()),
            capture: deps.capture,
        }
    }

    /// Detect → align → liveness → embed. `None` if skipped (no face, not live, etc.).
    ///
    /// Single supported path from a captured `Frame` to an embedding (batch and future streaming).
    fn try_embed_from_frame(&self, frame: &Frame) -> Result<Option<Embedding>, AppError> {
        let t0 = Instant::now();
        let Some(det) = self.detector.detect_primary(frame)? else {
            tracing::debug!(
                w = frame.width,
                h = frame.height,
                elapsed_ms = t0.elapsed().as_millis(),
                "pipeline: detect → no face"
            );
            return Ok(None);
        };
        tracing::debug!(
            w = frame.width,
            h = frame.height,
            bbox = ?det.bbox,
            has_landmarks = det.landmarks.is_some(),
            "pipeline: detect → face"
        );

        let t_align = Instant::now();
        let aligned = self.aligner.align(frame, &det)?;
        tracing::trace!(
            elapsed_ms = t_align.elapsed().as_millis(),
            "pipeline: align ok"
        );

        match self.liveness.verify_live(&aligned) {
            Ok(()) => {}
            Err(LivenessError::NotLive) => {
                tracing::debug!(
                    elapsed_ms = t0.elapsed().as_millis(),
                    "pipeline: liveness → not live"
                );
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }

        let t_emb = Instant::now();
        let emb = self.face_embedder.embed(&aligned)?;
        let summ = emb.summary();
        tracing::debug!(
            dim = emb.0.len(),
            embed_ms = t_emb.elapsed().as_millis(),
            total_ms = t0.elapsed().as_millis(),
            probe_min = summ.min,
            probe_max = summ.max,
            probe_mean = summ.mean,
            probe_l2 = summ.l2_norm,
            "pipeline: embed ok"
        );
        Ok(Some(emb))
    }

    /// Run `try_embed_from_frame` on each frame of one stream (batch path; streaming can reuse the same step per frame).
    fn modality_probes_from_frames(
        &self,
        frames: &[Frame],
        log_ctx: &'static str,
    ) -> Result<Vec<Option<Embedding>>, AppError> {
        let mut out = Vec::with_capacity(frames.len());
        for (i, frame) in frames.iter().enumerate() {
            let emb = self.try_embed_from_frame(frame)?;
            if emb.is_none() {
                tracing::debug!(
                    frame_index = i,
                    ctx = log_ctx,
                    "verify: modality frame produced no embedding"
                );
            }
            out.push(emb);
        }
        Ok(out)
    }

    pub fn ping(&self) -> Result<(), AppError> {
        match self.health.status() {
            HealthStatus::Healthy => Ok(()),
            HealthStatus::Degraded { reason } => Err(AppError::Unhealthy(reason)),
        }
    }

    pub fn verify(&self, user: &UserId) -> Result<bool, AppError> {
        let span = tracing::info_span!("verify", uid = user.0);
        let _g = span.enter();

        match self.health.status() {
            HealthStatus::Healthy => {}
            HealthStatus::Degraded { reason } => return Err(AppError::Unhealthy(reason)),
        }

        return self.verify_batch(user);
    }

    fn verify_batch(&self, user: &UserId) -> Result<bool, AppError> {
        let spec = self.capture.verify.validate()?;
        tracing::info!(
            warmup_discard = spec.warmup_discard,
            frame_count = spec.frame_count,
            "verify: capture spec"
        );

        let t_cap = Instant::now();
        let frames = self.video.capture(spec)?;

        tracing::info!(
            frame_count = frames.len(),
            capture_ms = t_cap.elapsed().as_millis(),
            "verify: frames from camera"
        );

        let Some(bundle) = self.template_store.load_all(user)? else {
            return Err(crate::domain::error::DomainError::NoEnrolledTemplate.into());
        };
        if bundle.is_empty() {
            return Err(crate::domain::error::DomainError::NoEnrolledTemplate.into());
        }
        let n_templates = bundle.templates.len();
        let quorum_need = template_quorum_required(n_templates);

        tracing::info!(
            templates = n_templates,
            quorum_required = quorum_need,
            template_dim = bundle.templates.first().map(|e| e.0.len()).unwrap_or(0),
            "verify: templates loaded"
        );

        let t_probe = Instant::now();
        let probes = self.modality_probes_from_frames(&frames, "verify")?;
        tracing::info!(
            frames = probes.len(),
            with_embedding = probes.iter().filter(|x| x.is_some()).count(),
            elapsed_ms = t_probe.elapsed().as_millis(),
            "verify: burst processed"
        );

        let t_match = Instant::now();
        let outcome = self.verification.verify_burst(&bundle, &probes);

        tracing::info!(
            accepted = outcome.accepted,
            quorum = outcome.quorum,
            best_sim = outcome.best_sim,
            has_probe = outcome.has_probe,
            elapsed_ms = t_match.elapsed().as_millis(),
            "verify: match"
        );

        if outcome.accepted {
            tracing::info!(total_ms = t_match.elapsed().as_millis(), "verify: accept");
            return Ok(true);
        }
        tracing::info!(
            total_ms = t_match.elapsed().as_millis(),
            templates = n_templates,
            "verify: reject"
        );
        Ok(false)
    }

    pub fn enroll(&self, user: &UserId) -> Result<(), AppError> {
        let span = tracing::info_span!("enroll", uid = user.0);
        let _g = span.enter();

        match self.health.status() {
            HealthStatus::Healthy => {}
            HealthStatus::Degraded { reason } => return Err(AppError::Unhealthy(reason)),
        }

        if self
            .template_store
            .load_all(user)?
            .is_some_and(|b| b.has_any_enrollment())
        {
            return Err(crate::domain::error::DomainError::AlreadyEnrolled.into());
        }

        return self.enroll_batch(user);
    }

    fn enroll_batch(&self, user: &UserId) -> Result<(), AppError> {
        let spec = self.capture.enroll.validate()?;
        tracing::info!(
            warmup_discard = spec.warmup_discard,
            frame_count = spec.frame_count,
            "enroll: capture spec"
        );

        let t_cap = Instant::now();

        let frames = self.video.capture(spec)?;
        tracing::info!(
            frame_count = frames.len(),
            capture_ms = t_cap.elapsed().as_millis(),
            "enroll: frames from camera"
        );

        let embeddings = if frames.is_empty() {
            Vec::new()
        } else {
            self.collect_embeddings(&frames, "enroll")?
        };

        if embeddings.is_empty() {
            tracing::warn!("enroll: no usable embeddings from any frame");
            return Err(crate::domain::error::DomainError::NoUsableFaceInCapture.into());
        }

        let template = crate::domain::Embedding::try_average(&embeddings)
            .ok_or(crate::domain::error::DomainError::EmbeddingAggregationFailed)?;

        tracing::info!(
            from_frames = embeddings.len(),
            template_dim = template.0.len(),
            "enroll: template averaged"
        );

        let mut bundle = TemplateBundle::empty();
        bundle.templates.push(template);
        self.template_store.save_all(user, &bundle)?;
        tracing::info!("enroll: stored ok");
        Ok(())
    }

    fn collect_embeddings(
        &self,
        frames: &[Frame],
        log_ctx: &'static str,
    ) -> Result<Vec<Embedding>, AppError> {
        let mut embeddings = Vec::with_capacity(frames.len());

        for (i, frame) in frames.iter().enumerate() {
            if let Some(e) = self.try_embed_from_frame(frame)? {
                tracing::debug!(
                    frame_index = i,
                    dim = e.0.len(),
                    ctx = log_ctx,
                    "capture: frame contributed embedding"
                );
                embeddings.push(e);
            } else {
                tracing::debug!(frame_index = i, ctx = log_ctx, "capture: frame skipped");
            }
        }

        Ok(embeddings)
    }

    /// Add templates from a new capture; user must already be enrolled.
    pub fn add_template(&self, user: &UserId) -> Result<(), AppError> {
        let span = tracing::info_span!("add_template", uid = user.0);
        let _g = span.enter();

        match self.health.status() {
            HealthStatus::Healthy => {}
            HealthStatus::Degraded { reason } => return Err(AppError::Unhealthy(reason)),
        }

        let bundle = self
            .template_store
            .load_all(user)?
            .filter(|b| b.has_any_enrollment())
            .ok_or(crate::domain::error::DomainError::NoEnrolledTemplate)?;
        tracing::debug!(
            existing_templates = bundle.templates.len(),
            "add_template: loaded existing"
        );

        return self.add_template_batch(user, bundle);
    }

    fn add_template_batch(
        &self,
        user: &UserId,
        mut bundle: TemplateBundle,
    ) -> Result<(), AppError> {
        let spec = self.capture.enroll.validate()?;
        tracing::info!(
            warmup_discard = spec.warmup_discard,
            frame_count = spec.frame_count,
            "add_template: capture spec"
        );

        let t_cap = Instant::now();
        let frames = self.video.capture(spec)?;
        tracing::info!(
            frame_count = frames.len(),
            capture_ms = t_cap.elapsed().as_millis(),
            "add_template: frames from camera"
        );

        let embeddings = if frames.is_empty() {
            Vec::new()
        } else {
            self.collect_embeddings(&frames, "add_template")?
        };

        if embeddings.is_empty() {
            tracing::warn!("add_template: no usable embeddings from any frame");
            return Err(crate::domain::error::DomainError::NoUsableFaceInCapture.into());
        }

        let new_t = crate::domain::Embedding::try_average(&embeddings)
            .ok_or(crate::domain::error::DomainError::EmbeddingAggregationFailed)?;
        bundle.templates.push(new_t);

        tracing::info!(
            templates = bundle.templates.len(),
            template_dim = bundle.templates.last().map(|e| e.0.len()).unwrap_or(0),
            "add_template: appended templates"
        );
        self.template_store.save_all(user, &bundle)?;
        tracing::info!("add_template: stored ok");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::error::AppError;
    use crate::domain::error::DomainError;
    use crate::domain::{
        BoundingBox, Embedding, FaceDetection, Frame, PixelFormat, StreamModality, TemplateBundle,
    };
    use crate::ports::{
        AlignError, CaptureError, CaptureSpec, DetectError, EmbeddingMatcher, FaceAligner,
        FaceDetector, FaceEmbedError, FaceEmbedder, Health, HealthStatus, LivenessChecker,
        LivenessError, StoreError, TemplateStore, VideoSource,
    };

    struct OkHealth;
    impl Health for OkHealth {
        fn status(&self) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    struct BadHealth;
    impl Health for BadHealth {
        fn status(&self) -> HealthStatus {
            HealthStatus::Degraded {
                reason: "camera offline",
            }
        }
    }

    struct TestVideo;

    impl VideoSource for TestVideo {
        fn modality(&self) -> StreamModality {
            StreamModality::Rgb
        }

        fn capture(&self, spec: CaptureSpec) -> Result<Vec<Frame>, CaptureError> {
            let spec = spec.validate()?;
            let f = Frame {
                modality: StreamModality::Rgb,
                width: 1,
                height: 1,
                format: PixelFormat::Gray8,
                bytes: vec![0],
            };
            Ok(vec![f; spec.frame_count as usize])
        }
    }

    struct ConstFaceEmbedder {
        out: Embedding,
    }

    impl FaceEmbedder for ConstFaceEmbedder {
        fn embed(&self, _frame: &Frame) -> Result<Embedding, FaceEmbedError> {
            Ok(self.out.clone())
        }
    }

    struct FullFrameDetector;

    impl FaceDetector for FullFrameDetector {
        fn detect_primary(&self, _frame: &Frame) -> Result<Option<FaceDetection>, DetectError> {
            Ok(Some(FaceDetection {
                bbox: BoundingBox::full_frame(),
                landmarks: None,
            }))
        }
    }

    struct CloneAligner;

    impl FaceAligner for CloneAligner {
        fn align(&self, frame: &Frame, _detection: &FaceDetection) -> Result<Frame, AlignError> {
            Ok(frame.clone())
        }
    }

    struct AlwaysLive;

    impl LivenessChecker for AlwaysLive {
        fn verify_live(&self, _aligned_face: &Frame) -> Result<(), LivenessError> {
            Ok(())
        }
    }

    struct MemoryStore {
        inner: std::sync::Mutex<std::collections::HashMap<UserId, TemplateBundle>>,
    }

    impl MemoryStore {
        fn with_template(user: UserId, emb: Embedding) -> Self {
            Self::with_templates(user, vec![emb])
        }

        fn with_templates(user: UserId, templates: Vec<Embedding>) -> Self {
            let mut m = std::collections::HashMap::new();
            m.insert(user, TemplateBundle { templates });
            Self {
                inner: std::sync::Mutex::new(m),
            }
        }

        fn empty() -> Self {
            Self {
                inner: std::sync::Mutex::new(std::collections::HashMap::new()),
            }
        }
    }

    impl TemplateStore for MemoryStore {
        fn load_all(&self, user: &UserId) -> Result<Option<TemplateBundle>, StoreError> {
            Ok(self.inner.lock().unwrap().get(user).cloned())
        }

        fn save_all(&self, user: &UserId, bundle: &TemplateBundle) -> Result<(), StoreError> {
            self.inner.lock().unwrap().insert(*user, bundle.clone());
            Ok(())
        }
    }

    struct ExactMatcher;
    impl EmbeddingMatcher for ExactMatcher {
        fn matches(&self, probe: &Embedding, enrolled: &Embedding) -> bool {
            probe == enrolled
        }

        fn similarity(&self, probe: &Embedding, enrolled: &Embedding) -> Option<f32> {
            Some(if probe == enrolled { 1.0 } else { 0.0 })
        }
    }

    fn app_with_store(store: Arc<MemoryStore>, embed_out: Embedding) -> TrueIdApp {
        let template_store: Arc<dyn TemplateStore> = store;
        TrueIdApp::new(super::TrueIdAppDeps {
            health: Arc::new(OkHealth),
            video: Arc::new(TestVideo),
            detector: Arc::new(FullFrameDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder { out: embed_out }),
            template_store,
            matcher: Arc::new(ExactMatcher),
            capture: MultiFramePolicy::default(),
        })
    }

    #[test]
    fn ping_ok_when_healthy() {
        let store = Arc::new(MemoryStore::empty());
        let app = app_with_store(store, Embedding(vec![1.0, 0.0]));
        assert!(app.ping().is_ok());
    }

    #[test]
    fn ping_err_when_degraded() {
        let store: Arc<dyn TemplateStore> = Arc::new(MemoryStore::empty());
        let app = TrueIdApp::new(super::TrueIdAppDeps {
            health: Arc::new(BadHealth),
            video: Arc::new(TestVideo),
            detector: Arc::new(FullFrameDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder {
                out: Embedding(vec![1.0]),
            }),
            template_store: store,
            matcher: Arc::new(ExactMatcher),
            capture: MultiFramePolicy::default(),
        });
        let err = app.ping().unwrap_err();
        assert!(err.to_string().contains("camera offline"));
    }

    #[test]
    fn verify_no_template() {
        let store = Arc::new(MemoryStore::empty());
        let app = app_with_store(store, Embedding(vec![1.0, 0.0]));
        let err = app.verify(&UserId(1000)).unwrap_err();
        assert!(matches!(
            err,
            AppError::Domain(DomainError::NoEnrolledTemplate)
        ));
    }

    #[test]
    fn verify_match() {
        let emb = Embedding(vec![0.5, 0.5, 0.0]);
        let store = Arc::new(MemoryStore::with_template(UserId(1000), emb.clone()));
        let app = app_with_store(store, emb);
        assert!(app.verify(&UserId(1000)).unwrap());
    }

    #[test]
    fn verify_mismatch() {
        let store = Arc::new(MemoryStore::with_template(
            UserId(1000),
            Embedding(vec![1.0, 0.0, 0.0]),
        ));
        let app = app_with_store(store, Embedding(vec![0.0, 1.0, 0.0]));
        assert!(!app.verify(&UserId(1000)).unwrap());
    }

    #[test]
    fn enroll_stores_template() {
        let emb = Embedding(vec![0.25, 0.75, 0.0]);
        let store = Arc::new(MemoryStore::empty());
        let app = app_with_store(Arc::clone(&store), emb.clone());
        app.enroll(&UserId(2000)).unwrap();
        let loaded = store.load_all(&UserId(2000)).unwrap().unwrap();
        assert_eq!(loaded.templates, vec![emb]);
    }

    #[test]
    fn enroll_rejects_when_already_enrolled() {
        let emb = Embedding(vec![1.0, 0.0]);
        let store = Arc::new(MemoryStore::with_template(UserId(3000), emb.clone()));
        let app = app_with_store(store, Embedding(vec![0.0, 1.0]));
        let err = app.enroll(&UserId(3000)).unwrap_err();
        assert!(matches!(
            err,
            AppError::Domain(DomainError::AlreadyEnrolled)
        ));
    }

    #[test]
    fn enroll_then_verify_succeeds() {
        let emb = Embedding(vec![9.0, 1.0, 0.0]);
        let store = Arc::new(MemoryStore::empty());
        let app = app_with_store(Arc::clone(&store), emb.clone());
        app.enroll(&UserId(4000)).unwrap();
        assert!(app.verify(&UserId(4000)).unwrap());
    }

    #[test]
    fn verify_accepts_when_quorum_met_two_templates_one_match() {
        let t0 = Embedding(vec![1.0, 0.0, 0.0]);
        let t1 = Embedding(vec![0.0, 1.0, 0.0]);
        let store = Arc::new(MemoryStore::with_templates(
            UserId(7000),
            vec![t0, t1.clone()],
        ));
        let app = app_with_store(store, t1);
        assert!(app.verify(&UserId(7000)).unwrap());
    }

    #[test]
    fn verify_rejects_when_quorum_not_met_three_templates_one_match() {
        let t0 = Embedding(vec![1.0, 0.0, 0.0]);
        let t1 = Embedding(vec![0.0, 1.0, 0.0]);
        let t2 = Embedding(vec![0.0, 0.0, 1.0]);
        let store = Arc::new(MemoryStore::with_templates(
            UserId(7001),
            vec![t0, t1, t2.clone()],
        ));
        let app = app_with_store(store, t2);
        assert!(!app.verify(&UserId(7001)).unwrap());
    }

    #[test]
    fn add_template_requires_prior_enrollment() {
        let store = Arc::new(MemoryStore::empty());
        let app = app_with_store(store, Embedding(vec![1.0, 0.0]));
        let err = app.add_template(&UserId(8000)).unwrap_err();
        assert!(matches!(
            err,
            AppError::Domain(DomainError::NoEnrolledTemplate)
        ));
    }

    #[test]
    fn add_template_appends_without_removing_first() {
        let first = Embedding(vec![1.0, 0.0, 0.0]);
        let second = Embedding(vec![0.0, 1.0, 0.0]);
        let store = Arc::new(MemoryStore::with_template(UserId(9000), first.clone()));
        let app = app_with_store(Arc::clone(&store), second.clone());
        app.add_template(&UserId(9000)).unwrap();
        let all = store.load_all(&UserId(9000)).unwrap().unwrap();
        assert_eq!(all.templates.len(), 2);
        assert_eq!(all.templates[0], first);
        assert_eq!(all.templates[1], second);
    }

    #[test]
    fn enroll_fails_when_no_face_detected() {
        struct NoFaceDetector;
        impl FaceDetector for NoFaceDetector {
            fn detect_primary(&self, _frame: &Frame) -> Result<Option<FaceDetection>, DetectError> {
                Ok(None)
            }
        }

        let store: Arc<dyn TemplateStore> = Arc::new(MemoryStore::empty());
        let app = TrueIdApp::new(super::TrueIdAppDeps {
            health: Arc::new(OkHealth),
            video: Arc::new(TestVideo),
            detector: Arc::new(NoFaceDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder {
                out: Embedding(vec![1.0, 0.0]),
            }),
            template_store: store,
            matcher: Arc::new(ExactMatcher),
            capture: MultiFramePolicy::default(),
        });
        let err = app.enroll(&UserId(6000)).unwrap_err();
        assert!(matches!(
            err,
            AppError::Domain(DomainError::NoUsableFaceInCapture)
        ));
    }

    #[test]
    fn enroll_fails_when_unhealthy() {
        let store: Arc<dyn TemplateStore> = Arc::new(MemoryStore::empty());
        let app = TrueIdApp::new(super::TrueIdAppDeps {
            health: Arc::new(BadHealth),
            video: Arc::new(TestVideo),
            detector: Arc::new(FullFrameDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder {
                out: Embedding(vec![1.0, 0.0]),
            }),
            template_store: store,
            matcher: Arc::new(ExactMatcher),
            capture: MultiFramePolicy::default(),
        });
        let err = app.enroll(&UserId(5000)).unwrap_err();
        assert!(err.to_string().contains("camera offline"));
    }
}
