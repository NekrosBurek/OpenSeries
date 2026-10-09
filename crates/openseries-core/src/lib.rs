//! Device knowledge and platform plumbing shared by the agent and the desktop UI.
//!
//! The agent owns all hardware I/O; the desktop only renders [`ipc::State`] and
//! sends [`ipc::Command`]s. Nothing in here touches a UI toolkit.

pub mod config;
pub mod devices;
pub mod ipc;
pub mod paths;

#[cfg(windows)]
pub mod audio;
#[cfg(windows)]
pub mod system;

pub const APP_NAME: &str = "OpenSeries";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO: &str = "NekrosBurek/OpenSeries";
/// Product renders live outside the code repo (same split as OpenLogi's asset mirror).
pub const ASSETS_BASE: &str = "https://raw.githubusercontent.com/NekrosBurek/openseries-assets/main/devices";
