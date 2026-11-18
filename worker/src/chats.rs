//! Phase 4: whole chats on a PC (spec: "Phase 4: whole chats on a PC").
//!
//! `chat.spawn` prepares the folder the way a job does (git: the project's
//! clone plus a worktree `<project>@<chat>` on branch `pc/<chat>`, made once
//! and kept; copy: `<project>@copy`), writes an MCP config naming this
//! executable (`crundi-worker mcp ...`) and starts the PC's own `claude` with
//! its three pipes. stdout is spooled to `<workDir>/.crundi-chats/<chat>.out`
//! so a chat outlives the link and `chat.attach {from}` resends it; stderr is
//! sent as it comes. Each chat has a loopback relay (127.0.0.1:0, a token)
//! the `mcp` subcommand talks to; its requests go over the link as
//! `mcp.list` / `mcp.call` and wait up to 10 minutes for the server.

use crate::jobfiles;
use crate::jobs::{self, emit, ensure_commit, git_raw, prepare_copy, valid_name, Events, Outbox, Prep, CHUNK};
use crate::util::{find_in_path, is_executable, random_hex};
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, watch};

const OUT_CHUNK: usize = 64 * 1024;
const COALESCE: Duration = Duration::from_millis(20);
/// A finished chat whose exit the server never collected is forgotten after an hour.
const KEEP_FINISHED: Duration = Duration::from_secs(3600);
/// close: SIGTERM, then a kill this long after.
const KILL_GRACE: Duration = Duration::from_secs(5);
const DRAIN_AFTER_EXIT: Duration = Duration::from_secs(2);
/// An MCP request waits this long for the server (across reconnects).
pub const WAIT_MCP: Duration = Duration::from_secs(600);
pub const NOT_REACHABLE: &str = "the server is not reachable";

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// The refusal when this PC has no Claude Code (spec text).
pub fn install_text(pc: &str) -> String {
    format!(
        "Install Claude Code on {pc}: curl -fsSL https://claude.ai/install.sh | bash \
         (Windows: irm https://claude.ai/install.ps1 | iex), then run claude once to sign in"
    )
}

/// Where `claude` may be: PATH, then the usual install places.
pub fn claude_candidates(home: Option<&Path>, appdata: Option<&Path>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = home {
        if cfg!(windows) {
            v.push(h.join(".local").join("bin").join("claude.exe"));
            v.push(h.join(".claude").join("local").join("claude.exe"));
        } else {
            v.push(h.join(".local/bin/claude"));
            v.push(h.join(".claude/local/claude"));
            v.push(h.join(".npm-global/bin/claude"));
            v.push(h.join(".bun/bin/claude"));
        }
    }
    if cfg!(windows) {
        if let Some(a) = appdata {
            v.push(a.join("npm").join("claude.cmd"));
        }
    } else {
        v.push(PathBuf::from("/usr/local/bin/claude"));
        v.push(PathBuf::from("/opt/homebrew/bin/claude"));
    }
    v
}

/// Resolve `claude` from a PATH value and a home folder (testable).
pub fn find_claude_in(path: Option<&std::ffi::OsStr>, home: Option<&Path>, appdata: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = path.and_then(|p| find_in_path("claude", p)) {
        return Some(p);
    }
    claude_candidates(home, appdata).into_iter().find(|p| is_executable(p))
}

/// Resolve `claude` on this PC.
pub fn find_claude() -> Option<PathBuf> {
    let path = std::env::var_os("PATH");
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    find_claude_in(path.as_deref(), dirs::home_dir().as_deref(), appdata.as_deref())
}

#[derive(Debug, Clone, Copy)]
pub struct ChatGate {
    pub paused: bool,
    pub allow_chats: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kill {
    Interrupt,
    Terminate,
    Kill,
}

struct ChatSt {
    out_bytes: u64,
    attached: bool,
    sent_off: u64,
    gen: u64,
    /// The `chat.exit` message, once the process has ended.
    exit: Option<Value>,
    exited_at: Option<Instant>,
    /// A newer spawn of the same chat took over: this one's streamer stops.
    superseded: bool,
}

struct ChatCell {
    id: String,
    project: String,
    out_path: PathBuf,
    mcp_path: PathBuf,
    spool: Mutex<Option<std::fs::File>>,
    st: Mutex<ChatSt>,
    tick: watch::Sender<u64>,
    /// Bytes for claude's stdin; None closes it.
    stdin: mpsc::UnboundedSender<Option<Vec<u8>>>,
    kill: mpsc::UnboundedSender<Kill>,
    /// `chat.bundle` / `chat.files` from the server while preparing.
    inbox: mpsc::UnboundedSender<Value>,
    pid: AtomicU32,
    stderr_off: AtomicU64,
    relay: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// What preparing printed (kept for a refusal's error).
    prep_log: Mutex<Vec<u8>>,
}

impl ChatCell {
    fn bump(&self) {
        self.tick.send_modify(|v| *v = v.wrapping_add(1));
    }

