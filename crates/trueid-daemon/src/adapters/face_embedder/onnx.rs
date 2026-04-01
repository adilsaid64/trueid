//! ONNX inference via tract.
//!
//! Input: float32 NCHW `[1,3,H,W]` or NHWC `[1,H,W,3]` (often 112×112).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{DynamicImage, Rgb, RgbImage, imageops::FilterType};
use tract_onnx::prelude::*;
use trueid_core::ports::{FaceEmbedError, FaceEmbedder};
use trueid_core::{Embedding, Frame, PixelFormat};

/// [`FaceEmbedder`] via ONNX.
pub struct OnnxFaceEmbedder {
    model: std::sync::Mutex<TypedRunnableModel<TypedModel>>,
    layout: InputLayout,
    input_h: usize,
    input_w: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputLayout {
    Nchw,
    Nhwc,
}

impl OnnxFaceEmbedder {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, FaceEmbedError> {
        let path = path.as_ref();
        let model = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(|e| FaceEmbedError::Failed(format!("load onnx {path:?}: {e}")))?
            .with_input_fact(
                0,
                InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, 112, 112)),
            )
            .map_err(|e| FaceEmbedError::Failed(format!("set input fact: {e}")))?
            .into_optimized()
            .map_err(|e| FaceEmbedError::Failed(format!("optimize onnx {path:?}: {e}")))?;

        let fact = model
            .input_fact(0)
            .map_err(|e| FaceEmbedError::Failed(format!("input fact: {e}")))?;
        if fact.datum_type != DatumType::F32 {
            return Err(FaceEmbedError::Failed(format!(
                "expected f32 model input, got {:?}",
                fact.datum_type
            )));
        }

        let shape = fact.shape.as_concrete().ok_or_else(|| {
            FaceEmbedError::Failed(
                "model input shape must be fully known (fix batch or export a fixed-shape ONNX)"
                    .into(),
            )
        })?;

        let (layout, input_h, input_w) = interpret_input_shape(shape)?;

        let model = model
            .into_runnable()
            .map_err(|e| FaceEmbedError::Failed(format!("runnable: {e}")))?;

        Ok(Self {
            model: std::sync::Mutex::new(model),
            layout,
            input_h,
            input_w,
        })
    }
}

fn interpret_input_shape(shape: &[usize]) -> Result<(InputLayout, usize, usize), FaceEmbedError> {
    match shape.len() {
        4 => {
            if shape[1] == 3 && shape[2] != 3 {
                Ok((InputLayout::Nchw, shape[2], shape[3]))
            } else if shape[3] == 3 {
                Ok((InputLayout::Nhwc, shape[1], shape[2]))
            } else {
                Err(FaceEmbedError::Failed(format!(
                    "unrecognized 4D input shape {shape:?}: expected [1,3,H,W] or [1,H,W,3]"
                )))
            }
        }
        n => Err(FaceEmbedError::Failed(format!(
            "expected rank-4 ONNX input, got rank {n} shape {shape:?}"
        ))),
    }
}

/// `(rgb - 127.5) / 128.0` on 8-bit channels.
fn normalize_arcface_rgb(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    (
        (r - 127.5) / 128.0,
        (g - 127.5) / 128.0,
        (b - 127.5) / 128.0,
    )
}

