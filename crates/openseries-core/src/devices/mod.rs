//! Which SteelSeries products we know, how to find them, and how to talk to them.
//!
//! Every SteelSeries USB device (VID 0x1038) is listed, supported or not, so the
//! home screen shows the user's whole setup. A model gains controls once its
//! protocol is implemented here. Protocols are ported from HeadsetControl
//! (headsets), rivalcfg (mice) and OpenRGB (keyboards); only the Arctis 7+ has
//! been verified on real hardware so far.

mod headsets;
mod hid;
mod keyboards;
mod mice;

use std::ffi::CString;

use hidapi::HidApi;
use serde::{Deserialize, Serialize};

pub use headsets::Headset;
pub use keyboards::Keyboard;
pub use mice::{Dpi, Mouse};

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
    Headset(Headset),
    Mouse(Mouse),
    Keyboard(Keyboard),
    /// Known product, no controls yet (shown with its proper name and render).
    None,
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
    /// Required usage page on that interface (0 = any vendor page).
    pub usage_page: u16,
}

const fn m(key: &'static str, name: &'static str, kind: Kind, pids: &'static [u16], image: &'static str, protocol: Protocol, interface: i32, usage_page: u16) -> Model {
    Model { key, name, kind, pids, image, protocol, interface, usage_page }
}

use Kind::{Headset as H, Keyboard as K, Mouse as M};

const NOVA7: Protocol = Protocol::Headset(Headset::Nova7 { discrete: false, chat_and_sidetone: true, sidetone_save: false });
const NOVA7_DISCRETE: Protocol = Protocol::Headset(Headset::Nova7 { discrete: true, chat_and_sidetone: true, sidetone_save: false });
const NOVA7P: Protocol = Protocol::Headset(Headset::Nova7 { discrete: false, chat_and_sidetone: false, sidetone_save: false });
const AEROX_W: Protocol = Protocol::Mouse(Mouse::AeroxWireless { dongle: false, zones: 3 });
const AEROX_W_DONGLE: Protocol = Protocol::Mouse(Mouse::AeroxWireless { dongle: true, zones: 3 });
const PRIME_W: Protocol = Protocol::Mouse(Mouse::AeroxWireless { dongle: false, zones: 1 });
const PRIME_W_DONGLE: Protocol = Protocol::Mouse(Mouse::AeroxWireless { dongle: true, zones: 1 });
const APEX_GEN1: Protocol = Protocol::Keyboard(Keyboard::ApexPerKey { id: 0x3a, frame: 643, gen3: false });
const APEX_GEN2: Protocol = Protocol::Keyboard(Keyboard::ApexPerKey { id: 0x40, frame: 643, gen3: false });
const APEX_GEN2_SMALL: Protocol = Protocol::Keyboard(Keyboard::ApexPerKey { id: 0x40, frame: 513, gen3: false });
const APEX_GEN3: Protocol = Protocol::Keyboard(Keyboard::ApexPerKey { id: 0x40, frame: 643, gen3: true });
const APEX_GEN3_WL: Protocol = Protocol::Keyboard(Keyboard::ApexPerKey { id: 0x61, frame: 643, gen3: true });
const VENDOR: u16 = 0xffc0;

