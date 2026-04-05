# Architecture

## Overview

Crates split **core** (ports + `TrueIdApp`) from **adapters** (camera, ONNX, files).

Per captured RGB frame, before matching:

1. **CameraCapture** — one logical burst (optional warm-up, then N frames); may run RGB only or RGB + IR in parallel (implementation detail in the daemon).
2. Detect face → align → liveness → embed
3. Compare probe to stored template(s); verify uses a **quorum** (≥ half of templates must match a probe on a frame)
4. Return accept/reject

---

## Structure

```mermaid
flowchart TD
    IPC[IPC]
    App[TrueIdApp]

    subgraph Ports
        Cam[CameraCapture]
        Health[Health]
        Det[FaceDetector]
        Align[FaceAligner]
        Live[LivenessChecker]
        FaceEmb[FaceEmbedder]
        Matcher[EmbeddingMatcher]
        Store[TemplateStore]
        Video[VideoSource]
    end

    subgraph Adapters
        RgbOnly[RgbOnlyCameraCapture]
        ParIR[ParallelRgbIrCameraCapture]
        V4L[V4lVideoSource]
        MockVideo[MockVideoSource]
        DefHealth[DefaultHealth]
        MockDet[FullFrameFaceDetector]
        YuNet[OnnxYuNetDetector]
        CropAlign[CropFaceAligner]
        PassAlign[PassthroughFaceAligner]
        MockLive[AlwaysLiveLiveness]
        Cosine[CosineMatcher]
        FileStore[FileTemplateStore]
        MockEmb[MockFaceEmbedder]
        OnnxFace[OnnxFaceEmbedder]
    end

    IPC --> App

    App --> Cam
    App --> Health
    App --> Det
    App --> Align
    App --> Live
    App --> FaceEmb
    App --> Matcher
    App --> Store

    Cam --> RgbOnly
    Cam --> ParIR
    RgbOnly --> Video
    ParIR --> Video
    Video --> V4L
    Video --> MockVideo
    Health --> DefHealth
    Det --> MockDet
    Det --> YuNet
    Align --> CropAlign
    Align --> PassAlign
    Live --> MockLive
    Matcher --> Cosine
    Store --> FileStore
    FaceEmb --> MockEmb
    FaceEmb --> OnnxFace
```

`VideoSource` stays the per-device primitive (`capture` → `Vec<Frame>`). `CameraCapture` composes one or two `VideoSource` instances and is what `TrueIdApp` calls.

---

## Components

* **TrueIdApp** — auth pipeline (`ping`, `enroll`, `verify`, `add_template`)
* **Health** — readiness gate before capture
* **CameraCapture** — `capture(CaptureSpec)` → **`CapturedBurst`** (`rgb` frames, optional `ir`)
* **VideoSource** — single stream; used only inside camera adapters (V4L, mock)
* **FaceDetector** — primary face → `FaceDetection`
* **FaceAligner** — crop/warp to a standard face image
* **LivenessChecker** — spoof check on aligned crop
* **FaceEmbedder** — face image → embedding
* **EmbeddingMatcher** — compare embeddings (e.g. cosine vs threshold)
* **TemplateStore** — persist templates (multiple per user supported)

Concrete behavior lives in adapters (V4L, mocks, ONNX, disk). **Config** (`config.yaml`) is read only in the daemon, not in core.

---

## Capture model

* One **`CameraCapture::capture`** call = one logical burst from the app’s perspective
* Under the hood: RGB-only adapter runs one `VideoSource::capture`; parallel RGB+IR runs two captures on separate threads (best-effort overlap, not hardware-synced)
* Warm-up frames optional (dropped), then N frames; no continuous streaming API

---

## Flow

```mermaid
sequenceDiagram
    Client->>IPC: verify
    IPC->>App: verify()

    App->>Cam: capture()
    Cam-->>App: CapturedBurst (rgb, optional ir)

    App->>App: detect → align → liveness → embed (per rgb frame)
    App->>Store: load templates
    App->>Matcher: compare (quorum across templates)

    App-->>IPC: result
```

IR frames are captured when configured but are not yet consumed by the core pipeline (reserved for future use).
