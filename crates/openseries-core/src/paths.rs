use std::path::PathBuf;

/// `%APPDATA%\OpenSeries` (Windows) / `$XDG_CONFIG_HOME/openseries` (Linux).
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(crate::APP_NAME);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn log_file() -> PathBuf {
    config_dir().join("agent.log")
}

/// Cached product renders, keyed by image name (`arctis_7.webp`).
pub fn image_cache() -> PathBuf {
    let dir = config_dir().join("images");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Local socket the agent listens on.
pub const PIPE: &str = "openseries-agent.sock";
