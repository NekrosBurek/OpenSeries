//! `config.toml`: one plain-text file the user can read, edit and sync.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    pub audio: AudioPrefs,
    pub chatmix: ChatMixPrefs,
    /// Last values written to each model (headsets can't report them back).
    pub devices: BTreeMap<String, DeviceSettings>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct AudioPrefs {
    pub output: Option<EndpointRef>,
    pub input: Option<EndpointRef>,
    /// Keep `output`/`input` as Windows default for every role, including Communications.
    pub lock: bool,
}

/// A Windows audio endpoint. Matched by `id`, then by `name` with the "4- " USB-port
/// prefix ignored, so moving the receiver to another port keeps the preference.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EndpointRef {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ChatMixPrefs {
    /// Drive per-app volumes from the headset's ChatMix dial (replaces Sonar).
    pub enabled: bool,
    /// Process names (case-insensitive, without `.exe`) treated as chat.
    pub chat_apps: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct DeviceSettings {
    pub sidetone: Option<u8>,
    pub auto_off: Option<u8>,
    pub eq: Option<Vec<f32>>,
    pub eq_preset: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self { audio: AudioPrefs::default(), chatmix: ChatMixPrefs::default(), devices: BTreeMap::new() }
    }
}

impl Default for AudioPrefs {
    fn default() -> Self {
        Self { output: None, input: None, lock: true }
    }
}

impl Default for ChatMixPrefs {
    fn default() -> Self {
        let apps = ["discord", "teams", "ms-teams", "zoom", "slack", "skype", "telegram", "signal", "whatsapp", "mumble", "teamspeak", "ts3client_win64", "element", "guilded"];
        Self { enabled: true, chat_apps: apps.iter().map(|s| s.to_string()).collect() }
    }
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(crate::paths::config_file())
            .ok()
            .and_then(|s| toml::from_str(&s).map_err(|e| tracing::warn!("config.toml ignored: {e}")).ok())
            .unwrap_or_default()
    }

    /// Write via a temp file + rename so a crash never leaves a half-written config.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = crate::paths::config_file();
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn device(&mut self, model: &str) -> &mut DeviceSettings {
        self.devices.entry(model.to_string()).or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let mut c = Config::default();
        assert!(c.audio.lock && c.chatmix.enabled);
        c.device("arctis_7_plus").sidetone = Some(2);
        let back: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(back, c);
        // Missing sections fall back to defaults instead of failing.
        let partial: Config = toml::from_str("[audio]\nlock = false\n").unwrap();
        assert!(!partial.audio.lock && partial.chatmix.enabled);
    }
}
