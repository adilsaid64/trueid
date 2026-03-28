pub mod application;
pub mod domain;
pub mod ports;

pub use application::app::{MultiFramePolicy, TrueIdApp};
pub use ports::CaptureSpec;
pub use application::error::AppError;
pub use domain::error::DomainError;
pub use domain::{Embedding, Frame, PixelFormat, StreamModality, UserId};
