//! The always-on agent: owns every device handle and audio call, enforces the
//! default-device lock, drives ChatMix, and serves state to the window over IPC.
//!
//! Threads: main = tray message pump, worker = HID + COM (single owner, so no
//! locking around hardware), one IPC acceptor + one reader per connected window.

use std::io::Write as _;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use hidapi::HidApi;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::SendHalf;
use openseries_core::audio::{self, Flow, ROLES};
use openseries_core::config::{Config, EndpointRef};
use openseries_core::devices::{self, Device, Found, Kind, Link, Setting};
use openseries_core::ipc::{self, AudioState, Check, Command, Diagnostics, Event, State};
use openseries_core::system::{self, Instance};
use tracing::{info, warn};

use crate::tray::TrayMsg;

type Clients = Arc<Mutex<Vec<SendHalf>>>;

pub fn run(lock: Instance, open_ui: bool) -> anyhow::Result<()> {
    migrate_go_config();
    crate::update::cleanup();
    thread::spawn(crate::ui::assets::harvest_gg);
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    let (tray_tx, tray_rx) = mpsc::channel::<TrayMsg>();
    let clients: Clients = Arc::default();
    let latest: Arc<Mutex<State>> = Arc::default();

    serve_ipc(cmd_tx.clone(), clients.clone(), latest.clone());
    thread::spawn(|| system::on_signal(crate::SHOW_EVENT, crate::spawn_ui));
    crate::update::schedule(tray_tx.clone());
    let worker = Worker::new(clients, latest, tray_tx.clone());
    thread::Builder::new().name("worker".into()).spawn(move || worker.run(cmd_rx))?;

    if open_ui {
        crate::spawn_ui();
    }
    crate::tray::run(tray_rx, cmd_tx, lock)
}

/// Accept window connections; each gets the current state immediately.
fn serve_ipc(cmds: Sender<Command>, clients: Clients, latest: Arc<Mutex<State>>) {
    let listener = match ipc::listen() {
        Ok(l) => l,
        Err(e) => {
            warn!("IPC unavailable, the window can't connect: {e}");
            return;
        }
    };
    thread::spawn(move || {
        for conn in listener.incoming().filter_map(Result::ok) {
            let (recv, mut send) = conn.split();
            let snapshot = latest.lock().unwrap().clone();
            if ipc::send(&mut send, &Event::State(Box::new(snapshot))).is_ok() {
                clients.lock().unwrap().push(send);
            }
            let cmds = cmds.clone();
            thread::spawn(move || {
                let _ = ipc::recv::<Command>(recv, |c| {
                    let _ = cmds.send(c);
                });
            });
        }
    });
}

fn broadcast(clients: &Clients, ev: &Event) {
    let mut line = serde_json::to_vec(ev).unwrap_or_default();
    line.push(b'\n');
    // Dropped windows fail the write and fall out of the list.
    clients.lock().unwrap().retain_mut(|c| c.write_all(&line).and_then(|_| c.flush()).is_ok());
}

struct Worker {
    api: Option<HidApi>,
    found: Vec<Found>,
    last_scan: Instant,
    last_audio: Instant,
    cfg: Config,
    devices: Vec<Device>,
    audio: AudioState,
    notice: Option<String>,
    update: Option<String>,
    autostart: bool,
    /// Dial position last pushed to app volumes, and when.
    chatmix_applied: Option<(u8, u8)>,
    chatmix_at: Instant,
    raw_log: Vec<(u16, Vec<u8>)>,
    clients: Clients,
    latest: Arc<Mutex<State>>,
    tray: Sender<TrayMsg>,
}

impl Worker {
    fn new(clients: Clients, latest: Arc<Mutex<State>>, tray: Sender<TrayMsg>) -> Self {
        let long_ago = Instant::now() - Duration::from_secs(60);
        Self {
            api: HidApi::new().map_err(|e| warn!("HID unavailable: {e}")).ok(),
            found: Vec::new(),
            last_scan: long_ago,
            last_audio: long_ago,
            cfg: Config::load(),
            devices: Vec::new(),
            audio: AudioState::default(),
            notice: None,
            update: None,
            autostart: system::autostart(),
            chatmix_applied: None,
            chatmix_at: long_ago,
            raw_log: Vec::new(),
            clients,
            latest,
            tray,
        }
    }

