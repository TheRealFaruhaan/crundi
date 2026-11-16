//! What Settings, Skills, Backup and Usage share (the Slint client's
//! kit helpers and src/ops/skills/common.rs): JSON reading as the web reads
//! it, the web's fmtBytes / fmtTok and its local dates (en-US, as the PC
//! client shows them), a POST that keeps the server's answer for any status
//! (a 409 conflict carries what to ask next), downloads, links, the
//! clipboard, toasts and timers from anywhere, and `Fields`: text boxes
//! whose text lives in a screen's state (set from the server or the driver,
//! typed by the person).

use chrono::{Datelike, Local, NaiveDateTime, TimeZone, Timelike};
use crundi_core::api::{Api, ApiError, ApiResult};
use crundi_widgets::{self as w, InputEvent, InputState, TextareaState};
use gpui::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

// ─── JSON and text ───

/// A field as text ("" when missing or null; numbers and bools as written).
pub fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    }
}

/// JavaScript's truthiness.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(x) => !x.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
    }
}

/// A field's truthiness (false when missing).
pub fn t(v: &Value, k: &str) -> bool { v.get(k).is_some_and(truthy) }

/// A number field (0 when missing).
pub fn n(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0)
}

/// encodeURIComponent.
pub fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~!*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// Milliseconds since 1970, now.
pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

