//! This PC as a worker (app/main.js's worker section; specs/workers-protocol.md):
//! off until switched on in Settings ("This PC as a worker"). The switch
//! joins this PC to the signed-in server (POST /api/workers join-self, kind
//! "native") and keeps the key in worker.json (0600: server, key, id,
//! workDir, enabled); the bundled `crundi-worker` runs embedded (JSON status
//! lines on stdout; closing its stdin ends it). Exit 3: the server refused
//! the key (forgotten, switched off); exit 4: replaced (not restarted);
//! otherwise restarted after 2 s, doubling to 60 s. A job or a chat starting
//! and ending on this PC is a notification.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
struct W {
    child: Option<Child>,
    pid: u32,
    run: u64,
    stopping: bool,
    backoff_ms: u64,
    retry_at: Option<Instant>,
    state: Value,
    log: Vec<String>,
}
static ST: Mutex<Option<W>> = Mutex::new(None);
static INBOX: Mutex<Vec<(u64, Ev)>> = Mutex::new(Vec::new());

enum Ev { Out(String), Err(String), Exit(Option<i32>) }

fn with<R>(f: impl FnOnce(&mut W) -> R) -> R {
    let mut g = ST.lock().unwrap();
    if g.is_none() { *g = Some(W { backoff_ms: 2000, state: json!({ "state": "off", "online": false, "detail": "", "since": 0 }), ..Default::default() }); }
    f(g.as_mut().unwrap())
}
fn post(run: u64, e: Ev) { INBOX.lock().unwrap().push((run, e)); crundi_webview::wake(); }
pub fn waiting() -> bool { !INBOX.lock().unwrap().is_empty() || !JOINED.lock().unwrap().is_empty() || with(|w| w.retry_at.is_some_and(|t| t <= Instant::now())) }

fn file() -> PathBuf { crundi_core::store::config_dir().join("worker.json") }
pub fn read() -> Value { std::fs::read(file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({})) }
fn write(v: &Value) {
    let p = file();
    let _ = std::fs::create_dir_all(crundi_core::store::config_dir());
    if std::fs::write(&p, serde_json::to_string_pretty(v).unwrap_or_default()).is_ok() {
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)); }
    }
}
fn add_log(line: &str) {
    let t = chrono::Local::now().format("%H:%M:%S");
    with(|w| { w.log.push(format!("{t}  {line}")); let n = w.log.len(); if n > 60 { w.log.drain(..n - 60); } });
}
fn set_state(next: Value) {
    with(|w| if let (Some(s), Some(n)) = (w.state.as_object_mut(), next.as_object()) { for (k, v) in n { s.insert(k.clone(), v.clone()); } });
    CHANGED.store(true, std::sync::atomic::Ordering::Relaxed);
    crundi_webview::wake();
}
static CHANGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The worker binary (wkBinary): `CRUNDI_WORKER` for tests, beside the
/// client (the package's `crundi-worker`), else a local build.
pub fn binary() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "crundi-worker.exe" } else { "crundi-worker" };
    if let Some(p) = std::env::var_os("CRUNDI_WORKER") { let p = PathBuf::from(p); return p.is_file().then_some(p); }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    [super::exe_dir().join(exe), home.join(".cache/crundi-native-target/release").join(exe), home.join(".cache/crundi-native-target/debug").join(exe)]
        .into_iter().find(|p| p.is_file())
}
fn default_dir() -> String { std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default().join("crundi-work").to_string_lossy().to_string() }
fn host_name() -> String { gethostname::gethostname().to_string_lossy().to_string() }
pub fn os() -> &'static str { if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" } }

/// wkStatus.
pub fn status() -> Value {
    let c = read();
    let s = |k: &str| c[k].as_str().unwrap_or("").to_string();
    let mut v = json!({
        "supported": true, "available": binary().is_some(), "enabled": c["enabled"].as_bool().unwrap_or(false),
        "configured": !s("server").is_empty() && !s("key").is_empty(), "server": s("server"), "id": s("id"),
        "workDir": if s("workDir").is_empty() { default_dir() } else { s("workDir") },
        "deviceId": crate::host::device_id(), "name": host_name(), "os": os(), "version": env!("CARGO_PKG_VERSION"),
    });
    with(|w| { if let (Some(o), Some(st)) = (v.as_object_mut(), w.state.as_object()) { for (k, x) in st { o.insert(k.clone(), x.clone()); } } v["log"] = json!(w.log.iter().rev().take(25).rev().cloned().collect::<Vec<_>>()); });
    v
}

