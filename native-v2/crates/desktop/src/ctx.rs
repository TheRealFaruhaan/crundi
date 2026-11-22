//! What the area's logic reads of the window (a copy of the session's
//! project, projects and chats, taken at each pump: the logic never reads a
//! store itself) and what it asks of it (`on_app`: a toast, the drawer shown
//! or hidden, a confirm, a server call), run at the next pump with the
//! window. Asking wakes the pump (lib.rs): nothing here polls. The logic
//! itself (drawer.rs, host.rs) is the Slint client's, by way of native-gpui.

use crundi_widgets as w;
use gpui::{App, Window};
use serde_json::Value;
use std::cell::RefCell;

#[derive(Default, Clone)]
#[allow(dead_code)]
pub struct Ctx {
    pub signed_in: bool,
    pub base: String,
    pub project: String,
    pub projects: Vec<Value>,
    pub terminals: Vec<Value>,
    /// The workbench's pane keys, and the focused chat.
    pub panes: Vec<String>,
    pub focused: String,
    pub tab: String,
    /// The signed-in server's API (a clone: cheap).
    pub api: Option<crundi_core::api::Api>,
}

type Effect = Box<dyn FnOnce(&mut Window, &mut App)>;

thread_local! {
    static CTX: RefCell<Ctx> = RefCell::new(Ctx::default());
    static EFFECTS: RefCell<Vec<Effect>> = RefCell::new(Vec::new());
    static DIRTY: RefCell<bool> = const { RefCell::new(true) };
}

pub fn get() -> Ctx { CTX.with(|c| c.borrow().clone()) }
pub fn set(c: Ctx) { CTX.with(|x| *x.borrow_mut() = c); }
pub fn project() -> String { CTX.with(|c| c.borrow().project.clone()) }

/// Run with the window at the next pump.
pub fn on_app(f: impl FnOnce(&mut Window, &mut App) + 'static) { EFFECTS.with(|e| e.borrow_mut().push(Box::new(f))); crundi_webview::wake(); }
pub fn has_effects() -> bool { EFFECTS.with(|e| !e.borrow().is_empty()) }
/// The logic said the views' copies moved (not taken yet).
pub fn is_dirty() -> bool { DIRTY.with(|d| *d.borrow()) }
/// Work from other threads waits for the main thread.
pub fn has_main() -> bool { !MAIN.lock().unwrap().is_empty() }
pub fn take_effects() -> Vec<Effect> { EFFECTS.with(|e| std::mem::take(&mut *e.borrow_mut())) }

/// The drawer's state changed: draw it again.
pub fn dirty() { DIRTY.with(|d| *d.borrow_mut() = true); crundi_webview::wake(); }
pub fn take_dirty() -> bool { DIRTY.with(|d| std::mem::replace(&mut *d.borrow_mut(), false)) }

pub fn toast(text: &str) { let t = text.to_string(); on_app(move |win, cx| w::toast(win, cx, t, w::Toast::Info)); }
pub fn toast_ok(text: &str) { let t = text.to_string(); on_app(move |win, cx| w::toast(win, cx, t, w::Toast::Success)); }
pub fn toast_error(text: &str) { let t = text.to_string(); on_app(move |win, cx| w::toast(win, cx, t, w::Toast::Error)); }

/// Ask first; `then(true)` on OK, `then(false)` on Cancel.
pub fn confirm(title: &str, ok: &str, danger: bool, then: impl Fn(bool) + 'static) {
    let (title, ok) = (title.to_string(), ok.to_string());
    on_app(move |win, cx| {
        let then = std::rc::Rc::new(then);
        let t2 = then.clone();
        w::confirm(win, cx, w::Confirm { title: title.into(), message: "".into(), ok: ok.into(), danger }, move |_, _| t2(true));
        let _ = then;
    });
}

/// A server call off the UI thread; `done` back on it.
pub fn call(work: impl FnOnce(&crundi_core::api::Api) -> crundi_core::api::ApiResult + Send + 'static, done: impl FnOnce(Result<Value, String>) + 'static) {
    on_app(move |_, cx| crundi_shell::call(cx, work, move |r, _| done(r)));
}

pub fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
pub fn n(v: &Value, k: &str) -> f64 { v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) }
pub fn truthy(v: &Value) -> bool { match v { Value::Bool(b) => *b, Value::Null => false, Value::Number(n) => n.as_f64() != Some(0.0), Value::String(s) => !s.is_empty(), _ => true } }

