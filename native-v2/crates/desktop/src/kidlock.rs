//! The kid lock's OS side (app/main.js kidLockWindow, kidKeys,
//! kidKeysLinux, kidTouchpad): while kid mode or the PC lock holds the
//! window, it stays above every other window and off the taskbar, and the
//! desktop's own ways out are switched off: on Linux the GNOME shortcuts
//! (gsettings) and KDE's Meta key, on Windows the Windows key, Alt+Tab and
//! Ctrl+Esc (a small low-level keyboard filter in a hidden PowerShell, as
//! the Electron app runs) and the touchpad's three and four finger gestures.
//! The old values go to a file first, so they come back even after a crash
//! (put back at the next start). The OS keeps what it never hands to an
//! app: Ctrl+Alt+Del and Win+L, the Super key under Wayland, VT switching.
//!
//! Under the test driver (CRUNDI_TEST_PORT) the desktop's settings are never
//! touched (they are the person's own), unless CRUNDI_KID_OS_TEST=1.

use gpui::Window;
use std::path::PathBuf;
use std::process::Command;

fn settings_allowed() -> bool {
    crundi_core::driver::port().is_none() || std::env::var("CRUNDI_KID_OS_TEST").is_ok_and(|v| v == "1")
}

#[allow(dead_code)]
fn restore_file(name: &str) -> PathBuf { crundi_core::store::config_dir().join(name) }

/// The window above the others and off the taskbar (setAlwaysOnTop,
/// setSkipTaskbar), or back as it was.
pub fn window(win: &Window, on: bool) {
    #[cfg(target_os = "linux")]
    x11_state(win, on);
    #[cfg(target_os = "windows")]
    win_state(win, on);
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let _ = (win, on);
}

/// The desktop's own shortcuts and gestures off (or back), on a thread of
/// its own (the tools take a moment).
pub fn desktop(on: bool) {
    if !settings_allowed() { return; }
    std::thread::spawn(move || {
        #[cfg(target_os = "linux")]
        linux_keys(on);
        #[cfg(target_os = "windows")]
        { windows_keys(on); windows_touchpad(on); }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        let _ = on;
    });
}

/// At start: what a crash left switched off comes back.
pub fn restore_left_over() { desktop(false); }

#[allow(dead_code)]
fn sh(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ─── Linux ───

#[cfg(target_os = "linux")]
const GNOME_WM: &[&str] = &["switch-applications", "switch-applications-backward", "switch-windows", "switch-windows-backward", "switch-group", "switch-group-backward",
    "cycle-windows", "cycle-windows-backward", "cycle-group", "cycle-group-backward", "panel-main-menu", "panel-run-dialog", "show-desktop", "minimize",
    "switch-to-workspace-left", "switch-to-workspace-right", "switch-to-workspace-up", "switch-to-workspace-down", "close"];
#[cfg(target_os = "linux")]
const GNOME_SHELL: &[&str] = &["toggle-overview", "toggle-application-view", "toggle-message-tray", "toggle-quick-settings", "focus-active-notification", "shift-overview-up", "shift-overview-down"];

/// KID_GNOME_KEYS: (schema, key, the value that switches it off).
#[cfg(target_os = "linux")]
fn gnome_keys() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut v = vec![("org.gnome.mutter", "overlay-key", "''")];
    v.extend(GNOME_WM.iter().map(|k| ("org.gnome.desktop.wm.keybindings", *k, "[]")));
    v.extend(GNOME_SHELL.iter().map(|k| ("org.gnome.shell.keybindings", *k, "[]")));
    v
}

#[cfg(target_os = "linux")]
fn kde_reconfigure() { if sh("qdbus", &["org.kde.KWin", "/KWin", "reconfigure"]).is_none() { let _ = sh("qdbus6", &["org.kde.KWin", "/KWin", "reconfigure"]); } }

