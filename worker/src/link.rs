//! The link: one websocket to `{server}/api/workers/link` (spec: "The link").

use crate::config::Config;
use crate::info::{self, InfoInputs};
use crate::jobs::{Gate, Jobs};
use crate::term::Terminals;
use crate::util::{PROTO, VERSION};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest, http::HeaderValue, Message};

pub const CAPS: &[&str] = &["info", "terminal", "pause", "jobs"];

#[derive(Debug, Clone)]
pub struct LinkOptions {
    /// "tray" | "headless" | "electron" | "native"
    pub kind: String,
    /// Where settings and a forgotten key are saved; None = never save (embedded).
    pub persist: Option<PathBuf>,
    pub backoff_base: Duration,
    pub backoff_max: Duration,
    pub info_every: Duration,
    /// The link counts as dead after this long with nothing from the server
    /// (it pings every 25 s).
    pub dead_after: Duration,
    /// Shell command for terminals; None = the platform default.
    pub shell: Option<Vec<String>>,
}

impl LinkOptions {
    pub fn new(kind: &str, persist: Option<PathBuf>) -> Self {
        LinkOptions {
            kind: kind.to_string(),
            persist,
            backoff_base: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            info_every: Duration::from_secs(30),
            dead_after: Duration::from_secs(90),
            shell: None,
        }
    }
}

/// Things the tray (or the embedding client) asks of a running link.
#[derive(Debug, Clone)]
pub enum Control {
    /// Pause/resume from this PC: `{type:"paused", paused, by:"pc"}`.
    Pause(bool),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    NotJoined,
    Connecting,
    Online,
    /// Lost or could not connect; retrying.
    Offline,
    /// 401: the key is bad, revoked or the worker was removed.
    Unauthorized,
    /// bye revoked/removed: the key has been forgotten.
    Removed,
    /// bye replaced: another link for this worker took over.
    Replaced,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: State,
    pub online: bool,
    pub server: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub paused: bool,
    pub allow_chats: bool,
    pub allow_commands: bool,
    pub terminals: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_in_sec: Option<u64>,
}

impl Status {
    pub fn from_config(cfg: &Config, state: State) -> Status {
        Status {
            state,
            online: state == State::Online,
            server: cfg.server.clone(),
            id: cfg.id.clone(),
            name: cfg.name.clone(),
            paused: cfg.paused,
            allow_chats: cfg.allow_chats,
            allow_commands: cfg.allow_commands,
            terminals: 0,
            error: None,
            retry_in_sec: None,
        }
    }

    /// One line of text for the tray's status item.
    pub fn line(&self) -> String {
        let name = self.name.clone().unwrap_or_default();
        match self.state {
            State::NotJoined => "Not joined to a server".into(),
            State::Connecting => format!("{name}: connecting..."),
            State::Online if self.paused => format!("{name}: online, paused"),
            State::Online => format!("{name}: online"),
            State::Offline => match self.retry_in_sec {
                Some(s) => format!("{name}: offline, retrying in {s} s"),
                None => format!("{name}: offline"),
            },
            State::Unauthorized | State::Removed => "This PC was removed from the server".into(),
            State::Replaced => "Another program took over this worker".into(),
            State::Stopped => "Stopped".into(),
        }
    }
}

/// How `run` ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Quit,
    NotJoined,
    /// HTTP 401 on connect.
    Unauthorized,
    /// bye revoked/removed; the key was forgotten.
    Removed(String),
    /// bye replaced.
    Replaced,
}

enum SessionEnd {
    Quit,
    Dropped(String),
    Bye(String),
}

struct Link {
    cfg: Config,
    opts: LinkOptions,
    status: watch::Sender<Status>,
    /// The person paused/resumed while offline: tell the server after welcome.
    pause_unsent: bool,
    /// Phase 2 jobs; they outlive a session (the link dropping).
    jobs: Jobs,
}

/// Run the link until quit, 401, or bye. Reconnects with backoff.
pub async fn run(
    cfg: Config,
    opts: LinkOptions,
    mut ctrl: mpsc::UnboundedReceiver<Control>,
    status: watch::Sender<Status>,
) -> Outcome {
    crate::util::install_crypto();
    let _ = cfg.ensure_work_dir();
    let jobs = Jobs::new(cfg.work_dir.clone());
    let mut link = Link { cfg, opts, status, pause_unsent: false, jobs };
    if !link.cfg.joined() {
        link.set_state(State::NotJoined, None, None);
        return Outcome::NotJoined;
    }
    let outcome = run_loop(&mut link, &mut ctrl).await;
    // Quitting or removed: running jobs are stopped.
    link.jobs.kill_all();
    outcome
}

