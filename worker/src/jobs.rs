//! Phase 2: commands on a PC (spec: "Phase 2: commands on a PC").
//!
//! A job outlives the link: its output is spooled to
//! `<workDir>/.crundi-jobs/<job>.log`, and a per-job streamer sends
//! `job.out` from the byte offset the server has (`job.attach {from}`), then
//! `job.exit`, the artifact chunks and `job.done`. Messages go out through a
//! bounded channel, so a big artifact is paced by the websocket.

use crate::jobfiles;
use crate::util::find_program;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

/// Output frames: up to 64 KB, gathered for ~50 ms.
const OUT_CHUNK: usize = 64 * 1024;
const COALESCE: Duration = Duration::from_millis(50);
/// Bundle, files and artifact chunks: 512 KB before base64.
pub const CHUNK: usize = 512 * 1024;
/// Finished jobs the server never collected are forgotten after an hour.
const KEEP_FINISHED: Duration = Duration::from_secs(3600);
/// How long to wait for a bundle or files from the server.
const WAIT_INBOUND: Duration = Duration::from_secs(600);
/// After the process exits, how long to wait for the rest of its output
/// (a background child may hold the pipe open).
const DRAIN_AFTER_EXIT: Duration = Duration::from_secs(2);

/// What the link tells a job start about the PC's switches.
#[derive(Debug, Clone, Copy)]
pub struct Gate {
    pub paused: bool,
    pub allow_commands: bool,
}

struct Finished {
    exit: Value,
    artifact: Option<PathBuf>,
    count: u64,
    bytes: u64,
}

struct JobSt {
    log_bytes: u64,
    /// Streaming to the current link (started on it, or job.attach).
    attached: bool,
    /// Next log offset to send.
    sent_off: u64,
    /// Bumped on every attach/detach so a streamer restarts cleanly.
    gen: u64,
    finished: Option<Finished>,
    finished_at: Option<Instant>,
}

struct JobCell {
    id: String,
    log_path: PathBuf,
    log: Mutex<Option<std::fs::File>>,
    st: Mutex<JobSt>,
    tick: watch::Sender<u64>,
    cancel: watch::Sender<bool>,
    inbox: mpsc::UnboundedSender<Value>,
}

impl JobCell {
    fn bump(&self) {
        self.tick.send_modify(|v| *v = v.wrapping_add(1));
    }

    /// Append to the job's log (spool) and wake the streamer.
    fn append(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let mut f = self.log.lock().unwrap();
        if let Some(f) = f.as_mut() {
            if f.write_all(data).is_ok() {
                self.st.lock().unwrap().log_bytes += data.len() as u64;
            }
        }
        drop(f);
        self.bump();
    }

    fn note(&self, text: &str) {
        self.append(format!("[crundi-worker] {text}\n").as_bytes());
    }
}

/// Where job (and chat) messages go: the current link's bounded channel,
/// or nowhere while the link is down.
#[derive(Clone, Default)]
pub(crate) struct Outbox(Arc<Mutex<Option<mpsc::Sender<String>>>>);

impl Outbox {
    pub fn set(&self, tx: Option<mpsc::Sender<String>>) {
        *self.0.lock().unwrap() = tx;
    }

    pub fn connected(&self) -> bool {
        self.0.lock().unwrap().as_ref().is_some_and(|t| !t.is_closed())
    }

    /// Send on the current link; false when there is none (the message is dropped).
    pub async fn send(&self, v: Value) -> bool {
        let tx = self.0.lock().unwrap().clone();
        match tx {
            Some(tx) => tx.send(v.to_string()).await.is_ok(),
            None => false,
        }
    }

    pub fn send_now(&self, v: Value) {
        let o = self.clone();
        tokio::spawn(async move {
            o.send(v).await;
        });
    }
}

/// Things the embedding app is told (`{"event":"job"|"chat",...}` lines in
/// embedded mode, desktop notifications in the tray).
pub type Events = Option<mpsc::UnboundedSender<Value>>;

pub(crate) fn emit(events: &Events, v: Value) {
    if let Some(e) = events {
        let _ = e.send(v);
    }
}

struct Inner {
    work_dir: PathBuf,
    jobs: Mutex<HashMap<String, Arc<JobCell>>>,
    outbox: Outbox,
    events: Events,
}

#[derive(Clone)]
pub struct Jobs {
    inner: Arc<Inner>,
}

pub(crate) fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && !s.starts_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

