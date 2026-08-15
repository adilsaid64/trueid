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
            frame: Frame {
                modality,
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

    fn open_session(&self) -> Result<Box<dyn VideoSession>, CaptureError> {
        Ok(Box::new(MockVideoSession {
            frame: self.frame.clone(),
        }))
    }
}
