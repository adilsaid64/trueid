use thiserror::Error;

use crate::domain::Frame;

#[derive(Debug, Error)]
pub enum LivenessError {
    /// Failed liveness (e.g. photo or replay).
    #[error("liveness: not a live face")]
    NotLive,

    #[error("{0}")]
    Failed(String),
}

/// Anti-spoof check on the aligned face image.
pub trait LivenessChecker: Send + Sync {
    fn verify_live(&self, aligned_face: &Frame) -> Result<(), LivenessError>;
}