pub static MODELS: &[Model] = &[
    // Headsets
    m("arctis_7_plus", "Arctis 7+", H, &[0x220e, 0x2236], "arctis_7.webp", Protocol::Headset(Headset::ArctisPlus), 3, VENDOR),
    m("arctis_7p_plus", "Arctis 7P+", H, &[0x2212], "arctis_7.webp", Protocol::Headset(Headset::ArctisPlus), 3, VENDOR),
    m("arctis_7x_plus", "Arctis 7X+", H, &[0x2216], "arctis_7.webp", Protocol::Headset(Headset::ArctisPlus), 3, VENDOR),
    m("arctis_nova_7", "Arctis Nova 7", H, &[0x2202], "arctis_nova_7.webp", NOVA7_DISCRETE, 3, VENDOR),
    m("arctis_nova_7", "Arctis Nova 7", H, &[0x22a1], "arctis_nova_7.webp", NOVA7, 3, VENDOR),
    m("arctis_nova_7_gen2", "Arctis Nova 7 Gen 2", H, &[0x227e], "arctis_nova_7_gen2_black.webp",
        Protocol::Headset(Headset::Nova7 { discrete: false, chat_and_sidetone: true, sidetone_save: true }), 3, VENDOR),
    m("arctis_nova_7x", "Arctis Nova 7X", H, &[0x2206, 0x22a4], "arctis_nova_7.webp", NOVA7_DISCRETE, 3, VENDOR),
    m("arctis_nova_7x", "Arctis Nova 7X", H, &[0x2258, 0x229e, 0x22ad, 0x22a5], "arctis_nova_7.webp", NOVA7, 3, VENDOR),
    m("arctis_nova_7_diablo", "Arctis Nova 7 Diablo IV", H, &[0x223a], "arctis_nova_7_diablo_iv.webp", NOVA7_DISCRETE, 3, VENDOR),
    m("arctis_nova_7_diablo", "Arctis Nova 7 Diablo IV", H, &[0x22a9], "arctis_nova_7_diablo_iv.webp", NOVA7, 3, VENDOR),
    m("arctis_nova_7_wow", "Arctis Nova 7 WoW Edition", H, &[0x227a], "arctis_nova_7_wow.webp", NOVA7_DISCRETE, 3, VENDOR),
    m("arctis_nova_7p", "Arctis Nova 7P", H, &[0x220a, 0x22a7, 0x2298], "arctis_nova_7_white.webp", NOVA7P, 3, VENDOR),
    m("arctis_nova_5", "Arctis Nova 5", H, &[0x2232], "arctis_nova_5_black.webp", Protocol::Headset(Headset::Nova5), 3, VENDOR),
    m("arctis_nova_5x", "Arctis Nova 5X", H, &[0x2253], "arctis_nova_5x_white.webp", Protocol::Headset(Headset::Nova5), 3, VENDOR),
    m("arctis_nova_pro_wireless", "Arctis Nova Pro Wireless", H, &[0x12e0, 0x12e5], "arctis_nova_pro_wireless.webp", Protocol::Headset(Headset::NovaPro), 4, 0),
    m("arctis_7_2019", "Arctis 7", H, &[0x1260, 0x12ad], "arctis_7.webp", Protocol::Headset(Headset::Arctis7Legacy), 5, 0),
    m("arctis_pro", "Arctis Pro", H, &[0x1252, 0x1280], "arctis_pro.webp", Protocol::Headset(Headset::Arctis7Legacy), 5, 0),
    m("arctis_1_wireless", "Arctis 1 Wireless", H, &[0x12b3, 0x12b6], "arctis_1.webp", Protocol::Headset(Headset::Arctis1Legacy), 3, 0xff43),
    m("arctis_7x", "Arctis 7X", H, &[0x12d7], "arctis_7.webp", Protocol::Headset(Headset::Arctis1Legacy), 3, 0xff43),
    m("arctis_7p", "Arctis 7P", H, &[0x12d5], "arctis_7.webp", Protocol::Headset(Headset::Arctis1Legacy), 3, 0xff43),
    m("arctis_9", "Arctis 9", H, &[0x12c2], "arctis_7.webp", Protocol::None, 0, 0),
    m("arctis_pro_wireless", "Arctis Pro Wireless", H, &[0x1290], "arctis_pro_wireless.webp", Protocol::None, 0, 0),
    m("arctis_nova_3", "Arctis Nova 3", H, &[0x12ec], "arctis_nova_3.webp", Protocol::None, 0, 0),
    m("arctis_nova_3p", "Arctis Nova 3P Wireless", H, &[0x2269, 0x226d], "arctis_nova_3_wireless_black.webp", Protocol::None, 0, 0),
    m("arctis_gamebuds", "Arctis GameBuds", H, &[0x230a], "arctis_gamebuds_black.webp", Protocol::None, 0, 0),
    // Mice
    m("aerox_3_wireless", "Aerox 3 Wireless", M, &[0x183a, 0x187a], "aerox_3_wireless_gen2_shadow.webp", AEROX_W, 3, VENDOR),
    m("aerox_3_wireless", "Aerox 3 Wireless", M, &[0x1838, 0x1878], "aerox_3_wireless_gen2_shadow.webp", AEROX_W_DONGLE, 3, VENDOR),
    m("aerox_5_wireless", "Aerox 5 Wireless", M, &[0x1854, 0x185e, 0x1862], "aerox_5.webp", AEROX_W, 3, VENDOR),
    m("aerox_5_wireless", "Aerox 5 Wireless", M, &[0x1852, 0x185c, 0x1860], "aerox_5.webp", AEROX_W_DONGLE, 3, VENDOR),
    m("aerox_9_wireless", "Aerox 9 Wireless", M, &[0x185a, 0x1876], "aerox_9_wireless.webp", AEROX_W, 3, VENDOR),
    m("aerox_9_wireless", "Aerox 9 Wireless", M, &[0x1858, 0x1874], "aerox_9_wireless.webp", AEROX_W_DONGLE, 3, VENDOR),
    m("prime_wireless", "Prime Wireless", M, &[0x1842, 0x184a], "prime.webp", PRIME_W, 3, VENDOR),
    m("prime_wireless", "Prime Wireless", M, &[0x1840, 0x1848], "prime.webp", PRIME_W_DONGLE, 3, VENDOR),
    m("aerox_3", "Aerox 3", M, &[0x1836], "aerox_3.webp", Protocol::Mouse(Mouse::AeroxWired { dpi: Dpi::Core, first_preset: 1, zones: 3 }), 3, 0),
    m("aerox_5", "Aerox 5", M, &[0x1850], "aerox_5.webp", Protocol::Mouse(Mouse::AeroxWired { dpi: Dpi::Air, first_preset: 0, zones: 3 }), 3, 0),
    m("prime_mini", "Prime Mini", M, &[0x184d], "prime.webp", Protocol::Mouse(Mouse::AeroxWired { dpi: Dpi::Div50, first_preset: 0, zones: 1 }), 3, 0),
    m("rival_3", "Rival 3", M, &[0x1824, 0x184c], "rival_3.webp", Protocol::Mouse(Mouse::Rival3), 3, 0),
    m("rival_3_wireless", "Rival 3 Wireless", M, &[0x1830], "rival_3_wireless_gen2.webp", Protocol::Mouse(Mouse::Rival3Wireless), 3, VENDOR),
    m("prime", "Prime", M, &[0x182e, 0x182a, 0x1856, 0x182c], "prime.webp", Protocol::Mouse(Mouse::Prime), 0, 0),
    m("rival_5", "Rival 5", M, &[0x183c, 0x183e], "rival_5.webp", Protocol::None, 0, 0),
    m("rival_3_gen2", "Rival 3 Gen 2", M, &[0x1870], "rival_3_gen2.webp", Protocol::None, 0, 0),
    m("rival_600", "Rival 600", M, &[0x1724, 0x172e], "rival_600.webp", Protocol::None, 0, 0),
    m("rival_310", "Rival 310", M, &[0x1720, 0x171e, 0x1736], "rival_310.webp", Protocol::None, 0, 0),
    // Keyboards
    m("apex_pro", "Apex Pro", K, &[0x1610], "apex_pro.webp", APEX_GEN1, 1, 0),
    m("apex_pro_tkl", "Apex Pro TKL", K, &[0x1614], "apex_pro_tkl_2022.webp", APEX_GEN1, 1, 0),
    m("apex_pro_tkl_2023", "Apex Pro TKL", K, &[0x1628], "apex_pro_tkl_2024.webp", APEX_GEN2, 1, 0),
    m("apex_pro_tkl_2023", "Apex Pro TKL Wireless", K, &[0x1630, 0x1632], "apex_pro_tkl_2024.webp", APEX_GEN3_WL, 3, VENDOR),
    m("apex_pro_tkl_gen3", "Apex Pro TKL Gen 3", K, &[0x1642], "apex_pro_tkl_2024.webp", APEX_GEN3, 1, 0),
    m("apex_pro_tkl_gen3", "Apex Pro TKL Gen 3 Wireless", K, &[0x1644, 0x1646], "apex_pro_tkl_2024.webp", APEX_GEN3_WL, 3, VENDOR),
    m("apex_pro_3", "Apex Pro", K, &[0x1640], "apex_pro_2024.webp", APEX_GEN1, 1, 0),
    m("apex_7", "Apex 7", K, &[0x1612], "apex_pro.webp", APEX_GEN1, 1, 0),
    m("apex_7_tkl", "Apex 7 TKL", K, &[0x1618], "apex_pro_tkl_2022.webp", APEX_GEN1, 1, 0),
    m("apex_5", "Apex 5", K, &[0x161c], "apex_pro.webp", APEX_GEN1, 1, 0),
    m("apex_9_tkl", "Apex 9 TKL", K, &[0x1634], "apex_9.webp", APEX_GEN2_SMALL, 1, 0),
    m("apex_9_mini", "Apex 9 Mini", K, &[0x1620], "apex_9.webp", APEX_GEN2_SMALL, 1, 0),
    m("apex_3_tkl", "Apex 3 TKL", K, &[0x1622], "apex_3_tkl.webp", Protocol::Keyboard(Keyboard::Apex3Tkl), 1, VENDOR),
    m("apex_3", "Apex 3", K, &[0x161a], "apex_3.webp", Protocol::Keyboard(Keyboard::Apex3), 3, 0),
];

