//! Running the link headless (logs to stderr) or embedded (JSON status lines
//! on stdout for the PC client; stdin closing means exit).

use crate::config::Config;
use crate::link::{self, Control, LinkOptions, Outcome, State, Status};
use serde_json::json;
use tokio::io::AsyncBufReadExt;

/// Exit codes.
pub const EXIT_OK: i32 = 0;
pub const EXIT_NOT_JOINED: i32 = 2;
/// The server refused the key (401) or removed/revoked this PC.
pub const EXIT_REMOVED: i32 = 3;
/// Another link for this worker took over.
pub const EXIT_REPLACED: i32 = 4;
/// Another worker already runs on this PC (the lock is taken).
pub const EXIT_CONFLICT: i32 = 5;

/// Take the one-worker-per-PC lock (waiting a little after an update's restart).
pub async fn take_lock(opts: &LinkOptions) -> Result<Option<crate::lock::WorkerLock>, String> {
    let Some(dir) = opts.lock_dir.clone() else { return Ok(None) };
    let kind = opts.kind.clone();
    let wait = crate::lock::start_wait();
    tokio::task::spawn_blocking(move || crate::lock::acquire_waiting(&dir, &kind, wait))
        .await
        .map_err(|e| e.to_string())?
        .map(Some)
        .map_err(|e| e.to_string())
}

pub fn exit_code(o: &Outcome) -> i32 {
    match o {
        Outcome::Quit => EXIT_OK,
        Outcome::NotJoined => EXIT_NOT_JOINED,
        Outcome::Unauthorized | Outcome::Removed(_) => EXIT_REMOVED,
        Outcome::Replaced => EXIT_REPLACED,
        Outcome::Updated(_) => EXIT_OK,
    }
}

fn log_status(s: &Status) {
    let mut line = format!("crundi-worker: {}", s.line());
    if let Some(e) = &s.error {
        if s.state == State::Offline || s.state == State::Unauthorized {
            line.push_str(&format!(" ({e})"));
        }
    }
    eprintln!("{line}");
}

