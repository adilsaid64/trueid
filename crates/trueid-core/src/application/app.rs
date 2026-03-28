use std::sync::Arc;

use crate::domain::{Embedding, Frame, UserId};
use crate::ports::{
    CaptureSpec, Embedder, EmbeddingMatcher, FaceAligner, FaceDetector, Health, HealthStatus,
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

pub struct TrueIdApp {
    health: Arc<dyn Health>,
    video: Arc<dyn VideoSource>,
    detector: Arc<dyn FaceDetector>,
    aligner: Arc<dyn FaceAligner>,
    liveness: Arc<dyn LivenessChecker>,
    embedder: Arc<dyn Embedder>,
    template_store: Arc<dyn TemplateStore>,
    matcher: Arc<dyn EmbeddingMatcher>,
    capture: MultiFramePolicy,
}

impl TrueIdApp {
    pub fn new(
        health: Arc<dyn Health>,
        video: Arc<dyn VideoSource>,
        detector: Arc<dyn FaceDetector>,
        aligner: Arc<dyn FaceAligner>,
        liveness: Arc<dyn LivenessChecker>,
        embedder: Arc<dyn Embedder>,
        template_store: Arc<dyn TemplateStore>,
        matcher: Arc<dyn EmbeddingMatcher>,
        capture: MultiFramePolicy,
    ) -> Self {
        Self {
            health,
            video,
            detector,
            aligner,
            liveness,
            embedder,
            template_store,
            matcher,
            capture,
        }
    }

    /// Detect → align → liveness → embed. `None` if skipped (no face, not live, etc.).
    fn try_embed_from_frame(&self, frame: &Frame) -> Result<Option<Embedding>, AppError> {
        let Some(det) = self.detector.detect_primary(frame)? else {
            return Ok(None);
        };
        let aligned = self.aligner.align(frame, &det)?;
        match self.liveness.verify_live(&aligned) {
            Ok(()) => {}
            Err(LivenessError::NotLive) => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        Ok(Some(self.embedder.embed(&aligned)?))
    }

    pub fn ping(&self) -> Result<(), AppError> {
        match self.health.status() {
            HealthStatus::Healthy => Ok(()),
            HealthStatus::Degraded { reason } => Err(AppError::Unhealthy(reason)),
        }
    }

    pub fn verify(&self, user: &UserId) -> Result<bool, AppError> {
        match self.health.status() {
            HealthStatus::Healthy => {}
            HealthStatus::Degraded { reason } => return Err(AppError::Unhealthy(reason)),
        }

        let spec = self.capture.verify.validate()?;
        let frames = self.video.capture(spec)?;
        let Some(enrolled) = self.template_store.load(user)? else {
            return Err(crate::domain::error::DomainError::NoEnrolledTemplate.into());
        };

        for frame in frames {
            let Some(probe) = self.try_embed_from_frame(&frame)? else {
                continue;
            };
            if self.matcher.matches(&probe, &enrolled) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn enroll(&self, user: &UserId) -> Result<(), AppError> {
        match self.health.status() {
            HealthStatus::Healthy => {}
            HealthStatus::Degraded { reason } => return Err(AppError::Unhealthy(reason)),
        }

        if self.template_store.load(user)?.is_some() {
            return Err(crate::domain::error::DomainError::AlreadyEnrolled.into());
        }

        let spec = self.capture.enroll.validate()?;
        let frames = self.video.capture(spec)?;
        let mut embeddings = Vec::with_capacity(frames.len());
        for frame in &frames {
            if let Some(e) = self.try_embed_from_frame(frame)? {
                embeddings.push(e);
            }
        }
        if embeddings.is_empty() {
            return Err(crate::domain::error::DomainError::NoUsableFaceInCapture.into());
        }
        let template = crate::domain::Embedding::try_average(&embeddings).ok_or(
            crate::domain::error::DomainError::EmbeddingAggregationFailed,
        )?;
        self.template_store.save(user, &template)?;
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
        AlignError, CaptureError, CaptureSpec, DetectError, EmbedError, Embedder, EmbeddingMatcher,
        FaceAligner, FaceDetector, Health, HealthStatus, LivenessChecker, LivenessError, StoreError,
        TemplateStore, VideoSource,
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

    struct ConstEmbedder {
        out: Embedding,
    }

    impl Embedder for ConstEmbedder {
        fn embed(&self, _frame: &Frame) -> Result<Embedding, EmbedError> {
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
        inner: std::sync::Mutex<std::collections::HashMap<UserId, Embedding>>,
    }

    impl MemoryStore {
        fn with_template(user: UserId, emb: Embedding) -> Self {
            let mut m = std::collections::HashMap::new();
            m.insert(user, emb);
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
        fn load(&self, user: &UserId) -> Result<Option<Embedding>, StoreError> {
            Ok(self.inner.lock().unwrap().get(user).cloned())
        }

        fn save(&self, user: &UserId, embedding: &Embedding) -> Result<(), StoreError> {
            self.inner.lock().unwrap().insert(*user, embedding.clone());
            Ok(())
        }
    }

    struct ExactMatcher;
    impl EmbeddingMatcher for ExactMatcher {
        fn matches(&self, probe: &Embedding, enrolled: &Embedding) -> bool {
            probe == enrolled
        }
    }

    fn app_with_store(store: Arc<MemoryStore>, embed_out: Embedding) -> TrueIdApp {
        let template_store: Arc<dyn TemplateStore> = store;
        TrueIdApp::new(
            Arc::new(OkHealth),
            Arc::new(TestFrame),
            Arc::new(FullFrameDetector),
            Arc::new(CloneAligner),
            Arc::new(AlwaysLive),
            Arc::new(ConstEmbedder { out: embed_out }),
            template_store,
            Arc::new(ExactMatcher),
            MultiFramePolicy::default(),
        )
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
        let app = TrueIdApp::new(
            Arc::new(BadHealth),
            Arc::new(TestFrame),
            Arc::new(FullFrameDetector),
            Arc::new(CloneAligner),
            Arc::new(AlwaysLive),
            Arc::new(ConstEmbedder {
                out: Embedding(vec![1.0]),
            }),
            store,
            Arc::new(ExactMatcher),
            MultiFramePolicy::default(),
        );
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
        let loaded = store.load(&UserId(2000)).unwrap();
        assert_eq!(loaded, Some(emb));
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
    fn enroll_fails_when_no_face_detected() {
        struct NoFaceDetector;
        impl FaceDetector for NoFaceDetector {
            fn detect_primary(&self, _frame: &Frame) -> Result<Option<FaceDetection>, DetectError> {
                Ok(None)
            }
        }

        let store: Arc<dyn TemplateStore> = Arc::new(MemoryStore::empty());
        let app = TrueIdApp::new(
            Arc::new(OkHealth),
            Arc::new(TestFrame),
            Arc::new(NoFaceDetector),
            Arc::new(CloneAligner),
            Arc::new(AlwaysLive),
            Arc::new(ConstEmbedder {
                out: Embedding(vec![1.0, 0.0]),
            }),
            store,
            Arc::new(ExactMatcher),
            MultiFramePolicy::default(),
        );
        let err = app.enroll(&UserId(6000)).unwrap_err();
        assert!(matches!(
            err,
            AppError::Domain(DomainError::NoUsableFaceInCapture)
        ));
    }

    #[test]
    fn enroll_fails_when_unhealthy() {
        let store: Arc<dyn TemplateStore> = Arc::new(MemoryStore::empty());
        let app = TrueIdApp::new(
            Arc::new(BadHealth),
            Arc::new(TestFrame),
            Arc::new(FullFrameDetector),
            Arc::new(CloneAligner),
            Arc::new(AlwaysLive),
            Arc::new(ConstEmbedder {
                out: Embedding(vec![1.0, 0.0]),
            }),
            store,
            Arc::new(ExactMatcher),
            MultiFramePolicy::default(),
        );
        let err = app.enroll(&UserId(5000)).unwrap_err();
        assert!(err.to_string().contains("camera offline"));
    }
}
