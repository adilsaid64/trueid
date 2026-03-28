use thiserror::Error;

use crate::domain::error::DomainError;
use crate::ports::{
    AlignError, CaptureError, DetectError, EmbedError, LivenessError, StoreError,
};

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("daemon is not healthy: {0}")]
    Unhealthy(&'static str),

    #[error(transparent)]
    Capture(#[from] CaptureError),

    #[error(transparent)]
    Detect(#[from] DetectError),

    #[error(transparent)]
    Align(#[from] AlignError),

    #[error(transparent)]
    Liveness(#[from] LivenessError),

    #[error(transparent)]
    Embed(#[from] EmbedError),

    #[error(transparent)]
    Store(#[from] StoreError),
}