    fn append(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let mut f = self.spool.lock().unwrap();
        if let Some(f) = f.as_mut() {
            if f.write_all(data).is_ok() {
                self.st.lock().unwrap().out_bytes += data.len() as u64;
            }
        }
        drop(f);
        self.bump();
    }

    fn running(&self) -> bool {
        self.st.lock().unwrap().exit.is_none()
    }
}

struct Pending {
    msg: Value,
    tx: oneshot::Sender<Value>,
}

struct Inner {
    work_dir: PathBuf,
    pc_name: String,
    /// Tests: use exactly this program as claude (missing file = not installed).
    claude: Option<PathBuf>,
    events: Events,
    chats: Mutex<HashMap<String, Arc<ChatCell>>>,
    out: Outbox,
    pending: Mutex<HashMap<String, Pending>>,
}

#[derive(Clone)]
pub struct Chats {
    inner: Arc<Inner>,
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

impl Chats {
    pub fn new(work_dir: PathBuf, pc_name: String, claude: Option<PathBuf>, events: Events) -> Chats {
        Chats {
            inner: Arc::new(Inner {
                work_dir,
                pc_name,
                claude,
                events,
                chats: Default::default(),
                out: Outbox::default(),
                pending: Default::default(),
            }),
        }
    }

    pub fn chats_dir(&self) -> PathBuf {
        self.inner.work_dir.join(".crundi-chats")
    }

    /// A link whose server lists "chats" was welcomed. MCP requests still
    /// waiting (their link dropped) are sent again on it.
    pub fn connect(&self, tx: mpsc::Sender<String>) {
        self.inner.out.set(Some(tx));
        let msgs: Vec<Value> = self.inner.pending.lock().unwrap().values().map(|p| p.msg.clone()).collect();
        let out = self.inner.out.clone();
        tokio::spawn(async move {
            for m in msgs {
                out.send(m).await;
            }
        });
    }

    pub fn disconnect(&self) {
        self.inner.out.set(None);
        for c in self.inner.chats.lock().unwrap().values() {
            let mut st = c.st.lock().unwrap();
            st.attached = false;
            st.gen += 1;
            drop(st);
            c.bump();
        }
    }

    /// For hello: `[{ chat, running, outBytes }]`.
    pub fn summary(&self) -> Vec<Value> {
        self.prune();
        let chats = self.inner.chats.lock().unwrap();
        let mut v: Vec<Value> = chats
            .values()
            .map(|c| {
                let st = c.st.lock().unwrap();
                json!({"chat": c.id, "running": st.exit.is_none(), "outBytes": st.out_bytes})
            })
            .collect();
        v.sort_by(|a, b| a["chat"].as_str().cmp(&b["chat"].as_str()));
        v
    }

    pub fn prune(&self) {
        let gone: Vec<Arc<ChatCell>> = {
            let chats = self.inner.chats.lock().unwrap();
            chats
                .values()
                .filter(|c| c.st.lock().unwrap().exited_at.is_some_and(|t| t.elapsed() >= KEEP_FINISHED))
                .cloned()
                .collect()
        };
        for c in gone {
            forget(&self.inner, &c);
        }
    }

    /// The worker is quitting: stop every chat's claude.
    pub fn kill_all(&self) {
        for c in self.inner.chats.lock().unwrap().values() {
            let pid = c.pid.load(Ordering::SeqCst);
            if pid != 0 && c.running() {
                signal(pid, Kill::Terminate);
            }
        }
    }

