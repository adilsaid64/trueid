//! OpenCV Zoo [YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet) ONNX (`face_detection_yunet_2023mar.onnx`).
//!
//! Pre/postprocess matches OpenCV `FaceDetectorYN` (`face_detect.cpp`): BGR NCHW float 0–255,
//! fixed input `1×3×640×640`, strides 8/16/32.

use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use image::{DynamicImage, Rgb, RgbImage, imageops::FilterType};
use tract_onnx::prelude::*;
use trueid_core::ports::{DetectError, FaceDetector};
use trueid_core::{BoundingBox, FaceDetection, FaceLandmarks, Frame, PixelFormat};

const DIVISOR: u32 = 32;
const INPUT_W: u32 = 640;
const INPUT_H: u32 = 640;

/// YuNet with OpenCV-default thresholds.
pub struct OnnxYuNetDetector {
    model: Mutex<TypedRunnableModel<TypedModel>>,
    score_threshold: f32,
    nms_threshold: f32,
    top_k: usize,
}

impl OnnxYuNetDetector {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, DetectError> {
        Self::from_file_with_thresholds(path, 0.6, 0.3, 5000)
    }

    pub fn from_file_with_thresholds(
        path: impl AsRef<Path>,
        score_threshold: f32,
        nms_threshold: f32,
        top_k: usize,
    ) -> Result<Self, DetectError> {
        let path = path.as_ref();
        let model = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(|e| DetectError::Failed(format!("load onnx {path:?}: {e}")))?
            .into_optimized()
            .map_err(|e| DetectError::Failed(format!("optimize onnx {path:?}: {e}")))?;

        let model = model
            .into_runnable()
            .map_err(|e| DetectError::Failed(format!("runnable: {e}")))?;

        Ok(Self {
            model: Mutex::new(model),
            score_threshold,
            nms_threshold,
            top_k,
        })
    }

    fn pad_dims(input_w: u32, input_h: u32) -> (u32, u32) {
        let pad_w = ((input_w.saturating_sub(1)) / DIVISOR + 1) * DIVISOR;
        let pad_h = ((input_h.saturating_sub(1)) / DIVISOR + 1) * DIVISOR;
        (pad_w, pad_h)
    }

    fn preprocess(frame: &Frame) -> Result<Tensor, DetectError> {
        let rgb = frame_to_rgb_image(frame)?;
        let dyn_img = DynamicImage::ImageRgb8(rgb);
        let resized = dyn_img
            .resize_exact(INPUT_W, INPUT_H, FilterType::Triangle)
            .to_rgb8();
        let (pad_w, pad_h) = Self::pad_dims(INPUT_W, INPUT_H);
        let mut padded = RgbImage::new(pad_w, pad_h);
        for y in 0..INPUT_H {
            for x in 0..INPUT_W {
                padded.put_pixel(x, y, *resized.get_pixel(x, y));
            }
        }
        // Bottom/right padding is already black (0,0,0).

        let mut data = Vec::with_capacity(3 * pad_h as usize * pad_w as usize);
        // NCHW BGR, float 0–255 (OpenCV dnn::blobFromImage default scale).
        for c in [2u8, 1, 0] {
            for y in 0..pad_h {
                for x in 0..pad_w {
                    let p = padded.get_pixel(x, y).0;
                    data.push(p[c as usize] as f32);
                }
            }
        }
        Tensor::from_shape(&[1, 3, pad_h as usize, pad_w as usize], &data)
            .map_err(|e| DetectError::Failed(format!("input tensor: {e}")))
    }

