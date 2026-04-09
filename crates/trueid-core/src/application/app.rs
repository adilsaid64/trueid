use std::sync::Arc;
use std::time::Instant;

use crate::domain::{Embedding, Frame, TemplateBundle, UserId};
use crate::ports::{
    CameraCapture, CaptureSpec, EmbeddingMatcher, FaceAligner, FaceDetector, FaceEmbedder, Health,
    HealthStatus, LivenessChecker, LivenessError, TemplateStore,
};

use super::error::AppError;
use super::pipeline::{EnrollPipelineMode, VerifyPipelineMode};
use super::verification_decision::{VerificationDecider, template_quorum_required};

pub use super::verification_decision::ModalityFusionConfig;

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
    pub camera: Arc<dyn CameraCapture>,
    pub detector: Arc<dyn FaceDetector>,
    pub aligner: Arc<dyn FaceAligner>,
    pub liveness: Arc<dyn LivenessChecker>,
    pub face_embedder: Arc<dyn FaceEmbedder>,
    pub template_store: Arc<dyn TemplateStore>,
    pub matcher: Arc<dyn EmbeddingMatcher>,
    pub capture: MultiFramePolicy,
    pub modality_fusion: ModalityFusionConfig,
    pub enroll_pipeline: EnrollPipelineMode,
    pub verify_pipeline: VerifyPipelineMode,
}

pub struct TrueIdApp {
    health: Arc<dyn Health>,
    camera: Arc<dyn CameraCapture>,
    detector: Arc<dyn FaceDetector>,
    aligner: Arc<dyn FaceAligner>,
    liveness: Arc<dyn LivenessChecker>,
    face_embedder: Arc<dyn FaceEmbedder>,
    template_store: Arc<dyn TemplateStore>,
    verification: VerificationDecider,
    capture: MultiFramePolicy,
    enroll_pipeline: EnrollPipelineMode,
    verify_pipeline: VerifyPipelineMode,
}