/// The web's fmtBytes: "512 B", "1.5 KB", "120 MB" ("—" for none).
pub fn fmt_bytes(n: f64) -> String {
    if !n.is_finite() { return "—".into(); }
    if n < 1024.0 { return format!("{} B", n.round() as i64); }
    let u = ["KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (n / 1024.0, 0);
    while v >= 1024.0 && i < u.len() - 1 { v /= 1024.0; i += 1; }
    if v >= 100.0 { format!("{} {}", v.round() as i64, u[i]) } else { format!("{:.1} {}", v, u[i]) }
}

/// A time field as ms: a number (ms) or an ISO date.
pub fn ms_of(v: &Value, k: &str) -> Option<f64> {
    match v.get(k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok().or_else(|| crundi_core::time::parse_iso(s).map(|x| x * 1000.0)),
        _ => None,
    }
}

fn local(ms: f64) -> Option<NaiveDateTime> {
    Local.timestamp_millis_opt(ms as i64).single().map(|d| d.naive_local())
}

const MON: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAY: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

fn h12(t: &NaiveDateTime) -> (u32, &'static str) {
    let h = t.hour();
    (if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" })
}

/// toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' }): "Nov 15, 2026, 3:04 PM".
pub fn medium_short_at(t: &NaiveDateTime) -> String {
    let (h, ap) = h12(t);
    format!("{} {}, {}, {}:{:02} {}", MON[t.month0() as usize], t.day(), t.year(), h, t.minute(), ap)
}
pub fn medium_short(ms: f64) -> String { local(ms).map(|t| medium_short_at(&t)).unwrap_or_default() }

/// toLocaleString(): "11/15/2026, 3:04:05 PM".
pub fn locale_at(t: &NaiveDateTime) -> String {
    let (h, ap) = h12(t);
    format!("{}/{}/{}, {}:{:02}:{:02} {}", t.month(), t.day(), t.year(), h, t.minute(), t.second(), ap)
}
pub fn locale(ms: f64) -> String { local(ms).map(|t| locale_at(&t)).unwrap_or_default() }

/// toLocaleDateString([], { day, month: 'short', year }): "Nov 15, 2026".
pub fn date_short(ms: f64) -> String {
    local(ms).map(|t| format!("{} {}, {}", MON[t.month0() as usize], t.day(), t.year())).unwrap_or_default()
}

/// The backup's bkWhen: "today at 03:04 PM", "tomorrow at …", "yesterday
/// at …", else "Sat, Nov 21 at 03:04 PM".
pub fn when_at(t: &NaiveDateTime, now: &NaiveDateTime) -> String {
    let (h, ap) = h12(t);
    let clock = format!("{:02}:{:02} {}", h, t.minute(), ap);
    match (t.date() - now.date()).num_days() {
        0 => format!("today at {clock}"),
        1 => format!("tomorrow at {clock}"),
        -1 => format!("yesterday at {clock}"),
        _ => format!("{}, {} {} at {clock}", DAY[t.weekday().num_days_from_sunday() as usize], MON[t.month0() as usize], t.day()),
    }
}
pub fn when(ms: f64) -> String {
    if ms <= 0.0 { return String::new(); }
    match (local(ms), local(now_ms())) { (Some(t), Some(n)) => when_at(&t, &n), _ => String::new() }
}

// ─── Server answers ───

/// A server answer with `ok: false` is an error (its words, or `fallback`).
pub fn ok_or(r: Result<Value, String>, fallback: &str) -> Result<Value, String> {
    match r {
        Ok(v) if v.get("ok") == Some(&Value::Bool(false)) => {
            let e = s(&v, "error");
            Err(if e.is_empty() { fallback.to_string() } else { e })
        }
        Ok(v) => Ok(v),
        Err(e) => Err(if e.is_empty() { fallback.to_string() } else { e }),
    }
}

/// The server's answer read from a raw (status, text): the body for any
/// status (a 409 conflict, a 400 with its words); 404 is "Not supported by
/// this server", no answer at all a network error.
pub fn raw(r: ApiResult<(u16, String)>) -> ApiResult {
    let (st, text) = r?;
    if st == 404 { return Err(ApiError::Status(404, String::new())); }
    if st == 0 { return Err(ApiError::Net("Could not reach the server".into())); }
    Ok(serde_json::from_str(&text).unwrap_or_else(|_| json!({ "ok": false, "error": format!("The server answered {st}") })))
}

/// POST JSON and keep the answer whatever its status (the web's
/// `(await apiFetch(...)).json()`).
pub fn post_raw(api: &Api, path: &str, body: &Value) -> ApiResult {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    raw(api.post_with_progress(path, &bytes, &|_, _| {}))
}

/// POST, then `done(answer)` on the UI thread: the server's JSON, or
/// {ok:false, error} when it could not be reached or said 404.
pub fn post(cx: &mut App, path: &str, body: Value, done: impl FnOnce(Value, &mut App) + 'static) {
    let path = path.to_string();
    crundi_shell::call(cx, move |api| post_raw(api, &path, &body), move |r, cx| done(r.unwrap_or_else(|e| json!({ "ok": false, "error": e })), cx));
}

/// The web's `{ ok:false, error }` → its words (or `fallback`).
pub fn err_of(d: &Value, fallback: &str) -> String {
    let e = s(d, "error");
    if e.is_empty() { fallback.to_string() } else { e }
}

pub fn ok(d: &Value) -> bool { d.get("ok").and_then(|x| x.as_bool()).unwrap_or(false) }

/// A query string from pairs, leaving out the empty ones (skQuery).
pub fn query(pairs: &[(&str, &str)]) -> String {
    pairs.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&")
}

// ─── The window from anywhere ───

/// Run `f` with the window, after the current update (a toast, a confirm).
pub fn with_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    cx.defer(move |cx| {
        let Some(win) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
        let _ = win.update(cx, move |_, window, cx| f(window, cx));
    });
}

pub fn toast(cx: &mut App, text: &str) { let t = text.to_string(); with_window(cx, move |w, cx| w::toast(w, cx, t, w::Toast::Info)); }
pub fn toast_ok(cx: &mut App, text: &str) { let t = text.to_string(); with_window(cx, move |w, cx| w::toast(w, cx, t, w::Toast::Success)); }
pub fn toast_error(cx: &mut App, text: &str) { let t = text.to_string(); with_window(cx, move |w, cx| w::toast(w, cx, t, w::Toast::Error)); }

/// A confirm (the ask layer's): `then(true)` on OK.
pub fn confirm(cx: &mut App, title: &str, message: &str, ok: &str, danger: bool, then: impl FnOnce(&mut App) + 'static) {
    let a = w::Ask { title: title.to_string().into(), message: message.to_string().into(), ok: ok.to_string().into(), danger, ..Default::default() };
    let then = std::cell::RefCell::new(Some(then));
    with_window(cx, move |window, cx| {
        w::ask_confirm(window, cx, a, move |yes, _, cx| if yes { if let Some(f) = then.borrow_mut().take() { f(cx); } });
    });
}

/// Run `f` once after `ms`.
pub fn after(cx: &mut App, ms: u64, f: impl FnOnce(&mut App) + 'static) {
    let timer = cx.background_executor().timer(std::time::Duration::from_millis(ms));
    cx.spawn(async move |cx: &mut AsyncApp| { timer.await; let _ = cx.update(f); }).detach();
}

/// A link: the system browser (under the test driver only recorded).
pub fn open_url(url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) { return; }
    if let Ok(mut o) = OPENED.lock() { *o = url.to_string(); }
    if std::env::var_os("CRUNDI_TEST_PORT").is_some() { return; }
    let u = url.to_string();
    std::thread::spawn(move || { let _ = open::that(&u); });
}

static OPENED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
pub fn last_opened() -> String { OPENED.lock().map(|o| o.clone()).unwrap_or_default() }

/// Text to the clipboard.
pub fn copy_text(cx: &mut App, text: &str) -> bool {
    if text.is_empty() { return false; }
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    true
}