/// Percent-encode a URL component.
pub fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { o.push(b as char); } else { o.push_str(&format!("%{b:02X}")); }
    }
    o
}

/// Show a project tab (or the Inbox's page, "inbox").
pub fn show_tab(tab: &str) {
    let t = tab.to_string();
    on_app(move |_, cx| { let s = crundi_shell::stores(cx); s.nav.update(cx, |n, cx| n.show_tab(&t, cx)); });
}

/// Open another project.
pub fn select_project(alias: &str) {
    let a = alias.to_string();
    on_app(move |_, cx| { let s = crundi_shell::stores(cx); s.projects.update(cx, |p, cx| p.pick(&a, cx)); });
}

/// Work with the signed-in API off the UI thread (bytes, say); `done` back on it.
pub fn bg<T: Send + 'static>(work: impl FnOnce(&crundi_core::api::Api) -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    on_app(move |_, cx| {
        let Some(api) = crundi_shell::api(cx) else { return };
        crundi_shell::run(cx, move || work(&api), move |r, _| done(r));
    });
}

/// Bring the window to the front (a notification's click, the tray's Open),
/// shown again first if the × hid it. With the window closed (Wayland's
/// hide), the pump opens it again first (lib.rs `windowless`).
///
/// The click is the person asking for it (main.js bringToFront): focus is
/// asked outright, and on Linux the window is held on top for 300 ms while
/// the desktop catches up (GNOME's focus-stealing prevention would show
/// "Crundi is ready" instead).
pub fn raise() {
    on_app(|win, _| {
        show_window(win);
        win.activate_window();
        crate::kidlock::steal_focus(win);
        RAISES.with(|r| r.set(r.get() + 1));
        if cfg!(target_os = "linux") && !window_locked() {
            crate::kidlock::above(win, true);
            RAISED_ABOVE.with(|a| a.set(true));
            crundi_webview::later(std::time::Duration::from_millis(300), || on_app(|win, _| {
                // (Kid mode or the PC lock keeps it above: theirs to let go.)
                if !window_locked() { crate::kidlock::above(win, false); }
                RAISED_ABOVE.with(|a| a.set(false));
            }));
        }
    });
}

