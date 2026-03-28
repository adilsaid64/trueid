# Architecture

## Overview

TrueID is a face authentication system with a small, modular core.

Flow:

1. Capture frames from the camera (one session: warm-up + N frames)
2. Convert frames to embeddings
3. Compare against stored templates
4. Return a decision

Core logic is independent of camera, storage, and model implementations.

---

## Structure

```mermaid
flowchart TD
    IPC[IPC]
    App[TrueIdApp]

    subgraph Ports
        Video[VideoSource]
        Embedder[Embedder]
        Matcher[Matcher]
        Store[TemplateStore]
    end

    subgraph Adapters
        V4L[V4lVideoSource]
        MockVideo[MockVideoSource]
        Cosine[CosineMatcher]
        FileStore[FileTemplateStore]
        MockEmbedder[MockEmbedder]
    end

    IPC --> App

    App --> Video
    App --> Embedder
    App --> Matcher
    App --> Store

    Video --> V4L
    Video --> MockVideo
    Matcher --> Cosine
    Store --> FileStore
    Embedder --> MockEmbedder
````

---

## Components

* **TrueIdApp** — runs the auth pipeline
* **VideoSource** — `capture(CaptureSpec)` → frames
* **Embedder** — frames → embeddings
* **Matcher** — compare embeddings
* **TemplateStore** — load/save templates

Adapters implement these (V4L camera, file storage, mock components).

---

## Capture Model

* One capture = one session
* Optional warm-up frames (discarded)
* Then N frames returned
* No continuous streaming

All frames are normalisd to `RGB8`.

---

## Flow

```mermaid
sequenceDiagram
    Client->>IPC: verify
    IPC->>App: verify()

    App->>Video: capture()
    Video-->>App: frames

    App->>Embedder: embed
    App->>Store: load template
    App->>Matcher: compare

    App-->>IPC: result
```
