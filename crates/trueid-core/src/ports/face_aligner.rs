use thiserror::Error;

use crate::domain::{FaceDetection, Frame};

#[derive(Debug, Error)]
pub enum AlignError {
    #[error("{0}")]
    Failed(String),
}

/// Crop/warp to a standard face image for embed and liveness.
pub trait FaceAligner: Send + Sync {
    fn align(&self, frame: &Frame, detection: &FaceDetection) -> Result<Frame, AlignError>;
}
