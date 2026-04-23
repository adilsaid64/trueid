use thiserror::Error;

use crate::domain::{FaceDetection, Frame};

#[derive(Debug, Error)]
pub enum PoseError {
    #[error("pose: face not sufficiently frontal")]
    NotFrontal,

    #[error("{0}")]
    Failed(String),
}

pub trait FacePoseEstimator: Send + Sync {
    fn check_frontal(
        &self,
        aligned_face: &Frame,
        detection: &FaceDetection,
    ) -> Result<(), PoseError>;
}
