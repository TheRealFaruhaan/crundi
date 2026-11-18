//! Phase 2 jobs against an in-test websocket server (unix: the commands use sh).
#![cfg(unix)]

use base64::Engine;
use crundi_worker::config::Config;
use crundi_worker::link::{self, LinkHandle, LinkOptions, Outcome};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

type Ws = WebSocketStream<TcpStream>;
const KEY: &str = "cw_w0123456789.c2VjcmV0c2VjcmV0";
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("cw-jobs-{name}-{}", crundi_worker::util::random_hex(6)));
    std::fs::create_dir_all(&p).unwrap();
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
    h: LinkHandle,
}

async fn start(name: &str) -> T {
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
    T { dir, links, h: link::spawn(cfg, o) }
}

impl T {
    /// Accept the next link, check hello, send welcome; returns (ws, hello).
    async fn connect(&mut self, settings: Value) -> (Ws, Value) {
        let mut ws = tokio::time::timeout(Duration::from_secs(20), self.links.recv()).await.unwrap().unwrap();
        let hello = recv_type(&mut ws, "hello").await;
        assert!(hello["caps"].as_array().unwrap().iter().any(|c| c == "jobs"));
        send(&mut ws, json!({"type": "welcome", "proto": 1, "id": "w0123456789", "name": "pc",
            "server": {"version": "9"}, "caps": ["terminal", "pause", "rename", "jobs"], "settings": settings})).await;
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
    }
}

async fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::Text(v.to_string().into())).await.unwrap();
}

#[derive(Default, Debug)]
struct Done {
    out: Vec<u8>,
    exit: Value,
    artifact: Vec<u8>,
    count: u64,
    phases: Vec<String>,
}

impl Done {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.out).into_owned()
    }
}

/// Read job frames until job.done, assembling output by offset.
async fn until_done(ws: &mut Ws, job: &str, mut d: Done) -> Done {
    let mut seq = 0;
    loop {
        let v = recv(ws).await;
        if v.get("job").and_then(Value::as_str) != Some(job) {
            continue;
        }
        match v["type"].as_str().unwrap() {
            "job.out" => {
                let off = v["off"].as_u64().unwrap() as usize;
                // Frames skipped by an earlier recv_type leave a gap: pad it.
                d.out.resize(off, 0);
                d.out.extend(B64.decode(v["b64"].as_str().unwrap()).unwrap());
            }
            "job.status" => d.phases.push(v["phase"].as_str().unwrap().to_string()),
            "job.exit" => d.exit = v,
            "job.artifact" => {
                assert!(!d.exit.is_null(), "artifact before exit");
                assert_eq!(v["seq"], seq);
                seq += 1;
                d.count = v["count"].as_u64().unwrap();
                d.artifact.extend(B64.decode(v["b64"].as_str().unwrap()).unwrap());
                if v["last"] == true {
                    assert_eq!(d.artifact.len() as u64, v["bytes"].as_u64().unwrap());
                }
            }
            "job.done" => return d,
            "job.refused" => panic!("refused: {v}"),
            _ => {}
        }
    }
}

fn job(id: &str, command: &str, extra: Value) -> Value {
    let mut v = json!({"type": "job.start", "job": id, "project": "proj", "command": command,
        "source": "none", "timeoutSec": 60, "artifacts": [], "lowPriority": true});
    for (k, x) in extra.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}

async fn run(ws: &mut Ws, start: Value) -> Done {
    let id = start["job"].as_str().unwrap().to_string();
    send(ws, start).await;
    let a = recv_type(ws, "job.accepted").await;
    assert_eq!(a["job"], id.as_str());
    until_done(ws, &id, Done::default()).await
}

#[tokio::test]
async fn command_output_and_exit_code() {
    let mut t = start("out").await;
    let (mut ws, hello) = t.connect(ok_settings()).await;
    assert_eq!(hello["jobs"], json!([]));
    let d = run(&mut ws, job("j01", "echo to-out; echo to-err 1>&2; echo \"$GREETING\"; exit 3", json!({"env": {"GREETING": "hello-env"}}))).await;
    let text = d.text();
    assert!(text.contains("to-out") && text.contains("to-err") && text.contains("hello-env"), "{text}");
    assert_eq!(d.exit["code"], 3);
    assert_eq!(d.exit["timedOut"], false);
    assert_eq!(d.exit["cancelled"], false);
    assert!(d.exit["durationMs"].as_u64().is_some());
    assert_eq!(d.count, 0);
    assert!(d.artifact.is_empty());
    assert_eq!(d.phases, ["preparing", "running", "collecting"]);
    // The log was spooled and is gone once collected.
    assert!(!t.dir.join("work/.crundi-jobs/j01.log").exists());
    // cwd must stay inside the project folder.
    let d = run(&mut ws, job("j02", "pwd", json!({"cwd": "../.."}))).await;
    assert_eq!(d.exit["code"], -1);
    assert!(d.exit["error"].as_str().unwrap().contains("inside"));
    t.quit().await;
}

