//! Phase 4 chats against an in-test websocket server, with a fake `claude`
//! (a shell script that echoes its stdin lines). Unix only (sh scripts).
#![cfg(unix)]

use base64::Engine;
use crundi_worker::config::Config;
use crundi_worker::link::{self, LinkHandle, LinkOptions, Outcome};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

type Ws = WebSocketStream<TcpStream>;
const KEY: &str = "cw_w0123456789.c2VjcmV0c2VjcmV0";
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Echoes stdin lines; a SIGINT ends it with 130.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
trap 'echo interrupted; exit 130' INT
echo "fake-claude starting" 1>&2
echo "args: $*"
echo "cwd: $(pwd)"
while IFS= read -r line; do
  echo "got: $line"
done
echo bye
"#;

/// Ignores SIGTERM, so a close has to kill it.
const STUBBORN_CLAUDE: &str = r#"#!/bin/sh
trap '' TERM
echo ready
while true; do sleep 1; done
"#;

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("cw-chats-{name}-{}", crundi_worker::util::random_hex(6)));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let p = bin.join(name);
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

async fn server() -> (String, mpsc::UnboundedReceiver<Ws>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((s, _)) = l.accept().await {
            let tx = tx.clone();
            tokio::spawn(async move {
                if let Ok(ws) = tokio_tungstenite::accept_async(s).await {
                    let _ = tx.send(ws);
                }
            });
        }
    });
    (url, rx)
}

struct T {
    dir: PathBuf,
    links: mpsc::UnboundedReceiver<Ws>,
    events: mpsc::UnboundedReceiver<Value>,
    h: LinkHandle,
}

/// `claude`: the fake script's body, or None for "not installed".
async fn start(name: &str, claude: Option<&str>) -> T {
    let dir = tmp(name);
    let (url, links) = server().await;
    let cfg = Config {
        server: Some(url),
        id: Some("w0123456789".into()),
        key: Some(KEY.into()),
        name: Some("pc".into()),
        work_dir: dir.join("work"),
        ..Config::default()
    };
    let mut o = LinkOptions::new("headless", Some(dir.join("config.json")));
    o.backoff_base = Duration::from_millis(50);
    o.backoff_max = Duration::from_millis(200);
    o.claude = Some(match claude {
        Some(body) => script(&dir, "claude", body),
        None => dir.join("bin/no-claude-here"),
    });
    let (ev_tx, events) = mpsc::unbounded_channel();
    o.events = Some(ev_tx);
    T { dir, links, events, h: link::spawn(cfg, o) }
}

impl T {
    async fn connect(&mut self, settings: Value) -> (Ws, Value) {
        let mut ws = tokio::time::timeout(Duration::from_secs(20), self.links.recv()).await.unwrap().unwrap();
        let hello = recv_type(&mut ws, "hello").await;
        assert!(hello["caps"].as_array().unwrap().iter().any(|c| c == "chats"));
        send(&mut ws, json!({"type": "welcome", "proto": 1, "id": "w0123456789", "name": "pc",
            "server": {"version": "9"}, "caps": ["terminal", "pause", "rename", "jobs", "chats"], "settings": settings})).await;
        (ws, hello)
    }

    async fn quit(self) {
        self.h.ctrl.send(link::Control::Quit).unwrap();
        assert_eq!(self.h.task.await.unwrap(), Outcome::Quit);
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn ok_settings() -> Value {
    json!({"allowChats": true, "allowCommands": true, "paused": false})
}

async fn recv(ws: &mut Ws) -> Value {
    loop {
        let m = tokio::time::timeout(Duration::from_secs(30), ws.next())
            .await
            .expect("timed out waiting for a frame")
            .expect("link closed")
            .unwrap();
        if let Message::Text(t) = m {
            return serde_json::from_str(t.as_str()).unwrap();
        }
    }
}

async fn recv_type(ws: &mut Ws, ty: &str) -> Value {
    loop {
        let v = recv(ws).await;
        if v["type"] == ty {
            return v;
        }
        assert_ne!(v["type"], "chat.refused", "refused: {v}");
    }
}

async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::Text(v.to_string().into())).await.unwrap();
}

