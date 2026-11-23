//! The homepool node (app/homepool-node.js; spec
//! specs/desktop/passwords-homepool-cru.md B): this PC lends its internet
//! connection to the server's homepool as an exit. One websocket to
//! /api/homepool/node?key=hpn_…; the server asks, stream by stream, for
//! outbound TCP connections, and the bytes go through the socket. It dials
//! out only, listens on nothing, and refuses private addresses.
//!
//! Frames: [type u8][stream id u32 BE][payload]; OPEN 1 ({"host","port"}),
//! DATA 2, CLOSE 3, OPENED 4, ERROR 5, PAUSE 6, RESUME 7. Text frames:
//! hello, stats, bye (server); info (node, on open and every 30 s).
//!
//! The Homepool tab's "This PC" card (ops) calls status(), share(), start(),
//! stop() and forget().

use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{Read, Write},
    net::{IpAddr, Shutdown, TcpStream, ToSocketAddrs},
    sync::{atomic::{AtomicBool, AtomicU64, Ordering}, mpsc, Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{client::IntoClientRequest, Message};

pub const OPEN: u8 = 1;
pub const DATA: u8 = 2;
pub const CLOSE: u8 = 3;
pub const OPENED: u8 = 4;
pub const ERROR: u8 = 5;
pub const PAUSE: u8 = 6;
pub const RESUME: u8 = 7;

/// A target the node must not open: this side's own networks
/// (homepool-node.js isPrivateTarget).
pub fn is_private(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']').to_lowercase();
    if h == "localhost" || h.ends_with(".localhost") || h.ends_with(".local") || h.ends_with(".internal") { return true; }
    match h.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => {
            let o = a.octets();
            o[0] == 10 || o[0] == 127 || o[0] == 0 || (o[0] == 169 && o[1] == 254) || (o[0] == 172 && (16..32).contains(&o[1]))
                || (o[0] == 192 && o[1] == 168) || (o[0] == 100 && (64..128).contains(&o[1])) || o[0] >= 224
        }
        Ok(IpAddr::V6(a)) => {
            let s = a.segments();
            a.is_loopback() || a.is_unspecified() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
                || a.to_ipv4_mapped().is_some_and(|v4| is_private(&v4.to_string()))
        }
        Err(_) => false,
    }
}

pub fn frame(t: u8, id: u32, p: &[u8]) -> Vec<u8> { crundi_webview::net::frame(t, id, p) }

#[derive(Default, Clone)]
struct Saved { device_id: String, server: String, key: String, enabled: bool }

#[derive(Default)]
struct Node {
    saved: Saved,
    state: String,
    detail: String,
    connected_at: Option<Instant>,
    node: Value,
    server_view: Value,
    public_ip: String,
    log: VecDeque<String>,
    opened: u64,
    streams: usize,
    stop: Option<Arc<AtomicBool>>,
    kick: Option<mpsc::Sender<()>>,
}

static N: Mutex<Option<Node>> = Mutex::new(None);
static UP: AtomicU64 = AtomicU64::new(0);
static DOWN: AtomicU64 = AtomicU64::new(0);

fn with<R>(f: impl FnOnce(&mut Node) -> R) -> R {
    let mut g = N.lock().unwrap();
    if g.is_none() { *g = Some(Node { saved: load(), state: "off".into(), ..Default::default() }); }
    f(g.as_mut().unwrap())
}

fn file() -> std::path::PathBuf { crundi_core::store::config_dir().join("homepool-node.json") }

fn load() -> Saved {
    let v: Value = std::fs::read(file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    Saved { device_id: crate::host::device_id(), server: s("server"), key: s("key"), enabled: v.get("enabled").and_then(|x| x.as_bool()).unwrap_or(false) }
}

fn save(s: &Saved) {
    let v = json!({ "deviceId": s.device_id, "server": s.server, "key": s.key, "enabled": s.enabled });
    let p = file();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let tmp = p.with_extension("tmp");
    if std::fs::write(&tmp, v.to_string()).is_ok() {
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)); }
        let _ = std::fs::rename(&tmp, &p);
    }
}

