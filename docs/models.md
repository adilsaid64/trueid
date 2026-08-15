# Models

Face detection and embedding ONNX files live under `/var/lib/trueid/models/` by default (`models.face_detector` and `models.face_embedding` in config).

Install them with:

```bash
sudo trueid-ctl get-models
```

That downloads:

- YuNet face detector from [opencv_zoo](https://github.com/opencv/opencv_zoo)
- ArcFace-style embedding from [immich buffalo_l](https://huggingface.co/immich-app/buffalo_l)

For local runs without hardware or ONNX files, set `camera.mock`, `development.mock_embedder`, and `development.mock_detector` as described in [developing.md](developing.md).
