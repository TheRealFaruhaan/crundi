//! The bundled server as Electron's startBot runs it (app/main.js 744-940):
//! `node --no-deprecation src/index.js` in the server folder, with an IPC
//! channel (Node's: NODE_CHANNEL_FD, JSON lines), the same environment
//! (DOTENV_PATH, DATA_DIR, CRUNDI_DEVICE_ID, CRUNDI_DEV, the login shell's
//! PATH, no NODE_OPTIONS), its output read for the port, the API key and
//! "Ready", its `restart-server` message, its exit (stopped or error), and
//! stopping it: the `shutdown` message, up to 6 s, then the whole process
//! tree. Nothing polls: the reader and waiter threads queue what they see
//! and wake the desktop's pump (`crundi_webview::wake`).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Electron's MAX_LOGS.
pub const MAX_LOGS: usize = 500;

/// What the threads saw, for the pump.
#[derive(Debug, Clone, PartialEq)]
pub enum Ev {
    Line(String),
    Ipc(serde_json::Value),
    Exit { run: u64, code: Option<i32>, signal: Option<i32> },
    SpawnFailed(String),
}

#[derive(Default)]
pub struct St {
    pub status: &'static str,
    pub logs: VecDeque<String>,
    pub pid: u32,
    pub run: u64,
    pub port: Option<u16>,
    pub key: String,
    pub user_stopped: bool,
    pub restart_requested: bool,
    pub quitting: bool,
    /// Electron's openRestoreForm: the next Ready opens the restore form.
    pub open_restore: bool,
    /// The IPC channel's writing end.
    ipc: Option<Box<dyn Write + Send>>,
    /// The process (its id) is gone: (run, exited) with a condvar, for stop's wait.
    exited: Option<Arc<(Mutex<bool>, Condvar)>>,
}

pub static ST: Mutex<Option<St>> = Mutex::new(None);
static INBOX: Mutex<Vec<Ev>> = Mutex::new(Vec::new());

pub fn with<R>(f: impl FnOnce(&mut St) -> R) -> R {
    let mut g = ST.lock().unwrap();
    if g.is_none() { *g = Some(St { status: "stopped", ..Default::default() }); }
    f(g.as_mut().unwrap())
}

fn post(e: Ev) { INBOX.lock().unwrap().push(e); crundi_webview::wake(); }
pub fn take() -> Vec<Ev> { std::mem::take(&mut *INBOX.lock().unwrap()) }
pub fn waiting() -> bool { !INBOX.lock().unwrap().is_empty() }

/// appendLog (also to stderr, the client's own log).
pub fn log(line: &str) {
    eprintln!("[server] {line}");
    // Electron's appendLog: every line into crundi.log as well.
    crundi_shell::lifecycle::log_line(line);
    with(|s| { s.logs.push_back(line.to_string()); while s.logs.len() > MAX_LOGS { s.logs.pop_front(); } });
}

/// What a line of the server's output says: (port, key, ready).
pub fn parse_line(line: &str) -> (Option<u16>, Option<String>, bool) {
    // "[webapp] HTTP server on 0.0.0.0:<port>" (127.0.0.1 in dev: the port either way).
    let port = line.find("HTTP server on ").and_then(|i| {
        let rest = &line[i + 15..];
        rest.rsplit_once(':').and_then(|(_, p)| p.trim().split(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse().ok()))
    });
    let key = line.find("[crundi] API_KEY=").map(|i| line[i + 17..].chars().take_while(|c| c.is_ascii_hexdigit()).collect::<String>()).filter(|k| !k.is_empty());
    (port, key, line.contains("[crundi] Ready"))
}

/// The login shell's PATH (loginShellPath): asked once, marked on both
/// sides, 5 s at most. Linux and macOS only.
pub fn login_shell_path() -> String {
    static CACHE: Mutex<Option<String>> = Mutex::new(None);
    if cfg!(windows) { return String::new(); }
    if let Some(p) = CACHE.lock().unwrap().clone() { return p; }
    let sh = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/bash".into());
    let out = Command::new(&sh).args(["-ilc", "printf \"__CRUNDI__%s__CRUNDI__\" \"$PATH\""])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok().and_then(|mut c| {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match c.try_wait() {
                    Ok(Some(_)) => break c.wait_with_output().ok(),
                    Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
                    _ => { let _ = c.kill(); let _ = c.wait(); log("[electron] Could not ask the login shell for its PATH: timed out"); break None; }
                }
            }
        });
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let p = text.split("__CRUNDI__").nth(1).map(|s| s.trim().to_string()).unwrap_or_default();
    *CACHE.lock().unwrap() = Some(p.clone());
    p
}

