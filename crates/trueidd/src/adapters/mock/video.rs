use trueid_core::ports::{CaptureError, CaptureSpec, VideoSource};
use trueid_core::{Frame, PixelFormat, StreamModality};

pub struct MockVideoSource {
    frame: Frame,
}

impl MockVideoSource {
    pub fn default_gray() -> Self {
        Self {
            frame: Frame {
                modality: StreamModality::Rgb,
                width: 2,
                height: 2,
                format: PixelFormat::Gray8,
                bytes: vec![0, 255, 128, 64],
            },
        }
    }
}

impl VideoSource for MockVideoSource {
    fn modality(&self) -> StreamModality {
        self.frame.modality
    }

    fn capture(&self, spec: CaptureSpec) -> Result<Vec<Frame>, CaptureError> {
        let spec = spec.validate()?;
        Ok((0..spec.frame_count).map(|_| self.frame.clone()).collect())
    }
}
