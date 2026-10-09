//! Mouse protocols, ported from rivalcfg (flozz/rivalcfg @ 779ea52, 2026-10-07).
//! Unverified on hardware: none of these mice were available while writing this.

use std::thread::sleep;
use std::time::Duration;

use hidapi::HidDevice;

use super::hid::{read_until, unprefix};
use super::{Battery, Device, Features, Link, Setting};

#[path = "dpi_tables.rs"]
mod tables;
use tables::{TRUEMOVE_AIR, TRUEMOVE_CORE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dpi {
    /// truemove_core nearest-table lookup, 1 byte.
    Core,
    /// truemove_air nearest-table lookup, 1 byte.
    Air,
    /// truemove_air lookup, 2 bytes LE (Rival 3 Wireless).
    Air16,
    /// DPI / 50, 2 bytes LE (Prime family).
    Div50,
}

impl Dpi {
    fn range(self) -> (u16, u16, u16) {
        match self {
            Dpi::Core => (200, 8500, 100),
            Dpi::Air | Dpi::Air16 => (100, 18000, 100),
            Dpi::Div50 => (50, 18000, 50),
        }
    }

    fn encode(self, dpi: u16) -> Vec<u8> {
        // Nearest table entry; ties go to the lower DPI (rivalcfg scans ascending, first minimum wins).
        let nearest = |t: &[(u8, u8)]| t.iter().min_by_key(|(k, _)| (i32::from(*k) * 100 - i32::from(dpi)).abs()).map_or(0, |(_, v)| *v);
        match self {
            Dpi::Core => vec![nearest(&TRUEMOVE_CORE)],
            Dpi::Air => vec![nearest(&TRUEMOVE_AIR)],
            Dpi::Air16 => u16::from(nearest(&TRUEMOVE_AIR)).to_le_bytes().to_vec(),
            Dpi::Div50 => {
                let (lo, hi, step) = self.range();
                let v = dpi.clamp(lo, hi);
                let rounded = (v + step / 2) / step * step; // rivalcfg rounds half up
                (rounded / 50).to_le_bytes().to_vec()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouse {
    /// Aerox 3/5/9 Wireless, Prime Wireless, Prime Mini Wireless. On the 2.4 GHz dongle every
    /// command's first byte gets 0x40 and each write is followed by a 64-byte read.
    AeroxWireless { dongle: bool, zones: u8 },
    /// Wired-only Aerox 3/5 and Prime Mini (zones 1 = single `21 00` colour).
    AeroxWired { dpi: Dpi, first_preset: u8, zones: u8 },
    Rival3,
    Rival3Wireless,
    Prime,
}

/// Polling rate codes are 0-based on some families and 1-based on others.
fn polling_code(hz: u16, one_based: bool) -> u8 {
    let c = match hz {
        h if h >= 1000 => 0,
        h if h >= 500 => 1,
        h if h >= 250 => 2,
        _ => 3,
    };
    c + u8::from(one_based)
}

impl Mouse {
    pub fn features(self) -> Features {
        let (dpi, zones, battery) = match self {
            Mouse::AeroxWireless { zones, .. } => (Dpi::Air, zones, true),
            Mouse::AeroxWired { dpi, zones, .. } => (dpi, zones, false),
            Mouse::Rival3 => (Dpi::Core, 4, false),
            Mouse::Rival3Wireless => (Dpi::Air16, 0, true),
            Mouse::Prime => (Dpi::Div50, 1, false),
        };
        let (lo, hi, step) = dpi.range();
        Features { battery, dpi_min: lo, dpi_max: hi, dpi_step: step, polling: true, color_zones: zones, ..Features::default() }
    }

    fn flag(self) -> u8 {
        if matches!(self, Mouse::AeroxWireless { dongle: true, .. }) { 0x40 } else { 0 }
    }

    pub fn read(self, dev: &HidDevice, d: &mut Device) {
        d.features = self.features();
        d.link = Link::Online;
        let battery = match self {
            Mouse::AeroxWireless { .. } => {
                let cmd = 0x92 | self.flag();
                dev.write(&[0x00, cmd]).ok().and_then(|_| read_until(dev, 300, |r| r.len() >= 2)).map(|r| {
                    let r = unprefix(&r, cmd).unwrap_or(&r).to_vec();
                    ((i32::from(r[1] & 0x7f) - 1) * 5, r[1] & 0x80 != 0)
                })
            }
            Mouse::Rival3Wireless => {
                dev.write(&[0x00, 0xaa, 0x01]).ok().and_then(|_| read_until(dev, 300, |r| r.len() >= 3)).map(|r| (i32::from(r[0]), r[2] != 0))
            }
            _ => return,
        };
        match battery {
            Some((level, charging)) if (0..=100).contains(&level) => d.battery = Some(Battery { percent: level as u8, charging }),
            // Out of range: the mouse is off/asleep behind its dongle.
            Some(_) => d.link = Link::Asleep,
            None => d.link = Link::NotResponding,
        }
    }

    pub fn apply(self, dev: &HidDevice, s: &Setting) -> anyhow::Result<()> {
        let f = self.flag();
        let mut packets = self.packets(s)?;
        packets.push(match self {
            Mouse::AeroxWireless { .. } => vec![0x11 | f, 0x00],
            Mouse::AeroxWired { .. } => vec![0x11, 0x00],
            Mouse::Rival3 => vec![0x09, 0x00],
            Mouse::Rival3Wireless => vec![0x09],
            Mouse::Prime => vec![0x59],
        });
        for p in packets {
            let mut buf = vec![0x00];
            buf.extend(p);
            dev.write(&buf)?;
            if f != 0 {
                let _ = read_until(dev, 200, |_| true);
            }
            // rivalcfg waits 50 ms between commands; some mice hang otherwise.
            sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    fn packets(self, s: &Setting) -> anyhow::Result<Vec<Vec<u8>>> {
        let f = self.flag();
        let mut packets: Vec<Vec<u8>> = Vec::new();
        match s {
            Setting::Dpi(stages) => {
                anyhow::ensure!((1..=5).contains(&stages.len()), "1 to 5 DPI stages");
                let (cmd, dpi, first): (Vec<u8>, Dpi, u8) = match self {
                    Mouse::AeroxWireless { .. } => (vec![0x2d | f], Dpi::Air, 0),
                    Mouse::AeroxWired { dpi, first_preset, .. } => (vec![0x2d], dpi, first_preset),
                    Mouse::Rival3 => (vec![0x0b, 0x00], Dpi::Core, 1),
                    Mouse::Rival3Wireless => (vec![0x20], Dpi::Air16, 1),
                    Mouse::Prime => (vec![0x61], Dpi::Div50, 0),
                };
                let mut p = cmd;
                p.push(stages.len() as u8);
                p.push(first);
                for d in stages {
                    p.extend(dpi.encode(*d));
                }
                packets.push(p);
            }
            Setting::Polling(hz) => packets.push(match self {
                Mouse::AeroxWireless { .. } => vec![0x2b | f, polling_code(*hz, false)],
                Mouse::AeroxWired { .. } => vec![0x2b, polling_code(*hz, true)],
                Mouse::Rival3 => vec![0x04, 0x00, polling_code(*hz, true)],
                Mouse::Rival3Wireless => vec![0x17, polling_code(*hz, false)],
                Mouse::Prime => vec![0x5d, polling_code(*hz, true)],
            }),
            Setting::Lighting { rgb, brightness } => {
                let [r, g, b] = rgb.map(|c| (u16::from(c) * u16::from((*brightness).min(100)) / 100) as u8);
                let (r, g, b) = (&r, &g, &b);
                match self {
                Mouse::AeroxWireless { zones, .. } => {
                    for z in 0..zones {
                        packets.push(vec![0x21 | f, 0x01, z, *r, *g, *b]);
                    }
                }
                Mouse::AeroxWired { zones: 1, .. } => packets.push(vec![0x21, 0x00, *r, *g, *b]),
                Mouse::AeroxWired { zones, .. } => {
                    for z in 0..zones {
                        // Zone mask, then 3 zero bytes per earlier zone, then RGB.
                        let mut p = vec![0x21, 1 << z];
                        p.extend(std::iter::repeat_n(0u8, 3 * usize::from(z)));
                        p.extend([*r, *g, *b]);
                        packets.push(p);
                    }
                }
                Mouse::Rival3 => {
                    for z in 1..=4 {
                        packets.push(vec![0x05, 0x00, z, *r, *g, *b, 0x64]);
                    }
                }
                Mouse::Prime => {
                    let mut p = vec![0x62, 0x01, *r, *g, *b];
                    p.extend([0u8; 15]);
                    p.push(0xff);
                    packets.push(p);
                }
                Mouse::Rival3Wireless => anyhow::bail!("no lighting control on this mouse"),
                }
            }
            _ => anyhow::bail!("not supported on this mouse"),
        }
        Ok(packets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_encoding_matches_rivalcfg_examples() {
        let air: Vec<u8> = [400, 800, 1200, 2400, 3200].iter().flat_map(|d| Dpi::Air.encode(*d)).collect();
        assert_eq!(air, [0x04, 0x09, 0x0d, 0x1b, 0x26]);
        assert_eq!([Dpi::Core.encode(800), Dpi::Core.encode(1600)].concat(), [0x12, 0x24]);
        assert_eq!([Dpi::Air16.encode(400), Dpi::Air16.encode(800)].concat(), [0x04, 0x00, 0x09, 0x00]);
        assert_eq!([Dpi::Div50.encode(400), Dpi::Div50.encode(800)].concat(), [0x08, 0x00, 0x10, 0x00]);
    }

    #[test]
    fn full_packets_match_rivalcfg_examples() {
        let aerox = Mouse::AeroxWireless { dongle: false, zones: 3 };
        assert_eq!(aerox.packets(&Setting::Dpi(vec![400, 800, 1200, 2400, 3200])).unwrap()[0], [0x2d, 0x05, 0x00, 0x04, 0x09, 0x0d, 0x1b, 0x26]);
        let dongle = Mouse::AeroxWireless { dongle: true, zones: 3 };
        assert_eq!(dongle.packets(&Setting::Lighting { rgb: [0xff, 0, 0], brightness: 100 }).unwrap()[0], [0x61, 0x01, 0x00, 0xff, 0x00, 0x00]);
        assert_eq!(Mouse::Rival3.packets(&Setting::Dpi(vec![800, 1600])).unwrap()[0], [0x0b, 0x00, 0x02, 0x01, 0x12, 0x24]);
        assert_eq!(Mouse::Rival3.packets(&Setting::Lighting { rgb: [0xff, 0, 0], brightness: 100 }).unwrap()[0], [0x05, 0x00, 0x01, 0xff, 0x00, 0x00, 0x64]);
        assert_eq!(Mouse::Rival3Wireless.packets(&Setting::Dpi(vec![400, 800])).unwrap()[0], [0x20, 0x02, 0x01, 0x04, 0x00, 0x09, 0x00]);
        assert_eq!(Mouse::Prime.packets(&Setting::Dpi(vec![400, 800])).unwrap()[0], [0x61, 0x02, 0x00, 0x08, 0x00, 0x10, 0x00]);
    }
}
