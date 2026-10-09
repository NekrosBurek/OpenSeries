//! The window process. Renders the agent's [`State`] and sends [`Command`]s back.
//! It holds no device handles, so it can come and go without disturbing anything.

pub mod assets;
mod headset;
mod pages;
mod widgets;

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{Bounds, ClickEvent, Size, Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions, div, px};
use gpui_component::{ActiveTheme as _, Root, Theme, ThemeMode, h_flex, v_flex};
use openseries_core::ipc::{self, Command, Diagnostics, Event, State};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};


enum Msg {
    Event(Event),
    Connected(bool),
    Raise,
    AssetReady,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Page {
    Home,
    Device(String),
    Audio,
    Troubleshoot,
    Settings,
}

pub struct AppView {
    state: Option<State>,
    connected: bool,
    page: Page,
    tab: usize,
    diagnostics: Option<Diagnostics>,
    diag_running: bool,
    /// Hovered call-out on the device render.
    hot: Option<usize>,
    /// First non-empty device list seen / auto-opened the only device once.
    routed: bool,
    auto_opened: bool,
    cmds: Sender<Command>,
    eq: headset::EqEditor,
    _subs: Vec<Subscription>,
}

impl AppView {
    pub fn send(&self, c: Command) {
        let _ = self.cmds.send(c);
    }

    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.tab = 0;
        cx.notify();
    }

    fn apply(&mut self, msg: Msg, window: &mut Window, cx: &mut Context<Self>) {
        match msg {
            Msg::Event(Event::State(s)) => {
                assets::prefetch(&s, cx);
                // Land on the headset straight away when it's the only thing plugged in.
                if !self.routed && !s.devices.is_empty() {
                    self.routed = true;
                }
                if self.page == Page::Home && self.routed && !self.auto_opened && s.devices.len() == 1 && s.devices[0].link != openseries_core::devices::Link::Unsupported {
                    self.page = Page::Device(s.devices[0].key.clone());
                    self.auto_opened = true;
                }
                self.eq.sync(&s, window, cx);
                self.state = Some(*s);
            }
            Msg::Event(Event::Diagnostics(d)) => {
                self.diagnostics = Some(d);
                self.diag_running = false;
            }
            Msg::Connected(c) => self.connected = c,
            Msg::Raise => window.activate_window(),
            Msg::AssetReady => {}
        }
        cx.notify();
    }
}

pub fn run() {
    let (tx, rx) = unbounded_channel::<Msg>();
    let cmds = ipc_client(tx.clone());
    {
        let tx = tx.clone();
        std::thread::spawn(move || openseries_core::system::on_signal(crate::UI_SHOW_EVENT, || {
            let _ = tx.send(Msg::Raise);
        }));
    }
    assets::set_notifier(tx.clone());

    let app = gpui_platform::application().with_assets(gpui_kit_assets::AllAssets);
    app.run(move |cx| {
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        widgets::brand_theme(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, Size::new(px(1240.), px(820.)), cx);
        let (rgba, size) = crate::icon::app(256);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(Size::new(px(900.), px(640.))),
            titlebar: Some(TitlebarOptions { title: Some("OpenSeries".into()), appears_transparent: false, traffic_light_position: None }),
            app_id: Some("openseries".into()),
            icon: image::RgbaImage::from_raw(size, size, rgba).map(Arc::new),
            ..WindowOptions::default()
        };
        let opened = cx.open_window(options, move |window, cx| {
            let view = cx.new(|cx| {
                let eq = headset::EqEditor::new(window, cx);
                let mut subs = Vec::new();
                eq.subscribe(&mut subs, cx);
                spawn_receiver(rx, window, cx);
                AppView { state: None, connected: false, page: Page::Home, tab: 0, diagnostics: None, diag_running: false, hot: None, routed: false, auto_opened: false, cmds, eq, _subs: subs }
            });
            cx.new(|cx| Root::new(view, window, cx).bg(cx.theme().background))
        });
        if let Err(e) = opened {
            tracing::error!("could not open window: {e}");
            cx.quit();
        }
    });
}

