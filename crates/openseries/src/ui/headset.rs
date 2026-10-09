//! Device detail: the product render with live call-outs (OpenLogi-style leader
//! lines), the onboard equalizer, and device settings.

use gpui::prelude::*;
use gpui::{App, ClickEvent, Entity, FontWeight, Hsla, Pixels, Point, SharedString, Subscription, Window, canvas, div, img, point, px};
use gpui_component::progress::Progress;
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::switch::Switch;
use gpui_component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit_assets::IconName as Lucide;
use openseries_core::devices::{Device, EQ_BANDS, EQ_PRESETS, Link, Setting};
use openseries_core::ipc::{Command, State};

use super::widgets::{self, rgb_hsla};
use super::{AppView, assets};

const TABS: [&str; 3] = ["Overview", "Equalizer", "Settings"];

pub fn detail(v: &mut AppView, d: &Device, window: &mut Window, cx: &mut Context<AppView>) -> impl IntoElement {
    let tabs = tab_strip(v, d, cx);
    let status = match d.link {
        Link::Online => (widgets::GREEN, "Connected".to_string()),
        Link::Asleep => (widgets::GREY, "Headset off".to_string()),
        Link::NotResponding => (widgets::RED, "Not responding".to_string()),
        Link::Unsupported => (widgets::GREY, "Detected".to_string()),
    };
    let right = widgets::pill(cx, status.0, status.1).into_any_element();
    let body = match (d.link, v.tab) {
        (Link::Unsupported, _) => unsupported(d, cx).into_any_element(),
        (_, 0) => overview(v, d, cx).into_any_element(),
        (_, 1) => equalizer(v, d, window, cx).into_any_element(),
        _ => settings(v, d, cx).into_any_element(),
    };
    v_flex()
        .size_full()
        .child(widgets::header(v, cx, d.name.clone(), true, tabs, Some(right)))
        .child(div().flex_1().min_h_0().child(body))
}

fn tab_strip(v: &AppView, d: &Device, cx: &mut Context<AppView>) -> Option<gpui::AnyElement> {
    if d.link == Link::Unsupported {
        return None;
    }
    Some(
        h_flex()
            .p_1()
            .gap_1()
            .rounded(px(10.))
            .bg(cx.theme().muted.opacity(0.5))
            .children(TABS.iter().enumerate().map(|(i, name)| {
                let on = v.tab == i;
                div()
                    .id(("tab", i))
                    .px_4()
                    .py_1p5()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .text_sm()
                    .when(on, |s| s.bg(cx.theme().background).text_color(cx.theme().foreground).font_weight(FontWeight::MEDIUM))
                    .when(!on, |s| s.text_color(cx.theme().muted_foreground).hover(|s| s.text_color(cx.theme().foreground)))
                    .child(*name)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.tab = i;
                        cx.notify();
                    }))
            }))
            .into_any_element(),
    )
}

// ---------- Overview: render + call-outs ----------

/// Stage geometry (logical px). The render is 336×500; hotspots are fractions of it.
const STAGE_W: f32 = 940.;
const STAGE_H: f32 = 500.;
const IMG_H: f32 = 470.;
const IMG_W: f32 = IMG_H * 336. / 500.;
const IMG_X: f32 = (STAGE_W - IMG_W) / 2.;
const IMG_Y: f32 = (STAGE_H - IMG_H) / 2.;
const CARD_W: f32 = 236.;
const CARD_H: f32 = 104.;
const LEFT_X: f32 = 24.;
const RIGHT_X: f32 = STAGE_W - 24. - CARD_W;

#[derive(Clone, Copy)]
struct Callout {
    /// Hotspot as a fraction of the render.
    at: (f32, f32),
    left: bool,
    /// Card top, stage coords.
    y: f32,
}

/// Points on the Arctis render: mic tip, power LED, visible (left) earcup, right yoke (ChatMix dial side).
const CALLOUTS: [Callout; 4] = [
    Callout { at: (0.336, 0.766), left: true, y: 150. },
    Callout { at: (0.214, 0.972), left: true, y: 300. },
    Callout { at: (0.86, 0.40), left: false, y: 110. },
    Callout { at: (0.64, 0.70), left: false, y: 290. },
];

fn hotspot(c: &Callout) -> (f32, f32) {
    (IMG_X + c.at.0 * IMG_W, IMG_Y + c.at.1 * IMG_H)
}

