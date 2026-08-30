use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamModality {
    Rgb,
    Ir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb8,
    Gray8,
}

impl PixelFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgb8 => 3,
            Self::Gray8 => 1,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FrameError {
    #[error("frame dimensions overflow ({width}×{height} {format:?})")]
    DimensionsOverflow {
        width: u32,
        height: u32,
        format: PixelFormat,
    },
    #[error("frame buffer length {actual} != {width}×{height} {format:?} (expected {expected})")]
    LengthMismatch {
        width: u32,
        height: u32,
        format: PixelFormat,
        actual: usize,
        expected: usize,
    },
}

/// One video frame. Buffer length is always `width × height × bytes_per_pixel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    modality: StreamModality,
    width: u32,
    height: u32,
    format: PixelFormat,
    bytes: Vec<u8>,
}

impl Frame {
    pub fn expected_len(width: u32, height: u32, format: PixelFormat) -> Option<usize> {
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(format.bytes_per_pixel()))
    }

    pub fn new(
        modality: StreamModality,
        width: u32,
        height: u32,
        format: PixelFormat,
        bytes: Vec<u8>,
    ) -> Result<Self, FrameError> {
        let Some(expected) = Self::expected_len(width, height, format) else {
            return Err(FrameError::DimensionsOverflow {
                width,
                height,
                format,
            });
        };
        if bytes.len() != expected {
            return Err(FrameError::LengthMismatch {
                width,
                height,
                format,
                actual: bytes.len(),
                expected,
            });
        }
        Ok(Self {
            modality,
            width,
            height,
            format,
            bytes,
        })
    }

    pub fn modality(&self) -> StreamModality {
        self.modality
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn format(&self) -> PixelFormat {
        self.format
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray8_accepts_matching_buffer() {
        let f = Frame::new(
            StreamModality::Rgb,
            2,
            2,
            PixelFormat::Gray8,
            vec![0, 255, 128, 64],
        )
        .unwrap();
        assert_eq!(f.width(), 2);
        assert_eq!(f.height(), 2);
        assert_eq!(f.bytes(), &[0, 255, 128, 64]);
    }

    #[test]
    fn rgb8_rejects_wrong_length() {
        let err = Frame::new(StreamModality::Rgb, 1, 1, PixelFormat::Rgb8, vec![0, 1]).unwrap_err();
        assert_eq!(
            err,
            FrameError::LengthMismatch {
                width: 1,
                height: 1,
                format: PixelFormat::Rgb8,
                actual: 2,
                expected: 3,
            }
        );
    }

    #[test]
    fn overflow_is_not_a_length_mismatch() {
        let err = Frame::new(
            StreamModality::Ir,
            u32::MAX,
            u32::MAX,
            PixelFormat::Rgb8,
            vec![],
        )
        .unwrap_err();
        assert!(matches!(err, FrameError::DimensionsOverflow { .. }));
    }
}
