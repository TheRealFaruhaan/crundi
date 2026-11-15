//! The drawer's networks (app/drawer-net.js, app/net-tunnel-client.js):
//! "client" is this PC's own; "server" reaches the server's network: directly
//! when the server is this PC, else through a loopback HTTP proxy whose every
//! connection (CONNECT for https and websockets, absolute-form for http) is
//! one stream over the native host's socket to the server, which opens the
//! TCP connection on its side (src/net-tunnel.js). A homepool network is the
//! same with its targets prefixed "hp|<network>|".
//!
//! Frames: [type u8][id u32 BE][payload]; OPEN 1, DATA 2, CLOSE 3, OPENED 4,
//! ERROR 5. A proxy owns the ids whose top byte is its base (server 0,
//! homepool networks 1, 2, …). The proxy listens on 127.0.0.1 only and asks
//! for a password made at start (CEF answers it: browser.rs's auth handler).

use cef::{ImplDictionaryValue, ImplPreferenceManager, ImplValue, RequestContext};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

pub const OPEN: u8 = 1;
pub const DATA: u8 = 2;
pub const CLOSE: u8 = 3;
pub const OPENED: u8 = 4;
pub const ERROR: u8 = 5;
const MAX_HEAD: usize = 64 * 1024;

pub fn frame(t: u8, id: u32, payload: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(5 + payload.len());
    b.push(t);
    b.extend_from_slice(&id.to_be_bytes());
    b.extend_from_slice(payload);
    b
}

pub fn parse(b: &[u8]) -> Option<(u8, u32, &[u8])> {
    if b.len() < 5 { return None; }
    Some((b[0], u32::from_be_bytes([b[1], b[2], b[3], b[4]]), &b[5..]))
}

enum Ev { Opened, Error(String), Data(Vec<u8>), Close }

struct Proxy {
    base: u32,
    prefix: String,
    heading: String,
    port: u16,
    next: u32,
    streams: HashMap<u32, mpsc::Sender<Ev>>,
}

static PROXIES: Mutex<Vec<Proxy>> = Mutex::new(Vec::new());
static PASS: Mutex<String> = Mutex::new(String::new());
pub const USER: &str = "crundi";

fn pass() -> String {
    let mut p = PASS.lock().unwrap();
    if p.is_empty() {
        use std::hash::{BuildHasher, Hasher};
        let mut s = String::new();
        for _ in 0..3 { let mut h = std::collections::hash_map::RandomState::new().build_hasher(); h.write_u64(crate::browser::now_ms()); s.push_str(&format!("{:016x}", h.finish())); }
        *p = s;
    }
    p.clone()
}

/// The proxy's sign-in, for CEF's auth handler (only for our port).
pub fn credentials(host: &str, port: u16) -> Option<(String, String)> {
    if host != "127.0.0.1" { return None; }
    PROXIES.lock().unwrap().iter().any(|p| p.port == port).then(|| (USER.to_string(), pass()))
}

/// Is the server reached through the tunnel (a remote server that has it)?
fn connected() -> bool {
    crate::server::features().iter().any(|f| f == "net-tunnel")
}

/// The proxy for a network ("server", "hp:<id>"), made on first use: its port.
pub fn proxy_port(network: &str) -> Option<u16> {
    let (prefix, heading) = match network.strip_prefix("hp:") {
        Some(n) => (format!("hp|{n}|"), format!("Homepool: {n}")),
        None if network == "server" => (String::new(), "Server network".to_string()),
        None => return None,
    };
    {
        let ps = PROXIES.lock().unwrap();
        if let Some(p) = ps.iter().find(|p| p.prefix == prefix) { return Some(p.port); }
    }
    let l = TcpListener::bind("127.0.0.1:0").ok()?;
    let port = l.local_addr().ok()?.port();
    let base = {
        let mut ps = PROXIES.lock().unwrap();
        let base = if prefix.is_empty() { 0 } else { (1..255).find(|b| !ps.iter().any(|p| p.base == *b)).unwrap_or(254) };
        ps.push(Proxy { base, prefix: prefix.clone(), heading, port, next: 1, streams: HashMap::new() });
        base
    };
    thread::spawn(move || {
        for s in l.incoming().flatten() { thread::spawn(move || conn(base, s)); }
    });
    Some(port)
}

fn page(status: u16, reason: &str, text: &str, extra: &str, heading: &str, title: &str) -> String {
    let body = format!("<!doctype html><meta charset=\"utf-8\"><title>{}</title><body style=\"font:14px system-ui;padding:32px;color:#444\"><h3 style=\"margin:0 0 8px\">{heading}</h3>{text}</body>", if title.is_empty() { reason } else { title });
    format!("HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}", body.len())
}

