use trueid_core::ports::{AlignError, FaceAligner};
use trueid_core::{FaceDetection, Frame};

/// Clones the full input frame (no crop / warp). Use when testing the rest of the pipeline without
/// caring about face alignment, or when no detector/landmark path is needed.
///
/// Pass-through aligner for pipeline tests. Default wiring uses [`CropFaceAligner`](super::CropFaceAligner);
/// enable via `development.passthrough_aligner` in config.
pub struct PassthroughFaceAligner;

impl FaceAligner for PassthroughFaceAligner {
    fn align(&self, frame: &Frame, _detection: &FaceDetection) -> Result<Frame, AlignError> {
        Ok(frame.clone())
    }
}
