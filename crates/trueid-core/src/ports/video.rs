use thiserror::Error;

use crate::domain::{Frame, StreamModality};

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("{0}")]
    Failed(String),
}

/// Warm-up frames to drop, then frames to return, in one `capture` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureSpec {
    pub warmup_discard: u32,
    pub frame_count: u32,
}

impl CaptureSpec {
    pub const fn new(warmup_discard: u32, frame_count: u32) -> Self {
        Self {
            warmup_discard,
            frame_count,
        }
    }

    pub const fn single() -> Self {
        Self {
            warmup_discard: 0,
            frame_count: 1,
        }
    }

    pub fn validate(self) -> Result<Self, CaptureError> {
        if self.frame_count == 0 {
            return Err(CaptureError::Failed(
                "CaptureSpec.frame_count must be >= 1".into(),
            ));
        }
        Ok(self)
    }
}

pub trait VideoSource: Send + Sync {
    fn modality(&self) -> StreamModality;

    /// Drop `warmup_discard` buffers, then return `frame_count` decoded frames in order.
    fn capture(&self, spec: CaptureSpec) -> Result<Vec<Frame>, CaptureError>;
}
