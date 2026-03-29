//! V4L2 camera → RGB8 [`Frame`]s.
//!
//! Opens `/dev/video{N}` only during each capture burst, decodes MJPEG (EXIF orientation) or raw
//! YUYV/grey, then optional `TRUEID_V4L_ROTATE_180` / `TRUEID_V4L_FLIP_VERTICAL`.
//! For MJPEG, env-based fixes run only when EXIF orientation is `NoTransforms`; otherwise they
//! would stack on top of EXIF and invert the image (e.g. upside-down aligned faces).

use std::io;
use std::io::Cursor;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use image::imageops;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbImage};
use v4l::buffer::Type;
use v4l::io::traits::{CaptureStream, Stream as V4lStream};
use v4l::io::userptr::Stream as UserptrStream;
use v4l::video::Capture;
use v4l::{Device, Format, FourCC};
use trueid_core::ports::{CaptureError, CaptureSpec, VideoSource};
use trueid_core::{Frame, PixelFormat, StreamModality};

pub struct V4lVideoSource {
    index: u32,
    width: u32,
    height: u32,
    /// Serializes bursts; the device fd is only held during `capture`.
    capture_lock: Mutex<()>,
}

impl V4lVideoSource {
    /// `/dev/video{index}`, requested size `width` x `height`.
    ///
    /// Tries MJPEG, YUYV, then 8-bit grey (`GREY` / `Y800`). IR-only nodes often negotiate grey.
    ///
    /// Opens the device briefly to validate it, then closes it so nothing holds the camera until
    /// the next [`VideoSource::capture`].
    pub fn open_with_dimensions(
        index: u32,
        width: u32,
        height: u32,
    ) -> Result<Self, CaptureError> {
        let dev = Device::new(index as usize).map_err(io_to_capture)?;
        let _active = negotiate_format(&dev, width, height)?;
        let _stream =
            UserptrStream::with_buffers(&dev, Type::VideoCapture, 4).map_err(io_to_capture)?;
        // Drop stream first so streaming is stopped; then device closes.
        drop(_stream);
        drop(dev);

        Ok(Self {
            index,
            width,
            height,
            capture_lock: Mutex::new(()),
        })
    }
}

fn negotiate_format(dev: &Device, width: u32, height: u32) -> Result<Format, CaptureError> {
    dev.set_format(&Format::new(width, height, FourCC::new(b"MJPG")))
        .or_else(|_| dev.set_format(&Format::new(width, height, FourCC::new(b"YUYV"))))
        .or_else(|_| dev.set_format(&Format::new(width, height, FourCC::new(b"GREY"))))
        .or_else(|_| dev.set_format(&Format::new(width, height, FourCC::new(b"Y800"))))
        .map_err(io_to_capture)
}

fn io_to_capture(e: io::Error) -> CaptureError {
    CaptureError::Failed(e.to_string())
}

