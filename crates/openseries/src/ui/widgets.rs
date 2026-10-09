//! Shared look: brand colours, page header, cards, status pills.

use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, FontWeight, Hsla, SharedString, div, px, rems};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{ActiveTheme as _, IconName, Sizable as _, Theme, h_flex, v_flex};


use super::{AppView, Page};

pub const GREEN: u32 = 0x22c55e;
pub const AMBER: u32 = 0xeab308;
pub const RED: u32 = 0xef4444;
pub const BLUE: u32 = 0x3b82f6;
pub const GREY: u32 = 0x6b7280;
/// SteelSeries orange.
pub const ORANGE: u32 = 0xff5a1f;

pub fn rgb_hsla(c: u32) -> Hsla {
    gpui::rgb(c).into()
}

pub fn accent() -> Hsla {
    rgb_hsla(ORANGE)
}

/// Dark theme with the brand accent on primary controls, sliders and switches.
pub fn brand_theme(cx: &mut App) {
    let t = Theme::global_mut(cx);
    let a = accent();
    // Neutral graphite surfaces with enough step between them to read as layers.
    t.background = rgb_hsla(0x0e1013);
    t.group_box = rgb_hsla(0x171a1f);
    t.border = rgb_hsla(0x262a31);
    t.muted = rgb_hsla(0x22262d);
    t.muted_foreground = rgb_hsla(0x8b919c);
    t.foreground = rgb_hsla(0xe7e9ec);
    t.popover = rgb_hsla(0x171a1f);
    t.primary = a;
    t.primary_hover = a.opacity(0.88);
    t.primary_active = a.opacity(0.78);
    t.primary_foreground = gpui::white();
    t.ring = a;
    t.button_primary = a;
    t.button_primary_hover = a.opacity(0.88);
    t.button_primary_active = a.opacity(0.78);
    t.button_primary_foreground = gpui::white();
    t.slider_bar = a;
    t.slider_thumb = gpui::white();
    t.switch = a;
    t.progress_bar = a;
    // Widgets read the resolved token layer, not the colours directly.
    let colors = t.colors.clone();
    t.tokens = (&colors).into();
    t.radius = px(8.);
    t.radius_lg = px(12.);
    cx.refresh_windows();
}

pub fn dot(c: u32) -> impl IntoElement {
    div().size(px(8.)).rounded_full().bg(rgb_hsla(c))
}

pub fn empty(cx: &App, text: &str) -> impl IntoElement {
    v_flex().size_full().items_center().justify_center().text_color(cx.theme().muted_foreground).child(text.to_string())
}

/// Rounded panel used for every settings block.
pub fn card(cx: &App) -> gpui::Div {
    v_flex().p_5().gap_3().rounded(px(12.)).bg(cx.theme().group_box).border_1().border_color(cx.theme().border.opacity(0.6))
}

pub fn card_title(cx: &App, title: impl Into<SharedString>, hint: Option<&str>) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(title.into()))
        .when_some(hint.map(str::to_string), |this, h| this.child(div().text_xs().text_color(cx.theme().muted_foreground).child(h)))
}

/// Status pill like OpenLogi's "● Connected".
pub fn pill(cx: &App, colour: u32, text: impl Into<SharedString>) -> impl IntoElement {
    h_flex()
        .gap_2()
        .h(px(30.))
        .px_3()
        .rounded_full()
        .border_1()
        .border_color(cx.theme().border)
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(dot(colour))
        .child(text.into())
}

/// Page header: [‹ Back]  Title    [tabs]    [right]
pub fn header(
    v: &AppView,
    cx: &mut Context<AppView>,
    title: impl Into<SharedString>,
    back: bool,
    tabs: Option<AnyElement>,
    right: Option<AnyElement>,
) -> impl IntoElement {
    let _ = v;
    h_flex()
        .h(px(72.))
        .px_6()
        .gap_4()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .flex_1()
                .gap_3()
                .when(back, |this| {
                    this.child(
                        Button::new("back")
                            .ghost()
                            .icon(IconName::ChevronLeft)
                            .label("Back")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::Home, cx))),
                    )
                })
                .child(div().text_size(rems(1.6)).font_weight(FontWeight::SEMIBOLD).child(title.into())),
        )
        .children(tabs)
        .child(h_flex().flex_1().justify_end().gap_2().children(right))
}

/// Segmented choice row (sidetone, auto-off, presets...).
pub fn segmented(
    id: &'static str,
    labels: &[&str],
    selected: Option<usize>,
    on_pick: impl Fn(usize, &mut gpui::Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_pick = std::rc::Rc::new(on_pick);
    h_flex().w_full().gap_1().children(labels.iter().enumerate().map(|(i, l)| {
        let on_pick = on_pick.clone();
        Button::new((id, i))
            .flex_1()
            .h(px(34.))
            .label(l.to_string())
            .when(selected == Some(i), |b| b.primary())
            .when(selected != Some(i), |b| b.outline())
            .on_click(move |_, w, cx| on_pick(i, w, cx))
    }))
}

pub fn icon_button(id: &'static str, icon: impl Into<gpui_component::Icon>, label: &str) -> Button {
    Button::new(id).ghost().small().icon(icon).label(label.to_string())
}

/// GG-style ChatMix balance: one line, centre = both at 100%, knob leans toward
/// the side that's louder (Game left, Chat right).
pub fn chatmix_line(cx: &App, mix: Option<openseries_core::devices::ChatMix>, height: f32) -> impl IntoElement {
    let (game, chat) = mix.map_or((100, 100), |m| (m.game, m.chat));
    // -1 = full game, +1 = full chat.
    let pos = if chat < 100 { -(100.0 - f32::from(chat)) / 100.0 } else { (100.0 - f32::from(game)) / 100.0 };
    let frac = (pos + 1.0) / 2.0;
    let knob = height * 0.6;
    let fill = if pos < 0.0 { accent() } else { rgb_hsla(BLUE) };
    let label = |name: &str, v: u8, colour: Hsla, cx: &App| {
        v_flex()
            .w(px(56.))
            .items_center()
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(name.to_string()))
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(colour).child(if mix.is_some() { format!("{v}%") } else { "—".into() }))
    };
    h_flex()
        .w_full()
        .gap_2()
        .child(label("GAME", game, accent(), cx))
        .child(
            div()
                .flex_1()
                .h(px(height))
                .relative()
                // Track.
                .child(div().absolute().left_0().right_0().top(px(height / 2. - 1.5)).h(px(3.)).rounded_full().bg(cx.theme().muted))
                // Fill from the centre to the knob.
                .child(
                    div()
                        .absolute()
                        .top(px(height / 2. - 1.5))
                        .h(px(3.))
                        .rounded_full()
                        .bg(fill)
                        .left(gpui::relative(frac.min(0.5)))
                        .w(gpui::relative((frac - 0.5).abs())),
                )
                // Centre tick: both at 100%.
                .child(div().absolute().left(gpui::relative(0.5)).ml(px(-1.)).top(px(height / 2. - 6.)).w(px(2.)).h(px(12.)).bg(cx.theme().muted_foreground))
                .child(
                    div()
                        .absolute()
                        .left(gpui::relative(frac))
                        .ml(px(-knob / 2.))
                        .top(px((height - knob) / 2.))
                        .size(px(knob))
                        .rounded_full()
                        .bg(gpui::white())
                        .border_2()
                        .border_color(if mix.is_some() { fill } else { cx.theme().muted }),
                ),
        )
        .child(label("CHAT", chat, rgb_hsla(BLUE), cx))
}
