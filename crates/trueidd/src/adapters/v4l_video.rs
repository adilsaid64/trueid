use std::io;
use std::sync::Mutex;

use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::io::userptr::Stream as UserptrStream;
use v4l::video::Capture;
use v4l::{Device, Format, FourCC};
use trueid_core::ports::{CaptureError, VideoSource};
use trueid_core::{Frame, PixelFormat, StreamModality};

pub struct V4lVideoSource {
    inner: Mutex<V4lInner>,
}

struct V4lInner {
    _dev: Device,
    stream: UserptrStream,
    width: u32,
    height: u32,
    fourcc: FourCC,
}

impl V4lVideoSource {
    /// Open `/dev/video{index}` (typically `0` for the default webcam).
    pub fn open(index: u32) -> Result<Self, CaptureError> {
        let dev = Device::new(index as usize).map_err(io_to_capture)?;
        let mjpg = Format::new(640, 480, FourCC::new(b"MJPG"));
        let active = match dev.set_format(&mjpg) {
            Ok(f) => f,
            Err(_) => {
                let yuyv = Format::new(640, 480, FourCC::new(b"YUYV"));
                dev.set_format(&yuyv).map_err(io_to_capture)?
            }
        };

        let mut stream =
            UserptrStream::with_buffers(&dev, Type::VideoCapture, 4).map_err(io_to_capture)?;
        stream.next().map_err(io_to_capture)?;

        Ok(Self {
            inner: Mutex::new(V4lInner {
                _dev: dev,
                stream,
                width: active.width,
                height: active.height,
                fourcc: active.fourcc,
            }),
        })
    }
}

fn io_to_capture(e: io::Error) -> CaptureError {
    CaptureError::Failed(e.to_string())
}

fn fourcc_is(f: &FourCC, tag: &[u8; 4]) -> bool {
    f.repr == *tag
}

fn decode_payload(
    fourcc: &FourCC,
    payload: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, CaptureError> {
    if fourcc_is(fourcc, b"MJPG") {
        let img = image::load_from_memory_with_format(payload, image::ImageFormat::Jpeg).map_err(
            |e| CaptureError::Failed(format!("mjpeg decode: {e}")),
        )?;
        let rgb = img.to_rgb8();
        if rgb.width() != width || rgb.height() != height {
            return Err(CaptureError::Failed(format!(
                "jpeg size {}x{} != expected {width}x{height}",
                rgb.width(),
                rgb.height()
            )));
        }
        Ok(rgb.into_raw())
    } else if fourcc_is(fourcc, b"YUYV") {
        yuyv_to_rgb(payload, width, height)
    } else {
        Err(CaptureError::Failed(format!(
            "unsupported pixel format (fourcc {:?})",
            fourcc.str().unwrap_or("?")
        )))
    }
}

fn yuyv_to_rgb(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, CaptureError> {
    let w = width as usize;
    let h = height as usize;
    let expected = w * h * 2;
    if data.len() < expected {
        return Err(CaptureError::Failed(format!(
            "yuyv buffer too short: {} < {expected}",
            data.len()
        )));
    }
    let mut out = vec![0u8; w * h * 3];
    for row in 0..h {
        for col in (0..w).step_by(2) {
            let i = row * w * 2 + col * 2;
            let y0 = data[i] as i32;
            let u = data[i + 1] as i32 - 128;
            let y1 = data[i + 2] as i32;
            let v = data[i + 3] as i32 - 128;

            let r0 = (y0 + ((359 * v) >> 8)).clamp(0, 255) as u8;
            let g0 = (y0 - ((88 * u + 183 * v) >> 8)).clamp(0, 255) as u8;
            let b0 = (y0 + ((454 * u) >> 8)).clamp(0, 255) as u8;

            let r1 = (y1 + ((359 * v) >> 8)).clamp(0, 255) as u8;
            let g1 = (y1 - ((88 * u + 183 * v) >> 8)).clamp(0, 255) as u8;
            let b1 = (y1 + ((454 * u) >> 8)).clamp(0, 255) as u8;

            let o0 = (row * w + col) * 3;
            out[o0] = r0;
            out[o0 + 1] = g0;
            out[o0 + 2] = b0;
            let o1 = (row * w + col + 1) * 3;
            out[o1] = r1;
            out[o1 + 1] = g1;
            out[o1 + 2] = b1;
        }
    }
    Ok(out)
}

impl VideoSource for V4lVideoSource {
    fn modality(&self) -> StreamModality {
        StreamModality::Rgb
    }

    fn next_frame(&self) -> Result<Frame, CaptureError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| CaptureError::Failed("camera mutex poisoned".to_string()))?;
        let fourcc = inner.fourcc;
        let width = inner.width;
        let height = inner.height;
        let (buf, meta) = inner.stream.next().map_err(io_to_capture)?;
        let len = meta.bytesused as usize;
        if len > buf.len() {
            return Err(CaptureError::Failed(format!(
                "invalid frame length {} > {}",
                len,
                buf.len()
            )));
        }
        let payload = &buf[..len];
        let bytes = decode_payload(&fourcc, payload, width, height)?;
        Ok(Frame {
            modality: StreamModality::Rgb,
            width,
            height,
            format: PixelFormat::Rgb8,
            bytes,
        })
    }
}
