use trueid_core::ports::{AlignError, FaceAligner};
use trueid_core::{FaceDetection, Frame};

/// Returns a copy of the frame (no real warp).
pub struct PassthroughFaceAligner;

impl FaceAligner for PassthroughFaceAligner {
    fn align(&self, frame: &Frame, _detection: &FaceDetection) -> Result<Frame, AlignError> {
        Ok(frame.clone())
    }
}