impl Jobs {
    pub fn new(work_dir: PathBuf, events: Events) -> Jobs {
        Jobs { inner: Arc::new(Inner { work_dir, jobs: Default::default(), outbox: Outbox::default(), events }) }
    }

    pub fn jobs_dir(&self) -> PathBuf {
        self.inner.work_dir.join(".crundi-jobs")
    }

    /// A link that lists "jobs" was welcomed: job messages go to `tx`.
    pub fn connect(&self, tx: mpsc::Sender<String>) {
        self.inner.outbox.set(Some(tx));
    }

    /// The link ended: stop streaming until the next job.attach.
    pub fn disconnect(&self) {
        self.inner.outbox.set(None);
        for cell in self.inner.jobs.lock().unwrap().values() {
            let mut st = cell.st.lock().unwrap();
            st.attached = false;
            st.gen += 1;
            drop(st);
            cell.bump();
        }
    }

    /// For hello: `[{ job, state, logBytes }]` of jobs still known.
    pub fn summary(&self) -> Vec<Value> {
        self.prune();
        let jobs = self.inner.jobs.lock().unwrap();
        let mut v: Vec<Value> = jobs
            .values()
            .map(|c| {
                let st = c.st.lock().unwrap();
                json!({
                    "job": c.id,
                    "state": if st.finished.is_some() { "finished" } else { "running" },
                    "logBytes": st.log_bytes,
                })
            })
            .collect();
        v.sort_by(|a, b| a["job"].as_str().cmp(&b["job"].as_str()));
        v
    }

    pub fn running(&self) -> Option<String> {
        let jobs = self.inner.jobs.lock().unwrap();
        jobs.values().find(|c| c.st.lock().unwrap().finished.is_none()).map(|c| c.id.clone())
    }

    /// Forget finished jobs older than an hour.
    pub fn prune(&self) {
        let mut gone = Vec::new();
        self.inner.jobs.lock().unwrap().retain(|_, c| {
            let st = c.st.lock().unwrap();
            let keep = st.finished_at.is_none_or(|t| t.elapsed() < KEEP_FINISHED);
            if !keep {
                gone.push(c.clone());
            }
            keep
        });
        for c in gone {
            forget_files(&c);
        }
    }

    /// Kill every running job (the worker is quitting).
    pub fn kill_all(&self) {
        for c in self.inner.jobs.lock().unwrap().values() {
            let _ = c.cancel.send(true);
        }
    }

    async fn send(inner: &Inner, v: Value) -> bool {
        inner.outbox.send(v).await
    }

    fn send_now(&self, v: Value) {
        self.inner.outbox.send_now(v);
    }

    /// Route a job message from the server.
    pub fn handle(&self, v: &Value, gate: Gate) {
        let ty = v.get("type").and_then(Value::as_str).unwrap_or("");
        let job = v.get("job").and_then(Value::as_str).unwrap_or("").to_string();
        match ty {
            "job.start" => self.start(v, gate),
            "job.bundle" | "job.files" => {
                let cell = self.inner.jobs.lock().unwrap().get(&job).cloned();
                if let Some(c) = cell {
                    let _ = c.inbox.send(v.clone());
                }
            }
            "job.cancel" => {
                let cell = self.inner.jobs.lock().unwrap().get(&job).cloned();
                if let Some(c) = cell {
                    let _ = c.cancel.send(true);
                }
            }
            "job.attach" => {
                let from = v.get("from").and_then(Value::as_u64).unwrap_or(0);
                let cell = self.inner.jobs.lock().unwrap().get(&job).cloned();
                match cell {
                    Some(c) => {
                        let mut st = c.st.lock().unwrap();
                        st.sent_off = from.min(st.log_bytes);
                        st.attached = true;
                        st.gen += 1;
                        drop(st);
                        c.bump();
                    }
                    // Forgotten (finished over an hour ago, or another run of the worker).
                    None => self.send_now(json!({"type": "job.done", "job": job, "unknown": true})),
                }
            }
            _ => {}
        }
    }

