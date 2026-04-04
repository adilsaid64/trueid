use std::sync::Arc;
use std::time::Instant;

use crate::domain::{Embedding, Frame, UserId};
use crate::ports::{
    CaptureSpec, EmbeddingMatcher, FaceAligner, FaceDetector, FaceEmbedder, Health, HealthStatus,
    LivenessChecker, LivenessError, TemplateStore, VideoSource,
};

use super::error::AppError;

/// Warm-up and burst length for enroll vs verify.
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

/// Minimum number of templates that must match a single probe for that frame to count toward verify.
/// This is ceil(n/2): e.g. 1→1, 2→1, 3→2, 4→2 (at least 50% of templates).
fn template_quorum_required(template_count: usize) -> usize {
    template_count.div_ceil(2)
}

/// Wired dependencies for [`TrueIdApp`].
pub struct TrueIdAppDeps {
    pub health: Arc<dyn Health>,
    pub video_rgb: Arc<dyn VideoSource>,
    pub video_ir: Option<Arc<dyn VideoSource>>,
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
    video_rgb: Arc<dyn VideoSource>,
    video_ir: Option<Arc<dyn VideoSource>>,
    detector: Arc<dyn FaceDetector>,
    aligner: Arc<dyn FaceAligner>,
    liveness: Arc<dyn LivenessChecker>,
    face_embedder: Arc<dyn FaceEmbedder>,
    template_store: Arc<dyn TemplateStore>,
    matcher: Arc<dyn EmbeddingMatcher>,
    capture: MultiFramePolicy,
}

impl TrueIdApp {
    pub fn new(deps: TrueIdAppDeps) -> Self {
        Self {
            health: deps.health,
            video_rgb: deps.video_rgb,
            video_ir: deps.video_ir,
            detector: deps.detector,
            aligner: deps.aligner,
            liveness: deps.liveness,
            face_embedder: deps.face_embedder,
            template_store: deps.template_store,
            matcher: deps.matcher,
            capture: deps.capture,
        }
    }

    /// Detect → align → liveness → embed. `None` if skipped (no face, not live, etc.).
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

        let spec = self.capture.verify.validate()?;
        tracing::info!(
            warmup_discard = spec.warmup_discard,
            frame_count = spec.frame_count,
            "verify: capture spec"
        );

        let t_cap = Instant::now();
        let frames_rgb = self.video_rgb.capture(spec)?;

        if let Some(ir) = &self.video_ir {
            tracing::debug!("verify: IR source available (not yet used)");
            let _ = ir.capture(spec)?; // TODO: make use of IR frames
        }

        tracing::info!(
            returned = frames_rgb.len(),
            capture_ms = t_cap.elapsed().as_millis(),
            "verify: frames from camera"
        );

        let Some(templates) = self.template_store.load_all(user)? else {
            return Err(crate::domain::error::DomainError::NoEnrolledTemplate.into());
        };
        if templates.is_empty() {
            return Err(crate::domain::error::DomainError::NoEnrolledTemplate.into());
        }
        let template_count = templates.len();
        let required_matches = template_quorum_required(template_count);
        tracing::info!(
            template_count,
            required_matches,
            quorum = "≥50% of templates must match this probe",
            template_dim = templates[0].0.len(),
            "verify: templates loaded"
        );

        let t_loop = Instant::now();

        for (i, frame) in frames_rgb.iter().enumerate() {
            let Some(probe) = self.try_embed_from_frame(frame)? else {
                tracing::debug!(frame_index = i, "verify: frame produced no embedding");
                continue;
            };

            let mut pass_count: usize = 0;
            let mut best_similarity: Option<f32> = None;
            for (ti, t) in templates.iter().enumerate() {
                let similarity = self.matcher.similarity(&probe, t);
                if let Some(s) = similarity {
                    best_similarity = Some(match best_similarity {
                        None => s,
                        Some(prev) => prev.max(s),
                    });
                }
                let passed = self.matcher.matches(&probe, t);
                if passed {
                    pass_count += 1;
                    tracing::debug!(
                        frame_index = i,
                        template_index = ti,
                        ?similarity,
                        "verify: template matched probe"
                    );
                } else {
                    tracing::info!(
                        frame_index = i,
                        template_index = ti,
                        ?similarity,
                        "verify: template did not match probe"
                    );
                }
            }

            let frame_accepted = pass_count >= required_matches;
            let probe_summary = probe.summary();
            tracing::info!(
                frame_index = i,
                pass_count,
                required_matches,
                template_count,
                frame_accepted,
                best_similarity = ?best_similarity,
                probe_min = probe_summary.min,
                probe_max = probe_summary.max,
                probe_mean = probe_summary.mean,
                probe_l2 = probe_summary.l2_norm,
                elapsed_ms = t_loop.elapsed().as_millis(),
                "verify: quorum result for frame"
            );

            if frame_accepted {
                tracing::info!(
                    total_ms = t_loop.elapsed().as_millis(),
                    pass_count,
                    required_matches,
                    "verify: accepted (quorum met on this frame)"
                );
                return Ok(true);
            }
        }
        tracing::info!(
            total_ms = t_loop.elapsed().as_millis(),
            required_matches,
            template_count,
            "verify: rejected (no frame met template quorum)"
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
            .is_some_and(|t| !t.is_empty())
        {
            return Err(crate::domain::error::DomainError::AlreadyEnrolled.into());
        }

        let spec = self.capture.enroll.validate()?;
        tracing::info!(
            warmup_discard = spec.warmup_discard,
            frame_count = spec.frame_count,
            "enroll: capture spec"
        );

        let t_cap = Instant::now();
        let frames = self.video_rgb.capture(spec)?;
        tracing::info!(
            returned = frames.len(),
            capture_ms = t_cap.elapsed().as_millis(),
            "enroll: frames from camera"
        );

        let mut embeddings = Vec::with_capacity(frames.len());
        for (i, frame) in frames.iter().enumerate() {
            if let Some(e) = self.try_embed_from_frame(frame)? {
                tracing::debug!(
                    frame_index = i,
                    dim = e.0.len(),
                    "enroll: frame contributed embedding"
                );
                embeddings.push(e);
            } else {
                tracing::debug!(frame_index = i, "enroll: frame skipped");
            }
        }
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
        self.template_store
            .save_all(user, std::slice::from_ref(&template))?;
        tracing::info!("enroll: stored ok");
        Ok(())
    }

