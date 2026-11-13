//! What the client remembers between runs: the server, the login's refresh
//! token and who signed in. The access token is never written down (it lasts
//! 15 minutes; a refresh gets a new one at start). Kept in the user's config
//! folder, readable by the user only.
//!
//!   Linux     $XDG_CONFIG_HOME/crundi-native (~/.config/crundi-native)
//!   Windows   %APPDATA%\Crundi Native
//!   macOS     ~/Library/Application Support/Crundi Native
//!   CRUNDI_CONFIG_DIR overrides all three (tests).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Saved {
    pub server: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub username: String,
    /// The project and chat open when the client was closed.
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub chat: String,
    /// The workbench layout of each project (src/mosaic.rs), as the web UI keeps
    /// it in localStorage.crundi_wb_mosaic.
    #[serde(default)]
    pub layouts: std::collections::HashMap<String, serde_json::Value>,
}

pub fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("CRUNDI_CONFIG_DIR") {
        return d.into();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(home).join("Crundi Native")
    } else if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support").join("Crundi Native")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join("crundi-native")
    }
}

fn file() -> PathBuf {
    config_dir().join("session.json")
}

pub fn load() -> Saved {
    std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save(s: &Saved) {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = file();
    let tmp = path.with_extension("tmp");
    let Ok(text) = serde_json::to_string_pretty(s) else { return };
    if std::fs::write(&tmp, text).is_err() { return; }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    let _ = std::fs::rename(&tmp, &path);
}

/// Change what is saved: load, edit, save.
pub fn update(f: impl FnOnce(&mut Saved)) {
    let mut s = load();
    f(&mut s);
    save(&s);
}
