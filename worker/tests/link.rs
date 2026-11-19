//! The link against an in-test websocket server.

#[cfg(unix)]
use base64::Engine;
use crundi_worker::config::Config;
use crundi_worker::link::{self, LinkOptions, Outcome, State};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

const KEY: &str = "cw_w0123456789.c2VjcmV0c2VjcmV0";
type Ws = WebSocketStream<TcpStream>;

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("cw-link-{name}-{}", crundi_worker::util::random_hex(6)));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A websocket server on 127.0.0.1:0. Accepts links with the right bearer key
/// on /api/workers/link, answers 401 otherwise. Accepted links arrive on the channel.
async fn server(reject_all: bool) -> (String, mpsc::UnboundedReceiver<Ws>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = l.accept().await else { break };
            let tx = tx.clone();
            tokio::spawn(async move {
                let cb = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
                    let auth = req.headers().get("authorization").and_then(|h| h.to_str().ok());
                    if reject_all || req.uri().path() != "/api/workers/link" || auth != Some(&format!("Bearer {KEY}")) {
                        let mut e = ErrorResponse::new(Some("unauthorized".into()));
                        *e.status_mut() = StatusCode::UNAUTHORIZED;
                        return Err(e);
                    }
                    Ok(resp)
                };
                if let Ok(ws) = tokio_tungstenite::accept_hdr_async(s, cb).await {
                    let _ = tx.send(ws);
                }
            });
        }
    });
    (url, rx)
}

fn cfg(server: &str, dir: &PathBuf) -> Config {
    Config {
        server: Some(server.to_string()),
        id: Some("w0123456789".into()),
        key: Some(KEY.into()),
        name: Some("test-pc".into()),
        work_dir: dir.join("work"),
        device_id: Some("dev1".into()),
        ..Config::default()
    }
}

fn opts(path: &PathBuf) -> LinkOptions {
    let mut o = LinkOptions::new("headless", Some(path.clone()));
    o.backoff_base = Duration::from_millis(50);
    o.backoff_max = Duration::from_millis(200);
    o.shell = Some(vec!["/bin/sh".into()]);
    o
}

async fn recv(ws: &mut Ws) -> Value {
    loop {
        let m = tokio::time::timeout(Duration::from_secs(20), ws.next())
            .await
            .expect("timed out waiting for a frame")
            .expect("link closed")
            .unwrap();
        if let Message::Text(t) = m {
            return serde_json::from_str(t.as_str()).unwrap();
        }
    }
}

/// The next frame of the given type, skipping others (e.g. periodic info).
async fn recv_type(ws: &mut Ws, ty: &str) -> Value {
    loop {
        let v = recv(ws).await;
        if v["type"] == ty {
            return v;
        }
    }
}

async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::Text(v.to_string().into())).await.unwrap();
}

async fn accept(rx: &mut mpsc::UnboundedReceiver<Ws>) -> Ws {
    tokio::time::timeout(Duration::from_secs(20), rx.recv()).await.expect("no link came").unwrap()
}

async fn welcome(ws: &mut Ws, settings: Value) {
    send(ws, json!({"type": "welcome", "proto": 1, "id": "w0123456789", "name": "test-pc", "server": {"version": "9.9.9"}, "caps": ["terminal", "pause", "rename"], "settings": settings})).await;
}

