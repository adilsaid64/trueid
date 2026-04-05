# trueid
My attempt at building a biometric authentication module  for linux systems in Rust.  A Windows Hello like system for linux and an alternative to Howdy

Still a WIP :) and open to contributions


* [Architecture](docs/architecture.md)
* [Run / config](docs/developing.md)
* [Model](docs/models.md)

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

Your IR camera might not work straight a way. You need to configure it with this [linux-enable-ir-emitter](https://github.com/EmixamPP/linux-enable-ir-emitter)