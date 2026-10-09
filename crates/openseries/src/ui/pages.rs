//! Home (device gallery) and the app-level pages: Audio, Troubleshoot, Settings.

use gpui::prelude::*;
use gpui::{App, ClickEvent, FontWeight, div, img, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::switch::Switch;
use gpui_component::{ActiveTheme as _, Icon, IconName, h_flex, v_flex};
use gpui_kit_assets::IconName as Lucide;
use openseries_core::config::EndpointRef;
use openseries_core::devices::{Device, Link};
use openseries_core::ipc::Command;

use super::headset::scroll_page;
use super::widgets::{self, rgb_hsla};
use super::{AppView, Page, assets};

fn nav(cx: &mut Context<AppView>) -> gpui::AnyElement {
    h_flex()
        .gap_1()
        .child(widgets::icon_button("nav-audio", Lucide::Volume2, "Audio").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::Audio, cx))))
        .child(widgets::icon_button("nav-diag", Lucide::Stethoscope, "Troubleshoot").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::Troubleshoot, cx))))
        .child(widgets::icon_button("nav-settings", IconName::Settings, "Settings").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::Settings, cx))))
        .into_any_element()
}

pub fn home(v: &mut AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let devices = v.state.as_ref().map(|s| s.devices.clone()).unwrap_or_default();
    let body = if devices.is_empty() {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .text_color(cx.theme().muted_foreground)
            .child(Icon::new(Lucide::Usb).size_12())
            .child(div().text_lg().text_color(cx.theme().foreground).child("No SteelSeries devices found"))
            .child("Plug in your receiver or device. It shows up here within a few seconds.")
            .into_any_element()
    } else {
        h_flex()
            .size_full()
            .justify_center()
            .items_center()
            .flex_wrap()
            .gap_6()
            .p_8()
            .children({
                let mut cards = Vec::new();
                for (i, d) in devices.iter().enumerate() {
                    cards.push(device_card(i, d, cx));
                }
                cards
            })
            .into_any_element()
    };
    let nav = nav(cx);
    v_flex().size_full().child(widgets::header(v, cx, "OpenSeries", false, None, Some(nav))).child(div().flex_1().min_h_0().child(body))
}

fn device_card(i: usize, d: &Device, cx: &mut Context<AppView>) -> gpui::AnyElement {
    let key = d.key.clone();
    let (colour, status) = match (d.link, d.battery) {
        (Link::Online, Some(b)) if b.charging => (widgets::BLUE, format!("{}% · charging", b.percent)),
        (Link::Online, Some(b)) => (widgets::GREEN, format!("{}%", b.percent)),
        (Link::Online, None) => (widgets::GREEN, "Connected".into()),
        (Link::Asleep, _) => (widgets::GREY, "Off".into()),
        (Link::NotResponding, _) => (widgets::RED, "Not responding".into()),
        (Link::Unsupported, _) => (widgets::GREY, "Not supported yet".into()),
    };
    v_flex()
        .id(("device", i))
        .w(px(300.))
        .h(px(380.))
        .p_5()
        .gap_3()
        .rounded(px(16.))
        .bg(cx.theme().group_box)
        .border_1()
        .border_color(cx.theme().border.opacity(0.6))
        .cursor_pointer()
        .hover(|s| s.border_color(widgets::accent()))
        .child(
            div()
                .w(px(258.))
                .h(px(250.))
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .children(assets::image_for(d).map(|p| img(p).w(px(258.)).h(px(250.)).object_fit(gpui::ObjectFit::Contain))),
        )
        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(d.name.clone()))
        .child(h_flex().gap_2().text_sm().text_color(cx.theme().muted_foreground).child(widgets::dot(colour)).child(status))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(Page::Device(key.clone()), cx)))
        .into_any_element()
}

pub fn gone(v: &mut AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    v_flex()
        .size_full()
        .child(widgets::header(v, cx, "Device disconnected", true, None, None))
        .child(widgets::empty(cx, "This device was unplugged. Plug it back in, or go back to your devices."))
}

