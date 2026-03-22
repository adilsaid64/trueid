use thiserror::Error;

use crate::domain::error::DomainError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("daemon is not healthy: {0}")]
    Unhealthy(&'static str),
}
