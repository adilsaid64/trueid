use trueid_core::ports::{AlignError, FaceAligner};
use trueid_core::{FaceDetection, Frame};

/// Returns a copy of the frame (no real warp).
///
/// Kept for tests or local experiments; the daemon uses [`super::CropFaceAligner`](super::CropFaceAligner) by default.
#[allow(dead_code)]
pub struct PassthroughFaceAligner;

impl FaceAligner for PassthroughFaceAligner {
    fn align(&self, frame: &Frame, _detection: &FaceDetection) -> Result<Frame, AlignError> {
        Ok(frame.clone())
    }
}
