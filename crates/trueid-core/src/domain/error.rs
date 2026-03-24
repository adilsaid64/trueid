use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("biometric verification is not available: {0}")]
    BiometricUnavailable(String),

    #[error("no enrolled template for this user")]
    NoEnrolledTemplate,

    #[error("verification score below threshold")]
    VerificationFailed,
}