#[tokio::test]
async fn hello_welcome_and_settings_persist() {
    let dir = tmp("hello");
    let path = dir.join("config.json");
    let (url, mut links) = server(false).await;
    let h = link::spawn(cfg(&url, &dir), opts(&path));
    let mut ws = accept(&mut links).await;

    let hello = recv(&mut ws).await;
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["proto"], 1);
    assert_eq!(hello["kind"], "headless");
    assert_eq!(hello["name"], "test-pc");
    assert_eq!(hello["deviceId"], "dev1");
    assert_eq!(hello["version"], crundi_worker::util::VERSION);
    assert_eq!(hello["caps"], json!(["info", "terminal", "pause", "jobs", "chats", "chat-sync"]));
    assert_eq!(hello["chats"], json!([]));
    assert_eq!(hello["jobs"], json!([]));
    for k in ["os", "arch", "cpu", "ramMb", "disk", "tools", "allow", "paused", "workDir"] {
        assert!(hello["info"].get(k).is_some(), "info lacks {k}");
    }

    welcome(&mut ws, json!({"allowChats": false, "allowCommands": true, "paused": true})).await;
    let info = recv_type(&mut ws, "info").await;
    assert_eq!(info["info"]["paused"], true);
    assert_eq!(info["info"]["allow"], json!({"chats": false, "commands": true}));
    let mut st = h.status.clone();
    tokio::time::timeout(Duration::from_secs(5), st.wait_for(|s| s.state == State::Online && s.paused))
        .await
        .unwrap()
        .unwrap();

    send(&mut ws, json!({"type": "settings", "name": "renamed", "paused": false, "allowCommands": false})).await;
    let info = recv_type(&mut ws, "info").await;
    assert_eq!(info["info"]["paused"], false);
    assert_eq!(info["info"]["allow"], json!({"chats": false, "commands": false}));
    let saved = Config::load(&path).unwrap();
    assert_eq!(saved.name.as_deref(), Some("renamed"));
    assert!(!saved.allow_chats && !saved.allow_commands && !saved.paused);
    assert_eq!(saved.key.as_deref(), Some(KEY));

    // A pause from the PC is sent as `paused` by "pc".
    h.ctrl.send(link::Control::Pause(true)).unwrap();
    let p = recv_type(&mut ws, "paused").await;
    assert_eq!(p, json!({"type": "paused", "paused": true, "by": "pc"}));
    assert!(Config::load(&path).unwrap().paused);

    // Unknown types are ignored.
    send(&mut ws, json!({"type": "something.new", "x": 1})).await;

    h.ctrl.send(link::Control::Quit).unwrap();
    assert_eq!(h.task.await.unwrap(), Outcome::Quit);
    std::fs::remove_dir_all(dir).ok();
}

