use trueid_core::ports::{CaptureError, VideoSession, VideoSource};
use trueid_core::{Frame, PixelFormat, StreamModality};

pub struct MockVideoSession {
    frame: Frame,
}

impl VideoSession for MockVideoSession {
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        Ok(self.frame.clone())
    }
}

pub struct MockVideoSource {
    frame: Frame,
}

impl MockVideoSource {
    pub fn with_modality(modality: StreamModality) -> Self {
        Self {
            frame: Frame::new(modality, 2, 2, PixelFormat::Gray8, vec![0, 255, 128, 64])
                .expect("2×2 Gray8 is 4 bytes"),
        }
    }
}

impl VideoSource for MockVideoSource {
    fn modality(&self) -> StreamModality {
        self.frame.modality()
    }

    fn open_session(&self) -> Result<Box<dyn VideoSession>, CaptureError> {
        Ok(Box::new(MockVideoSession {
            frame: self.frame.clone(),
        }))
    }
}
