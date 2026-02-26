use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::effect::format::PostProcessDef;
use crate::gpu::layer::BlendMode;
use crate::params::ParamValue;

/// Per-layer state saved in a preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerPreset {
    pub effect_name: String,
    #[serde(default)]
    pub params: HashMap<String, ParamValue>,
    #[serde(default)]
    pub blend_mode: BlendMode,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub custom_name: Option<String>,
    #[serde(default)]
    pub media_path: Option<String>,
    #[serde(default)]
    pub media_speed: Option<f32>,
    #[serde(default)]
    pub media_looping: Option<bool>,
}

fn default_opacity() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

impl Default for BlendMode {
    fn default() -> Self {
        BlendMode::Normal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub layers: Vec<LayerPreset>,
    #[serde(default)]
    pub active_layer: usize,
    #[serde(default)]
    pub postprocess: PostProcessDef,
}

pub struct PresetStore {
    pub presets: Vec<(String, Preset)>,
    pub current_preset: Option<usize>,
    pub dirty: bool,
}

impl PresetStore {
    pub fn new() -> Self {
        Self {
            presets: Vec::new(),
            current_preset: None,
            dirty: false,
        }
    }

    /// Mark the current preset as dirty (modified since last save/load).
    pub fn mark_dirty(&mut self) {
        if self.current_preset.is_some() {
            self.dirty = true;
        }
    }

    /// Get the name of the currently loaded preset, if any.
    pub fn current_name(&self) -> Option<&str> {
        self.current_preset
            .and_then(|i| self.presets.get(i))
            .map(|(name, _)| name.as_str())
    }

    pub fn presets_dir() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        config_dir.join("phosphor").join("presets")
    }

    pub fn scan(&mut self) {
        let dir = Self::presets_dir();
        self.presets.clear();

        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(contents) => match serde_json::from_str::<Preset>(&contents) {
                    Ok(preset) => {
                        self.presets.push((name, preset));
                    }
                    Err(e) => {
                        log::warn!("Failed to parse preset {}: {e}", path.display());
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read preset {}: {e}", path.display());
                }
            }
        }

        self.presets.sort_by(|a, b| a.0.cmp(&b.0));
        self.current_preset = None;
        self.dirty = false;