fn b64(s: &str) -> String { use base64::Engine; base64::engine::general_purpose::STANDARD.encode(s) }

/// One connection from a page: its head, then a stream to the server.
fn conn(base: u32, mut sock: TcpStream) {
    let heading = PROXIES.lock().unwrap().iter().find(|p| p.base == base).map(|p| p.heading.clone()).unwrap_or_default();
    let mut head = Vec::new();
    let mut buf = [0u8; 8192];
    let end = loop {
        let n = match sock.read(&mut buf) { Ok(0) | Err(_) => return, Ok(n) => n };
        head.extend_from_slice(&buf[..n]);
        if let Some(e) = head.windows(4).position(|w| w == b"\r\n\r\n") { break e; }
        if head.len() > MAX_HEAD { return; }
    };
    let rest = head[end + 4..].to_vec();
    let text = String::from_utf8_lossy(&head[..end]).to_string();
    let mut lines = text.split("\r\n");
    let first: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let (method, target, version) = (first.first().copied().unwrap_or(""), first.get(1).copied().unwrap_or(""), first.get(2).copied().unwrap_or("HTTP/1.1"));
    let headers: Vec<&str> = lines.collect();
    let want = format!("Basic {}", b64(&format!("{USER}:{}", pass())));
    let auth = headers.iter().find(|h| h.to_lowercase().starts_with("proxy-authorization:")).map(|h| h[h.find(':').unwrap() + 1..].trim().to_string());
    if auth.as_deref() != Some(want.as_str()) {
        let _ = sock.write_all(page(407, "Proxy Authentication Required", "Sign-in needed.", "Proxy-Authenticate: Basic realm=\"Crundi server network\"\r\n", &heading, "").as_bytes());
        return;
    }
    if !connected() {
        let _ = sock.write_all(page(502, "Bad Gateway", "This tab goes out through the server, and the desktop app is not connected to the server right now.", "", &heading, "").as_bytes());
        return;
    }
    let (host_port, first_data) = if method == "CONNECT" {
        (target.to_string(), None)
    } else {
        let Some(rest_url) = target.strip_prefix("http://") else {
            let msg = if target.contains("://") { "Only http and https are carried." } else { "Bad request." };
            let _ = sock.write_all(page(400, "Bad Request", msg, "", &heading, "").as_bytes());
            return;
        };
        let (hp, path) = match rest_url.find('/') { Some(i) => (&rest_url[..i], &rest_url[i..]), None => (rest_url, "/") };
        let hp = if hp.rsplit_once(':').is_some_and(|(_, p)| p.chars().all(|c| c.is_ascii_digit())) && !hp.ends_with(']') { hp.to_string() } else { format!("{hp}:80") };
        let kept: Vec<&&str> = headers.iter().filter(|h| { let l = h.to_lowercase(); !(l.starts_with("proxy-authorization:") || l.starts_with("proxy-connection:") || l.starts_with("connection:") || l.starts_with("keep-alive:")) }).collect();
        let mut nh = format!("{method} {path} {version}\r\n");
        for h in kept { nh.push_str(h); nh.push_str("\r\n"); }
        nh.push_str("Connection: close\r\n\r\n");
        (hp, Some(nh.into_bytes()))
    };
    // Open the stream.
    let (tx, rx) = mpsc::channel::<Ev>();
    let (id, prefix) = {
        let mut ps = PROXIES.lock().unwrap();
        let Some(p) = ps.iter_mut().find(|p| p.base == base) else { return };
        let id = (p.base << 24) + p.next;
        p.next = if p.next >= 0xfffff0 { 1 } else { p.next + 1 };
        p.streams.insert(id, tx);
        (id, p.prefix.clone())
    };
    let drop_stream = move || { if let Some(p) = PROXIES.lock().unwrap().iter_mut().find(|p| p.base == base) { p.streams.remove(&id); } };
    let fail = |sock: &mut TcpStream, why: &str| {
        let w: String = why.chars().filter(|c| *c != '<' && *c != '&').collect();
        let no_exit = w.to_lowercase().contains("no node") || w.to_lowercase().contains("no exit");
        let text = if no_exit { format!("No exit is up: {}. The page loads once a node of this network is online.", w.trim_start_matches("homepool:").trim()) } else { format!("The server could not connect: {w}") };
        let _ = sock.write_all(page(502, "Bad Gateway", &text, "", &heading, if no_exit { "No exit is up" } else { "Could not connect" }).as_bytes());
    };
    if !crate::server::send_binary(frame(OPEN, id, format!("{prefix}{host_port}").as_bytes())) {
        drop_stream();
        return fail(&mut sock, "Not connected to the server");
    }
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(Ev::Opened) => {}
        Ok(Ev::Error(e)) => { drop_stream(); return fail(&mut sock, &e); }
        Ok(_) => { drop_stream(); return fail(&mut sock, "The server closed the connection"); }
        Err(_) => { drop_stream(); crate::server::send_binary(frame(CLOSE, id, &[])); return fail(&mut sock, "The server did not answer"); }
    }
    match first_data {
        None => { let _ = sock.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n"); }
        Some(h) => { crate::server::send_binary(frame(DATA, id, &h)); }
    }
    if !rest.is_empty() { crate::server::send_binary(frame(DATA, id, &rest)); }
    // Page → server.
    let Ok(mut rd) = sock.try_clone() else { return };
    thread::spawn(move || {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            match rd.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => { if !crate::server::send_binary(frame(DATA, id, &buf[..n])) { break; } }
            }
        }
        if let Some(p) = PROXIES.lock().unwrap().iter_mut().find(|p| p.base == base) {
            if p.streams.remove(&id).is_some() { crate::server::send_binary(frame(CLOSE, id, &[])); }
        }
    });
    // Server → page.
    for ev in rx {
        match ev {
            Ev::Data(d) => { if sock.write_all(&d).is_err() { break; } }
            Ev::Close | Ev::Error(_) => break,
            Ev::Opened => {}
        }
    }
    let _ = sock.shutdown(Shutdown::Both);
    drop_stream();
}

