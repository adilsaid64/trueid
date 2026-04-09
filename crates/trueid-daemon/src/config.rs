use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const SYSTEM_CONFIG: &str = "/etc/trueid/config.yaml";
const BUNDLED_CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/config/config.yaml");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub logging: LoggingConfig,
    pub camera: CameraConfig,
    pub models: ModelsConfig,
    pub paths: PathsConfig,
    pub pipeline: PipelineConfig,
    pub verification: VerificationConfig,
    pub development: DevelopmentConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    /// `batch` — burst capture; `streaming` — reserved (returns not implemented until built).
    pub enroll: PipelineModeYaml,
    pub verify: PipelineModeYaml,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PipelineModeYaml {
    #[default]
    Batch,
    Streaming,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            enroll: PipelineModeYaml::Batch,
            verify: PipelineModeYaml::Batch,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Default `tracing` filter (e.g. `info`).
    pub level: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CameraConfig {
    pub rgb_index: u32,
    pub enable_rgb: bool,
    pub ir_index: u32,
    pub enable_ir: bool,
    pub width: u32,
    pub height: u32,
    /// Use in-memory frames instead of V4L (no `/dev/video*`).
    pub mock: bool,
    pub v4l: V4lConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct V4lConfig {
    pub rotate_180: bool,
    /// Ignored when `rotate_180` is true.
    pub flip_vertical: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    pub face_embedding: String,
    pub face_detector: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    pub templates: String,
    pub debug_aligned_faces: Option<String>,
    pub debug_v4l_frames: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VerificationConfig {
    pub match_threshold: f32,
    pub fusion: ModalityFusionYaml,
    /// Enroll vs verify burst lengths (warmup discard + frame count). Matches [`trueid_core::MultiFramePolicy`] defaults when omitted.
    pub capture: CapturePolicyYaml,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CapturePolicyYaml {
    pub enroll: CaptureSpecYaml,
    pub verify: CaptureSpecYaml,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CaptureSpecYaml {
    pub warmup_discard: u32,
    pub frame_count: u32,
}

impl Default for CaptureSpecYaml {
    fn default() -> Self {
        Self {
            warmup_discard: 2,
            frame_count: 3,
        }
    }
}

impl Default for CapturePolicyYaml {
    fn default() -> Self {
        Self {
            enroll: CaptureSpecYaml {
                warmup_discard: 2,
                frame_count: 5,
            },
            verify: CaptureSpecYaml::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ModalityFusionYaml {
    pub weight_rgb: f32,
    pub weight_ir: f32,
    pub fusion_threshold: f32,
}

impl Default for ModalityFusionYaml {
    fn default() -> Self {
        Self {
            weight_rgb: 0.45,
            weight_ir: 0.55,
            fusion_threshold: 0.5,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DevelopmentConfig {
    pub mock_embedder: bool,
    pub mock_detector: bool,
    pub passthrough_aligner: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
        }
    }
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            rgb_index: 0,
            ir_index: 2,
            enable_rgb: true,
            enable_ir: false,
            width: 640,
            height: 480,
            mock: false,
            v4l: V4lConfig::default(),
        }
    }
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            face_embedding: "/var/lib/trueid/models/face_embedding.onnx".to_string(),
            face_detector: "/var/lib/trueid/models/face_detection_yunet_2023mar.onnx".to_string(),
        }
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            templates: "/var/lib/trueid/templates".to_string(),
            debug_aligned_faces: None,
            debug_v4l_frames: None,
        }
    }
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            match_threshold: 0.70,
            fusion: ModalityFusionYaml::default(),
            capture: CapturePolicyYaml::default(),
        }
    }
}

fn resolve_config_path() -> PathBuf {
    let system = Path::new(SYSTEM_CONFIG);
    if system.exists() {
        return system.to_path_buf();
    }
    let bundled = Path::new(BUNDLED_CONFIG);
    if bundled.exists() {
        return bundled.to_path_buf();
    }
    PathBuf::from("config/config.yaml")
}

pub fn load_config() -> Config {
    let path = resolve_config_path();
    if !path.exists() {
        eprintln!(
            "trueid-daemon: no config at {} (also tried bundled path); using defaults",
            path.display()
        );
        return Config::default();
    }

    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "trueid-daemon: failed to read {}: {e}; using defaults",
                path.display()
            );
            return Config::default();
        }
    };

    serde_yaml::from_str(&contents).unwrap_or_else(|e| {
        eprintln!(
            "trueid-daemon: invalid YAML in {}: {e}; using defaults",
            path.display()
        );
        Config::default()
    })
}