fn grab_raw_payload(stream: &mut UserptrStream) -> Result<Vec<u8>, CaptureError> {
    let (buf, meta) = stream.next().map_err(io_to_capture)?;
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
) -> Result<(Vec<u8>, u32, u32), CaptureError> {
    let (bytes, w, h, skip_env_sensor_fix) = if matches_fourcc(fourcc, b"MJPG") {
        decode_mjpeg_apply_exif(payload)?
    } else if matches_fourcc(fourcc, b"YUYV") {
        let bytes = yuyv_to_rgb(payload, width, height)?;
        (bytes, width, height, false)
    } else if matches_fourcc(fourcc, b"GREY") || matches_fourcc(fourcc, b"Y800") {
        let bytes = grey8_to_rgb(payload, width, height)?;
        (bytes, width, height, false)
    } else {
        return Err(CaptureError::Failed(format!(
            "unsupported fourcc {:?} (want MJPG, YUYV, GREY, or Y800)",
            fourcc.str().unwrap_or("?")
        )));
    };
    apply_optional_sensor_fix(bytes, w, h, skip_env_sensor_fix)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum V4lPixelFix {
    None,
    Rotate180,
    FlipVertical,
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn pixel_fix_from_env() -> V4lPixelFix {
    static FIX: OnceLock<V4lPixelFix> = OnceLock::new();
    *FIX.get_or_init(|| {
        let f = if env_truthy("TRUEID_V4L_ROTATE_180") {
            V4lPixelFix::Rotate180
        } else if env_truthy("TRUEID_V4L_FLIP_VERTICAL") {
            V4lPixelFix::FlipVertical
        } else {
            V4lPixelFix::None
        };
        if f != V4lPixelFix::None {
            tracing::info!(?f, "v4l: sensor pixel fix enabled");
        }
        f
    })
}

fn apply_optional_sensor_fix(
    rgb: Vec<u8>,
    width: u32,
    height: u32,
    skip_env_sensor_fix: bool,
) -> Result<(Vec<u8>, u32, u32), CaptureError> {
    let fix = pixel_fix_from_env();
    if fix == V4lPixelFix::None {
        return Ok((rgb, width, height));
    }
    if skip_env_sensor_fix {
        tracing::info!(
            ?fix,
            "v4l: skipping env sensor fix (MJPEG already oriented via EXIF; \
             TRUEID_V4L_ROTATE_180 would flip it again)"
        );
        return Ok((rgb, width, height));
    }
    let img = RgbImage::from_raw(width, height, rgb).ok_or_else(|| {
        CaptureError::Failed("sensor fix: invalid rgb dimensions".into())
    })?;
    let out = if fix == V4lPixelFix::Rotate180 {
        imageops::rotate180(&img)
    } else {
        imageops::flip_vertical(&img)
    };
    let w = out.width();
    let h = out.height();
    Ok((out.into_raw(), w, h))
}

/// Returns `(rgb, w, h, skip_env_sensor_fix)`.
/// `skip_env_sensor_fix` is true when EXIF requested a non-identity orientation so we do not also
/// apply `TRUEID_V4L_ROTATE_180` / `TRUEID_V4L_FLIP_VERTICAL` (avoids double correction).
fn decode_mjpeg_apply_exif(payload: &[u8]) -> Result<(Vec<u8>, u32, u32, bool), CaptureError> {
    let mut decoder = ImageReader::with_format(Cursor::new(payload), ImageFormat::Jpeg)
        .into_decoder()
        .map_err(|e| CaptureError::Failed(format!("mjpeg decoder: {e}")))?;
    let orientation = decoder
        .orientation()
        .map_err(|e| CaptureError::Failed(format!("mjpeg orientation: {e}")))?;
    let mut img = DynamicImage::from_decoder(decoder)
        .map_err(|e| CaptureError::Failed(format!("mjpeg decode: {e}")))?;
    if orientation != Orientation::NoTransforms {
        tracing::debug!(?orientation, "v4l: applying JPEG EXIF orientation");
    }
    img.apply_orientation(orientation);
    let rgb = img.to_rgb8();
    let w = rgb.width();
    let h = rgb.height();
    let skip_env_sensor_fix = orientation != Orientation::NoTransforms;
    Ok((rgb.into_raw(), w, h, skip_env_sensor_fix))
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
    if !width.is_multiple_of(2) {
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

        let _guard = self
            .capture_lock
            .lock()
            .map_err(|_| CaptureError::Failed("camera mutex poisoned".to_string()))?;

        let dev = Device::new(self.index as usize).map_err(io_to_capture)?;
        let active = negotiate_format(&dev, self.width, self.height)?;
        let fourcc = active.fourcc;
        let width = active.width;
        let height = active.height;
        let mut stream =
            UserptrStream::with_buffers(&dev, Type::VideoCapture, 4).map_err(io_to_capture)?;

        let burst_result = (|| -> Result<Vec<Vec<u8>>, CaptureError> {
            for _ in 0..warmup {
                grab_raw_payload(&mut stream)?;
            }
            let mut raws = Vec::with_capacity(keep);
            for _ in 0..keep {
                raws.push(grab_raw_payload(&mut stream)?);
            }
            Ok(raws)
        })();

        let _ = stream.stop();

        let raws = burst_result?;

        let mut frames = Vec::with_capacity(keep);
        for raw in raws {
            let (bytes, fw, fh) = decode_payload(&fourcc, &raw, width, height)?;
            frames.push(Frame {
                modality: StreamModality::Rgb,
                width: fw,
                height: fh,
                format: PixelFormat::Rgb8,
                bytes,
            });
        }
        let (log_w, log_h) = frames
            .last()
            .map(|f| (f.width, f.height))
            .unwrap_or((width, height));
        tracing::info!(
            warmup_discard = warmup,
            returned = frames.len(),
            fourcc = ?fourcc.str().unwrap_or("?"),
            negotiated_w = width,
            negotiated_h = height,
            w = log_w,
            h = log_h,
            elapsed_ms = t0.elapsed().as_millis(),
            "v4l: capture done"
        );
        Ok(frames)
    }
}
