//! Windows odds and ends: processes, autostart, driver status, single instance.

use std::path::Path;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_DEVNODE_STATUS_FLAGS, CM_Get_DevNode_Status, CM_PROB, DIGCF_ALLCLASSES, DIGCF_PRESENT, SP_DEVINFO_DATA,
    SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, MAX_PATH};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, INFINITE, OpenEventW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW, SYNCHRONIZATION_SYNCHRONIZE, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::core::{HSTRING, PWSTR, w};

/// Executable stem of a process ("Discord"), if we may query it.
pub fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| {
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
        })
    }
}

pub struct Proc {
    pub name: String,
    /// Working set in MB; None for processes we can't open (services).
    pub mb: Option<u64>,
}

/// Running SteelSeries processes (GG, Sonar, Moments...).
pub fn steelseries_processes() -> Vec<Proc> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]);
            if name.to_ascii_lowercase().starts_with("steelseries") {
                let mut mb = None;
                if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, e.th32ProcessID) {
                    let mut c = PROCESS_MEMORY_COUNTERS::default();
                    if GetProcessMemoryInfo(h, &mut c, size_of::<PROCESS_MEMORY_COUNTERS>() as u32).is_ok() {
                        mb = Some(c.WorkingSetSize as u64 >> 20);
                    }
                    let _ = CloseHandle(h);
                }
                out.push(Proc { name, mb });
            }
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

pub struct PnpDevice {
    pub instance_id: String,
    pub name: String,
    /// Device Manager problem code; 0 = working.
    pub problem: u32,
}

/// Present device nodes whose instance id contains `needle` (e.g. "VID_1038&PID_220E").
pub fn pnp_devices(needle: &str) -> Vec<PnpDevice> {
    let needle = needle.to_ascii_uppercase();
    let mut out = Vec::new();
    unsafe {
        let Ok(set) = SetupDiGetClassDevsW(None, None, None, DIGCF_ALLCLASSES | DIGCF_PRESENT) else { return out };
        let mut i = 0;
        loop {
            let mut d = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInfo(set, i, &mut d).is_err() {
                break;
            }
            i += 1;
            let mut buf = [0u16; 512];
            if SetupDiGetDeviceInstanceIdW(set, &d, Some(&mut buf), None).is_err() {
                continue;
            }
            let id = utf16(&buf);
            if !id.to_ascii_uppercase().contains(&needle) {
                continue;
            }
            let mut name = String::new();
            for prop in [SPDRP_FRIENDLYNAME, SPDRP_DEVICEDESC] {
                let mut raw = [0u8; 512];
                if SetupDiGetDeviceRegistryPropertyW(set, &d, prop, None, Some(&mut raw), None).is_ok() {
                    let wide: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                    name = utf16(&wide);
                    break;
                }
            }
            let (mut status, mut problem) = (CM_DEVNODE_STATUS_FLAGS(0), CM_PROB(0));
            let _ = CM_Get_DevNode_Status(&mut status, &mut problem, d.DevInst, 0);
            out.push(PnpDevice { instance_id: id, name, problem: problem.0 });
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    out
}

fn utf16(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Restart a USB device (same as unplug/replug). Shows a UAC prompt.
pub fn restart_device(instance_id: &str) -> anyhow::Result<()> {
    let args = HSTRING::from(format!("/restart-device \"{instance_id}\""));
    let r = unsafe { ShellExecuteW(None, w!("runas"), w!("pnputil.exe"), &args, None, SW_HIDE) };
    anyhow::ensure!(r.0 as isize > 32, "elevation was declined");
    Ok(())
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

pub fn autostart() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(crate::APP_NAME))
        .is_ok()
}

pub fn set_autostart(on: bool) -> anyhow::Result<()> {
    let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
    if on {
        let exe = std::env::current_exe()?;
        key.set_value(crate::APP_NAME, &format!("\"{}\" --tray", exe.display()))?;
    } else {
        let _ = key.delete_value(crate::APP_NAME);
    }
    Ok(())
}

/// Process-wide named mutex. Returns None if another instance holds `name`.
pub struct Instance(HANDLE);

impl Instance {
    pub fn acquire(name: &str) -> Option<Self> {
        unsafe {
            let h = CreateMutexW(None, false, &HSTRING::from(format!("Local\\{name}"))).ok()?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(h);
                return None;
            }
            Some(Self(h))
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Cross-process "please show your window" signal.
pub fn signal(name: &str) -> bool {
    unsafe {
        match OpenEventW(EVENT_MODIFY_STATE, false, &HSTRING::from(format!("Local\\{name}"))) {
            Ok(h) => {
                let ok = SetEvent(h).is_ok();
                let _ = CloseHandle(h);
                ok
            }
            Err(_) => false,
        }
    }
}

/// Block forever, calling `f` each time [`signal`] fires.
pub fn on_signal(name: &str, mut f: impl FnMut()) {
    unsafe {
        let Ok(h) = CreateEventW(None, false, false, &HSTRING::from(format!("Local\\{name}"))) else { return };
        let _ = SYNCHRONIZATION_SYNCHRONIZE;
        while WaitForSingleObject(h, INFINITE).0 == 0 {
            f();
        }
    }
}
