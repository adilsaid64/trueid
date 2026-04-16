use thiserror::Error;

use crate::domain::{Frame, StreamModality};

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("{0}")]
    Failed(String),
}

/// One open camera stream: pull frames sequentially until the session is dropped.
pub trait VideoSession: Send {
    fn next_frame(&mut self) -> Result<Frame, CaptureError>;
}

/// Opens exclusive streaming sessions (see [`VideoSession`]).
pub trait VideoSource: Send + Sync {
    fn modality(&self) -> StreamModality;

    fn open_session(&self) -> Result<Box<dyn VideoSession>, CaptureError>;
}