    fn start(&self, v: &Value, gate: Gate) {
        let job = v.get("job").and_then(Value::as_str).unwrap_or("").to_string();
        let refuse = |e: &str| self.send_now(json!({"type": "job.refused", "job": job, "error": e}));
        if !valid_name(&job) {
            return refuse("bad job id");
        }
        let project = v.get("project").and_then(Value::as_str).unwrap_or("");
        if !project.is_empty() && !valid_name(project) {
            return refuse("bad project name");
        }
        if gate.paused {
            return refuse("this PC is paused");
        }
        if !gate.allow_commands {
            return refuse("commands are not allowed on this PC");
        }
        if self.inner.jobs.lock().unwrap().contains_key(&job) {
            return refuse("a job with this id is already known");
        }
        if let Some(other) = self.running() {
            return refuse(&format!("already running a job ({other})"));
        }
        let command = v.get("command").and_then(Value::as_str).unwrap_or("");
        if command.trim().is_empty() {
            return refuse("no command");
        }
        let dir = self.jobs_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return refuse(&format!("cannot create {}: {e}", dir.display()));
        }
        let log_path = dir.join(format!("{job}.log"));
        let log = match std::fs::File::create(&log_path) {
            Ok(f) => f,
            Err(e) => return refuse(&format!("cannot create the log: {e}")),
        };
        let (inbox_tx, inbox_rx) = mpsc::unbounded_channel();
        let cell = Arc::new(JobCell {
            id: job.clone(),
            log_path,
            log: Mutex::new(Some(log)),
            st: Mutex::new(JobSt {
                log_bytes: 0,
                attached: true,
                sent_off: 0,
                gen: 0,
                finished: None,
                finished_at: None,
            }),
            tick: watch::channel(0).0,
            cancel: watch::channel(false).0,
            inbox: inbox_tx,
        });
        self.inner.jobs.lock().unwrap().insert(job.clone(), cell.clone());
        let inner = self.inner.clone();
        let spec = v.clone();
        tokio::spawn(async move {
            // Accepted first, then output can follow.
            Jobs::send(&inner, json!({"type": "job.accepted", "job": cell.id})).await;
            emit(&inner.events, json!({"event": "job", "state": "started", "job": cell.id,
                "command": spec.get("command").cloned().unwrap_or(Value::Null),
                "project": spec.get("project").cloned().unwrap_or(Value::Null)}));
            tokio::spawn(stream(inner.clone(), cell.clone()));
            run_job(inner, cell, spec, inbox_rx).await;
        });
    }
}

fn forget_files(c: &JobCell) {
    let _ = std::fs::remove_file(&c.log_path);
    if let Some(f) = &c.st.lock().unwrap().finished {
        if let Some(a) = &f.artifact {
            let _ = std::fs::remove_file(a);
        }
    }
}

/// Send `v` if the job is still attached in generation `gen`.
async fn send_if(inner: &Inner, cell: &JobCell, gen: u64, v: Value) -> bool {
    {
        let st = cell.st.lock().unwrap();
        if !st.attached || st.gen != gen {
            return false;
        }
    }
    if Jobs::send(inner, v).await {
        return true;
    }
    // No link (or it just closed): wait for the next job.attach.
    let mut st = cell.st.lock().unwrap();
    if st.gen == gen {
        st.attached = false;
    }
    false
}