fn spawn_msg(chat: &str, extra: Value) -> Value {
    let mut v = json!({"type": "chat.spawn", "chat": chat, "project": "proj",
        "args": ["-p", "--output-format", "stream-json", "--mcp-config", "/server/only.json"],
        "source": "none", "env": {"GREETING": "hi"}});
    for (k, x) in extra.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}

fn chat_in(chat: &str, text: &str) -> Value {
    json!({"type": "chat.in", "chat": chat, "b64": B64.encode(text)})
}

/// Gather the chat's stdout (by offset) until it contains `needle`; returns
/// the chat.exit if it came first.
async fn stdout_until(ws: &mut Ws, chat: &str, out: &mut Vec<u8>, needle: &str) -> Option<Value> {
    while !String::from_utf8_lossy(out).contains(needle) {
        let v = recv(ws).await;
        if v["chat"] != chat {
            continue;
        }
        match v["type"].as_str().unwrap() {
            "chat.out" if v["stream"] == "stdout" => {
                let off = v["off"].as_u64().unwrap() as usize;
                out.truncate(off.min(out.len()));
                out.resize(off, 0);
                out.extend(B64.decode(v["b64"].as_str().unwrap()).unwrap());
            }
            "chat.exit" => return Some(v),
            "chat.refused" => panic!("refused: {v}"),
            _ => {}
        }
    }
    None
}

async fn exit_of(ws: &mut Ws, chat: &str) -> Value {
    loop {
        let v = recv_type(ws, "chat.exit").await;
        if v["chat"] == chat {
            return v;
        }
    }
}