async fn run_loop(link: &mut Link, ctrl: &mut mpsc::UnboundedReceiver<Control>) -> Outcome {
    let mut attempt: u32 = 0;
    loop {
        link.set_state(State::Connecting, None, None);
        let err = match link.connect().await {
            Ok(ws) => match link.session(ws, ctrl).await {
                (SessionEnd::Quit, _) => {
                    link.set_state(State::Stopped, None, None);
                    return Outcome::Quit;
                }
                (SessionEnd::Bye(reason), _) if reason == "replaced" => {
                    link.set_state(State::Replaced, None, None);
                    return Outcome::Replaced;
                }
                (SessionEnd::Bye(reason), _) if reason == "revoked" || reason == "removed" => {
                    link.cfg.key = None;
                    link.save();
                    link.set_state(State::Removed, Some(format!("bye: {reason}")), None);
                    return Outcome::Removed(reason);
                }
                (SessionEnd::Bye(reason), welcomed) => {
                    if welcomed {
                        attempt = 0;
                    }
                    format!("the server said bye ({reason})")
                }
                (SessionEnd::Dropped(e), welcomed) => {
                    if welcomed {
                        attempt = 0;
                    }
                    e
                }
            },
            Err(ConnectError::Unauthorized) => {
                link.set_state(State::Unauthorized, Some("the server refused this worker's key (401)".into()), None);
                return Outcome::Unauthorized;
            }
            Err(ConnectError::Other(e)) => e,
        };
        let delay = backoff(attempt, link.opts.backoff_base, link.opts.backoff_max);
        attempt = attempt.saturating_add(1);
        link.set_state(State::Offline, Some(err), Some(delay));
        let until = Instant::now() + delay;
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(until) => break,
                c = ctrl.recv() => match c {
                    None | Some(Control::Quit) => {
                        link.set_state(State::Stopped, None, None);
                        return Outcome::Quit;
                    }
                    Some(Control::Pause(p)) => {
                        if link.cfg.paused != p {
                            link.cfg.paused = p;
                            link.pause_unsent = true;
                            link.save();
                        }
                        let (e, r) = {
                            let s = link.status.borrow();
                            (s.error.clone(), s.retry_in_sec)
                        };
                        link.set_state(State::Offline, e, r.map(Duration::from_secs));
                    }
                }
            }
        }
    }
}

/// 1 s doubling to 60 s, plus up to 25 % jitter.
pub fn backoff(attempt: u32, base: Duration, max: Duration) -> Duration {
    let d = base.saturating_mul(1u32 << attempt.min(16)).min(max);
    let jitter = rand::rng().random_range(0.0..0.25);
    d + d.mul_f64(jitter)
}

enum ConnectError {
    Unauthorized,
    Other(String),
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

impl Link {
    fn save(&self) {
        if let Some(p) = &self.opts.persist {
            if let Err(e) = self.cfg.save(p) {
                eprintln!("crundi-worker: could not save the config: {e:#}");
            }
        }
    }

    fn set_state(&self, state: State, error: Option<String>, retry: Option<Duration>) {
        let mut s = Status::from_config(&self.cfg, state);
        s.error = error;
        s.retry_in_sec = retry.map(|d| d.as_secs().max(1));
        let prev_terms = self.status.borrow().terminals;
        s.terminals = if state == State::Online { prev_terms } else { 0 };
        self.status.send_replace(s);
    }

    fn set_terminals(&self, n: usize) {
        self.status.send_if_modified(|s| {
            let changed = s.terminals != n;
            s.terminals = n;
            changed
        });
    }

    fn info_inputs(&self) -> InfoInputs {
        InfoInputs {
            work_dir: self.cfg.work_dir.clone(),
            allow_chats: self.cfg.allow_chats,
            allow_commands: self.cfg.allow_commands,
            paused: self.cfg.paused,
        }
    }

    async fn connect(&self) -> Result<Ws, ConnectError> {
        let server = self.cfg.server.as_deref().unwrap_or_default();
        let key = self.cfg.key.as_deref().unwrap_or_default();
        let url = crate::util::link_url(server).map_err(|e| ConnectError::Other(e.to_string()))?;
        let mut req = url
            .as_str()
            .into_client_request()
            .map_err(|e| ConnectError::Other(format!("bad link address {url}: {e}")))?;
        let auth = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| ConnectError::Other("the key has characters a header cannot carry".into()))?;
        req.headers_mut().insert("Authorization", auth);
        req.headers_mut().insert(
            "User-Agent",
            HeaderValue::from_str(&format!("crundi-worker/{VERSION}")).unwrap(),
        );
        let fut = tokio_tungstenite::connect_async(req);
        match tokio::time::timeout(Duration::from_secs(30), fut).await {
            Err(_) => Err(ConnectError::Other("timed out connecting".into())),
            Ok(Ok((ws, _))) => Ok(ws),
            Ok(Err(tungstenite::Error::Http(resp))) if resp.status().as_u16() == 401 => {
                Err(ConnectError::Unauthorized)
            }
            Ok(Err(tungstenite::Error::Http(resp))) => {
                Err(ConnectError::Other(format!("the server answered {}", resp.status())))
            }
            Ok(Err(e)) => Err(ConnectError::Other(e.to_string())),
        }
    }