thread_local! {
    /// Raises asked so far, and the window held above by one now (the driver's look).
    static RAISES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static RAISED_ABOVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The kid lock's watchdog: its last look, and what it put right (shown, restored, full screen, to the front).
    static WATCH: std::cell::Cell<(Option<std::time::Instant>, u64, [u64; 4])> = const { std::cell::Cell::new((None, 0, [0; 4])) };
    /// The X11 window's id (the driver's look; 0: none).
    static X11_ID: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// How often the watchdog looks (main.js kidWatchdog: 700 ms).
const WATCH_EVERY: std::time::Duration = std::time::Duration::from_millis(700);

/// Kid mode holds the window (the watchdog is kid mode's alone, as main.js's).
fn kid_held() -> bool { window_locked() && crate::cru_kid::on() }

/// When the watchdog looks next (lib.rs `next_due`): only while kid mode holds the window.
pub fn watch_due() -> Option<std::time::Instant> {
    if !kid_held() { return None; }
    Some(WATCH.with(|w| w.get().0).map(|t| t + WATCH_EVERY).unwrap_or_else(std::time::Instant::now))
}

/// Each pump: while kid mode holds the window, whatever got
/// past the keys (a gesture, task view, a shortcut the OS keeps) is put
/// right: shown, un-minimised, full screen, in front (main.js kidWatchdog).
pub fn watchdog(win: &mut Window) {
    #[cfg(target_os = "linux")]
    { if X11_ID.with(|x| x.get()) == 0 { X11_ID.with(|x| x.set(crate::kidlock::x11_window(win).unwrap_or(0))); } }
    if !kid_held() { WATCH.with(|w| { let mut v = w.get(); v.0 = None; w.set(v); }); return; }
    let (last, mut ticks, mut fixed) = WATCH.with(|w| w.get());
    if last.is_some_and(|t| t.elapsed() < WATCH_EVERY) { return; }
    ticks += 1;
    if HIDDEN.with(|h| h.get()) { show_window(win); fixed[0] += 1; }
    if crate::kidlock::restore_minimized(win) { fixed[1] += 1; }
    // On X11 what the window manager says (its _NET_WM_STATE), else the platform's flags.
    #[cfg(target_os = "linux")]
    let wm = crate::kidlock::x11_wm_state(win);
    #[cfg(not(target_os = "linux"))]
    let wm: Option<(bool, bool, bool)> = None;
    let (full, front) = wm.map(|(f, a, _)| (f, a)).unwrap_or((win.is_fullscreen(), win.is_window_active()));
    if !full { crate::kidlock::fullscreen_on(win); fixed[2] += 1; }
    if !front { crate::kidlock::window(win, true); win.activate_window(); fixed[3] += 1; }
    WATCH.with(|w| w.set((Some(std::time::Instant::now()), ticks, fixed)));
}

#[cfg(target_os = "linux")]
fn wm_state_json(win: &Window) -> Option<serde_json::Value> { crate::kidlock::x11_wm_state(win).map(|(f, a, h)| serde_json::json!({ "fullscreen": f, "focused": a, "hidden": h })) }
#[cfg(not(target_os = "linux"))]
fn wm_state_json(_: &Window) -> Option<serde_json::Value> { None }

/// The raise and the watchdog, for the driver.
pub fn window_props(win: Option<&Window>) -> serde_json::Value {
    let (_, ticks, f) = WATCH.with(|w| w.get());
    serde_json::json!({
        "raises": RAISES.with(|r| r.get()), "raisedAbove": RAISED_ABOVE.with(|a| a.get()),
        "watch": { "ticks": ticks, "shown": f[0], "restored": f[1], "fullscreen": f[2], "front": f[3] },
        "x11": X11_ID.with(|x| x.get()),
        "fullscreen": win.map(|w| w.is_fullscreen()), "active": win.map(|w| w.is_window_active()),
        "wmState": win.and_then(wm_state_json),
    })
}

thread_local! {
    static HIDDEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The window was closed to hide it (Wayland): its bounds, to open it again there.
    static CLOSED: std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>> = const { std::cell::Cell::new(None) };
    /// The window lock (kid mode, the PC lock): on, and whether it made the window full screen.
    static LOCKED: std::cell::Cell<(bool, bool)> = const { std::cell::Cell::new((false, false)) };
}

static FORCE_CLOSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn forced_close() -> bool { FORCE_CLOSE.load(std::sync::atomic::Ordering::Relaxed) || std::env::var("CRUNDI_HIDE").is_ok_and(|v| v == "close") }
/// The driver: hide by closing ("close"), or the platform's way ("").
pub fn force_hide(how: &str) { FORCE_CLOSE.store(how == "close", std::sync::atomic::Ordering::Relaxed); }
/// How the × hides here, without the window (the driver's look).
pub fn hide_how_name() -> &'static str {
    if forced_close() { return "close"; }
    if cfg!(target_os = "linux") { if std::env::var_os("WAYLAND_DISPLAY").is_some() { "close" } else { "unmap" } } else { "hide" }
}

/// The × hid the window (the driver's look).
pub fn hidden() -> bool { HIDDEN.with(|h| h.get()) }
/// The window is closed to hide it (Wayland): the bounds to open it again with.
pub fn closed() -> Option<gpui::Bounds<gpui::Pixels>> { CLOSED.with(|c| c.get()) }
/// Opened again (lib.rs): shown.
pub fn reopened() { CLOSED.with(|c| c.set(None)); HIDDEN.with(|h| h.set(false)); }

/// How the × hides the window here: "unmap" (X11), "close" (Wayland: the
/// window closed, the app going on in the tray, opened again by the tray's
/// Open with the same state), "hide" (Windows: SW_HIDE; macOS: the app
/// hidden). CRUNDI_HIDE=close tries Wayland's way anywhere (the suite, on Xvfb).
pub fn hide_how(win: &Window) -> &'static str {
    if forced_close() { return "close"; }
    #[cfg(target_os = "linux")] { if x11_id(win).is_some() { "unmap" } else { "close" } }
    #[cfg(not(target_os = "linux"))] { let _ = win; "hide" }
}

/// The window's × with the tray up (Electron's mainWindow 'close': hidden,
/// the app goes on; the tray's Open brings it back). Never minimised.
pub fn hide_window(win: &mut Window, cx: &mut App) {
    HIDDEN.with(|h| h.set(true));
    match hide_how(win) {
        "close" => { CLOSED.with(|c| c.set(Some(win.bounds()))); win.remove_window(); }
        #[cfg(target_os = "linux")]
        "unmap" => { let _ = cx; x11_map(win, false); }
        _ => platform_hide(win, cx),
    }
}