#[tokio::test]
async fn chat_pipes_kill_and_events() {
    let mut t = start("pipes", Some(FAKE_CLAUDE)).await;
    let (mut ws, hello) = t.connect(ok_settings()).await;
    assert_eq!(hello["chats"], json!([]));
    send(&mut ws, spawn_msg("c1", json!({}))).await;
    let started = recv_type(&mut ws, "chat.started").await;
    assert_eq!(started["chat"], "c1");
    assert!(started["pid"].as_u64().unwrap() > 0);
    let ev = t.events.recv().await.unwrap();
    assert_eq!(ev, json!({"event": "chat", "state": "started", "chat": "c1", "project": "proj"}));

    // stderr comes as it is, stdout by offset.
    let err = recv_type(&mut ws, "chat.out").await;
    let mut out = Vec::new();
    if err["stream"] == "stderr" {
        assert!(String::from_utf8_lossy(&B64.decode(err["b64"].as_str().unwrap()).unwrap()).contains("fake-claude starting"));
    } else {
        out.extend(B64.decode(err["b64"].as_str().unwrap()).unwrap());
    }
    assert!(stdout_until(&mut ws, "c1", &mut out, "cwd: ").await.is_none());
    let text = String::from_utf8_lossy(&out).into_owned();
    // The server's --mcp-config is dropped and ours goes last.
    let mcp = t.dir.join("work/.crundi-chats/c1.mcp.json");
    assert!(text.contains(&format!("args: -p --output-format stream-json --mcp-config {}", mcp.display())), "{text}");
    let cfg: Value = serde_json::from_slice(&std::fs::read(&mcp).unwrap()).unwrap();
    let srv = &cfg["mcpServers"]["crundi"];
    assert_eq!(srv["command"], std::env::current_exe().unwrap().to_string_lossy().as_ref());
    let args: Vec<&str> = srv["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert_eq!(&args[..3], ["mcp", "--chat", "c1"]);
    assert_eq!(args[3], "--port");
    assert!(args[4].parse::<u16>().unwrap() > 0);
    assert_eq!(args[5], "--token");
    assert_eq!(args[6].len(), 32);

    send(&mut ws, chat_in("c1", "hello there\n")).await;
    assert!(stdout_until(&mut ws, "c1", &mut out, "got: hello there\n").await.is_none());
    assert!(t.dir.join("work/.crundi-chats/c1.out").exists(), "stdout is spooled");

    // A second spawn of a running chat is refused.
    send(&mut ws, spawn_msg("c1", json!({}))).await;
    let r = recv_type(&mut ws, "chat.refused").await;
    assert!(r["error"].as_str().unwrap().contains("already running"), "{r}");

    // close: SIGTERM.
    send(&mut ws, json!({"type": "chat.kill", "chat": "c1"})).await;
    let exit = exit_of(&mut ws, "c1").await;
    assert_eq!(exit["signal"], "SIGTERM", "{exit}");
    assert_eq!(exit["code"], 143);
    let ev = t.events.recv().await.unwrap();
    assert_eq!(ev["state"], "ended");
    // Collected: the spool and the MCP config are gone.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!t.dir.join("work/.crundi-chats/c1.out").exists());
    assert!(!mcp.exists());
    t.quit().await;
}

#[tokio::test]
async fn chat_survives_a_reconnect_and_attach_resends() {
    let mut t = start("attach", Some(FAKE_CLAUDE)).await;
    let (mut ws, _) = t.connect(ok_settings()).await;
    send(&mut ws, spawn_msg("c2", json!({}))).await;
    recv_type(&mut ws, "chat.started").await;
    send(&mut ws, chat_in("c2", "one\n")).await;
    let mut out = Vec::new();
    stdout_until(&mut ws, "c2", &mut out, "got: one\n").await;
    drop(ws); // the link drops; claude keeps running

    let (mut ws, hello) = t.connect(ok_settings()).await;
    let chats = hello["chats"].as_array().unwrap();
    assert_eq!(chats.len(), 1, "{hello}");
    assert_eq!(chats[0]["chat"], "c2");
    assert_eq!(chats[0]["running"], true);
    assert!(chats[0]["outBytes"].as_u64().unwrap() >= out.len() as u64);
    // stdin delivered on reconnect, then attach from 5.
    send(&mut ws, chat_in("c2", "two\n")).await;
    send(&mut ws, json!({"type": "chat.attach", "chat": "c2", "from": 5})).await;
    let first = loop {
        let v = recv_type(&mut ws, "chat.out").await;
        if v["stream"] == "stdout" {
            break v;
        }
    };
    assert_eq!(first["off"], 5);
    let mut again = out[..5].to_vec();
    again.extend(B64.decode(first["b64"].as_str().unwrap()).unwrap());
    stdout_until(&mut ws, "c2", &mut again, "got: two\n").await;
    let text = String::from_utf8_lossy(&again).into_owned();
    assert!(text.starts_with("args: ") && text.contains("got: one\n"), "{text}");

    // interrupt: SIGINT (the script traps it and exits 130).
    send(&mut ws, json!({"type": "chat.kill", "chat": "c2", "signal": "SIGINT"})).await;
    let exit = exit_of(&mut ws, "c2").await;
    assert_eq!(exit["code"], 130, "{exit}");

    // Gone: hello lists nothing; attach says unknown.
    drop(ws);
    let (mut ws, hello) = t.connect(ok_settings()).await;
    assert_eq!(hello["chats"], json!([]));
    send(&mut ws, json!({"type": "chat.attach", "chat": "c2", "from": 0})).await;
    let e = exit_of(&mut ws, "c2").await;
    assert_eq!(e["unknown"], true);
    t.quit().await;
}

#[tokio::test]
async fn close_kills_a_claude_that_ignores_sigterm() {
    let mut t = start("stubborn", Some(STUBBORN_CLAUDE)).await;
    let (mut ws, _) = t.connect(ok_settings()).await;
    send(&mut ws, spawn_msg("c3", json!({}))).await;
    recv_type(&mut ws, "chat.started").await;
    let mut out = Vec::new();
    stdout_until(&mut ws, "c3", &mut out, "ready").await;
    let at = Instant::now();
    send(&mut ws, json!({"type": "chat.kill", "chat": "c3", "signal": "SIGTERM"})).await;
    let exit = exit_of(&mut ws, "c3").await;
    assert_eq!(exit["signal"], "SIGKILL", "{exit}");
    let took = at.elapsed();
    assert!(took >= Duration::from_secs(4) && took < Duration::from_secs(15), "{took:?}");
    t.quit().await;
}

#[tokio::test]
async fn refused_without_claude_or_when_not_allowed() {
    let mut t = start("missing", None).await;
    let (mut ws, _) = t.connect(ok_settings()).await;
    send(&mut ws, spawn_msg("c4", json!({}))).await;
    let r = recv_type(&mut ws, "chat.refused").await;
    let e = r["error"].as_str().unwrap();
    assert!(e.starts_with("Install Claude Code on pc: curl -fsSL https://claude.ai/install.sh | bash"), "{e}");
    assert!(e.contains("irm https://claude.ai/install.ps1 | iex") && e.contains("sign in"), "{e}");
    send(&mut ws, json!({"type": "settings", "allowChats": false})).await;
    send(&mut ws, spawn_msg("c5", json!({}))).await;
    let r = recv_type(&mut ws, "chat.refused").await;
    assert!(r["error"].as_str().unwrap().contains("not allowed"), "{r}");
    send(&mut ws, json!({"type": "settings", "allowChats": true, "paused": true})).await;
    send(&mut ws, spawn_msg("c6", json!({}))).await;
    let r = recv_type(&mut ws, "chat.refused").await;
    assert!(r["error"].as_str().unwrap().contains("paused"), "{r}");
    t.quit().await;
}

#[test]
fn claude_is_found_on_path_or_where_it_installs() {
    use crundi_worker::chats::find_claude_in;
    let d = tmp("find");
    let p = script(&d, "claude", "#!/bin/sh\n");
    let path = std::env::join_paths([d.join("nothing"), d.join("bin")]).unwrap();
    assert_eq!(find_claude_in(Some(&path), None, None), Some(p.clone()));
    // Not on PATH: ~/.local/bin/claude.
    let home = d.join("home");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    let local = home.join(".local/bin/claude");
    std::fs::copy(&p, &local).unwrap();
    std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o755)).unwrap();
    let empty = std::ffi::OsString::from(d.join("nothing"));
    assert_eq!(find_claude_in(Some(&empty), Some(&home), None), Some(local));
    if !Path::new("/usr/local/bin/claude").exists() && !Path::new("/opt/homebrew/bin/claude").exists() {
        assert_eq!(find_claude_in(Some(&empty), Some(&d.join("nohome")), None), None);
    }
    std::fs::remove_dir_all(&d).ok();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Pulled {
    bundle: Vec<u8>,
    head: String,
    patch: Vec<u8>,
}

