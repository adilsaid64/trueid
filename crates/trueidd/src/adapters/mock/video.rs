use trueid_core::ports::{CaptureError, VideoSource};
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

    fn next_frame(&self) -> Result<Frame, CaptureError> {
        Ok(self.frame.clone())
    }
}
