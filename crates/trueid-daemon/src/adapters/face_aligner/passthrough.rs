use trueid_core::ports::{AlignError, FaceAligner};
use trueid_core::{FaceDetection, Frame};

/// Clones the full input frame (no crop / warp). Use when testing the rest of the pipeline without
/// caring about face alignment, or when no detector/landmark path is needed.
///
/// Default daemon wiring uses [`CropFaceAligner`](super::CropFaceAligner); enable with `TRUEID_USE_PASSTHROUGH_ALIGNER=1`.
pub struct PassthroughFaceAligner;

impl FaceAligner for PassthroughFaceAligner {
    fn align(&self, frame: &Frame, _detection: &FaceDetection) -> Result<Frame, AlignError> {
        Ok(frame.clone())
    }
}