pub(crate) fn read_at(path: &Path, off: u64, len: usize) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(off))?;
    let mut buf = vec![0u8; len];
    let mut got = 0;
    while got < len {
        let n = f.read(&mut buf[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    buf.truncate(got);
    Ok(buf)
}

/// The per-job streamer: log from `sent_off`, then exit, artifacts, done.
async fn stream(inner: Arc<Inner>, cell: Arc<JobCell>) {
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut rx = cell.tick.subscribe();
    loop {
        rx.borrow_and_update();
        let (attached, gen, off, log_bytes, done) = {
            let st = cell.st.lock().unwrap();
            (st.attached, st.gen, st.sent_off, st.log_bytes, st.finished.is_some())
        };
        if !attached {
            if rx.changed().await.is_err() {
                return;
            }
            continue;
        }
        if off < log_bytes {
            let len = ((log_bytes - off) as usize).min(OUT_CHUNK);
            let Ok(data) = read_at(&cell.log_path, off, len) else { return };
            if data.is_empty() {
                continue;
            }
            let n = data.len() as u64;
            let msg = json!({"type": "job.out", "job": cell.id, "off": off, "b64": b64.encode(&data)});
            if send_if(&inner, &cell, gen, msg).await {
                let mut st = cell.st.lock().unwrap();
                if st.gen == gen {
                    st.sent_off = off + n;
                }
            }
            continue;
        }
        if done {
            if finish(&inner, &cell, gen).await {
                inner.jobs.lock().unwrap().remove(&cell.id);
                forget_files(&cell);
                return;
            }
            continue;
        }
        // Caught up: wait for more, then gather for a moment.
        if rx.changed().await.is_err() {
            return;
        }
        tokio::time::sleep(COALESCE).await;
    }
}

/// Send exit, the artifact chunks and done; false when the link changed.
async fn finish(inner: &Inner, cell: &JobCell, gen: u64) -> bool {
    let (exit, artifact, count, bytes) = {
        let st = cell.st.lock().unwrap();
        let f = st.finished.as_ref().unwrap();
        (f.exit.clone(), f.artifact.clone(), f.count, f.bytes)
    };
    if !send_if(inner, cell, gen, exit).await {
        return false;
    }
    let b64 = base64::engine::general_purpose::STANDARD;
    let file = artifact.filter(|_| count > 0).and_then(|p| std::fs::File::open(p).ok());
    match file {
        None => {
            let m = json!({"type": "job.artifact", "job": cell.id, "seq": 0, "b64": "", "last": true, "count": 0, "bytes": 0});
            if !send_if(inner, cell, gen, m).await {
                return false;
            }
        }
        Some(mut f) => {
            let mut seq = 0u64;
            let mut sent = 0u64;
            loop {
                let mut buf = vec![0u8; CHUNK];
                let mut got = 0;
                while got < CHUNK {
                    match f.read(&mut buf[got..]) {
                        Ok(0) => break,
                        Ok(n) => got += n,
                        Err(_) => return false,
                    }
                }
                buf.truncate(got);
                sent += got as u64;
                let last = got < CHUNK || sent >= bytes;
                let m = json!({"type": "job.artifact", "job": cell.id, "seq": seq, "b64": b64.encode(&buf), "last": last, "count": count, "bytes": bytes});
                if !send_if(inner, cell, gen, m).await {
                    return false;
                }
                seq += 1;
                if last {
                    break;
                }
            }
        }
    }
    send_if(inner, cell, gen, json!({"type": "job.done", "job": cell.id})).await
}

async fn status(inner: &Inner, cell: &JobCell, phase: &str, detail: &str) {
    Jobs::send(inner, json!({"type": "job.status", "job": cell.id, "phase": phase, "detail": detail})).await;
}

struct RunResult {
    code: i64,
    signal: Option<i32>,
    duration: Duration,
    timed_out: bool,
    cancelled: bool,
}

async fn run_job(inner: Arc<Inner>, cell: Arc<JobCell>, spec: Value, mut inbox: mpsc::UnboundedReceiver<Value>) {
    let mut cancel = cell.cancel.subscribe();
    let project = spec.get("project").and_then(Value::as_str).unwrap_or("").to_string();
    let source = spec.get("source").and_then(Value::as_str).unwrap_or("none").to_string();
    status(&inner, &cell, "preparing", &source).await;
    let prepared = tokio::select! {
        r = prepare(&inner, &cell, &spec, &project, &source, &mut inbox) => r,
        _ = async { let _ = cancel.wait_for(|c| *c).await; } => Err(anyhow!("cancelled")),
    };
    let result = match prepared.and_then(|dir| resolve_cwd(&dir, &spec).map(|cwd| (dir, cwd))) {
        Err(e) => {
            cell.note(&format!("preparing failed: {e:#}"));
            let cancelled = *cell.cancel.borrow();
            Err((format!("{e:#}"), cancelled))
        }
        Ok((dir, cwd)) => {
            status(&inner, &cell, "running", "").await;
            match run_process(&cell, &spec, &cwd).await {
                Ok(r) => Ok((dir, r)),
                Err(e) => {
                    cell.note(&format!("could not start the command: {e:#}"));
                    Err((format!("{e:#}"), false))
                }
            }
        }
    };
    let finished = match result {
        Err((error, cancelled)) => Finished {
            exit: json!({"type": "job.exit", "job": cell.id, "code": -1, "durationMs": 0, "timedOut": false, "cancelled": cancelled, "error": error}),
            artifact: None,
            count: 0,
            bytes: 0,
        },
        Ok((dir, r)) => {
            let mut exit = json!({
                "type": "job.exit", "job": cell.id, "code": r.code,
                "durationMs": r.duration.as_millis() as u64,
                "timedOut": r.timed_out, "cancelled": r.cancelled,
            });
            if let Some(s) = r.signal {
                exit["signal"] = json!(s);
            }
            status(&inner, &cell, "collecting", "").await;
            let globs: Vec<String> = spec
                .get("artifacts")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|g| g.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let out = inner.work_dir.join(".crundi-jobs").join(format!("{}.tar.gz", cell.id));
            let (count, bytes, artifact) = if globs.is_empty() {
                (0, 0, None)
            } else {
                let o = out.clone();
                match tokio::task::spawn_blocking(move || jobfiles::pack_artifacts(&dir, &globs, &o)).await {
                    Ok(Ok((count, bytes, notes))) => {
                        for n in notes {
                            cell.note(&n);
                        }
                        (count, bytes, Some(out))
                    }
                    Ok(Err(e)) => {
                        cell.note(&format!("packing artifacts failed: {e:#}"));
                        (0, 0, None)
                    }
                    Err(e) => {
                        cell.note(&format!("packing artifacts failed: {e}"));
                        (0, 0, None)
                    }
                }
            };
            Finished { exit, artifact, count, bytes }
        }
    };
    let mut ended = json!({"event": "job", "state": "ended", "job": cell.id,
        "command": spec.get("command").cloned().unwrap_or(Value::Null),
        "project": spec.get("project").cloned().unwrap_or(Value::Null)});
    if let Some(c) = finished.exit.get("code") {
        ended["exitCode"] = c.clone();
    }
    emit(&inner.events, ended);
    // Close the spool, then mark finished; the streamer sends the rest.
    *cell.log.lock().unwrap() = None;
    {
        let mut st = cell.st.lock().unwrap();
        st.finished = Some(finished);
        st.finished_at = Some(Instant::now());
    }
    cell.bump();
}

/// `cwd` from the job, which must stay inside the project folder.
pub(crate) fn resolve_cwd(dir: &Path, spec: &Value) -> Result<PathBuf> {
    let rel = spec.get("cwd").and_then(Value::as_str).unwrap_or("").trim();
    if rel.is_empty() || rel == "." {
        return Ok(dir.to_path_buf());
    }
    let Some(r) = jobfiles::safe_rel(&rel.replace('\\', "/")) else { bail!("cwd must stay inside the project: {rel}") };
    let p = dir.join(r);
    let canon = std::fs::canonicalize(&p).with_context(|| format!("no such folder: {rel}"))?;
    let base = std::fs::canonicalize(dir)?;
    if !canon.starts_with(&base) {
        bail!("cwd must stay inside the project: {rel}");
    }
    if !canon.is_dir() {
        bail!("not a folder: {rel}");
    }
    Ok(canon)
}

async fn prepare(
    inner: &Inner,
    cell: &JobCell,
    spec: &Value,
    project: &str,
    source: &str,
    inbox: &mut mpsc::UnboundedReceiver<Value>,
) -> Result<PathBuf> {
    let wd = &inner.work_dir;
    std::fs::create_dir_all(wd)?;
    let log = |b: &[u8]| cell.append(b);
    let p = Prep { log: &log, out: &inner.outbox, kind: "job", id: &cell.id, tmp_dir: wd.join(".crundi-jobs") };
    match source {
        "git" => {
            if project.is_empty() {
                bail!("a git job needs a project");
            }
            let dir = wd.join(project);
            let commit = ensure_commit(&p, spec, &dir, inbox).await?;
            let (ok, _) = git(&p, &dir, &["-c", "advice.detachedHead=false", "checkout", "-q", "--force", "-B", "crundi/job", &commit], false).await?;
            if !ok {
                bail!("git checkout {commit} failed");
            }
            Ok(dir)
        }
        "copy" => {
            if project.is_empty() {
                bail!("a copy job needs a project");
            }
            let dir = wd.join(format!("{project}@copy"));
            prepare_copy(&p, &dir, inbox).await?;
            Ok(dir)
        }
        "none" | "" => {
            let dir = if project.is_empty() { wd.clone() } else { wd.join(project) };
            Ok(if dir.is_dir() { dir } else { wd.clone() })
        }
        other => bail!("unknown source {other}"),
    }
}

/// What preparing a folder needs from its owner (a job or a chat): where
/// its log goes, the link, and how its messages are named
/// (`<kind>.need`, `<kind>.manifest` out; `<kind>.bundle`, `<kind>.files` in).
pub(crate) struct Prep<'a> {
    pub log: &'a (dyn Fn(&[u8]) + Send + Sync),
    pub out: &'a Outbox,
    /// "job" | "chat": the message prefix and the id field's name.
    pub kind: &'static str,
    pub id: &'a str,
    /// Where the bundle / tar being received is written.
    pub tmp_dir: PathBuf,
}

impl Prep<'_> {
    pub(crate) fn note(&self, text: &str) {
        (self.log)(format!("[crundi-worker] {text}\n").as_bytes());
    }

    fn msg(&self, suffix: &str) -> Value {
        let mut m = json!({"type": format!("{}.{suffix}", self.kind)});
        m[self.kind] = json!(self.id);
        m
    }
}

