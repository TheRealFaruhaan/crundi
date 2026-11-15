//! Helpers the Skills, Backup, Usage and Info screens share: the web's
//! fmtBytes / fmtTok, its local dates (en-US, as the PC client shows them),
//! a POST that keeps the server's answer for any status (a 409 conflict
//! carries what to ask next), downloads, the clipboard and links.

use crate::kit::{self, Api, ApiError, ApiResult};
use chrono::{Datelike, Local, NaiveDateTime, TimeZone, Timelike};
use serde_json::{json, Value};

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

/// A byte count from a field (None when missing: "—").
pub fn bytes_of(v: &Value, k: &str) -> String {
    match v.get(k).and_then(|x| x.as_f64()) { Some(n) => fmt_bytes(n), None => "—".into() }
}

/// The web's fmtTok: 2M, 500k, 950.
pub fn fmt_tok(n: f64) -> String {
    if n >= 1e6 { let x = (n / 1e5).round() / 10.0; if x.fract() == 0.0 { format!("{}M", x as i64) } else { format!("{x}M") } }
    else if n >= 1e3 { format!("{}k", (n / 1e3).round() as i64) }
    else { format!("{}", n as i64) }
}

/// The web's parseTok: "2M", "500k", "1,500,000" → tokens.
pub fn parse_tok(raw: &str) -> Option<f64> {
    let s = raw.trim().to_lowercase().replace(',', "");
    let (num, mul) = if let Some(x) = s.strip_suffix('m') { (x.trim(), 1e6) } else if let Some(x) = s.strip_suffix('k') { (x.trim(), 1e3) } else { (s.as_str(), 1.0) };
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit() || c == '.') || num.matches('.').count() > 1 || num.starts_with('.') || num.ends_with('.') { return None; }
    num.parse::<f64>().ok().map(|v| (v * mul).round())
}