/// The file the person picks (the driver's "pick-next" in tests).
pub fn pick_file(filters: bool) -> Option<std::path::PathBuf> {
    crundi_core::driver::take_pick().or_else(|| {
        let d = rfd::FileDialog::new();
        if filters { d.add_filter("Skills", &["zip", "skill", "md"]).pick_file() } else { d.pick_file() }
    })
}

/// Save what a GET answers to a file the person picks (the system's save
/// dialog in Downloads; the driver's "pick-next" in tests).
pub fn download(cx: &mut App, url: String, name: String) {
    let Some(api) = crundi_shell::api(cx) else { return };
    crundi_shell::run(cx, move || -> Option<Result<String, String>> {
        let path = crundi_core::driver::take_pick().or_else(|| {
            let mut d = rfd::FileDialog::new().set_title(format!("Save {name}")).set_file_name(&name);
            if let Some(h) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
                let dl = std::path::PathBuf::from(h).join("Downloads");
                if dl.is_dir() { d = d.set_directory(dl); }
            }
            d.save_file()
        })?;
        Some(match api.get_bytes(&url) {
            Ok((200, bytes)) => std::fs::write(&path, bytes).map(|_| name.clone()).map_err(|e| e.to_string()),
            Ok((_, text)) => Err(serde_json::from_slice::<Value>(&text).ok().map(|v| err_of(&v, "Could not download")).unwrap_or_else(|| "Could not download".into())),
            Err(e) => Err(e.to_string()),
        })
    }, |r, cx| match r {
        Some(Ok(name)) => toast(cx, &format!("Saved {name}")),
        Some(Err(e)) => toast_error(cx, &e),
        None => {}
    });
}

// ─── Fields: text boxes over a screen's state ───

enum Box_ { Line(Entity<InputState>), Area(Entity<TextareaState>) }