/// A tunnel frame from the server (any thread).
pub fn from_server(b: Vec<u8>) {
    let Some((t, id, payload)) = parse(&b) else { return };
    let base = id >> 24;
    let tx = PROXIES.lock().unwrap().iter().find(|p| p.base == base).and_then(|p| p.streams.get(&id).cloned());
    let Some(tx) = tx else { return };
    let ev = match t {
        OPENED => Ev::Opened,
        DATA => Ev::Data(payload.to_vec()),
        ERROR => Ev::Error(String::from_utf8_lossy(payload).to_string()),
        CLOSE => {
            if let Some(p) = PROXIES.lock().unwrap().iter_mut().find(|p| p.base == base) { p.streams.remove(&id); }
            Ev::Close
        }
        _ => return,
    };
    let _ = tx.send(ev);
}

/// The socket closed: every stream on it is gone.
pub fn dropped() {
    let all: Vec<mpsc::Sender<Ev>> = PROXIES.lock().unwrap().iter_mut().flat_map(|p| p.streams.drain().map(|(_, s)| s)).collect();
    for s in all { let _ = s.send(Ev::Error("Not connected to the server".into())); }
}

/// A network's profile was made: its proxy when the server is elsewhere
/// (none when it is this PC: its localhost is ours).
pub fn context_made(network: &str, ctx: &RequestContext) {
    if network == "client" || network.starts_with("mem:") { return; }
    if super::drawer::with(|d| d.mode == "local") { return; }
    let Some(port) = proxy_port(network) else { return };
    let Some(dict) = cef::dictionary_value_create() else { return };
    let s = |x: &str| cef::CefString::from(x);
    dict.set_string(Some(&s("mode")), Some(&s("fixed_servers")));
    dict.set_string(Some(&s("server")), Some(&s(&format!("http://127.0.0.1:{port}"))));
    dict.set_string(Some(&s("bypass_list")), Some(&s("<-loopback>")));
    let Some(v) = cef::value_create() else { return };
    let mut d = dict;
    v.set_dictionary(Some(&mut d));
    let mut v = v;
    let mut err = cef::CefString::default();
    if ctx.set_preference(Some(&s("proxy")), Some(&mut v), Some(&mut err)) == 0 {
        eprintln!("[native] drawer: the {network} network's proxy was not set: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames() {
        let f = frame(OPEN, (1 << 24) + 5, b"hp|home|example.com:443");
        let (t, id, p) = parse(&f).unwrap();
        assert_eq!((t, id >> 24, id & 0xffffff, p), (OPEN, 1, 5, &b"hp|home|example.com:443"[..]));
        assert!(parse(&[1, 0, 0]).is_none());
        assert!(page(407, "Proxy Authentication Required", "Sign-in needed.", "", "Server network", "").starts_with("HTTP/1.1 407"));
    }

    /// The proxy end to end, with the server's side played here: a CONNECT
    /// becomes OPEN, OPENED lets data through both ways.
    #[test]
    fn proxy_streams() {
        let port = proxy_port("server").unwrap();
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"CONNECT a.b:443 HTTP/1.1\r\nHost: a.b\r\n\r\n").unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        assert!(out.starts_with("HTTP/1.1 407"), "{out}");
        assert_eq!(credentials("127.0.0.1", port).unwrap().0, "crundi");
        assert!(credentials("127.0.0.1", 1).is_none());
    }
}