/// serverPath: the login shell's PATH, then ours, then ~/.local/bin and
/// /usr/local/bin, then the bundled Node's folder last.
pub fn server_path(login: &str, ours: &str, home: &str, node_dir: Option<&str>) -> String {
    let mut parts: Vec<String> = vec![];
    let mut add = |list: &str| for d in list.split(':') { if !d.is_empty() && !parts.iter().any(|p| p == d) { parts.push(d.to_string()); } };
    add(login);
    add(ours);
    add(&format!("{home}/.local/bin:/usr/local/bin"));
    if let Some(n) = node_dir { add(n); }
    parts.join(":")
}

/// What the spawn is given (from mod.rs's paths).
pub struct Spawn {
    pub node: PathBuf,
    pub server_dir: PathBuf,
    pub env_path: PathBuf,
    pub data_dir: PathBuf,
    pub device_id: String,
    pub dev: bool,
    pub bundled_node_dir: Option<PathBuf>,
}

/// startBot's spawn (the caller has set the status to starting).
pub fn spawn(sp: Spawn) {
    std::thread::spawn(move || {
        let entry = sp.server_dir.join("src").join("index.js");
        log(&format!("[electron] node: {}", sp.node.display()));
        log(&format!("[electron] script: {}", entry.display()));
        log(&format!("[electron] exists: {}", entry.exists()));
        log(&format!("[electron] cwd: {}", sp.server_dir.display()));
        log(&format!("[electron] env: {}", sp.env_path.display()));
        let mut cmd = Command::new(&sp.node);
        cmd.arg("--no-deprecation").arg(&entry).current_dir(&sp.server_dir)
            .env("DOTENV_PATH", &sp.env_path).env("DATA_DIR", &sp.data_dir)
            // The server knows a desktop parent by this (its restart-server
            // message): not ELECTRON_RUN, which would send browser calls over
            // this channel rather than the native host's socket.
            .env("CRUNDI_NATIVE_RUN", "1")
            .env_remove("NODE_OPTIONS")
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        if !cfg!(windows) {
            let home = std::env::var("HOME").unwrap_or_default();
            let nd = sp.bundled_node_dir.as_ref().map(|p| p.to_string_lossy().to_string());
            cmd.env("PATH", server_path(&login_shell_path(), &std::env::var("PATH").unwrap_or_default(), &home, nd.as_deref()));
        }
        if sp.dev { cmd.env("CRUNDI_DEV", "1"); log("[electron] DEV mode: port 8889, Cloudflare disabled"); }
        if !sp.device_id.is_empty() { cmd.env("CRUNDI_DEVICE_ID", &sp.device_id); }
        let ipc = platform::prepare(&mut cmd);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => { log(&format!("[electron] Spawn failed: {e}")); post(Ev::SpawnFailed(e.to_string())); return; }
        };
        let pid = child.id();
        let writer = platform::after_spawn(ipc, pid);
        let exited = Arc::new((Mutex::new(false), Condvar::new()));
        let run = with(|s| { s.run += 1; s.pid = pid; s.ipc = writer.as_ref().map(|w| w.0.try_clone_box()).flatten(); s.exited = Some(exited.clone()); s.run });
        log(&format!("[electron] Process spawned, pid={pid}"));
        // The IPC channel's messages (restart-server…).
        if let Some((_, reader)) = writer {
            std::thread::spawn(move || for line in BufReader::new(reader).lines() {
                let Ok(l) = line else { break };
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&l) { post(Ev::Ipc(v)); }
            });
        }
        if let Some(out) = child.stdout.take() {
            std::thread::spawn(move || for line in BufReader::new(out).lines() { let Ok(l) = line else { break }; if !l.is_empty() { post(Ev::Line(l)); } });
        }
        if let Some(err) = child.stderr.take() {
            std::thread::spawn(move || for line in BufReader::new(err).lines() { let Ok(l) = line else { break }; if !l.is_empty() { post(Ev::Line(format!("[err] {l}"))); } });
        }
        let st = child.wait();
        let (code, signal) = match st {
            Ok(s) => (s.code(), platform::signal_of(&s)),
            Err(_) => (None, None),
        };
        { let (m, c) = &*exited; *m.lock().unwrap() = true; c.notify_all(); }
        post(Ev::Exit { run, code, signal });
    });
}

