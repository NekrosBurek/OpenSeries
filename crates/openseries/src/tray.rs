//! Notification-area icon on the agent's main thread, plus its Win32 message pump.

use std::sync::mpsc::{Receiver, Sender};

use openseries_core::devices::Device;
use openseries_core::ipc::Command;
use openseries_core::system::Instance;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

pub enum TrayMsg {
    Update { tooltip: String, device: Option<Device> },
    /// A new build was installed next to us: hand over and exit.
    Restart,
    Quit,
}

fn icon(d: Option<&Device>) -> Option<Icon> {
    let (rgba, size) = crate::icon::tray(d);
    Icon::from_rgba(rgba, size, size).ok()
}

pub fn run(rx: Receiver<TrayMsg>, _cmds: Sender<Command>, lock: Instance) -> anyhow::Result<()> {
    let status = MenuItem::new("Starting…", false, None);
    let open = MenuItem::new("Open OpenSeries", true, None);
    let quit = MenuItem::new("Quit OpenSeries", true, None);
    let menu = Menu::new();
    menu.append_items(&[&status, &PredefinedMenuItem::separator(), &open, &quit])?;
    let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_menu_on_left_click(false).with_tooltip("OpenSeries");
    if let Some(i) = icon(None) {
        builder = builder.with_icon(i);
    }
    let tray = builder.build()?;

    loop {
        pump(100);
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                crate::spawn_ui();
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == *open.id() {
                crate::spawn_ui();
            } else if ev.id == *quit.id() {
                return Ok(());
            }
        }
        while let Ok(msg) = rx.try_recv() {
            match msg {
                TrayMsg::Update { tooltip, device } => {
                    let _ = tray.set_tooltip(Some(&tooltip));
                    status.set_text(&tooltip);
                    let _ = tray.set_icon(icon(device.as_ref()));
                }
                TrayMsg::Restart => {
                    drop(tray);
                    drop(lock); // let the new build take the single-instance lock
                    if let Ok(exe) = std::env::current_exe() {
                        let _ = std::process::Command::new(exe).arg("--tray").spawn();
                    }
                    return Ok(());
                }
                TrayMsg::Quit => return Ok(()),
            }
        }
    }
}

/// Dispatch pending window messages, waiting up to `ms` for new ones.
#[cfg(windows)]
fn pump(ms: u32) {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT, TranslateMessage,
    };
    unsafe {
        MsgWaitForMultipleObjects(None, false, ms, QS_ALLINPUT);
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(not(windows))]
fn pump(ms: u32) {
    std::thread::sleep(std::time::Duration::from_millis(u64::from(ms)));
}