async fn pull_git(ws: &mut Ws, chat: &str, req: &str, base: &str) -> Pulled {
    send(ws, json!({"type": "chat.pull", "chat": chat, "req": req, "base": base})).await;
    let mut p = Pulled { bundle: Vec::new(), head: String::new(), patch: Vec::new() };
    let mut seq = 0;
    loop {
        let v = recv_type(ws, "chat.bundle").await;
        assert_eq!(v["req"], req);
        assert!(v.get("error").is_none(), "{v}");
        assert_eq!(v["seq"], seq);
        seq += 1;
        p.bundle.extend(B64.decode(v["b64"].as_str().unwrap()).unwrap());
        if v["last"] == true {
            p.head = v["head"].as_str().unwrap().to_string();
            break;
        }
    }
    let v = recv_type(ws, "chat.patch").await;
    assert_eq!(v["req"], req);
    assert!(v.get("error").is_none(), "{v}");
    p.patch = B64.decode(v["b64"].as_str().unwrap()).unwrap();
    p
}

#[tokio::test]
async fn git_chat_pull_gives_a_bundle_and_a_patch() {
    let mut t = start("gitpull", Some(FAKE_CLAUDE)).await;
    let bare = t.dir.join("origin.git");
    let srv = t.dir.join("srv");
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::create_dir_all(&srv).unwrap();
    git(&bare, &["init", "--bare", "-q"]);
    git(&srv, &["init", "-q", "-b", "main"]);
    std::fs::write(srv.join("a.txt"), "one\n").unwrap();
    git(&srv, &["add", "."]);
    git(&srv, &["commit", "-q", "-m", "c1"]);
    let c1 = git(&srv, &["rev-parse", "HEAD"]);
    git(&srv, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&srv, &["push", "-q", "origin", "main"]);
    let url = format!("file://{}", bare.display());
    let gitspec = json!({"source": "git", "git": {"url": url, "branch": "main", "commit": c1}});

    let (mut ws, _) = t.connect(ok_settings()).await;
    send(&mut ws, spawn_msg("g1", gitspec.clone())).await;
    recv_type(&mut ws, "chat.started").await;
    let mut out = Vec::new();
    stdout_until(&mut ws, "g1", &mut out, "proj@g1\n").await;
    let wt = t.dir.join("work/proj@g1");
    assert!(String::from_utf8_lossy(&out).contains(&format!("cwd: {}", wt.display())), "{}", String::from_utf8_lossy(&out));
    assert_eq!(git(&wt, &["rev-parse", "--abbrev-ref", "HEAD"]), "pc/g1");

    // Claude's work: a commit, a changed tracked file, a new untracked file (binary).
    std::fs::write(wt.join("b.txt"), "committed on the pc\n").unwrap();
    git(&wt, &["add", "b.txt"]);
    git(&wt, &["commit", "-q", "-m", "pc work"]);
    let pc_head = git(&wt, &["rev-parse", "HEAD"]);
    std::fs::write(wt.join("a.txt"), "one\nchanged\n").unwrap();
    std::fs::write(wt.join("new.bin"), [0u8, 1, 2, 255, 0, 10]).unwrap();
    let status_before = git(&wt, &["status", "--porcelain"]);

    let p = pull_git(&mut ws, "g1", "r1", &c1).await;
    assert_eq!(p.head, pc_head);
    assert!(!p.bundle.is_empty());
    // The PC's index is as it was (new.bin still untracked).
    assert_eq!(git(&wt, &["status", "--porcelain"]), status_before);
    assert!(status_before.contains("?? new.bin"), "{status_before}");

    // Apply on a clone of the server's repo at base.
    let check = t.dir.join("check");
    git(&t.dir, &["clone", "-q", srv.to_str().unwrap(), check.to_str().unwrap()]);
    let bundle = t.dir.join("pull.bundle");
    std::fs::write(&bundle, &p.bundle).unwrap();
    git(&check, &["bundle", "verify", "-q", bundle.to_str().unwrap()]);
    git(&check, &["fetch", "-q", bundle.to_str().unwrap(), "refs/heads/pc/g1:refs/heads/from-pc"]);
    assert_eq!(git(&check, &["rev-parse", "from-pc"]), pc_head);
    git(&check, &["checkout", "-q", "from-pc"]);
    let patch = t.dir.join("pull.patch");
    std::fs::write(&patch, &p.patch).unwrap();
    git(&check, &["apply", patch.to_str().unwrap()]);
    assert_eq!(std::fs::read_to_string(check.join("a.txt")).unwrap(), "one\nchanged\n");
    assert_eq!(std::fs::read_to_string(check.join("b.txt")).unwrap(), "committed on the pc\n");
    assert_eq!(std::fs::read(check.join("new.bin")).unwrap(), [0u8, 1, 2, 255, 0, 10]);

    // Nothing new since head: one empty last chunk with head == base.
    let p2 = pull_git(&mut ws, "g1", "r2", &pc_head).await;
    assert!(p2.bundle.is_empty());
    assert_eq!(p2.head, pc_head);
    assert!(!p2.patch.is_empty(), "the uncommitted work is still there");

    // Close and resume: the same folder, branch and uncommitted work.
    send(&mut ws, json!({"type": "chat.kill", "chat": "g1"})).await;
    exit_of(&mut ws, "g1").await;
    send(&mut ws, spawn_msg("g1", gitspec)).await;
    recv_type(&mut ws, "chat.started").await;
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), pc_head);
    assert_eq!(std::fs::read_to_string(wt.join("a.txt")).unwrap(), "one\nchanged\n");
    send(&mut ws, json!({"type": "chat.kill", "chat": "g1"})).await;
    exit_of(&mut ws, "g1").await;
    // An unknown chat's pull says so.
    send(&mut ws, json!({"type": "chat.pull", "chat": "nope", "req": "r3", "base": ""})).await;
    let e = recv_type(&mut ws, "chat.patch").await;
    assert!(e["error"].as_str().unwrap().contains("does not know"), "{e}");
    t.quit().await;
}