fn log(line: &str) {
    let t = chrono::Local::now().format("%H:%M:%S").to_string();
    with(|n| { n.log.push_back(format!("{t} {line}")); while n.log.len() > 60 { n.log.pop_front(); } });
}

fn set_state(s: &str, detail: &str) { with(|n| { n.state = s.to_string(); n.detail = detail.to_string(); }); }

/// What the "This PC" card shows (main.js homepool status()).
pub fn status() -> Value {
    let up = UP.load(Ordering::Relaxed);
    let down = DOWN.load(Ordering::Relaxed);
    with(|n| json!({
        "supported": true, "configured": !n.saved.key.is_empty(), "enabled": n.saved.enabled, "server": n.saved.server,
        "deviceId": n.saved.device_id, "name": gethostname::gethostname().to_string_lossy(),
        "state": if n.saved.enabled { n.state.clone() } else { "off".into() }, "detail": n.detail,
        "connectedAt": n.connected_at.map(|t| crundi_webview::engine::now_ms().saturating_sub(t.elapsed().as_millis() as u64)),
        "node": n.node, "streams": n.streams, "bytesUp": up, "bytesDown": down, "opened": n.opened,
        "serverView": n.server_view, "publicIp": n.public_ip,
        "log": n.log.iter().rev().take(25).rev().cloned().collect::<Vec<_>>(),
    }))
}

/// Share this PC: save the server and key, enabled, and start.
pub fn share(server: &str, key: &str) -> Result<(), String> {
    let s = server.to_lowercase();
    if !(s.starts_with("http://") || s.starts_with("https://")) || !key.starts_with("hpn_") { return Err("Bad node settings.".into()); }
    with(|n| { n.saved.server = server.trim_end_matches('/').to_string(); n.saved.key = key.to_string(); n.saved.enabled = true; save(&n.saved); });
    start()
}

pub fn start() -> Result<(), String> {
    let (server, key) = with(|n| (n.saved.server.clone(), n.saved.key.clone()));
    if key.is_empty() { return Err("Not set up.".into()); }
    with(|n| { n.saved.enabled = true; save(&n.saved); });
    halt();
    let stop = Arc::new(AtomicBool::new(false));
    let (ktx, krx) = mpsc::channel();
    with(|n| { n.stop = Some(stop.clone()); n.kick = Some(ktx); n.state = "connecting".into(); });
    let s2 = stop.clone();
    thread::spawn(move || run(server, key, stop, krx));
    thread::spawn(move || public_ip_loop(s2));
    Ok(())
}

/// Where the public address is asked (main.js hpRefreshIp); CRUNDI_PUBLIC_IP_URL
/// stands in for it under the test driver.
fn ip_url() -> String {
    std::env::var("CRUNDI_PUBLIC_IP_URL").ok().filter(|u| !u.is_empty() && crundi_core::driver::port().is_some()).unwrap_or_else(|| "https://api.ipify.org".into())
}

