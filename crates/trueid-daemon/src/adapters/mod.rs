//! Wiring for concrete ports (barrel re-exports).

mod face_aligner;
mod face_detector;
mod face_embedder;
mod health;
mod liveness;
mod matcher;
mod template_store;
mod video;

pub use face_aligner::{CropFaceAligner, PassthroughFaceAligner};
pub use face_detector::{FullFrameFaceDetector, build_face_detector};
pub use face_embedder::{MockFaceEmbedder, build_face_embedder};
pub use health::DefaultHealth;
pub use liveness::AlwaysLiveLiveness;
pub use matcher::CosineMatcher;
pub use template_store::FileTemplateStore;
pub use video::{MockVideoSource, ParallelRgbIrCameraCapture, RgbOnlyCameraCapture, V4lVideoSource};