fn tar_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (p, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, p, *data).unwrap();
    }
    b.into_inner().unwrap()
}

#[tokio::test]
async fn copy_chat_pull_sends_changed_files_and_removals() {
    let mut t = start("copypull", Some(FAKE_CLAUDE)).await;
    let (mut ws, _) = t.connect(ok_settings()).await;
    send(&mut ws, spawn_msg("k1", json!({"source": "copy"}))).await;
    let m = recv_type(&mut ws, "chat.manifest").await;
    assert_eq!(m["chat"], "k1");
    assert_eq!(m["files"], json!({}));
    let tar = tar_of(&[("a.txt", b"A\n"), ("b.txt", b"B\n"), ("keep.txt", b"K\n")]);
    send(&mut ws, json!({"type": "chat.files", "chat": "k1", "seq": 0, "b64": B64.encode(&tar), "last": true, "removed": []})).await;
    recv_type(&mut ws, "chat.started").await;
    let copy = t.dir.join("work/proj@copy");
    assert_eq!(std::fs::read_to_string(copy.join("a.txt")).unwrap(), "A\n");

    std::fs::write(copy.join("a.txt"), "A2\n").unwrap();
    std::fs::remove_file(copy.join("b.txt")).unwrap();
    std::fs::create_dir_all(copy.join("sub")).unwrap();
    std::fs::write(copy.join("sub/c.txt"), "C\n").unwrap();
    std::fs::create_dir_all(copy.join("node_modules")).unwrap();
    std::fs::write(copy.join("node_modules/x.js"), "x").unwrap();

    send(&mut ws, json!({"type": "chat.pull", "chat": "k1", "req": "p1", "base": ""})).await;
    let mut data = Vec::new();
    let removed = loop {
        let v = recv_type(&mut ws, "chat.files").await;
        assert_eq!(v["req"], "p1");
        assert!(v.get("error").is_none(), "{v}");
        data.extend(B64.decode(v["b64"].as_str().unwrap()).unwrap());
        if v["last"] == true {
            break v["removed"].clone();
        }
    };
    assert_eq!(removed, json!(["b.txt"]));
    let mut ar = tar::Archive::new(&data[..]);
    let mut got: Vec<(String, String)> = ar
        .entries()
        .unwrap()
        .map(|e| {
            let mut e = e.unwrap();
            let p = e.path().unwrap().to_string_lossy().into_owned();
            let mut s = String::new();
            e.read_to_string(&mut s).unwrap();
            (p, s)
        })
        .collect();
    got.sort();
    assert_eq!(got, [("a.txt".to_string(), "A2\n".to_string()), ("sub/c.txt".to_string(), "C\n".to_string())]);

    // Resumed: the copy is not synced again (the chat's edits stay).
    send(&mut ws, json!({"type": "chat.kill", "chat": "k1"})).await;
    exit_of(&mut ws, "k1").await;
    send(&mut ws, spawn_msg("k1", json!({"source": "copy"}))).await;
    recv_type(&mut ws, "chat.started").await;
    assert_eq!(std::fs::read_to_string(copy.join("a.txt")).unwrap(), "A2\n");
    t.quit().await;
}

