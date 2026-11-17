//! The PC terminal: `term.open/input/resize/close` from the server, output as
//! `term.data` (base64, small chunks coalesced for ~16 ms), `term.exit`.

use base64::Engine;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde_json::json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub const MAX_TERMINALS: usize = 8;
const COALESCE: Duration = Duration::from_millis(16);
const MAX_FRAME: usize = 64 * 1024;
/// After the shell exits, how long to wait for the rest of its output.
const DRAIN_AFTER_EXIT: Duration = Duration::from_millis(500);

/// Outgoing JSON frames (already serialised).
pub type Out = mpsc::UnboundedSender<String>;

struct Term {
    master: Box<dyn MasterPty + Send>,
    input: std::sync::mpsc::Sender<Vec<u8>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

/// The default shell: PowerShell (pwsh, else Windows PowerShell), else cmd on
/// Windows; `$SHELL -l`, else `/bin/sh`, elsewhere.
pub fn default_shell() -> Vec<String> {
    #[cfg(windows)]
    {
        for p in ["pwsh", "powershell"] {
            if let Some(path) = crate::util::find_program(p) {
                return vec![path.display().to_string(), "-NoLogo".into()];
            }
        }
        vec![std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into())]
    }
    #[cfg(not(windows))]
    {
        match std::env::var("SHELL") {
            Ok(s) if !s.is_empty() && std::path::Path::new(&s).exists() => vec![s, "-l".into()],
            _ => vec!["/bin/sh".into()],
        }
    }
}

#[derive(Clone)]
pub struct Terminals {
    terms: Arc<Mutex<HashMap<String, Term>>>,
    out: Out,
    shell: Vec<String>,
    work_dir: PathBuf,
}

fn send(out: &Out, v: serde_json::Value) {
    let _ = out.send(v.to_string());
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows.clamp(2, 1000), cols: cols.clamp(2, 1000), pixel_width: 0, pixel_height: 0 }
}

impl Terminals {
    pub fn new(out: Out, shell: Option<Vec<String>>, work_dir: PathBuf) -> Self {
        Terminals {
            terms: Default::default(),
            out,
            shell: shell.filter(|s| !s.is_empty()).unwrap_or_else(default_shell),
            work_dir,
        }
    }

    pub fn count(&self) -> usize {
        self.terms.lock().unwrap().len()
    }

    fn error(&self, r: &str, e: impl std::fmt::Display) {
        send(&self.out, json!({"type": "term.error", "ref": r, "error": e.to_string()}));
    }

    pub fn open(&self, r: &str, cols: u16, rows: u16, cwd: Option<&str>) {
        if r.is_empty() || r.len() > 40 {
            return self.error(r, "bad ref");
        }
        {
            let terms = self.terms.lock().unwrap();
            if terms.contains_key(r) {
                return self.error(r, "a terminal with this ref is already open");
            }
            if terms.len() >= MAX_TERMINALS {
                return self.error(r, format!("at most {MAX_TERMINALS} terminals"));
            }
        }
        let cwd = match cwd.filter(|c| !c.is_empty()) {
            Some(c) => crate::join::absolute(std::path::Path::new(c)),
            None => self.work_dir.clone(),
        };
        if !cwd.is_dir() {
            let _ = std::fs::create_dir_all(&self.work_dir);
            if !cwd.is_dir() {
                return self.error(r, format!("no such folder: {}", cwd.display()));
            }
        }
        if let Err(e) = self.spawn(r, size(cols, rows), cwd) {
            self.error(r, format!("could not open a shell: {e:#}"));
        }
    }

    fn spawn(&self, r: &str, size: PtySize, cwd: PathBuf) -> anyhow::Result<()> {
        let pair = native_pty_system().openpty(size)?;
        let mut cmd = CommandBuilder::new(&self.shell[0]);
        cmd.args(&self.shell[1..]);
        cmd.cwd(cwd);
        if !cfg!(windows) {
            cmd.env("TERM", "xterm-256color");
        }
        cmd.env("CRUNDI_WORKER", "1");
        let mut child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;

        // Input: a thread owns the writer, so a full pty never blocks the link.
        let (in_tx, in_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            while let Ok(buf) = in_rx.recv() {
                if writer.write_all(&buf).and_then(|_| writer.flush()).is_err() {
                    break;
                }
            }
        });

        // Output: a thread reads the pty into a channel.
        let (data_tx, mut data_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if data_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        // Exit: a thread waits for the shell.
        let (exit_tx, mut exit_rx) = oneshot::channel::<u32>();
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code()).unwrap_or(1);
            let _ = exit_tx.send(code);
        });

        self.terms.lock().unwrap().insert(
            r.to_string(),
            Term { master: pair.master, input: in_tx, killer },
        );

        let out = self.out.clone();
        let terms = self.terms.clone();
        let r = r.to_string();
        tokio::spawn(async move {
            let b64 = base64::engine::general_purpose::STANDARD;
            let mut pending: Vec<u8> = Vec::new();
            let mut reader_done = false;
            let mut code: Option<u32> = None;
            let flush = |pending: &mut Vec<u8>| {
                if !pending.is_empty() {
                    send(&out, json!({"type": "term.data", "ref": r, "b64": b64.encode(&pending[..])}));
                    pending.clear();
                }
            };
            loop {
                if reader_done && code.is_some() {
                    break;
                }
                let drain_deadline = async {
                    if code.is_some() {
                        tokio::time::sleep(DRAIN_AFTER_EXIT).await
                    } else {
                        std::future::pending::<()>().await
                    }
                };
                tokio::select! {
                    chunk = data_rx.recv(), if !reader_done => match chunk {
                        Some(c) => {
                            pending.extend_from_slice(&c);
                            // Gather what arrives in the next ~16 ms into one frame.
                            let until = tokio::time::Instant::now() + COALESCE;
                            while pending.len() < MAX_FRAME {
                                match tokio::time::timeout_at(until, data_rx.recv()).await {
                                    Ok(Some(c)) => pending.extend_from_slice(&c),
                                    Ok(None) => { reader_done = true; break; }
                                    Err(_) => break,
                                }
                            }
                            flush(&mut pending);
                        }
                        None => reader_done = true,
                    },
                    c = &mut exit_rx, if code.is_none() => {
                        code = Some(c.unwrap_or(1));
                    },
                    _ = drain_deadline => break,
                }
            }
            flush(&mut pending);
            // Dropping the Term closes the pty (ends the reader on Windows).
            terms.lock().unwrap().remove(&r);
            send(&out, json!({"type": "term.exit", "ref": r, "code": code.unwrap_or(1)}));
        });
        Ok(())
    }

    pub fn input(&self, r: &str, b64: &str) {
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else {
            return self.error(r, "bad base64");
        };
        match self.terms.lock().unwrap().get(r) {
            Some(t) => {
                let _ = t.input.send(bytes);
            }
            None => self.error(r, "no such terminal"),
        }
    }

    pub fn resize(&self, r: &str, cols: u16, rows: u16) {
        if let Some(t) = self.terms.lock().unwrap().get(r) {
            let _ = t.master.resize(size(cols, rows));
        }
    }

    /// Kill the shell; its `term.exit` follows.
    pub fn close(&self, r: &str) {
        if let Some(t) = self.terms.lock().unwrap().get_mut(r) {
            let _ = t.killer.kill();
        }
    }

    /// Kill every shell (the link ended).
    pub fn close_all(&self) {
        let mut terms = self.terms.lock().unwrap();
        for t in terms.values_mut() {
            let _ = t.killer.kill();
        }
        terms.clear();
    }
}