    pub fn handle(&self, v: &Value, gate: ChatGate) {
        let chat = s(v, "chat").to_string();
        let cell = || self.inner.chats.lock().unwrap().get(&chat).cloned();
        match s(v, "type") {
            "chat.spawn" => self.spawn(v, gate),
            "chat.in" => {
                if let Some(c) = cell() {
                    if let Ok(b) = B64.decode(s(v, "b64")) {
                        if !b.is_empty() {
                            let _ = c.stdin.send(Some(b));
                        }
                    }
                    if v.get("eof").and_then(Value::as_bool) == Some(true) {
                        let _ = c.stdin.send(None);
                    }
                }
            }
            "chat.kill" => {
                if let Some(c) = cell() {
                    let k = match s(v, "signal").to_ascii_uppercase().as_str() {
                        "SIGINT" | "INT" | "INTERRUPT" => Kill::Interrupt,
                        "SIGKILL" | "KILL" => Kill::Kill,
                        _ => Kill::Terminate,
                    };
                    let _ = c.kill.send(k);
                }
            }
            "chat.attach" => {
                let from = v.get("from").and_then(Value::as_u64).unwrap_or(0);
                match cell() {
                    Some(c) => {
                        let mut st = c.st.lock().unwrap();
                        st.sent_off = from.min(st.out_bytes);
                        st.attached = true;
                        st.gen += 1;
                        drop(st);
                        c.bump();
                    }
                    None => self.inner.out.send_now(json!({"type": "chat.exit", "chat": chat, "code": -1, "unknown": true})),
                }
            }
            "chat.bundle" | "chat.files" => {
                if let Some(c) = cell() {
                    let _ = c.inbox.send(v.clone());
                }
            }
            "chat.pull" => {
                let inner = self.inner.clone();
                let v = v.clone();
                tokio::spawn(async move { pull(&inner, &v).await });
            }
            "mcp.tools" | "mcp.result" => {
                let req = s(v, "req");
                let p = self.inner.pending.lock().unwrap().remove(req);
                if let Some(p) = p {
                    let answer = if s(v, "type") == "mcp.tools" {
                        json!({"tools": v.get("tools").cloned().unwrap_or(json!([]))})
                    } else {
                        match v.get("result") {
                            Some(r) if !r.is_null() => r.clone(),
                            _ => json!({"ok": false, "error": v.get("error").cloned().unwrap_or(json!("no result"))}),
                        }
                    };
                    let _ = p.tx.send(answer);
                }
            }
            _ => {}
        }
    }

    fn find_claude(&self) -> Option<PathBuf> {
        match &self.inner.claude {
            Some(p) => p.is_file().then(|| p.clone()),
            None => find_claude(),
        }
    }