/// Run git in `dir` (no prompts). Returns (ok, stdout, stderr).
pub(crate) async fn git_raw(dir: &Path, args: &[&str]) -> Result<(bool, Vec<u8>, Vec<u8>)> {
    let git = find_program("git").ok_or_else(|| anyhow!("git is not installed on this PC"))?;
    let mut c = crate::util::tokio_command(&git);
    c.args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let out = c.output().await.context("running git")?;
    Ok((out.status.success(), out.stdout, out.stderr))
}

/// Run git in `dir`, its output going to the log. Returns (ok, stdout).
pub(crate) async fn git(p: &Prep<'_>, dir: &Path, args: &[&str], quiet: bool) -> Result<(bool, String)> {
    if !quiet {
        (p.log)(format!("$ git {}\n", args.join(" ")).as_bytes());
    }
    let (ok, stdout, stderr) = git_raw(dir, args).await?;
    if !quiet {
        (p.log)(&stdout);
        (p.log)(&stderr);
    }
    Ok((ok, String::from_utf8_lossy(&stdout).trim().to_string()))
}

async fn has_commit(p: &Prep<'_>, dir: &Path, commit: &str) -> bool {
    git(p, dir, &["cat-file", "-e", &format!("{commit}^{{commit}}")], true)
        .await
        .map(|(ok, _)| ok)
        .unwrap_or(false)
}

