//! Outbound ports: traits the application uses to talk to the outside world.
//!
//! Implementations live in `trueid-daemon` under `adapters/outbound/`, one
//! subdirectory per port. Inbound entry points are [`crate::TrueIdApp`] methods,
//! not traits in this module.

pub mod face_aligner;
pub mod face_detector;
pub mod face_embedder;
pub mod face_pose;
pub mod health;
pub mod liveness;
pub mod matcher;
pub mod template_store;
pub mod video;

pub use face_aligner::{AlignError, FaceAligner};
pub use face_detector::{DetectError, FaceDetector};
pub use face_embedder::{FaceEmbedError, FaceEmbedder};
pub use face_pose::{FacePoseEstimator, PoseError};
pub use health::{Health, HealthStatus};
pub use liveness::{LivenessChecker, LivenessError};
pub use matcher::EmbeddingMatcher;
pub use template_store::{StoreError, TemplateStore};
pub use video::{CaptureError, VideoSession, VideoSource};
