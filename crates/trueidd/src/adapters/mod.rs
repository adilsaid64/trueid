mod default;
mod file;
mod matcher;
mod mock;
mod onnx_face;
mod v4l;

pub use default::DefaultHealth;
pub use file::FileTemplateStore;
pub use matcher::CosineMatcher;
pub use mock::{
    AlwaysLiveLiveness, FullFrameFaceDetector, MockFaceEmbedder, MockVideoSource, PassthroughFaceAligner,
};
pub use onnx_face::build_face_embedder;
pub use v4l::V4lVideoSource;
