//! Trading a join code for a worker key (spec: "Joining", step 2).

use crate::config::Config;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRequest {
    pub code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// "tray" | "headless" | "electron" | "native"
    pub kind: String,
    pub version: String,
    pub os: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JoinReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// The accepted join: what the server gave back.
#[derive(Debug, Clone, PartialEq)]
pub struct Joined {
    pub id: String,
    pub key: String,
    pub name: String,
}

/// Join codes are 8 characters of `ABCDEFGHJKMNPQRSTUVWXYZ23456789`; accept
/// lower case, spaces and dashes as typed.
pub fn clean_code(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// POST {server}/api/workers/join.
pub fn request(server: &str, req: &JoinRequest) -> Result<Joined> {
    crate::util::install_crypto();
    let url = crate::util::join_url(server)?;
    let mut resp = ureq::post(&url)
        .config()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .header("Content-Type", "application/json")
        .send_json(req)
        .with_context(|| format!("could not reach {url}"))?;
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    let reply: Option<JoinReply> = serde_json::from_str(&text).ok();
    if !(200..300).contains(&status) {
        let msg = reply
            .and_then(|r| r.error)
            .unwrap_or_else(|| format!("the server answered {status}"));
        bail!("join refused: {msg}");
    }
    let reply = reply.ok_or_else(|| anyhow!("the server's answer was not JSON (is this a Crundi server?)"))?;
    if !reply.ok {
        bail!("join refused: {}", reply.error.unwrap_or_else(|| "unknown error".into()));
    }
    let id = reply.id.ok_or_else(|| anyhow!("the answer had no id"))?;
    let key = reply.key.ok_or_else(|| anyhow!("the answer had no key"))?;
    if !key.starts_with("cw_") {
        bail!("the answer's key is not a worker key");
    }
    Ok(Joined { id, key, name: reply.name.unwrap_or(req.name.clone()) })
}

pub struct JoinArgs<'a> {
    pub server: &'a str,
    pub code: &'a str,
    pub name: Option<&'a str>,
    pub work_dir: Option<PathBuf>,
    pub kind: &'a str,
}

/// Join and save `{ server, id, key, name, workDir }` into the config file.
/// Other saved settings (allow switches, device id) are kept.
pub fn join_and_save(config_path: &Path, args: JoinArgs) -> Result<Config> {
    let mut cfg = Config::load(config_path)?;
    let server = crate::util::normalize_server(args.server)?;
    let code = clean_code(args.code);
    if code.is_empty() {
        bail!("no join code");
    }
    if let Some(wd) = args.work_dir {
        cfg.work_dir = absolute(&wd);
    }
    cfg.ensure_work_dir()?;
    let name = args
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| cfg.display_name());
    let device_id = cfg.ensure_device_id();
    let joined = request(
        &server,
        &JoinRequest {
            code,
            name,
            device_id: Some(device_id),
            kind: args.kind.to_string(),
            version: crate::util::VERSION.to_string(),
            os: crate::util::os_name().to_string(),
        },
    )?;
    cfg.server = Some(server);
    cfg.id = Some(joined.id);
    cfg.key = Some(joined.key);
    cfg.name = Some(joined.name);
    cfg.paused = false;
    cfg.save(config_path)?;
    Ok(cfg)
}

/// Make a path absolute (relative to the current folder), expanding `~/`.
pub fn absolute(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        if let Some(h) = dirs::home_dir() {
            return h.join(rest);
        }
    }
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf())
    }
}
