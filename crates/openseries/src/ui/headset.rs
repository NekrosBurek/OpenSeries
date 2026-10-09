//! Device detail: the product render with live call-outs (OpenLogi-style leader
//! lines), the onboard equalizer, and device settings.

use gpui::prelude::*;
use gpui::{App, ClickEvent, Entity, FontWeight, Hsla, Pixels, Point, SharedString, Subscription, Window, canvas, div, img, point, px};
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::switch::Switch;
use gpui_component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit_assets::IconName as Lucide;
use openseries_core::devices::{Device, EQ_ALL_PRESETS, EQ_BANDS, Link, Setting};
use openseries_core::ipc::{Command, State};

use super::widgets::{self, rgb_hsla};
use super::{AppView, assets};

const TABS: [&str; 4] = ["Overview", "Equalizer", "Profiles", "Settings"];

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
        (_, 2) => profiles(v, cx).into_any_element(),
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

/// Stage geometry (logical px). The render is fitted to `IMG_H`; hotspots are fractions of it.
const STAGE_W: f32 = 980.;
const STAGE_H: f32 = 520.;
const IMG_H: f32 = 500.;
const CARD_W: f32 = 236.;
const CARD_H: f32 = 104.;
const LEFT_X: f32 = 16.;
const RIGHT_X: f32 = STAGE_W - 16. - CARD_W;

/// Card slots: microphone, battery (left); ChatMix, sound, receiver (right).
const SLOTS: [(bool, f32); 5] = [(true, 150.), (true, 300.), (false, 70.), (false, 208.), (false, 346.)];

/// A product render and where its parts are, as fractions of the image.
#[derive(Clone, Copy)]
struct Art {
    aspect: f32,
    spots: [Option<(f32, f32)>; 5],
}

impl Art {
    fn w(&self) -> f32 {
        IMG_H * self.aspect
    }
    fn x(&self) -> f32 {
        (STAGE_W - self.w()) / 2.
    }
    fn y(&self) -> f32 {
        (STAGE_H - IMG_H) / 2.
    }
    fn spot(&self, i: usize) -> Option<(f32, f32)> {
        self.spots[i].map(|(fx, fy)| (self.x() + fx * self.w(), self.y() + fy * IMG_H))
    }
}

