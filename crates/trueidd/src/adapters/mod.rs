mod default;
mod file;
mod matcher;
mod mock;
mod onnx_face;
mod v4l_video;

pub use default::DefaultHealth;
pub use file::FileTemplateStore;
pub use matcher::CosineMatcher;
pub use mock::{
    AlwaysLiveLiveness, FullFrameFaceDetector, MockEmbedder, MockVideoSource, PassthroughFaceAligner,
};
pub use onnx_face::build_embedder;
pub use v4l_video::V4lVideoSource;