async fn rev(p: &Prep<'_>, dir: &Path, r: &str) -> Option<String> {
    git(p, dir, &["rev-parse", "--verify", "-q", &format!("{r}^{{commit}}")], true)
        .await
        .ok()
        .filter(|(ok, s)| *ok && !s.is_empty())
        .map(|(_, s)| s)
}

/// What to offer the server as `have` for a bundle: the newer of
/// `origin/<branch>` and `refs/crundi-bundle/heads/<branch>` (what an earlier
/// bundle brought), so the server sends only commits the PC lacks.
async fn newest_have(p: &Prep<'_>, dir: &Path, branch: &str) -> String {
    if branch.is_empty() {
        return String::new();
    }
    let a = rev(p, dir, &format!("refs/remotes/origin/{branch}")).await;
    let b = rev(p, dir, &format!("refs/crundi-bundle/heads/{branch}")).await;
    match (a, b) {
        (None, None) => String::new(),
        (Some(x), None) | (None, Some(x)) => x,
        (Some(a), Some(b)) if a == b => a,
        (Some(a), Some(b)) => {
            let anc = |x: &str, y: &str| {
                let (x, y) = (x.to_string(), y.to_string());
                async move { git(p, dir, &["merge-base", "--is-ancestor", &x, &y], true).await.map(|r| r.0).unwrap_or(false) }
            };
            if anc(&a, &b).await {
                b
            } else if anc(&b, &a).await {
                a
            } else {
                // Unrelated lines: the one committed later.
                let t = |x: String| async move {
                    let r = git(p, dir, &["log", "-1", "--format=%ct", &x], true).await.ok();
                    (r.and_then(|(_, s)| s.parse::<u64>().ok()).unwrap_or(0), x)
                };
                let (ta, a) = t(a).await;
                let (tb, b) = t(b).await;
                if tb > ta { b } else { a }
            }
        }
    }
}