    fn postprocess(
        outputs: TVec<TValue>,
        pad_w: u32,
        pad_h: u32,
        score_threshold: f32,
        nms_threshold: f32,
        top_k: usize,
    ) -> Result<Vec<FaceCandidate>, DetectError> {
        if outputs.len() != 12 {
            return Err(DetectError::Failed(format!(
                "yunet expects 12 outputs, got {}",
                outputs.len()
            )));
        }

        let strides = [8i32, 16, 32];
        let mut candidates = Vec::new();

        for i in 0..3 {
            let stride = strides[i];
            let cols = (pad_w as i32 / stride) as usize;
            let rows = (pad_h as i32 / stride) as usize;

            let cls = tensor_to_vec_f32(&outputs[i])?;
            let obj = tensor_to_vec_f32(&outputs[i + 3])?;
            let bbox = tensor_to_vec_f32(&outputs[i + 6])?;
            let kps = tensor_to_vec_f32(&outputs[i + 9])?;

            let expected = rows * cols;
            if cls.len() < expected || obj.len() < expected {
                return Err(DetectError::Failed("cls/obj shape mismatch".into()));
            }
            if bbox.len() < expected * 4 || kps.len() < expected * 10 {
                return Err(DetectError::Failed("bbox/kps shape mismatch".into()));
            }

            for r in 0..rows {
                for c in 0..cols {
                    let idx = r * cols + c;
                    let cls_score = cls[idx].clamp(0.0, 1.0);
                    let obj_score = obj[idx].clamp(0.0, 1.0);
                    let score = (cls_score * obj_score).sqrt();
                    if score < score_threshold {
                        continue;
                    }

                    let cx = (c as f32 + bbox[idx * 4]) * stride as f32;
                    let cy = (r as f32 + bbox[idx * 4 + 1]) * stride as f32;
                    let w = bbox[idx * 4 + 2].exp() * stride as f32;
                    let h = bbox[idx * 4 + 3].exp() * stride as f32;
                    let x1 = cx - w / 2.0;
                    let y1 = cy - h / 2.0;

                    let mut lm = [(0.0f32, 0.0f32); 5];
                    for n in 0..5 {
                        let lx = (kps[idx * 10 + 2 * n] + c as f32) * stride as f32;
                        let ly = (kps[idx * 10 + 2 * n + 1] + r as f32) * stride as f32;
                        lm[n] = (lx, ly);
                    }

                    candidates.push(FaceCandidate {
                        x1,
                        y1,
                        w,
                        h,
                        score,
                        landmarks: lm,
                    });
                }
            }
        }

        let kept = nms(&candidates, nms_threshold, top_k);
        Ok(kept.into_iter().map(|i| candidates[i].clone()).collect())
    }
}

#[derive(Clone)]
struct FaceCandidate {
    x1: f32,
    y1: f32,
    w: f32,
    h: f32,
    score: f32,
    landmarks: [(f32, f32); 5],
}

fn tensor_to_vec_f32(t: &TValue) -> Result<Vec<f32>, DetectError> {
    let v = t
        .to_array_view::<f32>()
        .map_err(|e| DetectError::Failed(format!("tensor f32 view: {e}")))?;
    Ok(v.iter().copied().collect())
}

fn iou(a: &FaceCandidate, b: &FaceCandidate) -> f32 {
    let ax2 = a.x1 + a.w;
    let ay2 = a.y1 + a.h;
    let bx2 = b.x1 + b.w;
    let by2 = b.y1 + b.h;
    let ix1 = a.x1.max(b.x1);
    let iy1 = a.y1.max(b.y1);
    let ix2 = ax2.min(bx2);
    let iy2 = ay2.min(by2);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 1e-6 {
        return 0.0;
    }
    inter / union
}