#[cfg(target_os = "linux")]
fn linux_keys(on: bool) {
    use serde_json::{json, Value};
    let file = restore_file("kid-keys-restore.json");
    if on {
        // Already off (or left off by a crash): kept as it was.
        if file.exists() { return; }
        let mut gnome = vec![];
        if sh("which", &["gsettings"]).is_some() {
            for (schema, key, _) in gnome_keys() {
                if let Some(cur) = sh("gsettings", &["get", schema, key]) { gnome.push(json!([schema, key, cur])); }
            }
        }
        let kw = if sh("which", &["kwriteconfig6"]).is_some() { "kwriteconfig6" } else if sh("which", &["kwriteconfig5"]).is_some() { "kwriteconfig5" } else { "" };
        let kde = (!kw.is_empty() && std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_uppercase().contains("KDE")).then(|| {
            let rd = kw.replace("write", "read");
            json!({ "tool": kw, "meta": sh(&rd, &["--file", "kwinrc", "--group", "ModifierOnlyShortcuts", "--key", "Meta"]).unwrap_or_default() })
        });
        let saved = json!({ "gnome": gnome, "kde": kde });
        let _ = std::fs::write(&file, serde_json::to_vec(&saved).unwrap_or_default());
        for (schema, key, off) in gnome_keys() {
            if saved["gnome"].as_array().is_some_and(|a| a.iter().any(|x| x[0] == schema && x[1] == key)) { let _ = sh("gsettings", &["set", schema, key, off]); }
        }
        if let Some(k) = saved.get("kde").filter(|k| !k.is_null()) {
            let tool = k["tool"].as_str().unwrap_or("");
            let _ = sh(tool, &["--file", "kwinrc", "--group", "ModifierOnlyShortcuts", "--key", "Meta", ""]);
            kde_reconfigure();
        }
        eprintln!("[kid] desktop shortcuts off ({} GNOME keys{})", saved["gnome"].as_array().map(|a| a.len()).unwrap_or(0), if saved["kde"].is_null() { "" } else { ", KDE Meta" });
    } else {
        let Ok(b) = std::fs::read(&file) else { return };
        let saved: Value = serde_json::from_slice(&b).unwrap_or(Value::Null);
        for x in saved["gnome"].as_array().cloned().unwrap_or_default() {
            if let (Some(s), Some(k), Some(v)) = (x[0].as_str(), x[1].as_str(), x[2].as_str()) { let _ = sh("gsettings", &["set", s, k, v]); }
        }
        if let Some(k) = saved.get("kde").filter(|k| !k.is_null()) {
            let tool = k["tool"].as_str().unwrap_or("");
            let _ = sh(tool, &["--file", "kwinrc", "--group", "ModifierOnlyShortcuts", "--key", "Meta", k["meta"].as_str().unwrap_or("")]);
            kde_reconfigure();
        }
        let _ = std::fs::remove_file(&file);
        eprintln!("[kid] desktop shortcuts restored");
    }
}

/// _NET_WM_STATE_ABOVE and _NET_WM_STATE_SKIP_TASKBAR (and the pager) on
/// the X11 window, asked of the window manager (EWMH).
#[cfg(target_os = "linux")]
fn x11_state(win: &Window, on: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask};
    let id = match HasWindowHandle::window_handle(win).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Xlib(h)) => h.window as u32,
        Ok(RawWindowHandle::Xcb(h)) => h.window.get(),
        _ => return,
    };
    let Ok((c, screen)) = x11rb::connect(None) else { return };
    let root = c.setup().roots[screen].root;
    let atom = |n: &str| c.intern_atom(false, n.as_bytes()).ok().and_then(|r| r.reply().ok()).map(|r| r.atom);
    let (Some(state), Some(above), Some(skip_tb), Some(skip_pg)) = (atom("_NET_WM_STATE"), atom("_NET_WM_STATE_ABOVE"), atom("_NET_WM_STATE_SKIP_TASKBAR"), atom("_NET_WM_STATE_SKIP_PAGER")) else { return };
    for (a, b) in [(above, 0), (skip_tb, skip_pg)] {
        // 1: add, 0: remove; the source 1 is an application.
        let ev = ClientMessageEvent::new(32, id, state, [u32::from(on), a, b, 1, 0]);
        let _ = c.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, ev);
    }
    let _ = c.flush();
}

