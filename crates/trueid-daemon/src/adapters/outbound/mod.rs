//! Driven adapters: implementations of core outbound ports.
//!
//! Each subdirectory matches a port in `trueid_core::ports`. Swap an impl here;
//! wire the choice in `crate::composition`.

mod face_aligner;
mod face_detector;
mod face_embedder;
mod face_pose;
pub(crate) mod frame_rgb;
mod health;
mod liveness;
mod matcher;
mod template_store;
mod video;

pub use face_aligner::{CropFaceAligner, PassthroughFaceAligner};
pub use face_detector::{FullFrameFaceDetector, build_face_detector};
pub use face_embedder::{MockFaceEmbedder, build_face_embedder};
pub use face_pose::{GeometricLandmarkPoseEstimator, PassthroughFacePoseEstimator};
pub use health::DefaultHealth;
pub use liveness::AlwaysLiveLiveness;
pub use matcher::CosineMatcher;
pub use template_store::FileTemplateStore;
pub use video::{MockVideoSource, V4lVideoSource};
