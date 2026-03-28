# Run locally

From the repo root.

## Daemon

Mock camera + mock face embedder (no ONNX file):

```bash
TRUEID_USE_MOCK=1 TRUEID_USE_MOCK_EMBEDDER=1 cargo run -p trueid-daemon
```

Mock camera, real ONNX embedder:

```bash
TRUEID_USE_MOCK=1 cargo run -p trueid-daemon
```

Real camera:

```bash
cargo run -p trueid-daemon
```

## Face detector ONNX (YuNet)

Default is **OpenCV Zoo YuNet** (`face_detection_yunet_2023mar.onnx`, fixed **640×640** input). Place it at:

`$XDG_DATA_HOME/trueid/models/face_detection_yunet_2023mar.onnx`

or set **`TRUEID_FACE_DETECTOR_MODEL`** to the file path.

Download (same file the adapter expects):

`https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx`

For **full-frame stub** (no detector model): `TRUEID_USE_MOCK_DETECTOR=1`.

## CLI

Another terminal:

```bash
cargo run -p trueid-ctl -- ping
cargo run -p trueid-ctl -- enroll
cargo run -p trueid-ctl -- verify
```

## Config

| Variable | Role | Default |
|----------|------|---------|
| `TRUEID_USE_MOCK` | Use in-memory frames instead of V4L | off |
| `TRUEID_USE_MOCK_EMBEDDER` | Constant embedding (no ONNX) | off |
| `TRUEID_USE_MOCK_DETECTOR` | Full-frame face stub (no YuNet ONNX) | off |
| `TRUEID_CAMERA_INDEX` | `/dev/video{N}` | `0` |
| `TRUEID_CAPTURE_WIDTH` / `TRUEID_CAPTURE_HEIGHT` | Requested capture size | `640` × `480` |
| `TRUEID_TEMPLATE_DIR` | Enrolled templates (JSON per user) | `$XDG_DATA_HOME/trueid/templates` |
| `TRUEID_FACE_MODEL` | ONNX embedder | unset → `$XDG_DATA_HOME/trueid/models/face_embedding.onnx` |
| `TRUEID_FACE_DETECTOR_MODEL` | ONNX YuNet detector | unset → `$XDG_DATA_HOME/trueid/models/face_detection_yunet_2023mar.onnx` |
| `TRUEID_MATCH_THRESHOLD` | Cosine match after L2-normalizing embeddings | `0.45` |

**ONNX embedder** — Expects float32 rank-4 input, NCHW `[1,3,H,W]` or NHWC `[1,H,W,3]` (common face sizes e.g. 112×112). Frames are resized to the model’s `H×W`, channels normalized `(x - 127.5) / 128.0`, output L2-normalized before matching. Details: `crates/trueid-daemon/src/adapters/face_embedder/onnx.rs`.

**ONNX detector** — YuNet only: BGR NCHW float 0–255, `1×3×640×640`, postprocess aligned with OpenCV `FaceDetectorYN`. See `face_detector/onnx_yunet.rs`.

**Default face pipeline** — YuNet detect (or mock), passthrough align, always-live liveness. Swap implementations in `main.rs`.
