## Download required models

This project requires a face detector and an embedding model.

> NOTE: This will be automated in a future update.

### 1. Create models directory

```bash
mkdir -p ~/.local/share/trueid/models
```

### 2. Download face detector (YuNet)

```bash
wget -O ~/.local/share/trueid/models/face_detection_yunet_2023mar.onnx \
https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx
```

### 3. Download embedding model (ArcFace)

```bash
wget -O ~/.local/share/trueid/models/face_embedding.onnx \
https://github.com/onnx/models/raw/main/validated/vision/body_analysis/arcface/model/arcfaceresnet100-8.onnx
```

### 4. Verify

```bash
ls ~/.local/share/trueid/models
```

Expected files:

```
face_detection_yunet_2023mar.onnx
arcface.onnx
```