// ---- MCP ----

struct Mcp {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    out: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
}

impl Mcp {
    fn spawn(args: &[String]) -> Mcp {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_crundi-worker"))
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let out = tokio::io::BufReader::new(child.stdout.take().unwrap()).lines();
        Mcp { child, stdin, out }
    }

    async fn send(&mut self, v: Value) {
        self.stdin.write_all(format!("{v}\n").as_bytes()).await.unwrap();
    }

    async fn reply(&mut self) -> Value {
        let l = tokio::time::timeout(Duration::from_secs(30), self.out.next_line()).await.expect("no MCP reply").unwrap().unwrap();
        serde_json::from_str(&l).unwrap()
    }

    async fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await;
        let r = self.reply().await;
        assert_eq!(r["id"], id, "{r}");
        r
    }
}

/// A fake worker relay: checks the token, answers list and call.
async fn fake_relay(token: &'static str) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((s, _)) = l.accept().await {
            tokio::spawn(async move {
                let (r, mut w) = s.into_split();
                let mut lines = tokio::io::BufReader::new(r).lines();
                let hello: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                if hello["token"] != token {
                    w.write_all(b"{\"ok\":false,\"error\":\"bad token\"}\n").await.unwrap();
                    return;
                }
                assert_eq!(hello["chat"], "c9");
                w.write_all(b"{\"ok\":true}\n").await.unwrap();
                while let Ok(Some(l)) = lines.next_line().await {
                    let v: Value = serde_json::from_str(&l).unwrap();
                    let result = match (v["op"].as_str().unwrap(), v["tool"].as_str().unwrap_or("")) {
                        ("list", _) => json!({"tools": [{"name": "show_image", "inputSchema": {"type": "object"}}]}),
                        (_, "browser_screenshot") => json!({"ok": true, "data": "iVBORw0KGgo="}),
                        (_, "fails") => json!({"ok": false, "error": "no such thing"}),
                        (_, "mixed") => json!({"ok": true, "content": [{"type": "text", "text": "hi"}, {"type": "image", "data": "AAAA", "mimeType": "image/jpeg"}]}),
                        (_, t) => json!({"ok": true, "echo": {"tool": t, "args": v["args"]}}),
                    };
                    w.write_all(format!("{}\n", json!({"id": v["id"], "result": result})).as_bytes()).await.unwrap();
                }
            });
        }
    });
    port
}

