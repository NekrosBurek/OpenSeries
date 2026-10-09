//! Which SteelSeries products we know, how to find them, and how to talk to them.
//!
//! Every SteelSeries USB device (VID 0x1038) is listed, supported or not, so the
//! home screen shows the user's whole setup. A model gains controls once its
//! protocol is implemented here.

mod arctis_nova;

use std::ffi::CString;

use hidapi::HidApi;
use serde::{Deserialize, Serialize};

pub const STEELSERIES_VID: u16 = 0x1038;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Headset,
    Mouse,
    Keyboard,
    Microphone,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// Arctis 7+ family (HeadsetControl's "SteelSeriesNovaDevice" command set).
    ArctisPlus,
}

pub struct Model {
    /// Stable id, also the config section name.
    pub key: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub pids: &'static [u16],
    /// Render file name in the asset store.
    pub image: &'static str,
    pub protocol: Protocol,
    /// HID interface carrying the vendor protocol.
    pub interface: i32,
}

pub static MODELS: &[Model] = &[
    Model { key: "arctis_7_plus", name: "Arctis 7+", kind: Kind::Headset, pids: &[0x220e, 0x2236], image: "arctis_7.webp", protocol: Protocol::ArctisPlus, interface: 3 },
    Model { key: "arctis_7p_plus", name: "Arctis 7P+", kind: Kind::Headset, pids: &[0x2212], image: "arctis_7.webp", protocol: Protocol::ArctisPlus, interface: 3 },
    Model { key: "arctis_7x_plus", name: "Arctis 7X+", kind: Kind::Headset, pids: &[0x2216], image: "arctis_7.webp", protocol: Protocol::ArctisPlus, interface: 3 },
];

pub fn model_for(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pids.contains(&pid))
}

/// What the UI may offer for a device.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features {
    pub battery: bool,
    pub chatmix: bool,
    pub sidetone: bool,
    pub equalizer: bool,
    pub auto_off: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// Device (or headset behind its receiver) is reachable.
    Online,
    /// Receiver present, wireless device off or out of range.
    Asleep,
    /// Present on USB but not answering.
    NotResponding,
    /// Shown for completeness; we don't speak its protocol yet.
    Unsupported,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatMix {
    pub game: u8,
    pub chat: u8,
}

/// Live device as published to the UI.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Device {
    /// Model key, or `usb_1038_xxxx` for unknown products.
    pub key: String,
    pub name: String,
    pub kind: Kind,
    pub pid: u16,
    pub image: String,
    pub link: Link,
    pub features: Features,
    pub battery: Option<Battery>,
    pub chatmix: Option<ChatMix>,
    /// Raw status bytes, logged when they change (helps map undocumented bytes).
    #[serde(skip)]
    pub raw: Vec<u8>,
}

/// One USB product found during a scan.
pub struct Found {
    pub pid: u16,
    pub product: String,
    pub kind: Kind,
    pub model: Option<&'static Model>,
    /// Path of the vendor-protocol interface, when the model is known.
    pub control: Option<CString>,
}

/// List SteelSeries products currently on USB (one entry per product, not per interface).
pub fn scan(api: &mut HidApi) -> Vec<Found> {
    if let Err(e) = api.refresh_devices() {
        tracing::warn!("HID refresh failed: {e}");
    }
    let mut out: Vec<Found> = Vec::new();
    for info in api.device_list().filter(|d| d.vendor_id() == STEELSERIES_VID) {
        let pid = info.product_id();
        let model = model_for(pid);
        let idx = match out.iter().position(|f| f.pid == pid) {
            Some(i) => i,
            None => {
                out.push(Found { pid, product: info.product_string().unwrap_or_default().to_string(), kind: Kind::Other, model, control: None });
                out.len() - 1
            }
        };
        let f = &mut out[idx];
        // Generic HID usages tell us what an unknown product is.
        match (info.usage_page(), info.usage()) {
            (0x01, 0x02) if f.kind == Kind::Other => f.kind = Kind::Mouse,
            (0x01, 0x06) if f.kind == Kind::Other => f.kind = Kind::Keyboard,
            _ => {}
        }
        if let Some(m) = model {
            f.kind = m.kind;
            if info.interface_number() == m.interface && info.usage_page() >= 0xff00 {
                f.control = Some(info.path().to_owned());
            }
        }
    }
    out
}

