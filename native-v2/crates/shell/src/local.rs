//! Local mode's state for the window (app/main.js startBot and the screens
//! of app/index.html): whether this install carries a server (the
//! all-in-one), whether it runs it (client.json's mode), the server's status
//! and log (the status screen), and the first-run setup wizard. The Desktop
//! area runs the server (crundi_desktop::local) and keeps this entity up to
//! date; the sign-in view shows the status screen or the wizard from it
//! (local_ui.rs) while the local server is not running and signed in to.
//!
//! The client's own choice of server, `client.json` in the client's config
//! folder ({mode: "local" | "remote", serverUrl}), is read and written here
//! (Electron's readClientConfig / writeClientConfig); the server's address
//! for remote mode stays the session's (core::store).

use gpui::*;
use serde_json::{json, Value};

/// What the window shows of local mode (one entity, notified on a change).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Local {
    /// This install carries a server (`server/src/index.js`: the all-in-one).
    pub bundled: bool,
    /// It runs that server (bundled, and client.json's mode is not remote).
    pub active: bool,
    /// startBot's status: "stopped", "starting", "running" or "error".
    pub status: String,
    /// The status screen's log (Electron's botLogs, the last lines).
    pub logs: Vec<String>,
    /// The first-run setup wizard is open (no .env yet).
    pub wizard: bool,
}

/// The status line's words (index.html updateStatus, the tray's statusLabels).
pub fn status_label(status: &str) -> &'static str {
    match status { "stopped" => "Stopped", "starting" => "Starting...", "running" => "Running", "error" => "Error", _ => "Unknown" }
}

/// The wizard's answers (index.html saveSetup, main.js setup:save).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Setup {
    pub bot_token: String,
    pub username: String,
    pub projects_dir: String,
    pub tunnel_token: String,
    pub tunnel_url: String,
    pub auto_update: bool,
    pub auto_launch: bool,
    /// "Restore from a backup" instead: the server starts with nothing set up.
    pub restore: bool,
}

/// What the screens ask of the Desktop area (set at its init).
#[derive(Clone, Copy)]
pub struct Hooks {
    pub start: fn(),
    pub stop: fn(),
    pub restart: fn(),
    pub save_setup: fn(Setup),
    /// The projects folder picker (setup:browse): the folder, or None.
    pub browse: fn() -> Option<String>,
}
struct HooksHandle(Hooks);
impl Global for HooksHandle {}
struct LocalHandle(Entity<Local>);
impl Global for LocalHandle {}
/// The handle for readers with `&App` (the driver's state).
pub(crate) struct LocalHandleRead(pub Entity<Local>);
impl Global for LocalHandleRead {}

/// The entity (made on first use).
pub fn local(cx: &mut App) -> Entity<Local> {
    if let Some(h) = cx.try_global::<LocalHandle>() { return h.0.clone(); }
    let e = cx.new(|_| Local { status: "stopped".into(), ..Default::default() });
    cx.set_global(LocalHandle(e.clone()));
    cx.set_global(LocalHandleRead(e.clone()));
    e
}

/// Change it (notifies only on a change).
pub fn update(cx: &mut App, f: impl FnOnce(&mut Local)) {
    let e = local(cx);
    e.update(cx, |l, cx| { let before = l.clone(); f(l); if *l != before { cx.notify(); } });
}

pub fn set_hooks(cx: &mut App, h: Hooks) { cx.set_global(HooksHandle(h)); }
pub fn hooks(cx: &App) -> Option<Hooks> { cx.try_global::<HooksHandle>().map(|h| h.0) }

/// The sign-in page's restore form (Electron's `?restore=1` after the
/// wizard's "Restore from a backup"), for the server at `base`.
pub fn open_restore(cx: &mut App, base: &str) {
    if let Some((_, open, _)) = crate::slots::restore(cx) { open(cx, base); }
}

// ─── client.json (the client's own) ───

fn client_file() -> std::path::PathBuf { crundi_core::store::config_dir().join("client.json") }

/// client.json as kept (an object; {} when there is none).
pub fn client_raw() -> Value {
    std::fs::read(client_file()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).filter(|v| v.is_object()).unwrap_or_else(|| json!({}))
}

/// (mode, serverUrl): readClientConfig. A build without a server is always
/// remote; with one, local unless client.json says remote.
pub fn client_config(bundled: bool) -> (String, String) {
    let v = client_raw();
    let remote = v["mode"] == "remote" || !bundled;
    (if remote { "remote" } else { "local" }.into(), v["serverUrl"].as_str().unwrap_or("").to_string())
}

/// writeClientConfig: the mode and the address (no trailing slashes); the
/// file's other keys are kept.
pub fn write_client_config(mode: &str, server_url: &str) -> bool {
    let mut v = client_raw();
    v["mode"] = json!(if mode == "remote" { "remote" } else { "local" });
    v["serverUrl"] = json!(server_url.trim_end_matches('/'));
    let p = client_file();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap_or_default()).is_ok()
}

/// Mini, the chat mascot, on this device (client.json "mini"; on unless
/// turned off in Settings > Desktop App). Read once, then kept in memory.
static MINI: std::sync::OnceLock<std::sync::atomic::AtomicBool> = std::sync::OnceLock::new();
fn mini_flag() -> &'static std::sync::atomic::AtomicBool {
    MINI.get_or_init(|| std::sync::atomic::AtomicBool::new(client_raw()["mini"].as_bool().unwrap_or(true)))
}
pub fn mini_on() -> bool { mini_flag().load(std::sync::atomic::Ordering::Relaxed) }
/// Saved with client.json's other keys kept.
pub fn set_mini(on: bool) -> bool {
    mini_flag().store(on, std::sync::atomic::Ordering::Relaxed);
    let mut v = client_raw();
    v["mini"] = json!(on);
    let p = client_file();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap_or_default()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{client_config, client_raw, status_label, write_client_config};

    #[test]
    fn the_status_words() {
        assert_eq!(status_label("starting"), "Starting...");
        assert_eq!(status_label("running"), "Running");
        assert_eq!(status_label("error"), "Error");
        assert_eq!(status_label("stopped"), "Stopped");
    }

    #[test]
    fn client_json_as_electron_keeps_it() {
        let d = std::env::temp_dir().join(format!("crundi-local-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        // SAFETY: this test alone sets it in this process's shell tests.
        unsafe { std::env::set_var("CRUNDI_CONFIG_DIR", &d); }
        std::fs::write(d.join("client.json"), r#"{"voiceWake":true}"#).unwrap();
        assert_eq!(client_config(true), ("local".into(), "".into()));
        assert_eq!(client_config(false).0, "remote");
        assert!(write_client_config("remote", "https://a.example/"));
        assert_eq!(client_config(true), ("remote".into(), "https://a.example".into()));
        assert_eq!(client_raw()["voiceWake"], true);
        let _ = std::fs::remove_dir_all(&d);
    }
}
