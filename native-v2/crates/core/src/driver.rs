//! The test driver's protocol (the Slint client's src/testctl.rs, unchanged
//! on the wire): with CRUNDI_TEST_PORT=<port>, one JSON command per line on
//! 127.0.0.1:<port>, one JSON answer per line. The UI side (crates/app)
//! answers each command on the UI thread, running the same functions the
//! window's buttons do, so test/phase1.mjs and the phase 2 tests drive this
//! client as they drove the Slint one.
//!
//!   {"cmd":"state"}                           what the app holds
//!   {"cmd":"props"}                           what the window shows
//!   {"cmd":"snapshot","path":"/tmp/x.png"}    the window as drawn
//!   {"cmd":"ui","name":"send","args":["hi"]}  a window action; "<area>:<name>" is an area's
//!   {"cmd":"area","area":"planning"}          an area's props alone
//!   {"cmd":"mouse","kind":"move|click|dblclick|down|up|leave|wheel","x":10,"y":20}
//!   {"cmd":"key","text":"Escape"|"Return"|"a…","mods":["ctrl"]}
//!       ("phase":"down" holds the key, "up" lets it go; neither: both)
//!   {"cmd":"resize","w":1400,"h":860}
//!
//! "state" also holds "renders": how many times each counted view (head,
//! composer, log, launcher, workbench, top-bar, sidebar, project-bar) has
//! rendered.

use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{mpsc, Mutex},
    thread,
    time::Duration,
};

/// A command for the UI thread and where its answer goes.
pub struct Request {
    pub cmd: Value,
    pub reply: mpsc::Sender<Value>,
}

impl Request {
    pub fn name(&self) -> &str { self.cmd.get("cmd").and_then(|v| v.as_str()).unwrap_or("") }
    /// The ui action's i-th argument as text (numbers and bools as written).
    pub fn arg(&self, i: usize) -> String { arg(&self.cmd, i) }
    pub fn args(&self) -> Vec<String> {
        let n = self.cmd.get("args").and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0);
        (0..n).map(|i| arg(&self.cmd, i)).collect()
    }
    pub fn str(&self, k: &str) -> String { self.cmd.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string() }
    pub fn num(&self, k: &str) -> f64 { self.cmd.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) }
    pub fn answer(self, v: Value) { let _ = self.reply.send(v); }
}

pub fn arg(c: &Value, i: usize) -> String {
    c.pointer(&format!("/args/{i}")).map(|v| match v { Value::String(s) => s.clone(), o => o.to_string() }).unwrap_or_default()
}

/// Listen on 127.0.0.1:<port>; every command goes to `to_ui` (called on the
/// connection's thread: send it to the UI thread). An answer that takes
/// longer than 20 s reads {"ok":false,"error":"timeout"}.
pub fn start(port: u16, to_ui: impl Fn(Request) + Send + Sync + Clone + 'static) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) else {
        eprintln!("[native] test driver: port {port} is taken");
        return;
    };
    eprintln!("[native] test driver on 127.0.0.1:{port}");
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let to_ui = to_ui.clone();
            thread::spawn(move || {
                let _ = conn.set_nodelay(true);
                let Ok(mut out) = conn.try_clone() else { return };
                for line in BufReader::new(conn).lines().map_while(Result::ok) {
                    let Ok(cmd) = serde_json::from_str::<Value>(&line) else { continue };
                    let (tx, rx) = mpsc::channel();
                    to_ui(Request { cmd, reply: tx });
                    let answer = rx.recv_timeout(Duration::from_secs(20)).unwrap_or(json!({ "ok": false, "error": "timeout" }));
                    // One write, sent at once (F12: `writeln!` wrote the line
                    // and its newline apart, and Nagle held the newline back
                    // until the harness's delayed ACK, ~40 ms a command).
                    let mut line = answer.to_string();
                    line.push('\n');
                    if out.write_all(line.as_bytes()).is_err() { break; }
                }
            });
        }
    });
}

/// How many times each counted view has rendered (the driver's "renders":
/// a suite checks that a change redraws only the views that show it).
static RENDERS: Mutex<Vec<(&'static str, u64)>> = Mutex::new(Vec::new());

/// A counted view rendered (call at the top of its `render`; cheap).
pub fn rendered(view: &'static str) {
    if let Ok(mut r) = RENDERS.lock() {
        match r.iter_mut().find(|(k, _)| *k == view) { Some(e) => e.1 += 1, None => r.push((view, 1)) }
    }
}

/// Every counted view's renders so far, as {"head": 12, …}.
pub fn renders() -> Value {
    let r = RENDERS.lock().map(|r| r.clone()).unwrap_or_default();
    Value::Object(r.into_iter().map(|(k, n)| (k.to_string(), json!(n))).collect())
}

/// The port from CRUNDI_TEST_PORT, if set.
pub fn port() -> Option<u16> {
    std::env::var("CRUNDI_TEST_PORT").ok().and_then(|p| p.parse().ok())
}

/// The file the attach button's picker answers with next (the driver's
/// stand-in for the system dialog, which a test can't drive).
static PICK: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

/// Canned answers for the next POSTs of a kind ("send", "cancel-queued"), in
/// order: the test's stand-in for a server that refuses or is too late.
static STUBS: Mutex<Vec<(String, Value)>> = Mutex::new(Vec::new());

pub fn set_pick(p: Option<std::path::PathBuf>) { if let Ok(mut x) = PICK.lock() { *x = p; } }
/// The driver's answer for the picker, taken once; None: the real dialog.
pub fn take_pick() -> Option<std::path::PathBuf> { PICK.lock().ok()?.take() }
pub fn pick_waiting() -> bool { PICK.lock().map(|p| p.is_some()).unwrap_or(false) }

pub fn push_stub(kind: &str, v: Value) { if let Ok(mut s) = STUBS.lock() { s.push((kind.to_string(), v)); } }
/// The next canned answer for that kind, taken once; None: the server's.
pub fn take_stub(kind: &str) -> Option<Value> {
    let mut s = STUBS.lock().ok()?;
    let i = s.iter().position(|(k, _)| k == kind)?;
    Some(s.remove(i).1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_and_stubs() {
        let c = json!({ "cmd": "ui", "name": "x", "args": ["a", 3, true] });
        assert_eq!(arg(&c, 0), "a");
        assert_eq!(arg(&c, 1), "3");
        assert_eq!(arg(&c, 2), "true");
        assert_eq!(arg(&c, 5), "");
        push_stub("send", json!({ "ok": false }));
        assert!(take_stub("cancel").is_none());
        assert_eq!(take_stub("send"), Some(json!({ "ok": false })));
        assert!(take_stub("send").is_none());
    }
}