/// Query a found product's live state. Never fails: errors become `Link::NotResponding`.
pub fn read(api: &HidApi, f: &Found) -> Device {
    let mut d = Device {
        key: f.model.map_or_else(|| format!("usb_1038_{:04x}", f.pid), |m| m.key.to_string()),
        name: f.model.map_or_else(|| clean_product_name(&f.product), |m| m.name.to_string()),
        kind: f.kind,
        pid: f.pid,
        image: f.model.map_or_else(|| guess_image(&f.product, f.kind), |m| m.image.to_string()),
        link: Link::Unsupported,
        features: Features::default(),
        battery: None,
        chatmix: None,
        raw: Vec::new(),
    };
    let (Some(m), Some(path)) = (f.model, &f.control) else { return d };
    match m.protocol {
        Protocol::ArctisPlus => arctis_nova::read(api, path, &mut d),
    }
    d
}

/// A setting change requested by the user.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Setting {
    Sidetone(u8),
    AutoOff(u8),
    Equalizer(Vec<f32>),
}

pub fn apply(api: &HidApi, f: &Found, s: &Setting) -> anyhow::Result<()> {
    let (Some(m), Some(path)) = (f.model, &f.control) else { anyhow::bail!("device not supported") };
    match m.protocol {
        Protocol::ArctisPlus => arctis_nova::apply(api, path, s),
    }
}

/// Equalizer bands (Hz labels) and the headset's factory presets, in dB.
pub const EQ_BANDS: [&str; 10] = ["31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
pub const EQ_PRESETS: [(&str, [f32; 10]); 4] = [
    ("Flat", [0.0; 10]),
    ("Bass Boost", [3.5, 4.0, 1.0, -1.5, -1.5, -1.0, -1.0, -1.0, -1.0, 5.5]),
    ("Smiley", [3.0, 1.5, -1.5, -4.0, -4.0, -2.5, 1.5, 3.0, 4.0, 3.5]),
    ("Focus", [-5.0, -1.0, -3.5, -2.5, 4.0, 6.0, 3.5, -3.5, 0.0, -3.5]),
];

pub fn eq_preset_name(bands: &[f32]) -> Option<&'static str> {
    EQ_PRESETS.iter().find(|(_, p)| p.as_slice() == bands).map(|(n, _)| *n)
}

/// "SteelSeries Rival 3 Wireless" -> "Rival 3 Wireless".
fn clean_product_name(s: &str) -> String {
    let s = s.trim().trim_start_matches("SteelSeries").trim();
    if s.is_empty() { "SteelSeries device".into() } else { s.into() }
}

/// GG's render names are the product name in snake case (`rival_3.webp`); generic art otherwise.
fn guess_image(product: &str, kind: Kind) -> String {
    let slug: String = clean_product_name(product)
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if slug.is_empty() || slug == "steelseries_device" {
        return format!("generics/{}.webp", generic(kind));
    }
    format!("{slug}.webp")
}

pub fn generic(kind: Kind) -> &'static str {
    match kind {
        Kind::Headset | Kind::Microphone => "headset",
        Kind::Mouse => "mouse",
        Kind::Keyboard => "keyboard",
        Kind::Other => "controller",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_images() {
        assert_eq!(clean_product_name("SteelSeries Rival 3 Wireless"), "Rival 3 Wireless");
        assert_eq!(guess_image("SteelSeries Rival 3", Kind::Mouse), "rival_3.webp");
        assert_eq!(guess_image("", Kind::Keyboard), "generics/keyboard.webp");
        assert_eq!(model_for(0x220e).unwrap().key, "arctis_7_plus");
    }
}
