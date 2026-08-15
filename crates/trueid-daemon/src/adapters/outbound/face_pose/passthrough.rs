use trueid_core::ports::{FacePoseEstimator, PoseError};
use trueid_core::{FaceDetection, Frame};

pub struct PassthroughFacePoseEstimator;

impl FacePoseEstimator for PassthroughFacePoseEstimator {
    fn check_frontal(
        &self,
        _aligned_face: &Frame,
        _detection: &FaceDetection,
    ) -> Result<(), PoseError> {
        Ok(())
    }
}
