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
| `TRUEID_CAMERA_INDEX` | `/dev/video{N}` | `0` |
| `TRUEID_CAPTURE_WIDTH` / `TRUEID_CAPTURE_HEIGHT` | Requested capture size | `640` × `480` |
| `TRUEID_TEMPLATE_DIR` | Enrolled templates (JSON per user) | `$XDG_DATA_HOME/trueid/templates` |
| `TRUEID_FACE_MODEL` | ONNX face model | unset → `$XDG_DATA_HOME/trueid/models/face_embedding.onnx` |
| `TRUEID_MATCH_THRESHOLD` | Cosine match after L2-normalizing embeddings | `0.45` |

**ONNX embedder** — Expects float32 rank-4 input, NCHW `[1,3,H,W]` or NHWC `[1,H,W,3]` (common face sizes e.g. 112×112). Frames are resized to the model’s `H×W`, channels normalized `(x - 127.5) / 128.0`, output L2-normalized before matching. Details: `crates/trueid-daemon/src/adapters/face_embedder/onnx.rs`.

**Default face pipeline** — Full-frame detect, passthrough align, always-live liveness (`face_detector/`, `face_aligner/`, `liveness/`). Swap implementations by changing adapters wired in `main.rs`.
