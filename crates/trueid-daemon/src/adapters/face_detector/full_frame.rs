use trueid_core::ports::{DetectError, FaceDetector};
use trueid_core::{BoundingBox, FaceDetection, Frame};

pub struct FullFrameFaceDetector;

impl FaceDetector for FullFrameFaceDetector {
    fn detect_primary(&self, _frame: &Frame) -> Result<Option<FaceDetection>, DetectError> {
        Ok(Some(FaceDetection {
            bbox: BoundingBox::full_frame(),
            landmarks: None,
        }))
    }
}
