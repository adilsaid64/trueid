use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("biometric verification is not available: {0}")]
    BiometricUnavailable(String),
}