/// The status line's words (pcWorkerApply).
pub fn words(s: &Value) -> String {
    if !s["available"].as_bool().unwrap_or(false) { return "This build of the app has no worker program in it yet.".into(); }
    let st = s["state"].as_str().unwrap_or("");
    let w = match st {
        "off" => if s["enabled"].as_bool().unwrap_or(false) { "Starting" } else { "Off" },
        "starting" => "Starting", "connecting" => "Connecting", "online" => "Online", "offline" => "Offline, retrying", "paused" => "Paused",
        "restarting" => "Restarting", "refused" => "Removed by the server", "replaced" => "Replaced by another copy", "missing" => "Not in this build",
        "error" => "Could not start", "conflict" => "Another worker on this PC is connected", x => x,
    };
    let d = s["detail"].as_str().unwrap_or("");
    let job = s["job"]["command"].as_str().map(|c| format!(" \u{b7} running: {c}")).unwrap_or_default();
    format!("{w}{}{job}", if d.is_empty() { String::new() } else { format!(": {d}") })
}

/// At start: on when it was left on.
pub fn init() { if read()["enabled"].as_bool().unwrap_or(false) { start(); } }

/// wkStart.
pub fn start() {
    let c = read();
    let (server, key) = (c["server"].as_str().unwrap_or("").to_string(), c["key"].as_str().unwrap_or("").to_string());
    if server.is_empty() || key.is_empty() || with(|w| w.child.is_some()) { return; }
    let Some(bin) = binary() else { set_state(json!({ "state": "missing", "online": false, "detail": "This build of the app has no worker program in it." })); return };
    let dir = c["workDir"].as_str().filter(|s| !s.is_empty()).map(str::to_string).unwrap_or_else(default_dir);
    let mut cmd = Command::new(bin);
    cmd.args(["--embedded", "--server", &server, "--key", &key, "--work-dir", &dir, "--name", &host_name(), "--kind", "native"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
    let mut child = match cmd.spawn() { Ok(c) => c, Err(e) => { set_state(json!({ "state": "error", "detail": e.to_string() })); return; } };
    let run = with(|w| { w.run += 1; w.stopping = false; w.retry_at = None; w.run });
    if let Some(o) = child.stdout.take() { std::thread::spawn(move || for l in BufReader::new(o).lines() { let Ok(l) = l else { break }; post(run, Ev::Out(l)); }); }
    if let Some(e) = child.stderr.take() { std::thread::spawn(move || for l in BufReader::new(e).lines() { let Ok(l) = l else { break }; post(run, Ev::Err(l)); }); }
    let pid = child.id();
    with(|w| { w.child = Some(child); w.pid = pid; });
    // The waiter: polls the child it does not own (the child stays here for stdin and kill).
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(200));
        let done = with(|w| if w.run != run { Some(None) } else { w.child.as_mut().and_then(|c| c.try_wait().ok().flatten()).map(|s| Some(s.code())) });
        match done { Some(Some(code)) => { post(run, Ev::Exit(code)); break; } Some(None) => break, None => {} }
    });
    set_state(json!({ "state": "starting", "online": false, "detail": "" }));
}

/// wkStop: stdin closed, killed after 3 s.
pub fn stop() {
    let child = with(|w| { w.retry_at = None; w.stopping = true; w.child.take() });
    let Some(mut c) = child else { set_state(json!({ "state": "off", "online": false })); return };
    drop(c.stdin.take());
    std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end { if c.try_wait().ok().flatten().is_some() { return; } std::thread::sleep(Duration::from_millis(50)); }
        let _ = c.kill(); let _ = c.wait();
    });
    set_state(json!({ "state": "off", "online": false }));
}

/// At quit: stopped before the client goes (3 s at most).
pub fn stop_blocking() {
    let child = with(|w| { w.stopping = true; w.retry_at = None; w.child.take() });
    let Some(mut c) = child else { return };
    drop(c.stdin.take());
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end { if c.try_wait().ok().flatten().is_some() { return; } std::thread::sleep(Duration::from_millis(50)); }
    let _ = c.kill(); let _ = c.wait();
}