    async fn info_json(&self) -> Value {
        serde_json::to_value(info::gather(self.info_inputs()).await).unwrap_or(Value::Null)
    }

    /// One connected session. Returns how it ended and whether welcome came.
    async fn session(
        &mut self,
        mut ws: Ws,
        ctrl: &mut mpsc::UnboundedReceiver<Control>,
    ) -> (SessionEnd, bool) {
        let info = self.info_json().await;
        let mut hello = json!({
            "type": "hello",
            "proto": PROTO,
            "version": VERSION,
            "kind": self.opts.kind,
            "name": self.cfg.display_name(),
            "caps": CAPS,
            "info": info,
        });
        if let Some(d) = &self.cfg.device_id {
            hello["deviceId"] = json!(d);
        }
        // Jobs still running (or finished, not yet collected), so the server can job.attach.
        hello["jobs"] = json!(self.jobs.summary());
        if let Err(e) = ws.send(Message::Text(hello.to_string().into())).await {
            return (SessionEnd::Dropped(e.to_string()), false);
        }

        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
        // Job messages: bounded, so artifact chunks are paced by the socket.
        let (job_tx, mut job_rx) = mpsc::channel::<String>(16);
        let terms = Terminals::new(out_tx.clone(), self.opts.shell.clone(), self.cfg.work_dir.clone());
        let mut welcomed = false;
        let mut info_tick = tokio::time::interval_at(Instant::now() + self.opts.info_every, self.opts.info_every);
        info_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_rx = Instant::now();

        let end = loop {
            let dead_at = last_rx + self.opts.dead_after;
            tokio::select! {
                m = ws.next() => {
                    last_rx = Instant::now();
                    match m {
                        None => break SessionEnd::Dropped("the server closed the link".into()),
                        Some(Err(e)) => break SessionEnd::Dropped(e.to_string()),
                        Some(Ok(Message::Close(_))) => break SessionEnd::Dropped("the server closed the link".into()),
                        Some(Ok(Message::Text(t))) => {
                            let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                            if let Some(end) = self.handle(&v, &terms, &out_tx, &job_tx, &mut welcomed) {
                                break end;
                            }
                            self.set_terminals(terms.count());
                        }
                        // Binary frames are phase 2; ping/pong is handled by tungstenite.
                        Some(Ok(_)) => {}
                    }
                }
                s = out_rx.recv() => {
                    let Some(s) = s else { continue };
                    if let Err(e) = ws.send(Message::Text(s.into())).await {
                        break SessionEnd::Dropped(e.to_string());
                    }
                    self.set_terminals(terms.count());
                }
                s = job_rx.recv() => {
                    let Some(s) = s else { continue };
                    if let Err(e) = ws.send(Message::Text(s.into())).await {
                        break SessionEnd::Dropped(e.to_string());
                    }
                }
                _ = info_tick.tick() => {
                    self.jobs.prune();
                    self.send_info(&out_tx);
                }
                c = ctrl.recv() => match c {
                    None | Some(Control::Quit) => {
                        let _ = tokio::time::timeout(Duration::from_secs(2), ws.close(None)).await;
                        break SessionEnd::Quit;
                    }
                    Some(Control::Pause(p)) => {
                        if self.cfg.paused != p {
                            self.cfg.paused = p;
                            self.save();
                        }
                        if welcomed {
                            let _ = out_tx.send(json!({"type": "paused", "paused": p, "by": "pc"}).to_string());
                            self.send_info(&out_tx);
                        } else {
                            self.pause_unsent = true;
                        }
                        self.set_state(if welcomed { State::Online } else { State::Connecting }, None, None);
                    }
                },
                _ = tokio::time::sleep_until(dead_at) => {
                    break SessionEnd::Dropped("no word from the server for too long".into());
                }
            }
        };
        // The server forgets this link's terminals when it ends; so do we.
        terms.close_all();
        // Jobs keep running; their output waits in the spool for job.attach.
        self.jobs.disconnect();
        (end, welcomed)
    }