/// One EWMH request to the window manager about our window (`_NET_WM_STATE`
/// add/remove of up to two states, `_NET_ACTIVE_WINDOW`…), as a client message to the root.
#[cfg(target_os = "linux")]
fn x11_ask(win: &Window, kind: &str, data: impl FnOnce(&dyn Fn(&str) -> Option<u32>) -> Option<[u32; 5]>) -> bool {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask};
    let Some(id) = x11_window(win) else { return false };
    let Ok((c, screen)) = x11rb::connect(None) else { return false };
    let root = c.setup().roots[screen].root;
    let atom = |n: &str| c.intern_atom(false, n.as_bytes()).ok().and_then(|r| r.reply().ok()).map(|r| r.atom);
    let (Some(kind), Some(d)) = (atom(kind), data(&atom)) else { return false };
    let ev = ClientMessageEvent::new(32, id, kind, d);
    let ok = c.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, ev).is_ok();
    let _ = c.flush();
    ok
}

/// The X11 window's id (None: not an X11 window).
#[cfg(target_os = "linux")]
pub fn x11_window(win: &Window) -> Option<u32> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(win).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Xlib(h)) => Some(h.window as u32),
        Ok(RawWindowHandle::Xcb(h)) => Some(h.window.get()),
        _ => None,
    }
}

/// The window held above the others (setAlwaysOnTop) or let go, alone (no taskbar change).
pub fn above(win: &Window, on: bool) {
    #[cfg(target_os = "linux")]
    { x11_ask(win, "_NET_WM_STATE", |a| Some([u32::from(on), a("_NET_WM_STATE_ABOVE")?, 0, 1, 0])); }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE};
        // SAFETY: the window's own handle, alive while `win` is.
        if let Some(h) = hwnd(win) { unsafe { SetWindowPos(h, if on { HWND_TOPMOST } else { HWND_NOTOPMOST }, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE); } }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let _ = (win, on);
}

/// Focus asked outright (app.focus({ steal: true })): on X11 as a pager
/// asks (source 2), which GNOME's focus-stealing prevention lets through.
pub fn steal_focus(win: &Window) {
    #[cfg(target_os = "linux")]
    { x11_ask(win, "_NET_ACTIVE_WINDOW", |_| Some([2, 0, 0, 0, 0])); }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
        // SAFETY: the window's own handle.
        if let Some(h) = hwnd(win) { unsafe { SetForegroundWindow(h); } }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let _ = win;
}

/// Full screen asked (setFullScreen(true): never a toggle, so a request the
/// window manager has not answered yet is not undone by the next one).
pub fn fullscreen_on(win: &mut Window) {
    #[cfg(target_os = "linux")]
    { if x11_ask(win, "_NET_WM_STATE", |a| Some([1, a("_NET_WM_STATE_FULLSCREEN")?, 0, 1, 0])) { return; } }
    if !win.is_fullscreen() { win.toggle_fullscreen(); }
}

