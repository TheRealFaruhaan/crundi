//! One worker per PC: an exclusive lock on `<config dir>/worker.lock`, held
//! for the life of the process (std's file lock: flock on Unix, LockFileEx on
//! Windows; the OS drops it when the process ends, also on exec). The tray or
//! headless worker and the PC client's embedded one never both connect: the
//! second one sees the lock taken and does not start its link.
//! `worker.owner` beside it says who holds it (pid, kind), for the message.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const LOCK_FILE: &str = "worker.lock";
pub const OWNER_FILE: &str = "worker.owner";

/// The lock; dropping it releases it.
#[derive(Debug)]
pub struct WorkerLock {
    _file: File,
    owner: PathBuf,
}

impl Drop for WorkerLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.owner);
    }
}

/// Why the lock could not be had.
#[derive(Debug)]
pub enum LockError {
    /// Another worker holds it; the text says which.
    Held(String),
    Io(std::io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Held(s) => f.write_str(s),
            LockError::Io(e) => write!(f, "could not take the worker lock: {e}"),
        }
    }
}

/// The folder the lock lives in for a config file: its folder.
pub fn lock_dir(config_path: &Path) -> PathBuf {
    config_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

fn describe(dir: &Path) -> String {
    let owner = std::fs::read_to_string(dir.join(OWNER_FILE)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let kind = owner.as_ref().and_then(|o| o.get("kind")).and_then(|k| k.as_str()).unwrap_or("").to_string();
    let pid = owner.as_ref().and_then(|o| o.get("pid")).and_then(|p| p.as_u64());
    let who = match kind.as_str() {
        "electron" | "native" => "the Crundi PC app's worker".to_string(),
        "tray" => "the Crundi worker tray app".to_string(),
        "headless" => "a headless Crundi worker".to_string(),
        _ => "another Crundi worker".to_string(),
    };
    match pid {
        Some(p) => format!("{who} (pid {p}) already runs on this PC; only one worker per PC may connect"),
        None => format!("{who} already runs on this PC; only one worker per PC may connect"),
    }
}

/// Take the lock in `dir` for a worker of `kind`.
pub fn acquire(dir: &Path, kind: &str) -> Result<WorkerLock, LockError> {
    std::fs::create_dir_all(dir).map_err(LockError::Io)?;
    let path = dir.join(LOCK_FILE);
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path).map_err(LockError::Io)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Err(LockError::Held(describe(dir))),
        Err(std::fs::TryLockError::Error(e)) => return Err(LockError::Io(e)),
    }
    let owner = dir.join(OWNER_FILE);
    let _ = std::fs::write(&owner, serde_json::json!({"pid": std::process::id(), "kind": kind}).to_string());
    Ok(WorkerLock { _file: file, owner })
}

/// Take the lock, waiting up to `wait` for another holder to let go (a
/// worker restarted by an update waits for the old process to end).
pub fn acquire_waiting(dir: &Path, kind: &str, wait: Duration) -> Result<WorkerLock, LockError> {
    let until = Instant::now() + wait;
    loop {
        match acquire(dir, kind) {
            Err(LockError::Held(_)) if Instant::now() < until => std::thread::sleep(Duration::from_millis(200)),
            r => return r,
        }
    }
}

/// How long to wait for the lock at start: a few seconds after an update's restart.
pub fn start_wait() -> Duration {
    if std::env::var_os(crate::update::RESTARTED_ENV).is_some() {
        Duration::from_secs(15)
    } else {
        Duration::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_lock_is_refused_until_the_first_is_dropped() {
        let d = std::env::temp_dir().join(format!("cw-lock-{}", crate::util::random_hex(6)));
        let a = acquire(&d, "tray").unwrap();
        match acquire(&d, "electron") {
            Err(LockError::Held(m)) => assert!(m.contains("tray app") && m.contains(&format!("pid {}", std::process::id())), "{m}"),
            other => panic!("expected Held, got {other:?}"),
        }
        drop(a);
        let b = acquire_waiting(&d, "electron", Duration::from_secs(1)).unwrap();
        drop(b);
        std::fs::remove_dir_all(&d).ok();
    }
}
