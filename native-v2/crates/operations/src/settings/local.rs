//! Settings kept on this PC, not on the server (the Slint client's
//! settings/local.rs): the terminal's newline key (the web keeps it in
//! localStorage). The Desktop App section's choices (automatic updates,
//! the channel, launching at startup) are crundi_shell::lifecycle's, kept
//! where Electron kept them.

use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Local {
    /// "shift" (default) or "ctrl".
    pub newline: String,
    pub auto_update: Option<bool>,
    /// "production" (default) or "dev".
    pub channel: String,
}

impl Local {
    pub fn newline_key(&self) -> String { if self.newline == "ctrl" { "ctrl".into() } else { "shift".into() } }

    pub fn from_json(v: &Value) -> Local {
        Local {
            newline: v.get("newline").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            auto_update: v.get("auto_update").and_then(|x| x.as_bool()),
            channel: v.get("channel").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({ "newline": self.newline, "auto_update": self.auto_update, "channel": self.channel })
    }
}

fn file() -> PathBuf { crundi_core::store::config_dir().join("ops-settings.json") }

/// The newline key lives in the client's saved settings (core's
/// `Saved.term_newline`, which the terminals read: F147); one set before
/// that, in this file, moves there the first time it is read.
pub fn load() -> Local {
    let mut l = std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).map(|v| Local::from_json(&v)).unwrap_or_default();
    let saved = crundi_core::store::load().term_newline;
    if !saved.is_empty() { l.newline = saved; }
    else if !l.newline.is_empty() { let n = l.newline.clone(); crundi_core::store::update(|s| s.term_newline = n); }
    l
}

pub fn update(f: impl FnOnce(&mut Local)) {
    let mut l = load();
    let was = l.newline.clone();
    f(&mut l);
    if l.newline != was { let n = l.newline.clone(); crundi_core::store::update(|s| s.term_newline = n); }
    let p = file();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    if let Ok(t) = serde_json::to_string_pretty(&l.to_json()) { let _ = std::fs::write(p, t); }
}

/// The hint under "Launch at startup" (the web's names Windows).
pub fn launch_hint() -> String {
    let os = if cfg!(target_os = "windows") { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "you sign in" };
    if cfg!(any(target_os = "windows", target_os = "macos")) { format!("Start Crundi in the tray when {os} starts.") } else { format!("Start Crundi in the tray when {os}.") }
}

/// Whether the client starts with the system (one implementation:
/// crundi_shell::lifecycle, Electron's names).
pub fn launch() -> bool { crundi_shell::lifecycle::launch_enabled() }

pub fn set_launch(on: bool) -> Result<(), String> { crundi_shell::lifecycle::set_launch(on).map(|_| ()) }

/// Automatic updates and the channel live in update-config.json, as Electron's.
pub fn auto_update() -> bool { crundi_shell::lifecycle::auto_update() }
pub fn channel() -> &'static str { crundi_shell::lifecycle::channel() }

#[cfg(test)]
mod tests {
    use super::Local;

    #[::core::prelude::v1::test]
    fn round_trip() {
        let l = Local { newline: "ctrl".into(), auto_update: Some(false), channel: "dev".into() };
        assert_eq!(Local::from_json(&l.to_json()), l);
        assert_eq!(Local::default().newline_key(), "shift");
        assert_eq!(l.newline_key(), "ctrl");
    }
}
