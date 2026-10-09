//! Windows Core Audio: endpoints, default device control, mic mute, and ChatMix
//! (per-app session volumes driven by the headset dial, in place of Sonar).
//!
//! All calls assume COM is initialised on the calling thread ([`init_com`]).

use std::ffi::c_void;

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, EDataFlow, ERole, IAudioSessionControl2, IAudioSessionManager2, IMMDevice,
    IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, eCapture, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ};
use windows::core::{HSTRING, Interface};

use crate::config::EndpointRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Output,
    Input,
}

impl Flow {
    fn data_flow(self) -> EDataFlow {
        match self {
            Flow::Output => eRender,
            Flow::Input => eCapture,
        }
    }
}

/// Console, Multimedia, Communications. Meetings use the last one, which is the
/// role GG/Sonar kept hijacking, so all three are always enforced together.
pub const ROLES: [ERole; 3] = [ERole(0), ERole(1), ERole(2)];

pub fn init_com() {
    // S_FALSE (already initialised) is fine.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn id_of(dev: &IMMDevice) -> windows::core::Result<String> {
    unsafe {
        let p = dev.GetId()?;
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const c_void));
        Ok(s)
    }
}

/// Active endpoints for a flow, with friendly names.
pub fn endpoints(flow: Flow) -> Vec<EndpointRef> {
    let run = || -> windows::core::Result<Vec<EndpointRef>> {
        let coll = unsafe { enumerator()?.EnumAudioEndpoints(flow.data_flow(), DEVICE_STATE_ACTIVE)? };
        let mut out = Vec::new();
        for i in 0..unsafe { coll.GetCount()? } {
            let dev = unsafe { coll.Item(i)? };
            let name = unsafe { dev.OpenPropertyStore(STGM_READ)?.GetValue(&PKEY_Device_FriendlyName)? }.to_string();
            out.push(EndpointRef { id: id_of(&dev)?, name });
        }
        Ok(out)
    };
    run().unwrap_or_else(|e| {
        tracing::warn!("listing audio endpoints: {e}");
        Vec::new()
    })
}

pub fn default_id(flow: Flow, role: ERole) -> Option<String> {
    let dev = unsafe { enumerator().ok()?.GetDefaultAudioEndpoint(flow.data_flow(), role).ok()? };
    id_of(&dev).ok()
}

// Undocumented but stable since Vista; every "set default device" tool uses it.
mod policy {
#![allow(non_snake_case)]
use std::ffi::c_void;
use windows::Win32::Media::Audio::ERole;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::{GUID, HRESULT, HSTRING, IUnknown, IUnknown_Vtbl, PCWSTR, interface};

#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
pub unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, id: PCWSTR, fmt: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, id: PCWSTR, default: i32, fmt: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, def: *mut i64, min: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
}

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

pub fn set_default(id: &str, role: ERole) -> windows::core::Result<()> {
    unsafe {
        let pc: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)?;
        let id = HSTRING::from(id);
        pc.SetDefaultEndpoint(PCWSTR(id.as_ptr()), role).ok()
    }
}
}
pub use policy::set_default;

fn device(id: &str) -> windows::core::Result<IMMDevice> {
    unsafe { enumerator()?.GetDevice(&HSTRING::from(id)) }
}

pub fn muted(id: &str) -> bool {
    let run = || -> windows::core::Result<bool> {
        let vol: IAudioEndpointVolume = unsafe { device(id)?.Activate(CLSCTX_ALL, None)? };
        Ok(unsafe { vol.GetMute()? }.as_bool())
    };
    run().unwrap_or(false)
}

pub fn set_muted(id: &str, on: bool) -> windows::core::Result<()> {
    unsafe {
        let vol: IAudioEndpointVolume = device(id)?.Activate(CLSCTX_ALL, None)?;
        vol.SetMute(on, std::ptr::null())
    }
}

/// "Headphones (4- Arctis 7+)" and "Headphones (7- Arctis 7+)" are the same device on another USB port.
pub fn same_device(a: &str, b: &str) -> bool {
    fn strip(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_ascii_digit() {
                let mut digits = c.to_string();
                while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                    digits.push(*d);
                    chars.next();
                }
                let rest: String = chars.clone().take(3).collect();
                if let Some(skip) = ["- ", " - "].iter().find(|p| rest.starts_with(**p)) {
                    for _ in 0..skip.len() {
                        chars.next();
                    }
                    continue;
                }
                out.push_str(&digits);
                continue;
            }
            out.push(c);
        }
        out
    }
    strip(a) == strip(b)
}

/// Resolve a saved preference against what's plugged in now.
pub fn resolve<'a>(eps: &'a [EndpointRef], want: &EndpointRef) -> Option<&'a EndpointRef> {
    eps.iter().find(|e| e.id == want.id).or_else(|| eps.iter().find(|e| same_device(&e.name, &want.name)))
}

/// Set every app's volume on `output`: chat apps to `chat`, everything else to `game` (0.0..=1.0).
/// System sounds are left alone.
pub fn apply_chatmix(output: &str, game: f32, chat: f32, is_chat: impl Fn(&str) -> bool) -> windows::core::Result<()> {
    unsafe {
        let mgr: IAudioSessionManager2 = device(output)?.Activate(CLSCTX_ALL, None)?;
        let sessions = mgr.GetSessionEnumerator()?;
        for i in 0..sessions.GetCount()? {
            let Ok(ctl) = sessions.GetSession(i).and_then(|s| s.cast::<IAudioSessionControl2>()) else { continue };
            if ctl.IsSystemSoundsSession().0 == 0 {
                continue; // S_OK means "is system sounds"
            }
            let pid = ctl.GetProcessId().unwrap_or(0);
            if pid == 0 {
                continue;
            }
            let name = crate::system::process_name(pid).unwrap_or_default();
            let level = if is_chat(&name) { chat } else { game };
            if let Ok(vol) = ctl.cast::<ISimpleAudioVolume>() {
                let _ = vol.SetMasterVolume(level.clamp(0.0, 1.0), std::ptr::null());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_port_prefix_is_ignored() {
        assert!(same_device("Headphones (4- Arctis 7+)", "Headphones (7- Arctis 7+)"));
        assert!(same_device("5 - LG ULTRAWIDE (AMD)", "5 - LG ULTRAWIDE (AMD)"));
        assert!(!same_device("Headphones (4- Arctis 7+)", "Microphone (4- Arctis 7+)"));
        // Model numbers stay significant.
        assert!(!same_device("Headphones (4- Arctis 7+)", "Headphones (4- Arctis 9)"));
    }
}
