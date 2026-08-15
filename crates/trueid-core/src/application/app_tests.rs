use super::*;
use crate::application::error::AppError;
use crate::domain::error::DomainError;
use crate::domain::{
    BoundingBox, Embedding, FaceDetection, Frame, PixelFormat, StreamModality, TemplateBundle,
};
use crate::ports::{
    AlignError, CaptureError, DetectError, EmbeddingMatcher, FaceAligner, FaceDetector,
    FaceEmbedError, FaceEmbedder, FacePoseEstimator, Health, HealthStatus, LivenessChecker,
    LivenessError, PoseError, StoreError, TemplateStore, VideoSession, VideoSource,
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

struct TestVideoSession {
    frame: Frame,
}

impl VideoSession for TestVideoSession {
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        Ok(self.frame.clone())
    }
}

struct TestVideo;

impl VideoSource for TestVideo {
    fn modality(&self) -> StreamModality {
        StreamModality::Rgb
    }

    fn open_session(&self) -> Result<Box<dyn VideoSession>, CaptureError> {
        Ok(Box::new(TestVideoSession {
            frame: Frame {
                modality: StreamModality::Rgb,
                width: 1,
                height: 1,
                format: PixelFormat::Gray8,
                bytes: vec![0],
            },
        }))
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

struct AlwaysFrontalPose;

impl FacePoseEstimator for AlwaysFrontalPose {
    fn check_frontal(
        &self,
        _aligned_face: &Frame,
        _detection: &FaceDetection,
    ) -> Result<(), PoseError> {
        Ok(())
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

fn deps(
    health: Arc<dyn Health>,
    detector: Arc<dyn FaceDetector>,
    store: Arc<dyn TemplateStore>,
    embed_out: Embedding,
) -> TrueIdAppDeps {
    TrueIdAppDeps {
        health,
        video: Arc::new(TestVideo),
        detector,
        aligner: Arc::new(CloneAligner),
        pose_estimator: Arc::new(AlwaysFrontalPose),
        liveness: Arc::new(AlwaysLive),
        face_embedder: Arc::new(ConstFaceEmbedder { out: embed_out }),
        template_store: store,
        matcher: Arc::new(ExactMatcher),
        streaming: StreamingPolicy::default(),
    }
}

fn app_with_store(store: Arc<MemoryStore>, embed_out: Embedding) -> TrueIdApp {
    TrueIdApp::new(deps(
        Arc::new(OkHealth),
        Arc::new(FullFrameDetector),
        store,
        embed_out,
    ))
}

#[test]
fn ping_ok_when_healthy() {
    let store = Arc::new(MemoryStore::empty());
    let app = app_with_store(store, Embedding::new(vec![1.0, 0.0]));
    assert!(app.ping().is_ok());
}

#[test]
fn ping_err_when_degraded() {
    let app = TrueIdApp::new(deps(
        Arc::new(BadHealth),
        Arc::new(FullFrameDetector),
        Arc::new(MemoryStore::empty()),
        Embedding::new(vec![1.0]),
    ));
    let err = app.ping().unwrap_err();
    assert!(err.to_string().contains("camera offline"));
}

#[test]
fn verify_no_template() {
    let store = Arc::new(MemoryStore::empty());
    let app = app_with_store(store, Embedding::new(vec![1.0, 0.0]));
    let err = app.verify(&UserId(1000)).unwrap_err();
    assert!(matches!(
        err,
        AppError::Domain(DomainError::NoEnrolledTemplate)
    ));
}

#[test]
fn verify_match() {
    let emb = Embedding::new(vec![0.5, 0.5, 0.0]);
    let store = Arc::new(MemoryStore::with_template(UserId(1000), emb.clone()));
    let app = app_with_store(store, emb);
    assert!(app.verify(&UserId(1000)).unwrap());
}

#[test]
fn verify_mismatch() {
    let store = Arc::new(MemoryStore::with_template(
        UserId(1000),
        Embedding::new(vec![1.0, 0.0, 0.0]),
    ));
    let app = app_with_store(store, Embedding::new(vec![0.0, 1.0, 0.0]));
    assert!(!app.verify(&UserId(1000)).unwrap());
}

#[test]
fn enroll_stores_template() {
    let emb = Embedding::new(vec![0.25, 0.75, 0.0]);
    let store = Arc::new(MemoryStore::empty());
    let app = app_with_store(Arc::clone(&store), emb.clone());
    app.enroll(&UserId(2000)).unwrap();
    let loaded = store.load_all(&UserId(2000)).unwrap().unwrap();
    assert_eq!(loaded.templates, vec![emb]);
}

#[test]
fn enroll_rejects_when_already_enrolled() {
    let emb = Embedding::new(vec![1.0, 0.0]);
    let store = Arc::new(MemoryStore::with_template(UserId(3000), emb.clone()));
    let app = app_with_store(store, Embedding::new(vec![0.0, 1.0]));
    let err = app.enroll(&UserId(3000)).unwrap_err();
    assert!(matches!(
        err,
        AppError::Domain(DomainError::AlreadyEnrolled)
    ));
}

#[test]
fn enroll_then_verify_succeeds() {
    let emb = Embedding::new(vec![9.0, 1.0, 0.0]);
    let store = Arc::new(MemoryStore::empty());
    let app = app_with_store(Arc::clone(&store), emb.clone());
    app.enroll(&UserId(4000)).unwrap();
    assert!(app.verify(&UserId(4000)).unwrap());
}

#[test]
fn verify_accepts_when_quorum_met_two_templates_one_match() {
    let t0 = Embedding::new(vec![1.0, 0.0, 0.0]);
    let t1 = Embedding::new(vec![0.0, 1.0, 0.0]);
    let store = Arc::new(MemoryStore::with_templates(
        UserId(7000),
        vec![t0, t1.clone()],
    ));
    let app = app_with_store(store, t1);
    assert!(app.verify(&UserId(7000)).unwrap());
}

#[test]
fn verify_rejects_when_quorum_not_met_three_templates_one_match() {
    let t0 = Embedding::new(vec![1.0, 0.0, 0.0]);
    let t1 = Embedding::new(vec![0.0, 1.0, 0.0]);
    let t2 = Embedding::new(vec![0.0, 0.0, 1.0]);
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
    let app = app_with_store(store, Embedding::new(vec![1.0, 0.0]));
    let err = app.add_template(&UserId(8000)).unwrap_err();
    assert!(matches!(
        err,
        AppError::Domain(DomainError::NoEnrolledTemplate)
    ));
}

#[test]
fn add_template_appends_without_removing_first() {
    let first = Embedding::new(vec![1.0, 0.0, 0.0]);
    let second = Embedding::new(vec![0.0, 1.0, 0.0]);
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

    let app = TrueIdApp::new(deps(
        Arc::new(OkHealth),
        Arc::new(NoFaceDetector),
        Arc::new(MemoryStore::empty()),
        Embedding::new(vec![1.0, 0.0]),
    ));
    let err = app.enroll(&UserId(6000)).unwrap_err();
    assert!(matches!(
        err,
        AppError::Domain(DomainError::NoUsableFaceInCapture)
    ));
}

#[test]
fn enroll_fails_when_unhealthy() {
    let app = TrueIdApp::new(deps(
        Arc::new(BadHealth),
        Arc::new(FullFrameDetector),
        Arc::new(MemoryStore::empty()),
        Embedding::new(vec![1.0, 0.0]),
    ));
    let err = app.enroll(&UserId(5000)).unwrap_err();
    assert!(err.to_string().contains("camera offline"));
}