fn overview(v: &mut AppView, d: &Device, cx: &mut Context<AppView>) -> impl IntoElement {
    let online = d.link == Link::Online;
    let cfg = v.state.as_ref().and_then(|s| s.config.devices.get(&d.key)).cloned().unwrap_or_default();
    let mic_muted = v.state.as_ref().is_some_and(|s| s.audio.mic_muted);
    let hot = v.hot;

    let battery = match d.battery {
        Some(b) if online => (format!("{}%", b.percent), if b.charging { "Charging".into() } else { auto_off_text(cfg.auto_off) }),
        _ => ("—".into(), if d.link == Link::Asleep { "Headset is off".into() } else { "Unavailable".into() }),
    };
    let chat = d.chatmix.filter(|_| online);
    let eq_name = cfg.eq_preset.clone().unwrap_or_else(|| "Not set".into());
    let sidetone = cfg.sidetone.map_or("Not set", |s| ["Off", "Low", "Medium", "High"][s.min(3) as usize]);

    let cards: [gpui::AnyElement; 4] = [
        callout_card(cx, 0, hot, "Microphone", if mic_muted { "Muted" } else { "Live" }, if mic_muted { widgets::RED } else { widgets::GREEN },
            "Click to toggle", Lucide::Mic, Some(Action::ToggleMic(!mic_muted))),
        callout_card(cx, 1, hot, "Battery", &battery.0, d.battery.map_or(widgets::GREY, |_| batt_colour(d)), &battery.1, Lucide::BatteryMedium, Some(Action::Tab(2))),
        chatmix_card(cx, 2, hot, chat),
        callout_card(cx, 3, hot, "Sound", &eq_name, widgets::ORANGE, &format!("Sidetone: {sidetone}"), Lucide::AudioLines, Some(Action::Tab(1))),
    ];

    let image = assets::image_for(d);
    let lines = canvas(|_, _, _| (), move |bounds, (), window, _| {
        let o = bounds.origin;
        for (i, c) in CALLOUTS.iter().enumerate() {
            let (hx, hy) = hotspot(c);
            let lit = hot == Some(i);
            let colour: Hsla = if lit { widgets::accent() } else { gpui::hsla(0., 0., 0.62, 0.45) };
            let (stub_x, anchor_x) = if c.left { (IMG_X - 12., LEFT_X + CARD_W) } else { (IMG_X + IMG_W + 12., RIGHT_X) };
            // Keep the stub on the hotspot's own side of the render.
            let stub_x = if c.left { stub_x.min(hx - 8.) } else { stub_x.max(hx + 8.) };
            let mut path = gpui::PathBuilder::stroke(px(if lit { 2. } else { 1. }));
            path.move_to(o + pt(hx, hy));
            path.line_to(o + pt(stub_x, hy));
            path.line_to(o + pt(anchor_x, c.y + CARD_H / 2.));
            if let Ok(p) = path.build() {
                window.paint_path(p, colour);
            }
            let r = px(if lit { 6. } else { 5. });
            let b = gpui::Bounds::centered_at(o + pt(hx, hy), gpui::size(r * 2., r * 2.));
            window.paint_quad(gpui::fill(b, gpui::hsla(0., 0., 0.08, 0.9)).corner_radii(r).border_widths(px(2.)).border_color(colour));
        }
    });

    div().size_full().flex().items_center().justify_center().child(
        div()
            .relative()
            .w(px(STAGE_W))
            .h(px(STAGE_H))
            // Soft glow behind the product, like a studio shot.
            .child(canvas(|_, _, _| (), |bounds, (), window, _| paint_glow(bounds, window)).absolute().top_0().left_0().size_full())
            .child(div().absolute().left(px(IMG_X)).top(px(IMG_Y)).w(px(IMG_W)).h(px(IMG_H)).child(match image {
                Some(p) => img(p).size_full().into_any_element(),
                None => placeholder(cx).into_any_element(),
            }))
            .child(lines.absolute().top_0().left_0().size_full())
            .children(cards.into_iter().zip(CALLOUTS).map(|(card, c)| {
                div().absolute().left(px(if c.left { LEFT_X } else { RIGHT_X })).top(px(c.y)).w(px(CARD_W)).h(px(CARD_H)).child(card)
            })),
    )
}

/// Soft radial glow behind the render: stacked translucent ellipses.
fn paint_glow(bounds: gpui::Bounds<Pixels>, window: &mut Window) {
    let c = bounds.origin + pt(IMG_X + IMG_W / 2., IMG_Y + IMG_H * 0.55);
    for i in 0..14 {
        let k = 1. - i as f32 / 14.;
        let (w, h) = (px(IMG_W * 1.5 * k + 40.), px(IMG_H * 0.95 * k + 20.));
        let b = gpui::Bounds::centered_at(c, gpui::size(w, h));
        window.paint_quad(gpui::fill(b, gpui::hsla(0.04, 0.9, 0.55, 0.012)).corner_radii(h / 2.));
    }
}

