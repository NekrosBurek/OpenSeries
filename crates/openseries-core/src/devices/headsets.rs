//! Headset protocols, ported from HeadsetControl (Sapd/HeadsetControl @ d0da29a, 2026-10-02).
//! Byte layouts are copied from source; only the Arctis 7+ is verified on hardware so far.
//!
//! UI-level values: sidetone 0..=3 (Off/Low/Medium/High), auto-off in minutes, EQ in dB.

use hidapi::HidDevice;

use super::hid::{self, map, out, read_until, unprefix};
use super::{Battery, ChatMix, Device, Features, Link, Setting};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Headset {
    /// Arctis 7+ family: status `b0` = [b0, link(1 off), batt 0..4, charging, game, chat]. Saves with `00 09`.
    ArctisPlus,
    /// Arctis Nova 7 family: status [b0, ?, level, state(0 off, 1/2 charging), game, chat]. No save.
    Nova7 { discrete: bool, chat_and_sidetone: bool, sidetone_save: bool },
    /// Arctis Nova 5 / 5X base station.
    Nova5,
    /// Arctis Nova Pro Wireless base station (legacy `06` reports).
    NovaPro,
    /// Arctis 7 (2017/2019), Arctis Pro 2019, Pro GameDAC.
    Arctis7Legacy,
    /// Arctis 1 Wireless, 7X, 7P (legacy `06`, 31-byte reports).
    Arctis1Legacy,
}

impl Headset {
    pub fn features(self) -> Features {
        let base = Features { battery: true, auto_off: true, ..Features::default() };
        match self {
            Headset::ArctisPlus => Features { chatmix: true, sidetone: true, equalizer: true, eq_max_db: 12, ..base },
            Headset::Nova7 { chat_and_sidetone, .. } => {
                Features { chatmix: chat_and_sidetone, sidetone: chat_and_sidetone, equalizer: true, eq_max_db: 10, ..base }
            }
            Headset::Nova5 => Features { chatmix: true, sidetone: true, equalizer: true, eq_max_db: 10, ..base },
            Headset::NovaPro => Features { sidetone: true, equalizer: true, eq_max_db: 10, ..base },
            Headset::Arctis7Legacy => Features { chatmix: true, sidetone: true, ..base },
            Headset::Arctis1Legacy => Features { sidetone: true, ..base },
        }
    }

    pub fn read(self, dev: &HidDevice, d: &mut Device) {
        d.link = Link::NotResponding;
        match self {
            Headset::ArctisPlus => {
                let Some(r) = status_b0(dev) else { return };
                d.raw = r.clone();
                if r[1] == 0x01 {
                    d.link = Link::Asleep;
                    return;
                }
                d.link = Link::Online;
                d.battery = Some(Battery { percent: r[2].min(4) * 25, charging: r[3] == 0x01 });
                d.chatmix = Some(ChatMix { game: r[4].min(100), chat: r[5].min(100) });
            }
            Headset::Nova7 { discrete, chat_and_sidetone, .. } => {
                let Some(r) = status_b0(dev) else { return };
                d.raw = r.clone();
                if r[3] == 0x00 {
                    d.link = Link::Asleep;
                    return;
                }
                d.link = Link::Online;
                let level = if discrete { map(i32::from(r[2]), 0, 4, 0, 100) as u8 } else { r[2].min(100) };
                d.battery = Some(Battery { percent: level, charging: matches!(r[3], 0x01 | 0x02) });
                if chat_and_sidetone {
                    d.chatmix = Some(ChatMix { game: r[4].min(100), chat: r[5].min(100) });
                }
            }
            Headset::Nova5 => {
                let Some(r) = status_b0(dev).filter(|r| r.len() >= 7) else { return };
                d.raw = r.clone();
                if r[1] == 0x02 {
                    d.link = Link::Asleep;
                    return;
                }
                d.link = Link::Online;
                d.battery = Some(Battery { percent: r[3].min(100), charging: r[4] == 0x01 });
                d.chatmix = Some(ChatMix { game: r[5].min(100), chat: r[6].min(100) });
            }
            Headset::NovaPro => {
                if out(dev, &[0x06, 0xb0], 32).is_err() {
                    return;
                }
                let Some(r) = read_until(dev, 600, |r| r.len() >= 16 && r[0] == 0x06 && r[1] == 0xb0) else { return };
                d.raw = r.clone();
                match r[15] {
                    0x01 => d.link = Link::Asleep,
                    s => {
                        d.link = Link::Online;
                        d.battery = Some(Battery { percent: map(i32::from(r[6]), 0, 8, 0, 100) as u8, charging: s == 0x02 });
                    }
                }
            }
            Headset::Arctis7Legacy => {
                let Some(b) = legacy_query(dev, 0x18) else { return };
                d.raw = b.clone();
                d.link = Link::Online;
                d.battery = Some(Battery { percent: b[2].min(100), charging: false });
                if let Some(c) = legacy_query(dev, 0x24) {
                    // 191..255, 255 = neutral (100%), 0 = that side maxed.
                    let pct = |v: u8| if v == 0 { 100 } else { map(i32::from(v), 191, 255, 0, 100) as u8 };
                    d.chatmix = Some(ChatMix { game: pct(c[2]), chat: pct(c[3]) });
                }
            }
            Headset::Arctis1Legacy => {
                let Some(b) = legacy_query(dev, 0x12) else { return };
                d.raw = b.clone();
                if b[2] == 0x01 {
                    d.link = Link::Asleep;
                    return;
                }
                d.link = Link::Online;
                d.battery = Some(Battery { percent: b[3].min(100), charging: false });
            }
        }
        d.features = self.features();
    }