fn nms(candidates: &[FaceCandidate], nms_threshold: f32, top_k: usize) -> Vec<usize> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&i, &j| {
        candidates[j]
            .score
            .partial_cmp(&candidates[i].score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut keep = Vec::new();
    let mut removed = vec![false; candidates.len()];

    for &i in &order {
        if removed[i] {
            continue;
        }
        keep.push(i);
        if keep.len() >= top_k {
            break;
        }
        for j in 0..candidates.len() {
            if i == j || removed[j] {
                continue;
            }
            if iou(&candidates[i], &candidates[j]) > nms_threshold {
                removed[j] = true;
            }
        }
    }
    keep
}

fn frame_to_rgb_image(frame: &Frame) -> Result<RgbImage, DetectError> {
    let w = frame.width as usize;
    let h = frame.height as usize;
    match frame.format {
        PixelFormat::Rgb8 => {
            let expected = w
                .checked_mul(h)
                .and_then(|n| n.checked_mul(3))
                .ok_or_else(|| DetectError::Failed("frame dimensions overflow".into()))?;
            if frame.bytes.len() != expected {
                return Err(DetectError::Failed(format!(
                    "rgb8 length {} != {}×{}×3",
                    frame.bytes.len(),
                    frame.width,
                    frame.height
                )));
            }
            RgbImage::from_raw(frame.width, frame.height, frame.bytes.clone())
                .ok_or_else(|| DetectError::Failed("invalid rgb8 buffer".into()))
        }
        PixelFormat::Gray8 => {
            if frame.bytes.len() != w * h {
                return Err(DetectError::Failed(format!(
                    "gray8 length {} != {}×{}",
                    frame.bytes.len(),
                    frame.width,
                    frame.height
                )));
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

/// Map YuNet padded 640-space pixels to normalized coords vs original [`Frame`].
fn to_detection(candidate: &FaceCandidate, frame: &Frame) -> FaceDetection {
    let fw = frame.width as f32;
    let fh = frame.height as f32;
    // Model space is INPUT_W×INPUT_H stretch of the full frame.
    let sx = fw / INPUT_W as f32;
    let sy = fh / INPUT_H as f32;

    let x1 = (candidate.x1 * sx / fw).clamp(0.0, 1.0);
    let y1 = (candidate.y1 * sy / fh).clamp(0.0, 1.0);
    let w = (candidate.w * sx / fw).clamp(0.0, 1.0);
    let h = (candidate.h * sy / fh).clamp(0.0, 1.0);

    // YuNet order: re, le, nt, rcm, lcm → domain: le, re, nt, mouth_left, mouth_right
    let lm = candidate.landmarks;
    let le = lm[1];
    let re = lm[0];
    let nt = lm[2];
    let rcm = lm[3];
    let lcm = lm[4];

    let norm_lm = |px: f32, py: f32| {
        let nx = (px * sx / fw).clamp(0.0, 1.0);
        let ny = (py * sy / fh).clamp(0.0, 1.0);
        (nx, ny)
    };

    FaceDetection {
        bbox: BoundingBox { x: x1, y: y1, w, h },
        landmarks: Some(FaceLandmarks {
            left_eye: norm_lm(le.0, le.1),
            right_eye: norm_lm(re.0, re.1),
            nose_tip: norm_lm(nt.0, nt.1),
            mouth_left: norm_lm(lcm.0, lcm.1),
            mouth_right: norm_lm(rcm.0, rcm.1),
        }),
    }
}

impl FaceDetector for OnnxYuNetDetector {
    fn detect_primary(&self, frame: &Frame) -> Result<Option<FaceDetection>, DetectError> {
        let t0 = Instant::now();
        let tensor = Self::preprocess(frame)?;
        let input = tensor.into_tvalue();
        let t_inf = Instant::now();
        let outputs = self
            .model
            .lock()
            .map_err(|_| DetectError::Failed("detector lock poisoned".into()))?
            .run(tvec!(input))
            .map_err(|e| DetectError::Failed(format!("inference: {e}")))?;
        let infer_ms = t_inf.elapsed().as_millis();

        let (pad_w, pad_h) = Self::pad_dims(INPUT_W, INPUT_H);
        let mut faces = Self::postprocess(
            outputs,
            pad_w,
            pad_h,
            self.score_threshold,
            self.nms_threshold,
            self.top_k,
        )?;

        if faces.is_empty() {
            tracing::debug!(
                infer_ms,
                total_ms = t0.elapsed().as_millis(),
                w = frame.width,
                h = frame.height,
                "yunet: no face after NMS"
            );
            return Ok(None);
        }
        faces.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let best = &faces[0];
        tracing::debug!(
            infer_ms,
            total_ms = t0.elapsed().as_millis(),
            score = best.score,
            candidates = faces.len(),
            w = frame.width,
            h = frame.height,
            "yunet: primary face"
        );
        Ok(Some(to_detection(best, frame)))
    }
}

/// `TRUEID_FACE_DETECTOR_MODEL` or `$XDG_DATA_HOME/trueid/models/face_detection_yunet_2023mar.onnx`.
pub fn default_detector_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("TRUEID_FACE_DETECTOR_MODEL") {
        return Some(std::path::PathBuf::from(p));
    }

    let system_path =
        std::path::PathBuf::from("/var/lib/trueid/models/face_detection_yunet_2023mar.onnx");
    if system_path.exists() {
        return Some(system_path);
    }

    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
        })?;

    Some(base.join("trueid/models/face_detection_yunet_2023mar.onnx"))
}

pub fn build_face_detector() -> Result<std::sync::Arc<dyn FaceDetector>, String> {
    let path = default_detector_path().ok_or_else(|| {
        "TRUEID_FACE_DETECTOR_MODEL not set and could not resolve XDG_DATA_HOME or HOME".to_string()
    })?;
    if !path.exists() {
        return Err(format!(
            "face detector ONNX not found at {}.\n\
             Download OpenCV Zoo YuNet: face_detection_yunet_2023mar.onnx into that folder, or set TRUEID_FACE_DETECTOR_MODEL.\n\
             Or set TRUEID_USE_MOCK_DETECTOR=1 for full-frame stub.",
            path.display()
        ));
    }
    Ok(std::sync::Arc::new(
        OnnxYuNetDetector::from_file(&path).map_err(|e| e.to_string())?,
    ))
}
