//! Domain types: users, frames, faces, embeddings, templates.
//!
//! No I/O. No ports. Application and adapters both depend on these.

pub mod embedding;
pub mod error;
pub mod face;
pub mod frame;
pub mod templates;
pub mod user;

pub use embedding::{Embedding, EmbeddingSummary};
pub use face::{BoundingBox, FaceDetection, FaceLandmarks};
pub use frame::{Frame, PixelFormat, StreamModality};
pub use templates::TemplateBundle;
pub use user::UserId;
