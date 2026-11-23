//! The client as a Crundi server's native host (app/main.js
//! connectNativeHost; spec specs/desktop/native-host.md): it registers on the
//! server's /ws and answers the server's browser requests, so the chats'
//! browser_* tools drive the hidden browsers ("<alias>:<name>") and the
//! drawer's tabs ("drawer:<tabId>") here.
//!
//! Ported from the Slint client's src/server.rs. Signed in, the live socket
//! (crundi_core::live) carries it, through `crundi_core::host` (this is the
//! host plugged in there): it asks `registration()` what to send on connect
//! and hands every "native-host" message to `on_message`; binary frames are
//! the drawer's net tunnel (crundi_webview::net). On its own (CRUNDI_SERVER,
//! test/drive.mjs) `start` opens a socket of its own.
//!
//! Every request is handled on the main thread (CEF lives there): the live
//! thread queues it, the pump runs it (`pump`); its answer goes back over a
//! channel, once, or a timeout's error.

use serde_json::{json, Map, Value};
use std::{cell::RefCell, rc::Rc, sync::{mpsc, Mutex}, thread, time::Duration};
use tungstenite::{stream::MaybeTlsStream, Message};

use crate::drawer;
use crundi_webview::{engine as browser, later};

/// What the socket's thread hands the main thread.
enum In {
    Ready,
    Down,
    Status(String),
    Notify(Value),
    Request(Value, mpsc::Sender<Value>),
}

static INBOX: Mutex<Vec<In>> = Mutex::new(Vec::new());
/// Queued for the main thread, which is woken for it (the pump runs only when woken).
/// Requests from the server wait (the driver's pump reasons).
pub fn waiting() -> bool { !INBOX.lock().unwrap().is_empty() }

fn post(m: In) { INBOX.lock().unwrap().push(m); crundi_webview::wake(); }

/// The socket's messages, on the main thread (the area's pump).
pub fn pump() {
    let all = std::mem::take(&mut *INBOX.lock().unwrap());
    for m in all {
        match m {
            In::Ready => crate::host_up(),
            In::Down => crate::host_down(),
            In::Status(t) => browser::status(&t),
            In::Notify(p) => crate::notify(&p),
            In::Request(payload, tx) => {
                let request_id = payload.get("requestId").cloned().unwrap_or(Value::Null);
                let reply: Reply = Box::new(move |mut result: Value| {
                    if let Some(o) = result.as_object_mut() {
                        o.insert("type".into(), json!("browserResult"));
                        o.insert("requestId".into(), request_id);
                    }
                    trace(&format!("reply {} ok={}", result["requestId"], result["ok"]));
                    let _ = tx.send(result);
                });
                browser::defer(move || handle(payload, reply));
            }
        }
    }
}

/// This client as crundi_core's native host.
pub struct Host;
impl crundi_core::host::NativeHost for Host {
    fn registration(&self) -> Option<Value> { registration() }
    fn set_binary_out(&self, tx: Option<mpsc::Sender<Vec<u8>>>) { set_binary_out(tx) }
    fn on_message(&self, text: &str, tx: mpsc::Sender<Value>, shown: &str) { on_message(text, tx, shown) }
    fn on_binary(&self, b: Vec<u8>) { on_binary(b) }
}

type Reply = Box<dyn FnOnce(Value) + 'static>;

/// What this client can do as a host (the server gates tools on these).
pub const CAPABILITIES: [&str; 4] = ["notify", "browser-touch", "browser-drawer", "browser-mouse"];
/// At most this many hidden browsers (main.js).
pub const MAX_HIDDEN: usize = 20;

static FEATURES: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Binary frames out (the net tunnel), on the socket that is up now.
static BIN_OUT: Mutex<Option<mpsc::Sender<Vec<u8>>>> = Mutex::new(None);

/// The register message for a new socket (None: this client does not host).
/// CRUNDI_NATIVE_HOST=0 turns hosting off.
pub fn registration() -> Option<Value> {
    if std::env::var("CRUNDI_NATIVE_HOST").is_ok_and(|v| v == "0") { return None; }
    Some(register())
}

/// The register message: what this host can do, its device, and who it is
/// (dev:app/main.js connectNativeHost: name, clientKind and version, so a
/// server with several PCs attached can tell them apart).
fn register() -> Value {
    json!({ "type": "register-native-host", "capabilities": CAPABILITIES, "deviceId": device_id(),
        "name": gethostname::gethostname().to_string_lossy(), "clientKind": CLIENT_KIND, "version": crundi_shell::lifecycle::version() })
}

/// The kind this client says it is (the workers' kinds: src/workers.js).
pub const CLIENT_KIND: &str = "native";