    fn spawn(&self, v: &Value, gate: ChatGate) {
        let chat = s(v, "chat").to_string();
        let out = self.inner.out.clone();
        let refuse = |e: &str| out.send_now(json!({"type": "chat.refused", "chat": chat, "error": e}));
        if !valid_name(&chat) {
            return refuse("bad chat id");
        }
        let project = s(v, "project");
        if !project.is_empty() && !valid_name(project) {
            return refuse("bad project name");
        }
        if gate.paused {
            return refuse("this PC is paused");
        }
        if !gate.allow_chats {
            return refuse("chats are not allowed on this PC");
        }
        let old = self.inner.chats.lock().unwrap().get(&chat).cloned();
        if let Some(old) = old {
            if old.running() {
                return refuse("this chat is already running on this PC");
            }
            old.st.lock().unwrap().superseded = true;
            old.bump();
            self.inner.chats.lock().unwrap().remove(&chat);
        }
        let Some(claude) = self.find_claude() else {
            return refuse(&install_text(&self.inner.pc_name));
        };
        let dir = self.chats_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return refuse(&format!("cannot create {}: {e}", dir.display()));
        }
        let out_path = dir.join(format!("{chat}.out"));
        let spool = match std::fs::File::create(&out_path) {
            Ok(f) => f,
            Err(e) => return refuse(&format!("cannot create the output spool: {e}")),
        };
        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
        let (kill_tx, kill_rx) = mpsc::unbounded_channel();
        let (inbox_tx, inbox_rx) = mpsc::unbounded_channel();
        let cell = Arc::new(ChatCell {
            id: chat.clone(),
            project: project.to_string(),
            out_path,
            mcp_path: dir.join(format!("{chat}.mcp.json")),
            spool: Mutex::new(Some(spool)),
            st: Mutex::new(ChatSt { out_bytes: 0, attached: true, sent_off: 0, gen: 0, exit: None, exited_at: None, superseded: false }),
            tick: watch::channel(0).0,
            stdin: stdin_tx,
            kill: kill_tx,
            inbox: inbox_tx,
            pid: AtomicU32::new(0),
            stderr_off: AtomicU64::new(0),
            relay: Mutex::new(None),
            prep_log: Mutex::new(Vec::new()),
        });
        self.inner.chats.lock().unwrap().insert(chat, cell.clone());
        let inner = self.inner.clone();
        let spec = v.clone();
        tokio::spawn(run_chat(inner, cell, spec, claude, inbox_rx, stdin_rx, kill_rx));
    }
}

/// Drop a chat: its record, spool, MCP config and relay (meta files stay for pulls).
fn forget(inner: &Inner, c: &Arc<ChatCell>) {
    {
        let mut chats = inner.chats.lock().unwrap();
        if chats.get(&c.id).is_some_and(|x| Arc::ptr_eq(x, c)) {
            chats.remove(&c.id);
        }
    }
    if let Some(h) = c.relay.lock().unwrap().take() {
        h.abort();
    }
    if !c.st.lock().unwrap().superseded {
        let _ = std::fs::remove_file(&c.out_path);
        let _ = std::fs::remove_file(&c.mcp_path);
    }
}

async fn send_if(inner: &Inner, cell: &ChatCell, gen: u64, v: Value) -> bool {
    {
        let st = cell.st.lock().unwrap();
        if !st.attached || st.gen != gen || st.superseded {
            return false;
        }
    }
    if inner.out.send(v).await {
        return true;
    }
    let mut st = cell.st.lock().unwrap();
    if st.gen == gen {
        st.attached = false;
    }
    false
}

/// The per-chat streamer: stdout from `sent_off`, then `chat.exit`.
async fn stream(inner: Arc<Inner>, cell: Arc<ChatCell>) {
    let mut rx = cell.tick.subscribe();
    loop {
        rx.borrow_and_update();
        let (attached, gen, off, out_bytes, exit, superseded) = {
            let st = cell.st.lock().unwrap();
            (st.attached, st.gen, st.sent_off, st.out_bytes, st.exit.clone(), st.superseded)
        };
        if superseded {
            return;
        }
        if !attached {
            if rx.changed().await.is_err() {
                return;
            }
            continue;
        }
        if off < out_bytes {
            let len = ((out_bytes - off) as usize).min(OUT_CHUNK);
            let Ok(data) = jobs::read_at(&cell.out_path, off, len) else { return };
            if data.is_empty() {
                continue;
            }
            let n = data.len() as u64;
            let msg = json!({"type": "chat.out", "chat": cell.id, "stream": "stdout", "off": off, "b64": B64.encode(&data)});
            if send_if(&inner, &cell, gen, msg).await {
                let mut st = cell.st.lock().unwrap();
                if st.gen == gen {
                    st.sent_off = off + n;
                }
            }
            continue;
        }
        if let Some(exit) = exit {
            if send_if(&inner, &cell, gen, exit).await {
                forget(&inner, &cell);
                return;
            }
            continue;
        }
        if rx.changed().await.is_err() {
            return;
        }
        tokio::time::sleep(COALESCE).await;
    }
}

fn signal_name(sig: i32) -> String {
    match sig {
        1 => "SIGHUP".into(),
        2 => "SIGINT".into(),
        3 => "SIGQUIT".into(),
        6 => "SIGABRT".into(),
        9 => "SIGKILL".into(),
        11 => "SIGSEGV".into(),
        13 => "SIGPIPE".into(),
        15 => "SIGTERM".into(),
        n => format!("SIG{n}"),
    }
}

#[cfg(windows)]
mod win {
    #[link(name = "kernel32")]
    extern "system" {
        pub fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> i32;
    }
}

/// Signal claude: interrupt = SIGINT (Windows: CTRL_BREAK to its group, else
/// it is stopped); terminate = SIGTERM (Windows: taskkill); kill = the tree.
fn signal(pid: u32, k: Kill) {
    #[cfg(unix)]
    unsafe {
        match k {
            Kill::Interrupt => {
                libc::kill(pid as i32, libc::SIGINT);
            }
            Kill::Terminate => {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            Kill::Kill => jobs::kill_tree(pid),
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        match k {
            Kill::Interrupt => {
                // Only reaches a process on our console; failing that, stop it
                // (what Node's kill('SIGINT') does on Windows).
                if unsafe { win::GenerateConsoleCtrlEvent(1, pid) } == 0 {
                    jobs::kill_tree(pid);
                }
            }
            Kill::Terminate => {
                let _ = std::process::Command::new("taskkill")
                    .args(["/T", "/PID", &pid.to_string()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .creation_flags(0x0800_0000)
                    .status();
            }
            Kill::Kill => jobs::kill_tree(pid),
        }
    }
}

fn meta_path(inner: &Inner, chat: &str) -> PathBuf {
    inner.work_dir.join(".crundi-chats").join(format!("{chat}.json"))
}

fn base_manifest_path(inner: &Inner, chat: &str) -> PathBuf {
    inner.work_dir.join(".crundi-chats").join(format!("{chat}.base.json"))
}

fn write_meta(inner: &Inner, chat: &str, meta: Value) -> Result<()> {
    let p = meta_path(inner, chat);
    std::fs::write(&p, serde_json::to_vec_pretty(&meta)?).with_context(|| format!("writing {}", p.display()))
}

/// Prepare the chat's folder; returns it.
async fn prepare_chat(inner: &Inner, cell: &ChatCell, spec: &Value, inbox: &mut mpsc::UnboundedReceiver<Value>) -> Result<PathBuf> {
    let wd = &inner.work_dir;
    std::fs::create_dir_all(wd)?;
    let source = match s(spec, "source") {
        "" => "none",
        x => x,
    };
    let project = cell.project.as_str();
    let chat = cell.id.as_str();
    let log = |b: &[u8]| {
        let mut l = cell.prep_log.lock().unwrap();
        l.extend_from_slice(b);
        if l.len() > 16 * 1024 {
            let cut = l.len() - 16 * 1024;
            l.drain(..cut);
        }
    };
    let p = Prep { log: &log, out: &inner.out, kind: "chat", id: chat, tmp_dir: wd.join(".crundi-chats") };
    match source {
        "git" => {
            if project.is_empty() {
                bail!("a git chat needs a project");
            }
            let clone = wd.join(project);
            let wt = wd.join(format!("{project}@{chat}"));
            let branch = format!("pc/{chat}");
            if wt.join(".git").exists() {
                // Resumed: the chat's folder and branch as it left them.
                if !meta_path(inner, chat).exists() {
                    write_meta(inner, chat, json!({"project": project, "source": "git", "dir": wt, "branch": branch}))?;
                }
                return Ok(wt);
            }
            let commit = ensure_commit(&p, spec, &clone, inbox).await?;
            let _ = jobs::git(&p, &clone, &["worktree", "prune"], true).await;
            let wts = wt.to_string_lossy().into_owned();
            let have_branch = jobs::git(&p, &clone, &["rev-parse", "--verify", "-q", &format!("refs/heads/{branch}")], true)
                .await
                .is_ok_and(|(ok, _)| ok);
            let (ok, _) = if have_branch {
                jobs::git(&p, &clone, &["worktree", "add", &wts, &branch], false).await?
            } else {
                jobs::git(&p, &clone, &["worktree", "add", "-b", &branch, &wts, &commit], false).await?
            };
            if !ok {
                bail!("git worktree add failed");
            }
            write_meta(inner, chat, json!({"project": project, "source": "git", "dir": wt, "branch": branch, "base": commit}))?;
            Ok(wt)
        }
        "copy" => {
            if project.is_empty() {
                bail!("a copy chat needs a project");
            }
            let dir = wd.join(format!("{project}@copy"));
            let base = base_manifest_path(inner, chat);
            if !base.exists() {
                prepare_copy(&p, &dir, inbox).await?;
                // What the copy held when the chat began: pulls send what changed since.
                let d = dir.clone();
                let m = tokio::task::spawn_blocking(move || jobfiles::manifest(&d)).await??;
                std::fs::write(&base, serde_json::to_vec(&m)?)?;
            }
            write_meta(inner, chat, json!({"project": project, "source": "copy", "dir": dir}))?;
            Ok(dir)
        }
        "none" => {
            let dir = if project.is_empty() { wd.clone() } else { wd.join(project) };
            let dir = if dir.is_dir() { dir } else { wd.clone() };
            write_meta(inner, chat, json!({"project": project, "source": "none", "dir": dir}))?;
            Ok(dir)
        }
        other => bail!("unknown source {other}"),
    }
}

/// Drop any `--mcp-config <x>` the server put in (its paths mean nothing here).
fn claude_args(spec: &Value, mcp: &Path) -> Vec<String> {
    let raw: Vec<String> = spec
        .get("args")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut it = raw.into_iter();
    while let Some(a) = it.next() {
        if a == "--mcp-config" {
            it.next();
            continue;
        }
        if a.starts_with("--mcp-config=") {
            continue;
        }
        out.push(a);
    }
    // Last, since --mcp-config takes several values.
    out.push("--mcp-config".into());
    out.push(mcp.to_string_lossy().into_owned());
    out
}

fn mcp_config(chat: &str, port: u16, token: &str) -> Value {
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "crundi-worker".into());
    json!({"mcpServers": {"crundi": {"command": exe,
        "args": ["mcp", "--chat", chat, "--port", port.to_string(), "--token", token]}}})
}

async fn run_chat(
    inner: Arc<Inner>,
    cell: Arc<ChatCell>,
    spec: Value,
    claude: PathBuf,
    mut inbox: mpsc::UnboundedReceiver<Value>,
    mut stdin_rx: mpsc::UnboundedReceiver<Option<Vec<u8>>>,
    mut kill_rx: mpsc::UnboundedReceiver<Kill>,
) {
    let refuse = |error: String| {
        let inner = inner.clone();
        let cell = cell.clone();
        async move {
            inner.out.send(json!({"type": "chat.refused", "chat": cell.id, "error": error})).await;
            // No streamer runs yet: just drop the record and its files.
            forget(&inner, &cell);
        }
    };
    let prepared = tokio::select! {
        r = prepare_chat(&inner, &cell, &spec, &mut inbox) => r,
        _ = kill_rx.recv() => Err(anyhow!("closed while preparing")),
    };
    let dir = match prepared.and_then(|d| jobs::resolve_cwd(&d, &spec)) {
        Ok(d) => d,
        Err(e) => {
            let log = String::from_utf8_lossy(&cell.prep_log.lock().unwrap()).into_owned();
            let tail: String = log.chars().rev().take(1500).collect::<Vec<_>>().into_iter().rev().collect();
            let mut msg = format!("preparing the folder failed: {e:#}");
            if !tail.trim().is_empty() {
                msg.push_str(&format!("\n{}", tail.trim_end()));
            }
            return refuse(msg).await;
        }
    };

    // The MCP relay for this chat, then the config naming it.
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", 0)).await {
        Ok(l) => l,
        Err(e) => return refuse(format!("cannot open the MCP relay: {e}")).await,
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let token = random_hex(16);
    *cell.relay.lock().unwrap() = Some(tokio::spawn(relay(inner.clone(), cell.id.clone(), token.clone(), listener)));
    let cfg = mcp_config(&cell.id, port, &token);
    if let Err(e) = std::fs::write(&cell.mcp_path, cfg.to_string()) {
        return refuse(format!("cannot write the MCP config: {e}")).await;
    }

    let mut c = crate::util::tokio_command(&claude);
    c.args(claude_args(&spec, &cell.mcp_path))
        .current_dir(&dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(env) = spec.get("env").and_then(Value::as_object) {
        for (k, v) in env {
            if let Some(v) = v.as_str() {
                if !k.is_empty() && !k.contains('=') {
                    c.env(k, v);
                }
            }
        }
    }
    #[cfg(unix)]
    c.process_group(0);
    #[cfg(windows)]
    c.creation_flags(0x0800_0000 | 0x0000_0200); // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP
    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return refuse(format!("could not start {}: {e}", claude.display())).await,
    };
    let pid = child.id().unwrap_or(0);
    cell.pid.store(pid, Ordering::SeqCst);
    inner.out.send(json!({"type": "chat.started", "chat": cell.id, "pid": pid})).await;
    emit(&inner.events, json!({"event": "chat", "state": "started", "chat": cell.id, "project": cell.project}));
    tokio::spawn(stream(inner.clone(), cell.clone()));

    // stdout -> spool
    let mut stdout = child.stdout.take().unwrap();
    let (eof_tx, eof_rx) = oneshot::channel::<()>();
    let ocell = cell.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => ocell.append(&buf[..n]),
            }
        }
        let _ = eof_tx.send(());
    });
    // stderr -> sent as it comes
    let mut stderr = child.stderr.take().unwrap();
    let (ecell, einner) = (cell.clone(), inner.clone());
    tokio::spawn(async move {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let off = ecell.stderr_off.fetch_add(n as u64, Ordering::SeqCst);
                    einner
                        .out
                        .send(json!({"type": "chat.out", "chat": ecell.id, "stream": "stderr", "off": off, "b64": B64.encode(&buf[..n])}))
                        .await;
                }
            }
        }
    });
    // chat.in -> stdin
    let mut stdin = child.stdin.take().unwrap();
    tokio::spawn(async move {
        while let Some(m) = stdin_rx.recv().await {
            let Some(b) = m else { break };
            if stdin.write_all(&b).await.is_err() || stdin.flush().await.is_err() {
                break;
            }
        }
    });

    let mut kill_at: Option<tokio::time::Instant> = None;
    let status = loop {
        let deadline = kill_at;
        tokio::select! {
            st = child.wait() => break st,
            k = kill_rx.recv() => {
                let Some(k) = k else { continue };
                signal(pid, k);
                if k == Kill::Terminate && kill_at.is_none() {
                    kill_at = Some(tokio::time::Instant::now() + KILL_GRACE);
                }
            }
            _ = async { tokio::time::sleep_until(deadline.unwrap()).await }, if deadline.is_some() => {
                signal(pid, Kill::Kill);
                kill_at = None;
            }
        }
    };
    let _ = tokio::time::timeout(DRAIN_AFTER_EXIT, eof_rx).await;
    let mut exit = json!({"type": "chat.exit", "chat": cell.id});
    match status {
        Ok(st) => {
            #[cfg(unix)]
            let sig = std::os::unix::process::ExitStatusExt::signal(&st);
            #[cfg(not(unix))]
            let sig: Option<i32> = None;
            let code = match (st.code(), sig) {
                (Some(c), _) => c as i64,
                (None, Some(s)) => 128 + s as i64,
                _ => -1,
            };
            exit["code"] = json!(code);
            if let Some(s) = sig {
                exit["signal"] = json!(signal_name(s));
            }
        }
        Err(e) => {
            exit["code"] = json!(-1);
            exit["error"] = json!(e.to_string());
        }
    }
    *cell.spool.lock().unwrap() = None;
    if let Some(h) = cell.relay.lock().unwrap().take() {
        h.abort();
    }
    {
        let mut st = cell.st.lock().unwrap();
        st.exit = Some(exit);
        st.exited_at = Some(Instant::now());
    }
    emit(&inner.events, json!({"event": "chat", "state": "ended", "chat": cell.id, "project": cell.project}));
    cell.bump();
}

