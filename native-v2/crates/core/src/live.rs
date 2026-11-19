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
    let subs = Arc::new(Mutex::new(Vec::<String>::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(Stats::default());
    let to_ui: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(to_ui);
    {
        let (api, subs, stop, stats) = (api.clone(), subs.clone(), stop.clone(), stats.clone());
        let to_ui = to_ui.clone();
        thread::Builder::new().name("live-ws".into()).spawn(move || socket_loop(api, timing, rx, subs, stop, stats, to_ui)).ok();
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

/// Start an event stream of generation `my`: it ends when a newer one starts.
fn start_stream(api: &Api, stop: &Arc<AtomicBool>, generation: &Arc<AtomicU64>, stats: &Arc<Stats>, to_ui: &Arc<dyn Fn(Event) + Send + Sync>) {
    let my = generation.fetch_add(1, Ordering::SeqCst) + 1;
    stats.streams.fetch_add(1, Ordering::Relaxed);
    let (api, stop, generation, to_ui) = (api.clone(), stop.clone(), generation.clone(), to_ui.clone());
    thread::Builder::new().name("live-events".into()).spawn(move || events_loop(api, stop, generation, my, to_ui)).ok();
}

fn socket_loop(api: Api, timing: Timing, rx: mpsc::Receiver<String>, subs: Arc<Mutex<Vec<String>>>, stop: Arc<AtomicBool>, stats: Arc<Stats>, to_ui: Arc<dyn Fn(Event) + Send + Sync>) {
    let (host_tx, host_rx) = mpsc::channel::<Value>();
    let generation = Arc::new(AtomicU64::new(0));
    start_stream(&api, &stop, &generation, &stats, &to_ui);
    while !stop.load(Ordering::Relaxed) {
        let url = match api.ws_url() {
            Ok(u) => u,
            Err(ApiError::SignedOut) => { to_ui(Event::SignedOut); return; }
            Err(_) => { thread::sleep(Duration::from_secs(3)); continue; }
        };
        let mut ws = match tungstenite::connect(url.as_str()) {
            Ok((ws, _)) => ws,
            Err(tungstenite::Error::Http(resp)) if resp.status().as_u16() == 401 => {
                let used = url.rsplit("token=").next().unwrap_or("").to_string();
                if let Err(ApiError::SignedOut) = api.refresh_after(&used) { to_ui(Event::SignedOut); return; }
                continue;
            }
            Err(e) => {
                log::warn!("live: cannot connect: {e}");
                to_ui(Event::Connected(false));
                thread::sleep(Duration::from_secs(3));
                continue;
            }
        };
        stats.connects.fetch_add(1, Ordering::Relaxed);
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
        to_ui(Event::Connected(false));
        // The event stream went through the same network: start it again.
        if silent { start_stream(&api, &stop, &generation, &stats, &to_ui); } else { thread::sleep(Duration::from_secs(2)); }
    }
}

fn events_loop(api: Api, stop: Arc<AtomicBool>, generation: Arc<AtomicU64>, my: u64, to_ui: Arc<dyn Fn(Event) + Send + Sync>) {
    let current = || generation.load(Ordering::SeqCst) == my && !stop.load(Ordering::Relaxed);
    while current() {
        let r = api.events(|name, data| {
            if !current() { return false; }
            let Ok(v) = serde_json::from_str::<Value>(data) else { return true };
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
            _ => {}
        }
        thread::sleep(Duration::from_secs(3));
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
        assert!(ups.load(Ordering::SeqCst) >= 2);
        live.stop();
    }

    #[test]
    fn the_default_heartbeat_reconnects_within_45_s() {
        // Silence, then the read slice, then a connect: well under the CR's 45 s.
        assert!(SILENT_FOR + READ_SLICE + Duration::from_secs(5) < Duration::from_secs(45));
        assert!(PING_EVERY * 2 <= SILENT_FOR);
    }
}
