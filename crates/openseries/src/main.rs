//! `openseries`         start the agent (tray) and open the window
//! `openseries --tray`  start the agent only (autostart, and what the Go v0.2 updater runs)
//! `openseries --ui`    the window process; spawned by the agent

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent;
mod icon;
mod tray;
mod ui;
mod update;

use openseries_core::system::{Instance, signal};

/// Mutex / event names shared by both processes.
pub const AGENT_LOCK: &str = "OpenSeries";
pub const SHOW_EVENT: &str = "OpenSeries.show";
pub const UI_LOCK: &str = "OpenSeries.ui";
pub const UI_SHOW_EVENT: &str = "OpenSeries.ui.show";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);

    if has("--ui") {
        let Some(_lock) = Instance::acquire(UI_LOCK) else {
            signal(UI_SHOW_EVENT);
            return Ok(());
        };
        init_logging("ui.log");
        ui::run();
        return Ok(());
    }

    let Some(lock) = Instance::acquire(AGENT_LOCK) else {
        // Already running (Go or Rust build): ask it to show its window.
        signal(SHOW_EVENT);
        return Ok(());
    };
    init_logging("agent.log");
    agent::run(lock, !has("--tray"))
}

fn init_logging(file: &str) {
    let path = openseries_core::paths::config_dir().join(file);
    let writer = std::fs::File::create(path).map(std::sync::Mutex::new);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    match writer {
        Ok(w) => tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false).with_writer(w).init(),
        Err(_) => tracing_subscriber::fmt().with_env_filter(filter).init(),
    }
    tracing::info!("{} {}", openseries_core::APP_NAME, openseries_core::VERSION);
}

/// Start the window process (it exits again when closed).
pub fn spawn_ui() {
    match std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg("--ui").spawn()) {
        Ok(_) => {}
        Err(e) => tracing::warn!("could not start the window: {e}"),
    }
}
