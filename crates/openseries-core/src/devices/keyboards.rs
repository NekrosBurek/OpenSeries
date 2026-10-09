//! Keyboard lighting, ported from OpenRGB (SteelSeries controllers @ deeecd3, 2026-10-08).
//! Unverified on hardware. Per-key "direct" frames are volatile, so the agent re-applies the
//! saved colour whenever a keyboard (re)connects.

use hidapi::HidDevice;

use super::hid::{feature, out};
use super::{Device, Features, Link, Setting};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyboard {
    /// Apex 5/7/9/Pro family: one feature-report frame of [keycode R G B] × 112.
    /// `id` 0x3A (gen 1), 0x40 (gen 2/3 wired), 0x61 (gen 3 wireless); `frame` = report length.
    ApexPerKey { id: u8, frame: usize, gen3: bool },
    /// Apex 3 TKL: 8 zones.
    Apex3Tkl,
    /// Apex 3: 10 zones, persistent.
    Apex3,
}

/// HID usages in OpenRGB's order (112 keys). Codes a board lacks are ignored by it.
const KEYS: [u8; 112] = [
    0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
    0x1a, 0x1b, 0x1c, 0x1d, // A..Z
    0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, // 1..0
    0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, // Enter..Caps
    0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, // F1..F12
    0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0x51, 0x52, // PrtSc..Up
    0x64, 0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xf0, // ISO\, modifiers, Fn
    0x31, 0x87, 0x88, 0x89, 0x8a, 0x8b, // ANSI\, JIS keys
    0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x61, 0x62, 0x63, // numpad
    0xfb, // media
];

impl Keyboard {
    pub fn features(self) -> Features {
        Features { color_zones: 1, brightness: true, ..Features::default() }
    }

    pub fn read(self, _dev: &HidDevice, d: &mut Device) {
        d.features = self.features();
        d.link = Link::Online;
    }

    pub fn apply(self, dev: &HidDevice, s: &Setting) -> anyhow::Result<()> {
        let Setting::Lighting { rgb, brightness } = s else { anyhow::bail!("not supported on this keyboard") };
        // Brightness is folded into the colour: direct frames are absolute.
        let scale = |c: u8| (u16::from(c) * u16::from((*brightness).min(100)) / 100) as u8;
        let [r, g, b] = rgb.map(scale);
        match self {
            Keyboard::ApexPerKey { id, frame, gen3 } => {
                if gen3 {
                    feature(dev, &[0x00, 0x4b], 65)?;
                }
                let mut buf = vec![0x00, id, KEYS.len() as u8];
                for k in KEYS {
                    buf.extend([k, r, g, b]);
                }
                feature(dev, &buf, frame)
            }
            Keyboard::Apex3Tkl => {
                let mut buf = vec![0x00, 0x21, 0xff];
                for _ in 0..8 {
                    buf.extend([r, g, b]);
                }
                out(dev, &buf, 65)?;
                out(dev, &[0x00, 0x23, 0x10], 65)
            }
            Keyboard::Apex3 => {
                out(dev, &[0x00, 0x0a, 0x00, 0x64], 33)?;
                let mut buf = vec![0x00, 0x0b, 0x00];
                for _ in 0..10 {
                    buf.extend([r, g, b]);
                }
                out(dev, &buf, 33)?;
                out(dev, &[0x00, 0x06, 0x00, 0x08], 33)?;
                out(dev, &[0x00, 0x09, 0x00, 0x00], 33)
            }
        }
    }

    /// Hand lighting back to the keyboard's own onboard profile.
    pub fn release(self, dev: &HidDevice) -> anyhow::Result<()> {
        match self {
            Keyboard::ApexPerKey { gen3, .. } => out(dev, &[0x00, if gen3 { 0x41 } else { 0x3b }], 65),
            _ => Ok(()),
        }
    }
}