fn pt(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

#[derive(Clone, Copy)]
enum Action {
    ToggleMic(bool),
    Tab(usize),
}

#[allow(clippy::too_many_arguments)]
fn callout_card(
    cx: &mut Context<AppView>,
    i: usize,
    hot: Option<usize>,
    title: &str,
    value: &str,
    colour: u32,
    sub: &str,
    icon: Lucide,
    action: Option<Action>,
) -> gpui::AnyElement {
    card_shell(cx, i, hot, action)
        .child(h_flex().gap_2().text_xs().text_color(cx.theme().muted_foreground).child(Icon::new(icon).size_3p5()).child(title.to_string()))
        .child(h_flex().gap_2().child(widgets::dot(colour)).child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(value.to_string())))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(sub.to_string()))
        .into_any_element()
}

fn chatmix_card(cx: &mut Context<AppView>, i: usize, hot: Option<usize>, c: Option<openseries_core::devices::ChatMix>) -> gpui::AnyElement {
    let (game, chat) = c.map_or((0., 0.), |c| (f32::from(c.game), f32::from(c.chat)));
    let row = |label: &str, value: f32, colour: u32, cx: &App| {
        h_flex()
            .gap_2()
            .text_xs()
            .child(div().w(px(34.)).text_color(cx.theme().muted_foreground).child(label.to_string()))
            .child(div().flex_1().child(Progress::new(label.to_string()).value(value).color(rgb_hsla(colour))))
            .child(div().w(px(34.)).text_right().child(if c.is_some() { format!("{value:.0}%") } else { "—".into() }))
    };
    card_shell(cx, i, hot, Some(Action::Tab(2)))
        .child(h_flex().gap_2().text_xs().text_color(cx.theme().muted_foreground).child(Icon::new(Lucide::SlidersHorizontal).size_3p5()).child("ChatMix · dial on right earcup"))
        .child(row("Game", game, widgets::ORANGE, cx))
        .child(row("Chat", chat, widgets::BLUE, cx))
        .into_any_element()
}

fn card_shell(cx: &mut Context<AppView>, i: usize, hot: Option<usize>, action: Option<Action>) -> gpui::Stateful<gpui::Div> {
    let lit = hot == Some(i);
    v_flex()
        .id(("callout", i))
        .size_full()
        .p_3()
        .gap_1p5()
        .justify_center()
        .rounded(px(10.))
        .bg(cx.theme().group_box)
        .border_1()
        .border_color(if lit { widgets::accent() } else { cx.theme().border.opacity(0.6) })
        .when(action.is_some(), |s| s.cursor_pointer())
        .on_hover(cx.listener(move |this, h: &bool, _, cx| {
            this.hot = if *h { Some(i) } else { this.hot.filter(|x| *x != i) };
            cx.notify();
        }))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| match action {
            Some(Action::ToggleMic(m)) => this.send(Command::SetMicMuted(m)),
            Some(Action::Tab(t)) => {
                this.tab = t;
                cx.notify();
            }
            None => {}
        }))
}

fn placeholder(cx: &App) -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(Lucide::Headphones).size_16())
        .child(div().text_xs().child("Loading product image…"))
}

fn batt_colour(d: &Device) -> u32 {
    let [r, g, b] = crate::icon::battery_rgb(d);
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

fn auto_off_text(m: Option<u8>) -> String {
    match m {
        Some(0) => "Never turns off".into(),
        Some(m) => format!("Turns off after {m} min idle"),
        None => "On battery".into(),
    }
}

fn unsupported(d: &Device, cx: &App) -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_4()
        .child(div().w(px(320.)).h(px(320.)).children(assets::image_for(d).map(|p| img(p).size_full())))
        .child(div().text_lg().child(d.name.clone()))
        .child(
            div()
                .max_w(px(460.))
                .text_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("OpenSeries sees this device but doesn't speak its protocol yet. Support is added per model; open an issue on GitHub with the model name to help."),
        )
}

// ---------- Equalizer ----------

pub struct EqEditor {
    sliders: Vec<Entity<SliderState>>,
    /// What the sliders show right now (dB).
    values: [f32; 10],
    synced: Option<Vec<f32>>,
}

