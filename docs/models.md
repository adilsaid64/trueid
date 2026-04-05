## Models

Face detector + embedding ONNX files required.

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
https://huggingface.co/immich-app/buffalo_l/resolve/main/recognition/model.onnx
```

### 4. Verify

```bash
ls ~/.local/share/trueid/models
```

Expected:

```
face_detection_yunet_2023mar.onnx
face_embedding.onnx
```
