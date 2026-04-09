//! `CameraCapture` over `VideoSource`: RGB-only or parallel RGB+IR (two threads).

use std::sync::Arc;
use std::thread;

use trueid_core::ports::{CameraCapture, CaptureError, CaptureSpec, CapturedBurst, VideoSource};

pub struct RgbOnlyCameraCapture {
    rgb: Arc<dyn VideoSource>,
}

impl RgbOnlyCameraCapture {
    pub fn new(rgb: Arc<dyn VideoSource>) -> Self {
        Self { rgb }
    }
}

impl CameraCapture for RgbOnlyCameraCapture {
    fn capture(&self, spec: CaptureSpec) -> Result<CapturedBurst, CaptureError> {
        let rgb = self.rgb.capture(spec)?;
        Ok(CapturedBurst {
            rgb: Some(rgb),
            ir: None,
        })
    }
}

pub struct IROnlyCameraCapture {
    ir: Arc<dyn VideoSource>,
}

impl IROnlyCameraCapture {
    pub fn new(ir: Arc<dyn VideoSource>) -> Self {
        Self { ir }
    }
}

impl CameraCapture for IROnlyCameraCapture {
    fn capture(&self, spec: CaptureSpec) -> Result<CapturedBurst, CaptureError> {
        let ir = self.ir.capture(spec)?;
        Ok(CapturedBurst {
            rgb: None,
            ir: Some(ir),
        })
    }
}

pub struct ParallelRgbIrCameraCapture {
    rgb: Arc<dyn VideoSource>,
    ir: Arc<dyn VideoSource>,
}

impl ParallelRgbIrCameraCapture {
    pub fn new(rgb: Arc<dyn VideoSource>, ir: Arc<dyn VideoSource>) -> Self {
        Self { rgb, ir }
    }
}

impl CameraCapture for ParallelRgbIrCameraCapture {
    fn capture(&self, spec: CaptureSpec) -> Result<CapturedBurst, CaptureError> {
        thread::scope(|s| {
            let h_rgb = s.spawn(|| self.rgb.capture(spec));
            let h_ir = s.spawn(|| self.ir.capture(spec));
            let rgb = h_rgb
                .join()
                .map_err(|_| CaptureError::Failed("RGB capture thread panicked".into()))??;
            let ir = h_ir
                .join()
                .map_err(|_| CaptureError::Failed("IR capture thread panicked".into()))??;
            Ok(CapturedBurst {
                rgb: Some(rgb),
                ir: Some(ir),
            })
        })
    }
}