/// Minimised (iconic, or hidden by the window manager)? Then shown again.
/// True when it was.
pub fn restore_minimized(win: &Window) -> bool {
    #[cfg(target_os = "linux")]
    {
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
        let Some(id) = x11_window(win) else { return false };
        let Ok((c, _)) = x11rb::connect(None) else { return false };
        let atom = |n: &str| c.intern_atom(false, n.as_bytes()).ok().and_then(|r| r.reply().ok()).map(|r| r.atom);
        let (Some(state), Some(hidden)) = (atom("_NET_WM_STATE"), atom("_NET_WM_STATE_HIDDEN")) else { return false };
        let Some(r) = c.get_property(false, id, state, AtomEnum::ATOM, 0, 64).ok().and_then(|r| r.reply().ok()) else { return false };
        let is_hidden = r.value32().is_some_and(|mut v| v.any(|a| a == hidden));
        if is_hidden {
            let _ = c.map_window(id);
            let _ = x11rb::connection::Connection::flush(&c);
            x11_ask(win, "_NET_WM_STATE", |a| Some([0, a("_NET_WM_STATE_HIDDEN")?, 0, 1, 0]));
        }
        is_hidden
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, ShowWindow, SW_RESTORE};
        let Some(h) = hwnd(win) else { return false };
        // SAFETY: the window's own handle.
        unsafe { if IsIconic(h) != 0 { ShowWindow(h, SW_RESTORE); true } else { false } }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    { let _ = win; false }
}

/// What the window manager says of the window (`_NET_WM_STATE`): full
/// screen, focused, hidden (minimised). None: not X11, or nothing said.
#[cfg(target_os = "linux")]
pub fn x11_wm_state(win: &Window) -> Option<(bool, bool, bool)> {
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
    let id = x11_window(win)?;
    let (c, _) = x11rb::connect(None).ok()?;
    let atom = |n: &str| c.intern_atom(false, n.as_bytes()).ok().and_then(|r| r.reply().ok()).map(|r| r.atom);
    let (state, full, focused, hidden) = (atom("_NET_WM_STATE")?, atom("_NET_WM_STATE_FULLSCREEN")?, atom("_NET_WM_STATE_FOCUSED")?, atom("_NET_WM_STATE_HIDDEN")?);
    let r = c.get_property(false, id, state, AtomEnum::ATOM, 0, 64).ok()?.reply().ok()?;
    let v: Vec<u32> = r.value32()?.collect();
    Some((v.contains(&full), v.contains(&focused), v.contains(&hidden)))
}

#[cfg(target_os = "windows")]
fn hwnd(win: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(win).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => Some(h.hwnd.get() as windows_sys::Win32::Foundation::HWND),
        _ => None,
    }
}

// ─── Windows ───

#[cfg(target_os = "windows")]
fn win_state(win: &Window, on: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_NOTOPMOST, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW};
    let h = match HasWindowHandle::window_handle(win).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get() as windows_sys::Win32::Foundation::HWND,
        _ => return,
    };
    // SAFETY: the window's own handle, alive while `win` is.
    unsafe {
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let ex = if on { (ex | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize) } else { (ex & !(WS_EX_TOOLWINDOW as isize)) | WS_EX_APPWINDOW as isize };
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex);
        SetWindowPos(h, if on { HWND_TOPMOST } else { HWND_NOTOPMOST }, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED);
    }
}

