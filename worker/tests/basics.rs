//! Config round-trip, the info JSON shape, and join against an in-test HTTP server.

use crundi_worker::config::Config;
use crundi_worker::info::{self, Allow, Cpu, Disk, Gpu, Info, InfoInputs};
use crundi_worker::join::{join_and_save, JoinArgs};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("cw-test-{name}-{}", crundi_worker::util::random_hex(6)));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn config_round_trip() {
    let dir = tmp("cfg");
    let path = dir.join("sub").join("config.json");
    // Missing file: defaults.
    let d = Config::load(&path).unwrap();
    assert!(!d.joined());
    assert!(d.allow_chats && d.allow_commands && !d.paused);
    assert!(d.work_dir.ends_with("crundi-work"));

    let mut c = Config {
        server: Some("https://crundi.example.com".into()),
        id: Some("w0123456789".into()),
        key: Some("cw_w0123456789.c2VjcmV0".into()),
        name: Some("Desk PC".into()),
        work_dir: dir.join("work"),
        allow_chats: false,
        allow_commands: true,
        paused: true,
        device_id: Some("abcd".into()),
        extra: Default::default(),
    };
    c.extra.insert("futureField".into(), json!({"x": 1}));
    c.save(&path).unwrap();
    let back = Config::load(&path).unwrap();
    assert_eq!(back, c);
    assert!(back.joined());

    // The file uses the spec's camelCase names.
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for k in ["server", "id", "key", "name", "workDir", "allowChats", "allowCommands", "paused", "deviceId", "futureField"] {
        assert!(v.get(k).is_some(), "config.json lacks {k}: {v}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    std::fs::remove_dir_all(dir).ok();
}

fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

#[test]
fn info_json_shape() {
    let mut tools = BTreeMap::new();
    tools.insert("git".to_string(), "2.43.0".to_string());
    tools.insert("node".to_string(), "20.11.1".to_string());
    let i = Info {
        os: "linux".into(),
        os_version: "Ubuntu 24.04".into(),
        arch: "x86_64".into(),
        hostname: "box".into(),
        cpu: Cpu { model: "Ryzen".into(), cores: 8, threads: 16 },
        ram_mb: 32768,
        ram_free_mb: 1000,
        load: Some([0.5, 0.25, 0.1]),
        gpus: vec![
            Gpu { vendor: "nvidia".into(), name: "RTX 4090".into(), vram_mb: Some(24564) },
            Gpu { vendor: "intel".into(), name: "UHD 770".into(), vram_mb: None },
        ],
        disk: Disk { path: "/home/a/crundi-work".into(), free_mb: 100, total_mb: 200 },
        work_dir: "/home/a/crundi-work".into(),
        on_battery: None,
        idle_sec: None,
        tools,
        allow: Allow { chats: true, commands: false },
        paused: false,
    };
    let v = serde_json::to_value(&i).unwrap();
    assert_eq!(
        keys(&v),
        [
            "allow", "arch", "cpu", "disk", "gpus", "hostname", "idleSec", "load", "onBattery", "os", "osVersion",
            "paused", "ramFreeMb", "ramMb", "tools", "workDir"
        ]
    );
    assert_eq!(v["cpu"], json!({"model": "Ryzen", "cores": 8, "threads": 16}));
    assert_eq!(v["gpus"][0], json!({"vendor": "nvidia", "name": "RTX 4090", "vramMb": 24564}));
    assert_eq!(v["gpus"][1], json!({"vendor": "intel", "name": "UHD 770"}));
    assert_eq!(v["disk"], json!({"path": "/home/a/crundi-work", "freeMb": 100, "totalMb": 200}));
    assert_eq!(v["load"], json!([0.5, 0.25, 0.1]));
    assert_eq!(v["onBattery"], Value::Null);
    assert_eq!(v["idleSec"], Value::Null);
    assert_eq!(v["tools"], json!({"git": "2.43.0", "node": "20.11.1"}));
    assert_eq!(v["allow"], json!({"chats": true, "commands": false}));
    assert_eq!(v["ramMb"], json!(32768));
}

#[tokio::test]
async fn info_gathers_on_this_machine() {
    let dir = tmp("info");
    let i = info::gather(InfoInputs { work_dir: dir.clone(), allow_chats: true, allow_commands: true, paused: true }).await;
    assert_eq!(i.os, std::env::consts::OS);
    assert!(i.ram_mb > 0);
    assert!(i.cpu.threads > 0);
    assert!(i.disk.total_mb > 0, "disk of the work folder: {:?}", i.disk);
    assert!(i.paused);
    if cfg!(unix) {
        assert!(i.load.is_some());
    }
    // The test machine builds Rust, so cargo is found with a version.
    assert!(i.tools.get("cargo").is_some_and(|v| v.contains('.')) || crundi_worker::util::find_program("cargo").is_none());
    std::fs::remove_dir_all(dir).ok();
}

/// A one-request HTTP server: returns (base url, receiver of (path, body)).
fn http_once(status: &'static str, reply: Value) -> (String, std::sync::mpsc::Receiver<(String, Value)>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        let mut r = BufReader::new(&s);
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        let path = line.split_whitespace().nth(1).unwrap().to_string();
        let mut len = 0;
        loop {
            let mut h = String::new();
            r.read_line(&mut h).unwrap();
            if h == "\r\n" {
                break;
            }
            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; len];
        r.read_exact(&mut body).unwrap();
        tx.send((path, serde_json::from_slice(&body).unwrap())).unwrap();
        let text = reply.to_string();
        let mut w = &s;
        write!(w, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).unwrap();
    });
    (format!("http://127.0.0.1:{port}"), rx)
}

