//! Landmark-based similarity warp (YuNet) or square bbox crop + resize to `output_size`.
//! Optional `TRUEID_DEBUG_ALIGNED_DIR`: write each aligned face as PNG for debugging.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};
use trueid_core::ports::{AlignError, FaceAligner};
use trueid_core::{BoundingBox, FaceDetection, FaceLandmarks, Frame, PixelFormat};

/// Default aligned face size (InsightFace / ArcFace-style models often use 112×112).
const DEFAULT_OUTPUT: u32 = 112;

/// Reference eye positions on a 112×112 canvas (InsightFace `norm_crop` template).
const REF112_LE: (f32, f32) = (38.2946, 51.6963);
const REF112_RE: (f32, f32) = (73.5318, 51.5014);

/// Expand bbox by this fraction of width/height before squaring (e.g. 0.25 → +25% size).
const DEFAULT_MARGIN: f32 = 0.25;

static ALIGNED_DUMP_SEQ: AtomicU64 = AtomicU64::new(0);

static ALIGNED_DUMP_ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

fn aligned_dump_root() -> Option<&'static Path> {
    ALIGNED_DUMP_ROOT
        .get_or_init(|| {
            std::env::var("TRUEID_DEBUG_ALIGNED_DIR")
                .ok()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
        })
        .as_deref()
}

fn maybe_dump_aligned_face(aligned: &Frame) {
    let Some(root) = aligned_dump_root() else {
        return;
    };
    if aligned.format != PixelFormat::Rgb8 {
        tracing::warn!("aligned dump: skip non-rgb8 frame");
        return;
    }

    if let Err(e) = std::fs::create_dir_all(root) {
        tracing::warn!(error = %e, path = %root.display(), "aligned dump: create_dir_all failed");
        return;
    }

    let seq = ALIGNED_DUMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let name = format!(
        "aligned-{}-{}-{}.png",
        now.as_secs(),
        now.subsec_nanos(),
        seq
    );
    let file = root.join(name);
    match image::save_buffer(
        &file,
        &aligned.bytes,
        aligned.width,
        aligned.height,
        image::ColorType::Rgb8,
    ) {
        Ok(()) => tracing::info!(path = %file.display(), w = aligned.width, h = aligned.height, "aligned face dumped"),
        Err(e) => tracing::warn!(error = %e, path = %file.display(), "aligned dump: save failed"),
    }
}

/// Face-aligned crop for [`FaceEmbedder`](trueid_core::ports::FaceEmbedder).
pub struct CropFaceAligner {
    output_size: u32,
    margin: f32,
}

impl Default for CropFaceAligner {
    fn default() -> Self {
        Self::new(DEFAULT_OUTPUT, DEFAULT_MARGIN)
    }
}

impl CropFaceAligner {
    pub fn new(output_size: u32, margin: f32) -> Self {
        Self {
            output_size: output_size.max(1),
            margin: margin.max(0.0),
        }
    }
}

impl FaceAligner for CropFaceAligner {
    fn align(&self, frame: &Frame, detection: &FaceDetection) -> Result<Frame, AlignError> {
        let t0 = Instant::now();
        let has_landmarks = detection.landmarks.is_some();
        tracing::debug!(
            w = frame.width,
            h = frame.height,
            output = self.output_size,
            margin = self.margin,
            has_landmarks,
            bbox = ?detection.bbox,
            "align: start"
        );

        let rgb = frame_to_rgb_image(frame).map_err(AlignError::Failed)?;
        let out = self.output_size;

        let cropped = if let Some(ref lm) = detection.landmarks {
            warp_similarity_eyes(&rgb, frame.width, frame.height, lm, out)?
        } else {
            let bb = square_crop_bbox(&detection.bbox, self.margin);
            tracing::trace!(?bb, "align: bbox-only crop (no landmarks)");
            crop_and_resize(&rgb, frame.width, frame.height, &bb, out)?
        };

        tracing::debug!(
            mode = if has_landmarks {
                "similarity_eyes"
            } else {
                "bbox_square"
            },
            out_w = out,
            out_h = out,
            elapsed_ms = t0.elapsed().as_millis(),
            "align: done"
        );

        let aligned = Frame {
            modality: frame.modality,
            width: out,
            height: out,
            format: PixelFormat::Rgb8,
            bytes: cropped.into_raw(),
        };
        maybe_dump_aligned_face(&aligned);
        Ok(aligned)
    }
}

fn frame_to_rgb_image(frame: &Frame) -> Result<RgbImage, String> {
    let w = frame.width as usize;
    let h = frame.height as usize;
    match frame.format {
        PixelFormat::Rgb8 => {
            let expected = w
                .checked_mul(h)
                .and_then(|n| n.checked_mul(3))
                .ok_or_else(|| "frame dimensions overflow".to_string())?;
            if frame.bytes.len() != expected {
                return Err(format!(
                    "rgb8 length {} != {}×{}×3",
                    frame.bytes.len(),
                    frame.width,
                    frame.height
                ));
            }
            RgbImage::from_raw(frame.width, frame.height, frame.bytes.clone())
                .ok_or_else(|| "invalid rgb8 buffer".to_string())
        }
        PixelFormat::Gray8 => {
            if frame.bytes.len() != w * h {
                return Err(format!(
                    "gray8 length {} != {}×{}",
                    frame.bytes.len(),
                    frame.width,
                    frame.height
                ));
            }
            let mut rgb = RgbImage::new(frame.width, frame.height);
            for y in 0..frame.height {
                for x in 0..frame.width {
                    let g = frame.bytes[(y * frame.width + x) as usize];
                    rgb.put_pixel(x, y, Rgb([g, g, g]));
                }
            }
            Ok(rgb)
        }
    }
}