// ---- the MCP relay (worker side) ----

/// One MCP request over the link; waits (across reconnects) up to WAIT_MCP.
async fn mcp_request(inner: &Inner, mut msg: Value) -> Result<Value, String> {
    let req = format!("m{}", random_hex(8));
    msg["req"] = json!(req);
    let (tx, rx) = oneshot::channel();
    inner.pending.lock().unwrap().insert(req.clone(), Pending { msg: msg.clone(), tx });
    if inner.out.connected() {
        inner.out.send(msg).await;
    }
    match tokio::time::timeout(WAIT_MCP, rx).await {
        Ok(Ok(v)) => Ok(v),
        _ => {
            inner.pending.lock().unwrap().remove(&req);
            Err(NOT_REACHABLE.into())
        }
    }
}

async fn relay(inner: Arc<Inner>, chat: String, token: String, listener: tokio::net::TcpListener) {
    loop {
        let Ok((sock, addr)) = listener.accept().await else { return };
        if !addr.ip().is_loopback() {
            continue;
        }
        tokio::spawn(relay_conn(inner.clone(), chat.clone(), token.clone(), sock));
    }
}

async fn relay_conn(inner: Arc<Inner>, chat: String, token: String, sock: tokio::net::TcpStream) {
    let (r, mut w) = sock.into_split();
    let mut lines = tokio::io::BufReader::new(r).lines();
    let first = match tokio::time::timeout(Duration::from_secs(10), lines.next_line()).await {
        Ok(Ok(Some(l))) => serde_json::from_str::<Value>(&l).unwrap_or(Value::Null),
        _ => return,
    };
    if s(&first, "token") != token || token.is_empty() {
        let _ = w.write_all(b"{\"ok\":false,\"error\":\"bad token\"}\n").await;
        return;
    }
    if w.write_all(b"{\"ok\":true}\n").await.is_err() {
        return;
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(l) = rx.recv().await {
            if w.write_all(format!("{l}\n").as_bytes()).await.is_err() {
                break;
            }
        }
    });
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let (inner, chat, tx) = (inner.clone(), chat.clone(), tx.clone());
        tokio::spawn(async move {
            let id = v.get("id").cloned().unwrap_or(Value::Null);
            let msg = match s(&v, "op") {
                "list" => json!({"type": "mcp.list", "chat": chat}),
                "call" => json!({"type": "mcp.call", "chat": chat, "tool": s(&v, "tool"),
                    "args": v.get("args").cloned().filter(|a| a.is_object()).unwrap_or(json!({}))}),
                op => {
                    let _ = tx.send(json!({"id": id, "error": format!("unknown op {op}")}).to_string());
                    return;
                }
            };
            let reply = match mcp_request(&inner, msg).await {
                Ok(r) => json!({"id": id, "result": r}),
                Err(e) => json!({"id": id, "error": e}),
            };
            let _ = tx.send(reply.to_string());
        });
    }
}

