# trueid

Linux face auth in Rust (PAM + daemon). Work in progress.

* [Architecture](docs/architecture.md)
* [Run / config](docs/developing.md)
* [Models](docs/models.md)

## Install

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

### IR Camera

IR emitters often need [linux-enable-ir-emitter](https://github.com/EmixamPP/linux-enable-ir-emitter).