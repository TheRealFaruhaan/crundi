//! Control from a Crundi server: the client registers on the server's /ws as
//! its native host, exactly as the Electron client does (app/main.js
//! connectNativeHost), and answers the server's browser requests for the two
//! panels. The server's own browser module (src/browser.js) drives it, so the
//! chats' browser_* tools reach these panels unchanged.
//!
//! The socket lives on its own thread; every request is handed to the main
//! thread (CEF and Slint live there) and its answer comes back over a channel.

use serde_json::{json, Value};
use std::{sync::mpsc, thread, time::Duration};
use tungstenite::{stream::MaybeTlsStream, Message};

use crate::browser;

type Reply = Box<dyn FnOnce(Value) + 'static>;

pub fn start(url: String) {
    let (tx, rx) = mpsc::channel::<Value>();
    thread::spawn(move || loop {
        let shown = url.split('?').next().unwrap_or("").to_string();
        match tungstenite::connect(url.as_str()) {
            Ok((mut ws, _)) => {
                if let MaybeTlsStream::Plain(s) = ws.get_mut() { let _ = s.set_read_timeout(Some(Duration::from_millis(30))); }
                let reg = json!({ "type": "register-native-host", "capabilities": ["browser-mouse"], "deviceId": "native-prototype" });
                if ws.send(Message::text(reg.to_string())).is_err() { continue; }
                ui_status(format!("Connected to {shown}, registering"));
                loop {
                    while let Ok(reply) = rx.try_recv() {
                        let _ = ws.send(Message::text(json!({ "type": "native-host-result", "payload": reply }).to_string()));
                    }
                    match ws.read() {
                        Ok(Message::Text(t)) => on_message(t.as_str(), tx.clone(), &shown),
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                        Err(_) => break,
                    }
                }
                ui_status(format!("Disconnected from {shown}; trying again"));
            }
            Err(e) => ui_status(format!("Cannot reach {shown}: {e}")),
        }
        thread::sleep(Duration::from_secs(3));
    });
}

/// CRUNDI_TRACE=1: each request and reply on stderr (where a run stalls).
fn trace(line: &str) {
    if std::env::var_os("CRUNDI_TRACE").is_some() {
        eprintln!("[native] {line}");
    }
}

fn ui_status(text: String) {
    let _ = slint::invoke_from_event_loop(move || browser::status(&text));
}

fn on_message(text: &str, tx: mpsc::Sender<Value>, shown: &str) {
    let Ok(m) = serde_json::from_str::<Value>(text) else { return };
    match m.get("type").and_then(|t| t.as_str()) {
        Some("native-host-ready") => ui_status(format!("Driven by {shown} (native host)")),
        Some("native-host") => {
            let payload = m.get("payload").cloned().unwrap_or(Value::Null);
            let request_id = payload.get("requestId").cloned().unwrap_or(Value::Null);
            // A notice (no requestId, e.g. "browsers"): nothing to answer.
            if request_id.is_null() { return; }
            trace(&format!("request {} {}", payload.get("type").and_then(|t| t.as_str()).unwrap_or("?"), request_id));
            let _ = slint::invoke_from_event_loop(move || {
                let reply: Reply = Box::new(move |mut result: Value| {
                    if let Some(o) = result.as_object_mut() {
                        o.insert("type".into(), json!("browserResult"));
                        o.insert("requestId".into(), request_id);
                    }
                    trace(&format!("reply {} ok={}", result["requestId"], result["ok"]));
                    let _ = tx.send(result);
                });
                browser::defer(move || dispatch(payload, reply));
            });
        }
        _ => {}
    }
}

fn err(reply: Reply, e: impl Into<String>) { reply(json!({ "ok": false, "error": e.into() })) }

fn panel_of(key: &str) -> Option<usize> {
    browser::with(|st| st.panels.iter().position(|p| p.key.as_deref() == Some(key)))
}

