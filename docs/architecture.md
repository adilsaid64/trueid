# Architecture

## Overview

Crates split **core** (ports + `TrueIdApp`) from **adapters** (camera, ONNX, files).

Per captured frame, before matching:

1. Capture (one session: optional warm-up, then N frames)
2. Detect face → align → liveness → embed
3. Compare to stored template
4. Return accept/reject

---

## Structure

```mermaid
flowchart TD
    IPC[IPC]
    App[TrueIdApp]

    subgraph Ports
        Video[VideoSource]
        Det[FaceDetector]
        Align[FaceAligner]
        Live[LivenessChecker]
        FaceEmb[FaceEmbedder]
        Matcher[Matcher]
        Store[TemplateStore]
    end

    subgraph Adapters
        V4L[V4lVideoSource]
        MockVideo[MockVideoSource]
        MockDet[FullFrameFaceDetector]
        YuNet[OnnxYuNetDetector]
        MockAlign[PassthroughFaceAligner]
        MockLive[AlwaysLiveLiveness]
        Cosine[CosineMatcher]
        FileStore[FileTemplateStore]
        MockEmb[MockFaceEmbedder]
        OnnxFace[OnnxFaceEmbedder]
    end

    IPC --> App

    App --> Video
    App --> Det
    App --> Align
    App --> Live
    App --> FaceEmb
    App --> Matcher
    App --> Store

    Video --> V4L
    Video --> MockVideo
    Det --> MockDet
    Det --> YuNet
    Align --> MockAlign
    Live --> MockLive
    Matcher --> Cosine
    Store --> FileStore
    FaceEmb --> MockEmb
    FaceEmb --> OnnxFace

```

---

## Components

* **TrueIdApp** — auth pipeline
* **VideoSource** — `capture(CaptureSpec)` → frames
* **FaceDetector** — primary face → `FaceDetection`
* **FaceAligner** — crop/warp to a standard face image
* **LivenessChecker** — spoof check on aligned crop
* **FaceEmbedder** — face image → embedding
* **Matcher** — compare embeddings
* **TemplateStore** — persist templates

Concrete behavior lives in adapters (V4L, mocks, ONNX, disk).

---

## Capture model

* One `capture` call = one camera session
* Warm-up frames optional (dropped)
* Then N frames returned; no continuous streaming

---

## Flow

```mermaid
sequenceDiagram
    Client->>IPC: verify
    IPC->>App: verify()

    App->>Video: capture()
    Video-->>App: frames

    App->>App: detect → align → liveness → embed
    App->>Store: load template
    App->>Matcher: compare

    App-->>IPC: result
```
