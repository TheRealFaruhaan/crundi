//! Small helpers shared by the modules.

use anyhow::{bail, Result};
use rand::Rng;
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTO: u32 = 1;

/// "windows" | "macos" | "linux" (or whatever Rust calls another OS).
pub fn os_name() -> &'static str {
    std::env::consts::OS
}

/// Random lowercase hex, `bytes` bytes long.
pub fn random_hex(bytes: usize) -> String {
    let mut rng = rand::rng();
    (0..bytes).map(|_| format!("{:02x}", rng.random::<u8>())).collect()
}

pub fn hostname() -> String {
    sysinfo::System::host_name()
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "pc".to_string())
}

/// Make sure rustls has a crypto provider (ring). Safe to call many times.
pub fn install_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Normalise a server address the person typed: add `https://` when there
/// is no scheme, drop trailing slashes. Accepts http(s) and ws(s).
pub fn normalize_server(s: &str) -> Result<String> {
    let s = s.trim().trim_end_matches('/');
    if s.is_empty() {
        bail!("no server address");
    }
    let lower = s.to_ascii_lowercase();
    let out = if lower.starts_with("https://") || lower.starts_with("http://") {
        s.to_string()
    } else if lower.starts_with("wss://") {
        format!("https://{}", &s[6..])
    } else if lower.starts_with("ws://") {
        format!("http://{}", &s[5..])
    } else if lower.contains("://") {
        bail!("the server address must start with https:// or http://");
    } else {
        format!("https://{s}")
    };
    Ok(out)
}

/// `{server}/api/workers/join`
pub fn join_url(server: &str) -> Result<String> {
    Ok(format!("{}/api/workers/join", normalize_server(server)?))
}

/// `ws(s)://{server}/api/workers/link`
pub fn link_url(server: &str) -> Result<String> {
    let s = normalize_server(server)?;
    let ws = if let Some(rest) = s.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = s.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        s
    };
    Ok(format!("{ws}/api/workers/link"))
}

/// Find a program on PATH (with PATHEXT on Windows).
pub fn find_program(name: &str) -> Option<PathBuf> {
    find_in_path(name, &std::env::var_os("PATH")?)
}

/// Is `p` a file we could run (unix: an exec bit)?
pub fn is_executable(p: &std::path::Path) -> bool {
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(windows)]
    {
        p.is_file()
    }
}

/// Find a program in the folders of a PATH-style list.
pub fn find_in_path(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    #[cfg(windows)]
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|e| !e.is_empty())
        .map(|e| e.to_ascii_lowercase())
        .collect();
    for dir in std::env::split_paths(path) {
        #[cfg(windows)]
        {
            for ext in &exts {
                let p = dir.join(format!("{name}{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let p = dir.join(name);
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// A tokio command for a found program. On Windows `.cmd`/`.bat` files go
/// through `cmd /C`, and no console window is shown.
pub fn tokio_command(program: &std::path::Path) -> tokio::process::Command {
    #[cfg(windows)]
    {
        let ext = program
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let mut c = if ext == "cmd" || ext == "bat" {
            let mut c = tokio::process::Command::new("cmd");
            c.arg("/C").arg(program);
            c
        } else {
            tokio::process::Command::new(program)
        };
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        c
    }
    #[cfg(not(windows))]
    {
        tokio::process::Command::new(program)
    }
}

/// Run a program with a timeout; returns stdout and stderr when it ran (any
/// exit code), None when it could not start or timed out.
pub async fn run_with_timeout(
    program: &std::path::Path,
    args: &[&str],
    timeout: std::time::Duration,
) -> Option<(bool, String, String)> {
    let mut cmd = tokio_command(program);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = cmd.spawn().ok()?;
    let out = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .ok()?
        .ok()?;
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(join_url("dev.example.com/").unwrap(), "https://dev.example.com/api/workers/join");
        assert_eq!(link_url("https://a.b").unwrap(), "wss://a.b/api/workers/link");
        assert_eq!(link_url("http://127.0.0.1:9").unwrap(), "ws://127.0.0.1:9/api/workers/link");
        assert_eq!(link_url("wss://a.b").unwrap(), "wss://a.b/api/workers/link");
        assert!(normalize_server("ftp://x").is_err());
        assert!(normalize_server("  ").is_err());
    }
}
