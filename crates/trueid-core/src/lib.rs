pub mod application;
pub mod domain;
pub mod ports;

pub use application::app::{MultiFramePolicy, TrueIdApp, TrueIdAppDeps};
pub use application::error::AppError;
pub use application::verification_decision::{
    BurstVerificationOutcome, ModalityFusionConfig, VerificationDecider,
};
pub use domain::error::DomainError;
pub use domain::{
    BoundingBox, Embedding, EmbeddingSummary, FaceDetection, FaceLandmarks, Frame, PixelFormat,
    StreamModality, TemplateBundle, UserId,
};
pub use ports::{
    AlignError, CameraCapture, CaptureSpec, CapturedBurst, DetectError, FaceAligner, FaceDetector,
    FaceEmbedError, FaceEmbedder, LivenessChecker, LivenessError,
};
