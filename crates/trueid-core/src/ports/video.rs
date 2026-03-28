use thiserror::Error;

use crate::domain::{Frame, StreamModality};

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("{0}")]
    Failed(String),
}

pub trait VideoSource: Send + Sync {
    fn modality(&self) -> StreamModality;

    /// One frame. Implementations vary (single grab vs streaming); don't assume preview semantics.
    fn next_frame(&self) -> Result<Frame, CaptureError>;
}
