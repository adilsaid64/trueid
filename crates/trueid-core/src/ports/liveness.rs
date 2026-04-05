use thiserror::Error;

use crate::domain::Frame;

#[derive(Debug, Error)]
pub enum LivenessError {
    #[error("liveness: not a live face")]
    NotLive,

    #[error("{0}")]
    Failed(String),
}

pub trait LivenessChecker: Send + Sync {
    fn verify_live(&self, aligned_face: &Frame) -> Result<(), LivenessError>;
}
