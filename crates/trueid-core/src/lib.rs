pub mod application;
pub mod domain;
pub mod ports;

pub use application::app::{StreamLimits, StreamingPolicy, TrueIdApp, TrueIdAppDeps};
pub use application::error::AppError;
pub use application::verification_decision::{BurstVerificationOutcome, VerificationDecider};
pub use domain::error::DomainError;
pub use domain::{
    BoundingBox, Embedding, EmbeddingSummary, FaceDetection, FaceLandmarks, Frame, PixelFormat,
    StreamModality, TemplateBundle, UserId,
};
pub use ports::{
    AlignError, CaptureError, DetectError, FaceAligner, FaceDetector, FaceEmbedError, FaceEmbedder,
    FacePoseEstimator, LivenessChecker, LivenessError, PoseError, VideoSession, VideoSource,
};
