//! V4L2 camera → RGB8 [`Frame`]s.
//!
//! One `capture`: open stream, dequeue warm-up buffers (payload discarded), then kept frames, then stop.

use std::io;
use std::sync::Mutex;
use std::time::Instant;

use v4l::buffer::Type;
use v4l::io::traits::{CaptureStream, Stream as V4lStream};
use v4l::io::userptr::Stream as UserptrStream;
use v4l::video::Capture;
use v4l::{Device, Format, FourCC};
use trueid_core::ports::{CaptureError, CaptureSpec, VideoSource};
use trueid_core::{Frame, PixelFormat, StreamModality};

pub struct V4lVideoSource {
    inner: Mutex<V4lInner>,
}

struct V4lInner {
    // Holds the device open; stream uses the same fd.
    #[allow(dead_code)]
    dev: Device,
    stream: UserptrStream,
    width: u32,
    height: u32,
    fourcc: FourCC,
}

impl V4lVideoSource {
    /// `/dev/video{index}`, requested size `width` x `height`.
    ///
    /// Tries MJPEG, YUYV, then 8-bit grey (`GREY` / `Y800`). IR-only nodes often negotiate grey.
    pub fn open_with_dimensions(
        index: u32,
        width: u32,
        height: u32,
    ) -> Result<Self, CaptureError> {
        let dev = Device::new(index as usize).map_err(io_to_capture)?;
        let active = dev
            .set_format(&Format::new(width, height, FourCC::new(b"MJPG")))
            .or_else(|_| dev.set_format(&Format::new(width, height, FourCC::new(b"YUYV"))))
            .or_else(|_| dev.set_format(&Format::new(width, height, FourCC::new(b"GREY"))))
            .or_else(|_| dev.set_format(&Format::new(width, height, FourCC::new(b"Y800"))))
            .map_err(io_to_capture)?;

        let stream =
            UserptrStream::with_buffers(&dev, Type::VideoCapture, 4).map_err(io_to_capture)?;

        Ok(Self {
            inner: Mutex::new(V4lInner {
                dev,
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

fn grab_raw_payload(inner: &mut V4lInner) -> Result<Vec<u8>, CaptureError> {
    let (buf, meta) = inner.stream.next().map_err(io_to_capture)?;
    let len = meta.bytesused as usize;
    if len == 0 {
        return Err(CaptureError::Failed("empty camera frame".into()));
    }
    if len > buf.len() {
        return Err(CaptureError::Failed(format!(
            "invalid frame length {} > {}",
            len,
            buf.len()
        )));
    }
    Ok(buf[..len].to_vec())
}

fn matches_fourcc(f: &FourCC, tag: &[u8; 4]) -> bool {
    f.repr == *tag
}

fn decode_payload(
    fourcc: &FourCC,
    payload: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, CaptureError> {
    if matches_fourcc(fourcc, b"MJPG") {
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
    } else if matches_fourcc(fourcc, b"YUYV") {
        yuyv_to_rgb(payload, width, height)
    } else if matches_fourcc(fourcc, b"GREY") || matches_fourcc(fourcc, b"Y800") {
        grey8_to_rgb(payload, width, height)
    } else {
        Err(CaptureError::Failed(format!(
            "unsupported fourcc {:?} (want MJPG, YUYV, GREY, or Y800)",
            fourcc.str().unwrap_or("?")
        )))
    }
}

fn grey8_to_rgb(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, CaptureError> {
    let w = width as usize;
    let h = height as usize;
    let expected = w.checked_mul(h).ok_or_else(|| {
        CaptureError::Failed("grey frame dimensions overflow".into())
    })?;
    if data.len() < expected {
        return Err(CaptureError::Failed(format!(
            "grey buffer too short: {} < {expected}",
            data.len()
        )));
    }
    let mut out = Vec::with_capacity(expected * 3);
    for &g in &data[..expected] {
        out.push(g);
        out.push(g);
        out.push(g);
    }
    Ok(out)
}

#[inline]
fn yuv_bt601_to_rgb(y: i32, u: i32, v: i32) -> (u8, u8, u8) {
    let c = y - 16;
    let d = u - 128;
    let e = v - 128;
    let r = (298 * c + 409 * e + 128) >> 8;
    let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
    let b = (298 * c + 516 * d + 128) >> 8;
    (
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
    )
}

fn yuyv_to_rgb(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, CaptureError> {
    if width % 2 != 0 {
        return Err(CaptureError::Failed(format!(
            "YUYV requires even width, got {width}"
        )));
    }
    let w = width as usize;
    let h = height as usize;
    let expected = w * h * 2;
    if data.len() < expected {
        return Err(CaptureError::Failed(format!(
            "yuyv buffer too short: {} < {expected}",
            data.len()
        )));
    }
    let data = &data[..expected];

    let mut out = vec![0u8; w * h * 3];
    for row in 0..h {
        for col in (0..w).step_by(2) {
            let i = row * w * 2 + col * 2;
            let y0 = data[i] as i32;
            let u = data[i + 1] as i32;
            let y1 = data[i + 2] as i32;
            let v = data[i + 3] as i32;

            let (r0, g0, b0) = yuv_bt601_to_rgb(y0, u, v);
            let (r1, g1, b1) = yuv_bt601_to_rgb(y1, u, v);

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

    fn capture(&self, spec: CaptureSpec) -> Result<Vec<Frame>, CaptureError> {
        let spec = spec.validate()?;
        let warmup = spec.warmup_discard as usize;
        let keep = spec.frame_count as usize;
        let t0 = Instant::now();
        tracing::debug!(
            warmup_discard = warmup,
            frame_count = keep,
            "v4l: capture burst"
        );

        let (fourcc, width, height, raws) = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| CaptureError::Failed("camera mutex poisoned".to_string()))?;

            let fourcc = inner.fourcc;
            let width = inner.width;
            let height = inner.height;

            let burst_result = (|| -> Result<Vec<Vec<u8>>, CaptureError> {
                for _ in 0..warmup {
                    grab_raw_payload(&mut inner)?;
                }
                let mut raws = Vec::with_capacity(keep);
                for _ in 0..keep {
                    raws.push(grab_raw_payload(&mut inner)?);
                }
                Ok(raws)
            })();

            let _ = inner.stream.stop();

            match burst_result {
                Ok(raws) => (fourcc, width, height, raws),
                Err(e) => return Err(e),
            }
        };

        let mut frames = Vec::with_capacity(keep);
        for raw in raws {
            let bytes = decode_payload(&fourcc, &raw, width, height)?;
            frames.push(Frame {
                modality: StreamModality::Rgb,
                width,
                height,
                format: PixelFormat::Rgb8,
                bytes,
            });
        }
        tracing::info!(
            warmup_discard = warmup,
            returned = frames.len(),
            fourcc = ?fourcc.str().unwrap_or("?"),
            w = width,
            h = height,
            elapsed_ms = t0.elapsed().as_millis(),
            "v4l: capture done"
        );
        Ok(frames)
    }
}
