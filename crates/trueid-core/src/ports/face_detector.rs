use thiserror::Error;

use crate::domain::{FaceDetection, Frame};

#[derive(Debug, Error)]
pub enum DetectError {
    #[error("{0}")]
    Failed(String),
}

pub trait FaceDetector: Send + Sync {
    fn detect_primary(&self, frame: &Frame) -> Result<Option<FaceDetection>, DetectError>;
}
