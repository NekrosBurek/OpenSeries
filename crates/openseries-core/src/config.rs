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
    /// Sound profiles; the first one is the fallback when no game is running.
    pub profiles: Vec<Profile>,
    /// Switch profile automatically when a game comes to the foreground.
    pub auto_switch: bool,
}

/// A named sound setup, applied when one of its apps (or, with `all_games`, any
/// detected game) is in the foreground.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// One of the built-in icons: "default", "competitive", "immersive", "voice".
    pub icon: String,
    pub eq: Vec<f32>,
    /// Process names (without `.exe`, case-insensitive).
    pub apps: Vec<String>,
    /// Catch-all for games that no other profile lists.
    pub all_games: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self { name: "Profile".into(), icon: "default".into(), eq: vec![0.0; 10], apps: Vec::new(), all_games: false }
    }
}

/// Starter profiles. Competitive titles get a footstep-forward EQ.
pub fn default_profiles() -> Vec<Profile> {
    let p = |name: &str, icon: &str, eq: [f32; 10], apps: &[&str], all_games: bool| Profile {
        name: name.into(),
        icon: icon.into(),
        eq: eq.to_vec(),
        apps: apps.iter().map(|s| s.to_string()).collect(),
        all_games,
    };
    vec![
        p("Default", "default", [0.0; 10], &[], false),
        p(
            "Competitive",
            "competitive",
            crate::devices::EQ_FOOTSTEPS,
            &[
                "cs2", "valorant-win64-shipping", "r5apex", "r5apex_dx12", "fortniteclient-win64-shipping", "tslgame", "overwatch",
                "rainbowsix", "rainbowsix_vulkan", "escapefromtarkov", "cod", "modernwarfare", "project8", "marvel-win64-shipping",
                "thefinals", "deltaforceclient-win64-shipping", "rustclient", "hunt", "bf2042", "bf6",
            ],
            false,
        ),
        p("Immersive", "immersive", crate::devices::EQ_IMMERSIVE, &[], true),
        p("Voice", "voice", crate::devices::EQ_VOICE, &["ms-teams", "teams", "zoom"], false),
    ]
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
    pub dpi: Option<Vec<u16>>,
    pub polling: Option<u16>,
    pub lighting: Option<Lighting>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lighting {
    pub rgb: [u8; 3],
    pub brightness: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self { audio: AudioPrefs::default(), chatmix: ChatMixPrefs::default(), devices: BTreeMap::new(), profiles: default_profiles(), auto_switch: true }
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

    /// Profile for a foreground process: an explicit app match wins, then the
    /// all-games catch-all, then the first (default) profile.
    pub fn profile_for(&self, process: Option<&str>, is_game: bool) -> usize {
        let Some(name) = process else { return 0 };
        self.profiles
            .iter()
            .position(|p| p.apps.iter().any(|a| a.eq_ignore_ascii_case(name)))
            .or_else(|| is_game.then(|| self.profiles.iter().position(|p| p.all_games)).flatten())
            .unwrap_or(0)
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
        assert_eq!(partial.profiles.len(), 4);
    }

    #[test]
    fn profile_matching() {
        let c = Config::default();
        let name = |p: Option<&str>, game| c.profiles[c.profile_for(p, game)].name.as_str();
        assert_eq!(name(Some("CS2"), true), "Competitive");
        assert_eq!(name(Some("eldenring"), true), "Immersive");
        assert_eq!(name(Some("ms-teams"), false), "Voice");
        assert_eq!(name(Some("chrome"), false), "Default");
        assert_eq!(name(None, false), "Default");
    }
}
