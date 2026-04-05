use std::fs;
use std::path::Path;
use std::sync::Arc;

use trueid_core::ports::{FaceAligner, FaceDetector, FaceEmbedder};
use trueid_core::{
    CameraCapture, Embedding, MultiFramePolicy, StreamModality, TrueIdApp, TrueIdAppDeps,
};
use trueid_ipc::SOCKET_PATH;

mod adapters;
mod config;
mod ipc;

// `/dev/video{N}` when `TRUEID_CAMERA_INDEX` unset.
const DEFAULT_RGB_CAMERA_INDEX: u32 = 0;
const DEFAULT_IR_CAMERA_INDEX: u32 = 2;

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

    let config = config::load_config();

    let health = Arc::new(adapters::DefaultHealth);
    let use_mock = std::env::var("TRUEID_USE_MOCK_VIDEO_SOURCE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let cap_w = parse_u32_env_positive("TRUEID_CAPTURE_WIDTH", 640);
    let cap_h = parse_u32_env_positive("TRUEID_CAPTURE_HEIGHT", 480);

    let video_rgb: Arc<dyn trueid_core::ports::VideoSource> = if use_mock {
        Arc::new(adapters::MockVideoSource::default_gray())
    } else {
        let index = config.rgb_camera_index.unwrap_or(DEFAULT_RGB_CAMERA_INDEX);
        Arc::new(
            adapters::V4lVideoSource::open_with_dimensions(
                index,
                cap_w,
                cap_h,
                StreamModality::Rgb,
            )
            .map_err(|e| {
                std::io::Error::other(format!(
                    "camera open failed (index {index}): {e}. \
                         Set TRUEID_USE_MOCK_VIDEO_SOURCE=1 to run without a device."
                ))
            })?,
        )
    };

    let camera: Arc<dyn CameraCapture> = if config.enable_ir.unwrap_or(false) {
        let video_ir: Arc<dyn trueid_core::ports::VideoSource> = if use_mock {
            Arc::new(adapters::MockVideoSource::default_gray())
        } else {
            let index = config.ir_camera_index.unwrap_or(DEFAULT_IR_CAMERA_INDEX);
            Arc::new(
                adapters::V4lVideoSource::open_with_dimensions(
                    index,
                    cap_w,
                    cap_h,
                    StreamModality::Ir,
                )
                .map_err(|e| {
                    std::io::Error::other(format!("IR camera open failed (index {index}): {e}"))
                })?,
            )
        };
        Arc::new(adapters::ParallelRgbIrCameraCapture::new(
            video_rgb, video_ir,
        ))
    } else {
        Arc::new(adapters::RgbOnlyCameraCapture::new(video_rgb))
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
    let match_threshold = config.match_threshold.unwrap_or_else(parse_match_threshold);
    let matcher = Arc::new(adapters::CosineMatcher::new(match_threshold));

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
        camera,
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