/// Calibrated renders get leader lines to their parts; anything else just shows the cards.
fn art_for(path: Option<&std::path::Path>) -> Art {
    let hd = path.is_some_and(|p| p.parent().and_then(|d| d.file_name()).is_some_and(|n| n == "hd"));
    let stem = path.and_then(|p| p.file_stem()).and_then(|s| s.to_str()).unwrap_or("");
    match (stem, hd) {
        // HD Arctis 7+ (956×1400): mic tip, power LED, ChatMix-side yoke, earcup, USB-C receiver.
        ("arctis_7", true) => Art { aspect: 956. / 1400., spots: [Some((0.318, 0.693)), Some((0.245, 0.90)), Some((0.915, 0.55)), Some((0.63, 0.64)), Some((0.617, 0.91))] },
        // GG's 336×500 render has no receiver in shot.
        ("arctis_7", false) => Art { aspect: 336. / 500., spots: [Some((0.336, 0.766)), Some((0.214, 0.972)), Some((0.86, 0.40)), Some((0.64, 0.70)), None] },
        _ => Art { aspect: 0.75, spots: [None; 5] },
    }
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
    let st = v.state.clone().unwrap_or_default();
    let profile = st.config.profiles.get(st.active_profile).map_or("Default".to_string(), |p| p.name.clone());
    let sound_sub = match &st.playing {
        Some(g) => format!("Playing {}", openseries_core::games::display_name(&g.exe).unwrap_or(&g.title)),
        None => "No game running".into(),
    };
    let sidetone = cfg.sidetone.map_or("Not set", |s| ["Off", "Low", "Medium", "High"][s.min(3) as usize]);
    let (rx_value, rx_colour, rx_sub) = match d.link {
        Link::Online => ("Linked", widgets::GREEN, "2.4 GHz wireless · USB-C receiver"),
        Link::Asleep => ("Waiting", widgets::GREY, "Receiver on, headset off or out of range"),
        _ => ("Not responding", widgets::RED, "Try Troubleshoot → Restart receiver"),
    };

    let cards: [gpui::AnyElement; 5] = [
        callout_card(cx, 0, hot, "Microphone", if mic_muted { "Muted" } else { "Live" }, if mic_muted { widgets::RED } else { widgets::GREEN },
            "Click to toggle", Lucide::Mic, Some(Action::ToggleMic(!mic_muted))),
        callout_card(cx, 1, hot, "Battery", &battery.0, d.battery.map_or(widgets::GREY, |_| batt_colour(d)), &battery.1, Lucide::BatteryMedium, Some(Action::Tab(3))),
        chatmix_card(cx, 2, hot, chat),
        callout_card(cx, 3, hot, "Sound profile", &profile, widgets::ORANGE, &format!("{sound_sub} · sidetone {sidetone}"), Lucide::AudioLines, Some(Action::Tab(2))),
        callout_card(cx, 4, hot, "Receiver", rx_value, rx_colour, rx_sub, Lucide::Usb, None),
    ];

    let image = assets::image_for(d);
    let art = art_for(image.as_deref());
    let lines = canvas(|_, _, _| (), move |bounds, (), window, _| {
        let o = bounds.origin;
        for (i, (left, y)) in SLOTS.iter().enumerate() {
            let Some((hx, hy)) = art.spot(i) else { continue };
            let lit = hot == Some(i);
            let colour: Hsla = if lit { widgets::accent() } else { gpui::hsla(0., 0., 0.62, 0.45) };
            let (stub_x, anchor_x) = if *left { (art.x() - 12., LEFT_X + CARD_W) } else { (art.x() + art.w() + 12., RIGHT_X) };
            // Keep the stub on the hotspot's own side of the render.
            let stub_x = if *left { stub_x.min(hx - 8.) } else { stub_x.max(hx + 8.) };
            let mut path = gpui::PathBuilder::stroke(px(if lit { 2. } else { 1. }));
            path.move_to(o + pt(hx, hy));
            path.line_to(o + pt(stub_x, hy));
            path.line_to(o + pt(anchor_x, y + CARD_H / 2.));
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
            .child(canvas(|_, _, _| (), move |bounds, (), window, _| paint_glow(bounds, &art, window)).absolute().top_0().left_0().size_full())
            .child(div().absolute().left(px(art.x())).top(px(art.y())).w(px(art.w())).h(px(IMG_H)).child(match image {
                Some(p) => img(p).size_full().object_fit(gpui::ObjectFit::Contain).into_any_element(),
                None => placeholder(cx).into_any_element(),
            }))
            .child(lines.absolute().top_0().left_0().size_full())
            .children(cards.into_iter().zip(SLOTS).map(|(card, (left, y))| {
                div().absolute().left(px(if left { LEFT_X } else { RIGHT_X })).top(px(y)).w(px(CARD_W)).h(px(CARD_H)).child(card)
            })),
    )
}

/// Soft radial glow behind the render: stacked translucent ellipses.
fn paint_glow(bounds: gpui::Bounds<Pixels>, art: &Art, window: &mut Window) {
    let c = bounds.origin + pt(art.x() + art.w() / 2., art.y() + IMG_H * 0.55);
    for i in 0..14 {
        let k = 1. - i as f32 / 14.;
        let (w, h) = (px(art.w() * 1.5 * k + 40.), px(IMG_H * 0.95 * k + 20.));
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
    let shell = card_shell(cx, i, hot, Some(Action::Tab(3)));
    let line = widgets::chatmix_line(cx, c, 22.);
    shell
        .child(h_flex().gap_2().text_xs().text_color(cx.theme().muted_foreground).child(Icon::new(Lucide::SlidersHorizontal).size_3p5()).child("ChatMix · right earcup dial"))
        .child(line)
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
    /// Profile whose EQ the sliders edit.
    pub editing: usize,
}

impl EqEditor {
    pub fn new(_window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let sliders = (0..10).map(|_| cx.new(|_| SliderState::new().max(12.).min(-12.).step(0.5).default_value(0.))).collect();
        Self { sliders, values: [0.; 10], synced: None, editing: 0 }
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

    /// Mirror the EQ of the profile being edited.
    pub fn sync(&mut self, s: &State, window: &mut Window, cx: &mut Context<AppView>) {
        let saved = s.config.profiles.get(self.editing).map(|p| p.eq.clone());
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
    fn commit_eq(&mut self, bands: Vec<f32>) {
        // Edits belong to a profile; the agent pushes them to the headset while it's active.
        self.eq.synced = Some(bands.clone());
        self.send(Command::SetProfileEq { profile: self.eq.editing, bands });
    }

    fn edit_profile(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.eq.editing = i;
        self.eq.synced = None;
        if let Some(s) = self.state.clone() {
            self.eq.sync(&s, window, cx);
        }
        cx.notify();
    }
}

fn equalizer(v: &mut AppView, d: &Device, _window: &mut Window, cx: &mut Context<AppView>) -> impl IntoElement {
    let online = d.link == Link::Online;
    let st = v.state.clone().unwrap_or_default();
    let editing = v.eq.editing.min(st.config.profiles.len().saturating_sub(1));
    let values = v.eq.values;
    let preset = EQ_ALL_PRESETS.iter().position(|(_, b)| b.as_slice() == values.as_slice());
    let names: Vec<&str> = EQ_ALL_PRESETS.iter().map(|(n, _)| *n).collect();
    let profile_names: Vec<String> =
        st.config.profiles.iter().enumerate().map(|(i, p)| if i == st.active_profile { format!("{} ●", p.name) } else { p.name.clone() }).collect();
    let profile_refs: Vec<&str> = profile_names.iter().map(String::as_str).collect();

    let view = cx.entity();
    let pick_profile = widgets::segmented("eq-profile", &profile_refs, Some(editing), move |i, window, cx| {
        view.update(cx, |this, cx| this.edit_profile(i, window, cx));
    });
    let view = cx.entity();
    let presets = widgets::segmented("eq-preset", &names, preset, move |i, window, cx| {
        view.update(cx, |this, cx| {
            let bands = EQ_ALL_PRESETS[i].1.to_vec();
            this.eq.set(&bands, window, cx);
            this.commit_eq(bands);
            cx.notify();
        });
    });
    let curve = canvas(move |_, _, _| (), move |bounds, (), window, _| paint_curve(bounds, &values, window));
    let applies = if editing == st.active_profile {
        "Active now (●): changes go straight to the headset."
    } else {
        "Not active: saved, and applied when this profile switches on."
    };

    scroll_page(
        v_flex()
            .gap_4()
            .child(offline_banner(online, cx))
            .child(widgets::card(cx).child(widgets::card_title(cx, "Editing profile", Some(applies))).child(pick_profile))
            .child(
                widgets::card(cx)
                    .child(widgets::card_title(
                        cx,
                        "Equalizer",
                        Some("Stored on the headset itself, so it also works on consoles. ±12 dB in 0.5 dB steps, written when you let go of a slider."),
                    ))
                    .child(presets)
                    .child(div().h(px(150.)).w_full().rounded(px(8.)).bg(cx.theme().background.opacity(0.6)).child(curve.size_full()))
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
    // Faint band columns so the dots read as positions on a grid.
    for (x, _) in &pts {
        let mut g = gpui::PathBuilder::stroke(px(1.));
        g.move_to(o + pt(*x, 10.));
        g.line_to(o + pt(*x, h - 10.));
        if let Ok(p) = g.build() {
            window.paint_path(p, gpui::hsla(0., 0., 0.6, 0.08));
        }
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
                    .child(widgets::chatmix_line(cx, d.chatmix.filter(|_| online), 28.))
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

// ---------- Profiles ----------

fn profile_icon(kind: &str) -> Lucide {
    match kind {
        "competitive" => Lucide::Crosshair,
        "immersive" => Lucide::Clapperboard,
        "voice" => Lucide::MessageCircle,
        _ => Lucide::Headphones,
    }
}

fn profiles(v: &mut AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    use gpui_component::button::{Button, ButtonVariants as _};
    let st = v.state.clone().unwrap_or_default();
    let active_name = st.config.profiles.get(st.active_profile).map_or("", |p| p.name.as_str()).to_string();
    let now = match &st.playing {
        Some(g) => format!("Now playing {} · using {active_name}", g.title),
        None => format!("No game running · using {active_name}"),
    };
    let auto = widgets::card(cx)
        .child(
            h_flex()
                .gap_4()
                .child(div().flex_1().min_w_0().child(widgets::card_title(
                    cx,
                    "Switch automatically",
                    Some("Detects games from Steam, Epic, Riot, GOG, Xbox, EA, Ubisoft and Battle.net, and keeps their profile until the game closes (alt-tabbing doesn't switch back)."),
                )))
                .child(Switch::new("auto-switch").checked(st.config.auto_switch).on_click(cx.listener(|this, on: &bool, _, _| this.send(Command::SetAutoSwitch(*on))))),
        )
        .child(h_flex().gap_2().text_sm().child(widgets::dot(if st.playing.is_some() { widgets::GREEN } else { widgets::GREY })).child(now));

    let mut cards = Vec::new();
    for (i, p) in st.config.profiles.iter().enumerate() {
        cards.push(profile_card(i, p, i == st.active_profile, cx));
    }

    let mut rows = Vec::new();
    for (gi, g) in st.recent_games.iter().enumerate() {
        let current = st.config.profiles.iter().position(|p| p.apps.iter().any(|a| a.eq_ignore_ascii_case(&g.exe)));
        let mut buttons = Vec::new();
        for (pi, p) in st.config.profiles.iter().enumerate().skip(1) {
            let exe = g.exe.clone();
            let b = Button::new(("assign", gi * 16 + pi)).small().label(p.name.clone());
            let b = if current == Some(pi) { b.primary() } else { b.outline() };
            buttons.push(b.on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.send(Command::AddProfileApp { profile: pi, exe: exe.clone() }))));
        }
        rows.push(
            h_flex()
                .gap_3()
                .py_1()
                .child(Icon::new(Lucide::Gamepad2).text_color(cx.theme().muted_foreground))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().truncate().child(openseries_core::games::display_name(&g.exe).map_or(g.title.clone(), String::from)))
                        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{}.exe", g.exe))),
                )
                .children(buttons)
                .into_any_element(),
        );
    }
    if rows.is_empty() {
        rows.push(div().text_sm().text_color(cx.theme().muted_foreground).child("Start a game and it shows up here.").into_any_element());
    }
    let recent = widgets::card(cx)
        .child(widgets::card_title(cx, "Recently detected games", Some("Assign a game to a profile and it switches whenever that game runs.")))
        .children(rows);

    scroll_page(v_flex().gap_4().child(auto).child(h_flex().flex_wrap().items_start().gap_4().children(cards)).child(recent))
}

fn profile_card(i: usize, p: &openseries_core::config::Profile, active: bool, cx: &mut Context<AppView>) -> gpui::AnyElement {
    use gpui_component::button::{Button, ButtonVariants as _};
    let mut eq = [0f32; 10];
    for (k, v) in p.eq.iter().take(10).enumerate() {
        eq[k] = *v;
    }
    let curve = canvas(move |_, _, _| (), move |bounds, (), window, _| paint_curve(bounds, &eq, window));
    let mut chips = Vec::new();
    const SHOWN: usize = 8;
    for (ai, a) in p.apps.iter().enumerate().take(SHOWN) {
        let exe = a.clone();
        chips.push(
            div()
                .id(("app", i * 64 + ai))
                .px_2()
                .py_0p5()
                .rounded(px(6.))
                .bg(cx.theme().muted)
                .text_xs()
                .cursor_pointer()
                .hover(|s| s.bg(rgb_hsla(widgets::RED).opacity(0.3)))
                .child(format!("{}  ×", openseries_core::games::display_name(a).unwrap_or(a)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.send(Command::RemoveProfileApp { profile: i, exe: exe.clone() }))),
        );
    }
    let empty_hint = if i == 0 { "Used when nothing else matches" } else { "No games assigned yet" };
    v_flex()
        .w(px(410.))
        .p_4()
        .gap_3()
        .rounded(px(12.))
        .bg(cx.theme().group_box)
        .border_1()
        .border_color(if active { widgets::accent() } else { cx.theme().border.opacity(0.6) })
        .child(
            h_flex()
                .gap_2()
                .child(Icon::new(profile_icon(&p.icon)).text_color(if active { widgets::accent() } else { cx.theme().muted_foreground }))
                .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(p.name.clone()))
                .when(active, |r| r.child(gpui_component::tag::Tag::primary().child("Active")))
                .child(div().flex_1())
                .when(!active, |r| {
                    r.child(Button::new(("activate", i)).small().outline().label("Use now").on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.send(Command::ActivateProfile(i)))))
                })
                .child(Button::new(("edit-eq", i)).small().ghost().label("Edit EQ").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.tab = 1;
                    this.edit_profile(i, window, cx);
                }))),
        )
        .child(div().h(px(64.)).w_full().rounded(px(8.)).bg(cx.theme().background.opacity(0.6)).child(curve.size_full()))
        .child(
            h_flex()
                .flex_wrap()
                .gap_1()
                .when(p.all_games, |r| r.child(gpui_component::tag::Tag::secondary().child("Any other game")))
                .when(p.apps.is_empty() && !p.all_games, |r| r.child(div().text_xs().text_color(cx.theme().muted_foreground).child(empty_hint)))
                .children(chips)
                .when(p.apps.len() > SHOWN, |r| r.child(div().px_2().py_0p5().text_xs().text_color(cx.theme().muted_foreground).child(format!("+{} more", p.apps.len() - SHOWN)))),
        )
        .into_any_element()
}
