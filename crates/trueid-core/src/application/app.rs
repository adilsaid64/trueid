use std::sync::Arc;
use std::time::Instant;

use crate::domain::{Embedding, Frame, TemplateBundle, UserId};
use crate::ports::{
    CameraCapture, CaptureSpec, EmbeddingMatcher, FaceAligner, FaceDetector, FaceEmbedder, Health,
    HealthStatus, LivenessChecker, LivenessError, TemplateStore,
};

use super::error::AppError;

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

/// RGB/IR weights and fusion threshold when both template lists exist.
#[derive(Debug, Clone, Copy)]
pub struct ModalityFusionConfig {
    pub weight_rgb: f32,
    pub weight_ir: f32,
    pub fusion_threshold: f32,
}

impl Default for ModalityFusionConfig {
    fn default() -> Self {
        Self {
            weight_rgb: 0.45,
            weight_ir: 0.55,
            fusion_threshold: 0.5,
        }
    }
}

/// Templates that must match one probe: ceil(n/2).
fn template_quorum_required(template_count: usize) -> usize {
    template_count.div_ceil(2)
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
}

pub struct TrueIdApp {
    health: Arc<dyn Health>,
    camera: Arc<dyn CameraCapture>,
    detector: Arc<dyn FaceDetector>,
    aligner: Arc<dyn FaceAligner>,
    liveness: Arc<dyn LivenessChecker>,
    face_embedder: Arc<dyn FaceEmbedder>,
    template_store: Arc<dyn TemplateStore>,
    matcher: Arc<dyn EmbeddingMatcher>,
    capture: MultiFramePolicy,
    modality_fusion: ModalityFusionConfig,
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
            matcher: deps.matcher,
            capture: deps.capture,
            modality_fusion: deps.modality_fusion,
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

    /// Max similarity vs templates and whether quorum holds.
    fn evaluate_probe_vs_templates(
        &self,
        probe: Option<&Embedding>,
        templates: &[Embedding],
    ) -> (f32, bool) {
        let Some(p) = probe else {
            return (0.0, false);
        };
        if templates.is_empty() {
            return (0.0, false);
        }
        let required = template_quorum_required(templates.len());
        let mut pass_count = 0usize;
        let mut best_sim = 0.0f32;
        for t in templates {
            if let Some(s) = self.matcher.similarity(p, t) {
                best_sim = best_sim.max(s);
            }
            if self.matcher.matches(p, t) {
                pass_count += 1;
            }
        }
        let quorum = pass_count >= required;
        (best_sim, quorum)
    }

    /// Max similarity and any-frame quorum for one modality over a burst.
    fn aggregate_modality_for_verify(
        &self,
        probes: &[Option<Embedding>],
        templates: &[Embedding],
    ) -> (f32, bool) {
        let mut any_quorum = false;
        let mut max_best_sim = 0.0f32;
        for p in probes {
            let (sim, q) = self.evaluate_probe_vs_templates(p.as_ref(), templates);
            max_best_sim = max_best_sim.max(sim);
            any_quorum |= q;
        }
        (max_best_sim, any_quorum)
    }

    /// RGB-only, IR-only, or weighted fusion from per-modality aggregates. Both quorums → accept; else weighted sims.
    fn fused_match_from_aggregates(
        &self,
        bundle: &TemplateBundle,
        (sim_r, q_r): (f32, bool),
        (sim_i, q_i): (f32, bool),
        has_r: bool,
        has_i: bool,
    ) -> bool {
        let fusion = self.modality_fusion;

        if bundle.ir.is_empty() {
            return q_r;
        }
        if bundle.rgb.is_empty() {
            return q_i;
        }

        if !has_r && !has_i {
            return false;
        }
        if !has_r && has_i {
            return q_i;
        }
        if has_r && !has_i {
            return q_r;
        }

        if q_r && q_i {
            return true;
        }

        if !q_r && !q_i {
            return false;
        }

        let sr = sim_r.clamp(0.0, 1.0);
        let si = sim_i.clamp(0.0, 1.0);
        fusion.weight_rgb * sr + fusion.weight_ir * si >= fusion.fusion_threshold
    }

    /// Run `try_embed_from_frame` on each frame of one stream.
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
            fusion = ?self.modality_fusion,
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

        let agg_rgb = match probes_rgb.as_deref() {
            Some(rgb) if !rgb.is_empty() => self.aggregate_modality_for_verify(rgb, &bundle.rgb),
            _ => (0.0f32, false),
        };

        let agg_ir = match probes_ir.as_deref() {
            Some(ir) if !ir.is_empty() => self.aggregate_modality_for_verify(ir, &bundle.ir),
            _ => (0.0f32, false),
        };

        let has_rgb_probe = probes_rgb
            .as_ref()
            .is_some_and(|v| v.iter().any(|p| p.is_some()));

        let has_ir_probe = probes_ir
            .as_ref()
            .is_some_and(|v| v.iter().any(|p| p.is_some()));

        let accepted =
            self.fused_match_from_aggregates(&bundle, agg_rgb, agg_ir, has_rgb_probe, has_ir_probe);

        tracing::info!(
            accepted,
            rgb_quorum = agg_rgb.1,
            ir_quorum = agg_ir.1,
            best_sim_rgb = agg_rgb.0,
            best_sim_ir = agg_ir.0,
            has_rgb_probe,
            has_ir_probe,
            elapsed_ms = t_fuse.elapsed().as_millis(),
            "verify: fusion"
        );

        if accepted {
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

    /// Add templates from a new capture; user must already be enrolled.
    pub fn add_template(&self, user: &UserId) -> Result<(), AppError> {
        let span = tracing::info_span!("add_template", uid = user.0);
        let _g = span.enter();

        match self.health.status() {
            HealthStatus::Healthy => {}
            HealthStatus::Degraded { reason } => return Err(AppError::Unhealthy(reason)),
        }

        let mut bundle = self
            .template_store
            .load_all(user)?
            .filter(|b| b.has_any_enrollment())
            .ok_or(crate::domain::error::DomainError::NoEnrolledTemplate)?;
        tracing::debug!(
            existing_rgb = bundle.rgb.len(),
            existing_ir = bundle.ir.len(),
            "add_template: loaded existing"
        );

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
        });
        let err = app.enroll(&UserId(5000)).unwrap_err();
        assert!(err.to_string().contains("camera offline"));
    }
}