fn show_window(win: &mut Window) {
    if !HIDDEN.with(|h| h.replace(false)) { return; }
    #[cfg(target_os = "linux")] { x11_map(win, true); }
    #[cfg(not(target_os = "linux"))] { platform_show(win); }
}

#[cfg(target_os = "macos")]
fn platform_hide(_: &mut Window, cx: &mut App) { cx.hide(); }
#[cfg(target_os = "macos")]
fn platform_show(_: &mut Window) {}

/// Windows: SW_HIDE, and SW_SHOW with the window brought to the front.
#[cfg(target_os = "windows")]
fn hwnd(win: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(win).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => Some(h.hwnd.get() as windows_sys::Win32::Foundation::HWND),
        _ => None,
    }
}
#[cfg(target_os = "windows")]
fn platform_hide(win: &mut Window, _: &mut App) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    // SAFETY: the window's own handle, alive while `win` is.
    if let Some(h) = hwnd(win) { unsafe { ShowWindow(h, SW_HIDE); } }
}
#[cfg(target_os = "windows")]
fn platform_show(win: &mut Window) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_SHOW};
    if let Some(h) = hwnd(win) { unsafe { ShowWindow(h, SW_SHOW); SetForegroundWindow(h); } }
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_hide(win: &mut Window, _: &mut App) { win.minimize_window() }
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_show(_: &mut Window) {}
#[cfg(target_os = "linux")]
fn platform_hide(win: &mut Window, _: &mut App) { win.remove_window() }

/// The window locked to the screen (app/main.js kidLockWindow): shown, in
/// front and full screen while kid mode or the PC lock is on; as it was
/// when both are off. Brought back to the front if it loses the keys (lib.rs).
pub fn lock_window(on: bool) {
    // Dialog keys dropped and the focus kept in the lock while it is on (F228).
    w::hold_keys(on);
    on_app(move |win, cx| {
        if on { w::ask_release_focus(win, cx); } else if !w::keys_held() { w::ask_refocus(win, cx); }
        // The kid screen and the PC lock draw over any dialog (top_layer, F212). The PC lock leaves
        // an open dialog as it was, under it (the web's lock, F218), as kid mode does (F231).
        let (was, made_full) = LOCKED.with(|l| l.get());
        if on == was { if on && !win.is_fullscreen() { win.toggle_fullscreen(); } return; }
        if on {
            show_window(win);
            win.activate_window();
            let full = !win.is_fullscreen();
            if full { win.toggle_fullscreen(); }
            // Above the others and off the taskbar; the desktop's ways out off.
            crate::kidlock::window(win, true);
            crate::kidlock::desktop(true);
            LOCKED.with(|l| l.set((true, full)));
        } else {
            if made_full && win.is_fullscreen() { win.toggle_fullscreen(); }
            crate::kidlock::window(win, false);
            crate::kidlock::desktop(false);
            LOCKED.with(|l| l.set((false, false)));
        }
    });
}
pub fn window_locked() -> bool { LOCKED.with(|l| l.get().0) }

/// The X11 window's id (None: not an X11 window).
#[cfg(target_os = "linux")]
fn x11_id(win: &Window) -> Option<u32> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(win).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Xlib(h)) => Some(h.window as u32),
        Ok(RawWindowHandle::Xcb(h)) => Some(h.window.get()),
        _ => None,
    }
}

/// Map or unmap the window on X11 (false: not an X11 window).
#[cfg(target_os = "linux")]
fn x11_map(win: &Window, on: bool) -> bool {
    use x11rb::protocol::xproto::ConnectionExt as _;
    let Some(id) = x11_id(win) else { return false };
    let Ok((c, _)) = x11rb::connect(None) else { return false };
    let r = if on { c.map_window(id).map(|k| k.check()) } else { c.unmap_window(id).map(|k| k.check()) };
    matches!(r, Ok(Ok(())))
}

/// Calls from other threads (the microphone's, the speaker's), run on the
/// main thread at the next pump.
static MAIN: std::sync::Mutex<Vec<Box<dyn FnOnce() + Send>>> = std::sync::Mutex::new(Vec::new());
pub fn main_thread(f: impl FnOnce() + Send + 'static) { MAIN.lock().unwrap().push(Box::new(f)); crundi_webview::wake(); }
pub fn take_main() -> Vec<Box<dyn FnOnce() + Send>> { std::mem::take(&mut *MAIN.lock().unwrap()) }