// ---- chat.pull ----

async fn pull(inner: &Inner, v: &Value) {
    let chat = s(v, "chat").to_string();
    let req = v.get("req").cloned().unwrap_or(Value::Null);
    let base = s(v, "base").to_string();
    let meta: Option<Value> = valid_name(&chat)
        .then(|| std::fs::read(meta_path(inner, &chat)).ok())
        .flatten()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let source = meta.as_ref().map(|m| s(m, "source").to_string()).unwrap_or_default();
    let dir = meta.as_ref().map(|m| PathBuf::from(s(m, "dir"))).unwrap_or_default();
    let res = match source.as_str() {
        "git" => pull_git(inner, &chat, &req, &base, &dir).await,
        "copy" => pull_copy(inner, &chat, &req, &dir).await,
        "" => Err(anyhow!("this PC does not know chat {chat}")),
        other => Err(anyhow!("a chat with source {other} has nothing to pull")),
    };
    if let Err(e) = res {
        let error = format!("{e:#}");
        if source == "copy" {
            inner.out.send(json!({"type": "chat.files", "chat": chat, "req": req, "seq": 0, "b64": "", "last": true, "removed": [], "error": error})).await;
        } else {
            inner.out.send(json!({"type": "chat.patch", "chat": chat, "req": req, "b64": "", "error": error})).await;
        }
    }
}