pub fn model_for(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pids.contains(&pid))
}

/// What the UI may offer for a device.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Features {
    pub battery: bool,
    pub chatmix: bool,
    pub sidetone: bool,
    pub equalizer: bool,
    /// EQ range the hardware accepts (±dB); profile values beyond it are clamped.
    pub eq_max_db: u8,
    pub auto_off: bool,
    /// DPI range; `dpi_max == 0` means no DPI control.
    pub dpi_min: u16,
    pub dpi_max: u16,
    pub dpi_step: u16,
    pub polling: bool,
    /// 0 = no lighting control.
    pub color_zones: u8,
    pub brightness: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// Device (or wireless device behind its receiver) is reachable.
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
    // Path quality per product: 2 = exact usage page, 1 = vendor page, 0 = interface only.
    let mut best: Vec<u8> = Vec::new();
    for info in api.device_list().filter(|d| d.vendor_id() == STEELSERIES_VID) {
        let pid = info.product_id();
        let model = model_for(pid);
        let idx = match out.iter().position(|f| f.pid == pid) {
            Some(i) => i,
            None => {
                out.push(Found { pid, product: info.product_string().unwrap_or_default().to_string(), kind: Kind::Other, model, control: None });
                best.push(0);
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
        let Some(m) = model else { continue };
        f.kind = m.kind;
        if matches!(m.protocol, Protocol::None) || info.interface_number() != m.interface {
            continue;
        }
        // Windows splits one interface into several collections; prefer the vendor one.
        let page = info.usage_page();
        let quality = if m.usage_page != 0 && page == m.usage_page { 3 } else if page >= 0xff00 { 2 } else { 1 };
        if m.usage_page != 0 && page != m.usage_page && f.control.is_some() {
            continue;
        }
        if f.control.is_none() || quality > best[idx] {
            f.control = Some(info.path().to_owned());
            best[idx] = quality;
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
    let Ok(dev) = api.open_path(path) else {
        d.link = Link::NotResponding;
        return d;
    };
    match m.protocol {
        Protocol::Headset(h) => h.read(&dev, &mut d),
        Protocol::Mouse(x) => x.read(&dev, &mut d),
        Protocol::Keyboard(k) => k.read(&dev, &mut d),
        Protocol::None => {}
    }
    d
}

/// A setting change requested by the user. Levels are UI-level; protocols map them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Setting {
    /// 0 = off .. 3 = high.
    Sidetone(u8),
    /// Minutes, 0 = never.
    AutoOff(u8),
    /// 10 bands in dB.
    Equalizer(Vec<f32>),
    /// 1..=5 DPI stages.
    Dpi(Vec<u16>),
    /// Polling rate in Hz.
    Polling(u16),
    /// Solid colour for every zone, brightness 0..=100 folded in.
    Lighting { rgb: [u8; 3], brightness: u8 },
}

pub fn apply(api: &HidApi, f: &Found, s: &Setting) -> anyhow::Result<()> {
    let (Some(m), Some(path)) = (f.model, &f.control) else { anyhow::bail!("device not supported") };
    let dev = api.open_path(path)?;
    match m.protocol {
        Protocol::Headset(h) => h.apply(&dev, s),
        Protocol::Mouse(x) => x.apply(&dev, s),
        Protocol::Keyboard(k) => k.apply(&dev, s),
        Protocol::None => anyhow::bail!("device not supported"),
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

/// OpenSeries presets on top of the headset's factory ones.
pub const EQ_FOOTSTEPS: [f32; 10] = [-4.0, -3.0, -2.0, -1.0, 0.0, 2.0, 4.0, 5.0, 3.0, 0.0];
pub const EQ_IMMERSIVE: [f32; 10] = [4.5, 3.5, 1.5, 0.0, -1.0, 0.0, 1.0, 2.0, 3.0, 3.0];
pub const EQ_VOICE: [f32; 10] = [-4.0, -3.0, -1.0, 0.5, 2.0, 3.5, 3.5, 2.0, 0.0, -1.5];

pub const EQ_ALL_PRESETS: [(&str, [f32; 10]); 7] = [
    EQ_PRESETS[0],
    EQ_PRESETS[1],
    EQ_PRESETS[2],
    EQ_PRESETS[3],
    ("Footsteps", EQ_FOOTSTEPS),
    ("Immersive", EQ_IMMERSIVE),
    ("Voice", EQ_VOICE),
];

pub fn eq_preset_name(bands: &[f32]) -> Option<&'static str> {
    EQ_ALL_PRESETS.iter().find(|(_, p)| p.as_slice() == bands).map(|(n, _)| *n)
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
