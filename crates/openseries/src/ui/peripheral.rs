//! Mouse and keyboard page: the render on the left, controls on the right.

use gpui::prelude::*;
use gpui::{ClickEvent, Entity, FontWeight, Subscription, Window, div, img, px};
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit_assets::IconName as Lucide;
use openseries_core::devices::{Device, Kind, Link, Setting};
use openseries_core::ipc::Command;

use super::widgets::{self, rgb_hsla};
use super::{AppView, assets};

const DEFAULT_DPI: [u16; 4] = [400, 800, 1600, 3200];
const POLLING: [u16; 4] = [125, 250, 500, 1000];
const SWATCHES: [[u8; 3]; 10] = [
    [0xff, 0x5a, 0x1f],
    [0xef, 0x44, 0x44],
    [0xf5, 0x9e, 0x0b],
    [0xfa, 0xcc, 0x15],
    [0x22, 0xc5, 0x5e],
    [0x06, 0xb6, 0xd4],
    [0x3b, 0x82, 0xf6],
    [0x8b, 0x5c, 0xf6],
    [0xec, 0x48, 0x99],
    [0xff, 0xff, 0xff],
];

/// DPI stage editor: chips pick a stage, one slider edits it, release writes all stages.
pub struct DpiEditor {
    slider: Entity<SliderState>,
    stages: Vec<u16>,
    selected: usize,
    /// Device key + config the stages were last loaded from.
    loaded: Option<(String, Option<Vec<u16>>)>,
}

impl DpiEditor {
    pub fn new(cx: &mut Context<AppView>) -> Self {
        let slider = cx.new(|_| SliderState::new().max(18000.).min(50.).step(50.).default_value(800.));
        Self { slider, stages: DEFAULT_DPI.to_vec(), selected: 1, loaded: None }
    }

    pub fn subscribe(&self, subs: &mut Vec<Subscription>, cx: &mut Context<AppView>) {
        subs.push(cx.subscribe(&self.slider, |this: &mut AppView, _, ev: &SliderEvent, cx| {
            let (v, commit) = match ev {
                SliderEvent::Change(v) => (v.start(), false),
                SliderEvent::Release(v) => (v.start(), true),
            };
            let i = this.dpi.selected.min(this.dpi.stages.len() - 1);
            this.dpi.stages[i] = v as u16;
            if commit {
                this.send_dpi();
            }
            cx.notify();
        }));
    }

    /// Load the saved stages when switching device or when the agent confirms a change.
    fn sync(&mut self, d: &Device, saved: Option<Vec<u16>>, window: &mut Window, cx: &mut Context<AppView>) {
        let key = (d.key.clone(), saved.clone());
        if self.loaded.as_ref() == Some(&key) {
            return;
        }
        self.loaded = Some(key);
        self.stages = saved.unwrap_or_else(|| DEFAULT_DPI.to_vec());
        self.selected = self.selected.min(self.stages.len() - 1);
        self.load_slider(window, cx);
    }

    fn load_slider(&self, window: &mut Window, cx: &mut Context<AppView>) {
        let v = f32::from(self.stages[self.selected]);
        self.slider.update(cx, |s, cx| s.set_value(v, window, cx));
    }
}

impl AppView {
    fn device_key(&self) -> Option<String> {
        match &self.page {
            super::Page::Device(k) => Some(k.clone()),
            _ => None,
        }
    }

    fn send_dpi(&mut self) {
        if let Some(device) = self.device_key() {
            self.send(Command::Apply { device, setting: Setting::Dpi(self.dpi.stages.clone()) });
        }
    }
}