/// While the node runs: the public address now and every 10 minutes; a new
/// one goes to the server at once (sendInfo). Offline: tried again later.
fn public_ip_loop(stop: Arc<AtomicBool>) {
    let url = ip_url();
    let mut next = Instant::now();
    // (Refused: the node's loop ended for good, and this one with it.)
    while !stop.load(Ordering::Relaxed) && with(|n| n.state != "refused") {
        if Instant::now() >= next {
            next = Instant::now() + Duration::from_secs(600);
            let got = ureq::get(&url).config().timeout_global(Some(Duration::from_secs(10))).build().call().ok()
                .and_then(|mut r| r.body_mut().read_to_string().ok()).map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty() && t.len() <= 60 && t.chars().all(|c| c.is_ascii_hexdigit() || c == '.' || c == ':'));
            if let Some(ip) = got {
                let changed = with(|n| if n.public_ip != ip { n.public_ip = ip.clone(); true } else { false });
                if changed { log(&format!("public IP {ip}")); INFO_DUE.store(true, Ordering::Relaxed); }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// A new public address: the session sends its info now.
static INFO_DUE: AtomicBool = AtomicBool::new(false);

fn halt() {
    let s = with(|n| { n.kick = None; n.stop.take() });
    if let Some(s) = s { s.store(true, Ordering::Relaxed); }
}

pub fn stop() {
    halt();
    with(|n| { n.saved.enabled = false; save(&n.saved); n.state = "stopped".into(); n.connected_at = None; });
    log("stopped");
}

/// Forget the node (keeps only this PC's id).
pub fn forget() {
    halt();
    with(|n| { n.saved = Saved { device_id: n.saved.device_id.clone(), ..Default::default() }; save(&n.saved); n.state = "off".into(); n.node = Value::Null; });
}

/// Reconnect now (a lost socket waits for its backoff otherwise).
pub fn kick() { with(|n| if let Some(k) = &n.kick { let _ = k.send(()); }); }

/// At launch: run when enabled.
pub fn init() {
    if with(|n| n.saved.enabled && !n.saved.key.is_empty()) { let _ = start(); }
}

fn ws_url(server: &str, key: &str) -> String {
    let base = server.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
    format!("{}/api/homepool/node?key={}", base.trim_end_matches('/'), key)
}

/// The node's socket, again and again (1 s doubling to 60 s), until stopped
/// or refused.
fn run(server: String, key: String, stop: Arc<AtomicBool>, kick: mpsc::Receiver<()>) {
    let mut wait = 1u64;
    while !stop.load(Ordering::Relaxed) {
        set_state("connecting", "");
        let mut req = match ws_url(&server, &key).into_client_request() { Ok(r) => r, Err(e) => { log(&format!("bad server address: {e}")); return; } };
        if let Ok(h) = format!("Bearer {key}").parse() { req.headers_mut().insert("Authorization", h); }
        match tungstenite::connect(req) {
            Ok((ws, _)) => {
                wait = 1;
                log("connected");
                with(|n| { n.state = "connected".into(); n.connected_at = Some(Instant::now()); });
                let refused = session(ws, &stop);
                with(|n| { n.connected_at = None; n.streams = 0; });
                if refused { return; }
                log("disconnected");
            }
            Err(tungstenite::Error::Http(r)) if matches!(r.status().as_u16(), 401 | 403) => {
                log("the server refused this node key");
                with(|n| { n.state = "refused".into(); n.saved.enabled = false; save(&n.saved); });
                return;
            }
            Err(e) => log(&format!("could not connect: {e}")),
        }
        if stop.load(Ordering::Relaxed) { break; }
        set_state("connecting", &format!("retrying in {wait}s"));
        let _ = kick.recv_timeout(Duration::from_secs(wait));
        wait = (wait * 2).min(60);
    }
    set_state("stopped", "");
}

enum Out { Bin(Vec<u8>), Text(String) }

/// One connected socket: true when the server said a final bye.
fn session(mut ws: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>, stop: &Arc<AtomicBool>) -> bool {
    match ws.get_mut() {
        tungstenite::stream::MaybeTlsStream::Plain(s) => { let _ = s.set_read_timeout(Some(Duration::from_millis(20))); }
        tungstenite::stream::MaybeTlsStream::Rustls(s) => { let _ = s.get_mut().set_read_timeout(Some(Duration::from_millis(20))); }
        _ => {}
    }
    let (tx, rx) = mpsc::channel::<Out>();
    // OPENED must go before the stream's first DATA: the pump starts after it is queued.
    let mut streams: HashMap<u32, TcpStream> = HashMap::new();
    let paused = Paused::default();
    let _ = tx.send(Out::Text(info()));
    let mut last_info = Instant::now();
    let (dial_tx, dial_rx) = mpsc::channel::<(u32, Result<TcpStream, String>)>();
    loop {
        if stop.load(Ordering::Relaxed) { let _ = ws.close(None); return false; }
        while let Ok((id, r)) = dial_rx.try_recv() {
            match r {
                Ok(s) => {
                    with(|n| { n.opened += 1; n.streams += 1; });
                    if let Ok(rd) = s.try_clone() { pump(id, rd, tx.clone(), paused.clone()); }
                    streams.insert(id, s);
                    let _ = tx.send(Out::Bin(frame(OPENED, id, &[])));
                }
                Err(e) => { let _ = tx.send(Out::Bin(frame(ERROR, id, e.as_bytes()))); }
            }
        }
        while let Ok(o) = rx.try_recv() {
            let r = match o { Out::Bin(b) => { UP.fetch_add(b.len().saturating_sub(5) as u64, Ordering::Relaxed); ws.send(Message::binary(b)) } Out::Text(t) => ws.send(Message::text(t)) };
            if r.is_err() { return false; }
        }
        if last_info.elapsed() > Duration::from_secs(30) || INFO_DUE.swap(false, Ordering::Relaxed) { last_info = Instant::now(); let _ = tx.send(Out::Text(info())); }
        match ws.read() {
            Ok(Message::Binary(b)) => {
                let Some((t, id, p)) = crundi_webview::net::parse(&b) else { continue };
                match t {
                    OPEN => {
                        let v: Value = serde_json::from_slice(p).unwrap_or(Value::Null);
                        let host = v.get("host").and_then(|h| h.as_str()).unwrap_or("").to_string();
                        let port = v.get("port").and_then(|h| h.as_u64()).unwrap_or(0) as u16;
                        if host.is_empty() || port == 0 { let _ = tx.send(Out::Bin(frame(ERROR, id, b"bad target"))); continue; }
                        // Dial off the socket's thread; the loop takes the answer.
                        let dtx = dial_tx.clone();
                        thread::spawn(move || { let _ = dtx.send((id, dial(&host, port))); });
                    }
                    DATA => {
                        DOWN.fetch_add(p.len() as u64, Ordering::Relaxed);
                        if let Some(s) = streams.get_mut(&id) { if s.write_all(p).is_err() { let _ = tx.send(Out::Bin(frame(CLOSE, id, &[]))); streams.remove(&id); } }
                    }
                    CLOSE => { paused.set(id, false); if let Some(s) = streams.remove(&id) { let _ = s.shutdown(Shutdown::Both); with(|n| n.streams = n.streams.saturating_sub(1)); } }
                    PAUSE => paused.set(id, true),
                    RESUME => paused.set(id, false),
                    _ => {}
                }
            }
            Ok(Message::Text(t)) => {
                let v: Value = serde_json::from_str(t.as_str()).unwrap_or(Value::Null);
                match v.get("type").and_then(|t| t.as_str()) {
                    Some("hello") => { let node = v.get("node").cloned().unwrap_or(Value::Null); log(&format!("node {} in {}", node["name"].as_str().unwrap_or("?"), node["networkName"].as_str().or(node["network"].as_str()).unwrap_or("?"))); with(|n| n.node = node); }
                    Some("stats") => with(|n| n.server_view = v.clone()),
                    Some("bye") => {
                        log(&format!("the server said bye: {}", v["reason"].as_str().unwrap_or("")));
                        if v.get("final").and_then(|f| f.as_bool()).unwrap_or(false) {
                            with(|n| { n.state = "refused".into(); n.saved.enabled = false; save(&n.saved); });
                            paused.clear();
                            for (_, s) in streams.drain() { let _ = s.shutdown(Shutdown::Both); }
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
    }
    paused.clear();
    for (_, s) in streams.drain() { let _ = s.shutdown(Shutdown::Both); }
    false
}

/// The streams the server paused: a stream's reader waits on it (woken by
/// RESUME, CLOSE or the socket's end; never a sleep loop).
#[derive(Clone, Default)]
struct Paused(Arc<(Mutex<HashSet<u32>>, Condvar)>);

impl Paused {
    fn set(&self, id: u32, on: bool) {
        let mut g = self.0.0.lock().unwrap();
        if on { g.insert(id); } else if g.remove(&id) { self.0.1.notify_all(); }
    }
    fn clear(&self) { self.0.0.lock().unwrap().clear(); self.0.1.notify_all(); }
    fn wait(&self, id: u32) {
        let mut g = self.0.0.lock().unwrap();
        while g.contains(&id) { g = self.0.1.wait(g).unwrap(); }
    }
}

/// Resolve here, refuse private addresses, connect.
fn dial(host: &str, port: u16) -> Result<TcpStream, String> {
    if is_private(host) { return Err("private address refused by the node".into()); }
    let addrs: Vec<_> = (host, port).to_socket_addrs().map_err(|_| "ENOTFOUND".to_string())?.collect();
    if addrs.iter().any(|a| is_private(&a.ip().to_string())) { return Err("private address refused by the node".into()); }
    let mut last = "ECONNREFUSED".to_string();
    for a in addrs {
        match TcpStream::connect_timeout(&a, Duration::from_secs(15)) { Ok(s) => return Ok(s), Err(e) => last = format!("{:?}", e.kind()) }
    }
    Err(last)
}

/// The target → the server, held while the server says PAUSE.
fn pump(id: u32, mut rd: TcpStream, tx: mpsc::Sender<Out>, paused: Paused) {
    thread::spawn(move || {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            paused.wait(id);
            match rd.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => { if tx.send(Out::Bin(frame(DATA, id, &buf[..n]))).is_err() { return; } }
            }
        }
        let _ = tx.send(Out::Bin(frame(CLOSE, id, &[])));
        with(|n| n.streams = n.streams.saturating_sub(1));
    });
}

fn info() -> String {
    let platform = if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "macos") { "darwin" } else { "linux" };
    let ip = with(|n| n.public_ip.clone());
    json!({ "type": "info", "platform": platform, "app": "crundi-desktop", "version": crundi_shell::lifecycle::version(), "network": "PC", "publicIp": ip, "paused": false, "pauseReason": "", "bytesToday": UP.load(Ordering::Relaxed) + DOWN.load(Ordering::Relaxed), "mobileBytesToday": 0 }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_targets() {
        for h in ["localhost", "a.localhost", "printer.local", "x.internal", "10.1.2.3", "127.0.0.1", "0.0.0.0", "169.254.1.1", "172.16.0.1", "172.31.9.9", "192.168.1.1", "100.64.0.1", "224.0.0.1", "::1", "::", "fd00::1", "fe80::1", "[::1]", "::ffff:192.168.1.1"] {
            assert!(is_private(h), "{h}");
        }
        for h in ["example.com", "8.8.8.8", "172.32.0.1", "100.128.0.1", "2001:4860::8888"] { assert!(!is_private(h), "{h}"); }
        assert_eq!(ws_url("https://a.b/", "hpn_x"), "wss://a.b/api/homepool/node?key=hpn_x");
        assert_eq!(ws_url("http://localhost:4000", "hpn_y"), "ws://localhost:4000/api/homepool/node?key=hpn_y");
    }

    #[test]
    fn a_paused_stream_waits_until_resumed() {
        let p = Paused::default();
        p.set(7, true);
        let q = p.clone();
        let t = thread::spawn(move || { q.wait(7); true });
        thread::sleep(Duration::from_millis(30));
        assert!(!t.is_finished());
        p.set(7, false);
        assert!(t.join().unwrap());
        p.wait(8);
    }

    #[test]
    fn frames() {
        let f = frame(OPEN, 0x01020304, b"{}");
        assert_eq!(&f[..5], &[OPEN, 1, 2, 3, 4]);
        assert_eq!(crundi_webview::net::parse(&f).map(|(t, id, p)| (t, id, p.to_vec())), Some((OPEN, 0x01020304, b"{}".to_vec())));
    }
}
