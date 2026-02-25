use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::types::{MidiMsgType, TriggerAction};

/// A single MIDI CC/Note → parameter or trigger mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiMapping {
    pub cc: u8,
    pub channel: u8, // 0 = omni (any channel)
    pub msg_type: MidiMsgType,
    #[serde(default = "default_min")]
    pub min_val: f32,
    #[serde(default = "default_max")]
    pub max_val: f32,
    #[serde(default)]
    pub invert: bool,
}

fn default_min() -> f32 {
    0.0
}
fn default_max() -> f32 {
    1.0
}

impl MidiMapping {
    /// Create a new mapping from a learned message.
    pub fn from_learn(cc: u8, _channel: u8, msg_type: MidiMsgType) -> Self {
        Self {
            cc,
            channel: 0, // omni by default
            msg_type,
            min_val: 0.0,
            max_val: 1.0,
            invert: false,
        }
    }

    /// Scale a raw 0-127 MIDI value to the mapped range.
    pub fn scale(&self, raw: u8) -> f32 {
        let normalized = raw as f32 / 127.0;
        let normalized = if self.invert {
            1.0 - normalized
        } else {
            normalized
        };
        self.min_val + (self.max_val - self.min_val) * normalized
    }

    /// Check if this mapping matches a given CC/channel/type.
    pub fn matches(&self, number: u8, channel: u8, msg_type: MidiMsgType) -> bool {
        self.cc == number
            && self.msg_type == msg_type
            && (self.channel == 0 || self.channel == channel)
    }
}

/// Persisted MIDI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub params: HashMap<String, MidiMapping>,
    #[serde(default)]
    pub triggers: HashMap<TriggerAction, MidiMapping>,
    #[serde(default)]
    pub port_name: Option<String>,
}

fn default_version() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

impl Default for MidiConfig {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: true,
            params: HashMap::new(),
            triggers: HashMap::new(),
            port_name: None,
        }
    }
}

impl MidiConfig {
    /// Path to the MIDI config file (~/.config/phosphor/midi.json).
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        config_dir.join("phosphor").join("midi.json")
    }

    /// Load config from disk, falling back to default on any error.
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(config) => {
                    log::info!("Loaded MIDI config from {}", path.display());
                    config
                }
                Err(e) => {
                    log::warn!("Failed to parse MIDI config: {e}");
                    Self::default()
                }
            },
            Err(_) => {
                log::info!("No MIDI config found, using defaults");
                Self::default()
            }
        }
    }

    /// Save config to disk.
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
                    log::error!("Failed to write MIDI config: {e}");
                } else {
                    log::debug!("Saved MIDI config to {}", path.display());
                }
            }
            Err(e) => log::error!("Failed to serialize MIDI config: {e}"),
        }
    }

    /// Find which param name is mapped to the given CC/channel/type.
    pub fn find_param(&self, number: u8, channel: u8, msg_type: MidiMsgType) -> Option<&str> {
        for (name, mapping) in &self.params {
            if mapping.matches(number, channel, msg_type) {
                return Some(name.as_str());
            }
        }
        None
    }

    /// Find which trigger action is mapped to the given CC/channel/type.
    pub fn find_trigger(
        &self,
        number: u8,
        channel: u8,
        msg_type: MidiMsgType,
    ) -> Option<TriggerAction> {
        for (action, mapping) in &self.triggers {
            if mapping.matches(number, channel, msg_type) {
                return Some(*action);
            }
        }
        None
    }
}
