//! The worker's config file: `<config dir>/crundi-worker/config.json`.
//!
//! Field names are camelCase, as in the spec:
//! `{ server, id, key, name, workDir, allowChats, allowCommands }`, plus
//! `deviceId` (made once, sent on join and hello) and `paused` (the last
//! pause state, from the server or the tray). Unknown fields are kept, so a
//! newer worker's config survives an older one saving it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_work_dir")]
    pub work_dir: PathBuf,
    #[serde(default = "yes")]
    pub allow_chats: bool,
    #[serde(default = "yes")]
    pub allow_commands: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// Fields this version does not know, kept as they were.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn yes() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: None,
            id: None,
            key: None,
            name: None,
            work_dir: default_work_dir(),
            allow_chats: true,
            allow_commands: true,
            paused: false,
            device_id: None,
            extra: Default::default(),
        }
    }
}

/// `~/crundi-work` (or `./crundi-work` when there is no home folder).
pub fn default_work_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crundi-work")
}

/// `<config dir>/crundi-worker/config.json`.
pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crundi-worker")
        .join("config.json")
}

impl Config {
    /// Load the config, or the defaults when the file does not exist yet.
    pub fn load(path: &Path) -> Result<Config> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("reading {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Save atomically (write a temporary file, then rename). The file holds
    /// the worker key, so on Unix it is made readable by the owner only.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating {}", dir.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&tmp, text + "\n").with_context(|| format!("writing {}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, path).with_context(|| format!("saving {}", path.display()))?;
        Ok(())
    }

    /// Joined means there is a server and a key.
    pub fn joined(&self) -> bool {
        self.server.is_some() && self.key.is_some()
    }

    /// The device id, made on first use.
    pub fn ensure_device_id(&mut self) -> String {
        if self.device_id.is_none() {
            self.device_id = Some(crate::util::random_hex(16));
        }
        self.device_id.clone().unwrap()
    }

    /// Create the work folder if it is missing.
    pub fn ensure_work_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.work_dir)
            .with_context(|| format!("creating the work folder {}", self.work_dir.display()))
    }

    /// The name to use: the saved one, else the host name.
    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(crate::util::hostname)
    }
}