    /// Append a new template from a fresh capture (user must already be enrolled).
    /// Existing templates are kept; verification accepts a probe if it matches any template.
    pub fn add_template(&self, user: &UserId) -> Result<(), AppError> {
        let span = tracing::info_span!("add_template", uid = user.0);
        let _g = span.enter();

        match self.health.status() {
            HealthStatus::Healthy => {}
            HealthStatus::Degraded { reason } => return Err(AppError::Unhealthy(reason)),
        }

        let mut templates = self
            .template_store
            .load_all(user)?
            .filter(|t| !t.is_empty())
            .ok_or(crate::domain::error::DomainError::NoEnrolledTemplate)?;
        tracing::debug!(
            existing_templates = templates.len(),
            "add_template: loaded existing"
        );

        let spec = self.capture.enroll.validate()?;
        tracing::info!(
            warmup_discard = spec.warmup_discard,
            frame_count = spec.frame_count,
            "add_template: capture spec"
        );

        let t_cap = Instant::now();
        let frames = self.video_rgb.capture(spec)?;
        tracing::info!(
            returned = frames.len(),
            capture_ms = t_cap.elapsed().as_millis(),
            "add_template: frames from camera"
        );

        let mut embeddings = Vec::with_capacity(frames.len());
        for (i, frame) in frames.iter().enumerate() {
            if let Some(e) = self.try_embed_from_frame(frame)? {
                tracing::debug!(
                    frame_index = i,
                    dim = e.0.len(),
                    "add_template: frame contributed embedding"
                );
                embeddings.push(e);
            } else {
                tracing::debug!(frame_index = i, "add_template: frame skipped");
            }
        }
        if embeddings.is_empty() {
            tracing::warn!("add_template: no usable embeddings from any frame");
            return Err(crate::domain::error::DomainError::NoUsableFaceInCapture.into());
        }
        let new_template = crate::domain::Embedding::try_average(&embeddings)
            .ok_or(crate::domain::error::DomainError::EmbeddingAggregationFailed)?;
        tracing::info!(
            from_frames = embeddings.len(),
            template_dim = new_template.0.len(),
            "add_template: new template averaged"
        );
        templates.push(new_template);
        self.template_store.save_all(user, &templates)?;
        tracing::info!(total_templates = templates.len(), "add_template: stored ok");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::error::AppError;
    use crate::domain::error::DomainError;
    use crate::domain::{
        BoundingBox, Embedding, FaceDetection, Frame, PixelFormat, StreamModality,
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

    struct TestFrame;
    impl VideoSource for TestFrame {
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

    /// Treats the full frame as one face (development / tests only).
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
        inner: std::sync::Mutex<std::collections::HashMap<UserId, Vec<Embedding>>>,
    }

    impl MemoryStore {
        fn with_template(user: UserId, emb: Embedding) -> Self {
            Self::with_templates(user, vec![emb])
        }

        fn with_templates(user: UserId, templates: Vec<Embedding>) -> Self {
            let mut m = std::collections::HashMap::new();
            m.insert(user, templates);
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
        fn load_all(&self, user: &UserId) -> Result<Option<Vec<Embedding>>, StoreError> {
            Ok(self.inner.lock().unwrap().get(user).cloned())
        }

        fn save_all(&self, user: &UserId, templates: &[Embedding]) -> Result<(), StoreError> {
            self.inner.lock().unwrap().insert(*user, templates.to_vec());
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
            video_rgb: Arc::new(TestFrame),
            video_ir: None,
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
    fn template_quorum_required_is_ceil_half() {
        assert_eq!(super::template_quorum_required(1), 1);
        assert_eq!(super::template_quorum_required(2), 1);
        assert_eq!(super::template_quorum_required(3), 2);
        assert_eq!(super::template_quorum_required(4), 2);
        assert_eq!(super::template_quorum_required(5), 3);
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
            video_rgb: Arc::new(TestFrame),
            video_ir: None,
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
        let loaded = store.load_all(&UserId(2000)).unwrap();
        assert_eq!(loaded, Some(vec![emb]));
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
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], first);
        assert_eq!(all[1], second);
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
            video_rgb: Arc::new(TestFrame),
            video_ir: None,
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
            video_rgb: Arc::new(TestFrame),
            video_ir: None,
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
