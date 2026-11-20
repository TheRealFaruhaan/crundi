//! The PC's idle time, for the PC lock's auto lock (app/main.js: every 15 s,
//! powerMonitor.getSystemIdleTime(); 'lock-screen'). A thread of its own,
//! running only while auto lock is on: every 15 s it reads how long the PC
//! has had no keyboard or mouse (anywhere, not only in this window) and
//! whether the OS's screen is locked. Once the idle time passes the
//! setting (or the screen locks), it asks for the lock once per idle
//! stretch; the UI thread hears of nothing else.
//!
//! - Linux, X11: the MIT-SCREEN-SAVER extension's ms since input;
//! - Linux, Wayland: GNOME's org.gnome.Mutter.IdleMonitor (or KDE's
//!   org.freedesktop.ScreenSaver) over the session bus; the screen's lock
//!   from org.gnome.ScreenSaver / org.freedesktop.ScreenSaver GetActive;
//! - Windows: GetLastInputInfo; macOS: CGEventSourceSecondsSinceLastEventType.

use std::sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Mutex};
use std::time::Duration;

const EVERY: Duration = Duration::from_secs(15);

static ON: AtomicBool = AtomicBool::new(false);
static AFTER_S: AtomicU64 = AtomicU64::new(600);
static RUNNING: AtomicBool = AtomicBool::new(false);
static FIRED: AtomicBool = AtomicBool::new(false);
/// The test driver's idle seconds and screen lock, instead of the OS's.
static FAKE: Mutex<Option<(u64, bool)>> = Mutex::new(None);

/// Auto lock on or off, after this long.
pub fn set(on: bool, after: Duration) {
    AFTER_S.store(after.as_secs().max(60), Ordering::Relaxed);
    ON.store(on, Ordering::Relaxed);
    if on && !RUNNING.swap(true, Ordering::SeqCst) {
        let _ = std::thread::Builder::new().name("crundi-idle".into()).spawn(|| {
            while ON.load(Ordering::Relaxed) {
                std::thread::sleep(EVERY);
                if ON.load(Ordering::Relaxed) { check(); }
            }
            RUNNING.store(false, Ordering::SeqCst);
        });
    }
}

pub fn watching() -> bool { RUNNING.load(Ordering::Relaxed) }

/// One look: the idle time and the screen's lock, and the lock asked once.
pub fn check() {
    if !ON.load(Ordering::Relaxed) { return; }
    let (idle, screen_locked) = match *FAKE.lock().unwrap() { Some(f) => (Some(f.0), f.1), None => (idle_secs(), screen_locked()) };
    let fire = |why: &'static str| {
        FIRED.store(true, Ordering::Relaxed);
        eprintln!("[lock] auto lock ({why})");
        crate::ctx::main_thread(|| {
            let mode = crate::pclock::with(|l| l.auto.mode.clone());
            if !crate::cru_kid::on() { crate::pclock::ask(&mode, "auto"); }
        });
    };
    if screen_locked && !FIRED.load(Ordering::Relaxed) { fire("the screen locked"); return; }
    let Some(idle) = idle else { return };
    if idle < 5 { FIRED.store(false, Ordering::Relaxed); return; }
    if !FIRED.load(Ordering::Relaxed) && idle >= AFTER_S.load(Ordering::Relaxed) { fire("idle"); }
}

/// The driver: idle seconds and the screen's lock as the OS would say (None: the OS's again).
pub fn fake(v: Option<(u64, bool)>) { *FAKE.lock().unwrap() = v; }

#[cfg(target_os = "linux")]
fn idle_secs() -> Option<u64> {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        if let Some(ms) = x11_idle_ms() { return Some(ms / 1000); }
    }
    dbus_idle_ms().or_else(x11_idle_ms).map(|ms| ms / 1000)
}

#[cfg(target_os = "linux")]
fn x11_idle_ms() -> Option<u64> {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::screensaver::ConnectionExt as _;
    let (c, screen) = x11rb::connect(None).ok()?;
    let root = c.setup().roots.get(screen)?.root;
    let r = c.screensaver_query_info(root).ok()?.reply().ok()?;
    Some(r.ms_since_user_input as u64)
}

#[cfg(target_os = "linux")]
fn session() -> Option<zbus::blocking::Connection> { zbus::blocking::Connection::session().ok() }

/// One call on the session bus, its one answer.
#[cfg(target_os = "linux")]
fn ask<T: serde::de::DeserializeOwned + zbus::zvariant::Type>(c: &zbus::blocking::Connection, dest: &str, path: &str, iface: &str, method: &str) -> Option<T> {
    c.call_method(Some(dest), path, Some(iface), method, &()).ok()?.body().deserialize::<T>().ok()
}

#[cfg(target_os = "linux")]
fn dbus_idle_ms() -> Option<u64> {
    let c = session()?;
    ask::<u64>(&c, "org.gnome.Mutter.IdleMonitor", "/org/gnome/Mutter/IdleMonitor/Core", "org.gnome.Mutter.IdleMonitor", "GetIdletime")
        .or_else(|| ask::<u32>(&c, "org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetSessionIdleTime").map(|ms| ms as u64))
}

#[cfg(target_os = "linux")]
fn screen_locked() -> bool {
    let Some(c) = session() else { return false };
    ask::<bool>(&c, "org.gnome.ScreenSaver", "/org/gnome/ScreenSaver", "org.gnome.ScreenSaver", "GetActive")
        .or_else(|| ask::<bool>(&c, "org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetActive"))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn idle_secs() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: a plain call with a sized struct.
    if unsafe { GetLastInputInfo(&mut li) } == 0 { return None; }
    let now = unsafe { GetTickCount() };
    Some(now.wrapping_sub(li.dwTime) as u64 / 1000)
}
#[cfg(target_os = "windows")]
fn screen_locked() -> bool { false }

#[cfg(target_os = "macos")]
fn idle_secs() -> Option<u64> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" { fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64; }
    // kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType.
    let s = unsafe { CGEventSourceSecondsSinceLastEventType(0, u32::MAX) };
    (s.is_finite() && s >= 0.).then_some(s as u64)
}
#[cfg(target_os = "macos")]
fn screen_locked() -> bool { false }

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn idle_secs() -> Option<u64> { None }
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn screen_locked() -> bool { false }
