use thiserror::Error;

use crate::domain::{Frame, StreamModality};

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("{0}")]
    Failed(String),
}

pub trait VideoSource: Send + Sync {
    fn modality(&self) -> StreamModality;

    fn next_frame(&self) -> Result<Frame, CaptureError>;
}