/// Square box around detection, expanded by `margin`, clamped to the unit square.
fn square_crop_bbox(b: &BoundingBox, margin: f32) -> BoundingBox {
    let cx = b.x + b.w * 0.5;
    let cy = b.y + b.h * 0.5;
    let w2 = b.w * (1.0 + margin);
    let h2 = b.h * (1.0 + margin);
    let mut side = w2.max(h2);
    side = side.min(1.0);

    let mut x = cx - side * 0.5;
    let mut y = cy - side * 0.5;
    if x < 0.0 {
        x = 0.0;
    }
    if y < 0.0 {
        y = 0.0;
    }
    if x + side > 1.0 {
        x = 1.0 - side;
    }
    if y + side > 1.0 {
        y = 1.0 - side;
    }

    BoundingBox {
        x,
        y,
        w: side,
        h: side,
    }
}

fn crop_and_resize(
    rgb: &RgbImage,
    fw: u32,
    fh: u32,
    bb: &BoundingBox,
    out: u32,
) -> Result<RgbImage, AlignError> {
    let x0 = (bb.x * fw as f32).floor().max(0.0) as u32;
    let y0 = (bb.y * fh as f32).floor().max(0.0) as u32;
    let x1 = ((bb.x + bb.w) * fw as f32).ceil().min(fw as f32) as u32;
    let y1 = ((bb.y + bb.h) * fh as f32).ceil().min(fh as f32) as u32;
    let cw = x1.saturating_sub(x0).max(1);
    let ch = y1.saturating_sub(y0).max(1);

    let sub = image::imageops::crop_imm(rgb, x0, y0, cw, ch).to_image();
    let dyn_img = DynamicImage::ImageRgb8(sub);
    let resized = dyn_img.resize_exact(out, out, FilterType::Triangle);
    Ok(resized.to_rgb8())
}

/// Maps subject left/right eye centers to InsightFace reference positions on an `out`×`out` canvas.
fn warp_similarity_eyes(
    rgb: &RgbImage,
    fw: u32,
    fh: u32,
    lm: &FaceLandmarks,
    out: u32,
) -> Result<RgbImage, AlignError> {
    let s = out as f32 / 112.0;
    let q1x = REF112_LE.0 * s;
    let q1y = REF112_LE.1 * s;
    let q2x = REF112_RE.0 * s;
    let q2y = REF112_RE.1 * s;

    let p1x = lm.left_eye.0 * fw as f32;
    let p1y = lm.left_eye.1 * fh as f32;
    let p2x = lm.right_eye.0 * fw as f32;
    let p2y = lm.right_eye.1 * fh as f32;

    let dx = p2x - p1x;
    let dy = p2y - p1y;
    let den = dx * dx + dy * dy;
    if den < 1e-6 {
        return Err(AlignError::Failed(
            "landmark eyes degenerate; cannot align".into(),
        ));
    }

    let dqx = q2x - q1x;
    let dqy = q2y - q1y;
    let sc = (dqx * dx + dqy * dy) / den;
    let ss = (dqy * dx - dqx * dy) / den;
    let tx = q1x - sc * p1x + ss * p1y;
    let ty = q1y - ss * p1x - sc * p1y;

    let inv_det = sc * sc + ss * ss;
    if inv_det < 1e-12 {
        return Err(AlignError::Failed("similarity scale too small".into()));
    }

    let mut out_img = RgbImage::new(out, out);
    let fw1 = fw as f32;
    let fh1 = fh as f32;

    for oy in 0..out {
        for ox in 0..out {
            let dst_x = ox as f32 + 0.5;
            let dst_y = oy as f32 + 0.5;
            let sx = (sc * (dst_x - tx) + ss * (dst_y - ty)) / inv_det;
            let sy = (-ss * (dst_x - tx) + sc * (dst_y - ty)) / inv_det;

            let p = sample_bilinear(rgb, sx, sy, fw1, fh1);
            out_img.put_pixel(ox, oy, p);
        }
    }

    Ok(out_img)
}

fn sample_bilinear(rgb: &RgbImage, sx: f32, sy: f32, fw: f32, fh: f32) -> Rgb<u8> {
    if !(sx >= 0.0 && sy >= 0.0 && sx < fw && sy < fh) {
        return Rgb([0, 0, 0]);
    }
    let x0 = sx.floor() as u32;
    let y0 = sy.floor() as u32;
    let x1 = (x0 + 1).min(rgb.width().saturating_sub(1));
    let y1 = (y0 + 1).min(rgb.height().saturating_sub(1));
    let fx = sx - x0 as f32;
    let fy = sy - y0 as f32;

    let c00 = rgb.get_pixel(x0, y0).0;
    let c10 = rgb.get_pixel(x1, y0).0;
    let c01 = rgb.get_pixel(x0, y1).0;
    let c11 = rgb.get_pixel(x1, y1).0;

    let blend = |a: u8, b: u8, c: u8, d: u8| -> u8 {
        let v = (1.0 - fx) * (1.0 - fy) * a as f32
            + fx * (1.0 - fy) * b as f32
            + (1.0 - fx) * fy * c as f32
            + fx * fy * d as f32;
        v.clamp(0.0, 255.0) as u8
    };

    Rgb([
        blend(c00[0], c10[0], c01[0], c11[0]),
        blend(c00[1], c10[1], c01[1], c11[1]),
        blend(c00[2], c10[2], c01[2], c11[2]),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use trueid_core::BoundingBox;

    #[test]
    fn square_crop_full_frame_stays_full() {
        let b = BoundingBox::full_frame();
        let s = square_crop_bbox(&b, 0.25);
        assert!((s.w - 1.0).abs() < 1e-5);
        assert!((s.x).abs() < 1e-5);
    }
}
