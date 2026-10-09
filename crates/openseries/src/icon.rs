//! Procedural icons (no binary art in the repo): the tray's battery ring and the app mark.

use std::f32::consts::{PI, TAU};

use openseries_core::devices::{Device, Link};

pub const ACCENT: [u8; 3] = [0xff, 0x5a, 0x1f];
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

struct Canvas {
    size: u32,
    px: Vec<u8>,
}

impl Canvas {
    fn new(size: u32) -> Self {
        Self { size, px: vec![0; (size * size * 4) as usize] }
    }

    /// Alpha-blend `rgb` at `coverage` (0..1) over pixel (x, y).
    fn blend(&mut self, x: u32, y: u32, rgb: [u8; 3], coverage: f32) {
        if coverage <= 0.0 {
            return;
        }
        let i = ((y * self.size + x) * 4) as usize;
        let a = coverage.min(1.0);
        let da = f32::from(self.px[i + 3]) / 255.0;
        let out_a = a + da * (1.0 - a);
        for c in 0..3 {
            let src = f32::from(rgb[c]);
            let dst = f32::from(self.px[i + c]);
            self.px[i + c] = ((src * a + dst * da * (1.0 - a)) / out_a.max(1e-6)) as u8;
        }
        self.px[i + 3] = (out_a * 255.0) as u8;
    }

    /// Paint every pixel whose centre passes `inside` (returns signed distance; negative = inside).
    fn shape(&mut self, rgb: [u8; 3], alpha: f32, sdf: impl Fn(f32, f32) -> f32) {
        for y in 0..self.size {
            for x in 0..self.size {
                let d = sdf(x as f32 + 0.5, y as f32 + 0.5);
                self.blend(x, y, rgb, (0.5 - d).clamp(0.0, 1.0) * alpha);
            }
        }
    }
}

/// Ring arc from 12 o'clock clockwise covering `frac` of the circle.
fn arc_sdf(cx: f32, cy: f32, r: f32, w: f32, frac: f32) -> impl Fn(f32, f32) -> f32 {
    move |x, y| {
        let (dx, dy) = (x - cx, y - cy);
        let ring = ((dx * dx + dy * dy).sqrt() - r).abs() - w / 2.0;
        // Angle measured clockwise from the top, 0..TAU.
        let ang = (dx.atan2(-dy) + TAU) % TAU;
        if frac >= 1.0 || ang <= frac * TAU {
            ring
        } else {
            // Round caps: distance to the nearer arc end.
            let end = |a: f32| {
                let (ex, ey) = (cx + r * a.sin(), cy - r * a.cos());
                ((x - ex).powi(2) + (y - ey).powi(2)).sqrt() - w / 2.0
            };
            end(0.0).min(end(frac * TAU))
        }
    }
}

fn circle_sdf(cx: f32, cy: f32, r: f32) -> impl Fn(f32, f32) -> f32 {
    move |x, y| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r
}

/// 32×32 RGBA tray icon: battery ring around a status dot.
pub fn tray(d: Option<&Device>) -> (Vec<u8>, u32) {
    let s = 32.0;
    let mut c = Canvas::new(32);
    let (cx, cy, r, w) = (s / 2.0, s / 2.0, 12.5, 4.0);
    c.shape([255, 255, 255], 0.28, arc_sdf(cx, cy, r, w, 1.0));
    let colour = d.map_or(GREY, battery_rgb);
    if let Some(b) = d.filter(|d| d.link == Link::Online).and_then(|d| d.battery) {
        c.shape(colour, 1.0, arc_sdf(cx, cy, r, w, (f32::from(b.percent) / 100.0).max(0.06)));
    }
    let dot = if d.is_some_and(|d| d.link == Link::Online) { [255, 255, 255] } else { GREY };
    c.shape(dot, 1.0, circle_sdf(cx, cy, 5.0));
    (c.px, 32)
}

/// App mark: dark rounded square, accent ring with a gap, white core.
pub fn app(size: u32) -> (Vec<u8>, u32) {
    let s = size as f32;
    let mut c = Canvas::new(size);
    let rr = s * 0.22;
    c.shape([0x16, 0x18, 0x1d], 1.0, move |x, y| {
        // Rounded-rectangle SDF.
        let (qx, qy) = ((x - s / 2.0).abs() - (s / 2.0 - rr), (y - s / 2.0).abs() - (s / 2.0 - rr));
        (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rr
    });
    let (r, w) = (s * 0.27, s * 0.11);
    // Rotate the gap to the lower right like a dial.
    let ring = arc_sdf(s / 2.0, s / 2.0, r, w, 0.8);
    c.shape(ACCENT, 1.0, move |x, y| {
        let (dx, dy) = (x - s / 2.0, y - s / 2.0);
        let a = PI * 0.6;
        ring(s / 2.0 + dx * a.cos() - dy * a.sin(), s / 2.0 + dx * a.sin() + dy * a.cos())
    });
    c.shape([255, 255, 255], 1.0, circle_sdf(s / 2.0, s / 2.0, s * 0.09));
    (c.px, size)
}