    fn send_info(&self, out: &mpsc::UnboundedSender<String>) {
        let inp = self.info_inputs();
        let out = out.clone();
        tokio::spawn(async move {
            let info = info::gather(inp).await;
            let _ = out.send(json!({"type": "info", "info": info}).to_string());
        });
    }

    /// Apply `{ name?, allowChats?, allowCommands?, paused? }`; true when anything changed.
    fn apply_settings(&mut self, v: &Value) -> bool {
        let mut changed = false;
        if let Some(n) = v.get("name").and_then(Value::as_str) {
            if !n.is_empty() && self.cfg.name.as_deref() != Some(n) {
                self.cfg.name = Some(n.to_string());
                changed = true;
            }
        }
        for (field, slot) in [
            ("allowChats", &mut self.cfg.allow_chats),
            ("allowCommands", &mut self.cfg.allow_commands),
        ] {
            if let Some(b) = v.get(field).and_then(Value::as_bool) {
                if *slot != b {
                    *slot = b;
                    changed = true;
                }
            }
        }
        if let Some(b) = v.get("paused").and_then(Value::as_bool) {
            if self.cfg.paused != b {
                self.cfg.paused = b;
                changed = true;
            }
        }
        changed
    }

    fn handle(
        &mut self,
        v: &Value,
        terms: &Terminals,
        out: &mpsc::UnboundedSender<String>,
        job_tx: &mpsc::Sender<String>,
        welcomed: &mut bool,
    ) -> Option<SessionEnd> {
        let s = |k: &str| v.get(k).and_then(Value::as_str);
        let n = |k: &str, d: u16| v.get(k).and_then(Value::as_u64).map(|x| x.min(1000) as u16).unwrap_or(d);
        match s("type")? {
            "welcome" => {
                *welcomed = true;
                if let Some(id) = s("id") {
                    self.cfg.id = Some(id.to_string());
                }
                if let Some(name) = s("name").filter(|n| !n.is_empty()) {
                    self.cfg.name = Some(name.to_string());
                }
                let local_pause = self.cfg.paused;
                if let Some(settings) = v.get("settings") {
                    self.apply_settings(settings);
                }
                if self.pause_unsent {
                    // Paused/resumed from the tray while offline: the PC's choice wins.
                    self.pause_unsent = false;
                    self.cfg.paused = local_pause;
                    let _ = out.send(json!({"type": "paused", "paused": local_pause, "by": "pc"}).to_string());
                }
                self.save();
                let server_jobs = v
                    .get("caps")
                    .and_then(Value::as_array)
                    .is_some_and(|c| c.iter().any(|x| x == "jobs"));
                if server_jobs {
                    self.jobs.connect(job_tx.clone());
                }
                self.set_state(State::Online, None, None);
                self.send_info(out);
            }
            "settings" => {
                self.apply_settings(v);
                self.save();
                self.set_state(State::Online, None, None);
                self.send_info(out);
            }
            "term.open" => {
                if let Some(r) = s("ref") {
                    terms.open(r, n("cols", 80), n("rows", 24), s("cwd"));
                }
            }
            "term.input" => {
                if let (Some(r), Some(b)) = (s("ref"), s("b64")) {
                    terms.input(r, b);
                }
            }
            "term.resize" => {
                if let Some(r) = s("ref") {
                    terms.resize(r, n("cols", 80), n("rows", 24));
                }
            }
            "term.close" => {
                if let Some(r) = s("ref") {
                    terms.close(r);
                }
            }
            t if t.starts_with("job.") => {
                let gate = Gate { paused: self.cfg.paused, allow_commands: self.cfg.allow_commands };
                self.jobs.handle(v, gate);
            }
            "bye" => return Some(SessionEnd::Bye(s("reason").unwrap_or("").to_string())),
            _ => {} // unknown types are ignored (newer server)
        }
        None
    }
}

/// Run a link on the current tokio runtime, with fresh channels.
pub struct LinkHandle {
    pub ctrl: mpsc::UnboundedSender<Control>,
    pub status: watch::Receiver<Status>,
    pub task: tokio::task::JoinHandle<Outcome>,
}

pub fn spawn(cfg: Config, opts: LinkOptions) -> LinkHandle {
    let (ctrl_tx, ctrl_rx) = mpsc::unbounded_channel();
    let (st_tx, st_rx) = watch::channel(Status::from_config(&cfg, State::Connecting));
    let task = tokio::spawn(run(cfg, opts, ctrl_rx, st_tx));
    LinkHandle { ctrl: ctrl_tx, status: st_rx, task }
}
