#!/bin/bash
# Post-install script to download models

MODEL_DIR="/usr/share/trueid/models"
mkdir -p "$MODEL_DIR"

echo "Downloading face detection model..."
wget -O "$MODEL_DIR/face_detection_yunet_2023mar.onnx" \
  https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx

echo "Downloading face embedding model..."
wget -O "$MODEL_DIR/face_embedding.onnx" \
  https://huggingface.co/immich-app/buffalo_l/resolve/main/recognition/model.onnx

echo "Models downloaded to $MODEL_DIR"