#[cfg(target_os = "windows")]
const KID_KEYS_PS: &str = r#"
$code = @"
using System; using System.Diagnostics; using System.Runtime.InteropServices; using System.Windows.Forms;
public static class CrundiKidKeys {
  delegate IntPtr Proc(int n, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern IntPtr SetWindowsHookEx(int id, Proc p, IntPtr h, uint t);
  [DllImport("user32.dll")] static extern IntPtr CallNextHookEx(IntPtr h, int n, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern short GetAsyncKeyState(int k);
  [DllImport("kernel32.dll")] static extern IntPtr GetModuleHandle(string n);
  static Proc keep; static IntPtr hook;
  static IntPtr Cb(int n, IntPtr w, IntPtr l) {
    if (n >= 0) {
      int vk = Marshal.ReadInt32(l); int flags = Marshal.ReadInt32(l, 8);
      bool alt = (flags & 0x20) != 0, ctrl = (GetAsyncKeyState(0x11) & 0x8000) != 0;
      if (vk == 0x5B || vk == 0x5C || vk == 0x5D) return (IntPtr)1;
      if (alt && (vk == 0x09 || vk == 0x1B || vk == 0x73 || vk == 0x20)) return (IntPtr)1;
      if (ctrl && vk == 0x1B) return (IntPtr)1;
    }
    return CallNextHookEx(hook, n, w, l);
  }
  public static void Run(int parent) {
    keep = Cb; hook = SetWindowsHookEx(13, keep, GetModuleHandle(null), 0);
    var t = new System.Windows.Forms.Timer(); t.Interval = 2000;
    t.Tick += (s, e) => { try { Process.GetProcessById(parent); } catch { Application.Exit(); } };
    t.Start(); Application.Run();
  }
}
"@
Add-Type -TypeDefinition $code -ReferencedAssemblies System.Windows.Forms
[CrundiKidKeys]::Run([int]$args[0])
"#;

#[cfg(target_os = "windows")]
static KEYS_PROC: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

/// The keyboard filter: on while the lock is (it ends on its own if this app goes).
#[cfg(target_os = "windows")]
fn windows_keys(on: bool) {
    use std::os::windows::process::CommandExt as _;
    let mut p = KEYS_PROC.lock().unwrap();
    if !on { if let Some(mut c) = p.take() { let _ = c.kill(); eprintln!("[kid] keyboard filter off"); } return; }
    if p.is_some() { return; }
    let file = restore_file("kid-keys.ps1");
    if std::fs::write(&file, KID_KEYS_PS).is_err() { return; }
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    match Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"]).arg(&file).arg(std::process::id().to_string())
        .creation_flags(CREATE_NO_WINDOW).spawn() {
        Ok(c) => { *p = Some(c); eprintln!("[kid] keyboard filter on (Windows key, Alt+Tab, Ctrl+Esc blocked)"); }
        Err(e) => eprintln!("[kid] keyboard filter: {e}"),
    }
}

#[cfg(target_os = "windows")]
const TOUCHPAD: &[&str] = &["ThreeFingerSlideEnabled", "FourFingerSlideEnabled", "ThreeFingerTapEnabled", "FourFingerTapEnabled", "ThreeFingerGestureSetting", "FourFingerGestureSetting"];
#[cfg(target_os = "windows")]
const TP_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad";

/// The touchpad's gestures off for this user (HKCU, no admin), and back.
#[cfg(target_os = "windows")]
fn windows_touchpad(on: bool) {
    use serde_json::{Map, Value};
    let file = restore_file("kid-touchpad-restore.json");
    if on {
        if file.exists() { return; }
        let mut saved = Map::new();
        for k in TOUCHPAD {
            let v = sh("reg", &["query", TP_KEY, "/v", k]).and_then(|o| o.split_whitespace().skip_while(|w| *w != "REG_DWORD").nth(1).and_then(|h| u32::from_str_radix(h.trim_start_matches("0x"), 16).ok()));
            saved.insert(k.to_string(), v.map(Value::from).unwrap_or(Value::Null));
        }
        let _ = std::fs::write(&file, serde_json::to_vec(&Value::Object(saved)).unwrap_or_default());
        for k in TOUCHPAD { let _ = sh("reg", &["add", TP_KEY, "/v", k, "/t", "REG_DWORD", "/d", "0", "/f"]); }
        eprintln!("[kid] touchpad gestures off");
    } else {
        let Ok(b) = std::fs::read(&file) else { return };
        let saved: Value = serde_json::from_slice(&b).unwrap_or(Value::Null);
        for k in TOUCHPAD {
            match saved.get(*k).and_then(|v| v.as_u64()) {
                None => { let _ = sh("reg", &["delete", TP_KEY, "/v", k, "/f"]); }
                Some(v) => { let _ = sh("reg", &["add", TP_KEY, "/v", k, "/t", "REG_DWORD", "/d", &v.to_string(), "/f"]); }
            }
        }
        let _ = std::fs::remove_file(&file);
        eprintln!("[kid] touchpad gestures restored");
    }
}