#[cfg(unix)]
#[tokio::test]
async fn terminal_runs_a_command() {
    let dir = tmp("term");
    let path = dir.join("config.json");
    let (url, mut links) = server(false).await;
    let h = link::spawn(cfg(&url, &dir), opts(&path));
    let mut ws = accept(&mut links).await;
    recv_type(&mut ws, "hello").await;
    welcome(&mut ws, json!({"allowChats": true, "allowCommands": true, "paused": false})).await;

    send(&mut ws, json!({"type": "term.open", "ref": "t1", "cols": 100, "rows": 30})).await;
    let b64 = base64::engine::general_purpose::STANDARD;
    let input = b64.encode("echo hi-$((40+2)); pwd; exit 7\n");
    send(&mut ws, json!({"type": "term.input", "ref": "t1", "b64": input})).await;
    send(&mut ws, json!({"type": "term.resize", "ref": "t1", "cols": 120, "rows": 40})).await;

    let mut out = String::new();
    let code = loop {
        let v = recv(&mut ws).await;
        match v["type"].as_str().unwrap() {
            "term.data" => {
                assert_eq!(v["ref"], "t1");
                out.push_str(&String::from_utf8_lossy(&b64.decode(v["b64"].as_str().unwrap()).unwrap()));
            }
            "term.exit" => break v["code"].as_u64().unwrap(),
            "term.error" => panic!("term.error: {v}"),
            _ => {}
        }
    };
    assert!(out.contains("hi-42"), "output: {out:?}");
    assert!(out.contains("work"), "cwd is the work folder: {out:?}");
    assert_eq!(code, 7);

    // More than 8 terminals are refused; an unknown ref is an error.
    send(&mut ws, json!({"type": "term.input", "ref": "nope", "b64": "eA=="})).await;
    let e = recv_type(&mut ws, "term.error").await;
    assert_eq!(e["ref"], "nope");
    for i in 0..9 {
        send(&mut ws, json!({"type": "term.open", "ref": format!("m{i}"), "cols": 80, "rows": 24})).await;
    }
    let e = recv_type(&mut ws, "term.error").await;
    assert_eq!(e["ref"], "m8");
    for i in 0..8 {
        send(&mut ws, json!({"type": "term.close", "ref": format!("m{i}")})).await;
    }
    let mut exits = 0;
    while exits < 8 {
        if recv(&mut ws).await["type"] == "term.exit" {
            exits += 1;
        }
    }

    h.ctrl.send(link::Control::Quit).unwrap();
    assert_eq!(h.task.await.unwrap(), Outcome::Quit);
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn bye_revoked_forgets_the_key() {
    let dir = tmp("bye");
    let path = dir.join("config.json");
    let c = cfg("http://placeholder", &dir);
    let (url, mut links) = server(false).await;
    let c = Config { server: Some(url), ..c };
    c.save(&path).unwrap();
    let h = link::spawn(c, opts(&path));
    let mut ws = accept(&mut links).await;
    recv_type(&mut ws, "hello").await;
    welcome(&mut ws, json!({"allowChats": true, "allowCommands": true, "paused": false})).await;
    send(&mut ws, json!({"type": "bye", "reason": "revoked"})).await;
    let outcome = tokio::time::timeout(Duration::from_secs(10), h.task).await.unwrap().unwrap();
    assert_eq!(outcome, Outcome::Removed("revoked".into()));
    let saved = Config::load(&path).unwrap();
    assert_eq!(saved.key, None);
    assert_eq!(saved.id.as_deref(), Some("w0123456789"));
    assert_eq!(h.status.borrow().state, State::Removed);
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn reconnects_after_the_server_drops() {
    let dir = tmp("reconnect");
    let path = dir.join("config.json");
    let (url, mut links) = server(false).await;
    let h = link::spawn(cfg(&url, &dir), opts(&path));

    let mut ws = accept(&mut links).await;
    recv_type(&mut ws, "hello").await;
    welcome(&mut ws, json!({"paused": false})).await;
    drop(ws); // the server goes away without a close

    let mut ws2 = accept(&mut links).await;
    let hello = recv_type(&mut ws2, "hello").await;
    assert_eq!(hello["name"], "test-pc");
    welcome(&mut ws2, json!({"paused": false})).await;
    let mut st = h.status.clone();
    tokio::time::timeout(Duration::from_secs(5), st.wait_for(|s| s.state == State::Online))
        .await
        .unwrap()
        .unwrap();

    // A clean close also reconnects.
    ws2.close(None).await.unwrap();
    let mut ws3 = accept(&mut links).await;
    recv_type(&mut ws3, "hello").await;

    h.ctrl.send(link::Control::Quit).unwrap();
    assert_eq!(h.task.await.unwrap(), Outcome::Quit);
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn unauthorized_stops() {
    let dir = tmp("401");
    let path = dir.join("config.json");
    let (url, _links) = server(true).await;
    let h = link::spawn(cfg(&url, &dir), opts(&path));
    let outcome = tokio::time::timeout(Duration::from_secs(10), h.task).await.unwrap().unwrap();
    assert_eq!(outcome, Outcome::Unauthorized);
    assert_eq!(crundi_worker::app::exit_code(&outcome), 3);
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn not_joined_does_not_connect() {
    let dir = tmp("nj");
    let h = link::spawn(Config { work_dir: dir.join("w"), ..Config::default() }, opts(&dir.join("c.json")));
    assert_eq!(h.task.await.unwrap(), Outcome::NotJoined);
    std::fs::remove_dir_all(dir).ok();
}
