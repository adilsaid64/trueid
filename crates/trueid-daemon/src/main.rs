use std::fs;
use std::path::Path;
use std::sync::Arc;

use trueid_core::ports::{FaceAligner, FaceDetector, FaceEmbedder};
use trueid_core::{Embedding, MultiFramePolicy, TrueIdApp, TrueIdAppDeps};
use trueid_ipc::SOCKET_PATH;

mod adapters;
mod ipc;

// `/dev/video{N}` when `TRUEID_CAMERA_INDEX` unset.
const DEFAULT_CAMERA_INDEX: u32 = 0;

fn parse_u32_env_positive(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

fn parse_match_threshold() -> f32 {
    std::env::var("TRUEID_MATCH_THRESHOLD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.70)
}

/// `tracing-subscriber` fmt layer + `EnvFilter` from `RUST_LOG` (same convention as `env_logger`).
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

fn main() -> std::io::Result<()> {
    init_tracing();

    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }

    let health = Arc::new(adapters::DefaultHealth);
    let video: Arc<dyn trueid_core::ports::VideoSource> =
        if std::env::var("TRUEID_USE_MOCK_VIDEO_SOURCE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            Arc::new(adapters::MockVideoSource::default_gray())
        } else {
            let index: u32 = std::env::var("TRUEID_CAMERA_INDEX")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_CAMERA_INDEX);
            let cap_w = parse_u32_env_positive("TRUEID_CAPTURE_WIDTH", 640);
            let cap_h = parse_u32_env_positive("TRUEID_CAPTURE_HEIGHT", 480);
            Arc::new(
                adapters::V4lVideoSource::open_with_dimensions(index, cap_w, cap_h).map_err(
                    |e| {
                        std::io::Error::other(format!(
                            "camera open failed (index {index}): {e}. \
                     Set TRUEID_USE_MOCK_VIDEO_SOURCE=1 to run without a device."
                        ))
                    },
                )?,
            )
        };
    let face_embedder: Arc<dyn FaceEmbedder> = if std::env::var("TRUEID_USE_MOCK_EMBEDDER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        Arc::new(adapters::MockFaceEmbedder::new(Embedding(vec![
            1.0, 0.0, 0.0,
        ])))
    } else {
        adapters::build_face_embedder().map_err(std::io::Error::other)?
    };
    let template_store = Arc::new(
        adapters::FileTemplateStore::open_default()
            .map_err(|e| std::io::Error::other(e.to_string()))?,
    );
    let matcher = Arc::new(adapters::CosineMatcher::new(parse_match_threshold()));

    let detector: Arc<dyn FaceDetector> = if std::env::var("TRUEID_USE_MOCK_DETECTOR")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        Arc::new(adapters::FullFrameFaceDetector)
    } else {
        adapters::build_face_detector().map_err(std::io::Error::other)?
    };
    let aligner: Arc<dyn FaceAligner> = if std::env::var("TRUEID_USE_PASSTHROUGH_ALIGNER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        Arc::new(adapters::PassthroughFaceAligner)
    } else {
        Arc::new(adapters::CropFaceAligner::default())
    };
    let liveness = Arc::new(adapters::AlwaysLiveLiveness);

    let app = Arc::new(TrueIdApp::new(TrueIdAppDeps {
        health,
        video,
        detector,
        aligner,
        liveness,
        face_embedder,
        template_store,
        matcher,
        capture: MultiFramePolicy::default(),
    }));

    ipc::run_unix_socket(SOCKET_PATH, app)
}
