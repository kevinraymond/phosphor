use serde::{Deserialize, Serialize};

use crate::ui::theme::ThemeMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsConfig {
    pub version: u32,
    pub theme: ThemeMode,
    #[serde(default)]
    pub audio_device: Option<String>,
}

impl Default for SettingsConfig {
    fn default() -> Self {
        Self {
            version: 1,
            theme: ThemeMode::Dark,
            audio_device: None,
        }
    }
}

impl SettingsConfig {
    pub fn load() -> Self {
        let Some(config_dir) = dirs::config_dir() else {
            return Self::default();
        };
        let path = config_dir.join("phosphor").join("settings.json");
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let Some(config_dir) = dirs::config_dir() else {
            return;
        };
        let dir = config_dir.join("phosphor");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("settings.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_config_defaults() {
        let c = SettingsConfig::default();
        assert_eq!(c.version, 1);
        assert_eq!(c.theme, ThemeMode::Dark);
        assert!(c.audio_device.is_none());
    }

    #[test]
    fn settings_config_serde_roundtrip() {
        let c = SettingsConfig::default();
        let json = serde_json::to_string(&c).unwrap();
        let c2: SettingsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c2.version, 1);
        assert_eq!(c2.theme, ThemeMode::Dark);
    }

    #[test]
    fn settings_config_with_audio_device() {
        let mut c = SettingsConfig::default();
        c.audio_device = Some("hw:0".to_string());
        let json = serde_json::to_string(&c).unwrap();
        let c2: SettingsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c2.audio_device, Some("hw:0".to_string()));
    }
}