impl EqEditor {
    pub fn new(_window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let sliders = (0..10).map(|_| cx.new(|_| SliderState::new().max(12.).min(-12.).step(0.5).default_value(0.))).collect();
        Self { sliders, values: [0.; 10], synced: None }
    }

    pub fn subscribe(&self, subs: &mut Vec<Subscription>, cx: &mut Context<AppView>) {
        for (i, s) in self.sliders.iter().enumerate() {
            subs.push(cx.subscribe(s, move |this: &mut AppView, _, ev: &SliderEvent, cx| match ev {
                SliderEvent::Change(v) => {
                    this.eq.values[i] = v.start();
                    cx.notify();
                }
                // Each write lands in the headset's flash, so only commit on release.
                SliderEvent::Release(v) => {
                    this.eq.values[i] = v.start();
                    this.commit_eq(this.eq.values.to_vec());
                }
            }));
        }
    }

    /// Mirror the saved EQ of the device being shown.
    pub fn sync(&mut self, s: &State, window: &mut Window, cx: &mut Context<AppView>) {
        let saved = s.devices.iter().find_map(|d| s.config.devices.get(&d.key).and_then(|c| c.eq.clone()));
        if saved == self.synced {
            return;
        }
        if let Some(bands) = &saved {
            self.set(bands, window, cx);
        }
        self.synced = saved;
    }

    fn set(&mut self, bands: &[f32], window: &mut Window, cx: &mut Context<AppView>) {
        for (i, (s, v)) in self.sliders.iter().zip(bands).enumerate() {
            self.values[i] = *v;
            s.update(cx, |s, cx| s.set_value(*v, window, cx));
        }
    }
}

impl AppView {
    fn current_device_key(&self) -> Option<String> {
        match &self.page {
            super::Page::Device(k) => Some(k.clone()),
            _ => None,
        }
    }

    fn commit_eq(&mut self, bands: Vec<f32>) {
        if let Some(device) = self.current_device_key() {
            self.send(Command::Apply { device, setting: Setting::Equalizer(bands) });
        }
    }
}

fn equalizer(v: &mut AppView, d: &Device, _window: &mut Window, cx: &mut Context<AppView>) -> impl IntoElement {
    let online = d.link == Link::Online;
    let preset = v.state.as_ref().and_then(|s| s.config.devices.get(&d.key)).and_then(|c| c.eq_preset.clone());
    let selected = EQ_PRESETS.iter().position(|(n, _)| Some(*n) == preset.as_deref());
    let names: Vec<&str> = EQ_PRESETS.iter().map(|(n, _)| *n).collect();
    let view = cx.entity();
    let presets = widgets::segmented("eq-preset", &names, selected, move |i, window, cx| {
        view.update(cx, |this, cx| {
            let bands = EQ_PRESETS[i].1.to_vec();
            this.eq.set(&bands, window, cx);
            this.commit_eq(bands);
            cx.notify();
        });
    });
    let values = v.eq.values;
    let curve = canvas(move |_, _, _| (), move |bounds, (), window, _| paint_curve(bounds, &values, window));

    scroll_page(
        v_flex()
            .gap_4()
            .child(offline_banner(online, cx))
            .child(
                widgets::card(cx)
                    .child(widgets::card_title(cx, "Equalizer", Some("Stored on the headset itself, so it works on any PC or console. ±12 dB in 0.5 dB steps.")))
                    .child(presets)
                    .child(div().h(px(130.)).w_full().rounded(px(8.)).bg(cx.theme().background.opacity(0.6)).child(curve.size_full()))
                    .child(h_flex().w_full().gap_1().children(v.eq.sliders.iter().enumerate().map(|(i, s)| {
                        v_flex()
                            .flex_1()
                            .items_center()
                            .gap_2()
                            .child(div().text_xs().font_weight(FontWeight::MEDIUM).child(format!("{:+.1}", values[i])))
                            .child(Slider::new(s).vertical().h(px(190.)))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(SharedString::new_static(EQ_BANDS[i])))
                    }))),
            ),
    )
}

