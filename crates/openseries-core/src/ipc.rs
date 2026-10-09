//! Agent <-> desktop protocol: newline-delimited JSON over a local socket
//! (named pipe on Windows). The agent pushes a full [`State`] whenever anything
//! changes; the desktop sends [`Command`]s. Full-state pushes keep the UI dumb.

use std::io::{self, BufRead, BufReader, Write};

use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream, prelude::*};
use serde::{Deserialize, Serialize};

use crate::config::{Config, EndpointRef};
use crate::devices::{Device, Setting};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct State {
    pub version: String,
    pub devices: Vec<Device>,
    pub audio: AudioState,
    pub config: Config,
    pub autostart: bool,
    /// Last update check result, for the footer.
    pub update: Option<String>,
    /// Last device command error, if any.
    pub notice: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AudioState {
    pub outputs: Vec<EndpointRef>,
    pub inputs: Vec<EndpointRef>,
    /// Endpoint the preference currently resolves to (None = unplugged).
    pub output: Option<String>,
    pub input: Option<String>,
    pub mic_muted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Command {
    Apply { device: String, setting: Setting },
    SetOutput(EndpointRef),
    SetInput(EndpointRef),
    SetLock(bool),
    SetMicMuted(bool),
    SetChatMix(bool),
    SetAutostart(bool),
    RunDiagnostics,
    RestartReceiver(String),
    CheckForUpdates,
    Quit,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Event {
    State(Box<State>),
    Diagnostics(Diagnostics),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Diagnostics {
    pub checks: Vec<Check>,
    /// USB instance id of the receiver, when restarting it may help.
    pub restart: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Check {
    pub ok: bool,
    pub title: String,
    pub detail: String,
}

fn name() -> io::Result<interprocess::local_socket::Name<'static>> {
    crate::paths::PIPE.to_ns_name::<GenericNamespaced>()
}

pub fn listen() -> io::Result<interprocess::local_socket::Listener> {
    ListenerOptions::new().name(name()?).create_sync()
}

pub fn connect() -> io::Result<Stream> {
    Stream::connect(name()?)
}

pub fn send<T: Serialize>(w: &mut impl Write, msg: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(msg)?;
    line.push(b'\n');
    w.write_all(&line)?;
    w.flush()
}

/// Read messages until EOF; malformed lines are logged and skipped.
pub fn recv<T: for<'de> Deserialize<'de>>(r: impl io::Read, mut f: impl FnMut(T)) -> io::Result<()> {
    for line in BufReader::new(r).lines() {
        match serde_json::from_str(&line?) {
            Ok(msg) => f(msg),
            Err(e) => tracing::warn!("bad ipc message: {e}"),
        }
    }
    Ok(())
}
