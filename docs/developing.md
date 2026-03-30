# Run locally

From the repo root.

## Daemon

Mock camera + mock face embedder (no ONNX file):

```bash
TRUEID_USE_MOCK_VIDEO_SOURCE=1 TRUEID_USE_MOCK_EMBEDDER=1 cargo run -p trueid-daemon
```

Mock camera, real ONNX embedder:

```bash
TRUEID_USE_MOCK_VIDEO_SOURCE=1 cargo run -p trueid-daemon
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
| `TRUEID_USE_MOCK_VIDEO_SOURCE` | Use in-memory frames instead of V4L | off |
| `TRUEID_USE_MOCK_EMBEDDER` | Constant embedding (no ONNX) | off |
| `TRUEID_USE_MOCK_DETECTOR` | Full-frame face stub (no YuNet ONNX) | off |
| `TRUEID_USE_PASSTHROUGH_ALIGNER` | Skip crop/warp; pass full frame to embedder (pipeline testing) | off |
| `TRUEID_CAMERA_INDEX` | `/dev/video{N}` | `0` |
| `TRUEID_CAPTURE_WIDTH` / `TRUEID_CAPTURE_HEIGHT` | Requested capture size | `640` × `480` |
| `TRUEID_TEMPLATE_DIR` | Enrolled templates (JSON per user) | `$XDG_DATA_HOME/trueid/templates` |
| `TRUEID_FACE_MODEL` | Face embedder | unset → `$XDG_DATA_HOME/trueid/models/face_embedding.onnx` |
| `TRUEID_FACE_DETECTOR_MODEL` | Face Detector | unset → `$XDG_DATA_HOME/trueid/models/face_detection_yunet_2023mar.onnx` |
| `TRUEID_MATCH_THRESHOLD` | Cosine match after L2-normalizing embeddings | `0.45` |
| `TRUEID_V4L_ROTATE_180` | Rotate each camera frame 180° (upside-down sensor, no EXIF) | off |
| `TRUEID_V4L_FLIP_VERTICAL` | Vertical flip only (if 180° is wrong); ignored if `ROTATE_180` is set | off |
| `TRUEID_DEBUG_ALIGNED_DIR` | Directory to save aligned-face PNGs (debug) | unset |