async fn git_ok(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let (ok, out, err) = git_raw(dir, args).await?;
    if !ok {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&err).trim());
    }
    Ok(out)
}

/// Send a file as numbered chunks; `extra` goes into every chunk, `last_extra` into the last.
async fn send_chunks(inner: &Inner, path: Option<&Path>, base: Value, last_extra: Value) -> Result<()> {
    let data = match path {
        Some(p) => std::fs::read(p)?,
        None => Vec::new(),
    };
    let chunks: Vec<&[u8]> = if data.is_empty() { vec![&[][..]] } else { data.chunks(CHUNK).collect() };
    let n = chunks.len();
    for (i, c) in chunks.into_iter().enumerate() {
        let mut m = base.clone();
        m["seq"] = json!(i);
        m["b64"] = json!(B64.encode(c));
        m["last"] = json!(i + 1 == n);
        if i + 1 == n {
            if let Some(o) = last_extra.as_object() {
                for (k, x) in o {
                    m[k] = x.clone();
                }
            }
        }
        if !inner.out.send(m).await {
            bail!("the link dropped while sending");
        }
    }
    Ok(())
}

async fn pull_git(inner: &Inner, chat: &str, req: &Value, base: &str, dir: &Path) -> Result<()> {
    let branch = format!("refs/heads/pc/{chat}");
    let head = String::from_utf8_lossy(&git_ok(dir, &["rev-parse", "--verify", &format!("{branch}^{{commit}}")]).await?)
        .trim()
        .to_string();
    let tmp = inner.work_dir.join(".crundi-chats").join(format!("{chat}-{}.bundle", random_hex(4)));
    let bundle_msg = json!({"type": "chat.bundle", "chat": chat, "req": req, "head": head});
    let base_known = !base.is_empty()
        && !base.starts_with('-')
        && git_raw(dir, &["cat-file", "-e", &format!("{base}^{{commit}}")]).await.is_ok_and(|r| r.0);
    if head == base {
        // git refuses an empty bundle: one empty last chunk, head == base.
        send_chunks(inner, None, bundle_msg, json!({})).await?;
    } else {
        let t = tmp.to_string_lossy().into_owned();
        let neg = format!("^{base}");
        let mut args = vec!["bundle", "create", t.as_str(), branch.as_str()];
        if base_known {
            args.push(&neg);
        }
        let made = git_ok(dir, &args).await;
        let r = match made {
            Ok(_) => send_chunks(inner, Some(&tmp), bundle_msg, json!({})).await,
            Err(e) if e.to_string().contains("empty bundle") => send_chunks(inner, None, bundle_msg, json!({})).await,
            Err(e) => Err(e),
        };
        let _ = std::fs::remove_file(&tmp);
        r?;
    }

    // Uncommitted work, new files included: intent-to-add them for the diff,
    // then take that back so the PC's index is as it was.
    let untracked = git_ok(dir, &["ls-files", "-z", "-o", "--exclude-standard"]).await?;
    let spec_file = inner.work_dir.join(".crundi-chats").join(format!("{chat}-{}.paths", random_hex(4)));
    let added = !untracked.is_empty();
    if added {
        std::fs::write(&spec_file, &untracked)?;
    }
    let sf = format!("--pathspec-from-file={}", spec_file.to_string_lossy());
    let diff = async {
        if added {
            git_ok(dir, &["add", "-N", &sf, "--pathspec-file-nul"]).await?;
        }
        git_ok(dir, &["diff", "--binary", "HEAD"]).await
    }
    .await;
    if added {
        let _ = git_raw(dir, &["reset", "-q", &sf, "--pathspec-file-nul"]).await;
        let _ = std::fs::remove_file(&spec_file);
    }
    let diff = diff?;
    if !inner.out.send(json!({"type": "chat.patch", "chat": chat, "req": req, "b64": B64.encode(&diff)})).await {
        bail!("the link dropped while sending");
    }
    Ok(())
}

async fn pull_copy(inner: &Inner, chat: &str, req: &Value, dir: &Path) -> Result<()> {
    let base: BTreeMap<String, (u64, String)> = std::fs::read(base_manifest_path(inner, chat))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let d = dir.to_path_buf();
    let now = tokio::task::spawn_blocking(move || jobfiles::manifest(&d)).await??;
    let changed: Vec<String> = now.iter().filter(|(k, v)| base.get(*k) != Some(v)).map(|(k, _)| k.clone()).collect();
    let removed: Vec<String> = base.keys().filter(|k| !now.contains_key(*k)).cloned().collect();
    let tmp = inner.work_dir.join(".crundi-chats").join(format!("{chat}-{}.tar", random_hex(4)));
    let path = if changed.is_empty() {
        None
    } else {
        let (d, t) = (dir.to_path_buf(), tmp.clone());
        tokio::task::spawn_blocking(move || jobfiles::pack_tar(&d, &changed, &t)).await??;
        Some(tmp.as_path())
    };
    let r = send_chunks(inner, path, json!({"type": "chat.files", "chat": chat, "req": req}), json!({"removed": removed})).await;
    let _ = std::fs::remove_file(&tmp);
    r
}