/// Receive `<kind>.bundle` / `<kind>.files` chunks into `path`; returns the last message.
async fn receive(inbox: &mut mpsc::UnboundedReceiver<Value>, kind: &str, path: &Path) -> Result<Value> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut f = std::fs::File::create(path)?;
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut expect = 0u64;
    loop {
        let m = tokio::time::timeout(WAIT_INBOUND, inbox.recv())
            .await
            .map_err(|_| anyhow!("no {kind} from the server for 10 minutes"))?
            .ok_or_else(|| anyhow!("ended while waiting for {kind}"))?;
        if m.get("type").and_then(Value::as_str) != Some(kind) {
            continue;
        }
        let seq = m.get("seq").and_then(Value::as_u64).unwrap_or(expect);
        if seq != expect {
            bail!("{kind} chunk {seq} came when {expect} was due");
        }
        expect += 1;
        let data = b64.decode(m.get("b64").and_then(Value::as_str).unwrap_or("")).context("bad base64")?;
        f.write_all(&data)?;
        if m.get("last").and_then(Value::as_bool).unwrap_or(false) {
            f.flush()?;
            return Ok(m);
        }
    }
}

/// The git source up to the commit: clone once into `dir`, fetch the
/// branch, and when the commit is still missing ask the server for a bundle.
/// Returns the commit to check out.
pub(crate) async fn ensure_commit(
    p: &Prep<'_>,
    spec: &Value,
    dir: &Path,
    inbox: &mut mpsc::UnboundedReceiver<Value>,
) -> Result<String> {
    let g = spec.get("git").cloned().unwrap_or(Value::Null);
    let url = g.get("url").and_then(Value::as_str).unwrap_or("");
    let branch = g.get("branch").and_then(Value::as_str).unwrap_or("");
    let mut commit = g.get("commit").and_then(Value::as_str).unwrap_or("").to_string();
    if branch.starts_with('-') || commit.starts_with('-') || url.starts_with('-') {
        bail!("bad git arguments");
    }
    if !dir.join(".git").exists() {
        if url.is_empty() {
            bail!("no git url to clone");
        }
        if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
            bail!("{} exists and is not a git clone", dir.display());
        }
        let parent = dir.parent().unwrap();
        std::fs::create_dir_all(parent)?;
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let (ok, _) = git(p, parent, &["clone", "--no-checkout", url, &name], false).await?;
        if !ok {
            bail!("git clone failed");
        }
    }
    if !branch.is_empty() {
        let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
        let (ok, _) = git(p, dir, &["fetch", "origin", &refspec], false).await?;
        if !ok {
            p.note(&format!("could not fetch {branch} from origin; asking the server"));
        }
    }
    if commit.is_empty() {
        if branch.is_empty() {
            bail!("no branch or commit to check out");
        }
        commit = rev(p, dir, &format!("refs/remotes/origin/{branch}"))
            .await
            .ok_or_else(|| anyhow!("branch {branch} not found"))?;
    }
    if !has_commit(p, dir, &commit).await {
        let have = newest_have(p, dir, branch).await;
        p.note(&format!("commit {commit} is not on origin; asking the server for a bundle"));
        let mut need = p.msg("need");
        need["kind"] = json!("bundle");
        need["have"] = json!(have);
        p.out.send(need).await;
        let path = p.tmp_dir.join(format!("{}.bundle", p.id));
        let got = receive(inbox, &format!("{}.bundle", p.kind), &path).await;
        let res = async {
            got?;
            let bp = path.to_string_lossy().into_owned();
            let (ok, _) = git(p, dir, &["fetch", "-q", &bp, "+refs/*:refs/crundi-bundle/*"], false).await?;
            if !ok || !has_commit(p, dir, &commit).await {
                let _ = git(p, dir, &["fetch", "-q", &bp, &commit], false).await;
            }
            if !has_commit(p, dir, &commit).await {
                bail!("the bundle did not hold commit {commit}");
            }
            Ok(())
        }
        .await;
        let _ = std::fs::remove_file(&path);
        res?;
    }
    Ok(commit)
}

/// The copy source: send the manifest, receive the tar of changes and the removals.
pub(crate) async fn prepare_copy(p: &Prep<'_>, dir: &Path, inbox: &mut mpsc::UnboundedReceiver<Value>) -> Result<()> {
    let d = dir.to_path_buf();
    let files = tokio::task::spawn_blocking(move || jobfiles::manifest(&d)).await??;
    p.note(&format!("the copy has {} files; sending the manifest", files.len()));
    let map: serde_json::Map<String, Value> =
        files.into_iter().map(|(k, (size, sha))| (k, json!([size, sha]))).collect();
    let mut m = p.msg("manifest");
    m["files"] = Value::Object(map);
    p.out.send(m).await;
    let path = p.tmp_dir.join(format!("{}.tar", p.id));
    let got = receive(inbox, &format!("{}.files", p.kind), &path).await;
    let res = async {
        let last = got?;
        let removed: Vec<String> = last
            .get("removed")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let (d, tp) = (dir.to_path_buf(), path.clone());
        let (written, gone) = tokio::task::spawn_blocking(move || -> Result<(u64, u64)> {
            let size = std::fs::metadata(&tp)?.len();
            let w = if size > 0 { jobfiles::extract_tar(&tp, &d)? } else { 0 };
            let g = jobfiles::remove_paths(&d, &removed)?;
            Ok((w, g))
        })
        .await??;
        p.note(&format!("copy updated: {written} files written, {gone} removed"));
        Ok::<(), anyhow::Error>(())
    }
    .await;
    let _ = std::fs::remove_file(&path);
    res
}

