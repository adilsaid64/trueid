use trueid_core::ports::{LivenessChecker, LivenessError};
use trueid_core::Frame;

/// Always passes liveness.
pub struct AlwaysLiveLiveness;

impl LivenessChecker for AlwaysLiveLiveness {
    fn verify_live(&self, _aligned_face: &Frame) -> Result<(), LivenessError> {
        Ok(())
    }
}