pub fn detail(v: &mut AppView, d: &Device, window: &mut Window, cx: &mut Context<AppView>) -> impl IntoElement {
    let saved = v.state.as_ref().and_then(|s| s.config.devices.get(&d.key)).cloned().unwrap_or_default();
    if d.features.dpi_max > 0 {
        v.dpi.sync(d, saved.dpi.clone(), window, cx);
    }
    let (colour, status) = match (d.link, d.battery) {
        (Link::Online, Some(b)) => (if b.charging { widgets::BLUE } else { widgets::GREEN }, format!("{}%{}", b.percent, if b.charging { " · charging" } else { "" })),
        (Link::Online, None) => (widgets::GREEN, "Connected".into()),
        (Link::Asleep, _) => (widgets::GREY, "Asleep".into()),
        (Link::NotResponding, _) => (widgets::RED, "Not responding".into()),
        (Link::Unsupported, _) => (widgets::GREY, "Detected".into()),
    };
    let pill = widgets::pill(cx, colour, status).into_any_element();

    let image = assets::image_for(d);
    let art = div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .p_8()
        .children(image.map(|p| img(p).max_w_full().max_h_full().object_fit(gpui::ObjectFit::Contain)));

    let mut cards: Vec<gpui::AnyElement> = Vec::new();
    cards.push(
        div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("Support for this model is ported from rivalcfg / OpenRGB and hasn't been tested on real hardware yet. Settings are written to the device's memory.")
            .into_any_element(),
    );
    if d.features.dpi_max > 0 {
        cards.push(dpi_card(v, d, cx));
    }
    if d.features.polling {
        let key = d.key.clone();
        let view = cx.entity();
        let labels: Vec<String> = POLLING.iter().map(|h| format!("{h} Hz")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let sel = saved.polling.and_then(|p| POLLING.iter().position(|x| *x == p));
        cards.push(
            widgets::card(cx)
                .child(widgets::card_title(cx, "Polling rate", Some("How often the mouse reports its position. 1000 Hz is smoothest; lower saves battery.")))
                .child(widgets::segmented("polling", &refs, sel, move |i, _, cx| {
                    view.read(cx).send(Command::Apply { device: key.clone(), setting: Setting::Polling(POLLING[i]) });
                }))
                .into_any_element(),
        );
    }
    if d.features.color_zones > 0 {
        cards.push(lighting_card(d, saved.lighting, cx));
    }

    let body = if d.link == Link::Unsupported {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(div().w(px(360.)).h(px(300.)).children(assets::image_for(d).map(|p| img(p).size_full())))
            .child(
                div()
                    .max_w(px(480.))
                    .text_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("OpenSeries recognises this device but doesn't control it yet. Open an issue on GitHub with the model name to get it supported."),
            )
            .into_any_element()
    } else {
        h_flex()
            .size_full()
            .child(art)
            .child(div().w(px(460.)).h_full().child(super::headset::scroll_page(v_flex().gap_4().children(cards))))
            .into_any_element()
    };
    v_flex().size_full().child(widgets::header(v, cx, d.name.clone(), true, None, Some(pill))).child(div().flex_1().min_h_0().child(body))
}

fn dpi_card(v: &mut AppView, d: &Device, cx: &mut Context<AppView>) -> gpui::AnyElement {
    let stages = v.dpi.stages.clone();
    let selected = v.dpi.selected.min(stages.len() - 1);
    let (lo, hi) = (d.features.dpi_min, d.features.dpi_max);
    let mut chips = Vec::new();
    for (i, dpi) in stages.iter().enumerate() {
        let on = i == selected;
        chips.push(
            v_flex()
                .id(("dpi-stage", i))
                .flex_1()
                .items_center()
                .py_2()
                .rounded(px(8.))
                .cursor_pointer()
                .border_1()
                .border_color(if on { widgets::accent() } else { cx.theme().border })
                .when(on, |s| s.bg(widgets::accent().opacity(0.12)))
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("Stage {}", i + 1)))
                .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(format!("{dpi}")))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.dpi.selected = i;
                    this.dpi.load_slider(window, cx);
                    cx.notify();
                }))
                .into_any_element(),
        );
    }
    let can_add = stages.len() < 5;
    let can_remove = stages.len() > 1;
    widgets::card(cx)
        .child(widgets::card_title(cx, "DPI stages", Some(&format!("The DPI button cycles through these. {lo}–{hi} DPI."))))
        .child(h_flex().gap_2().children(chips))
        .child(Slider::new(&v.dpi.slider).horizontal())
        .child(
            h_flex()
                .gap_2()
                .child(
                    gpui_component::button::Button::new("dpi-add").small().outline().label("Add stage").disabled(!can_add).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        let last = *this.dpi.stages.last().unwrap_or(&800);
                        this.dpi.stages.push((last * 2).min(18000));
                        this.dpi.selected = this.dpi.stages.len() - 1;
                        this.dpi.load_slider(window, cx);
                        this.send_dpi();
                        cx.notify();
                    })),
                )
                .child(
                    gpui_component::button::Button::new("dpi-remove").small().outline().label("Remove stage").disabled(!can_remove).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        let i = this.dpi.selected.min(this.dpi.stages.len() - 1);
                        this.dpi.stages.remove(i);
                        this.dpi.selected = i.saturating_sub(1);
                        this.dpi.load_slider(window, cx);
                        this.send_dpi();
                        cx.notify();
                    })),
                ),
        )
        .into_any_element()
}

fn lighting_card(d: &Device, saved: Option<openseries_core::config::Lighting>, cx: &mut Context<AppView>) -> gpui::AnyElement {
    let current = saved.map_or(([0xff, 0x5a, 0x1f], 100), |l| (l.rgb, l.brightness));
    let mut swatches = Vec::new();
    for (i, c) in SWATCHES.iter().enumerate() {
        let (c, key) = (*c, d.key.clone());
        let on = current.0 == c && saved.is_some();
        let hex = (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
        swatches.push(
            div()
                .id(("swatch", i))
                .size(px(30.))
                .rounded_full()
                .bg(rgb_hsla(hex))
                .cursor_pointer()
                .border_2()
                .border_color(if on { gpui::white() } else { gpui::transparent_black() })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    let brightness = if current.1 == 0 { 100 } else { current.1 };
                    this.send(Command::Apply { device: key.clone(), setting: Setting::Lighting { rgb: c, brightness } });
                }))
                .into_any_element(),
        );
    }
    let key = d.key.clone();
    let rgb = current.0;
    let view = cx.entity();
    let levels = [0u8, 25, 50, 75, 100];
    let sel = saved.and_then(|l| levels.iter().position(|b| *b == l.brightness));
    let what = if d.kind == Kind::Keyboard { "every key" } else { "every zone" };
    widgets::card(cx)
        .child(widgets::card_title(cx, "Lighting", Some(&format!("One colour for {what}."))))
        .child(h_flex().flex_wrap().gap_2().children(swatches))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Brightness"))
        .child(widgets::segmented("brightness", &["Off", "25%", "50%", "75%", "100%"], sel, move |i, _, cx| {
            view.read(cx).send(Command::Apply { device: key.clone(), setting: Setting::Lighting { rgb, brightness: levels[i] } });
        }))
        .child(h_flex().gap_2().text_xs().text_color(cx.theme().muted_foreground).child(Icon::new(Lucide::Info).size_3p5()).child("Keyboards get the colour back automatically after reconnecting."))
        .into_any_element()
}
