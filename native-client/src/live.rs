//! The live link to the server, on two threads:
//!
//! - **/ws**: one socket for the chats this client has open
//!   (subscribe-ui → ui-history, ui-event) and for the server's browser
//!   control (register-native-host → native-host requests, answered by
//!   src/server.rs). Re-subscribes after every reconnect.
//! - **/api/events**: the event stream, for changes to the chat list
//!   (the "state" event).
//!
//! Both reconnect on their own with a fresh token, until [Live::stop].

use crate::api::{Api, ApiError};
use serde_json::{json, Value};
use std::{
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

pub struct Live {
    out: mpsc::Sender<String>,
    subs: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
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

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// What the live link tells the app (on the UI thread).
pub enum Event {
    Connected(bool),
    /// A ui-history or ui-event message.
    Chat(Value),
    /// The event stream's "state": projects, chats (terminals), services…
    State(Value),
    SignedOut,
}

pub fn start(api: Api, on_event: fn(Event)) -> Live {
    let (out, rx) = mpsc::channel::<String>();
    let subs = Arc::new(Mutex::new(Vec::<String>::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let to_ui = move |e: Event| { let _ = slint::invoke_from_event_loop(move || on_event(e)); };

    {
        let (api, subs, stop) = (api.clone(), subs.clone(), stop.clone());
        thread::spawn(move || socket_loop(api, rx, subs, stop, to_ui));
    }
    {
        let (api, stop) = (api.clone(), stop.clone());
        thread::spawn(move || events_loop(api, stop, to_ui));
    }
    Live { out, subs, stop }
}

fn set_timeout(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) {
    let t = Some(Duration::from_millis(30));
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => { let _ = s.set_read_timeout(t); }
        MaybeTlsStream::Rustls(s) => { let _ = s.sock.set_read_timeout(t); }
        _ => {}
    }
}

fn socket_loop(api: Api, rx: mpsc::Receiver<String>, subs: Arc<Mutex<Vec<String>>>, stop: Arc<AtomicBool>, to_ui: impl Fn(Event)) {
    let (host_tx, host_rx) = mpsc::channel::<Value>();
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
                eprintln!("[native] live: cannot connect: {e}");
                to_ui(Event::Connected(false));
                thread::sleep(Duration::from_secs(3));
                continue;
            }
        };
        set_timeout(&mut ws);
        // Messages queued while away are stale (subscriptions are redone below).
        while rx.try_recv().is_ok() {}
        // The server's browser control: only when asked (CRUNDI_NATIVE_HOST=1),
        // since it takes over from an Electron client on the same server.
        if std::env::var_os("CRUNDI_NATIVE_HOST").is_some() {
            let reg = json!({ "type": "register-native-host", "capabilities": ["browser-mouse"], "deviceId": device_id() });
            let _ = ws.send(Message::text(reg.to_string()));
        }
        for id in subs.lock().unwrap().clone() {
            let _ = ws.send(Message::text(json!({ "type": "subscribe-ui", "id": id }).to_string()));
        }
        to_ui(Event::Connected(true));

        while !stop.load(Ordering::Relaxed) {
            while let Ok(text) = rx.try_recv() { let _ = ws.send(Message::text(text)); }
            while let Ok(reply) = host_rx.try_recv() {
                let _ = ws.send(Message::text(json!({ "type": "native-host-result", "payload": reply }).to_string()));
            }
            match ws.read() {
                Ok(Message::Text(t)) => {
                    let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                    match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                        "ui-history" | "ui-event" => to_ui(Event::Chat(v)),
                        "native-host" | "native-host-ready" => crate::server::on_message(t.as_str(), host_tx.clone(), "the server"),
                        _ => {}
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
        let _ = ws.close(None);
        if !stop.load(Ordering::Relaxed) {
            to_ui(Event::Connected(false));
            thread::sleep(Duration::from_secs(2));
        }
    }
}

fn events_loop(api: Api, stop: Arc<AtomicBool>, to_ui: impl Fn(Event)) {
    while !stop.load(Ordering::Relaxed) {
        let r = api.events(|name, data| {
            if stop.load(Ordering::Relaxed) { return; }
            if name == "state" {
                if let Ok(v) = serde_json::from_str::<Value>(data) { to_ui(Event::State(v)); }
            }
        });
        match r {
            Err(ApiError::SignedOut) => { to_ui(Event::SignedOut); return; }
            Err(ApiError::Status(401, _)) => continue,
            _ => {}
        }
        thread::sleep(Duration::from_secs(3));
    }
}

/// This computer, for the server's list of native hosts.
fn device_id() -> String {
    format!("native-{}", gethostname::gethostname().to_string_lossy())
}