    pub fn apply(self, dev: &HidDevice, s: &Setting) -> anyhow::Result<()> {
        for packet in self.packets(s)? {
            match packet {
                Packet::Out(bytes, len) => out(dev, &bytes, len)?,
            }
        }
        Ok(())
    }

    fn packets(self, s: &Setting) -> anyhow::Result<Vec<Packet>> {
        let nova = |b: Vec<u8>| Packet::Out(b, 65);
        let legacy = |b: Vec<u8>| Packet::Out(b, 32);
        let mut p = Vec::new();
        match (self, s) {
            (Headset::ArctisPlus, Setting::Sidetone(l)) => p.push(nova(vec![0x00, 0x39, (*l).min(3)])),
            (Headset::ArctisPlus, Setting::AutoOff(m)) => p.push(nova(vec![0x00, 0xa3, (*m).min(90)])),
            (Headset::ArctisPlus, Setting::Equalizer(b)) => p.push(nova(eq_bytes(&[0x00, 0x33], b, 0x18, 2.0, 12.0, true)?)),
            (Headset::Nova7 { chat_and_sidetone: true, sidetone_save, .. }, Setting::Sidetone(l)) => {
                p.push(nova(vec![0x00, 0x39, (*l).min(3)]));
                if sidetone_save {
                    p.push(nova(vec![0x00, 0x09]));
                }
                return Ok(p);
            }
            (Headset::Nova7 { .. }, Setting::AutoOff(m)) => return Ok(vec![nova(vec![0x00, 0xa3, *m])]),
            // Nova 7: one raw step per dB (HeadsetControl truncates half steps).
            (Headset::Nova7 { .. }, Setting::Equalizer(b)) => return Ok(vec![nova(eq_bytes(&[0x00, 0x33], b, 0x14, 1.0, 10.0, true)?)]),
            (Headset::Nova5, Setting::Sidetone(l)) => p.push(nova(vec![0x00, 0x39, [0, 3, 6, 10][usize::from((*l).min(3))]])),
            (Headset::Nova5, Setting::AutoOff(m)) => return Ok(vec![nova(vec![0x00, 0xa3, *m])]),
            (Headset::Nova5, Setting::Equalizer(b)) => p.push(nova(nova5_eq(b)?)),
            (Headset::NovaPro, Setting::Sidetone(l)) => p.push(legacy(vec![0x06, 0x39, (*l).min(3)])),
            (Headset::NovaPro, Setting::AutoOff(m)) => {
                let code = match *m {
                    0 => 0,
                    m if m >= 45 => 6,
                    m if m >= 23 => 5,
                    m if m >= 13 => 4,
                    m if m >= 8 => 3,
                    m if m >= 3 => 2,
                    _ => 1,
                };
                p.push(legacy(vec![0x06, 0xc1, code]));
            }
            (Headset::NovaPro, Setting::Equalizer(b)) => {
                // Select "Custom", then the bands. No save after a custom EQ.
                return Ok(vec![Packet::Out(vec![0x06, 0x2e, 0x04], 3), legacy(eq_bytes(&[0x06, 0x33], b, 0x14, 2.0, 10.0, false)?)]);
            }
            (Headset::Arctis7Legacy | Headset::Arctis1Legacy, Setting::Sidetone(l)) => {
                // UI level -> HeadsetControl's 0..128 -> device 0..0x12.
                let lvl = [0, 43, 85, 128][usize::from((*l).min(3))];
                p.push(legacy(if lvl == 0 { vec![0x06, 0x35] } else { vec![0x06, 0x35, 0x01, 0x00, map(lvl, 0, 128, 0, 0x12) as u8] }));
            }
            (Headset::Arctis7Legacy, Setting::AutoOff(m)) => p.push(legacy(vec![0x06, 0x51, (*m).min(90)])),
            (Headset::Arctis1Legacy, Setting::AutoOff(m)) => p.push(legacy(vec![0x06, 0x53, (*m).min(90)])),
            _ => anyhow::bail!("not supported on this headset"),
        }
        // Persist to the headset.
        match self {
            Headset::ArctisPlus => p.push(nova(vec![0x00, 0x09])),
            Headset::Nova5 => {
                p.push(nova(vec![0x00, 0x09]));
                p.push(nova(vec![0x00, 0x35, 0x01]));
            }
            Headset::NovaPro | Headset::Arctis7Legacy | Headset::Arctis1Legacy => p.push(legacy(vec![0x06, 0x09])),
            Headset::Nova7 { .. } => {}
        }
        Ok(p)
    }
}

