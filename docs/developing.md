# Run locally

From the repo root:

## Start the daemon

Without a camera (mock mode):

```bash
TRUEID_USE_MOCK=1 cargo run -p trueidd
````

With a real camera:

```bash
cargo run -p trueidd
```

---

## Use the CLI

In another terminal:

```bash
cargo run -p trueidctl -- ping
cargo run -p trueidctl -- enroll
cargo run -p trueidctl -- verify
```

---

## Optional config

* `TRUEID_CAMERA_INDEX` (default: `0`)
* `TRUEID_CAPTURE_WIDTH` (default: `640`)
* `TRUEID_CAPTURE_HEIGHT` (default: `480`)
* `TRUEID_TEMPLATE_DIR` (default: `$XDG_DATA_HOME/trueid/templates`)