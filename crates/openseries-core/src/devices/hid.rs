//! Small HID helpers shared by every protocol.

use std::time::{Duration, Instant};

use hidapi::HidDevice;

/// Output report, zero-padded to `len` bytes (report id included).
pub fn out(dev: &HidDevice, bytes: &[u8], len: usize) -> anyhow::Result<()> {
    let mut buf = bytes.to_vec();
    if buf.len() < len {
        buf.resize(len, 0);
    }
    dev.write(&buf)?;
    Ok(())
}

/// Feature report, zero-padded to `len` bytes (report id included).
pub fn feature(dev: &HidDevice, bytes: &[u8], len: usize) -> anyhow::Result<()> {
    let mut buf = bytes.to_vec();
    if buf.len() < len {
        buf.resize(len, 0);
    }
    dev.send_feature_report(&buf)?;
    Ok(())
}

/// Read input reports until `accept` says yes or `ms` pass.
pub fn read_until(dev: &HidDevice, ms: u64, accept: impl Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_millis(ms);
    let mut buf = [0u8; 128];
    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now()).as_millis() as i32;
        let n = dev.read_timeout(&mut buf, left.max(1)).ok()?;
        if n > 0 && accept(&buf[..n]) {
            return Some(buf[..n].to_vec());
        }
    }
    None
}

/// Drop anything queued (replies we don't need, async status reports).
pub fn drain(dev: &HidDevice, ms: i32) {
    let mut buf = [0u8; 128];
    while matches!(dev.read_timeout(&mut buf, ms), Ok(n) if n > 0) {}
}

/// Strip a leading zero report id some backends keep, so replies start with the command echo.
pub fn unprefix(r: &[u8], cmd: u8) -> Option<&[u8]> {
    if r.first() == Some(&cmd) {
        Some(r)
    } else if r.len() > 1 && r[0] == 0 && r[1] == cmd {
        Some(&r[1..])
    } else {
        None
    }
}

/// HeadsetControl's `map()`: clamp, then integer linear map.
pub fn map(v: i32, in_min: i32, in_max: i32, out_min: i32, out_max: i32) -> i32 {
    let v = v.clamp(in_min.min(in_max), in_min.max(in_max));
    out_min + (v - in_min) * (out_max - out_min) / (in_max - in_min)
}