/// A time field as ms: a number (ms) or an ISO date.
pub fn ms_of(v: &Value, k: &str) -> Option<f64> {
    match v.get(k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok().or_else(|| kit::parse_iso(s).map(|x| x * 1000.0)),
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

/// toLocaleTimeString(): "3:04:05 PM".
pub fn time_at(t: &NaiveDateTime) -> String {
    let (h, ap) = h12(t);
    format!("{}:{:02}:{:02} {}", h, t.minute(), t.second(), ap)
}
pub fn time(ms: f64) -> String { local(ms).map(|t| time_at(&t)).unwrap_or_default() }

/// toLocaleDateString([], { day, month: 'short', year }): "Nov 15, 2026".
pub fn date_short(ms: f64) -> String {
    local(ms).map(|t| format!("{} {}, {}", MON[t.month0() as usize], t.day(), t.year())).unwrap_or_default()
}

/// The invite's "Ends …": under two days the clock ("Sat 3:04 PM"),
/// otherwise the day ("Sat, Nov 21").
pub fn ends_at(t: &NaiveDateTime, short: bool) -> String {
    let wd = DAY[t.weekday().num_days_from_sunday() as usize];
    if short { let (h, ap) = h12(t); format!("{wd} {h}:{:02} {ap}", t.minute()) } else { format!("{wd}, {} {}", MON[t.month0() as usize], t.day()) }
}
pub fn ends(ms: f64, short: bool) -> String { local(ms).map(|t| ends_at(&t, short)).unwrap_or_default() }

/// The invite's "Access ends": "Sat, Nov 21, 3:04 PM".
pub fn ends_long(ms: f64) -> String {
    local(ms).map(|t| { let (h, ap) = h12(&t); format!("{}, {} {}, {}:{:02} {}", DAY[t.weekday().num_days_from_sunday() as usize], MON[t.month0() as usize], t.day(), h, t.minute(), ap) }).unwrap_or_default()
}

/// The backup's bkWhen: "today at 03:04 PM", "tomorrow at …", "yesterday
/// at …", else "Sat, Nov 21 at 03:04 PM".
pub fn when_at(t: &NaiveDateTime, now: &NaiveDateTime) -> String {
    let (h, ap) = h12(t);
    let clock = format!("{:02}:{:02} {}", h, t.minute(), ap);
    let diff = (t.date() - now.date()).num_days();
    match diff {
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
pub fn post(path: &str, body: Value, done: impl FnOnce(Value) + Send + 'static) {
    let path = path.to_string();
    kit::call(move |api| post_raw(api, &path, &body), move |r| done(r.unwrap_or_else(|e| json!({ "ok": false, "error": e }))));
}

/// The web's `{ ok:false, error }` → its words (or `fallback`).
pub fn err_of(d: &Value, fallback: &str) -> String {
    let e = kit::s(d, "error");
    if e.is_empty() { fallback.to_string() } else { e }
}

pub fn ok(d: &Value) -> bool { d.get("ok").and_then(|x| x.as_bool()).unwrap_or(false) }

/// A query string from pairs, leaving out the empty ones (skQuery).
pub fn query(pairs: &[(&str, &str)]) -> String {
    pairs.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}={}", kit::enc(v))).collect::<Vec<_>>().join("&")
}

/// Save what a GET answers to a file the person picks (the system's save
/// dialog in Downloads; the driver's "pick-next" in tests).
pub fn download(url: String, name: String) {
    let Some(api) = kit::api() else { return };
    kit::bg(move || -> Option<Result<String, String>> {
        let path = crate::testctl::take_pick().or_else(|| {
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
    }, |r| match r {
        Some(Ok(name)) => kit::toast(&format!("Saved {name}")),
        Some(Err(e)) => kit::toast_error(&e),
        None => {}
    });
}

thread_local! { static OPENED: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }

/// A link: the system browser (under the test driver only recorded).
pub fn open_url(url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) { return; }
    OPENED.with(|o| *o.borrow_mut() = url.to_string());
    if std::env::var_os("CRUNDI_TEST_PORT").is_some() { return; }
    let u = url.to_string();
    std::thread::spawn(move || { let _ = open::that(&u); });
}

pub fn last_opened() -> String { OPENED.with(|o| o.borrow().clone()) }

/// Text to the clipboard (kept owned: on X11 the copy lives with it).
pub fn copy_text(text: &str) -> bool {
    let Ok(mut cb) = arboard::Clipboard::new() else { return false };
    if cb.set_text(text.to_string()).is_err() { return false; }
    crate::app::keep_clipboard(cb);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime { NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 5).unwrap() }

    #[test]
    fn bytes_as_the_web() {
        assert_eq!(fmt_bytes(512.0), "512 B");
        assert_eq!(fmt_bytes(1536.0), "1.5 KB");
        assert_eq!(fmt_bytes(150.0 * 1024.0 * 1024.0), "150 MB");
        assert_eq!(fmt_bytes(f64::NAN), "—");
    }

    #[test]
    fn tokens() {
        assert_eq!(fmt_tok(2_000_000.0), "2M");
        assert_eq!(fmt_tok(1_500_000.0), "1.5M");
        assert_eq!(fmt_tok(500_000.0), "500k");
        assert_eq!(fmt_tok(950.0), "950");
        assert_eq!(parse_tok("2M"), Some(2e6));
        assert_eq!(parse_tok("500k"), Some(5e5));
        assert_eq!(parse_tok("1,500,000"), Some(1.5e6));
        assert_eq!(parse_tok("lots"), None);
        assert_eq!(parse_tok(""), None);
    }

    #[test]
    fn dates() {
        let t = at(2026, 11, 15, 15, 4);
        assert_eq!(medium_short_at(&t), "Nov 15, 2026, 3:04 PM");
        assert_eq!(locale_at(&t), "11/15/2026, 3:04:05 PM");
        assert_eq!(time_at(&at(2026, 1, 2, 0, 7)), "12:07:05 AM");
        assert_eq!(ends_at(&t, true), "Sun 3:04 PM");
        assert_eq!(ends_at(&t, false), "Sun, Nov 15");
    }

    #[test]
    fn backup_when() {
        let now = at(2026, 11, 15, 10, 0);
        assert_eq!(when_at(&at(2026, 11, 15, 3, 0), &now), "today at 03:00 AM");
        assert_eq!(when_at(&at(2026, 11, 16, 15, 30), &now), "tomorrow at 03:30 PM");
        assert_eq!(when_at(&at(2026, 11, 14, 23, 59), &now), "yesterday at 11:59 PM");
        assert_eq!(when_at(&at(2026, 11, 20, 3, 0), &now), "Fri, Nov 20 at 03:00 AM");
    }

    #[test]
    fn queries_leave_out_blanks() {
        assert_eq!(query(&[("scope", "global"), ("name", ""), ("path", "a b.md")]), "scope=global&path=a%20b.md");
    }
}