#[test]
fn join_saves_the_key() {
    let dir = tmp("join");
    let path = dir.join("config.json");
    let (url, rx) = http_once(
        "200 OK",
        json!({"ok": true, "id": "w0123456789", "key": "cw_w0123456789.c2VjcmV0", "name": "Desk"}),
    );
    let cfg = join_and_save(
        &path,
        JoinArgs { server: &format!("{url}/"), code: "abcd-2345", name: Some("Desk"), work_dir: Some(dir.join("wk")), kind: "headless" },
    )
    .unwrap();
    let (p, body) = rx.recv().unwrap();
    assert_eq!(p, "/api/workers/join");
    assert_eq!(body["code"], "ABCD2345");
    assert_eq!(body["name"], "Desk");
    assert_eq!(body["kind"], "headless");
    assert_eq!(body["os"], std::env::consts::OS);
    assert_eq!(body["version"], crundi_worker::util::VERSION);
    assert!(body["deviceId"].as_str().is_some_and(|d| d.len() == 32));

    assert_eq!(cfg.id.as_deref(), Some("w0123456789"));
    let saved = Config::load(&path).unwrap();
    assert_eq!(saved.server.as_deref(), Some(url.as_str()));
    assert_eq!(saved.key.as_deref(), Some("cw_w0123456789.c2VjcmV0"));
    assert_eq!(saved.name.as_deref(), Some("Desk"));
    assert_eq!(saved.work_dir, dir.join("wk"));
    assert!(dir.join("wk").is_dir(), "the work folder is created");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn join_refused_reports_the_error() {
    let dir = tmp("join-bad");
    let path = dir.join("config.json");
    let (url, _rx) = http_once("400 Bad Request", json!({"ok": false, "error": "unknown or expired code"}));
    let err = join_and_save(
        &path,
        JoinArgs { server: &url, code: "ZZZZZZZZ", name: None, work_dir: Some(dir.join("wk")), kind: "tray" },
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("unknown or expired code"), "{err:#}");
    assert!(!Config::load(&path).unwrap().joined());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_second_worker_on_the_pc_exits_with_5() {
    let d = tmp("lock");
    let cfg = crundi_worker::config::Config {
        server: Some("http://127.0.0.1:9".into()),
        id: Some("w0123456789".into()),
        key: Some("cw_w0123456789.c2VjcmV0".into()),
        work_dir: d.join("work"),
        ..Default::default()
    };
    let path = d.join("config.json");
    cfg.save(&path).unwrap();
    let held = crundi_worker::lock::acquire(&d, "electron").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_crundi-worker"))
        .args(["--headless", "--config"])
        .arg(&path)
        .env_remove("CRUNDI_WORKER_RESTARTED")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5), "{}", String::from_utf8_lossy(&out.stderr));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("PC app's worker") && err.contains("only one worker per PC"), "{err}");
    drop(held);
    std::fs::remove_dir_all(&d).ok();
}
