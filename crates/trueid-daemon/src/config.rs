//! Daemon YAML config. Core does not read this file; [`crate::composition`] maps it
//! onto `trueid_core` types and outbound adapter constructors.

use serde::Deserialize;
use std::fs;
use std::io;
use std::path::PathBuf;

use trueid_core::{StreamLimits, StreamingPolicy};

const SYSTEM_CONFIG: &str = "/etc/trueid/config.yaml";
const BUNDLED_CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/config/config.yaml");

#[derive(Debug, Clone, Default, Deserialize)]
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
    pub mock: bool,
    pub v4l: V4lConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct V4lConfig {
    pub rotate_180: bool,
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
    /// Enroll vs verify streaming limits (warmup discard + max frames). Matches [`trueid_core::StreamingPolicy`] defaults when omitted.
    pub capture: CapturePolicyYaml,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CapturePolicyYaml {
    pub enroll: StreamLimitsYaml,
    pub verify: StreamLimitsYaml,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StreamLimitsYaml {
    pub warmup_discard: u32,
    /// Maximum frames to pull from the camera after warmup (legacy key: `frame_count`).
    #[serde(alias = "frame_count")]
    pub max_frames: u32,
}

impl StreamLimitsYaml {
    fn from_limits(limits: StreamLimits) -> Self {
        Self {
            warmup_discard: limits.warmup_discard,
            max_frames: limits.max_frames,
        }
    }
}

impl Default for StreamLimitsYaml {
    fn default() -> Self {
        Self::from_limits(StreamingPolicy::default().verify)
    }
}

impl Default for CapturePolicyYaml {
    fn default() -> Self {
        let policy = StreamingPolicy::default();
        Self {
            enroll: StreamLimitsYaml::from_limits(policy.enroll),
            verify: StreamLimitsYaml::from_limits(policy.verify),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DevelopmentConfig {
    pub mock_embedder: bool,
    pub mock_detector: bool,
    pub passthrough_aligner: bool,
    pub passthrough_pose_estimator: bool,
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
            capture: CapturePolicyYaml::default(),
        }
    }
}

fn resolve_config_path() -> io::Result<PathBuf> {
    let candidates = [
        PathBuf::from(SYSTEM_CONFIG),
        PathBuf::from(BUNDLED_CONFIG),
        PathBuf::from("config/config.yaml"),
    ];
    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "no config found (tried {SYSTEM_CONFIG}, bundled crate config, config/config.yaml)"
        ),
    ))
}

pub fn load_config() -> io::Result<Config> {
    let path = resolve_config_path()?;
    let contents = fs::read_to_string(&path)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to read {}: {e}", path.display())))?;
    serde_yaml::from_str(&contents).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid YAML in {}: {e}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_defaults_match_core_policy() {
        let yaml = CapturePolicyYaml::default();
        let policy = StreamingPolicy::default();
        assert_eq!(yaml.enroll.warmup_discard, policy.enroll.warmup_discard);
        assert_eq!(yaml.enroll.max_frames, policy.enroll.max_frames);
        assert_eq!(yaml.verify.warmup_discard, policy.verify.warmup_discard);
        assert_eq!(yaml.verify.max_frames, policy.verify.max_frames);
    }

    #[test]
    fn rejects_invalid_yaml() {
        assert!(serde_yaml::from_str::<Config>("logging: [").is_err());
    }
}
