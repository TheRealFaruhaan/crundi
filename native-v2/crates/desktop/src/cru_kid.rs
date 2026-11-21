//! Kid mode (app/vendor/crundi-voice.js kidMode, pinPad, pinTry, kidExit,
//! kidPaused; src/webapp-html.js applyKidMode; capability 'voice-kid'): the
//! child's Cru over the whole window, left only with the grown-ups' PIN.
//!
//! The state comes from GET /api/voice/kid at sign-in and the server's
//! `voice-kid` event (and the voice socket's `kid`). As the web, the kid
//! screen opens on the device that started kid mode (kept in the config
//! folder: kid-device); "off" closes it anywhere. The tray's "Kid mode"
//! starts it here (POST /api/voice/kid/start { by: "tray" }).

use crate::{cru, ctx as kit};
use serde_json::{json, Value};
use std::{cell::RefCell, time::{Duration, Instant}};

/// The PIN pad closes by itself after this long.
pub const PIN_OPEN: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Kid {
    pub on: bool,
    pub paused: bool,
    pub muted: bool,
    pub pin_at: Option<Instant>,
    pub pin: String,
    pub pin_err: String,
}

thread_local! { static K: RefCell<Kid> = RefCell::new(Kid::default()); }
pub fn with<R>(f: impl FnOnce(&mut Kid) -> R) -> R { K.with(|k| f(&mut k.borrow_mut())) }
pub fn on() -> bool { with(|k| k.on) }

fn device_file() -> std::path::PathBuf { crundi_core::store::config_dir().join("kid-device") }
fn kid_device() -> bool { device_file().exists() }
fn mark_device(on: bool) { if on { let _ = std::fs::write(device_file(), "1"); } else { let _ = std::fs::remove_file(device_file()); } }

/// At sign-in: what kid mode is now.
pub fn load() {
    kit::call(|api| api.get("/api/voice/kid"), |r| if let Ok(v) = r { if kit::truthy(&v["ok"]) { state(&v); } });
}

/// Kid mode's state from the server: on (here, when this is the kid device), off, paused.
pub fn state(st: &Value) {
    let on = kit::truthy(st.get("on").unwrap_or(&Value::Null));
    if !on { mark_device(false); if with(|k| k.on) { exit(); } return; }
    if !kid_device() { return; }
    if !with(|k| k.on) {
        with(|k| { k.on = true; k.pin_at = None; k.pin.clear(); k.pin_err.clear(); });
        // The window locked to the kid screen (app/main.js kidLockWindow).
        kit::lock_window(true);
        kit::clear_dialogs_later();
        if cru::with(|c| c.open) { cru::close(); }
        cru::open(false);
    }
    paused(kit::truthy(st.get("paused").unwrap_or(&Value::Null)));
    cru::render();
}

/// A grown-up paused it from another device: the break scene, the mic let go.
fn paused(on: bool) {
    if with(|k| k.paused == on) { return; }
    with(|k| k.paused = on);
    if on { cru::stop(); } else if with(|k| !k.muted) { cru::listen(); }
}

/// The tray's "Kid mode".
pub fn start(by: &str) {
    let body = json!({ "by": by });
    kit::call(move |api| api.post("/api/voice/kid/start", &body), |r| match r {
        Ok(v) if kit::truthy(&v["ok"]) => { mark_device(true); state(&v); }
        Ok(v) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Kid mode needs a PIN: set one in Settings, Voice assistant, Kid mode".into())),
        Err(_) => kit::toast_error("Kid mode needs a PIN: set one in Settings, Voice assistant, Kid mode"),
    });
}

/// The grown-ups' lock: the PIN pad (closes after a minute).
pub fn pin_open() {
    if !on() { return; }
    with(|k| { k.pin_at = Some(Instant::now()); k.pin.clear(); k.pin_err.clear(); });
    crundi_webview::later(PIN_OPEN, || if with(|k| k.pin_at.is_some_and(|t| t.elapsed() >= PIN_OPEN)) { pin_close(); });
    cru::render();
}

pub fn pin_close() { with(|k| { k.pin_at = None; k.pin.clear(); }); cru::render(); }
pub fn pin_shown() -> bool { with(|k| k.pin_at.is_some()) }

/// The PIN box typed in (digits only, eight at most).
pub fn set_pin(v: &str) {
    let v: String = v.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
    let moved = with(|k| { let m = k.pin != v; if m { k.pin = v; k.pin_err.clear(); } m });
    if moved { cru::render(); }
}

/// A key of the pad: a digit, "Back", or "OK".
pub fn pin_key(key: &str) {
    match key {
        "OK" => pin_try(),
        "Back" => { with(|k| { k.pin.pop(); }); cru::render(); }
        d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => { with(|k| if k.pin.len() < 8 { k.pin.push_str(d); }); cru::render(); }
        _ => {}
    }
}

/// The PIN typed: out of kid mode, or "That is not the PIN".
pub fn pin_try() {
    let pin: String = with(|k| k.pin.chars().filter(|c| c.is_ascii_digit()).collect());
    if pin.is_empty() { return; }
    let body = json!({ "pin": pin });
    kit::call(move |api| api.post("/api/voice/kid/exit", &body), |r| match r {
        Ok(v) if kit::truthy(&v["ok"]) => exit(),
        Ok(v) => { with(|k| { k.pin.clear(); k.pin_err = Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "That is not the PIN".into()); }); cru::render(); }
        Err(e) => { with(|k| { k.pin.clear(); k.pin_err = if e.contains("PIN") || e.contains("403") { "That is not the PIN".into() } else { "Could not reach the server".into() }; }); cru::render(); }
    });
}

/// Kid mode is over (the PIN, or another screen ended it).
pub fn exit() {
    mark_device(false);
    with(|k| *k = Kid::default());
    if !crate::pclock::on() { kit::lock_window(false); }
    if cru::with(|c| c.open) { cru::close(); }
    cru::render();
}

/// The kid screen's mute: the microphone off (and on again).
pub fn mute() {
    let m = with(|k| { k.muted = !k.muted; k.muted });
    if m { cru::stop(); } else { cru::listen(); }
    cru::render();
}

pub fn props() -> Value {
    with(|k| json!({ "on": k.on, "paused": k.paused, "muted": k.muted, "pin": k.pin_at.is_some(), "pinLen": k.pin.len(), "pinError": k.pin_err }))
}

/// For the test driver: this PC is the kid device (as a start from its tray).
pub fn test_mark(on: bool) { mark_device(on); }