pub fn audio(v: &mut AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let s = v.state.clone().unwrap_or_default();
    let list = |title: &str, hint: &str, eps: &[EndpointRef], current: Option<&String>, input: bool, cx: &mut Context<AppView>| {
        widgets::card(cx).child(widgets::card_title(cx, title.to_string(), Some(hint))).child(v_flex().gap_1().children(eps.iter().enumerate().map(|(i, e)| {
            let on = current == Some(&e.id);
            let e2 = e.clone();
            let sonar = e.name.contains("Sonar");
            h_flex()
                .id((if input { "in" } else { "out" }, i))
                .gap_3()
                .px_3()
                .py_2()
                .rounded(px(8.))
                .cursor_pointer()
                .when(on, |r| r.bg(widgets::accent().opacity(0.14)))
                .hover(|r| r.bg(cx.theme().muted.opacity(0.4)))
                .child(div().w(px(18.)).children(on.then(|| Icon::new(IconName::Check).text_color(widgets::accent()))))
                .child(div().flex_1().text_sm().child(e.name.clone()))
                .when(sonar, |r| r.child(div().text_xs().text_color(cx.theme().muted_foreground).child("GG virtual device")))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    this.send(if input { Command::SetInput(e2.clone()) } else { Command::SetOutput(e2.clone()) })
                }))
        })))
    };
    let outputs = list("Headphones & speakers", "Where sound plays.", &s.audio.outputs, s.audio.output.as_ref(), false, cx);
    let inputs = list("Microphone", "Used by Discord, Teams, Zoom and every other app.", &s.audio.inputs, s.audio.input.as_ref(), true, cx);
    let lock = widgets::card(cx).child(
        h_flex()
            .gap_4()
            .child(div().flex_1().min_w_0().child(widgets::card_title(
                cx,
                "Keep these as the default",
                Some("Sets them as Windows default for every role, including Communications (calls and meetings), and puts them back if GG, a game or Windows changes it. Survives moving the receiver to another USB port."),
            )))
            .child(Switch::new("lock").checked(s.config.audio.lock).on_click(cx.listener(|this, on: &bool, _, _| this.send(Command::SetLock(*on))))),
    );
    let mute = widgets::card(cx).child(
        h_flex()
            .gap_4()
            .child(div().flex_1().min_w_0().child(widgets::card_title(cx, "Mute microphone", Some("Windows-level mute for the chosen microphone."))))
            .child(Switch::new("mute").checked(s.audio.mic_muted).on_click(cx.listener(|this, on: &bool, _, _| this.send(Command::SetMicMuted(*on))))),
    );
    v_flex()
        .size_full()
        .child(widgets::header(v, cx, "Audio", true, None, None))
        .child(div().flex_1().min_h_0().child(scroll_page(v_flex().gap_4().child(lock).child(outputs).child(inputs).child(mute))))
}

pub fn troubleshoot(v: &mut AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let running = v.diag_running;
    let diag = v.diagnostics.clone();
    let mut card = widgets::card(cx).child(
        h_flex()
            .gap_4()
            .child(div().flex_1().min_w_0().child(widgets::card_title(cx, "Check my setup", Some("Drivers, receiver, headset link, Windows default devices, Sonar leftovers and GG."))))
            .child(Button::new("run-diag").primary().label(if running { "Checking…" } else { "Run checks" }).loading(running).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.diag_running = true;
                this.send(Command::RunDiagnostics);
                cx.notify();
            }))),
    );
    if let Some(d) = &diag {
        card = card.child(div().h(px(1.)).w_full().bg(cx.theme().border)).children(d.checks.iter().map(|c| {
            h_flex()
                .gap_3()
                .py_1()
                .items_start()
                .child(Icon::new(if c.ok { IconName::CircleCheck } else { IconName::TriangleAlert }).text_color(rgb_hsla(if c.ok { widgets::GREEN } else { widgets::AMBER })))
                .child(v_flex().child(div().text_sm().font_weight(FontWeight::MEDIUM).child(c.title.clone())).child(div().text_xs().text_color(cx.theme().muted_foreground).child(c.detail.clone())))
        }));
        if let Some(id) = d.restart.clone() {
            card = card.child(
                h_flex().pt_2().child(Button::new("restart").danger().icon(Lucide::RotateCcw).label("Restart receiver (asks for admin)").on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    this.send(Command::RestartReceiver(id.clone()))
                }))),
            );
        }
    }
    v_flex().size_full().child(widgets::header(v, cx, "Troubleshoot", true, None, None)).child(div().flex_1().min_h_0().child(scroll_page(card)))
}

pub fn settings(v: &mut AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let s = v.state.clone().unwrap_or_default();
    let row = |title: &str, hint: &str, control: gpui::AnyElement, cx: &App| {
        widgets::card(cx).child(h_flex().gap_4().child(div().flex_1().min_w_0().child(widgets::card_title(cx, title.to_string(), Some(hint)))).child(control))
    };
    let autostart = Switch::new("autostart").checked(s.autostart).on_click(cx.listener(|this, on: &bool, _, _| this.send(Command::SetAutostart(*on)))).into_any_element();
    let update = Button::new("update").outline().label("Check for updates").on_click(cx.listener(|this, _: &ClickEvent, _, _| this.send(Command::CheckForUpdates))).into_any_element();
    let quit = Button::new("quit").danger().label("Quit OpenSeries").on_click(cx.listener(|this, _: &ClickEvent, window, _| {
        this.send(Command::Quit);
        window.remove_window();
    }))
    .into_any_element();
    let hint = format!("Version {} · updates install automatically from GitHub Releases.{}", s.version, s.update.as_deref().map(|u| format!(" {u}")).unwrap_or_default());
    v_flex().size_full().child(widgets::header(v, cx, "Settings", true, None, None)).child(div().flex_1().min_h_0().child(scroll_page(
        v_flex()
            .gap_4()
            .child(row("Start with Windows", "Runs quietly in the tray: battery, default devices and ChatMix keep working with this window closed.", autostart, cx))
            .child(row("Updates", &hint, update, cx))
            .child(row("Quit", "Stops the tray agent too. Default-device lock and ChatMix stop until you start OpenSeries again.", quit, cx))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("Config: {}", openseries_core::paths::config_file().display()))),
    )))
}
