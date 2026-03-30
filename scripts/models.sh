#!/bin/sh
set -e

MODEL_DIR="/var/lib/trueid/models"

mkdir -p "$MODEL_DIR"

download() {
  url="$1"
  dest="$2"

  if [ -f "$dest" ]; then
    echo "Model already exists: $dest"
    return
  fi

  echo "Downloading $(basename "$dest")..."

  if command -v curl >/dev/null 2>&1; then
    curl -L --fail -o "$dest" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$dest" "$url"
  else
    echo "Error: neither curl nor wget is installed"
    exit 1
  fi
}

download \
  "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx" \
  "$MODEL_DIR/face_detection_yunet_2023mar.onnx"

download \
  "https://huggingface.co/immich-app/buffalo_l/resolve/main/recognition/model.onnx" \
  "$MODEL_DIR/face_embedding.onnx"

echo "Models installed in $MODEL_DIR"