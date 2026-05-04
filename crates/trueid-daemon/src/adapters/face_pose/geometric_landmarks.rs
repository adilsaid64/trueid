use trueid_core::ports::{FacePoseEstimator, PoseError};
use trueid_core::{FaceDetection, FaceLandmarks, Frame};

#[derive(Debug, Clone)]
pub struct GeometricLandmarkPoseEstimator {
    pub max_abs_roll_deg: f32,
    pub max_abs_yaw_ratio: f32,
}

impl Default for GeometricLandmarkPoseEstimator {
    fn default() -> Self {
        Self {
            max_abs_roll_deg: 30.0,
            max_abs_yaw_ratio: 0.45,
        }
    }
}

impl GeometricLandmarkPoseEstimator {
    fn check_landmarks(&self, lm: &FaceLandmarks) -> Result<(), PoseError> {
        let dx = lm.right_eye.0 - lm.left_eye.0;
        let dy = lm.right_eye.1 - lm.left_eye.1;
        let inter_eye = (dx * dx + dy * dy).sqrt();
        if !inter_eye.is_finite() || inter_eye < 1e-5 {
            return Err(PoseError::Failed("inter-ocular distance too small".into()));
        }

        let roll_rad = dy.atan2(dx);
        let max_roll_rad = self.max_abs_roll_deg.to_radians();
        if roll_rad.abs() > max_roll_rad {
            return Err(PoseError::NotFrontal);
        }

        let eye_mid_x = (lm.left_eye.0 + lm.right_eye.0) * 0.5;
        let yaw_ratio = (lm.nose_tip.0 - eye_mid_x) / inter_eye;
        if !yaw_ratio.is_finite() || yaw_ratio.abs() > self.max_abs_yaw_ratio {
            return Err(PoseError::NotFrontal);
        }

        Ok(())
    }
}

impl FacePoseEstimator for GeometricLandmarkPoseEstimator {
    fn check_frontal(
        &self,
        _aligned_face: &Frame,
        detection: &FaceDetection,
    ) -> Result<(), PoseError> {
        let Some(ref lm) = detection.landmarks else {
            return Err(PoseError::NotFrontal);
        };
        self.check_landmarks(lm)
    }
}
