pub mod embedding;
pub mod error;
pub mod face;
pub mod frame;
pub mod templates;
pub mod user;

pub use embedding::{Embedding, EmbeddingSummary};
pub use face::{BoundingBox, FaceDetection, FaceLandmarks};
pub use frame::{Frame, FramePair, PixelFormat, StreamModality};
pub use templates::TemplateBundle;
pub use user::UserId;
