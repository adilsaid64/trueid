use std::fs;
use std::path::Path;
use std::sync::Arc;

use trueid_core::Embedding;
use trueid_core::TrueIdApp;
use trueid_ipc::SOCKET_PATH;

mod adapters;
mod ipc;

/// V4L device index when `TRUEID_CAMERA_INDEX` is unset (`/dev/video{N}`).
/// `0` is typical for the RGB webcam; use `TRUEID_CAMERA_INDEX=2` if your IR node is the capture device.
const DEFAULT_CAMERA_INDEX: u32 = 0;

fn main() -> std::io::Result<()> {
    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }

    let health = Arc::new(adapters::DefaultHealth);
    let video: Arc<dyn trueid_core::ports::VideoSource> = if std::env::var("TRUEID_USE_MOCK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        Arc::new(adapters::MockVideoSource::default_gray())
    } else {
        let index: u32 = std::env::var("TRUEID_CAMERA_INDEX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_CAMERA_INDEX);
        Arc::new(
            adapters::V4lVideoSource::open(index).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!(
                        "camera open failed (index {index}): {e}. \
                         Set TRUEID_USE_MOCK=1 to run without a device."
                    ),
                )
            })?,
        )
    };
    let embedder = Arc::new(adapters::MockEmbedder::new(Embedding(vec![
        1.0, 0.0, 0.0,
    ])));
    let template_store = Arc::new(adapters::FileTemplateStore::open_default().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    })?);
    let matcher = Arc::new(adapters::CosineMatcher::new(0.99));

    let app = Arc::new(TrueIdApp::new(
        health,
        video,
        embedder,
        template_store,
        matcher,
    ));

    ipc::run_unix_socket(SOCKET_PATH, app)
}