        log::info!("Scanned {} presets from {}", self.presets.len(), dir.display());
    }

    fn sanitize_name(name: &str) -> String {
        let sanitized: String = name
            .chars()
            .map(|c| if c == '/' || c == '\\' || c == '.' { '_' } else { c })
            .collect();
        let trimmed = sanitized.trim();
        if trimmed.len() > 64 {
            trimmed[..64].to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn save(
        &mut self,
        name: &str,
        layers: Vec<LayerPreset>,
        active_layer: usize,
        postprocess: &PostProcessDef,
    ) -> Result<usize> {
        let name = Self::sanitize_name(name);
        if name.is_empty() {
            anyhow::bail!("Preset name cannot be empty");
        }

        let dir = Self::presets_dir();
        std::fs::create_dir_all(&dir)?;

        let preset = Preset {
            layers,
            active_layer,
            postprocess: postprocess.clone(),
        };

        let path = dir.join(format!("{name}.json"));
        let json = serde_json::to_string_pretty(&preset)?;
        std::fs::write(&path, json)?;
        log::info!("Saved preset '{}' to {}", name, path.display());

        self.scan();

        let idx = self
            .presets
            .iter()
            .position(|(n, _)| n == &name)
            .unwrap_or(0);
        self.current_preset = Some(idx);
        self.dirty = false;
        Ok(idx)
    }

    pub fn load(&self, index: usize) -> Option<&Preset> {
        self.presets.get(index).map(|(_, p)| p)
    }

    pub fn delete(&mut self, index: usize) -> Result<()> {
        let (name, _) = self
            .presets
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("Invalid preset index"))?;

        let path = Self::presets_dir().join(format!("{name}.json"));
        if path.exists() {
            std::fs::remove_file(&path)?;
            log::info!("Deleted preset '{}'", name);
        }

        self.scan();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_name_strips_slashes() {
        assert_eq!(PresetStore::sanitize_name("a/b\\c"), "a_b_c");
    }

    #[test]
    fn sanitize_name_strips_dots() {
        assert_eq!(PresetStore::sanitize_name("my.preset"), "my_preset");
    }

    #[test]
    fn sanitize_name_trims_whitespace() {
        assert_eq!(PresetStore::sanitize_name("  hello  "), "hello");
    }

    #[test]
    fn sanitize_name_max_64_chars() {
        let long = "a".repeat(100);
        assert_eq!(PresetStore::sanitize_name(&long).len(), 64);
    }

    #[test]
    fn preset_store_new_empty() {
        let s = PresetStore::new();
        assert!(s.presets.is_empty());
        assert!(s.current_preset.is_none());
        assert!(!s.dirty);
    }

    #[test]
    fn mark_dirty_without_current_preset() {
        let mut s = PresetStore::new();
        s.mark_dirty();
        assert!(!s.dirty); // no current preset, so dirty stays false
    }

    #[test]
    fn mark_dirty_with_current_preset() {
        let mut s = PresetStore::new();
        s.current_preset = Some(0);
        s.mark_dirty();
        assert!(s.dirty);
    }

    #[test]
    fn current_name_returns_correct() {
        let mut s = PresetStore::new();
        let preset = Preset {
            layers: vec![],
            active_layer: 0,
            postprocess: PostProcessDef::default(),
        };
        s.presets.push(("Test Preset".into(), preset));
        s.current_preset = Some(0);
        assert_eq!(s.current_name(), Some("Test Preset"));
    }

    // ---- Additional tests ----

    #[test]
    fn sanitize_name_all_bad_chars() {
        assert_eq!(PresetStore::sanitize_name("/.\\."), "____");
    }

    #[test]
    fn sanitize_name_whitespace_only() {
        assert_eq!(PresetStore::sanitize_name("   "), "");
    }

    #[test]
    fn sanitize_name_exact_64_chars() {
        let s = "a".repeat(64);
        assert_eq!(PresetStore::sanitize_name(&s).len(), 64);
    }

    #[test]
    fn sanitize_name_65_truncated() {
        let s = "b".repeat(65);
        assert_eq!(PresetStore::sanitize_name(&s).len(), 64);
    }

    #[test]
    fn layer_preset_minimal_json_defaults() {
        let json = r#"{"effect_name": "Aurora"}"#;
        let lp: LayerPreset = serde_json::from_str(json).unwrap();
        assert_eq!(lp.effect_name, "Aurora");
        assert!(lp.params.is_empty());
        assert_eq!(lp.blend_mode, BlendMode::Normal);
        assert!((lp.opacity - 1.0).abs() < 1e-6);
        assert!(lp.enabled);
        assert!(!lp.locked);
        assert!(!lp.pinned);
        assert!(lp.custom_name.is_none());
        assert!(lp.media_path.is_none());
    }

    #[test]
    fn preset_serde_roundtrip() {
        let preset = Preset {
            layers: vec![LayerPreset {
                effect_name: "Test".into(),
                params: HashMap::new(),
                blend_mode: BlendMode::Add,
                opacity: 0.5,
                enabled: true,
                locked: false,
                pinned: true,
                custom_name: Some("My Layer".into()),
                media_path: None,
                media_speed: None,
                media_looping: None,
            }],
            active_layer: 0,
            postprocess: PostProcessDef::default(),
        };
        let json = serde_json::to_string(&preset).unwrap();
        let p2: Preset = serde_json::from_str(&json).unwrap();
        assert_eq!(p2.layers.len(), 1);
        assert_eq!(p2.layers[0].effect_name, "Test");
        assert_eq!(p2.layers[0].blend_mode, BlendMode::Add);
        assert!((p2.layers[0].opacity - 0.5).abs() < 1e-6);
        assert!(p2.layers[0].pinned);
    }

    #[test]
    fn current_name_none_when_no_preset() {
        let s = PresetStore::new();
        assert!(s.current_name().is_none());
    }
}