/// Headless: run until Ctrl-C (or SIGTERM), 401 or bye.
pub async fn headless(cfg: Config, opts: LinkOptions) -> i32 {
    if !cfg.joined() {
        eprintln!("crundi-worker: not joined. Run: crundi-worker join <server> <code>");
        return EXIT_NOT_JOINED;
    }
    eprintln!(
        "crundi-worker {} ({}): {} as {}, work folder {}",
        crate::util::VERSION,
        opts.kind,
        cfg.server.as_deref().unwrap_or(""),
        cfg.display_name(),
        cfg.work_dir.display()
    );
    let _lock = match take_lock(&opts).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("crundi-worker: not starting: {e}");
            return EXIT_CONFLICT;
        }
    };
    let mut opts = opts;
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
    opts.events = Some(ev_tx);
    tokio::spawn(async move {
        while let Some(v) = ev_rx.recv().await {
            eprintln!("crundi-worker: {}", event_line(&v));
        }
    });
    let mut h = link::spawn(cfg, opts);
    let mut status = h.status.clone();
    tokio::spawn(async move {
        let mut last: Option<(State, bool, Option<String>)> = None;
        while status.changed().await.is_ok() {
            let s = status.borrow().clone();
            let key = (s.state, s.paused, s.name.clone());
            if last.as_ref() != Some(&key) {
                log_status(&s);
                last = Some(key);
            }
        }
    });
    let ctrl = h.ctrl.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        eprintln!("crundi-worker: stopping");
        let _ = ctrl.send(Control::Quit);
    });
    let outcome = (&mut h.task).await.unwrap_or(Outcome::Quit);
    match &outcome {
        Outcome::Unauthorized | Outcome::Removed(_) => {
            eprintln!("crundi-worker: this PC was removed from the server; join again with a new code")
        }
        Outcome::Replaced => eprintln!("crundi-worker: another program took over this worker"),
        Outcome::Updated(v) => {
            eprintln!("crundi-worker: restarting as {v}");
            crate::update::restart();
        }
        _ => {}
    }
    exit_code(&outcome)
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => return std::future::pending().await,
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Embedded: stdout gets `{"event":"status",...}` lines; stdin takes
/// `{"type":"pause","paused":bool}` / `{"type":"quit"}` lines, and its
/// closing means exit.
pub async fn embedded(cfg: Config, opts: LinkOptions) -> i32 {
    let emit = |v: serde_json::Value| {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{v}");
        let _ = out.flush();
    };
    // Jobs and chats starting and ending, so the client can notify.
    let mut opts = opts;
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
    opts.events = Some(ev_tx);
    tokio::spawn(async move {
        while let Some(v) = ev_rx.recv().await {
            emit(v);
        }
    });
    // One worker per PC: while another holds the lock, say so and wait for
    // it (or for the client to tell us to quit).
    let (quit_tx, mut quit_rx) = tokio::sync::watch::channel(false);
    let (ctrl_fwd_tx, mut ctrl_fwd_rx) = tokio::sync::mpsc::unbounded_channel::<Control>();
    {
        let tx = ctrl_fwd_tx.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(l)) => {
                        let Ok(v) = serde_json::from_str::<serde_json::Value>(&l) else { continue };
                        match v.get("type").and_then(|t| t.as_str()) {
                            Some("pause") => {
                                let p = v.get("paused").and_then(|p| p.as_bool()).unwrap_or(true);
                                let _ = tx.send(Control::Pause(p));
                            }
                            Some("quit") => {
                                let _ = quit_tx.send(true);
                                let _ = tx.send(Control::Quit);
                                break;
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        let _ = quit_tx.send(true);
                        let _ = tx.send(Control::Quit);
                        break;
                    }
                }
            }
        });
    }
    let mut paused_while_waiting: Option<bool> = None;
    let _lock = loop {
        match take_lock(&opts).await {
            Ok(l) => break l,
            Err(e) => {
                let mut s = Status::from_config(&cfg, State::Conflict);
                s.error = Some(e.clone());
                let mut v = status_event(&s);
                v["state"] = json!("conflict");
                v["detail"] = json!(e);
                emit(v);
                let wait = tokio::time::sleep(std::time::Duration::from_secs(10));
                tokio::pin!(wait);
                loop {
                    tokio::select! {
                        _ = &mut wait => break,
                        _ = quit_rx.changed() => {
                            emit(json!({"event": "log", "text": "worker stopped (another worker runs on this PC)", "exitCode": EXIT_CONFLICT}));
                            return EXIT_CONFLICT;
                        }
                        c = ctrl_fwd_rx.recv() => {
                            if let Some(Control::Pause(p)) = c {
                                paused_while_waiting = Some(p);
                            }
                        }
                    }
                }
            }
        }
    };
    let mut cfg = cfg;
    if let Some(p) = paused_while_waiting {
        cfg.paused = p;
    }
    let mut h = link::spawn(cfg, opts);
    let mut status = h.status.clone();
    {
        let s = status.borrow().clone();
        emit(status_event(&s));
    }
    tokio::spawn(async move {
        while status.changed().await.is_ok() {
            let s = status.borrow().clone();
            emit(status_event(&s));
        }
    });
    let ctrl = h.ctrl.clone();
    drop(ctrl_fwd_tx);
    tokio::spawn(async move {
        while let Some(c) = ctrl_fwd_rx.recv().await {
            let _ = ctrl.send(c);
        }
    });
    let outcome = (&mut h.task).await.unwrap_or(Outcome::Quit);
    let code = exit_code(&outcome);
    let text = match &outcome {
        Outcome::Unauthorized | Outcome::Removed(_) => "this PC was removed from the server".to_string(),
        Outcome::Replaced => "another program took over this worker".to_string(),
        o => format!("worker stopped ({o:?})"),
    };
    emit(json!({"event": "log", "text": text, "exitCode": code}));
    code
}

/// A job or chat event as one line of text (headless log, notifications).
pub fn event_line(v: &serde_json::Value) -> String {
    let g = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let project = g("project");
    let in_project = if project.is_empty() { String::new() } else { format!(" in {project}") };
    match (g("event").as_str(), g("state").as_str()) {
        ("job", "started") => format!("job {} started{in_project}: {}", g("job"), g("command")),
        ("job", _) => match v.get("exitCode").and_then(|c| c.as_i64()) {
            Some(c) => format!("job {} ended{in_project} (exit {c})", g("job")),
            None => format!("job {} ended{in_project}", g("job")),
        },
        ("chat", st) => {
            let what = match g("mode").as_str() {
                "terminal" => "Claude terminal",
                "shell" => "shell",
                _ => "chat",
            };
            if st == "started" {
                format!("{what} {} started on this PC{in_project}", g("chat"))
            } else {
                format!("{what} {} ended{in_project}", g("chat"))
            }
        }
        _ => v.to_string(),
    }
}

fn status_event(s: &Status) -> serde_json::Value {
    let mut v = serde_json::to_value(s).unwrap_or_default();
    v["event"] = json!("status");
    // The PC client shows "paused" as its own state (online stays true).
    if s.state == State::Online && s.paused {
        v["state"] = json!("paused");
    }
    v["detail"] = json!(match &s.error {
        Some(e) if s.state != State::Online => format!("{} ({e})", s.line()),
        _ => s.line(),
    });
    v["line"] = json!(s.line());
    v
}
