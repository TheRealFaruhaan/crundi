//! The PC lock's system-wide keys (app/main.js 401-406, globalShortcut):
//! Ctrl+Shift+L the visible lock, Super+Shift+L the full one, wherever the
//! keys are. Linux under X11: a key grab on the root window (not under
//! Wayland, as Electron); Windows: RegisterHotKey. A key that another app
//! holds is logged ("taken by another app") and the in-window binding
//! (lib.rs bind_keys) still works.
//!
//! The keys are heard on a thread of their own; each press is queued and
//! the area's pump acts on it (`take`), as the tray's picks.

use std::sync::Mutex;

static PRESSED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
/// (accelerator, registered) for the driver.
static REGISTERED: Mutex<Vec<(&'static str, bool)>> = Mutex::new(Vec::new());

fn pressed(mode: &'static str) { PRESSED.lock().unwrap().push(mode); crundi_webview::wake(); }

/// The presses since the last pump: "visible" or "full".
pub fn take() -> Vec<&'static str> { std::mem::take(&mut *PRESSED.lock().unwrap()) }
pub fn has_presses() -> bool { !PRESSED.lock().unwrap().is_empty() }

fn registered(acc: &'static str, ok: bool) {
    if !ok { crundi_shell::lifecycle::log_line(&format!("[lock] {acc} is taken by another app")); }
    REGISTERED.lock().unwrap().push((acc, ok));
}

/// What the driver sees: each accelerator and whether it is held.
pub fn props() -> serde_json::Value {
    serde_json::json!(REGISTERED.lock().unwrap().iter().map(|(a, ok)| serde_json::json!({ "accelerator": a, "registered": ok })).collect::<Vec<_>>())
}

/// Register both (once, at start).
pub fn start() { platform::start(); }

#[cfg(target_os = "linux")]
mod platform {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};
    use x11rb::protocol::Event;

    const XK_L: u32 = 0x006c;

    pub fn start() {
        // Not under Wayland (no global keys there); nor with no X display.
        if std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland") && std::env::var_os("WAYLAND_DISPLAY").is_some() { return; }
        if std::env::var_os("DISPLAY").is_none() { return; }
        std::thread::Builder::new().name("global-keys".into()).spawn(run).ok();
    }

    fn run() {
        let Ok((conn, screen)) = x11rb::connect(None) else { return };
        let root = conn.setup().roots[screen].root;
        let (min, max) = (conn.setup().min_keycode, conn.setup().max_keycode);
        let Some(map) = conn.get_keyboard_mapping(min, max - min + 1).ok().and_then(|c| c.reply().ok()) else { return };
        let per = map.keysyms_per_keycode as usize;
        let Some(code) = map.keysyms.chunks(per.max(1)).position(|k| k.contains(&XK_L)).map(|i| min + i as u8) else { return };
        // Each also with Caps Lock and Num Lock on (they are modifiers to X).
        let extra = [ModMask::from(0u16), ModMask::LOCK, ModMask::M2, ModMask::LOCK | ModMask::M2];
        for (acc, mods) in [("CommandOrControl+Shift+L", ModMask::CONTROL | ModMask::SHIFT), ("Super+Shift+L", ModMask::M4 | ModMask::SHIFT)] {
            let ok = extra.iter().all(|e| conn.grab_key(true, root, mods | *e, code, GrabMode::ASYNC, GrabMode::ASYNC).ok().and_then(|c| c.check().ok()).is_some());
            super::registered(acc, ok);
        }
        let _ = conn.flush();
        while let Ok(ev) = conn.wait_for_event() {
            if let Event::KeyPress(k) = ev {
                if k.detail != code { continue; }
                let st = u16::from(k.state);
                let (ctrl, sup) = (st & u16::from(ModMask::CONTROL) != 0, st & u16::from(ModMask::M4) != 0);
                if sup && !ctrl { super::pressed("full"); } else if ctrl && !sup { super::pressed("visible"); }
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    pub fn start() {
        std::thread::Builder::new().name("global-keys".into()).spawn(|| unsafe {
            let l = 'L' as u32;
            let a = RegisterHotKey(std::ptr::null_mut(), 1, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, l) != 0;
            super::registered("CommandOrControl+Shift+L", a);
            let b = RegisterHotKey(std::ptr::null_mut(), 2, MOD_WIN | MOD_SHIFT | MOD_NOREPEAT, l) != 0;
            super::registered("Super+Shift+L", b);
            if !a && !b { return; }
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY { super::pressed(if msg.wParam == 2 { "full" } else { "visible" }); }
            }
        }).ok();
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod platform {
    pub fn start() {}
}
