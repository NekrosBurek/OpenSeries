//! Product renders: cached under %APPDATA%\OpenSeries\images, filled from a local
//! GG install if present, otherwise from the openseries-assets mirror.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use openseries_core::devices::{Device, generic};
use openseries_core::ipc::State;
use tokio::sync::mpsc::UnboundedSender;

use super::Msg;

const GG_DIR: &str = r"C:\ProgramData\SteelSeries\GG\apps\engine\assets\loadoutImages\devices";

static NOTIFY: OnceLock<UnboundedSender<Msg>> = OnceLock::new();
static IN_FLIGHT: Mutex<Option<HashSet<String>>> = Mutex::new(None);

pub(super) fn set_notifier(tx: UnboundedSender<Msg>) {
    let _ = NOTIFY.set(tx);
}

fn cached(name: &str) -> PathBuf {
    openseries_core::paths::image_cache().join(name)
}

/// HD variant of a render (`arctis_7.webp` -> `hd/arctis_7.png`).
fn hd_name(image: &str) -> String {
    format!("hd/{}.png", image.trim_end_matches(".webp"))
}

/// Render to show for a device: HD, else its own, else the generic for its kind, else nothing yet.
pub fn image_for(d: &Device) -> Option<PathBuf> {
    let hd = cached(&hd_name(&d.image));
    if hd.exists() {
        return Some(hd);
    }
    let own = cached(&d.image);
    if own.exists() {
        return Some(own);
    }
    let fallback = cached(&format!("generics/{}.webp", generic(d.kind)));
    fallback.exists().then_some(fallback)
}

/// Fetch any missing renders in the background; the view repaints when they land.
pub(super) fn prefetch(s: &State, _cx: &mut gpui::App) {
    for d in &s.devices {
        for name in [hd_name(&d.image), d.image.clone(), format!("generics/{}.webp", generic(d.kind))] {
            if cached(&name).exists() {
                continue;
            }
            let mut guard = IN_FLIGHT.lock().unwrap();
            if !guard.get_or_insert_with(HashSet::new).insert(name.clone()) {
                continue;
            }
            drop(guard);
            std::thread::spawn(move || {
                match fetch(&name) {
                    Ok(()) => {
                        if let Some(tx) = NOTIFY.get() {
                            let _ = tx.send(Msg::AssetReady);
                        }
                    }
                    Err(e) => tracing::info!("no render for {name}: {e}"),
                }
            });
        }
    }
}

fn fetch(name: &str) -> anyhow::Result<()> {
    let dest = cached(name);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let local = Path::new(GG_DIR).join(name);
    if local.exists() {
        std::fs::copy(local, &dest)?;
        return Ok(());
    }
    // Mirror layout: devices/<name>.webp (GG renders) and hd/<name>.png.
    let url = match name.strip_prefix("hd/") {
        Some(hd) => format!("{}/hd/{hd}", openseries_core::ASSETS_ROOT),
        None => format!("{}/devices/{name}", openseries_core::ASSETS_ROOT),
    };
    let mut body = ureq::get(&url).call()?.into_body();
    let bytes = body.read_to_vec()?;
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, dest)?;
    Ok(())
}

/// Copy every render a local GG install has, so they survive uninstalling GG.
pub fn harvest_gg() {
    let Ok(entries) = std::fs::read_dir(GG_DIR) else { return };
    let cache = openseries_core::paths::image_cache();
    for e in entries.flatten() {
        let src = e.path();
        if src.is_dir() {
            let sub = cache.join(e.file_name());
            let _ = std::fs::create_dir_all(&sub);
            for f in std::fs::read_dir(&src).into_iter().flatten().flatten() {
                let _ = copy_if_missing(&f.path(), &sub.join(f.file_name()));
            }
        } else {
            let _ = copy_if_missing(&src, &cache.join(e.file_name()));
        }
    }
}

fn copy_if_missing(src: &Path, dest: &Path) -> std::io::Result<()> {
    if !dest.exists() {
        std::fs::copy(src, dest)?;
    }
    Ok(())
}
