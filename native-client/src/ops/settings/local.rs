//! Settings kept on this PC, not on the server: the terminal's newline key
//! (the web keeps it in localStorage), the Desktop App section's choices,
//! and launching at startup (the system's own place for it: an autostart
//! entry on Linux, a LaunchAgent on macOS, the Run key on Windows).
//! Under the test driver the autostart entry goes in the client's config
//! folder, never the real one.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Local {
    /// "shift" (default) or "ctrl".
    #[serde(default)]
    pub newline: String,
    #[serde(default)]
    pub auto_update: Option<bool>,
    /// "production" (default) or "dev".
    #[serde(default)]
    pub channel: String,
}

impl Local {
    pub fn newline_key(&self) -> String { if self.newline == "ctrl" { "ctrl".into() } else { "shift".into() } }
}

fn file() -> PathBuf { crate::store::config_dir().join("ops-settings.json") }

pub fn load() -> Local {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn update(f: impl FnOnce(&mut Local)) {
    let mut l = load();
    f(&mut l);
    let p = file();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    if let Ok(t) = serde_json::to_string_pretty(&l) { let _ = std::fs::write(p, t); }
}

fn testing() -> bool { std::env::var_os("CRUNDI_TEST_PORT").is_some() }

fn home() -> PathBuf { std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default() }

/// The hint under "Launch at startup" (the web's names Windows).
pub fn launch_hint() -> String {
    let os = if cfg!(target_os = "windows") { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "you sign in" };
    if cfg!(any(target_os = "windows", target_os = "macos")) { format!("Start Crundi in the tray when {os} starts.") } else { format!("Start Crundi in the tray when {os}.") }
}

/// The autostart file (Linux, macOS; and every system under the test driver).
fn entry() -> PathBuf {
    if testing() { return crate::store::config_dir().join("autostart").join("crundi-native.desktop"); }
    if cfg!(target_os = "macos") { return home().join("Library/LaunchAgents/com.crundi.native.plist"); }
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"));
    base.join("autostart").join("crundi-native.desktop")
}

fn exe() -> String { std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default() }

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Whether the client starts with the system.
pub fn launch() -> bool {
    if cfg!(target_os = "windows") && !testing() {
        return std::process::Command::new("reg").args(["query", RUN_KEY, "/v", "Crundi Native"]).output().map(|o| o.status.success()).unwrap_or(false);
    }
    entry().exists()
}

pub fn set_launch(on: bool) -> Result<(), String> {
    if cfg!(target_os = "windows") && !testing() {
        let mut c = std::process::Command::new("reg");
        if on { c.args(["add", RUN_KEY, "/v", "Crundi Native", "/t", "REG_SZ", "/d", &format!("\"{}\"", exe()), "/f"]); }
        else { c.args(["delete", RUN_KEY, "/v", "Crundi Native", "/f"]); }
        return match c.output() { Ok(o) if o.status.success() => Ok(()), _ => Err("Could not change the startup setting".into()) };
    }
    let p = entry();
    if !on { return match std::fs::remove_file(&p) { Ok(()) => Ok(()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(e) => Err(e.to_string()) }; }
    if let Some(d) = p.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
    let text = if cfg!(target_os = "macos") && !testing() {
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>com.crundi.native</string><key>ProgramArguments</key><array><string>{}</string></array><key>RunAtLoad</key><true/></dict></plist>\n", exe())
    } else {
        format!("[Desktop Entry]\nType=Application\nName=Crundi\nExec=\"{}\"\nX-GNOME-Autostart-enabled=true\n", exe())
    };
    std::fs::write(&p, text).map_err(|e| e.to_string())
}