fn list_tar_gz(bytes: &[u8]) -> Vec<(String, String)> {
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut ar = tar::Archive::new(gz);
    let mut out = Vec::new();
    for e in ar.entries().unwrap() {
        let mut e = e.unwrap();
        let p = e.path().unwrap().to_string_lossy().into_owned();
        let mut s = String::new();
        e.read_to_string(&mut s).unwrap();
        out.push((p, s));
    }
    out
}

#[tokio::test]
async fn artifacts_are_packed() {
    let mut t = start("art").await;
    std::fs::create_dir_all(t.dir.join("work/proj")).unwrap();
    let (mut ws, _) = t.connect(ok_settings()).await;
    let d = run(
        &mut ws,
        job("j03", "mkdir -p dist/sub && echo data > dist/a.txt && echo deep > dist/sub/b.txt && echo no > other.txt",
            json!({"artifacts": ["dist/**/*.txt"]})),
    )
    .await;
    assert_eq!(d.exit["code"], 0, "{}", d.text());
    assert_eq!(d.count, 2);
    let files = list_tar_gz(&d.artifact);
    assert_eq!(files, [("dist/a.txt".to_string(), "data\n".to_string()), ("dist/sub/b.txt".to_string(), "deep\n".to_string())]);
    assert!(t.dir.join("work/proj/other.txt").exists(), "ran in the project folder");
    t.quit().await;
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

#[tokio::test]
async fn git_source_with_bundle() {
    let mut t = start("git").await;
    // "GitHub": a bare repo with one commit on main.
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
    // A second commit only the server has.
    std::fs::write(srv.join("b.txt"), "two\n").unwrap();
    git(&srv, &["add", "."]);
    git(&srv, &["commit", "-q", "-m", "c2"]);
    let c2 = git(&srv, &["rev-parse", "HEAD"]);
    let url = format!("file://{}", bare.display());

    let (mut ws, _) = t.connect(ok_settings()).await;
    let d = run(
        &mut ws,
        job("j10", "cat a.txt && mkdir -p target && echo kept > target/cache",
            json!({"source": "git", "git": {"url": url, "branch": "main", "commit": c1}})),
    )
    .await;
    assert_eq!(d.exit["code"], 0, "{}", d.text());
    assert!(d.text().contains("one"));

    // c2 is not on origin: the worker asks for a bundle.
    send(&mut ws, job("j11", "cat b.txt && cat target/cache && git rev-parse HEAD",
        json!({"source": "git", "git": {"url": url, "branch": "main", "commit": c2}}))).await;
    recv_type(&mut ws, "job.accepted").await;
    let need = recv_type(&mut ws, "job.need").await;
    assert_eq!(need["kind"], "bundle");
    assert_eq!(need["have"], c1.as_str());
    let bundle = t.dir.join("x.bundle");
    git(&srv, &["bundle", "create", "-q", bundle.to_str().unwrap(), "main", &format!("^{c1}")]);
    let bytes = std::fs::read(&bundle).unwrap();
    let chunks: Vec<&[u8]> = bytes.chunks(300).collect();
    for (i, c) in chunks.iter().enumerate() {
        send(&mut ws, json!({"type": "job.bundle", "job": "j11", "seq": i, "b64": B64.encode(c), "last": i + 1 == chunks.len()})).await;
    }
    let d = until_done(&mut ws, "j11", Done::default()).await;
    let text = d.text();
    assert_eq!(d.exit["code"], 0, "{text}");
    assert!(text.contains("two"), "{text}");
    assert!(text.contains("kept"), "build caches persist: {text}");
    assert!(text.contains(&c2), "{text}");
    let branch = git(&t.dir.join("work/proj"), &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(branch, "crundi/job");
    t.quit().await;
}

fn tar_of(files: &[(&str, &[u8], u32)], links: &[(&str, &str)]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (p, data, mode) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(*mode);
        h.set_cksum();
        b.append_data(&mut h, p, *data).unwrap();
    }
    for (p, target) in links {
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        h.set_mode(0o777);
        b.append_link(&mut h, p, target).unwrap();
    }
    b.into_inner().unwrap()
}

async fn send_files(ws: &mut Ws, job: &str, tar: &[u8], removed: Value) {
    let chunks: Vec<&[u8]> = if tar.is_empty() { vec![&[][..]] } else { tar.chunks(1000).collect() };
    for (i, c) in chunks.iter().enumerate() {
        let last = i + 1 == chunks.len();
        let mut m = json!({"type": "job.files", "job": job, "seq": i, "b64": B64.encode(c), "last": last});
        if last {
            m["removed"] = removed.clone();
        }
        send(ws, m).await;
    }
}

#[tokio::test]
async fn copy_source_sends_only_changes() {
    let mut t = start("copy").await;
    let (mut ws, _) = t.connect(ok_settings()).await;

    send(&mut ws, job("j20", "cat a.txt; ./sub/run.sh; cat link; mkdir -p node_modules && touch node_modules/x", json!({"source": "copy"}))).await;
    recv_type(&mut ws, "job.accepted").await;
    let m = recv_type(&mut ws, "job.manifest").await;
    assert_eq!(m["files"], json!({}));
    let tar = tar_of(&[("a.txt", b"A\n", 0o644), ("sub/run.sh", b"#!/bin/sh\necho ran-script\n", 0o755)], &[("link", "a.txt")]);
    send_files(&mut ws, "j20", &tar, json!([])).await;
    let d = until_done(&mut ws, "j20", Done::default()).await;
    let text = d.text();
    assert_eq!(d.exit["code"], 0, "{text}");
    assert!(text.contains("ran-script"), "exec bit kept: {text}");
    assert_eq!(text.matches("A\n").count() >= 2, true, "symlink kept: {text}");
    let copy = t.dir.join("work/proj@copy");
    assert!(copy.join(".crundi-manifest.json").exists());

    // Second run: the manifest lists what is there (not node_modules); only a change and a removal come.
    send(&mut ws, job("j21", "cat a.txt; test ! -e sub/run.sh && echo script-gone; test -e node_modules/x && echo cache-kept", json!({"source": "copy"}))).await;
    recv_type(&mut ws, "job.accepted").await;
    let m = recv_type(&mut ws, "job.manifest").await;
    let files = m["files"].as_object().unwrap();
    let mut names: Vec<&str> = files.keys().map(String::as_str).collect();
    names.sort();
    assert_eq!(names, ["a.txt", "link", "sub/run.sh"]);
    assert_eq!(files["a.txt"][0], 2);
    assert_eq!(files["a.txt"][1], "06f961b802bc46ee168555f066d28f4f0e9afdf3f88174c1ee6f9de004fc30a0"); // sha256("A\n")
    send_files(&mut ws, "j21", &tar_of(&[("a.txt", b"B\n", 0o644)], &[]), json!(["sub/run.sh"])).await;
    let d = until_done(&mut ws, "j21", Done::default()).await;
    let text = d.text();
    assert!(text.contains("B\n") && text.contains("script-gone") && text.contains("cache-kept"), "{text}");
    assert!(!copy.join("sub").exists(), "empty folder removed");

    // A tar escaping the folder is refused.
    send(&mut ws, job("j22", "true", json!({"source": "copy"}))).await;
    recv_type(&mut ws, "job.manifest").await;
    // Write the name straight into the header (tar's builder refuses "..").
    let mut h = tar::Header::new_old();
    h.set_size(1);
    h.set_mode(0o644);
    let name = b"../evil.txt";
    h.as_old_mut().name[..name.len()].copy_from_slice(name);
    h.set_cksum();
    let mut b = tar::Builder::new(Vec::new());
    b.append(&h, &b"x"[..]).unwrap();
    let evil = b.into_inner().unwrap();
    send_files(&mut ws, "j22", &evil, json!([])).await;
    let d = until_done(&mut ws, "j22", Done::default()).await;
    assert_eq!(d.exit["code"], -1);
    assert!(d.exit["error"].as_str().unwrap().contains("outside"), "{}", d.exit);
    assert!(!t.dir.join("work/evil.txt").exists());
    t.quit().await;
}

#[tokio::test]
async fn cancel_kills_the_tree() {
    let mut t = start("cancel").await;
    let (mut ws, _) = t.connect(ok_settings()).await;
    let started = Instant::now();
    send(&mut ws, job("j30", "sleep 37.5 & sleep 30; echo never", json!({}))).await;
    recv_type(&mut ws, "job.accepted").await;
    loop {
        let s = recv_type(&mut ws, "job.status").await;
        if s["phase"] == "running" {
            break;
        }
    }
    // One at a time: a second job is refused.
    send(&mut ws, job("j31", "true", json!({}))).await;
    let r = recv_type(&mut ws, "job.refused").await;
    assert!(r["error"].as_str().unwrap().contains("already running"), "{r}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    send(&mut ws, json!({"type": "job.cancel", "job": "j30"})).await;
    let d = until_done(&mut ws, "j30", Done::default()).await;
    assert_eq!(d.exit["cancelled"], true);
    assert!(!d.text().contains("never"));
    assert!(started.elapsed() < Duration::from_secs(15));
    // The background sleep was killed too (same process group).
    let mut gone = false;
    for _ in 0..20 {
        let out = std::process::Command::new("pgrep").args(["-f", "sleep 37.5"]).output().unwrap();
        if out.stdout.is_empty() {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(gone, "the background child outlived the cancel");
    t.quit().await;
}

#[tokio::test]
async fn timeout_stops_the_job() {
    let mut t = start("timeout").await;
    let (mut ws, _) = t.connect(ok_settings()).await;
    let d = run(&mut ws, job("j40", "sleep 30", json!({"timeoutSec": 1}))).await;
    assert_eq!(d.exit["timedOut"], true);
    assert!(d.exit["durationMs"].as_u64().unwrap() < 10_000);
    assert!(d.text().contains("timed out"));
    t.quit().await;
}

#[tokio::test]
async fn job_survives_a_reconnect_and_attach_resends() {
    let mut t = start("attach").await;
    let (mut ws, _) = t.connect(ok_settings()).await;
    send(&mut ws, job("j50", "echo first; sleep 2; echo second", json!({}))).await;
    recv_type(&mut ws, "job.accepted").await;
    let mut seen = Vec::new();
    while !String::from_utf8_lossy(&seen).contains("first") {
        let v = recv_type(&mut ws, "job.out").await;
        seen.extend(B64.decode(v["b64"].as_str().unwrap()).unwrap());
    }
    drop(ws); // the link drops mid-job

    let (mut ws, hello) = t.connect(ok_settings()).await;
    let jobs = hello["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1, "{hello}");
    assert_eq!(jobs[0]["job"], "j50");
    assert!(jobs[0]["logBytes"].as_u64().unwrap() >= seen.len() as u64);
    send(&mut ws, json!({"type": "job.attach", "job": "j50", "from": 3})).await;
    let first = recv_type(&mut ws, "job.out").await;
    assert_eq!(first["off"], 3);
    let mut d = Done { out: seen[..3].to_vec(), ..Done::default() };
    d.out.extend(B64.decode(first["b64"].as_str().unwrap()).unwrap());
    let d = until_done(&mut ws, "j50", d).await;
    let text = d.text();
    assert!(text.starts_with("$ echo first"), "{text}");
    assert!(text.contains("first\n") && text.contains("second\n"), "{text}");
    assert_eq!(d.exit["code"], 0);

    // Collected: the next hello lists nothing, and an attach to it says unknown.
    drop(ws);
    let (mut ws, hello) = t.connect(ok_settings()).await;
    assert_eq!(hello["jobs"], json!([]));
    send(&mut ws, json!({"type": "job.attach", "job": "j50", "from": 0})).await;
    let done = recv_type(&mut ws, "job.done").await;
    assert_eq!(done["unknown"], true);
    t.quit().await;
}

#[tokio::test]
async fn refused_while_paused_or_not_allowed() {
    let mut t = start("paused").await;
    let (mut ws, _) = t.connect(json!({"allowChats": true, "allowCommands": true, "paused": true})).await;
    send(&mut ws, job("j60", "true", json!({}))).await;
    let r = recv_type(&mut ws, "job.refused").await;
    assert_eq!(r["job"], "j60");
    assert!(r["error"].as_str().unwrap().contains("paused"), "{r}");
    send(&mut ws, json!({"type": "settings", "paused": false, "allowCommands": false})).await;
    send(&mut ws, job("j61", "true", json!({}))).await;
    let r = recv_type(&mut ws, "job.refused").await;
    assert!(r["error"].as_str().unwrap().contains("not allowed"), "{r}");
    send(&mut ws, json!({"type": "settings", "allowCommands": true})).await;
    let d = run(&mut ws, job("j62", "echo fine", json!({}))).await;
    assert_eq!(d.exit["code"], 0);
    t.quit().await;
}
