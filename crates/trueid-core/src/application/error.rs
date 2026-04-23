use thiserror::Error;

use crate::domain::error::DomainError;
use crate::ports::{
    AlignError, CaptureError, DetectError, FaceEmbedError, LivenessError, PoseError, StoreError,
};

#[derive(Debug, Error)]
pub enum AppError {
    #[error("pipeline not implemented: {0}")]
    PipelineNotImplemented(&'static str),

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
    Pose(#[from] PoseError),

    #[error(transparent)]
    FaceEmbed(#[from] FaceEmbedError),

    #[error(transparent)]
    Store(#[from] StoreError),
}
