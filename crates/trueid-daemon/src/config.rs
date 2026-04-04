use serde::Deserialize;
use std::fs;
use std::path::Path;

const CONFIG_PATH: &str = "/etc/trueid/config.yaml";

#[derive(Debug, Deserialize)]
pub struct Config {
    pub rgb_camera_index: Option<u32>,
    pub ir_camera_index: Option<u32>,
    pub enable_ir: Option<bool>,
    pub match_threshold: Option<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rgb_camera_index: Some(0),
            ir_camera_index: Some(2),
            enable_ir: Some(false),
            match_threshold: Some(0.70),
        }
    }
}

pub fn load_config() -> Config {
    let path = Path::new(CONFIG_PATH);

    if !path.exists() {
        tracing::warn!("config not found at {}, using defaults", CONFIG_PATH);
        return Config::default();
    }

    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("failed to read config.yaml: {e}, using defaults");
            return Config::default();
        }
    };

    serde_yaml::from_str(&contents).unwrap_or_else(|e| {
        tracing::error!("invalid config.yaml: {e}, using defaults");
        Config::default()
    })
}