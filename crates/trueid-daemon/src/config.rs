use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const SYSTEM_CONFIG: &str = "/etc/trueid/config.yaml";
const BUNDLED_CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/config/config.yaml");

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub logging: LoggingConfig,
    pub camera: CameraConfig,
    pub models: ModelsConfig,
    pub paths: PathsConfig,
    pub verification: VerificationConfig,
    pub development: DevelopmentConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// `tracing` filter level for the daemon (e.g. `info`, `debug`, `warn`).
    pub level: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CameraConfig {
    pub rgb_index: u32,
    pub ir_index: u32,
    pub enable_ir: bool,
    pub width: u32,
    pub height: u32,
    /// Use in-memory frames instead of V4L (no `/dev/video*`).
    pub mock: bool,
    pub v4l: V4lConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct V4lConfig {
    /// Rotate each decoded RGB frame 180° (e.g. upside-down sensor with no EXIF).
    pub rotate_180: bool,
    /// Vertical flip only; ignored if `rotate_180` is true.
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
    /// Directory for per-user template JSON files.
    pub templates: String,
    /// If set, save each aligned face as PNG under this directory (debug).
    pub debug_aligned_faces: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VerificationConfig {
    pub match_threshold: f32,
}

#[derive(Debug, Clone, Deserialize)]
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
            enable_ir: false,
            width: 640,
            height: 480,
            mock: false,
            v4l: V4lConfig::default(),
        }
    }
}

impl Default for V4lConfig {
    fn default() -> Self {
        Self {
            rotate_180: false,
            flip_vertical: false,
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
        }
    }
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            match_threshold: 0.70,
        }
    }
}

impl Default for DevelopmentConfig {
    fn default() -> Self {
        Self {
            mock_embedder: false,
            mock_detector: false,
            passthrough_aligner: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            logging: LoggingConfig::default(),
            camera: CameraConfig::default(),
            models: ModelsConfig::default(),
            paths: PathsConfig::default(),
            verification: VerificationConfig::default(),
            development: DevelopmentConfig::default(),
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