fn frame_to_rgb_image(frame: &Frame) -> Result<RgbImage, FaceEmbedError> {
    let w = frame.width as usize;
    let h = frame.height as usize;
    match frame.format {
        PixelFormat::Rgb8 => {
            let expected = w
                .checked_mul(h)
                .and_then(|n| n.checked_mul(3))
                .ok_or_else(|| FaceEmbedError::Failed("frame dimensions overflow".into()))?;
            if frame.bytes.len() != expected {
                return Err(FaceEmbedError::Failed(format!(
                    "rgb8 length {} != {}×{}×3",
                    frame.bytes.len(),
                    frame.width,
                    frame.height
                )));
            }
            RgbImage::from_raw(frame.width, frame.height, frame.bytes.clone())
                .ok_or_else(|| FaceEmbedError::Failed("invalid rgb8 buffer".into()))
        }
        PixelFormat::Gray8 => {
            if frame.bytes.len() != w * h {
                return Err(FaceEmbedError::Failed(format!(
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

fn build_input_tensor(
    rgb: RgbImage,
    layout: InputLayout,
    out_h: usize,
    out_w: usize,
) -> Result<Tensor, FaceEmbedError> {
    let dyn_img = DynamicImage::ImageRgb8(rgb);
    let resized = dyn_img.resize_exact(out_w as u32, out_h as u32, FilterType::Triangle);
    let resized = resized.to_rgb8();

    let mut data = Vec::with_capacity(3 * out_h * out_w);
    match layout {
        InputLayout::Nchw => {
            for c in 0..3 {
                for y in 0..out_h {
                    for x in 0..out_w {
                        let p = resized.get_pixel(x as u32, y as u32).0;
                        let v = [p[0] as f32, p[1] as f32, p[2] as f32];
                        let n = normalize_arcface_rgb(v[0], v[1], v[2]);
                        let ch = [n.0, n.1, n.2][c];
                        data.push(ch);
                    }
                }
            }
            Tensor::from_shape(&[1, 3, out_h, out_w], &data)
        }
        InputLayout::Nhwc => {
            for y in 0..out_h {
                for x in 0..out_w {
                    let p = resized.get_pixel(x as u32, y as u32).0;
                    let n = normalize_arcface_rgb(p[0] as f32, p[1] as f32, p[2] as f32);
                    data.push(n.0);
                    data.push(n.1);
                    data.push(n.2);
                }
            }
            Tensor::from_shape(&[1, out_h, out_w, 3], &data)
        }
    }
    .map_err(|e| FaceEmbedError::Failed(format!("build tensor: {e}")))
}

fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-12 {
        for x in &mut v {
            *x /= n;
        }
    }
    v
}

impl FaceEmbedder for OnnxFaceEmbedder {
    fn embed(&self, frame: &Frame) -> Result<Embedding, FaceEmbedError> {
        let rgb = frame_to_rgb_image(frame)?;
        let tensor = build_input_tensor(rgb, self.layout, self.input_h, self.input_w)?;
        let input = tensor.into_tvalue();

        let outputs = self
            .model
            .lock()
            .map_err(|_| FaceEmbedError::Failed("embedder lock poisoned".into()))?
            .run(tvec!(input))
            .map_err(|e| FaceEmbedError::Failed(format!("inference: {e}")))?;

        let first = outputs
            .first()
            .ok_or_else(|| FaceEmbedError::Failed("model returned no outputs".into()))?;

        let view = first
            .to_array_view::<f32>()
            .map_err(|e| FaceEmbedError::Failed(format!("output dtype: {e}")))?;
        let flat: Vec<f32> = view.iter().copied().collect();
        Ok(Embedding(l2_normalize(flat)))
    }
}

/// `TRUEID_FACE_MODEL` or `/var/lib/trueid/models/face_embedding.onnx`
/// or `$XDG_DATA_HOME/trueid/models/face_embedding.onnx`.
pub fn default_model_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TRUEID_FACE_MODEL") {
        return Some(PathBuf::from(p));
    }

    let system_path = PathBuf::from("/var/lib/trueid/models/face_embedding.onnx");
    if system_path.exists() {
        return Some(system_path);
    }

    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;

    Some(base.join("trueid/models/face_embedding.onnx"))
}

/// Load ONNX embedder from disk or return an error string.
pub fn build_face_embedder() -> Result<Arc<dyn FaceEmbedder>, String> {
    let path = default_model_path().ok_or_else(|| {
        "TRUEID_FACE_MODEL not set and could not resolve XDG_DATA_HOME or HOME".to_string()
    })?;
    if !path.exists() {
        return Err(format!(
            "face ONNX model not found at {}.\n\
             Place an InsightFace-compatible ONNX (f32 input) there, or set TRUEID_FACE_MODEL.\n\
             For quick UI tests without a model, set TRUEID_USE_MOCK_EMBEDDER=1.",
            path.display()
        ));
    }
    Ok(Arc::new(
        OnnxFaceEmbedder::from_file(&path).map_err(|e| e.to_string())?,
    ))
}