/// Kill the whole process tree of the job.
pub(crate) fn kill_tree(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
}

fn shell_command(command: &str, low_priority: bool) -> tokio::process::Command {
    #[cfg(unix)]
    {
        let nice = if low_priority { find_program("nice") } else { None };
        let mut c = match nice {
            Some(n) => {
                let mut c = tokio::process::Command::new(n);
                c.args(["-n", "10", "/bin/sh", "-lc", command]);
                c
            }
            None => {
                let mut c = tokio::process::Command::new("/bin/sh");
                c.args(["-lc", command]);
                c
            }
        };
        // Its own process group, so cancel and timeout kill the whole tree.
        c.process_group(0);
        c
    }
    #[cfg(windows)]
    {
        let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into());
        let mut c = tokio::process::Command::new(comspec);
        c.raw_arg(format!("/d /s /c \"{command}\""));
        let mut flags = 0x0800_0000 | 0x0000_0200; // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP
        if low_priority {
            flags |= 0x0000_4000; // BELOW_NORMAL_PRIORITY_CLASS
        }
        c.creation_flags(flags);
        c
    }
}

async fn run_process(cell: &Arc<JobCell>, spec: &Value, cwd: &Path) -> Result<RunResult> {
    let command = spec.get("command").and_then(Value::as_str).unwrap_or("");
    let low = spec.get("lowPriority").and_then(Value::as_bool).unwrap_or(true);
    let timeout = Duration::from_secs(spec.get("timeoutSec").and_then(Value::as_u64).unwrap_or(7200).max(1));
    let mut c = shell_command(command, low);
    c.current_dir(cwd).env("CRUNDI_JOB", &cell.id).stdin(std::process::Stdio::null());
    if let Some(env) = spec.get("env").and_then(Value::as_object) {
        for (k, v) in env {
            if let Some(v) = v.as_str() {
                if !k.is_empty() && !k.contains('=') {
                    c.env(k, v);
                }
            }
        }
    }
    // stdout and stderr share one pipe, so the log keeps their order.
    let (mut reader, writer) = std::io::pipe()?;
    c.stdout(writer.try_clone()?).stderr(writer);
    cell.append(format!("$ {command}\n").as_bytes());
    let started = Instant::now();
    let mut child = c.spawn()?;
    drop(c); // closes our copies of the pipe's write end
    let pid = child.id().unwrap_or(0);

    let (eof_tx, mut eof_rx) = tokio::sync::oneshot::channel::<()>();
    let rcell = cell.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => rcell.append(&buf[..n]),
            }
        }
        let _ = eof_tx.send(());
    });

    let mut cancel = cell.cancel.subscribe();
    let mut timed_out = false;
    let mut cancelled = false;
    let status = tokio::select! {
        s = child.wait() => s?,
        _ = tokio::time::sleep(timeout) => {
            timed_out = true;
            kill_tree(pid);
            child.wait().await?
        }
        _ = async { let _ = cancel.wait_for(|c| *c).await; } => {
            cancelled = true;
            kill_tree(pid);
            child.wait().await?
        }
    };
    let duration = started.elapsed();
    let _ = tokio::time::timeout(DRAIN_AFTER_EXIT, &mut eof_rx).await;
    if timed_out {
        cell.note(&format!("timed out after {} s; stopped", timeout.as_secs()));
    } else if cancelled {
        cell.note("cancelled");
    }
    #[cfg(unix)]
    let signal = std::os::unix::process::ExitStatusExt::signal(&status);
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    let code = match (status.code(), signal) {
        (Some(c), _) => c as i64,
        (None, Some(s)) => 128 + s as i64,
        _ => -1,
    };
    Ok(RunResult { code, signal, duration, timed_out, cancelled })
}
