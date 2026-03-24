use thiserror::Error;

use crate::domain::error::DomainError;
use crate::ports::{CaptureError, EmbedError, StoreError};

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("daemon is not healthy: {0}")]
    Unhealthy(&'static str),

    #[error(transparent)]
    Capture(#[from] CaptureError),

    #[error(transparent)]
    Embed(#[from] EmbedError),

    #[error(transparent)]
    Store(#[from] StoreError),
}