#[tokio::test]
async fn mcp_subcommand_against_a_fake_relay() {
    let port = fake_relay("tok").await;
    let args: Vec<String> = ["mcp", "--chat", "c9", "--port", &port.to_string(), "--token", "tok"].iter().map(|s| s.to_string()).collect();
    let mut m = Mcp::spawn(&args);
    let r = m.call(1, "initialize", json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}})).await;
    assert_eq!(r["jsonrpc"], "2.0");
    assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(r["result"]["capabilities"], json!({"tools": {}}));
    assert_eq!(r["result"]["serverInfo"]["name"], "crundi");
    m.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).await; // no answer
    let r = m.call(2, "ping", json!({})).await;
    assert_eq!(r["result"], json!({}));
    let r = m.call(3, "tools/list", json!({})).await;
    assert_eq!(r["result"]["tools"][0]["name"], "show_image");
    let r = m.call(4, "tools/call", json!({"name": "browser_screenshot", "arguments": {"key": "k"}})).await;
    assert_eq!(r["result"], json!({"content": [{"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"}]}));
    let r = m.call(5, "tools/call", json!({"name": "fails", "arguments": {}})).await;
    assert_eq!(r["result"], json!({"content": [{"type": "text", "text": "Error: no such thing"}], "isError": true}));
    let r = m.call(6, "tools/call", json!({"name": "mixed"})).await;
    assert_eq!(r["result"]["content"][1]["mimeType"], "image/jpeg");
    let r = m.call(7, "tools/call", json!({"name": "kanban_list", "arguments": {"a": 1}})).await;
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(serde_json::from_str::<Value>(text).unwrap(), json!({"ok": true, "echo": {"tool": "kanban_list", "args": {"a": 1}}}));
    assert!(text.starts_with("{\n  \"ok\": true,"), "pretty JSON in insertion order: {text}");
    assert!(r["result"].get("isError").is_none());
    let r = m.call(8, "resources/list", json!({})).await;
    assert_eq!(r["error"]["code"], -32601);
    drop(m.stdin);
    let st = tokio::time::timeout(Duration::from_secs(10), m.child.wait()).await.unwrap().unwrap();
    assert!(st.success());

    // A wrong token: calls fail as MCP errors, not a crash.
    let args: Vec<String> = ["mcp", "--chat", "c9", "--port", &port.to_string(), "--token", "wrong"].iter().map(|s| s.to_string()).collect();
    let mut m = Mcp::spawn(&args);
    let r = m.call(1, "tools/call", json!({"name": "x"})).await;
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("bad token"), "{r}");
}