enum Packet {
    /// Output report padded to the given length (report id included).
    Out(Vec<u8>, usize),
}

/// `00 b0` status query on the Nova-style interface; reply normalised to start with `b0`.
fn status_b0(dev: &HidDevice) -> Option<Vec<u8>> {
    dev.write(&[0x00, 0xb0]).ok()?;
    let r = read_until(dev, 800, |r| unprefix(r, 0xb0).is_some_and(|r| r.len() >= 6))?;
    unprefix(&r, 0xb0).map(<[u8]>::to_vec)
}

/// Legacy `06 cmd` request; reply indexed exactly as HeadsetControl reads it.
fn legacy_query(dev: &HidDevice, cmd: u8) -> Option<Vec<u8>> {
    hid::drain(dev, 1);
    dev.write(&[0x06, cmd]).ok()?;
    read_until(dev, 800, |r| r.len() >= 4)
}

/// `prefix` + one byte per band (`base + step*dB`, truncated) [+ trailing 0].
fn eq_bytes(prefix: &[u8], bands: &[f32], base: u8, step: f32, max_db: f32, trailing_zero: bool) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(bands.len() == 10, "equalizer needs 10 bands");
    let mut v = prefix.to_vec();
    v.extend(bands.iter().map(|b| (f32::from(base) + b.clamp(-max_db, max_db) * step) as u8));
    if trailing_zero {
        v.push(0);
    }
    Ok(v)
}

/// Nova 5: 10 × [freq LE16, flag, gain, q LE16] after `00 33`.
fn nova5_eq(bands: &[f32]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(bands.len() == 10, "equalizer needs 10 bands");
    const FREQ: [u16; 10] = [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
    let mut v = vec![0x00, 0x33];
    for (i, b) in bands.iter().enumerate() {
        let gain = (20.0 + 2.0 * b.clamp(-10.0, 10.0)) as u8;
        let flag = match i {
            0 if gain != 20 => 0x04,
            9 if gain != 20 => 0x05,
            _ => 0x01,
        };
        v.extend(FREQ[i].to_le_bytes());
        v.extend([flag, gain, 0x86, 0x05]);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(h: Headset, s: Setting) -> Vec<Vec<u8>> {
        h.packets(&s).unwrap().into_iter().map(|Packet::Out(b, _)| b).collect()
    }

    #[test]
    fn arctis_plus_matches_headsetcontrol() {
        let bass = vec![3.5, 4.0, 1.0, -1.5, -1.5, -1.0, -1.0, -1.0, -1.0, 5.5];
        let p = bytes(Headset::ArctisPlus, Setting::Equalizer(bass));
        assert_eq!(p[0], [0x00, 0x33, 0x1f, 0x20, 0x1a, 0x15, 0x15, 0x16, 0x16, 0x16, 0x16, 0x23, 0x00]);
        assert_eq!(p[1], [0x00, 0x09]);
        assert_eq!(bytes(Headset::ArctisPlus, Setting::Sidetone(9))[0], [0x00, 0x39, 0x03]);
    }

    #[test]
    fn nova7_eq_uses_one_step_per_db_and_no_save() {
        // HeadsetControl's Nova 7 "Bass" preset bytes.
        let bass = vec![3.5, 5.5, 4.0, 1.0, -1.5, -1.5, -1.0, -1.0, -1.0, -1.0];
        let h = Headset::Nova7 { discrete: false, chat_and_sidetone: true, sidetone_save: false };
        let p = bytes(h, Setting::Equalizer(bass));
        assert_eq!(p.len(), 1);
        assert_eq!(p[0][2..12], [0x17, 0x19, 0x18, 0x15, 0x12, 0x12, 0x13, 0x13, 0x13, 0x13]);
    }

    #[test]
    fn nova5_eq_layout() {
        // HeadsetControl's computed Nova 5 "Bass" gains.
        let bass = vec![3.5, 5.5, 4.0, 1.0, -1.5, -1.5, -1.0, -1.0, -1.0, -1.0];
        let p = bytes(Headset::Nova5, Setting::Equalizer(bass));
        let gains: Vec<u8> = (0..10).map(|i| p[0][2 + i * 6 + 3]).collect();
        assert_eq!(gains, [0x1b, 0x1f, 0x1c, 0x16, 0x11, 0x11, 0x12, 0x12, 0x12, 0x12]);
        assert_eq!(p[0][2..6], [32, 0, 0x04, 0x1b]); // band 0 non-flat -> flag 04
        assert_eq!(p[1..], [vec![0x00, 0x09], vec![0x00, 0x35, 0x01]]);
    }

    #[test]
    fn legacy_sidetone() {
        let p = bytes(Headset::Arctis7Legacy, Setting::Sidetone(3));
        assert_eq!(p[0], [0x06, 0x35, 0x01, 0x00, 0x12]);
        assert_eq!(p[1], [0x06, 0x09]);
        assert_eq!(bytes(Headset::Arctis1Legacy, Setting::Sidetone(0))[0], [0x06, 0x35]);
    }
}