/// Smooth response curve through the ten band values.
fn paint_curve(bounds: gpui::Bounds<Pixels>, values: &[f32; 10], window: &mut Window) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let o = bounds.origin;
    let y_of = |db: f32| h / 2. - db / 12. * (h / 2. - 14.);
    let x_of = |i: f32| 24. + i * (w - 48.) / 9.;
    // Zero line.
    let mut zero = gpui::PathBuilder::stroke(px(1.)).dash_array(&[px(4.), px(4.)]);
    zero.move_to(o + pt(12., h / 2.));
    zero.line_to(o + pt(w - 12., h / 2.));
    if let Ok(p) = zero.build() {
        window.paint_path(p, gpui::hsla(0., 0., 0.6, 0.25));
    }
    // Catmull-Rom through the points, sampled.
    let pts: Vec<(f32, f32)> = values.iter().enumerate().map(|(i, v)| (x_of(i as f32), y_of(*v))).collect();
    let get = |i: isize| pts[i.clamp(0, 9) as usize];
    let mut path = gpui::PathBuilder::stroke(px(2.5));
    path.move_to(o + pt(pts[0].0, pts[0].1));
    for i in 0..9isize {
        let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        for s in 1..=12 {
            let t = s as f32 / 12.;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2. * b + (-a + c) * t + (2. * a - 5. * b + 4. * c - d) * t2 + (-a + 3. * b - 3. * c + d) * t3);
            path.line_to(o + pt(f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1)));
        }
    }
    if let Ok(p) = path.build() {
        window.paint_path(p, widgets::accent());
    }
    for (x, y) in pts {
        let r = px(3.5);
        window.paint_quad(gpui::fill(gpui::Bounds::centered_at(o + pt(x, y), gpui::size(r * 2., r * 2.)), widgets::accent()).corner_radii(r));
    }
}

// ---------- Settings ----------

fn settings(v: &mut AppView, d: &Device, cx: &mut Context<AppView>) -> impl IntoElement {
    let online = d.link == Link::Online;
    let st = v.state.clone().unwrap_or_default();
    let cfg = st.config.devices.get(&d.key).cloned().unwrap_or_default();
    let key = d.key.clone();
    let view = cx.entity();

    let sidetone = {
        let (view, key) = (view.clone(), key.clone());
        widgets::segmented("sidetone", &["Off", "Low", "Medium", "High"], cfg.sidetone.map(usize::from), move |i, _, cx| {
            view.read(cx).send(Command::Apply { device: key.clone(), setting: Setting::Sidetone(i as u8) });
        })
    };
    const OFF: [u8; 5] = [0, 10, 30, 60, 90];
    let auto_off = {
        let (view, key) = (view.clone(), key.clone());
        let sel = cfg.auto_off.and_then(|m| OFF.iter().position(|x| *x == m));
        widgets::segmented("auto-off", &["Never", "10 min", "30 min", "1 hour", "90 min"], sel, move |i, _, cx| {
            view.read(cx).send(Command::Apply { device: key.clone(), setting: Setting::AutoOff(OFF[i]) });
        })
    };

    scroll_page(
        v_flex()
            .gap_4()
            .child(offline_banner(online, cx))
            .child(widgets::card(cx).child(widgets::card_title(cx, "Sidetone", Some("Hear your own voice in the headset while you talk."))).child(sidetone))
            .child(widgets::card(cx).child(widgets::card_title(cx, "Auto power-off", Some("Turn the headset off after it sits idle."))).child(auto_off))
            .child(
                widgets::card(cx)
                    .child(
                        h_flex()
                            .gap_4()
                            .child(div().flex_1().min_w_0().child(widgets::card_title(
                                cx,
                                "ChatMix",
                                Some("The dial on the right earcup balances game and chat. OpenSeries applies it per app: chat apps follow Chat, everything else follows Game. No Sonar needed."),
                            )))
                            .child(Switch::new("chatmix").checked(st.config.chatmix.enabled).on_click(cx.listener(|this, on: &bool, _, _| this.send(Command::SetChatMix(*on))))),
                    )
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("Chat apps: {}", st.config.chatmix.chat_apps.join(", ")))),
            ),
    )
}

fn offline_banner(online: bool, cx: &App) -> impl IntoElement {
    div().when(!online, |s| {
        s.child(
            h_flex()
                .gap_2()
                .p_3()
                .rounded(px(8.))
                .bg(rgb_hsla(widgets::AMBER).opacity(0.12))
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(Icon::new(gpui_component::IconName::TriangleAlert).text_color(rgb_hsla(widgets::AMBER)))
                .child("Turn the headset on to change its settings."),
        )
    })
}

pub fn scroll_page(content: impl IntoElement) -> impl IntoElement {
    use gpui_component::scroll::ScrollableElement as _;
    v_flex().size_full().overflow_y_scrollbar().child(div().w_full().flex().justify_center().py_6().px_6().child(div().w_full().max_w(px(860.)).child(content)))
}
