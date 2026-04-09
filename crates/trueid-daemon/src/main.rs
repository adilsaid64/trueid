use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use trueid_core::ports::{FaceAligner, FaceDetector, FaceEmbedder};
use trueid_core::{
    CameraCapture, Embedding, ModalityFusionConfig, MultiFramePolicy, StreamModality, TrueIdApp,
    TrueIdAppDeps,
};
use trueid_ipc::SOCKET_PATH;

mod adapters;
mod config;
mod ipc;

fn init_tracing(level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(level))
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

fn main() -> std::io::Result<()> {
    let cfg = config::load_config();
    init_tracing(&cfg.logging.level);

    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }

    let health = Arc::new(adapters::DefaultHealth);

    let cap_w = cfg.camera.width;
    let cap_h = cfg.camera.height;
    let v4l_rotate = cfg.camera.v4l.rotate_180;
    let v4l_flip = cfg.camera.v4l.flip_vertical;
    let debug_v4l = cfg
        .paths
        .debug_v4l_frames
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    if !cfg.camera.enable_rgb && !cfg.camera.enable_ir {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid config: at least one of `camera.enable_rgb` or `camera.enable_ir` must be true",
        ));
    }

    let video_rgb: Option<Arc<dyn trueid_core::ports::VideoSource>> = if cfg.camera.enable_rgb {
        Some(if cfg.camera.mock {
            Arc::new(adapters::MockVideoSource::default_gray())
        } else {
            let index = cfg.camera.rgb_index;
            Arc::new(
                adapters::V4lVideoSource::open_with_dimensions(
                    index,
                    cap_w,
                    cap_h,
                    StreamModality::Rgb,
                    v4l_rotate,
                    v4l_flip,
                    debug_v4l.clone(),
                )
                .map_err(|e| {
                    std::io::Error::other(format!(
                        "camera open failed (index {index}): {e}. \
                         Set `camera.mock: true` in config to run without a device."
                    ))
                })?,
            )
        })
    } else {
        None
    };

    let video_ir: Option<Arc<dyn trueid_core::ports::VideoSource>> = if cfg.camera.enable_ir {
        Some(if cfg.camera.mock {
            Arc::new(adapters::MockVideoSource::default_gray())
        } else {
            let index = cfg.camera.ir_index;
            Arc::new(
                adapters::V4lVideoSource::open_with_dimensions(
                    index,
                    cap_w,
                    cap_h,
                    StreamModality::Ir,
                    v4l_rotate,
                    v4l_flip,
                    debug_v4l.clone(),
                )
                .map_err(|e| {
                    std::io::Error::other(format!("IR camera open failed (index {index}): {e}"))
                })?,
            )
        })
    } else {
        None
    };

    let camera: Arc<dyn CameraCapture> = match (video_rgb, video_ir) {
        (Some(video_rgb), Some(video_ir)) => Arc::new(adapters::ParallelRgbIrCameraCapture::new(
            video_rgb, video_ir,
        )),
        (Some(video_rgb), None) => Arc::new(adapters::RgbOnlyCameraCapture::new(video_rgb)),
        (None, Some(video_ir)) => Arc::new(adapters::IROnlyCameraCapture::new(video_ir)),
        (None, None) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid config: at least one of `camera.enable_rgb` or `camera.enable_ir` must be true",
            ));
        }
    };

    let face_embedder: Arc<dyn FaceEmbedder> = if cfg.development.mock_embedder {
        Arc::new(adapters::MockFaceEmbedder::new(Embedding(vec![
            1.0, 0.0, 0.0,
        ])))
    } else {
        let p = PathBuf::from(&cfg.models.face_embedding);
        adapters::build_face_embedder(&p).map_err(std::io::Error::other)?
    };

    let template_store = Arc::new(
        adapters::FileTemplateStore::open(&cfg.paths.templates)
            .map_err(|e| std::io::Error::other(e.to_string()))?,
    );

    let match_threshold = cfg.verification.match_threshold;
    let matcher = Arc::new(adapters::CosineMatcher::new(match_threshold));

    let detector: Arc<dyn FaceDetector> = if cfg.development.mock_detector {
        Arc::new(adapters::FullFrameFaceDetector)
    } else {
        let p = PathBuf::from(&cfg.models.face_detector);
        adapters::build_face_detector(&p).map_err(std::io::Error::other)?
    };

    let debug_aligned = cfg
        .paths
        .debug_aligned_faces
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    let aligner: Arc<dyn FaceAligner> = if cfg.development.passthrough_aligner {
        Arc::new(adapters::PassthroughFaceAligner)
    } else {
        Arc::new(adapters::CropFaceAligner::with_debug_dir(debug_aligned))
    };
    let liveness = Arc::new(adapters::AlwaysLiveLiveness);

    let modality_fusion = ModalityFusionConfig {
        weight_rgb: cfg.verification.fusion.weight_rgb,
        weight_ir: cfg.verification.fusion.weight_ir,
        fusion_threshold: cfg.verification.fusion.fusion_threshold,
    };

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
        modality_fusion,
    }));

    ipc::run_unix_socket(SOCKET_PATH, app)
}
