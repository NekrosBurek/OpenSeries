//! Icons for the tray and window, drawn by [`crate::art`] (no binary art in the repo).

use openseries_core::devices::{Device, Link};

const GREEN: [u8; 3] = [0x22, 0xc5, 0x5e];
const AMBER: [u8; 3] = [0xea, 0xb3, 0x08];
const RED: [u8; 3] = [0xef, 0x44, 0x44];
const BLUE: [u8; 3] = [0x3b, 0x82, 0xf6];
const GREY: [u8; 3] = [0x9c, 0xa3, 0xaf];

/// Battery-coloured accent for a device (shared with the UI).
pub fn battery_rgb(d: &Device) -> [u8; 3] {
    match (d.link, d.battery) {
        (Link::Online, Some(b)) if b.charging => BLUE,
        (Link::Online, Some(b)) if b.percent <= 25 => RED,
        (Link::Online, Some(b)) if b.percent <= 50 => AMBER,
        (Link::Online, _) => GREEN,
        (Link::NotResponding, _) => RED,
        _ => GREY,
    }
}

/// 32×32 RGBA tray icon. No device: the app mark.
pub fn tray(d: Option<&Device>) -> (Vec<u8>, u32) {
    let Some(d) = d else { return (crate::art::logo(32), 32) };
    let level = d.battery.filter(|_| d.link == Link::Online).map(|b| f32::from(b.percent) / 100.0);
    let dot = if d.link == Link::Online { [255, 255, 255] } else { GREY };
    (crate::art::tray(level, battery_rgb(d), dot), 32)
}

pub fn app(size: u32) -> (Vec<u8>, u32) {
    (crate::art::logo(size), size)
}