#[tokio::test]
async fn mcp_relay_end_to_end_survives_a_reconnect() {
    let mut t = start("relay", Some(FAKE_CLAUDE)).await;
    let (mut ws, _) = t.connect(ok_settings()).await;
    send(&mut ws, spawn_msg("m1", json!({}))).await;
    recv_type(&mut ws, "chat.started").await;
    let cfg: Value = serde_json::from_slice(&std::fs::read(t.dir.join("work/.crundi-chats/m1.mcp.json")).unwrap()).unwrap();
    let args: Vec<String> = cfg["mcpServers"]["crundi"]["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect();
    // claude would start the worker binary with these args.
    let mut m = Mcp::spawn(&args);
    let r = m.call(1, "initialize", json!({})).await;
    assert_eq!(r["result"]["serverInfo"]["name"], "crundi");

    m.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await;
    let list = recv_type(&mut ws, "mcp.list").await;
    assert_eq!(list["chat"], "m1");
    send(&mut ws, json!({"type": "mcp.tools", "req": list["req"], "tools": [{"name": "kanban_list"}]})).await;
    let r = m.reply().await;
    assert_eq!(r["result"]["tools"], json!([{"name": "kanban_list"}]));

    m.send(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "browser_screenshot", "arguments": {"key": "k1"}}})).await;
    let call = recv_type(&mut ws, "mcp.call").await;
    assert_eq!(call["chat"], "m1");
    assert_eq!(call["tool"], "browser_screenshot");
    assert_eq!(call["args"], json!({"key": "k1"}));
    // The link drops before the answer: the call waits and is sent again.
    drop(ws);
    let (mut ws, _) = t.connect(ok_settings()).await;
    let again = recv_type(&mut ws, "mcp.call").await;
    assert_eq!(again["req"], call["req"]);
    send(&mut ws, json!({"type": "mcp.result", "req": again["req"], "result": {"ok": true, "data": "QUJD"}})).await;
    let r = m.reply().await;
    assert_eq!(r["id"], 3);
    assert_eq!(r["result"]["content"][0], json!({"type": "image", "data": "QUJD", "mimeType": "image/png"}));

    // A wrong token is refused by the relay.
    let mut bad = args.clone();
    *bad.last_mut().unwrap() = "0".repeat(32);
    let mut m2 = Mcp::spawn(&bad);
    let r = m2.call(1, "tools/list", json!({})).await;
    assert!(r["error"]["message"].as_str().unwrap().contains("bad token"), "{r}");
    // Output (and the exit) flow again once the server attaches.
    send(&mut ws, json!({"type": "chat.attach", "chat": "m1", "from": 0})).await;
    send(&mut ws, json!({"type": "chat.kill", "chat": "m1"})).await;
    exit_of(&mut ws, "m1").await;
    t.quit().await;
}
