use trueid_core::ports::{FacePoseEstimator, PoseError};
use trueid_core::{FaceDetection, FaceLandmarks, Frame};

#[derive(Debug, Clone)]
pub struct GeometricLandmarkPoseEstimator {
    pub max_abs_roll_deg: f32,
    pub max_abs_yaw_ratio: f32,
    pub expected_nose_y_ratio: f32,
    pub max_pitch_t_deviation: f32,
}

impl Default for GeometricLandmarkPoseEstimator {
    fn default() -> Self {
        Self {
            max_abs_roll_deg: 22.0,
            max_abs_yaw_ratio: 0.28,
            expected_nose_y_ratio: 0.42,
            max_pitch_t_deviation: 0.16,
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

        let eye_y = (lm.left_eye.1 + lm.right_eye.1) * 0.5;
        let mouth_y = (lm.mouth_left.1 + lm.mouth_right.1) * 0.5;
        let face_h = mouth_y - eye_y;
        if !face_h.is_finite() || face_h < 1e-5 {
            return Err(PoseError::Failed(
                "face height (eyes–mouth) too small".into(),
            ));
        }
        let t = (lm.nose_tip.1 - eye_y) / face_h;
        if !t.is_finite() || (t - self.expected_nose_y_ratio).abs() > self.max_pitch_t_deviation {
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

#[cfg(test)]
mod tests {
    use super::*;
    use trueid_core::BoundingBox;

    fn det_with_landmarks(lm: FaceLandmarks) -> FaceDetection {
        FaceDetection {
            bbox: BoundingBox::full_frame(),
            landmarks: Some(lm),
        }
    }

    fn frontal_like() -> FaceLandmarks {
        FaceLandmarks {
            left_eye: (0.4, 0.35),
            right_eye: (0.6, 0.35),
            nose_tip: (0.5, 0.52),
            mouth_left: (0.44, 0.72),
            mouth_right: (0.56, 0.72),
        }
    }

    #[test]
    fn accepts_frontal_like() {
        let p = GeometricLandmarkPoseEstimator::default();
        let f = Frame {
            modality: trueid_core::StreamModality::Rgb,
            width: 1,
            height: 1,
            format: trueid_core::PixelFormat::Gray8,
            bytes: vec![0],
        };
        assert!(
            p.check_frontal(&f, &det_with_landmarks(frontal_like()))
                .is_ok()
        );
    }

    #[test]
    fn rejects_strong_roll() {
        let p = GeometricLandmarkPoseEstimator::default();
        let f = Frame {
            modality: trueid_core::StreamModality::Rgb,
            width: 1,
            height: 1,
            format: trueid_core::PixelFormat::Gray8,
            bytes: vec![0],
        };
        let mut lm = frontal_like();
        lm.right_eye.1 = 0.50;
        assert!(matches!(
            p.check_frontal(&f, &det_with_landmarks(lm)),
            Err(PoseError::NotFrontal)
        ));
    }

    #[test]
    fn rejects_strong_yaw() {
        let p = GeometricLandmarkPoseEstimator::default();
        let f = Frame {
            modality: trueid_core::StreamModality::Rgb,
            width: 1,
            height: 1,
            format: trueid_core::PixelFormat::Gray8,
            bytes: vec![0],
        };
        let mut lm = frontal_like();
        lm.nose_tip.0 = 0.65;
        assert!(matches!(
            p.check_frontal(&f, &det_with_landmarks(lm)),
            Err(PoseError::NotFrontal)
        ));
    }

    #[test]
    fn no_landmarks_rejected() {
        let p = GeometricLandmarkPoseEstimator::default();
        let f = Frame {
            modality: trueid_core::StreamModality::Rgb,
            width: 1,
            height: 1,
            format: trueid_core::PixelFormat::Gray8,
            bytes: vec![0],
        };
        let d = FaceDetection {
            bbox: BoundingBox::full_frame(),
            landmarks: None,
        };
        assert!(matches!(
            p.check_frontal(&f, &d),
            Err(PoseError::NotFrontal)
        ));
    }
}
