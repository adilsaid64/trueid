//! Five-point landmark similarity warp to InsightFace 112 reference (YuNet) or square bbox crop.
//! Optional `TRUEID_DEBUG_ALIGNED_DIR`: write each aligned face as PNG for debugging.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use image::imageops::FilterType;
use image::{DynamicImage, Rgb, RgbImage};
use nalgebra::{Matrix2, SVD};
use trueid_core::ports::{AlignError, FaceAligner};
use trueid_core::{BoundingBox, FaceDetection, FaceLandmarks, Frame, PixelFormat};

/// Default aligned face size (InsightFace / ArcFace-style models often use 112×112).
const DEFAULT_OUTPUT: u32 = 112;

/// ArcFace / InsightFace 112×112 canonical template (same order as [`FaceLandmarks`]).
const REF112_FIVE: [(f32, f32); 5] = [
    (38.2946, 51.6963),   // left eye
    (73.5318, 51.5014),   // right eye
    (56.0252, 71.7366),   // nose
    (41.5493, 92.3655),   // mouth left
    (70.7299, 92.2041),   // mouth right
];

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
            warp_similarity_five_point(&rgb, frame.width, frame.height, lm, out)?
        } else {
            tracing::warn!(
                "align: no landmarks; using square bbox crop (poor match for ArcFace-style embedders)"
            );
            let bb = square_crop_bbox(&detection.bbox, self.margin);
            tracing::trace!(?bb, "align: bbox-only crop (no landmarks)");
            crop_and_resize(&rgb, frame.width, frame.height, &bb, out)?
        };

        tracing::debug!(
            mode = if has_landmarks {
                "similarity_five_point"
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

/// Umeyama similarity (InsightFace `norm_crop` / scikit-image `SimilarityTransform`).
/// Maps `src` landmark pixels → `dst` reference: `dst ≈ c * R * src + t` with orthogonal `R`.
fn umeyama_similarity_2d(
    src: &[[f32; 2]; 5],
    dst: &[[f32; 2]; 5],
) -> Result<([f32; 4], [f32; 2]), AlignError> {
    let n = 5.0f64;
    let mut mu_s = [0f64; 2];
    let mut mu_d = [0f64; 2];
    for i in 0..5 {
        mu_s[0] += src[i][0] as f64;
        mu_s[1] += src[i][1] as f64;
        mu_d[0] += dst[i][0] as f64;
        mu_d[1] += dst[i][1] as f64;
    }
    mu_s[0] /= n;
    mu_s[1] /= n;
    mu_d[0] /= n;
    mu_d[1] /= n;

    let mut h = Matrix2::zeros();
    let mut var_s = 0.0f64;
    for i in 0..5 {
        let scx = src[i][0] as f64 - mu_s[0];
        let scy = src[i][1] as f64 - mu_s[1];
        let dcx = dst[i][0] as f64 - mu_d[0];
        let dcy = dst[i][1] as f64 - mu_d[1];
        var_s += scx * scx + scy * scy;
        h += Matrix2::new(scx * dcx, scx * dcy, scy * dcx, scy * dcy);
    }

    if var_s < 1e-18 {
        return Err(AlignError::Failed("degenerate source landmarks".into()));
    }

    let svd = SVD::new(h, true, true);
    let u = svd.u.ok_or_else(|| AlignError::Failed("SVD(U) failed".into()))?;
    let mut v_t = svd.v_t.ok_or_else(|| AlignError::Failed("SVD(Vt) failed".into()))?;
    let sig = svd.singular_values;

    let mut r = v_t.transpose() * u.transpose();
    if r.determinant() < 0.0 {
        let last = v_t.nrows() - 1;
        for j in 0..v_t.ncols() {
            v_t[(last, j)] = -v_t[(last, j)];
        }
        r = v_t.transpose() * u.transpose();
    }

    let c = (sig[0] + sig[1]) / var_s;
    let mu_s_v = nalgebra::Vector2::new(mu_s[0], mu_s[1]);
    let mu_d_v = nalgebra::Vector2::new(mu_d[0], mu_d[1]);
    let t_v = mu_d_v - c * r * mu_s_v;

    let lin = c * r;

    let m00 = lin[(0, 0)] as f32;
    let m01 = lin[(0, 1)] as f32;
    let m10 = lin[(1, 0)] as f32;
    let m11 = lin[(1, 1)] as f32;

    let det = m00 * m11 - m01 * m10;
    if det.abs() < 1e-12 {
        return Err(AlignError::Failed("similarity transform degenerate".into()));
    }

    tracing::trace!(
        m00, m01, m10, m11,
        tx = t_v.x,
        ty = t_v.y,
        scale = c,
        "align: umeyama similarity fit"
    );

    Ok(([m00, m01, m10, m11], [t_v.x as f32, t_v.y as f32]))
}

fn warp_similarity_five_point(
    rgb: &RgbImage,
    fw: u32,
    fh: u32,
    lm: &FaceLandmarks,
    out: u32,
) -> Result<RgbImage, AlignError> {
    let s = out as f32 / 112.0;
    let fw_f = fw as f32;
    let fh_f = fh as f32;

    let src: [[f32; 2]; 5] = [
        [lm.left_eye.0 * fw_f, lm.left_eye.1 * fh_f],
        [lm.right_eye.0 * fw_f, lm.right_eye.1 * fh_f],
        [lm.nose_tip.0 * fw_f, lm.nose_tip.1 * fh_f],
        [lm.mouth_left.0 * fw_f, lm.mouth_left.1 * fh_f],
        [lm.mouth_right.0 * fw_f, lm.mouth_right.1 * fh_f],
    ];

    let mut dst = [[0f32; 2]; 5];
    for (i, r) in REF112_FIVE.iter().enumerate() {
        dst[i][0] = r.0 * s;
        dst[i][1] = r.1 * s;
    }

    let (m, t) = umeyama_similarity_2d(&src, &dst)?;
    let (m00, m01, m10, m11) = (m[0], m[1], m[2], m[3]);
    let (tx, ty) = (t[0], t[1]);

    let det = m00 * m11 - m01 * m10;

    let mut out_img = RgbImage::new(out, out);
    for oy in 0..out {
        for ox in 0..out {
            let dst_x = ox as f32 + 0.5;
            let dst_y = oy as f32 + 0.5;

            let px = dst_x - tx;
            let py = dst_y - ty;

            let sx = ( m11 * px - m01 * py) / det;
            let sy = (-m10 * px + m00 * py) / det;

            let p = sample_bilinear(rgb, sx, sy, fw_f, fh_f);
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

    #[test]
    fn umeyama_recovers_identity() {
        let src: [[f32; 2]; 5] = [
            [10.0, 20.0],
            [80.0, 25.0],
            [45.0, 60.0],
            [30.0, 95.0],
            [72.0, 92.0],
        ];
        let dst = src;
    
        let (m, t) = umeyama_similarity_2d(&src, &dst).unwrap();
        let (m00, m01, m10, m11) = (m[0], m[1], m[2], m[3]);
        let (tx, ty) = (t[0], t[1]);
    
        // Identity matrix
        assert!((m00 - 1.0).abs() < 1e-3, "m00={m00}");
        assert!(m01.abs() < 1e-3, "m01={m01}");
        assert!(m10.abs() < 1e-3, "m10={m10}");
        assert!((m11 - 1.0).abs() < 1e-3, "m11={m11}");
    
        // Zero translation
        assert!(tx.abs() < 1e-2, "tx={tx}");
        assert!(ty.abs() < 1e-2, "ty={ty}");
    }
    #[test]
    fn umeyama_recovers_uniform_scale_and_translation() {
        let src: [[f32; 2]; 5] = [
            [10.0, 5.0],
            [30.0, 8.0],
            [20.0, 18.0],
            [12.0, 40.0],
            [28.0, 38.0],
        ];
    
        let scale = 2.0_f32;
        let tx = 3.0_f32;
        let ty = -7.0_f32;
    
        let mut dst = [[0f32; 2]; 5];
        for i in 0..5 {
            dst[i][0] = scale * src[i][0] + tx;
            dst[i][1] = scale * src[i][1] + ty;
        }
    
        let (m, t) = umeyama_similarity_2d(&src, &dst).unwrap();
        let (m00, m01, m10, m11) = (m[0], m[1], m[2], m[3]);
        let (got_tx, got_ty) = (t[0], t[1]);
    
        // Scale matrix (no rotation)
        assert!(m01.abs() < 1e-3, "m01={m01}");
        assert!(m10.abs() < 1e-3, "m10={m10}");
        assert!((m00 - scale).abs() < 1e-2, "m00={m00}");
        assert!((m11 - scale).abs() < 1e-2, "m11={m11}");
    
        // Translation
        assert!((got_tx - tx).abs() < 1e-2, "tx={got_tx}");
        assert!((got_ty - ty).abs() < 1e-2, "ty={got_ty}");
    }
}
