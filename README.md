# trueid

Linux facial authentication system written in Rust.

A Windows Hello–like experience for Linux. You run **either** an RGB **or** an IR camera. enrollment and verification use the same modality.

Project is still a work in progress and open to contributions :)

## How it works (current behavior)

- **trueid-daemon** opens a **streaming session** on the configured camera (`VideoSource::open_session`), pulls frames with `next_frame`, and runs detect → align → liveness → embed per frame until limits in config are reached.
- **Templates** are stored as a single list of embeddings per user (JSON on disk under `paths.templates`).
- **Verify** compares probe embeddings against enrolled templates (quorum-style matching over the stream). There is **no** separate RGB/IR fusion; use one stream type per deployment.

## Features I want to add next

- Better liveness detector. Currently anything passes liveness; streaming gives more room to improve this later.

- Extend the CLI tool: delete templates, edit config, etc.

- Storing embeddings as encrypted files rather than raw JSON.

- Maybe notifications, a small UI—still undecided.

## Components

`trueid` is composed of three core components:

| Component | Description | Responsibilities |
| --------- | ----------- | ---------------- |
| **trueid-ctl** | CLI for talking to the daemon | Enroll, verify, add-template, `get-models`, ping |
| **trueid-pam** | PAM module | Hooks into login, `sudo`, and other PAM services |
| **trueid-daemon** | Background service | V4L (or mock) video session, ONNX face pipeline, template I/O, Unix socket IPC |

* [Architecture](docs/architecture.md) (may lag the code slightly—prefer this README + `docs/developing.md` for current behavior)
* [Run / config](docs/developing.md)
* [Models](docs/models.md)

## Installation

### Ubuntu / Debian

```bash
wget https://github.com/adilsaid64/trueid/releases/latest/download/trueid-*-ubuntu.deb
sudo dpkg -i trueid-*-ubuntu.deb
```

### Fedora

```bash
wget https://github.com/adilsaid64/trueid/releases/latest/download/trueid-*-fedora.rpm
sudo dnf install ./trueid-*-fedora.rpm
```

### Build from source

```bash
git clone https://github.com/adilsaid64/trueid
cd trueid
cargo build --release
```

## Camera: RGB or IR

Configure **exactly one** of `camera.enable_rgb` or `camera.enable_ir`. Set `rgb_index` or `ir_index` to match `/dev/video*`.

If you use a Windows Hello–style device, you may need the IR emitter enabled with [linux-enable-ir-emitter](https://github.com/EmixamPP/linux-enable-ir-emitter).

Quick checks:

```bash
ls /dev/video*
ffplay /dev/video2   # example: inspect a given index
```

## Usage

After installation:

### 1. Download ML models

```bash
sudo trueid-ctl get-models
```

### 2. Edit config

```bash
sudo vim /etc/trueid/config.yaml
```

Important keys:

- **Camera:** `enable_rgb` / `enable_ir` (one true), `rgb_index` / `ir_index`, resolution, `mock` for dev without hardware.
- **Paths:** `paths.templates` must be writable by the daemon user (often `/var/lib/trueid/templates`).
- **Verification:** `verification.match_threshold`, and `verification.capture` with `warmup_discard` plus **`max_frames`** per operation (enroll vs verify). Legacy YAML may still use `frame_count`; it is accepted as an alias for `max_frames`.

Enroll/verify can take **tens of seconds** (camera + models). The CLI waits up to the IPC read timeout (see `trueid_ipc::IPC_READ_TIMEOUT`).

Then restart the service, e.g. `sudo systemctl restart trueid`.

### 3. Enroll

Use your Linux uid (`id -u`):

```bash
sudo trueid-ctl enroll --uid 1000
```

### 4. Test verify

```bash
sudo trueid-ctl verify --uid 1000
```

### 5. Add more templates

```bash
sudo trueid-ctl add-template --uid 1000
```

## PAM integration

Add this line to the PAM service you want to enable trueid for:

```
auth    [success=1 default=ignore] pam_trueid.so
```
