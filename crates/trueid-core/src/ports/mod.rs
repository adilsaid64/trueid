pub mod embedder;
pub mod health;
pub mod matcher;
pub mod template_store;
pub mod video;

pub use embedder::{EmbedError, Embedder};
pub use health::{Health, HealthStatus};
pub use matcher::EmbeddingMatcher;
pub use template_store::{StoreError, TemplateStore};
pub use video::{CaptureError, VideoSource};
