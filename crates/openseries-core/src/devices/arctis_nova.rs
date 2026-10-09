//! Arctis 7+ family (0x220e/0x2212/0x2216/0x2236) over the receiver's vendor interface.
//!
//! Status (`b0`) reply, report id stripped: `[b0, link, battery 0..4, charging, game 0..100, chat 0..100, ..]`
//! where link 0x01 = headset off, 0x03 = online. Verified on hardware.
//! Setters are from HeadsetControl; every set is followed by `09` (persist to flash).

use std::ffi::CStr;
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};

use super::{Battery, ChatMix, Device, Features, Link, Setting};

const STATUS: u8 = 0xb0;
const SAVE: [u8; 2] = [0x00, 0x09];

pub(super) fn read(api: &HidApi, path: &CStr, d: &mut Device) {
    d.link = Link::NotResponding;
    let Ok(dev) = api.open_path(path) else { return };
    let Some(r) = query(&dev, STATUS) else { return };
    d.raw = r.clone();
    if r[1] == 0x01 {
        d.link = Link::Asleep;
        return;
    }
    d.link = Link::Online;
    d.features = Features { battery: true, chatmix: true, sidetone: true, equalizer: true, auto_off: true };
    d.battery = Some(Battery { percent: r[2].min(4) * 25, charging: r[3] == 0x01 });
    d.chatmix = Some(ChatMix { game: r[4].min(100), chat: r[5].min(100) });
}

/// Send `[0, cmd]` and wait for a reply starting with `cmd`, skipping unrelated reports.
fn query(dev: &HidDevice, cmd: u8) -> Option<Vec<u8>> {
    dev.write(&[0x00, cmd]).ok()?;
    let deadline = Instant::now() + Duration::from_millis(800);
    let mut buf = [0u8; 64];
    while Instant::now() < deadline {
        let n = dev.read_timeout(&mut buf, 200).ok()?;
        // Some backends keep the zero report id in front; normalise to `[cmd, ..]`.
        let r = if n > 1 && buf[0] == 0 && buf[1] == cmd { &buf[1..n] } else { &buf[..n] };
        if r.len() >= 6 && r[0] == cmd {
            return Some(r.to_vec());
        }
    }
    None
}

pub(super) fn apply(api: &HidApi, path: &CStr, s: &Setting) -> anyhow::Result<()> {
    let cmd = command(s)?;
    let dev = api.open_path(path)?;
    dev.write(&cmd)?;
    dev.write(&SAVE)?;
    Ok(())
}

fn command(s: &Setting) -> anyhow::Result<Vec<u8>> {
    Ok(match s {
        Setting::Sidetone(level) => vec![0x00, 0x39, (*level).min(3)],
        Setting::AutoOff(minutes) => vec![0x00, 0xa3, (*minutes).min(90)],
        Setting::Equalizer(bands) => {
            anyhow::ensure!(bands.len() == 10, "equalizer needs 10 bands");
            let mut v = vec![0x00, 0x33];
            // 0x18 = 0 dB, one step = 0.5 dB, range ±12 dB.
            v.extend(bands.iter().map(|b| (0x18 as f32 + (b.clamp(-12.0, 12.0) * 2.0).round()) as u8));
            v.push(0x00);
            v
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_encoding_matches_headsetcontrol_presets() {
        // HeadsetControl's "Bass Boost" bytes.
        let bass = [3.5, 4.0, 1.0, -1.5, -1.5, -1.0, -1.0, -1.0, -1.0, 5.5];
        let got = command(&Setting::Equalizer(bass.to_vec())).unwrap();
        assert_eq!(got, [0x00, 0x33, 0x1f, 0x20, 0x1a, 0x15, 0x15, 0x16, 0x16, 0x16, 0x16, 0x23, 0x00]);
        assert_eq!(command(&Setting::Sidetone(9)).unwrap(), [0x00, 0x39, 0x03]);
    }
}