impl TrueIdApp {
    pub fn new(deps: TrueIdAppDeps) -> Self {
        Self {
            health: deps.health,
            camera: deps.camera,
            detector: deps.detector,
            aligner: deps.aligner,
            liveness: deps.liveness,
            face_embedder: deps.face_embedder,
            template_store: deps.template_store,
            verification: VerificationDecider::new(deps.matcher.clone(), deps.modality_fusion),
            capture: deps.capture,
            enroll_pipeline: deps.enroll_pipeline,
            verify_pipeline: deps.verify_pipeline,
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

        match self.verify_pipeline {
            VerifyPipelineMode::Batch => self.verify_batch(user),
            VerifyPipelineMode::Streaming => Err(AppError::PipelineNotImplemented(
                "verify: streaming pipeline not implemented yet",
            )),
        }
    }

    fn verify_batch(&self, user: &UserId) -> Result<bool, AppError> {
        let spec = self.capture.verify.validate()?;
        tracing::info!(
            warmup_discard = spec.warmup_discard,
            frame_count = spec.frame_count,
            "verify: capture spec"
        );

        let t_cap = Instant::now();
        let burst = self.camera.capture(spec)?;
        let frames_rgb = burst.rgb;
        let frames_ir = burst.ir;

        tracing::info!(
            rgb_frames = frames_rgb.as_ref().map_or(0, |v| v.len()),
            ir_frames = frames_ir.as_ref().map_or(0, |v| v.len()),
            capture_ms = t_cap.elapsed().as_millis(),
            "verify: frames from camera"
        );

        let Some(bundle) = self.template_store.load_all(user)? else {
            return Err(crate::domain::error::DomainError::NoEnrolledTemplate.into());
        };
        if bundle.is_empty() {
            return Err(crate::domain::error::DomainError::NoEnrolledTemplate.into());
        }
        let n_rgb = bundle.rgb.len();
        let n_ir = bundle.ir.len();
        let req_rgb = if n_rgb > 0 {
            template_quorum_required(n_rgb)
        } else {
            0
        };
        let req_ir = if n_ir > 0 {
            template_quorum_required(n_ir)
        } else {
            0
        };

        tracing::info!(
            rgb_templates = n_rgb,
            ir_templates = n_ir,
            rgb_quorum = req_rgb,
            ir_quorum = req_ir,
            fusion = ?self.verification.modality_fusion(),
            template_dim = bundle
                .rgb
                .first()
                .or_else(|| bundle.ir.first())
                .map(|e| e.0.len())
                .unwrap_or(0),
            "verify: templates loaded"
        );

        let probes_rgb = if let Some(ref rgb_frames) = frames_rgb {
            let t_rgb = Instant::now();
            let p = self.modality_probes_from_frames(rgb_frames, "verify_rgb")?;
            tracing::info!(
                frames = p.len(),
                with_embedding = p.iter().filter(|x| x.is_some()).count(),
                elapsed_ms = t_rgb.elapsed().as_millis(),
                "verify: RGB burst processed"
            );
            Some(p)
        } else {
            None
        };

        let probes_ir = if let Some(ref ir_frames) = frames_ir {
            let t_ir = Instant::now();
            let p = self.modality_probes_from_frames(ir_frames, "verify_ir")?;
            tracing::info!(
                frames = p.len(),
                with_embedding = p.iter().filter(|x| x.is_some()).count(),
                elapsed_ms = t_ir.elapsed().as_millis(),
                "verify: IR burst processed"
            );
            Some(p)
        } else {
            None
        };

        let t_fuse = Instant::now();

        let outcome =
            self.verification
                .verify_burst(&bundle, probes_rgb.as_deref(), probes_ir.as_deref());

        tracing::info!(
            accepted = outcome.accepted,
            rgb_quorum = outcome.rgb_quorum,
            ir_quorum = outcome.ir_quorum,
            best_sim_rgb = outcome.best_sim_rgb,
            best_sim_ir = outcome.best_sim_ir,
            has_rgb_probe = outcome.has_rgb_probe,
            has_ir_probe = outcome.has_ir_probe,
            elapsed_ms = t_fuse.elapsed().as_millis(),
            "verify: fusion"
        );

        if outcome.accepted {
            tracing::info!(total_ms = t_fuse.elapsed().as_millis(), "verify: accept");
            return Ok(true);
        }
        tracing::info!(
            total_ms = t_fuse.elapsed().as_millis(),
            rgb_templates = n_rgb,
            ir_templates = n_ir,
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

        match self.enroll_pipeline {
            EnrollPipelineMode::Batch => self.enroll_batch(user),
            EnrollPipelineMode::Streaming => Err(AppError::PipelineNotImplemented(
                "enroll: streaming pipeline not implemented yet",
            )),
        }
    }

    fn enroll_batch(&self, user: &UserId) -> Result<(), AppError> {
        let spec = self.capture.enroll.validate()?;
        tracing::info!(
            warmup_discard = spec.warmup_discard,
            frame_count = spec.frame_count,
            "enroll: capture spec"
        );

        let t_cap = Instant::now();
        let burst = self.camera.capture(spec)?;
        tracing::info!(
            rgb_returned = burst.rgb.as_ref().map_or(0, |v| v.len()),
            has_ir = burst.ir.is_some(),
            capture_ms = t_cap.elapsed().as_millis(),
            "enroll: frames from camera"
        );
        let frames_rgb = burst.rgb.unwrap_or_default();
        let frames_ir = burst.ir.unwrap_or_default();

        let embeddings_rgb = if frames_rgb.is_empty() {
            Vec::new()
        } else {
            self.collect_embeddings(&frames_rgb, "enroll_rgb")?
        };
        let embeddings_ir = if frames_ir.is_empty() {
            Vec::new()
        } else {
            self.collect_embeddings(&frames_ir, "enroll_ir")?
        };

        if embeddings_rgb.is_empty() && embeddings_ir.is_empty() {
            tracing::warn!("enroll: no usable embeddings from any frame");
            return Err(crate::domain::error::DomainError::NoUsableFaceInCapture.into());
        }

        let template_rgb = if embeddings_rgb.is_empty() {
            None
        } else {
            Some(
                crate::domain::Embedding::try_average(&embeddings_rgb)
                    .ok_or(crate::domain::error::DomainError::EmbeddingAggregationFailed)?,
            )
        };
        let template_ir = if embeddings_ir.is_empty() {
            None
        } else {
            Some(
                crate::domain::Embedding::try_average(&embeddings_ir)
                    .ok_or(crate::domain::error::DomainError::EmbeddingAggregationFailed)?,
            )
        };

        tracing::info!(
            from_rgb_frames = embeddings_rgb.len(),
            from_ir_frames = embeddings_ir.len(),
            rgb_template_dim = template_rgb.as_ref().map(|t| t.0.len()).unwrap_or(0),
            ir_template_dim = template_ir.as_ref().map(|t| t.0.len()).unwrap_or(0),
            "enroll: templates averaged"
        );

        let mut bundle = TemplateBundle::empty();
        if let Some(t) = template_rgb {
            bundle.rgb.push(t);
        }
        if let Some(t) = template_ir {
            bundle.ir.push(t);
        }
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
            existing_rgb = bundle.rgb.len(),
            existing_ir = bundle.ir.len(),
            "add_template: loaded existing"
        );

        match self.enroll_pipeline {
            EnrollPipelineMode::Batch => self.add_template_batch(user, bundle),
            EnrollPipelineMode::Streaming => Err(AppError::PipelineNotImplemented(
                "add_template: streaming pipeline not implemented yet",
            )),
        }
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
        let burst = self.camera.capture(spec)?;
        tracing::info!(
            rgb_returned = burst.rgb.as_ref().map_or(0, |v| v.len()),
            has_ir = burst.ir.is_some(),
            capture_ms = t_cap.elapsed().as_millis(),
            "add_template: frames from camera"
        );

        let embeddings_rgb = match burst.rgb.as_deref() {
            Some(frames) if !frames.is_empty() => {
                self.collect_embeddings(frames, "add_template_rgb")?
            }
            _ => Vec::new(),
        };
        let embeddings_ir = match burst.ir.as_deref() {
            Some(frames) if !frames.is_empty() => {
                self.collect_embeddings(frames, "add_template_ir")?
            }
            _ => Vec::new(),
        };

        if embeddings_rgb.is_empty() && embeddings_ir.is_empty() {
            tracing::warn!("add_template: no usable embeddings from any frame");
            return Err(crate::domain::error::DomainError::NoUsableFaceInCapture.into());
        }

        if !embeddings_rgb.is_empty() {
            let new_rgb = crate::domain::Embedding::try_average(&embeddings_rgb)
                .ok_or(crate::domain::error::DomainError::EmbeddingAggregationFailed)?;
            bundle.rgb.push(new_rgb);
        }

        if !embeddings_ir.is_empty() {
            let new_ir = crate::domain::Embedding::try_average(&embeddings_ir)
                .ok_or(crate::domain::error::DomainError::EmbeddingAggregationFailed)?;
            bundle.ir.push(new_ir);
        }

        tracing::info!(
            rgb_templates = bundle.rgb.len(),
            ir_templates = bundle.ir.len(),
            template_dim = bundle.rgb.last().map(|e| e.0.len()).unwrap_or(0),
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
    use crate::application::pipeline::{EnrollPipelineMode, VerifyPipelineMode};
    use crate::domain::error::DomainError;
    use crate::domain::{
        BoundingBox, Embedding, FaceDetection, Frame, PixelFormat, StreamModality, TemplateBundle,
    };
    use crate::ports::{
        AlignError, CameraCapture, CaptureError, CaptureSpec, CapturedBurst, DetectError,
        EmbeddingMatcher, FaceAligner, FaceDetector, FaceEmbedError, FaceEmbedder, Health,
        HealthStatus, LivenessChecker, LivenessError, StoreError, TemplateStore,
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

    struct TestCamera;
    impl CameraCapture for TestCamera {
        fn capture(&self, spec: CaptureSpec) -> Result<CapturedBurst, CaptureError> {
            let spec = spec.validate()?;
            let f = Frame {
                modality: StreamModality::Rgb,
                width: 1,
                height: 1,
                format: PixelFormat::Gray8,
                bytes: vec![0],
            };
            Ok(CapturedBurst {
                rgb: Some(vec![f; spec.frame_count as usize]),
                ir: None,
            })
        }
    }

    struct TestCameraRgbIr;
    impl CameraCapture for TestCameraRgbIr {
        fn capture(&self, spec: CaptureSpec) -> Result<CapturedBurst, CaptureError> {
            let spec = spec.validate()?;
            let n = spec.frame_count as usize;
            let rgb = Frame {
                modality: StreamModality::Rgb,
                width: 1,
                height: 1,
                format: PixelFormat::Gray8,
                bytes: vec![0],
            };
            let ir = Frame {
                modality: StreamModality::Ir,
                width: 1,
                height: 1,
                format: PixelFormat::Gray8,
                bytes: vec![1],
            };
            Ok(CapturedBurst {
                rgb: Some(vec![rgb; n]),
                ir: Some(vec![ir; n]),
            })
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
            Self::with_rgb_templates(user, vec![emb])
        }

        fn with_rgb_templates(user: UserId, rgb: Vec<Embedding>) -> Self {
            let mut m = std::collections::HashMap::new();
            m.insert(user, TemplateBundle { rgb, ir: vec![] });
            Self {
                inner: std::sync::Mutex::new(m),
            }
        }

        fn with_templates(user: UserId, templates: Vec<Embedding>) -> Self {
            Self::with_rgb_templates(user, templates)
        }

        fn with_rgb_ir(user: UserId, rgb: Vec<Embedding>, ir: Vec<Embedding>) -> Self {
            let mut m = std::collections::HashMap::new();
            m.insert(user, TemplateBundle { rgb, ir });
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

    /// Test matcher: high-x templates quorum with low similarity; others do not.
    struct AsymmetricWeakRgbMatcher;
    impl EmbeddingMatcher for AsymmetricWeakRgbMatcher {
        fn matches(&self, _probe: &Embedding, enrolled: &Embedding) -> bool {
            enrolled.0.first().copied().unwrap_or(0.0) >= 0.99
        }

        fn similarity(&self, _probe: &Embedding, enrolled: &Embedding) -> Option<f32> {
            if enrolled.0.first().copied().unwrap_or(0.0) >= 0.99 {
                Some(0.36)
            } else {
                Some(0.2)
            }
        }
    }

    fn app_with_store(store: Arc<MemoryStore>, embed_out: Embedding) -> TrueIdApp {
        let template_store: Arc<dyn TemplateStore> = store;
        TrueIdApp::new(super::TrueIdAppDeps {
            health: Arc::new(OkHealth),
            camera: Arc::new(TestCamera),
            detector: Arc::new(FullFrameDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder { out: embed_out }),
            template_store,
            matcher: Arc::new(ExactMatcher),
            capture: MultiFramePolicy::default(),
            modality_fusion: ModalityFusionConfig::default(),
            enroll_pipeline: EnrollPipelineMode::Batch,
            verify_pipeline: VerifyPipelineMode::Batch,
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
            camera: Arc::new(TestCamera),
            detector: Arc::new(FullFrameDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder {
                out: Embedding(vec![1.0]),
            }),
            template_store: store,
            matcher: Arc::new(ExactMatcher),
            capture: MultiFramePolicy::default(),
            modality_fusion: ModalityFusionConfig::default(),
            enroll_pipeline: EnrollPipelineMode::Batch,
            verify_pipeline: VerifyPipelineMode::Batch,
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
        assert_eq!(loaded.rgb, vec![emb]);
        assert!(loaded.ir.is_empty());
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
    fn verify_fusion_weighted_score_ignores_quorum_as_one() {
        let probe = Embedding(vec![0.5, 0.5, 0.5]);
        let t_rgb = Embedding(vec![1.0, 0.0, 0.0]);
        let t_ir = Embedding(vec![0.0, 0.0, 1.0]);
        let store = Arc::new(MemoryStore::with_rgb_ir(
            UserId(7100),
            vec![t_rgb],
            vec![t_ir],
        ));
        let template_store: Arc<dyn TemplateStore> = store;
        let app = TrueIdApp::new(super::TrueIdAppDeps {
            health: Arc::new(OkHealth),
            camera: Arc::new(TestCameraRgbIr),
            detector: Arc::new(FullFrameDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder { out: probe.clone() }),
            template_store,
            matcher: Arc::new(AsymmetricWeakRgbMatcher),
            capture: MultiFramePolicy::default(),
            modality_fusion: ModalityFusionConfig::default(),
            enroll_pipeline: EnrollPipelineMode::Batch,
            verify_pipeline: VerifyPipelineMode::Batch,
        });
        assert!(!app.verify(&UserId(7100)).unwrap());
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
        assert_eq!(all.rgb.len(), 2);
        assert_eq!(all.rgb[0], first);
        assert_eq!(all.rgb[1], second);
        assert!(all.ir.is_empty());
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
            camera: Arc::new(TestCamera),
            detector: Arc::new(NoFaceDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder {
                out: Embedding(vec![1.0, 0.0]),
            }),
            template_store: store,
            matcher: Arc::new(ExactMatcher),
            capture: MultiFramePolicy::default(),
            modality_fusion: ModalityFusionConfig::default(),
            enroll_pipeline: EnrollPipelineMode::Batch,
            verify_pipeline: VerifyPipelineMode::Batch,
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
            camera: Arc::new(TestCamera),
            detector: Arc::new(FullFrameDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder {
                out: Embedding(vec![1.0, 0.0]),
            }),
            template_store: store,
            matcher: Arc::new(ExactMatcher),
            capture: MultiFramePolicy::default(),
            modality_fusion: ModalityFusionConfig::default(),
            enroll_pipeline: EnrollPipelineMode::Batch,
            verify_pipeline: VerifyPipelineMode::Batch,
        });
        let err = app.enroll(&UserId(5000)).unwrap_err();
        assert!(err.to_string().contains("camera offline"));
    }

    #[test]
    fn enroll_streaming_returns_not_implemented() {
        let store = Arc::new(MemoryStore::empty());
        let template_store: Arc<dyn TemplateStore> = store;
        let app = TrueIdApp::new(super::TrueIdAppDeps {
            health: Arc::new(OkHealth),
            camera: Arc::new(TestCamera),
            detector: Arc::new(FullFrameDetector),
            aligner: Arc::new(CloneAligner),
            liveness: Arc::new(AlwaysLive),
            face_embedder: Arc::new(ConstFaceEmbedder {
                out: Embedding(vec![1.0, 0.0]),
            }),
            template_store,
            matcher: Arc::new(ExactMatcher),
            capture: MultiFramePolicy::default(),
            modality_fusion: ModalityFusionConfig::default(),
            enroll_pipeline: EnrollPipelineMode::Streaming,
            verify_pipeline: VerifyPipelineMode::Batch,
        });
        let err = app.enroll(&UserId(42)).unwrap_err();
        assert!(matches!(err, AppError::PipelineNotImplemented(_)));
    }
}
