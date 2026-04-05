# Run locally

From the repo root.

## Daemon

The daemon reads **`config.yaml`**: first `/etc/trueid/config.yaml` if it exists, otherwise the file bundled with the crate (`crates/trueid-daemon/config/config.yaml`). All daemon settings live there — see that file for the full schema.

Mock camera + mock face embedder (no ONNX file): set in `config.yaml`:

```yaml
camera:
  mock: true
development:
  mock_embedder: true
  mock_detector: true
```

Then:

```bash
cargo run -p trueid-daemon
```

Real camera + ONNX models: point `models.face_embedding` and `models.face_detector` at your `.onnx` files (defaults assume `/var/lib/trueid/models/...`).

To inspect decoded V4L output (RGB as colour PNG, IR as greyscale PNG), set `paths.debug_v4l_frames` to a directory; each burst creates `rgb/burst_<nanos>/frame_*.png` and, when IR is enabled, `ir/burst_<nanos>/…`.

## CLI

Another terminal:

```bash
cargo run -p trueid-ctl -- ping
cargo run -p trueid-ctl -- enroll
cargo run -p trueid-ctl -- verify
```

## Logging

`logging.level` in `config.yaml` drives `tracing` (e.g. `info`, `debug`). You can still narrow modules with **`RUST_LOG`** if you want (standard `tracing-subscriber` behaviour), but the daemon no longer reads `TRUEID_*` environment variables for configuration.