/// worker:enable.
pub fn enable(server: &str, key: &str, id: &str, work_dir: &str) -> Result<(), String> {
    let server = server.trim_end_matches('/');
    let key_ok = key.starts_with("cw_w") && key.len() > 15 && key[4..14].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) && key.as_bytes()[14] == b'.';
    if !(server.starts_with("http://") || server.starts_with("https://")) || !key_ok { return Err("Bad worker settings.".into()); }
    stop();
    let mut c = read();
    let wd = if work_dir.trim().is_empty() { c["workDir"].as_str().unwrap_or("").to_string() } else { work_dir.trim().to_string() };
    c["server"] = json!(server); c["key"] = json!(key); c["id"] = json!(id); c["enabled"] = json!(true);
    if !wd.is_empty() { c["workDir"] = json!(wd); }
    write(&c);
    // Started by the pump in 300 ms (this may run on the join's thread).
    with(|w| w.retry_at = Some(Instant::now() + Duration::from_millis(300)));
    crundi_webview::wake();
    Ok(())
}

/// worker:setDir.
pub fn set_dir(dir: &str) -> Result<(), String> {
    let v = dir.trim();
    if v.is_empty() { return Err("A folder is needed.".into()); }
    let mut c = read(); c["workDir"] = json!(v); write(&c);
    if with(|w| w.child.is_some()) { stop(); with(|w| w.retry_at = Some(Instant::now() + Duration::from_millis(3500))); }
    Ok(())
}

/// worker:disable.
pub fn disable() { let mut c = read(); c["enabled"] = json!(false); write(&c); stop(); }

/// worker:start.
pub fn resume() -> Result<(), String> {
    let mut c = read();
    if c["key"].as_str().unwrap_or("").is_empty() { return Err("Not set up.".into()); }
    c["enabled"] = json!(true); write(&c); start(); Ok(())
}

/// The switch's join, done on its thread: its answer for the pump (a toast).
static JOINED: Mutex<Vec<Result<(), String>>> = Mutex::new(Vec::new());
pub fn joined(r: Result<(), String>) { JOINED.lock().unwrap().push(r); CHANGED.store(true, std::sync::atomic::Ordering::Relaxed); crundi_webview::wake(); }
pub fn take_joined() -> Vec<Result<(), String>> { std::mem::take(&mut *JOINED.lock().unwrap()) }

/// The status changed since the last look (Settings draws its part again).
pub fn take_changed() -> bool { CHANGED.swap(false, std::sync::atomic::Ordering::Relaxed) }

/// The pump: the worker's lines, its exit, a retry falling due.
pub fn pump() {
    let evs = std::mem::take(&mut *INBOX.lock().unwrap());
    for (run, e) in evs {
        if with(|w| w.run != run) { continue; }
        match e {
            Ev::Out(line) => out_line(line.trim()),
            Ev::Err(l) => { let l = l.trim(); if !l.is_empty() { add_log(&l.chars().take(300).collect::<String>()); eprintln!("[worker] {l}"); } }
            Ev::Exit(code) => exit(code),
        }
    }
    if with(|w| w.retry_at.is_some_and(|t| t <= Instant::now())) {
        with(|w| w.retry_at = None);
        if read()["enabled"].as_bool().unwrap_or(false) { start(); }
    }
}

/// When the pump must run for a retry.
pub fn next_due() -> Option<Instant> { with(|w| w.retry_at) }

