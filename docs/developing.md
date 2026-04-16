# Run locally

From the repo root.

## Daemon

Config: `/etc/trueid/config.yaml` if present, else `crates/trueid-daemon/config/config.yaml` in the repo.

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

Real camera: set `models.face_embedding` and `models.face_detector` to your ONNX paths (defaults under `/var/lib/trueid/models/`).

`paths.debug_v4l_frames`: decoded frames as PNGs under `rgb/…` or `ir/…` per streaming session.

`verification.capture`: `warmup_discard` frames are discarded, then at most `max_frames` frames are pulled per enroll/verify/add-template (YAML may use legacy key `frame_count`).

## CLI

Another terminal:

```bash
cargo run -p trueid-ctl -- ping
cargo run -p trueid-ctl -- enroll
cargo run -p trueid-ctl -- verify
```

## Logging

`logging.level` sets the default `tracing` filter; override with `RUST_LOG` if needed.