    fn run(mut self, rx: Receiver<Command>) {
        audio::init_com();
        loop {
            self.tick();
            // 300 ms keeps the ChatMix dial feeling live; commands wake us immediately.
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(c) => {
                    self.handle(c);
                    while let Ok(c) = rx.try_recv() {
                        self.handle(c);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn tick(&mut self) {
        if let Some(api) = self.api.as_mut() {
            if self.last_scan.elapsed() >= Duration::from_secs(3) {
                self.last_scan = Instant::now();
                self.found = devices::scan(api);
            }
            let api = &*api;
            self.devices = self.found.iter().map(|f| devices::read(api, f)).collect();
        }
        self.log_raw_changes();
        if self.last_audio.elapsed() >= Duration::from_secs(1) {
            self.last_audio = Instant::now();
            self.refresh_audio();
        }
        self.drive_chatmix();
        self.publish();
    }

    /// Unknown status bytes (mute button, volume wheel...) show up here first.
    fn log_raw_changes(&mut self) {
        for d in self.devices.iter().filter(|d| !d.raw.is_empty()) {
            match self.raw_log.iter_mut().find(|(pid, _)| *pid == d.pid) {
                Some((_, last)) if *last == d.raw => {}
                Some((_, last)) => {
                    info!("raw {:04x}: {:02x?}", d.pid, &d.raw[..d.raw.len().min(12)]);
                    *last = d.raw.clone();
                }
                None => self.raw_log.push((d.pid, d.raw.clone())),
            }
        }
    }

    fn refresh_audio(&mut self) {
        let outputs = audio::endpoints(Flow::Output);
        let inputs = audio::endpoints(Flow::Input);
        let mut changed = false;
        changed |= auto_pick(&outputs, &mut self.cfg.audio.output);
        changed |= auto_pick(&inputs, &mut self.cfg.audio.input);
        if changed {
            self.save();
        }
        let out = self.cfg.audio.output.as_ref().and_then(|w| audio::resolve(&outputs, w)).cloned();
        let inp = self.cfg.audio.input.as_ref().and_then(|w| audio::resolve(&inputs, w)).cloned();
        if self.cfg.audio.lock {
            for (flow, e) in [(Flow::Output, &out), (Flow::Input, &inp)] {
                let Some(e) = e else { continue }; // preferred device unplugged: leave Windows alone
                for role in ROLES {
                    let cur = audio::default_id(flow, role);
                    if cur.as_deref() != Some(e.id.as_str()) {
                        let was = cur.and_then(|id| outputs.iter().chain(&inputs).find(|x| x.id == id).map(|x| x.name.clone()));
                        let r = audio::set_default(&e.id, role);
                        info!("{flow:?} role {} was {was:?}, restored {:?}: {r:?}", role.0, e.name);
                    }
                }
            }
        }
        self.audio = AudioState {
            mic_muted: inp.as_ref().is_some_and(|e| audio::muted(&e.id)),
            output: out.map(|e| e.id),
            input: inp.map(|e| e.id),
            outputs,
            inputs,
        };
    }

    /// ChatMix without Sonar: the dial sets per-app volumes on the headset output.
    fn drive_chatmix(&mut self) {
        let Some(out) = self.audio.output.clone() else { return };
        let dial = self.devices.iter().find(|d| d.link == Link::Online).and_then(|d| d.chatmix).map(|c| (c.game, c.chat));
        let want = match (self.cfg.chatmix.enabled, dial) {
            (true, Some(d)) => Some(d),
            // Turned off: put every app back to full volume once.
            (false, _) if self.chatmix_applied.is_some() => Some((100, 100)),
            _ => return,
        };
        // Re-apply now and then so apps started later pick up the mix.
        if want == self.chatmix_applied && self.chatmix_at.elapsed() < Duration::from_secs(3) {
            return;
        }
        let (game, chat) = want.unwrap_or((100, 100));
        let apps = &self.cfg.chatmix.chat_apps;
        let is_chat = |name: &str| apps.iter().any(|a| a.eq_ignore_ascii_case(name));
        if let Err(e) = audio::apply_chatmix(&out, f32::from(game) / 100.0, f32::from(chat) / 100.0, is_chat) {
            warn!("chatmix: {e}");
        }
        self.chatmix_applied = if self.cfg.chatmix.enabled { want } else { None };
        self.chatmix_at = Instant::now();
    }

    fn publish(&mut self) {
        let state = State {
            version: openseries_core::VERSION.into(),
            devices: self.devices.clone(),
            audio: self.audio.clone(),
            config: self.cfg.clone(),
            autostart: self.autostart,
            update: self.update.clone(),
            notice: self.notice.clone(),
        };
        let mut latest = self.latest.lock().unwrap();
        if *latest == state {
            return;
        }
        *latest = state.clone();
        drop(latest);
        let primary = state.devices.iter().find(|d| d.kind == Kind::Headset).or(state.devices.first());
        let _ = self.tray.send(TrayMsg::Update { tooltip: tooltip(primary), device: primary.cloned() });
        broadcast(&self.clients, &Event::State(Box::new(state)));
    }

    fn handle(&mut self, c: Command) {
        info!("command: {c:?}");
        match c {
            Command::Apply { device, setting } => self.apply(&device, setting),
            Command::SetOutput(e) => self.set_pref(|cfg| cfg.audio.output = Some(e)),
            Command::SetInput(e) => self.set_pref(|cfg| cfg.audio.input = Some(e)),
            Command::SetLock(on) => self.set_pref(|cfg| cfg.audio.lock = on),
            Command::SetChatMix(on) => self.set_pref(|cfg| cfg.chatmix.enabled = on),
            Command::SetMicMuted(on) => {
                if let Some(id) = &self.audio.input {
                    let _ = audio::set_muted(id, on);
                }
                self.last_audio = Instant::now() - Duration::from_secs(5);
            }
            Command::SetAutostart(on) => {
                if let Err(e) = system::set_autostart(on) {
                    self.notice = Some(format!("Start with Windows: {e}"));
                }
                self.autostart = system::autostart();
            }
            Command::RunDiagnostics => {
                let d = self.diagnose();
                broadcast(&self.clients, &Event::Diagnostics(d));
            }
            Command::RestartReceiver(id) => {
                if let Err(e) = system::restart_device(&id) {
                    self.notice = Some(format!("Restart receiver: {e}"));
                }
            }
            Command::CheckForUpdates => {
                self.update = Some("Checking for updates…".into());
                let tray = self.tray.clone();
                thread::spawn(move || crate::update::check_now(&tray));
            }
            Command::Quit => {
                let _ = self.tray.send(TrayMsg::Quit);
            }
        }
    }

    fn set_pref(&mut self, f: impl FnOnce(&mut Config)) {
        f(&mut self.cfg);
        self.save();
        self.last_audio = Instant::now() - Duration::from_secs(5);
    }

    fn apply(&mut self, device: &str, setting: Setting) {
        let Some(api) = &self.api else { return };
        let Some(found) = self.found.iter().find(|f| f.model.is_some_and(|m| m.key == device)) else {
            self.notice = Some("Device is not connected".into());
            return;
        };
        match devices::apply(api, found, &setting) {
            Ok(()) => {
                self.notice = None;
                let s = self.cfg.device(device);
                match setting {
                    Setting::Sidetone(v) => s.sidetone = Some(v),
                    Setting::AutoOff(v) => s.auto_off = Some(v),
                    Setting::Equalizer(bands) => {
                        s.eq_preset = Some(devices::eq_preset_name(&bands).unwrap_or("Custom").into());
                        s.eq = Some(bands);
                    }
                }
                self.save();
            }
            Err(e) => self.notice = Some(format!("Couldn't change the setting: {e}")),
        }
    }

    fn save(&self) {
        if let Err(e) = self.cfg.save() {
            warn!("saving config: {e}");
        }
    }

    fn diagnose(&mut self) -> Diagnostics {
        let mut checks = Vec::new();
        let mut check = |ok: bool, title: &str, detail: String| checks.push(Check { ok, title: title.into(), detail });
        let mut restart = None;

        let nodes = system::pnp_devices("VID_1038");
        let broken: Vec<_> = nodes.iter().filter(|n| n.problem != 0).collect();
        check(
            broken.is_empty(),
            "Drivers",
            if broken.is_empty() {
                format!("{} SteelSeries device nodes, all working", nodes.len())
            } else {
                broken.iter().map(|n| format!("{} (Device Manager code {})", n.name, n.problem)).collect::<Vec<_>>().join(", ")
            },
        );
        if self.devices.is_empty() {
            check(false, "Devices", "No SteelSeries device found. Plug the receiver directly into the PC, not a hub.".into());
        }
        for d in &self.devices {
            let (ok, detail) = match d.link {
                Link::Online => (true, d.battery.map_or("Connected".into(), |b| format!("Connected, battery {}%", b.percent))),
                Link::Asleep => (true, "Receiver OK, headset is off or out of range".into()),
                Link::NotResponding => (false, "Receiver is not answering. Restarting it usually fixes this.".into()),
                Link::Unsupported => (true, "Detected (no controls in OpenSeries yet)".into()),
            };
            if !ok || !broken.is_empty() {
                let usb = format!("VID_1038&PID_{:04X}", d.pid);
                restart = nodes.iter().find(|n| n.instance_id.to_ascii_uppercase().starts_with(&format!("USB\\{usb}\\"))).map(|n| n.instance_id.clone());
            }
            check(ok, &d.name, detail);
        }

        let gg = system::steelseries_processes();
        let sonar = gg.iter().any(|p| p.name.to_ascii_lowercase().contains("sonar"));
        for (flow, label, pref) in [(Flow::Output, "Output", &self.cfg.audio.output), (Flow::Input, "Microphone", &self.cfg.audio.input)] {
            let eps = audio::endpoints(flow);
            let want = pref.as_ref().and_then(|p| audio::resolve(&eps, p));
            for (role, role_name) in [(ROLES[0], "default"), (ROLES[2], "communications")] {
                let cur = audio::default_id(flow, role).and_then(|id| eps.iter().find(|e| e.id == id)).map(|e| e.name.clone()).unwrap_or_else(|| "none".into());
                let ok = want.is_none_or(|w| w.name == cur);
                let mut detail = cur.clone();
                if cur.contains("Sonar") && !sonar {
                    detail.push_str(" (a Sonar device, but Sonar isn't running, so it's silent)");
                }
                check(ok, &format!("Windows {role_name} {}", label.to_lowercase()), detail);
            }
        }
        if !gg.is_empty() {
            let mb: u64 = gg.iter().filter_map(|p| p.mb).sum();
            check(true, "SteelSeries GG", format!("Still installed and running: {} processes using {mb} MB. You can uninstall it.", gg.len()));
        }
        Diagnostics { checks, restart }
    }
}

/// First run: prefer the SteelSeries headset (never Sonar's virtual devices).
fn auto_pick(eps: &[EndpointRef], pref: &mut Option<EndpointRef>) -> bool {
    if pref.is_some() {
        return false;
    }
    let pick = eps.iter().find(|e| e.name.contains("Arctis") && !e.name.contains("Sonar"));
    if let Some(e) = pick {
        info!("auto-picked {}", e.name);
        *pref = Some(e.clone());
    }
    pick.is_some()
}

fn tooltip(d: Option<&Device>) -> String {
    let Some(d) = d else { return "OpenSeries: no device".into() };
    match (d.link, d.battery) {
        (Link::Online, Some(b)) if b.charging => format!("{}: {}% · charging", d.name, b.percent),
        (Link::Online, Some(b)) => format!("{}: {}%", d.name, b.percent),
        (Link::Online, None) => format!("{}: connected", d.name),
        (Link::Asleep, _) => format!("{}: off", d.name),
        (Link::NotResponding, _) => format!("{}: not responding", d.name),
        (Link::Unsupported, _) => d.name.clone(),
    }
}

/// Carry device choices over from the Go build's config.json.
fn migrate_go_config() {
    let dir = openseries_core::paths::config_dir();
    let (json, toml) = (dir.join("config.json"), openseries_core::paths::config_file());
    if toml.exists() || !json.exists() {
        return;
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Ep {
        id: String,
        name: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Old {
        out: Option<Ep>,
        r#in: Option<Ep>,
        #[serde(default)]
        unlocked: bool,
    }
    let Ok(old) = std::fs::read_to_string(&json).map_err(anyhow::Error::from).and_then(|s| Ok(serde_json::from_str::<Old>(&s)?)) else { return };
    let mut cfg = Config::default();
    let conv = |e: Ep| EndpointRef { id: e.id, name: e.name };
    cfg.audio.output = old.out.filter(|e| !e.id.is_empty()).map(conv);
    cfg.audio.input = old.r#in.filter(|e| !e.id.is_empty()).map(conv);
    cfg.audio.lock = !old.unlocked;
    if cfg.save().is_ok() {
        info!("migrated config.json from the Go build");
    }
}