/// Forward background messages into the view on the UI thread.
fn spawn_receiver(mut rx: UnboundedReceiver<Msg>, window: &mut Window, cx: &mut Context<AppView>) {
    let handle = window.window_handle();
    cx.spawn(async move |this, cx| {
        while let Some(msg) = rx.recv().await {
            let alive = cx.update_window(handle, |_, window, cx| this.update(cx, |v, cx| v.apply(msg, window, cx)).is_ok());
            if !matches!(alive, Ok(true)) {
                break;
            }
        }
    })
    .detach();
}

/// Keep a connection to the agent: reconnect if it restarts (e.g. after an update),
/// close the window if it's gone for good.
fn ipc_client(tx: UnboundedSender<Msg>) -> Sender<Command> {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Command>();
    let cmd_rx = Arc::new(std::sync::Mutex::new(cmd_rx));
    std::thread::spawn(move || {
        let mut misses = 0;
        loop {
            match ipc::connect() {
                Ok(stream) => {
                    misses = 0;
                    let _ = tx.send(Msg::Connected(true));
                    let (recv, mut send) = interprocess::local_socket::traits::Stream::split(stream);
                    let rx = cmd_rx.clone();
                    let writer = std::thread::spawn(move || {
                        let rx = rx.lock().unwrap();
                        while let Ok(c) = rx.recv() {
                            if ipc::send(&mut send, &c).is_err() {
                                break;
                            }
                        }
                    });
                    let ev_tx = tx.clone();
                    let _ = ipc::recv::<Event>(recv, |e| {
                        let _ = ev_tx.send(Msg::Event(e));
                    });
                    let _ = tx.send(Msg::Connected(false));
                    drop(writer);
                }
                Err(_) => {
                    misses += 1;
                    if misses == 3 {
                        // No agent at all: start one in the tray.
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = std::process::Command::new(exe).arg("--tray").spawn();
                        }
                    }
                    if misses > 20 {
                        std::process::exit(0);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    cmd_tx
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match (&self.state, self.page.clone()) {
            (None, _) => widgets::empty(cx, if self.connected { "Loading…" } else { "Starting OpenSeries…" }).into_any_element(),
            (Some(_), Page::Home) => pages::home(self, cx).into_any_element(),
            (Some(s), Page::Device(key)) => match s.devices.iter().find(|d| d.key == key).cloned() {
                Some(d) => headset::detail(self, &d, window, cx).into_any_element(),
                None => pages::gone(self, cx).into_any_element(),
            },
            (Some(_), Page::Audio) => pages::audio(self, cx).into_any_element(),
            (Some(_), Page::Troubleshoot) => pages::troubleshoot(self, cx).into_any_element(),
            (Some(_), Page::Settings) => pages::settings(self, cx).into_any_element(),
        };
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(div().flex_1().min_h_0().child(body))
            .child(footer(self, cx))
    }
}

fn footer(v: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let (dot, text) = match (&v.state, v.connected) {
        (_, false) => (widgets::GREY, "Connecting to the OpenSeries agent…".to_string()),
        (Some(s), true) if s.config.audio.lock => (widgets::GREEN, "Default devices locked · calls and meetings use your headset".into()),
        (Some(_), true) => (widgets::AMBER, "Default-device lock is off".into()),
        (None, true) => (widgets::GREY, String::new()),
    };
    let notice = v.state.as_ref().and_then(|s| s.notice.clone());
    h_flex()
        .h(px(36.))
        .px_5()
        .gap_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(widgets::dot(dot))
        .child(text)
        .when_some(notice, |this, n| this.child(div().text_color(widgets::rgb_hsla(widgets::RED)).child(n)))
        .child(div().flex_1())
        .child(
            div()
                .id("footer-version")
                .cursor_pointer()
                .hover(|s| s.text_color(cx.theme().foreground))
                .child(format!("v{}", openseries_core::VERSION))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::Settings, cx))),
        )
}