/// The server's request, on the main thread. Message types and fields are
/// the ones src/browser.js sends a native host.
fn dispatch(m: Value, reply: Reply) {
    let ty = m.get("type").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let key = m.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
    let s = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let n = |k: &str| m.get(k).and_then(|v| v.as_f64());

    if ty == "browserOpen" {
        // A browser for this key: the first panel nobody is driving.
        let free = browser::with(|st| st.panels.iter().position(|p| p.key.is_none()));
        let Some(i) = panel_of(&key).or(free) else { return err(reply, "Both panels are in use: close one first") };
        browser::with(|st| st.panels[i].key = Some(key.clone()));
        browser::sync(i);
        let url = s("url");
        if url.is_empty() { return reply(json!({ "ok": true })); }
        return browser::navigate(i, &url, Box::new(move || reply(json!({ "ok": true }))));
    }

    // Prototype-only: the whole window, for checking a build on any OS.
    if ty == "windowSnapshot" {
        return match browser::window_snapshot() {
            Some(data) => reply(json!({ "ok": true, "data": data })),
            None => err(reply, "No snapshot"),
        };
    }

    let Some(i) = panel_of(&key) else { return err(reply, format!("No panel for browser '{key}' (open it first)")) };
    match ty.as_str() {
        "browserClose" => {
            browser::with(|st| st.panels[i].key = None);
            browser::sync(i);
            reply(json!({ "ok": true }));
        }
        "browserNavigate" => {
            let url = s("url");
            browser::navigate(i, &url, Box::new(move || {
                let u = browser::with(|st| st.panels[i].url.clone());
                reply(json!({ "ok": true, "url": u }));
            }));
        }
        "browserEval" => browser::eval(i, &s("code"), Box::new(move |r| match r {
            Ok(v) => reply(json!({ "ok": true, "result": match v { Value::String(s) => s, other => other.to_string() } })),
            Err(e) => err(reply, e),
        })),
        "browserReadPage" => browser::eval(i, "JSON.stringify({ title: document.title, url: location.href, text: (document.body ? document.body.innerText : '').slice(0, 50000) })", Box::new(move |r| match r {
            Ok(Value::String(s)) => { let v: Value = serde_json::from_str(&s).unwrap_or(Value::Null); reply(json!({ "ok": true, "title": v["title"], "url": v["url"], "text": v["text"], "consoleLogs": [] })) }
            Ok(_) => err(reply, "no page"),
            Err(e) => err(reply, e),
        })),
        "browserClick" => {
            let sel = serde_json::to_string(&s("selector")).unwrap_or_default();
            browser::eval(i, &format!("(function(){{var el=document.querySelector({sel}); if(!el) return false; el.click(); return true;}})()"), Box::new(move |r| match r {
                Ok(Value::Bool(true)) => reply(json!({ "ok": true })),
                Ok(_) => err(reply, "Element not found"),
                Err(e) => err(reply, e),
            }));
        }
        // On the GPU path the frame is a texture: Chromium takes the picture.
        "browserScreenshot" if browser::gpu_on() => browser::cdp(i, "Page.captureScreenshot", json!({ "format": "png" }), Box::new(move |ok, v| {
            match v.get("data").and_then(|d| d.as_str()) {
                Some(data) if ok => reply(json!({ "ok": true, "data": data })),
                _ => err(reply, "Screenshot failed"),
            }
        })),
        "browserScreenshot" => match browser::screenshot(i) {
            Some(data) => reply(json!({ "ok": true, "data": data })),
            None => err(reply, "No frame painted yet"),
        },
        "browserMouse" => match browser::mouse(i, n("x").unwrap_or(0.0) as f32, n("y").unwrap_or(0.0) as f32, &s("action")) {
            Ok(()) => reply(json!({ "ok": true, "x": n("x"), "y": n("y"), "held": Value::Null })),
            Err(e) => err(reply, e),
        },
        "browserResize" => {
            browser::resize(i, n("width").unwrap_or(1280.0) as i32, n("height").unwrap_or(800.0) as i32);
            reply(json!({ "ok": true }));
        }
        // Prototype-only requests (not in today's server): a named device,
        // a pin where the person sees it, and reading back the marks.
        "browserDevice" => {
            let name = s("device").to_lowercase();
            match browser::DEVICES.iter().position(|d| d.name.to_lowercase() == name) {
                Some(d) => { browser::set_device(i, d); reply(json!({ "ok": true, "device": browser::DEVICES[d].name })) }
                None => err(reply, format!("No device '{}'", s("device"))),
            }
        }
        "browserPoint" => {
            browser::point(i, n("x").unwrap_or(0.0) as f32, n("y").unwrap_or(0.0) as f32, &s("label"));
            reply(json!({ "ok": true }));
        }
        "browserMarks" => reply(json!({ "ok": true, "marks": browser::marks(i) })),
        other => err(reply, format!("{other} is not in this prototype")),
    }
}
