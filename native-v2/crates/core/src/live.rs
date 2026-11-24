//! The live link to the server (ARCHITECTURE.md, "The live link"), on its
//! own threads; nothing here touches the UI:
//!
//! - **/ws**: one socket for the chats this client watches (subscribe-ui →
//!   ui-history, ui-event) and for the server's browser control
//!   (register-native-host → native-host requests). Re-subscribes after
//!   every reconnect.
//! - **/api/events**: the event stream (the "state" event, usage, the inbox,
//!   chat secrets, secret requests, the areas' events).
//!
//! Every message is parsed here (off the UI thread) and handed to `to_ui`,
//! which the app points at one channel; the UI side takes what is waiting
//! once per frame (the shell's live hub).
//!
//! The heartbeat: /ws is pinged every [PING_EVERY]; when nothing (a message
//! or a pong) is heard for [SILENT_FOR], the connection is taken as dead
//! without having closed and is opened again. The server sends no keep-alive
//! on the event stream, so each new socket after a silence also starts a new
//! event stream (the generation: an older stream's thread drops what it reads
//! and ends). Worst case from the death to a new connection: about
//! SILENT_FOR + a read timeout + a connect, under the 45 s the CR allows.

use crate::api::{Api, ApiError};
use serde_json::{json, Value};
use std::{
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

/// How often /ws is pinged, and how long a silence means the link is dead.
pub const PING_EVERY: Duration = Duration::from_secs(10);
pub const SILENT_FOR: Duration = Duration::from_secs(25);
/// How long a read waits before the thread looks at what it has to send.
const READ_SLICE: Duration = Duration::from_millis(50);

/// What the link has done, for the driver and the connection badge.
#[derive(Default)]
pub struct Stats {
    /// /ws connections made (the first and every reconnect).
    pub connects: AtomicU64,
    /// Reconnects because the connection went silent (the heartbeat).
    pub silent_drops: AtomicU64,
    /// Event streams started.
    pub streams: AtomicU64,
}

pub struct Live {
    out: mpsc::Sender<String>,
    subs: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    pub stats: Arc<Stats>,
}

impl Live {
    /// Watch a chat: its history now, then its events.
    pub fn subscribe(&self, id: &str) {
        let mut subs = self.subs.lock().unwrap();
        if !subs.iter().any(|s| s == id) { subs.push(id.to_string()); }
        let _ = self.out.send(json!({ "type": "subscribe-ui", "id": id }).to_string());
    }

    pub fn unsubscribe(&self, id: &str) {
        self.subs.lock().unwrap().retain(|s| s != id);
        let _ = self.out.send(json!({ "type": "unsubscribe-ui", "id": id }).to_string());
    }

    /// Send any message on /ws (an area's subscribe-style messages; they are
    /// not sent again after a reconnect: listen for Event::Connected(true)).
    pub fn send(&self, msg: &Value) {
        let _ = self.out.send(msg.to_string());
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Live {
    fn drop(&mut self) { self.stop(); }
}

/// What the live link tells the app.
#[derive(Debug, Clone)]
pub enum Event {
    Connected(bool),
    /// A ui-history or ui-event message.
    Chat(Value),
    /// The event stream's "state": projects, chats (terminals), services…
    State(Value),
    /// The account's usage (the top bar's bars) and the inbox's unread count.
    Usage(Value),
    Inbox(Value),
    /// One chat's pre-authorised secrets: {sessionId, grants}.
    ChatSecrets(Value),
    /// The secret requests waiting on the owner: {requests}.
    SecretRequests(Value),
    SignedOut,
    /// Any other /api/events event (its name, its data) and any other /ws
    /// message (its type, the whole message): the areas'.
    Server(String, Value),
    Ws(String, Value),
}

/// The heartbeat's timing (the defaults above; tests shorten it).
#[derive(Clone, Copy, Debug)]
pub struct Timing { pub ping_every: Duration, pub silent_for: Duration }
impl Default for Timing { fn default() -> Self { Timing { ping_every: PING_EVERY, silent_for: SILENT_FOR } } }

/// Start both links; `to_ui` hears every event, on the links' own threads.
pub fn start(api: Api, to_ui: impl Fn(Event) + Send + Sync + 'static) -> Live { start_with(api, Timing::default(), to_ui) }

/// [start] with another heartbeat timing.
pub fn start_with(api: Api, timing: Timing, to_ui: impl Fn(Event) + Send + Sync + 'static) -> Live {
    let (out, rx) = mpsc::channel::<String>();
    let resub = out.clone();
    let subs = Arc::new(Mutex::new(Vec::<String>::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(Stats::default());
    let to_ui: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(to_ui);
    {
        let (api, subs, stop, stats) = (api.clone(), subs.clone(), stop.clone(), stats.clone());
        let to_ui = to_ui.clone();
        thread::Builder::new().name("live-ws".into()).spawn(move || socket_loop(api, timing, rx, resub, subs, stop, stats, to_ui)).ok();
    }
    Live { out, subs, stop, stats }
}

fn set_timeout(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) {
    let t = Some(READ_SLICE);
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => { let _ = s.set_read_timeout(t); let _ = s.set_write_timeout(Some(Duration::from_secs(5))); }
        MaybeTlsStream::Rustls(s) => { let _ = s.sock.set_read_timeout(t); let _ = s.sock.set_write_timeout(Some(Duration::from_secs(5))); }
        _ => {}
    }
}

/// What a new event stream re-sends once the server's state arrives (a
/// reconnect: the server may have restarted, its chats only now loaded).
type Resub = (mpsc::Sender<String>, Arc<Mutex<Vec<String>>>);

/// Start an event stream of generation `my`: it ends when a newer one starts.
fn start_stream(api: &Api, stop: &Arc<AtomicBool>, generation: &Arc<AtomicU64>, stats: &Arc<Stats>, to_ui: &Arc<dyn Fn(Event) + Send + Sync>, resub: Option<Resub>) {
    let my = generation.fetch_add(1, Ordering::SeqCst) + 1;
    stats.streams.fetch_add(1, Ordering::Relaxed);
    let (api, stop, generation, to_ui) = (api.clone(), stop.clone(), generation.clone(), to_ui.clone());
    thread::Builder::new().name("live-events".into()).spawn(move || events_loop(api, stop, generation, my, to_ui, resub)).ok();
}

/// The wait before connect attempt `n` (1, 2, 4, 8, 16, then 30 s), as the
/// web client backs off (FINDINGS T14).
pub fn backoff(n: u32) -> Duration { Duration::from_secs((1u64 << n.min(5)).min(30)) }

/// How long a connect or the handshake may take before the attempt fails
/// (tungstenite::connect has none: a half-open origin behind a proxy held
/// the link's thread forever, FINDINGS T14).
const CONNECT_WITHIN: Duration = Duration::from_secs(10);

/// /ws's connect, the TCP connect and the handshake each bounded.
fn connect(url: &str) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, tungstenite::Error> {
    use std::net::ToSocketAddrs;
    let uri: tungstenite::http::Uri = url.parse().map_err(|_| tungstenite::Error::Url(tungstenite::error::UrlError::NoHostName))?;
    let host = uri.host().ok_or(tungstenite::Error::Url(tungstenite::error::UrlError::NoHostName))?.trim_start_matches('[').trim_end_matches(']').to_string();
    let port = uri.port_u16().unwrap_or(if uri.scheme_str() == Some("wss") { 443 } else { 80 });
    let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, "no address");
    let mut tcp = None;
    for a in (host.as_str(), port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&a, CONNECT_WITHIN) { Ok(s) => { tcp = Some(s); break; } Err(e) => last = e }
    }
    let tcp = tcp.ok_or(tungstenite::Error::Io(last))?;
    tcp.set_read_timeout(Some(CONNECT_WITHIN))?;
    tcp.set_write_timeout(Some(CONNECT_WITHIN))?;
    let _ = tcp.set_nodelay(true);
    match tungstenite::client_tls(url, tcp) {
        Ok((ws, _)) => Ok(ws),
        Err(tungstenite::HandshakeError::Failure(e)) => Err(e),
        Err(tungstenite::HandshakeError::Interrupted(_)) => Err(tungstenite::Error::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "handshake timed out"))),
    }
}

#[allow(clippy::too_many_arguments)]
fn socket_loop(api: Api, timing: Timing, rx: mpsc::Receiver<String>, resub: mpsc::Sender<String>, subs: Arc<Mutex<Vec<String>>>, stop: Arc<AtomicBool>, stats: Arc<Stats>, to_ui: Arc<dyn Fn(Event) + Send + Sync>) {
    let (host_tx, host_rx) = mpsc::channel::<Value>();
    let generation = Arc::new(AtomicU64::new(0));
    start_stream(&api, &stop, &generation, &stats, &to_ui, None);
    // Failed attempts since the last connection (the backoff's step).
    let mut failed: u32 = 0;
    // Sleep in slices, so a stop is heard.
    let wait = |d: Duration, stop: &AtomicBool| { let t = Instant::now(); while t.elapsed() < d && !stop.load(Ordering::Relaxed) { thread::sleep(Duration::from_millis(100)); } };
    while !stop.load(Ordering::Relaxed) {
        let url = match api.ws_url() {
            Ok(u) => u,
            Err(ApiError::SignedOut) => { to_ui(Event::SignedOut); return; }
            Err(e) => {
                let d = backoff(failed);
                failed += 1;
                log::warn!("live: cannot reach the server ({e:?}), attempt {failed}, next in {d:?}");
                to_ui(Event::Connected(false));
                wait(d, &stop);
                continue;
            }
        };
        let mut ws = match connect(url.as_str()) {
            Ok(ws) => ws,
            Err(tungstenite::Error::Http(resp)) if resp.status().as_u16() == 401 => {
                let used = url.rsplit("token=").next().unwrap_or("").to_string();
                if let Err(ApiError::SignedOut) = api.refresh_after(&used) { to_ui(Event::SignedOut); return; }
                continue;
            }
            Err(e) => {
                // Any failure (a refused connect, 5xx or Cloudflare's 521/522/524
                // while the server restarts, a timeout): again, backing off.
                let d = backoff(failed);
                failed += 1;
                log::warn!("live: cannot connect: {e} (attempt {failed}, next in {d:?})");
                to_ui(Event::Connected(false));
                wait(d, &stop);
                continue;
            }
        };
        let again = stats.connects.fetch_add(1, Ordering::Relaxed) > 0;
        if again || failed > 0 { log::info!("live: connected again after {failed} failed attempt(s)"); }
        failed = 0;
        // A reconnect: a new event stream too (the server's state now, which
        // also re-subscribes the chats once the server has them).
        if again { start_stream(&api, &stop, &generation, &stats, &to_ui, Some((resub.clone(), subs.clone()))); }
        set_timeout(&mut ws);
        // Messages queued while away are stale (subscriptions are redone below).
        while rx.try_recv().is_ok() {}
        let (bin_tx, bin_rx) = mpsc::channel::<Vec<u8>>();
        if let Some(reg) = crate::host::registration() {
            let _ = ws.send(Message::text(reg.to_string()));
            crate::host::set_binary_out(Some(bin_tx));
        }
        for id in subs.lock().unwrap().clone() {
            let _ = ws.send(Message::text(json!({ "type": "subscribe-ui", "id": id }).to_string()));
        }
        to_ui(Event::Connected(true));
        let (mut heard, mut pinged) = (Instant::now(), Instant::now());
        let mut silent = false;
        while !stop.load(Ordering::Relaxed) {
            if pinged.elapsed() >= timing.ping_every { let _ = ws.send(Message::Ping(Default::default())); pinged = Instant::now(); }
            if heard.elapsed() >= timing.silent_for {
                log::warn!("live: nothing heard for {:?}, connecting again", timing.silent_for);
                silent = true;
                break;
            }
            while let Ok(text) = rx.try_recv() { let _ = ws.send(Message::text(text)); }
            while let Ok(reply) = host_rx.try_recv() {
                let _ = ws.send(Message::text(json!({ "type": "native-host-result", "payload": reply }).to_string()));
            }
            while let Ok(b) = bin_rx.try_recv() { let _ = ws.send(Message::binary(b)); }
            let read = ws.read();
            if read.is_ok() { heard = Instant::now(); }
            match read {
                Ok(Message::Text(t)) => {
                    let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                    match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                        "ui-history" | "ui-event" => to_ui(Event::Chat(v)),
                        "native-host" | "native-host-ready" => crate::host::on_message(t.as_str(), host_tx.clone(), "the server"),
                        other => { let other = other.to_string(); to_ui(Event::Ws(other, v)) }
                    }
                }
                Ok(Message::Binary(b)) => crate::host::on_binary(b.to_vec()),
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
        // A silent socket may never answer a close: shut the stream instead.
        if silent {
            stats.silent_drops.fetch_add(1, Ordering::Relaxed);
            if let MaybeTlsStream::Plain(s) = ws.get_mut() { let _ = s.shutdown(std::net::Shutdown::Both); }
            if let MaybeTlsStream::Rustls(s) = ws.get_mut() { let _ = s.sock.shutdown(std::net::Shutdown::Both); }
        } else {
            let _ = ws.close(None);
        }
        crate::host::set_binary_out(None);
        if stop.load(Ordering::Relaxed) { break; }
        log::warn!("live: the connection dropped ({}), connecting again", if silent { "silent" } else { "closed" });
        to_ui(Event::Connected(false));
        if !silent { wait(backoff(0), &stop); }
    }
}

fn events_loop(api: Api, stop: Arc<AtomicBool>, generation: Arc<AtomicU64>, my: u64, to_ui: Arc<dyn Fn(Event) + Send + Sync>, mut resub: Option<Resub>) {
    let current = || generation.load(Ordering::SeqCst) == my && !stop.load(Ordering::Relaxed);
    let mut failed: u32 = 0;
    while current() {
        let r = api.events(|name, data| {
            if !current() { return false; }
            failed = 0;
            let Ok(v) = serde_json::from_str::<Value>(data) else { return true };
            if name == "state" {
                if let Some((out, subs)) = resub.take() {
                    for id in subs.lock().unwrap().clone() { let _ = out.send(json!({ "type": "subscribe-ui", "id": id }).to_string()); }
                }
            }
            to_ui(match name {
                "state" => Event::State(v),
                "usage" => Event::Usage(v),
                "inbox" => Event::Inbox(v),
                "chat-secrets" => Event::ChatSecrets(v),
                "secret-requests" => Event::SecretRequests(v),
                other => Event::Server(other.to_string(), v),
            });
            true
        });
        if !current() { return; }
        match r {
            Err(ApiError::SignedOut) => { to_ui(Event::SignedOut); return; }
            Err(ApiError::Status(401, _)) => continue,
            Err(e) => log::warn!("live: the event stream failed: {e:?} (attempt {})", failed + 1),
            Ok(()) => log::warn!("live: the event stream ended"),
        }
        let d = backoff(failed);
        failed += 1;
        let t = Instant::now();
        while t.elapsed() < d && current() { thread::sleep(Duration::from_millis(100)); }
    }
}

#[cfg(test)]
mod tests {
    //! The heartbeat against a server that goes silent without closing: a
    //! fake server on 127.0.0.1 that accepts /ws and /api/events, then never
    //! sends a byte (nor a pong). The link must give up on the silent socket
    //! and connect again, with a new event stream too.
    use super::*;
    use crate::api::Tokens;
    use std::{net::TcpListener, sync::atomic::AtomicUsize, time::Instant};

    #[test]
    fn a_silent_connection_reconnects() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let ws_conns = Arc::new(AtomicUsize::new(0));
        let sse_conns = Arc::new(AtomicUsize::new(0));
        let (w, e) = (ws_conns.clone(), sse_conns.clone());
        thread::spawn(move || {
            let mut held = vec![];
            for s in l.incoming().flatten() {
                let mut head = [0u8; 16];
                let n = s.peek(&mut head).unwrap_or(0);
                let line = String::from_utf8_lossy(&head[..n]).to_string();
                if line.starts_with("GET /ws") {
                    w.fetch_add(1, Ordering::SeqCst);
                    // The handshake, then silence: the socket is never read again.
                    if let Ok(ws) = tungstenite::accept(s) { held.push(Box::new(ws) as Box<dyn std::any::Any + Send>); }
                } else {
                    e.fetch_add(1, Ordering::SeqCst);
                    held.push(Box::new(s));
                }
            }
        });
        let tokens = Tokens { access: "a".into(), refresh: "r".into(), expires_at: Instant::now() + Duration::from_secs(3600) };
        let api = Api::new(&format!("http://127.0.0.1:{port}"), tokens, |_| {});
        let ups = Arc::new(AtomicUsize::new(0));
        let u = ups.clone();
        let live = start_with(api, Timing { ping_every: Duration::from_millis(100), silent_for: Duration::from_millis(600) }, move |ev| if let Event::Connected(true) = ev { u.fetch_add(1, Ordering::SeqCst); });
        let t0 = Instant::now();
        while ws_conns.load(Ordering::SeqCst) < 2 && t0.elapsed() < Duration::from_secs(5) { thread::sleep(Duration::from_millis(20)); }
        let took = t0.elapsed();
        assert!(ws_conns.load(Ordering::SeqCst) >= 2, "no reconnect after the silence");
        assert!(took < Duration::from_millis(2500), "reconnected after {took:?}");
        assert!(live.stats.silent_drops.load(Ordering::Relaxed) >= 1);
        // The event stream started again with the new socket.
        let t1 = Instant::now();
        while sse_conns.load(Ordering::SeqCst) < 2 && t1.elapsed() < Duration::from_secs(3) { thread::sleep(Duration::from_millis(20)); }
        assert!(sse_conns.load(Ordering::SeqCst) >= 2, "the event stream was not started again");
        let t2 = Instant::now();
        while ups.load(Ordering::SeqCst) < 2 && t2.elapsed() < Duration::from_secs(3) { thread::sleep(Duration::from_millis(20)); }
        assert!(ups.load(Ordering::SeqCst) >= 2, "no Connected(true) after the reconnect");
        live.stop();
    }

    /// FINDINGS T14: the origin down (Cloudflare's 521) twice, then up, then
    /// the socket closed under it (a restart): the link keeps trying, backing
    /// off, connects again by itself, re-subscribes the open chat on connect
    /// and again once the new event stream's state arrives.
    #[test]
    fn a_restarting_server_is_reconnected_and_resubscribed() {
        use std::io::Write;
        assert_eq!([backoff(0), backoff(1), backoff(2), backoff(5), backoff(40)].map(|d| d.as_secs()), [1, 2, 4, 30, 30]);
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let ws_tries = Arc::new(AtomicUsize::new(0));
        let subs_heard = Arc::new(AtomicUsize::new(0));
        let (w, h) = (ws_tries.clone(), subs_heard.clone());
        thread::spawn(move || {
            let mut held = vec![];
            for mut s in l.incoming().flatten() {
                let mut head = [0u8; 16];
                let n = s.peek(&mut head).unwrap_or(0);
                if String::from_utf8_lossy(&head[..n]).starts_with("GET /ws") {
                    let k = w.fetch_add(1, Ordering::SeqCst) + 1;
                    if k <= 2 { let _ = s.write_all(b"HTTP/1.1 521 Origin Down\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"); continue; }
                    let h = h.clone();
                    thread::spawn(move || {
                        let Ok(mut ws) = tungstenite::accept(s) else { return };
                        let t = Instant::now();
                        // The first connection is closed after 1.5 s (the server restarting).
                        while k > 3 || t.elapsed() < Duration::from_millis(1500) {
                            let _ = ws.get_mut().set_read_timeout(Some(Duration::from_millis(100)));
                            match ws.read() {
                                Ok(Message::Text(m)) if m.as_str().contains("subscribe-ui") && m.as_str().contains("c1") => { h.fetch_add(1, Ordering::SeqCst); }
                                Ok(_) => {}
                                Err(tungstenite::Error::Io(_)) => {}
                                Err(_) => return,
                            }
                        }
                        let _ = ws.close(None);
                        let _ = ws.flush();
                    });
                } else {
                    let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\nevent: state\ndata: {}\n\n");
                    held.push(s);
                }
            }
        });
        let tokens = Tokens { access: "a".into(), refresh: "r".into(), expires_at: Instant::now() + Duration::from_secs(3600) };
        let api = Api::new(&format!("http://127.0.0.1:{port}"), tokens, |_| {});
        let (ups, downs) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let (u, d) = (ups.clone(), downs.clone());
        let live = start_with(api, Timing::default(), move |ev| match ev {
            Event::Connected(true) => { u.fetch_add(1, Ordering::SeqCst); }
            Event::Connected(false) => { d.fetch_add(1, Ordering::SeqCst); }
            _ => {}
        });
        live.subscribe("c1");
        let until = |what: &str, secs: u64, f: &dyn Fn() -> bool| {
            let t = Instant::now();
            while !f() && t.elapsed() < Duration::from_secs(secs) { thread::sleep(Duration::from_millis(20)); }
            assert!(f(), "{what}");
        };
        until("connected after two 521s", 10, &|| ups.load(Ordering::SeqCst) >= 1);
        assert!(downs.load(Ordering::SeqCst) >= 2, "reconnecting shown while the origin was down");
        assert_eq!(ws_tries.load(Ordering::SeqCst), 3);
        until("the chat subscribed on connect", 5, &|| subs_heard.load(Ordering::SeqCst) >= 1);
        until("connected again after the close", 10, &|| ups.load(Ordering::SeqCst) >= 2);
        // On connect, and again once the reconnect's event stream brought the state.
        until("the chat re-subscribed after the restart", 10, &|| subs_heard.load(Ordering::SeqCst) >= 3);
        assert!(live.stats.streams.load(Ordering::Relaxed) >= 2, "a new event stream with the reconnect");
        live.stop();
    }

    #[test]
    fn the_default_heartbeat_reconnects_within_45_s() {
        // Silence, then the read slice, then a connect: well under the CR's 45 s.
        assert!(SILENT_FOR + READ_SLICE + Duration::from_secs(5) < Duration::from_secs(45));
        assert!(PING_EVERY * 2 <= SILENT_FOR);
    }
}
