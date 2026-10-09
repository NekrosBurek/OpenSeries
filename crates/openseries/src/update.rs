//! Self-update from GitHub Releases: one `openseries-<os>-<arch>[.exe]` asset per platform.
//! Same asset name as the Go v0.2 builds, so they update straight into this one.

use std::io::Read as _;
use std::sync::mpsc::Sender;
use std::time::Duration;

use openseries_core::{REPO, VERSION};
use tracing::{info, warn};

use crate::tray::TrayMsg;

fn asset_name() -> String {
    let os = if cfg!(windows) { "windows" } else { std::env::consts::OS };
    let arch = if cfg!(target_arch = "x86_64") { "amd64" } else { std::env::consts::ARCH };
    format!("openseries-{os}-{arch}{}", std::env::consts::EXE_SUFFIX)
}

/// Check a minute after start, then daily.
pub fn schedule(tray: Sender<TrayMsg>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        loop {
            check_now(&tray);
            std::thread::sleep(Duration::from_secs(24 * 3600));
        }
    });
}

pub fn check_now(tray: &Sender<TrayMsg>) {
    if cfg!(debug_assertions) {
        return; // dev builds never replace themselves
    }
    match update() {
        Ok(Some(tag)) => {
            info!("installed {tag}, restarting");
            let _ = tray.send(TrayMsg::Restart);
        }
        Ok(None) => info!("up to date ({VERSION})"),
        Err(e) => warn!("update check failed: {e}"),
    }
}

fn update() -> anyhow::Result<Option<String>> {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
        assets: Vec<Asset>,
    }
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    let agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(120))).build().new_agent();
    let rel: Release = agent
        .get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", "OpenSeries")
        .call()?
        .body_mut()
        .read_json()?;
    if !newer(&rel.tag_name, VERSION) {
        return Ok(None);
    }
    let want = asset_name();
    let asset = rel.assets.iter().find(|a| a.name == want).ok_or_else(|| anyhow::anyhow!("{} has no {want}", rel.tag_name))?;
    let mut bytes = Vec::new();
    agent.get(&asset.browser_download_url).call()?.into_body().into_reader().read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() > 1 << 20, "download looks truncated ({} bytes)", bytes.len());

    let exe = std::env::current_exe()?;
    let (new, old) = (exe.with_extension("new"), exe.with_extension("old"));
    std::fs::write(&new, &bytes)?;
    // Windows can rename a running executable but not overwrite it.
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old)?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(e.into());
    }
    Ok(Some(rel.tag_name))
}

/// `v0.10.0` > `0.9.3`; pre-release suffixes are ignored.
fn newer(tag: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> { s.trim_start_matches('v').split(['.', '-']).take(3).map(|p| p.parse().unwrap_or(0)).collect() };
    parse(tag) > parse(current)
}

/// Remove the previous binary left behind by an update.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_compare() {
        assert!(super::newer("v0.10.0", "0.9.3"));
        assert!(!super::newer("v0.3.0", "0.3.0"));
        assert!(!super::newer("v0.2.0", "0.3.0"));
    }
}
