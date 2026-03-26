mod default;
mod matcher;
mod memory;
mod mock;
mod v4l_video;

pub use default::{DefaultBiometric, DefaultHealth};
pub use matcher::CosineMatcher;
pub use memory::MemoryTemplateStore;
pub use mock::{MockEmbedder, MockVideoSource};
pub use v4l_video::V4lVideoSource;
