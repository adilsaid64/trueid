//! Composition root: config → outbound adapters → [`TrueIdApp`].
//!
//! Core never reads YAML or constructs V4L/ONNX types. This module is the only
//! place that chooses which outbound impl to inject.

use std::path::PathBuf;
use std::sync::Arc;

use trueid_core::ports::{FaceAligner, FaceDetector, FaceEmbedder, FacePoseEstimator};
use trueid_core::{
    Embedding, StreamLimits, StreamModality, StreamingPolicy, TrueIdApp, TrueIdAppDeps, VideoSource,
};

use crate::adapters::outbound;
use crate::config::Config;

pub fn build(cfg: &Config) -> std::io::Result<Arc<TrueIdApp>> {
    let use_rgb = cfg.camera.enable_rgb;
    let use_ir = cfg.camera.enable_ir;
    if use_rgb == use_ir {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid config: enable exactly one of `camera.enable_rgb` or `camera.enable_ir`",
        ));
    }

    let modality = if use_rgb {
        StreamModality::Rgb
    } else {
        StreamModality::Ir
    };
    let index = if use_rgb {
        cfg.camera.rgb_index
    } else {
        cfg.camera.ir_index
    };

    let debug_v4l = cfg
        .paths
        .debug_v4l_frames
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    let video: Arc<dyn VideoSource> = if cfg.camera.mock {
        Arc::new(outbound::MockVideoSource::with_modality(modality))
    } else {
        Arc::new(
            outbound::V4lVideoSource::open_with_dimensions(
                index,
                cfg.camera.width,
                cfg.camera.height,
                modality,
                cfg.camera.v4l.rotate_180,
                cfg.camera.v4l.flip_vertical,
                debug_v4l,
            )
            .map_err(|e| {
                std::io::Error::other(format!(
                    "camera open failed (index {index}): {e}. \
                     Set `camera.mock: true` in config to run without a device."
                ))
            })?,
        )
    };

    let face_embedder: Arc<dyn FaceEmbedder> = if cfg.development.mock_embedder {
        Arc::new(outbound::MockFaceEmbedder::new(Embedding::new(vec![
            1.0, 0.0, 0.0,
        ])))
    } else {
        let p = PathBuf::from(&cfg.models.face_embedding);
        outbound::build_face_embedder(&p).map_err(std::io::Error::other)?
    };

    let template_store = Arc::new(
        outbound::FileTemplateStore::open(&cfg.paths.templates)
            .map_err(|e| std::io::Error::other(e.to_string()))?,
    );

    let matcher = Arc::new(outbound::CosineMatcher::new(
        cfg.verification.match_threshold,
    ));

    let detector: Arc<dyn FaceDetector> = if cfg.development.mock_detector {
        Arc::new(outbound::FullFrameFaceDetector)
    } else {
        let p = PathBuf::from(&cfg.models.face_detector);
        outbound::build_face_detector(&p).map_err(std::io::Error::other)?
    };

    let debug_aligned = cfg
        .paths
        .debug_aligned_faces
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    let aligner: Arc<dyn FaceAligner> = if cfg.development.passthrough_aligner {
        Arc::new(outbound::PassthroughFaceAligner)
    } else {
        Arc::new(outbound::CropFaceAligner::with_debug_dir(debug_aligned))
    };

    let pose_estimator: Arc<dyn FacePoseEstimator> =
        if cfg.development.passthrough_pose_estimator || cfg.development.mock_detector {
            Arc::new(outbound::PassthroughFacePoseEstimator)
        } else {
            Arc::new(outbound::GeometricLandmarkPoseEstimator::default())
        };

    let streaming = StreamingPolicy {
        enroll: StreamLimits::new(
            cfg.verification.capture.enroll.warmup_discard,
            cfg.verification.capture.enroll.max_frames,
        ),
        verify: StreamLimits::new(
            cfg.verification.capture.verify.warmup_discard,
            cfg.verification.capture.verify.max_frames,
        ),
    };

    Ok(Arc::new(TrueIdApp::new(TrueIdAppDeps {
        health: Arc::new(outbound::DefaultHealth),
        video,
        detector,
        aligner,
        pose_estimator,
        liveness: Arc::new(outbound::AlwaysLiveLiveness),
        face_embedder,
        template_store,
        matcher,
        streaming,
    })))
}
