//! Wiring for concrete ports (barrel re-exports).

mod face_aligner;
mod face_detector;
mod face_embedder;
mod health;
mod liveness;
mod matcher;
mod template_store;
mod video;

pub use face_aligner::PassthroughFaceAligner;
pub use face_detector::FullFrameFaceDetector;
pub use face_embedder::{build_face_embedder, MockFaceEmbedder};
pub use health::DefaultHealth;
pub use liveness::AlwaysLiveLiveness;
pub use matcher::CosineMatcher;
pub use template_store::FileTemplateStore;
pub use video::{MockVideoSource, V4lVideoSource};
