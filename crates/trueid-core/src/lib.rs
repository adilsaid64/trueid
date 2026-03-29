pub mod application;
pub mod domain;
pub mod ports;

pub use application::app::{MultiFramePolicy, TrueIdApp};
pub use ports::{
    AlignError, CaptureSpec, DetectError, FaceAligner, FaceDetector, FaceEmbedError, FaceEmbedder,
    LivenessChecker, LivenessError,
};
pub use application::error::AppError;
pub use domain::error::DomainError;
pub use domain::{
    BoundingBox, Embedding, EmbeddingSummary, FaceDetection, FaceLandmarks, Frame, PixelFormat,
    StreamModality, UserId,
};
