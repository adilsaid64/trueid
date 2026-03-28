//! Face box and landmarks in normalized coordinates (0–1 of frame width/height).

/// Bounding box: top-left `(x, y)`, size `(w, h)`, all relative to frame size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl BoundingBox {
    pub fn full_frame() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        }
    }

    /// In range and positive area (allows tiny float slack).
    pub fn is_valid(&self) -> bool {
        self.w > 1e-6
            && self.h > 1e-6
            && self.x >= -1e-3
            && self.y >= -1e-3
            && self.x + self.w <= 1.0 + 1e-3
            && self.y + self.h <= 1.0 + 1e-3
    }
}

/// Five landmark points (normalized), for alignment. Optional if the detector only gives a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceLandmarks {
    pub left_eye: (f32, f32),
    pub right_eye: (f32, f32),
    pub nose_tip: (f32, f32),
    pub mouth_left: (f32, f32),
    pub mouth_right: (f32, f32),
}

/// One face: box plus optional landmarks.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceDetection {
    pub bbox: BoundingBox,
    pub landmarks: Option<FaceLandmarks>,
}
