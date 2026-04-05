# trueid

Linux facial authentication system written in Rust.  

A Windows Hello–like experience for Linux, with support for RGB and optional IR cameras.

Project is still a work in progress and open to contributions :)


## Components

`trueid` is composed of three core components:

| Component            | Description                          | Responsibilities                                                                                               |
| -------------------- | ------------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| **trueid-ctl**    | CLI tool for interacting with trueid | Enroll users, verify authentication, manage templates, download models                                         |
| **trueid-pam** | PAM module for system integration    | Hooks into login, `sudo`, and other PAM services                                                               |
| **trueid-daemon**        | Background daemon                    | Camera capture (RGB/IR), face detection, alignment, liveness checks, embedding + matching, template management |


* [Architecture](docs/architecture.md)
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

## IR Camera

If you're using a Windows Hello–compatible device or any camera with IR support, you may need to enable the IR emitter with [linux-enable-ir-emitter](https://github.com/EmixamPP/linux-enable-ir-emitter)

## Usage

After installaion

### 1. Download ML Models
Download embedding and face detection models first.

```bash
sudo trueid-ctl get-models
```

### 2. Edit Config
By default, IR is disabled. You may also need to adjust the video device indices for your RGB and IR cameras.

Typical defaults:

- RGB: /dev/video0
- IR: /dev/video2

```bash
sudo vim /etc/trueid/config.yaml
```

Then restart the processes by `sudo systemctl restart trueid`

### 3. Enroll

You can find the `uid` you should use by running  `id -u`

```bash
sudo trueid-ctl enroll --uid 1000
```

### 4. Test Verify

```bash
sudo trueid-ctl verify --uid 1000
```

### 5. Add more templates
Capture more samples to improve accuracy under different conditions

```bash
sudo trueid-ctl add-template --uid 1000
```