/// Send a message on the IPC channel (Node's JSON serialization: one line).
pub fn send(msg: &serde_json::Value) -> bool {
    with(|s| match s.ipc.as_mut() {
        Some(w) => w.write_all(format!("{msg}\n").as_bytes()).and_then(|_| w.flush()).is_ok(),
        None => false,
    })
}

/// stopServerProcess: the `shutdown` message, up to 6 s for it to exit,
/// then the whole tree killed (what it started, cloudflared, PTYs). Blocks
/// the calling thread.
pub fn stop_blocking(grace: Duration) {
    let (pid, exited) = with(|s| (s.pid, s.exited.clone()));
    if pid == 0 { return; }
    // The tree as it is now: once the server is gone its children are
    // re-parented and could no longer be found from it.
    let tree = platform::descendants(pid);
    if !send(&serde_json::json!("shutdown")) { platform::terminate(pid); }
    if let Some(e) = exited {
        let (m, c) = &*e;
        let g = m.lock().unwrap();
        let _ = c.wait_timeout_while(g, grace, |done| !*done);
    }
    platform::kill_tree(pid, &tree);
    with(|s| { if s.pid == pid { s.pid = 0; s.ipc = None; } });
}

/// Ask it to go (the restart-server message: Electron's botProcess.kill()).
pub fn terminate() { let pid = with(|s| s.pid); if pid != 0 { platform::terminate(pid); } }

/// Is this process still there?
pub fn alive(pid: u32) -> bool { platform::alive(pid) }

trait CloneBox { fn try_clone_box(&self) -> Option<Box<dyn Write + Send>>; }

#[cfg(unix)]
mod platform {
    use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
    use std::os::unix::net::UnixStream;
    use std::os::unix::process::{CommandExt, ExitStatusExt};

    /// The channel's fd in the child (Node reads NODE_CHANNEL_FD).
    const FD: i32 = 3;

    pub struct Ipc(pub UnixStream);
    impl super::CloneBox for Ipc {
        fn try_clone_box(&self) -> Option<Box<dyn std::io::Write + Send>> { self.0.try_clone().ok().map(|s| Box::new(s) as Box<dyn std::io::Write + Send>) }
    }