/// How many times this window told the server it came to the front.
static ACTIVE_SENT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The person came to this window (dev:app/main.js browser-window-focus):
/// the server sends its new pages here when several PCs are attached.
pub fn window_active(cx: &gpui::App) {
    if registration().is_none() || BIN_OUT.lock().unwrap().is_none() { return; }
    crundi_shell::ws_send(cx, &json!({ "type": "native-host-active" }));
    ACTIVE_SENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
pub fn active_sent() -> u64 { ACTIVE_SENT.load(std::sync::atomic::Ordering::Relaxed) }

/// This PC's id for the server (homepool, the host list): made once, kept.
pub fn device_id() -> String {
    let path = crundi_core::store::config_dir().join("device-id");
    if let Ok(s) = std::fs::read_to_string(&path) { let s = s.trim().to_string(); if s.starts_with("pc-") { return s; } }
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let id = format!("pc-{:x}{}", n.wrapping_mul(0x9e37_79b9_7f4a_7c15) & 0xffff_ffff_ffff, radix36((n / 1_000_000) as u64));
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
    let _ = std::fs::write(&path, &id);
    id
}

fn radix36(mut n: u64) -> String {
    let mut v = vec![];
    while n > 0 { v.push(std::char::from_digit((n % 36) as u32, 36).unwrap_or('0')); n /= 36; }
    v.iter().rev().collect()
}

/// The server's features ("net-tunnel", "browser-drawer", "browser-homepool").
pub fn features() -> Vec<String> { FEATURES.lock().unwrap().clone() }

/// The socket is up (Some: where binary frames go) or down (None).
pub fn set_binary_out(tx: Option<mpsc::Sender<Vec<u8>>>) {
    let down = tx.is_none();
    *BIN_OUT.lock().unwrap() = tx;
    if down {
        FEATURES.lock().unwrap().clear();
        post(In::Down);
    }
}

/// A binary frame to the server (false when no socket is up).
pub fn send_binary(b: Vec<u8>) -> bool {
    BIN_OUT.lock().unwrap().as_ref().is_some_and(|tx| tx.send(b).is_ok())
}

/// A binary frame from the server (any thread).
pub fn on_binary(b: Vec<u8>) {
    crundi_webview::net::from_server(b);
}

// ─── Browser-only mode (CRUNDI_SERVER: test/drive.mjs) ───

pub fn start(url: String) {
    let (tx, rx) = mpsc::channel::<Value>();
    thread::spawn(move || loop {
        let shown = url.split('?').next().unwrap_or("").to_string();
        match tungstenite::connect(url.as_str()) {
            Ok((mut ws, _)) => {
                if let MaybeTlsStream::Plain(s) = ws.get_mut() { let _ = s.set_read_timeout(Some(Duration::from_millis(30))); }
                let reg = registration().unwrap_or_else(register);
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
    if std::env::var_os("CRUNDI_TRACE").is_some() { eprintln!("[native] {line}"); }
}

fn ui_status(text: String) { post(In::Status(text)); }

/// A "native-host" or "native-host-ready" message (any thread).
pub fn on_message(text: &str, tx: mpsc::Sender<Value>, shown: &str) {
    let Ok(m) = serde_json::from_str::<Value>(text) else { return };
    match m.get("type").and_then(|t| t.as_str()) {
        Some("native-host-ready") => {
            let f: Vec<String> = m.get("features").and_then(|f| f.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            *FEATURES.lock().unwrap() = f;
            ui_status(format!("Driven by {shown} (native host)"));
            post(In::Ready);
        }
        Some("native-host") => {
            let payload = m.get("payload").cloned().unwrap_or(Value::Null);
            let request_id = payload.get("requestId").cloned().unwrap_or(Value::Null);
            // A notice (no requestId): a notification to show; "browsers" is ignored.
            if request_id.is_null() {
                if payload.get("type").and_then(|t| t.as_str()) == Some("notify") { post(In::Notify(payload)); }
                return;
            }
            trace(&format!("request {} {}", payload.get("type").and_then(|t| t.as_str()).unwrap_or("?"), request_id));
            post(In::Request(payload, tx));
        }
        _ => {}
    }
}

// ─── Requests ───

fn err_v(e: impl Into<String>) -> Value { json!({ "ok": false, "error": e.into() }) }

/// The answer once: the handler's, or the timeout's error (main.js withTimeout).
fn guarded(ty: &str, ms: u64, reply: Reply) -> Reply {
    let slot: Rc<RefCell<Option<Reply>>> = Rc::new(RefCell::new(Some(reply)));
    let s2 = slot.clone();
    let ty = ty.to_string();
    let timer = crundi_webview::later_id(Duration::from_millis(ms), move || {
        if let Some(r) = s2.borrow_mut().take() { r(err_v(format!("{ty} timed out after {ms}ms (page may be stalled or showing a dialog)"))); }
    });
    // Answered in time: the timeout goes with it (else it wakes the idle pump later, F155).
    Box::new(move |v| { if let Some(r) = slot.borrow_mut().take() { crundi_webview::cancel(timer); r(v); } })
}

fn timeout_of(ty: &str, m: &Value) -> u64 {
    match ty {
        "browserWait" => m.get("timeout").and_then(|t| t.as_u64()).unwrap_or(30000) + 3000,
        "browserOpen" | "browserNavigate" | "browserDrawerOpen" | "browserDrawerSetNetwork" => 35000,
        "browserMouse" => m.get("duration").and_then(|t| t.as_u64()).unwrap_or(0).min(10000) + 25000,
        "browserTouch" => m.get("steps").and_then(|s| s.as_array()).map(|a| a.iter().map(|x| x.get("wait").and_then(|w| w.as_u64()).unwrap_or(0)).sum::<u64>()).unwrap_or(0) + 25000,
        _ => 25000,
    }
}

fn handle(m: Value, reply: Reply) {
    let ty = m.get("type").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let reply = guarded(&ty, timeout_of(&ty, &m), reply);
    let key = m.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
    // A drawer tab's answers lose the tab's passwords (BD redact).
    let reply: Reply = if key.starts_with("drawer:") {
        let k = key.clone();
        Box::new(move |v| reply(crate::passwords::redact(&k, v)))
    } else { reply };
    dispatch(&ty, &key, m, reply);
}

fn s(m: &Value, k: &str) -> String { m.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string() }
fn n(m: &Value, k: &str) -> Option<f64> { m.get(k).and_then(|v| v.as_f64()) }

/// The panel a key names: a hidden browser's, or a drawer tab's (made and
/// checked first: BD prepare).
fn target(ty: &str, key: &str, m: &Value) -> Result<usize, String> {
    if let Some(id) = key.strip_prefix("drawer:") {
        if !crate::rules::valid_tab_id(id) { return Err("No such drawer tab".into()); }
        let Some((project, t)) = drawer::find(id) else { return Err("No such drawer tab (it may have been closed)".into()) };
        if ty != "browserClose" && ty != "browserResize" {
            let alias = s(m, "alias").to_lowercase();
            if !alias.is_empty() && alias != project { return Err("That drawer tab belongs to another project.".into()); }
            if let Some(net) = t.network.strip_prefix("hp:") {
                if !m.get("hpAllowed").and_then(|v| v.as_bool()).unwrap_or(false) {
                    return Err(format!("This tab goes out through homepool ({net}). A chat may use homepool tabs only once the owner approves it: it asks the owner (Homepool › Requests), then try again."));
                }
            }
        }
        return drawer::ensure(id).ok_or_else(|| "The drawer tab did not load".to_string());
    }
    browser::with(|st| st.panels.iter().position(|p| !p.closed && p.key.as_deref() == Some(key))).ok_or_else(|| "Browser not found".into())
}

/// The page's address now.
fn url_of(i: usize) -> String { browser::with(|st| st.panels[i].url.clone()) }

fn eval_v(i: usize, code: String, done: Box<dyn FnOnce(Result<Value, String>)>) { browser::eval(i, &code, done); }

fn js(v: &str) -> String { serde_json::to_string(v).unwrap_or_default() }

fn dispatch(ty: &str, key: &str, m: Value, reply: Reply) {
    match ty {
        // ─── The drawer ───
        "browserDrawerList" => return reply(drawer_list(&m)),
        "browserDrawerOpen" => return drawer_open(&m, reply),
        // ─── Opening a hidden browser ───
        "browserOpen" => return open(key, &m, reply),
        // Prototype-only: the whole window, for checking a build on any OS.
        "windowSnapshot" => return match browser::window_snapshot() {
            Some(data) => reply(json!({ "ok": true, "data": data })),
            None => reply(err_v("No snapshot")),
        },
        _ => {}
    }
    let i = match target(ty, key, &m) { Ok(i) => i, Err(e) => {
        // Closing what is gone is fine.
        if ty == "browserClose" && e == "Browser not found" { return reply(err_v(e)); }
        return reply(err_v(e));
    } };
    if let Some(id) = key.strip_prefix("drawer:") { drawer::wake(id); }
    match ty {
        "browserClose" => {
            if let Some(id) = key.strip_prefix("drawer:") { drawer::close_tab(id); } else { browser::close_panel(i); }
            reply(json!({ "ok": true }))
        }
        "browserNavigate" => {
            let url = s(&m, "url");
            if key.starts_with("drawer:") && !crate::rules::ok_url(&url) { return reply(err_v("Only http and https addresses open here.")); }
            load(i, &url, Box::new(move |r| match r {
                Ok(()) => reply(json!({ "ok": true, "url": url_of(i) })),
                Err(e) => reply(err_v(e)),
            }));
        }
        "browserScreenshot" => crundi_webview::shot::capture(i, Box::new(move |r| match r {
            Ok(png) => { use base64::Engine; reply(json!({ "ok": true, "data": base64::engine::general_purpose::STANDARD.encode(png) })) }
            Err(_) => reply(err_v("Screenshot failed")),
        })),
        "browserReadPage" => eval_v(i, "(document.body ? document.body.innerText : '').substring(0, 50000)".into(), Box::new(move |r| match r {
            Ok(v) => {
                let (title, url, logs) = browser::with(|st| { let p = &st.panels[i]; (p.title.clone(), p.url.clone(), p.console.iter().rev().take(20).rev().map(log_entry).collect::<Vec<_>>()) });
                reply(json!({ "ok": true, "title": title, "url": url, "text": v.as_str().unwrap_or(""), "consoleLogs": logs }))
            }
            Err(e) => reply(err_v(e)),
        })),
        "browserSource" => eval_v(i, "document.documentElement.outerHTML".into(), Box::new(move |r| match r {
            Ok(v) => { let (t, u) = title_url(i); reply(json!({ "ok": true, "html": v.as_str().unwrap_or(""), "url": u, "title": t })) }
            Err(e) => reply(err_v(e)),
        })),
        "browserClick" => eval_v(i, format!("(function(){{var el=document.querySelector({}); if(el) el.click(); return !!el;}})()", js(&s(&m, "selector"))), Box::new(move |r| match r {
            Ok(_) => reply(json!({ "ok": true, "url": url_of(i) })),
            Err(e) => reply(err_v(e)),
        })),
        "browserType" => {
            let sel = s(&m, "selector");
            let text = s(&m, "text");
            let focus = if sel.is_empty() { "true".to_string() } else { format!("(function(){{var el=document.querySelector({}); if(el) el.focus(); return true;}})()", js(&sel)) };
            eval_v(i, focus, Box::new(move |r| {
                if let Err(e) = r { return reply(err_v(e)); }
                browser::cdp(i, "Input.insertText", json!({ "text": text }), Box::new(move |ok, v| {
                    if ok { reply(json!({ "ok": true, "url": url_of(i) })) } else { reply(err_v(kit_msg(&v))) }
                }));
            }));
        }
        "browserFill" => eval_v(i, format!("(function(s,v){{var el=document.querySelector(s); if(!el) throw new Error('Element not found'); el.focus(); el.value=v; el.dispatchEvent(new Event('input',{{bubbles:true}})); el.dispatchEvent(new Event('change',{{bubbles:true}})); return true;}})({},{})", js(&s(&m, "selector")), js(&s(&m, "value"))), Box::new(move |r| match r {
            Ok(_) => reply(json!({ "ok": true, "url": url_of(i) })),
            Err(e) => reply(err_v(clean_err(&e))),
        })),
        "browserSelect" => eval_v(i, format!("(function(s,v){{var el=document.querySelector(s); if(!el) throw new Error('Element not found'); el.value=v; el.dispatchEvent(new Event('change',{{bubbles:true}})); return true;}})({},{})", js(&s(&m, "selector")), js(&s(&m, "value"))), Box::new(move |r| match r {
            Ok(_) => reply(json!({ "ok": true })),
            Err(e) => reply(err_v(clean_err(&e))),
        })),
        "browserEval" => eval_v(i, s(&m, "code"), Box::new(move |r| match r {
            Ok(Value::Null) => reply(json!({ "ok": true })),
            Ok(Value::String(x)) => reply(json!({ "ok": true, "result": x })),
            Ok(v) => reply(json!({ "ok": true, "result": v.to_string() })),
            Err(e) => reply(err_v(clean_err(&e))),
        })),
        "browserMouse" => crate::pointer::mouse(i, key, &m, reply),
        "browserTouch" => crate::pointer::touch(i, &m, reply),
        "browserResize" => {
            let (w, h) = (n(&m, "width").unwrap_or(1280.0) as i32, n(&m, "height").unwrap_or(720.0) as i32);
            if let Some(id) = key.strip_prefix("drawer:") { drawer::set_custom(id, w, h); } else { browser::resize(i, w.clamp(200, 1920), h.clamp(200, 1080)); }
            reply(json!({ "ok": true }))
        }
        "browserScroll" => {
            let sel = s(&m, "selector");
            let target = if sel.is_empty() { "window".to_string() } else { format!("document.querySelector({})", js(&sel)) };
            eval_v(i, format!("(function(){{var t={target}; if(t) t.scrollBy({},{}); return true;}})()", n(&m, "x").unwrap_or(0.0), n(&m, "y").unwrap_or(0.0)), Box::new(move |r| match r {
                Ok(_) => reply(json!({ "ok": true })),
                Err(e) => reply(err_v(clean_err(&e))),
            }));
        }
        "browserWait" => {
            let sel = s(&m, "selector");
            let to = m.get("timeout").and_then(|t| t.as_u64()).unwrap_or(30000);
            eval_v(i, format!("new Promise(function(res,rej){{var s={sel}; if(document.querySelector(s)) return res(true); var o=new MutationObserver(function(){{ if(document.querySelector(s)){{ o.disconnect(); clearTimeout(t); res(true); }} }}); o.observe(document.body||document.documentElement,{{childList:true,subtree:true}}); var t=setTimeout(function(){{ o.disconnect(); rej(new Error('Timeout waiting for '+s)); }},{to}); }})", sel = js(&sel)), Box::new(move |r| match r {
                Ok(_) => reply(json!({ "ok": true, "found": true })),
                Err(e) => reply(err_v(clean_err(&e))),
            }));
        }
        "browserGoBack" | "browserGoForward" => {
            browser::history(i, if ty == "browserGoBack" { -1 } else { 1 });
            // The new address once the navigation commits.
            later(Duration::from_millis(400), move || reply(json!({ "ok": true, "url": url_of(i) })));
        }
        "browserConsole" => {
            let (logs, total) = browser::with(|st| { let c = &st.panels[i].console; (c.iter().map(log_entry).collect::<Vec<_>>(), c.len()) });
            if m.get("countOnly").and_then(|v| v.as_bool()).unwrap_or(false) { return reply(json!({ "ok": true, "count": total, "total": total })); }
            let (a, b) = range(&m, total);
            if m.get("clear").and_then(|v| v.as_bool()).unwrap_or(false) { browser::with(|st| st.panels[i].console.clear()); }
            reply(json!({ "ok": true, "logs": logs[a..b], "total": total }))
        }
        "browserNetwork" => match s(&m, "action").as_str() {
            "start" => {
                browser::with(|st| { let p = &mut st.panels[i]; p.net_on = true; p.net_log.clear(); });
                browser::cdp(i, "Network.enable", json!({}), Box::new(move |_, _| reply(json!({ "ok": true, "recording": true }))));
            }
            "stop" => { browser::with(|st| st.panels[i].net_on = false); reply(json!({ "ok": true, "recording": false })) }
            "clear" => { browser::with(|st| st.panels[i].net_log.clear()); reply(json!({ "ok": true })) }
            "log" => {
                let (reqs, rec) = browser::with(|st| { let p = &st.panels[i]; (p.net_log.iter().cloned().collect::<Vec<_>>(), p.net_on) });
                let total = reqs.len();
                if m.get("countOnly").and_then(|v| v.as_bool()).unwrap_or(false) { return reply(json!({ "ok": true, "recording": rec, "total": total })); }
                let (a, b) = range(&m, total);
                reply(json!({ "ok": true, "requests": reqs[a..b], "recording": rec, "total": total }))
            }
            _ => reply(err_v("Unknown network action")),
        },
        "browserCookies" => cookies(i, &m, reply),
        "browserSnapshot" => {
            let depth = n(&m, "maxDepth").unwrap_or(10.0) as i64;
            let max = n(&m, "maxLength").unwrap_or(50000.0) as i64;
            eval_v(i, format!("(function(){{function w(e,d){{ if(!e||d>{depth}) return null; var o={{role:e.getAttribute&&e.getAttribute('role')||(e.tagName||'').toLowerCase(), label:(e.getAttribute&&e.getAttribute('aria-label'))||(e.textContent||'').trim().slice(0,80)}}; var k=[]; for(var c of (e.children||[])){{ var x=w(c,d+1); if(x) k.push(x); }} if(k.length) o.children=k; return o; }} return JSON.stringify(w(document.body,0),null,2).substring(0,{max}); }})()"), Box::new(move |r| match r {
                Ok(v) => { let snap = v.as_str().unwrap_or("").to_string(); let (t, u) = title_url(i); reply(json!({ "ok": true, "snapshot": snap, "tree": snap, "length": snap.len(), "url": u, "title": t })) }
                Err(e) => reply(err_v(clean_err(&e))),
            }));
        }
        "browserElements" => {
            let sel = Some(s(&m, "selector")).filter(|x| !x.is_empty()).unwrap_or_else(|| "a,button,input,select,textarea,[role=\"button\"],[onclick]".into());
            let count_only = m.get("countOnly").and_then(|v| v.as_bool()).unwrap_or(false);
            let m2 = m.clone();
            eval_v(i, format!("JSON.stringify(Array.from(document.querySelectorAll({})).map(function(e){{return {{tag:e.tagName.toLowerCase(),id:e.id||undefined,class:(typeof e.className==='string'&&e.className)||undefined,text:(e.innerText||e.value||'').trim().slice(0,80),type:e.type||undefined,href:e.href||undefined,value:e.value||undefined}};}}))", js(&sel)), Box::new(move |r| match r {
                Ok(v) => {
                    let all: Vec<Value> = v.as_str().and_then(|x| serde_json::from_str(x).ok()).unwrap_or_default();
                    let total = all.len();
                    let (t, u) = title_url(i);
                    if count_only { return reply(json!({ "ok": true, "count": total, "total": total, "url": u, "title": t })); }
                    let (a, b) = range(&m2, total);
                    reply(json!({ "ok": true, "elements": all[a..b], "total": total, "url": u, "title": t }))
                }
                Err(e) => reply(err_v(clean_err(&e))),
            }));
        }
        "browserPdf" => browser::cdp(i, "Page.printToPDF", json!({ "landscape": m.get("landscape").and_then(|v| v.as_bool()).unwrap_or(false), "printBackground": true }), Box::new(move |ok, v| {
            match v.get("data").and_then(|d| d.as_str()) { Some(d) if ok => reply(json!({ "ok": true, "data": d })), _ => reply(err_v(kit_msg(&v))) }
        })),
        "browserDrawerSetNetwork" => {
            let Some(id) = key.strip_prefix("drawer:") else { return reply(err_v("Only drawer tabs can do that")) };
            let Some(net) = crate::rules::norm_network(&s(&m, "network")) else { return reply(err_v("network must be \"client\", \"server\" or \"homepool:<network>\"")) };
            if net.starts_with("hp:") && !m.get("hpAllowed").and_then(|v| v.as_bool()).unwrap_or(false) { return reply(err_v("A homepool tab needs the owner's approval for this chat.")); }
            drawer::set_network(id, &net);
            let i2 = drawer::find(id).and_then(|(_, t)| t.panel);
            let label = drawer::label(&net);
            let Some(i2) = i2 else { return reply(json!({ "ok": true, "network": net, "networkLabel": label, "url": "" })) };
            // Answer once the tab's page has loaded again on its new network
            // (as Electron's, which awaits the load), so the next call meets it.
            wait_created(i2, 0, Box::new(move || wait_settled(i2, 0, Box::new(move || {
                reply(json!({ "ok": true, "network": net, "networkLabel": label, "url": url_of(i2) }))
            }))));
        }
        "browserDrawerAnnotate" => crate::marks::annotate_request(i, key, &m, reply),
        "browserDrawerAnnotations" => crate::marks::annotations_request(i, key, &m, reply),
        // Prototype-only requests, kept for test/drive.mjs: a named device,
        // a pin, the marks.
        "browserDevice" => {
            let name = s(&m, "device").to_lowercase();
            match browser::DEVICES.iter().position(|d| d.name.to_lowercase() == name) {
                Some(d) => { browser::set_device(i, d); reply(json!({ "ok": true, "device": browser::DEVICES[d].name })) }
                None => reply(err_v(format!("No device '{}'", s(&m, "device")))),
            }
        }
        "browserPoint" => { browser::point(i, n(&m, "x").unwrap_or(0.0) as f32, n(&m, "y").unwrap_or(0.0) as f32, &s(&m, "label")); reply(json!({ "ok": true })) }
        "browserMarks" => reply(json!({ "ok": true, "marks": browser::marks(i) })),
        other => reply(err_v(format!("{other} is not supported by this app"))),
    }
}

fn title_url(i: usize) -> (String, String) { browser::with(|st| (st.panels[i].title.clone(), st.panels[i].url.clone())) }

fn log_entry(v: &Value) -> Value { json!({ "time": v["time"], "text": v["text"], "type": v["type"] }) }

/// start / end of a slice, as JavaScript's slice(start || 0, end || len).
fn range(m: &Value, len: usize) -> (usize, usize) {
    let a = n(m, "start").map(|x| x.max(0.0) as usize).unwrap_or(0).min(len);
    let b = n(m, "end").filter(|x| *x > 0.0).map(|x| x as usize).unwrap_or(len).min(len).max(a);
    (a, b)
}

fn kit_msg(v: &Value) -> String { v.get("message").and_then(|m| m.as_str()).unwrap_or("failed").to_string() }

/// "Error: X" (a thrown Error's description) → "X".
fn clean_err(e: &str) -> String {
    let first = e.lines().next().unwrap_or(e);
    first.strip_prefix("Error: ").unwrap_or(first).to_string()
}

/// Load [url] in panel [i]: done on the load's end, or its failure.
fn load(i: usize, url: &str, done: Box<dyn FnOnce(Result<(), String>)>) {
    browser::navigate(i, url, Box::new(move || {
        match browser::with(|st| st.panels.get(i).and_then(|p| p.load_error.clone())) {
            Some(e) => done(Err(e)),
            None => done(Ok(())),
        }
    }));
}

fn open(key: &str, m: &Value, reply: Reply) {
    if browser::with(|st| st.panels.iter().any(|p| !p.closed && p.key.as_deref() == Some(key))) { return reply(err_v(format!("Browser \"{key}\" already open"))); }
    if browser::with(|st| st.panels.iter().filter(|p| !p.closed && p.key.is_some()).count()) >= MAX_HIDDEN { return reply(err_v(format!("Too many browser instances (max {MAX_HIDDEN})"))); }
    let w = n(m, "width").unwrap_or(1280.0).clamp(200.0, 1920.0) as i32;
    let h = n(m, "height").unwrap_or(720.0).clamp(200.0, 1080.0) as i32;
    let url = Some(s(m, "url")).filter(|u| !u.is_empty()).unwrap_or_else(|| "about:blank".into());
    let i = browser::open_panel("about:blank", &format!("mem:{key}"));
    browser::with(|st| { let p = &mut st.panels[i]; p.key = Some(key.to_string()); p.custom = Some((w, h)); });
    browser::sync(i);
    crate::hidden_changed();
    // Loaded once the browser exists (its first frame is about:blank).
    let key = key.to_string();
    wait_created(i, 0, Box::new(move || {
        browser::apply_device(i);
        load(i, &url, Box::new(move |r| match r {
            Ok(()) => reply(json!({ "ok": true })),
            Err(e) => {
                browser::close_panel(i);
                crate::hidden_changed();
                let _ = key;
                reply(err_v(e))
            }
        }));
    }));
}

/// Until a panel has left about:blank and stopped loading (at most ~30 s).
fn wait_settled(i: usize, tries: u32, then: Box<dyn FnOnce()>) {
    let settled = browser::with(|st| st.panels.get(i).is_none_or(|p| p.closed || (!p.loading && !p.url.is_empty() && p.url != "about:blank")));
    if settled || tries > 600 { return then(); }
    later(Duration::from_millis(50), move || wait_settled(i, tries + 1, then));
}

fn wait_created(i: usize, tries: u32, then: Box<dyn FnOnce()>) {
    if browser::with(|st| st.panels[i].browser.is_some()) || tries > 100 { return then(); }
    later(Duration::from_millis(50), move || wait_created(i, tries + 1, then));
}

fn drawer_list(m: &Value) -> Value {
    let all = m.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
    let alias = s(m, "alias").to_lowercase();
    let active = drawer::active_tab().map(|t| t.id);
    let open = drawer::with(|d| d.open);
    let shown_proj = drawer::project_key();
    let tabs: Vec<Value> = drawer::with(|d| d.projects.iter().filter(|(p, _)| all || **p == alias).flat_map(|(p, ps)| ps.tabs.iter().map(move |t| (p.clone(), ps.active.clone(), t.clone()))).collect::<Vec<_>>())
        .into_iter().map(|(p, act, t)| json!({
            "tabId": t.id, "project": p, "url": t.url, "title": t.title, "network": t.network, "networkLabel": drawer::label(&t.network),
            "sleeping": t.sleeping, "active": act.as_deref() == Some(t.id.as_str()),
            "shown": open && p == shown_proj && active.as_deref() == Some(t.id.as_str()), "loaded": t.panel.is_some(),
            "device": match t.device { Some(d) => json!(browser::DEVICES[d].name), None => json!(t.custom.map(|(w, h)| format!("Custom {w}x{h}"))) },
        })).collect();
    json!({ "ok": true, "tabs": tabs })
}

fn drawer_open(m: &Value, reply: Reply) {
    let url = Some(s(m, "url")).filter(|u| !u.is_empty()).unwrap_or_else(|| "about:blank".into());
    if !crate::rules::ok_url(&url) { return reply(err_v("Only http and https addresses open in the drawer.")); }
    let alias = s(m, "alias");
    if alias.is_empty() { return reply(err_v("Which project? (no alias)")); }
    let Some(net) = crate::rules::norm_network(&s(m, "network")) else { return reply(err_v("network must be \"client\", \"server\" or \"homepool:<network>\"")) };
    if net.starts_with("hp:") && !m.get("hpAllowed").and_then(|v| v.as_bool()).unwrap_or(false) { return reply(err_v("A homepool tab needs the owner's approval for this chat.")); }
    let show = m.get("show").and_then(|v| v.as_bool()).unwrap_or(false);
    let project = kit_project_of(&alias);
    let id = match drawer::new_tab(&project, &url, &net, show, true) {
        Ok(id) => id,
        Err(e) if e.starts_with("At most") => return reply(err_v(format!("This project already has {} drawer tabs: close one first.", drawer::MAX_TABS))),
        Err(e) => return reply(err_v(e)),
    };
    if show && project.eq_ignore_ascii_case(&crate::ctx::project()) { drawer::open(); }
    let Some(i) = drawer::find(&id).and_then(|(_, t)| t.panel) else { return reply(err_v("The drawer tab did not load")) };
    let label = drawer::label(&net);
    wait_created(i, 0, Box::new(move || {
        load(i, &url, Box::new(move |r| {
            let mut v = json!({ "ok": true, "tabId": id, "network": net, "networkLabel": label, "url": url_of(i) });
            if let Err(e) = r { v["warning"] = json!(e); }
            reply(v)
        }));
    }));
}

/// The project's own spelling (aliases are matched without case).
fn kit_project_of(alias: &str) -> String {
    crate::ctx::get().projects.iter().map(|p| crate::ctx::s(p, "alias")).find(|a| a.eq_ignore_ascii_case(alias)).unwrap_or_else(|| alias.to_string())
}

fn cookies(i: usize, m: &Value, reply: Reply) {
    match s(m, "action").as_str() {
        "get" | "" => {
            let urls = Some(s(m, "url")).filter(|u| !u.is_empty()).map(|u| json!([u]));
            let filter = m.get("filter").cloned().unwrap_or(Value::Null);
            let params = match urls { Some(u) => json!({ "urls": u }), None => json!({}) };
            browser::cdp(i, "Network.getCookies", params, Box::new(move |ok, v| {
                if !ok { return reply(err_v(kit_msg(&v))); }
                let mut list = v.get("cookies").and_then(|c| c.as_array()).cloned().unwrap_or_default();
                if let Some(f) = filter.as_object() {
                    list.retain(|c| f.iter().all(|(k, want)| k == "url" || c.get(k) == Some(want) || (k == "domain" && c.get("domain").and_then(|d| d.as_str()).is_some_and(|d| d.trim_start_matches('.').ends_with(want.as_str().unwrap_or("").trim_start_matches('.'))))));
                }
                reply(json!({ "ok": true, "cookies": list }))
            }));
        }
        "set" => {
            let Some(c) = m.get("cookie").filter(|c| c.is_object()).cloned() else { return reply(err_v("cookie is required")) };
            let mut c: Map<String, Value> = c.as_object().cloned().unwrap_or_default();
            if !c.contains_key("url") && !c.contains_key("domain") { c.insert("url".into(), json!(url_of(i))); }
            if let Some(e) = c.remove("expirationDate") { c.insert("expires".into(), e); }
            browser::cdp(i, "Network.setCookie", Value::Object(c), Box::new(move |ok, v| if ok { reply(json!({ "ok": true })) } else { reply(err_v(kit_msg(&v))) }));
        }
        "delete" => {
            let name = m.get("cookie").map(|c| s(c, "name")).unwrap_or_default();
            let url = Some(s(m, "url")).filter(|u| !u.is_empty()).unwrap_or_else(|| url_of(i));
            browser::cdp(i, "Network.deleteCookies", json!({ "name": name, "url": url }), Box::new(move |ok, v| if ok { reply(json!({ "ok": true })) } else { reply(err_v(kit_msg(&v))) }));
        }
        _ => reply(err_v("Unknown cookie action")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_and_errors() {
        assert_eq!(range(&json!({}), 5), (0, 5));
        assert_eq!(range(&json!({ "start": 2, "end": 4 }), 5), (2, 4));
        assert_eq!(range(&json!({ "start": 9 }), 5), (5, 5));
        assert_eq!(clean_err("Error: Element not found\n    at <anonymous>"), "Element not found");
        assert!(CAPABILITIES.contains(&"browser-drawer"));
    }
}
