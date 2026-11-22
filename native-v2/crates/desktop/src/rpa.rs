//! Locked-screen capture on Windows (app/main.js `rdp:status` / `rdp:setup`,
//! src/rpa.js): the DisconnectRDP scheduled task (the `tscon` trick), made
//! by the installer (build/installer.nsh) or here once with admin rights.
//! Elsewhere there is nothing to set up ({ supported: false }).
//!
//! The setup wizard's "Locked-screen capture" step shows this status and
//! its "Set up now (requires admin)" button calls `setup`.

use serde_json::{json, Value};

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub const TASK_NAME: &str = "DisconnectRDP";
const BAT_FILENAME: &str = "disconnect-rdp.bat";

/// rdp:status: { ok, supported, installed, isRdp }.
pub fn status() -> Value {
    if !cfg!(target_os = "windows") { return json!({ "ok": true, "supported": false, "installed": false, "isRdp": false }); }
    json!({ "ok": true, "supported": true, "installed": is_task_installed(), "isRdp": is_rdp_session() })
}

/// rdp:setup: the task made (directly, else through the UAC prompt).
pub fn setup() -> Value {
    if !cfg!(target_os = "windows") { return json!({ "ok": false, "error": "Windows only" }); }
    match create_task() { Ok(()) => json!({ "ok": true }), Err(e) => json!({ "ok": false, "error": e }) }
}

/// An RDP session is named "RDP-Tcp#N"; the console is "Console".
pub fn is_rdp_session() -> bool { std::env::var("SESSIONNAME").is_ok_and(|s| s.starts_with("RDP-Tcp")) }

/// The batch file the task runs: the server's scripts/ beside the app (the
/// contract's install layout), else the repo's own when run from a build.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn bat_path() -> std::path::PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    let mut tried = vec![exe_dir.join("server").join("scripts").join(BAT_FILENAME)];
    if let Ok(d) = std::env::var("CRUNDI_SERVER_DIR") { tried.insert(0, std::path::PathBuf::from(d).join("scripts").join(BAT_FILENAME)); }
    tried.iter().find(|p| p.is_file()).cloned().unwrap_or_else(|| tried.last().cloned().unwrap_or_default())
}

#[cfg(target_os = "windows")]
fn cmd(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt as _;
    let mut c = std::process::Command::new(program);
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c
}

#[cfg(target_os = "windows")]
fn is_task_installed() -> bool {
    cmd("schtasks").args(["/query", "/tn", TASK_NAME]).output().is_ok_and(|o| o.status.success())
}
#[cfg(not(target_os = "windows"))]
fn is_task_installed() -> bool { false }

#[cfg(target_os = "windows")]
fn create_task() -> Result<(), String> {
    let bat = bat_path();
    if !bat.is_file() { return Err(format!("Batch file not found: {}", bat.display())); }
    // A far-future dummy schedule (never runs by itself), with admin rights when triggered.
    let tr = format!("\"{}\"", bat.display());
    let args = ["/create", "/tn", TASK_NAME, "/tr", tr.as_str(), "/sc", "once", "/sd", "01/01/2099", "/st", "00:00", "/rl", "highest", "/f"];
    if cmd("schtasks").args(args).output().is_ok_and(|o| o.status.success()) { return Ok(()); }
    // Access denied: the same command elevated (UAC), then checked.
    let tmp = std::env::temp_dir().join(format!("create-disconnect-rdp-task-{}.bat", std::process::id()));
    let line = format!("schtasks /create /tn \"{TASK_NAME}\" /tr \"\\\"{}\\\"\" /sc once /sd 01/01/2099 /st 00:00 /rl highest /f", bat.display());
    std::fs::write(&tmp, format!("@echo off\r\n{line}\r\nexit /b %ERRORLEVEL%\r\n")).map_err(|e| e.to_string())?;
    let ps = format!("Start-Process -FilePath '{}' -Verb RunAs -Wait -WindowStyle Hidden", tmp.display().to_string().replace('\'', "''"));
    let r = cmd("powershell").args(["-NoProfile", "-Command", &ps]).output();
    let _ = std::fs::remove_file(&tmp);
    match r {
        Ok(_) if is_task_installed() => Ok(()),
        Ok(o) if !o.status.success() => Err(String::from_utf8_lossy(&o.stderr).trim().chars().take(200).collect()),
        Ok(_) => Err("UAC elevation completed but task was not created. Check Event Viewer or run as admin manually.".into()),
        Err(e) => Err(e.to_string()),
    }
}
#[cfg(not(target_os = "windows"))]
fn create_task() -> Result<(), String> { Err("Windows only".into()) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_off_windows() {
        if cfg!(target_os = "windows") { return; }
        assert_eq!(status(), json!({ "ok": true, "supported": false, "installed": false, "isRdp": false }));
        assert_eq!(setup(), json!({ "ok": false, "error": "Windows only" }));
        assert!(bat_path().ends_with("disconnect-rdp.bat"));
    }
}