    /// The socket pair, its child end at fd 3, and the child in a process
    /// group of its own (so the group goes with it).
    pub fn prepare(cmd: &mut std::process::Command) -> Option<UnixStream> {
        let (ours, theirs) = UnixStream::pair().ok()?;
        let raw = theirs.into_raw_fd();
        cmd.env("NODE_CHANNEL_FD", FD.to_string()).env("NODE_CHANNEL_SERIALIZATION_MODE", "json");
        cmd.process_group(0);
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                if raw == FD {
                    let fl = libc::fcntl(FD, libc::F_GETFD);
                    if fl < 0 || libc::fcntl(FD, libc::F_SETFD, fl & !libc::FD_CLOEXEC) < 0 { return Err(std::io::Error::last_os_error()); }
                } else if libc::dup2(raw, FD) < 0 { return Err(std::io::Error::last_os_error()); }
                Ok(())
            });
        }
        // Our copy of the child's end is closed once it is spawned (after_spawn).
        CHILD_END.with(|c| c.set(raw));
        Some(ours)
    }

    thread_local! { static CHILD_END: std::cell::Cell<i32> = const { std::cell::Cell::new(-1) }; }

    pub fn after_spawn(ours: Option<UnixStream>, _pid: u32) -> Option<(Ipc, UnixStream)> {
        let raw = CHILD_END.with(|c| c.replace(-1));
        // SAFETY: the fd is ours, made by prepare, closed once.
        if raw >= 0 { unsafe { drop(std::fs::File::from_raw_fd(raw)); } }
        let s = ours?;
        let r = s.try_clone().ok()?;
        let _ = s.as_raw_fd();
        Some((Ipc(s), r))
    }

    pub fn signal_of(s: &std::process::ExitStatus) -> Option<i32> { s.signal() }

    pub fn terminate(pid: u32) { unsafe { libc::kill(pid as i32, libc::SIGTERM); } }

    pub fn alive(pid: u32) -> bool {
        if pid == 0 { return false; }
        // A zombie is gone for our purposes.
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(s) => s.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().next()).is_some_and(|st| st != "Z"),
            Err(_) => unsafe { libc::kill(pid as i32, 0) == 0 },
        }
    }

    /// Every process under `pid` (Linux: /proc; elsewhere none).
    pub fn descendants(pid: u32) -> Vec<u32> {
        let mut kids: Vec<(u32, u32)> = vec![];
        if let Ok(rd) = std::fs::read_dir("/proc") {
            for e in rd.flatten() {
                let Ok(p) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
                let Ok(st) = std::fs::read_to_string(format!("/proc/{p}/stat")) else { continue };
                let Some((_, rest)) = st.rsplit_once(')') else { continue };
                let ppid = rest.split_whitespace().nth(1).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
                kids.push((p, ppid));
            }
        }
        let mut out = vec![];
        let mut todo = vec![pid];
        while let Some(p) = todo.pop() {
            for (c, pp) in &kids { if *pp == p && !out.contains(c) { out.push(*c); todo.push(*c); } }
        }
        out
    }

    /// killTree: the group (kill -pgid), the server, and what was under it.
    pub fn kill_tree(pid: u32, tree: &[u32]) {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
            libc::kill(pid as i32, libc::SIGKILL);
            for p in tree { libc::kill(*p as i32, libc::SIGKILL); }
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;

    pub struct Ipc(pub std::fs::File);
    impl super::CloneBox for Ipc {
        fn try_clone_box(&self) -> Option<Box<dyn std::io::Write + Send>> { self.0.try_clone().ok().map(|s| Box::new(s) as Box<dyn std::io::Write + Send>) }
    }

    /// No window (windowsHide). Node's IPC pipe on Windows is not given yet:
    /// stopping goes straight to the tree (taskkill /T /F).
    pub fn prepare(cmd: &mut std::process::Command) -> Option<()> {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
        None
    }
    pub fn after_spawn(_: Option<()>, _pid: u32) -> Option<(Ipc, std::fs::File)> { None }
    pub fn signal_of(_: &std::process::ExitStatus) -> Option<i32> { None }
    pub fn terminate(pid: u32) { kill_tree(pid, &[]); }
    pub fn alive(pid: u32) -> bool {
        std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).creation_flags(0x0800_0000).output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
    }
    pub fn descendants(_pid: u32) -> Vec<u32> { vec![] }
    pub fn kill_tree(pid: u32, _tree: &[u32]) {
        let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).creation_flags(0x0800_0000).output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_port_the_key_and_ready() {
        assert_eq!(parse_line("[webapp] HTTP server on 0.0.0.0:8888"), (Some(8888), None, false));
        assert_eq!(parse_line("[webapp] HTTP server on 127.0.0.1:4801"), (Some(4801), None, false));
        assert_eq!(parse_line("[crundi] API_KEY=0a1b2c"), (None, Some("0a1b2c".into()), false));
        assert_eq!(parse_line("[crundi] Ready!"), (None, None, true));
        assert_eq!(parse_line("something else"), (None, None, false));
    }

    #[test]
    fn the_path_as_server_path_builds_it() {
        let p = server_path("/a:/b", "/b:/usr/bin", "/home/x", Some("/opt/Crundi/node/bin"));
        assert_eq!(p, "/a:/b:/usr/bin:/home/x/.local/bin:/usr/local/bin:/opt/Crundi/node/bin");
    }

    #[cfg(target_os = "linux")] // the tree from /proc (macOS, out of scope, has none)
    #[test]
    fn a_tree_is_found_and_killed() {
        let mut c = Command::new("sh").args(["-c", "sleep 30 & sleep 30 & wait"]).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let pid = c.id();
        let tree = platform::descendants(pid);
        assert!(tree.len() >= 2, "{tree:?}");
        platform::kill_tree(pid, &tree);
        let _ = c.wait();
        std::thread::sleep(Duration::from_millis(200));
        assert!(tree.iter().all(|p| !alive(*p)));
    }
}