/// What a field does on Enter or on leaving it (the screen's handler).
pub type OnKey = fn(&'static str, &str, &mut App);

/// A screen's text boxes: their text (`get` / `set`) lives here, so the
/// server's answers and the driver set it without a window; the boxes are
/// made when first drawn and take a `set` text at their next draw.
#[derive(Default)]
pub struct Fields {
    vals: HashMap<&'static str, String>,
    boxes: HashMap<&'static str, (Box_, Subscription)>,
    push: HashSet<&'static str>,
    ph: HashMap<&'static str, SharedString>,
}

impl Fields {
    pub fn get(&self, k: &str) -> String { self.vals.get(k).cloned().unwrap_or_default() }
    pub fn trim(&self, k: &str) -> String { self.get(k).trim().to_string() }
    pub fn set(&mut self, k: &'static str, v: impl Into<String>) {
        let v = v.into();
        if self.vals.get(k) == Some(&v) && !self.push.contains(k) { return; }
        self.vals.insert(k, v);
        self.push.insert(k);
    }
    /// Typed by the person (no push back into the box).
    fn typed(&mut self, k: &'static str, v: String) { self.vals.insert(k, v); }
}

/// A screen's Fields, reached through its Global.
pub trait HasFields: Global { fn fields(&mut self) -> &mut Fields; }

fn on_change<G: HasFields>(k: &'static str, v: String, cx: &mut App) {
    if cx.has_global::<G>() { cx.global_mut::<G>().fields().typed(k, v); }
}

/// The one-line box for `k` (made on first use; `masked`: a password).
pub fn line<G: HasFields>(window: &mut Window, cx: &mut App, k: &'static str, ph: &str, masked: bool, on_key: Option<OnKey>) -> Entity<InputState> {
    let ph: SharedString = ph.to_string().into();
    let have = cx.global_mut::<G>().fields().boxes.get(k).and_then(|(b, _)| if let Box_::Line(e) = b { Some(e.clone()) } else { None });
    let e = match have {
        Some(e) => e,
        None => {
            let (ph2, init) = (ph.clone(), cx.global_mut::<G>().fields().get(k));
            let e = cx.new(|cx| { let mut s = InputState::new(window, cx).placeholder(ph2).masked(masked); s.set_value(init, window, cx); s });
            let sub = cx.subscribe(&e, move |e, ev: &InputEvent, cx| match ev {
                InputEvent::Change => { let v = e.read(cx).value().to_string(); on_change::<G>(k, v, cx); }
                InputEvent::PressEnter { .. } => if let Some(f) = on_key { let v = e.read(cx).value().to_string(); f(k, &v, cx); },
                InputEvent::Blur => if let Some(f) = on_key { let v = e.read(cx).value().to_string(); f(k, &format!("\u{0}blur{v}"), cx); },
                _ => {}
            });
            let f = cx.global_mut::<G>().fields();
            f.boxes.insert(k, (Box_::Line(e.clone()), sub));
            f.push.remove(k);
            f.ph.insert(k, ph.clone());
            e
        }
    };
    sync::<G>(window, cx, k, ph, Some(&e), None);
    e
}

/// The many-line box for `k` (`rows` high).
pub fn area<G: HasFields>(window: &mut Window, cx: &mut App, k: &'static str, ph: &str, rows: usize, on_submit: Option<OnKey>) -> Entity<TextareaState> {
    let ph: SharedString = ph.to_string().into();
    let have = cx.global_mut::<G>().fields().boxes.get(k).and_then(|(b, _)| if let Box_::Area(e) = b { Some(e.clone()) } else { None });
    let e = match have {
        Some(e) => e,
        None => {
            let (ph2, init) = (ph.clone(), cx.global_mut::<G>().fields().get(k));
            let e = cx.new(|cx| { let mut s = TextareaState::new(window, cx).rows(rows).placeholder(ph2); s.set_value(init, window, cx); s });
            let sub = cx.subscribe(&e, move |e, ev: &InputEvent, cx| match ev {
                InputEvent::Change => { let v = e.read(cx).value().to_string(); on_change::<G>(k, v, cx); }
                // Ctrl+Enter (secondary) submits; a plain Enter is a new line.
                InputEvent::PressEnter { secondary: true, .. } => if let Some(f) = on_submit { let v = e.read(cx).value().to_string(); on_change::<G>(k, v.clone(), cx); f(k, &v, cx); },
                _ => {}
            });
            let f = cx.global_mut::<G>().fields();
            f.boxes.insert(k, (Box_::Area(e.clone()), sub));
            f.push.remove(k);
            f.ph.insert(k, ph.clone());
            e
        }
    };
    sync::<G>(window, cx, k, ph, None, Some(&e));
    e
}

/// A `set` text into its box, and a changed placeholder.
fn sync<G: HasFields>(window: &mut Window, cx: &mut App, k: &'static str, ph: SharedString, l: Option<&Entity<InputState>>, a: Option<&Entity<TextareaState>>) {
    let f = cx.global_mut::<G>().fields();
    let v = if f.push.remove(k) { Some(f.get(k)) } else { None };
    let new_ph = if f.ph.get(k) != Some(&ph) { f.ph.insert(k, ph.clone()); Some(ph) } else { None };
    if v.is_none() && new_ph.is_none() { return; }
    if let Some(e) = l {
        e.update(cx, |s, cx| { if let Some(v) = v { s.set_value(v, window, cx); } if let Some(p) = new_ph { s.set_placeholder(p, window, cx); } });
    } else if let Some(e) = a {
        e.update(cx, |s, cx| { if let Some(v) = v { s.set_value(v, window, cx); } if let Some(p) = new_ph { s.set_placeholder(p, window, cx); } });
    }
}

/// A field's handler argument: the text, and whether it came from leaving
/// the box (Blur) rather than Enter.
pub fn key_arg(arg: &str) -> (&str, bool) {
    match arg.strip_prefix("\u{0}blur") { Some(v) => (v, true), None => (arg, false) }
}

#[cfg(test)]
mod tests {
    use super::{fmt_bytes, medium_short_at, locale_at, when_at, query, key_arg};
    use chrono::{NaiveDate, NaiveDateTime};

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime { NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 5).unwrap() }

    #[::core::prelude::v1::test]
    fn bytes_as_the_web() {
        assert_eq!(fmt_bytes(512.0), "512 B");
        assert_eq!(fmt_bytes(1536.0), "1.5 KB");
        assert_eq!(fmt_bytes(150.0 * 1024.0 * 1024.0), "150 MB");
        assert_eq!(fmt_bytes(f64::NAN), "—");
    }

    #[::core::prelude::v1::test]
    fn dates() {
        let t = at(2026, 11, 15, 15, 4);
        assert_eq!(medium_short_at(&t), "Nov 15, 2026, 3:04 PM");
        assert_eq!(locale_at(&t), "11/15/2026, 3:04:05 PM");
        let now = at(2026, 11, 15, 10, 0);
        assert_eq!(when_at(&at(2026, 11, 15, 3, 0), &now), "today at 03:00 AM");
        assert_eq!(when_at(&at(2026, 11, 16, 15, 30), &now), "tomorrow at 03:30 PM");
        assert_eq!(when_at(&at(2026, 11, 14, 23, 59), &now), "yesterday at 11:59 PM");
        assert_eq!(when_at(&at(2026, 11, 20, 3, 0), &now), "Fri, Nov 20 at 03:00 AM");
    }

    #[::core::prelude::v1::test]
    fn queries_and_keys() {
        assert_eq!(query(&[("scope", "global"), ("name", ""), ("path", "a b.md")]), "scope=global&path=a%20b.md");
        assert_eq!(key_arg("\u{0}blur12"), ("12", true));
        assert_eq!(key_arg("12"), ("12", false));
    }
}
