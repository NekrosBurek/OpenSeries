//! Dependency-free vector drawing (signed distance fields, anti-aliased).
//! Shared by the app at runtime (tray, window icon) and build.rs (the .exe icon).

pub struct Canvas {
    pub size: u32,
    pub px: Vec<u8>,
}

impl Canvas {
    pub fn new(size: u32) -> Self {
        Self { size, px: vec![0; (size * size * 4) as usize] }
    }

    fn blend(&mut self, x: u32, y: u32, rgb: [u8; 3], coverage: f32) {
        if coverage <= 0.0 {
            return;
        }
        let i = ((y * self.size + x) * 4) as usize;
        let a = coverage.min(1.0);
        let da = f32::from(self.px[i + 3]) / 255.0;
        let out_a = a + da * (1.0 - a);
        for c in 0..3 {
            let (src, dst) = (f32::from(rgb[c]), f32::from(self.px[i + c]));
            self.px[i + c] = ((src * a + dst * da * (1.0 - a)) / out_a.max(1e-6)) as u8;
        }
        self.px[i + 3] = (out_a * 255.0) as u8;
    }

    /// Fill where `sdf` < 0 (coordinates in 0..1 of the canvas), colour chosen per pixel.
    pub fn fill(&mut self, alpha: f32, sdf: impl Fn(f32, f32) -> f32, colour: impl Fn(f32, f32) -> [u8; 3]) {
        let s = self.size as f32;
        for y in 0..self.size {
            for x in 0..self.size {
                let (u, v) = ((x as f32 + 0.5) / s, (y as f32 + 0.5) / s);
                // Distance in pixels for a one-pixel anti-aliased edge at any size.
                let d = sdf(u, v) * s;
                self.blend(x, y, colour(u, v), (0.5 - d).clamp(0.0, 1.0) * alpha);
            }
        }
    }
}

pub fn circle(cx: f32, cy: f32, r: f32) -> impl Fn(f32, f32) -> f32 {
    move |x, y| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r
}

pub fn rounded_rect(cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> impl Fn(f32, f32) -> f32 {
    move |x, y| {
        let (qx, qy) = ((x - cx).abs() - (hw - r), (y - cy).abs() - (hh - r));
        (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
    }
}

/// Stroke of a circular arc spanning `from..to` radians, measured clockwise from 12 o'clock, round caps.
pub fn arc(cx: f32, cy: f32, r: f32, w: f32, from: f32, to: f32) -> impl Fn(f32, f32) -> f32 {
    use std::f32::consts::TAU;
    move |x, y| {
        let (dx, dy) = (x - cx, y - cy);
        let ang = (dx.atan2(-dy) - from).rem_euclid(TAU);
        if ang <= to - from {
            ((dx * dx + dy * dy).sqrt() - r).abs() - w / 2.0
        } else {
            let end = |a: f32| ((x - (cx + r * a.sin())).powi(2) + (y - (cy - r * a.cos())).powi(2)).sqrt() - w / 2.0;
            end(from).min(end(to))
        }
    }
}

fn lerp(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t) as u8)
}

/// The OpenSeries mark: orange headphones on a graphite tile.
pub fn logo(size: u32) -> Vec<u8> {
    let mut c = Canvas::new(size);
    let tile = rounded_rect(0.5, 0.5, 0.5, 0.5, 0.23);
    c.fill(1.0, &tile, |_, v| lerp([0x2a, 0x2e, 0x37], [0x10, 0x12, 0x16], v));
    // Hairline highlight along the top edge for depth.
    c.fill(0.2, |x, y| tile(x, y).abs() - 0.006, |_, v| lerp([0x5a, 0x60, 0x6c], [0x1a, 0x1c, 0x21], v * 2.0));

    let orange = |_: f32, v: f32| lerp([0xff, 0x82, 0x34], [0xff, 0x45, 0x10], (v - 0.2) / 0.6);
    let (cx, cy, r) = (0.5, 0.56, 0.255);
    let a = 1.95_f32; // band reaches just below horizontal on both sides
    c.fill(1.0, arc(cx, cy, r, 0.075, -a, a), orange);
    let (ex, ey) = (r * a.sin(), cy - r * a.cos());
    for side in [-1.0, 1.0] {
        c.fill(1.0, rounded_rect(cx + side * ex, ey + 0.035, 0.07, 0.13, 0.055), orange);
    }
    // The dial: an open ring with a knob, between the cups.
    let (dx, dy, dr) = (0.5, 0.635, 0.085);
    c.fill(1.0, arc(dx, dy, dr, 0.034, 0.5, 5.6), |_, _| [0xf4, 0xf5, 0xf7]);
    c.fill(1.0, circle(dx + dr * 0.25_f32.sin(), dy - dr * 0.25_f32.cos(), 0.026), orange);
    c.px
}

/// 32 px tray icon: battery ring around a status dot.
pub fn tray(level: Option<f32>, ring: [u8; 3], dot: [u8; 3]) -> Vec<u8> {
    let mut c = Canvas::new(32);
    let (cx, cy, r, w) = (0.5, 0.5, 12.5 / 32.0, 4.0 / 32.0);
    c.fill(0.28, arc(cx, cy, r, w, 0.0, std::f32::consts::TAU), |_, _| [255, 255, 255]);
    if let Some(l) = level {
        c.fill(1.0, arc(cx, cy, r, w, 0.0, l.max(0.06) * std::f32::consts::TAU), |_, _| ring);
    }
    c.fill(1.0, circle(cx, cy, 5.0 / 32.0), |_, _| dot);
    c.px
}

#[cfg(test)]
mod tests {
    /// `cargo test -p openseries logo_preview -- --ignored` writes target/logo_preview.png.
    #[test]
    #[ignore]
    fn logo_preview() {
        let mut sheet = image::RgbaImage::new(256 + 64 + 32 + 16 + 40, 256);
        let mut x = 0;
        for s in [256u32, 64, 32, 16] {
            let img = image::RgbaImage::from_raw(s, s, super::logo(s)).unwrap();
            image::imageops::overlay(&mut sheet, &img, x, 0);
            x += i64::from(s) + 10;
        }
        sheet.save("../../target/logo_preview.png").unwrap();
    }
}