fn out_line(line: &str) {
    if line.is_empty() { return; }
    let Ok(ev) = serde_json::from_str::<Value>(line) else { add_log(&line.chars().take(300).collect::<String>()); return };
    match ev["event"].as_str().unwrap_or("") {
        "status" => {
            let online = ev["online"].as_bool().unwrap_or(false);
            let since = if online { json!(chrono::Utc::now().timestamp_millis()) } else { with(|w| w.state["since"].clone()) };
            set_state(json!({ "state": if online { "online".to_string() } else { ev["state"].as_str().unwrap_or("connecting").to_string() }, "online": online, "detail": ev["detail"].as_str().unwrap_or(""), "since": since }));
            if online { with(|w| w.backoff_ms = 2000); }
        }
        k @ ("job" | "chat") => {
            let job = k == "job";
            let what = if job { ev["command"].as_str().unwrap_or("").chars().take(120).collect::<String>() } else { format!("a chat in {}", ev["project"].as_str().filter(|s| !s.is_empty()).unwrap_or("a project")) };
            let started = ev["state"] == "started";
            if job { set_state(json!({ "job": if started { json!({ "id": ev["job"], "command": what, "project": ev["project"].as_str().unwrap_or(""), "since": chrono::Utc::now().timestamp_millis() }) } else { Value::Null } })); }
            let code = ev["exitCode"].as_i64();
            let title = if started { if job { "Crundi is running a job on this PC".to_string() } else { "A Crundi chat started on this PC".to_string() } }
                else if job { if code == Some(0) { "Job finished on this PC".into() } else { format!("Job ended on this PC{}", code.map(|c| format!(" (exit {c})")).unwrap_or_default()) } }
                else { "A Crundi chat on this PC ended".into() };
            add_log(&format!("{title}: {what}"));
            let proj = ev["project"].as_str().filter(|p| !p.is_empty() && job).map(|p| format!(" ({p})")).unwrap_or_default();
            crate::notify::show_plain(&title, &format!("{what}{proj}"));
        }
        "log" => add_log(ev["text"].as_str().unwrap_or("")),
        _ => add_log(&line.chars().take(300).collect::<String>()),
    }
}

fn exit(code: Option<i32>) {
    let stopping = with(|w| { w.child = None; w.pid = 0; w.stopping });
    if stopping { set_state(json!({ "state": "off", "online": false })); return; }
    match code {
        Some(3) => {
            let mut c = read(); if let Some(o) = c.as_object_mut() { o.remove("key"); } c["enabled"] = json!(false); write(&c);
            set_state(json!({ "state": "refused", "online": false, "detail": "The server no longer accepts this PC. Switch it on again to rejoin." }));
        }
        Some(4) => set_state(json!({ "state": "replaced", "online": false, "detail": "Another copy of this worker connected in its place. Switch it off and on to take over again." })),
        c => {
            set_state(json!({ "state": "restarting", "online": false, "detail": format!("stopped (exit {}); starting again", c.map(|c| c.to_string()).unwrap_or("null".into())) }));
            with(|w| { w.retry_at = Some(Instant::now() + Duration::from_millis(w.backoff_ms)); w.backoff_ms = (w.backoff_ms * 2).min(60_000); });
        }
    }
}

/// The Settings switch on (pcWorkerToggle): join this PC to the signed-in
/// server, then run the worker; `done` gets the error to toast, if any.
pub fn join(api: crundi_core::api::Api, base: String, work_dir: String, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    std::thread::spawn(move || {
        let s = status();
        if s["configured"].as_bool().unwrap_or(false) && s["server"].as_str() == Some(base.as_str()) { return done(resume()); }
        let r = api.post("/api/workers", &json!({ "action": "join-self", "name": s["name"], "deviceId": s["deviceId"], "kind": "native", "version": s["version"], "os": s["os"] }));
        let v = match r {
            Ok(v) => v,
            Err(crundi_core::api::ApiError::Status(404, _)) => return done(Err("This server does not support workers yet. Update the server to use them.".into())),
            Err(e) => return done(Err(e.to_string())),
        };
        if v["ok"] == false { return done(Err(v["error"].as_str().unwrap_or("Could not join").to_string())); }
        done(enable(&base, v["key"].as_str().unwrap_or(""), v["id"].as_str().unwrap_or(""), &work_dir).map_err(|e| if e.is_empty() { "Could not start".into() } else { e }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_status_words() {
        assert_eq!(words(&json!({ "available": false })), "This build of the app has no worker program in it yet.");
        assert_eq!(words(&json!({ "available": true, "state": "online" })), "Online");
        assert_eq!(words(&json!({ "available": true, "state": "off", "enabled": true })), "Starting");
        assert_eq!(words(&json!({ "available": true, "state": "restarting", "detail": "stopped (exit 1); starting again" })), "Restarting: stopped (exit 1); starting again");
        assert_eq!(words(&json!({ "available": true, "state": "online", "job": { "command": "make" } })), "Online \u{b7} running: make");
    }
}
