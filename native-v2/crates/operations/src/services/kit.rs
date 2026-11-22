//! Small helpers Services and Docker share (the Slint client's kit:: calls
//! they used): reading JSON, the toast and the ask layer from a server
//! answer (which comes with the App only), a server answer's refusal, URL
//! escaping, opening an address.

use crundi_widgets as w;
use gpui::*;
use serde_json::Value;

/// A field as text ("" missing; numbers and booleans as written).
pub fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(x) => x.to_string(),
    }
}

/// A field as a number (0 missing; a numeric string read).
pub fn n(v: &Value, k: &str) -> f64 {
    match v.get(k) {
        Some(Value::Number(x)) => x.as_f64().unwrap_or(0.0),
        Some(Value::String(x)) => x.trim().parse().unwrap_or(0.0),
        Some(Value::Bool(b)) => *b as i32 as f64,
        _ => 0.0,
    }
}

/// The web's !!value.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(x)) => x.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(x)) => !x.is_empty(),
        Some(_) => true,
    }
}

pub fn t(v: &Value, k: &str) -> bool { truthy(v.get(k)) }

pub fn arr<'a>(v: &'a Value, k: &str) -> Vec<&'a Value> {
    v.get(k).and_then(|x| x.as_array()).map(|a| a.iter().collect()).unwrap_or_default()
}

pub fn strs(v: &Value, k: &str) -> Vec<String> {
    arr(v, k).into_iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect()
}

/// encodeURIComponent.
pub fn enc(v: &str) -> String {
    let mut o = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) { o.push(b as char); } else { o.push_str(&format!("%{b:02X}")); }
    }
    o
}

/// A {ok:false, error} answer as Err(the server's words or `fallback`).
pub fn refused(r: Result<Value, String>, fallback: &str) -> Result<Value, String> {
    match r {
        Ok(v) if v.get("ok").and_then(|x| x.as_bool()) == Some(false) => Err({ let e = s(&v, "error"); if e.is_empty() { fallback.into() } else { e } }),
        other => other,
    }
}

/// Run with the window (a server call's answer comes with the App only).
pub fn with_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App)) {
    if let Some(h) = cx.active_window().or_else(|| cx.windows().first().copied()) {
        let _ = h.update(cx, |_, window, cx| f(window, cx));
    }
}

/// Deferred: a caller may be inside the window's own update already (a
/// click, a driver action), where the window cannot be borrowed again.
pub fn toast(cx: &mut App, text: &str, kind: w::Toast) {
    let t = text.to_string();
    cx.defer(move |cx| with_window(cx, move |window, cx| w::toast(window, cx, t, kind)));
}
pub fn ok(cx: &mut App, text: &str) { toast(cx, text, w::Toast::Success); }
pub fn info(cx: &mut App, text: &str) { toast(cx, text, w::Toast::Info); }
pub fn err(cx: &mut App, text: &str) { toast(cx, text, w::Toast::Error); }

/// The signed-in server's address.
pub fn base(cx: &App) -> String { crundi_shell::api(cx).map(|a| a.base().trim_end_matches('/').to_string()).unwrap_or_default() }

/// An address in the browser (a path one is on this server); under the
/// test driver it is only recorded. Returns the full address.
pub fn open_url(cx: &mut App, url: &str) -> String {
    if url.is_empty() { return String::new(); }
    let full = if url.starts_with('/') { format!("{}{}", base(cx), url) } else { url.to_string() };
    if std::env::var_os("CRUNDI_TEST_PORT").is_none() { cx.open_url(&full); }
    full
}

/// A text box over an InputState, made once (needs the window).
pub fn input(window: &mut Window, cx: &mut App, placeholder: &str) -> Entity<w::InputState> {
    let ph = placeholder.to_string();
    cx.new(|cx| w::InputState::new(window, cx).placeholder(ph))
}

pub fn value(i: &Option<Entity<w::InputState>>, cx: &App) -> String {
    i.as_ref().map(|i| i.read(cx).value().to_string()).unwrap_or_default()
}

pub fn set_value(i: &Option<Entity<w::InputState>>, v: &str, window: &mut Window, cx: &mut App) {
    if let Some(i) = i { let v = v.to_string(); i.update(cx, |s, cx| s.set_value(v, window, cx)); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[::core::prelude::v1::test]
    fn reading() {
        let v = json!({ "a": "x", "b": 3, "c": null, "d": true, "e": "7" });
        assert_eq!((s(&v, "a"), s(&v, "b"), s(&v, "c"), s(&v, "z")), ("x".into(), "3".into(), String::new(), String::new()));
        assert_eq!((n(&v, "b"), n(&v, "e"), n(&v, "a")), (3.0, 7.0, 0.0));
        assert!(t(&v, "d") && t(&v, "a") && !t(&v, "c") && !t(&v, "z"));
        assert_eq!(enc("a b/c:d"), "a%20b%2Fc%3Ad");
        assert_eq!(refused(Ok(json!({ "ok": false })), "Failed"), Err("Failed".into()));
        assert_eq!(refused(Ok(json!({ "ok": false, "error": "no" })), "Failed"), Err("no".into()));
    }
}
