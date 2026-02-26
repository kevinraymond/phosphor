use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Output resolution for NDI capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputResolution {
    Match,
    Res720p,
    Res1080p,
    Res4K,
}

impl OutputResolution {
    pub const ALL: &[OutputResolution] = &[
        OutputResolution::Match,
        OutputResolution::Res720p,
        OutputResolution::Res1080p,
        OutputResolution::Res4K,
    ];

    pub fn dimensions(self, window_w: u32, window_h: u32) -> (u32, u32) {
        match self {
            OutputResolution::Match => (window_w, window_h),
            OutputResolution::Res720p => (1280, 720),
            OutputResolution::Res1080p => (1920, 1080),
            OutputResolution::Res4K => (3840, 2160),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            OutputResolution::Match => "Match Window",
            OutputResolution::Res720p => "720p",
            OutputResolution::Res1080p => "1080p",
            OutputResolution::Res4K => "4K",
        }
    }
}

impl Default for OutputResolution {
    fn default() -> Self {
        OutputResolution::Match
    }
}

/// Persisted NDI output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NdiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_source_name")]
    pub source_name: String,
    #[serde(default)]
    pub resolution: OutputResolution,
}

fn default_source_name() -> String {
    "Phosphor".to_string()
}

impl Default for NdiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            source_name: default_source_name(),
            resolution: OutputResolution::default(),
        }
    }
}

impl NdiConfig {
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        config_dir.join("phosphor").join("ndi.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(config) => {
                    log::info!("Loaded NDI config from {}", path.display());
                    config
                }
                Err(e) => {
                    log::warn!("Failed to parse NDI config: {e}");
                    Self::default()
                }
            },
            Err(_) => {
                log::info!("No NDI config found, using defaults");
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::error!("Failed to create config dir: {e}");
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    log::error!("Failed to write NDI config: {e}");
                } else {
                    log::debug!("Saved NDI config to {}", path.display());
                }
            }
            Err(e) => log::error!("Failed to serialize NDI config: {e}"),
        }
    }
}
