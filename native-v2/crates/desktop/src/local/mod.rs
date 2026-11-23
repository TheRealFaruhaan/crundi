//! Local mode, the edition, the first-run wizard and moving an Electron
//! install over (package/CONTRACT.md; app/main.js startBot, setup:save,
//! client.json, homepool-node.json, worker.json, drawer-history.json).
//!
//! - The edition: with `<exe dir>/server/src/index.js` (or
//!   `CRUNDI_SERVER_DIR`) the install is the all-in-one and runs that server
//!   (unless client.json says remote); otherwise it is the client.
//! - The server's data and .env stay in Electron's folder (`user_data`).
//! - On the first start the client takes, once, Electron's client.json,
//!   update-config.json, homepool-node.json (its deviceId too),
//!   drawer-history.json and worker.json (`import`).

pub mod server;
pub mod worker;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The folder the client runs from.
pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default()
}

/// The server folder (`CRUNDI_SERVER_DIR` for tests), when it has the server.
pub fn server_dir() -> Option<PathBuf> {
    let d = std::env::var_os("CRUNDI_SERVER_DIR").map(PathBuf::from).unwrap_or_else(|| exe_dir().join("server"));
    d.join("src").join("index.js").is_file().then_some(d)
}

/// serverBundled: the all-in-one.
pub fn bundled() -> bool { server_dir().is_some() }

/// The bundled Node (`CRUNDI_NODE` for tests): `node/bin/node` (Linux),
/// `node\node.exe` (Windows); else the system's `node`.
pub fn node() -> (PathBuf, bool) {
    if let Some(n) = std::env::var_os("CRUNDI_NODE") { return (n.into(), false); }
    let b = if cfg!(windows) { exe_dir().join("node").join("node.exe") } else { exe_dir().join("node").join("bin").join("node") };
    if b.is_file() { (b, true) } else { ("node".into(), false) }
}

fn config_home() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
    if cfg!(windows) { std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming")) }
    else if cfg!(target_os = "macos") { home.join("Library").join("Application Support") }
    else { std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")) }
}

/// Electron's userData (CONTRACT.md "Where things live"): the server's .env
/// and data, and where the client imports from.
pub fn user_data_for(bundled: bool) -> PathBuf {
    let name = if !bundled { "Crundi Client" } else if cfg!(target_os = "linux") { "crundi-desktop" } else { "Crundi" };
    config_home().join(name)
}
pub fn user_data() -> PathBuf { user_data_for(bundled()) }
pub fn env_path() -> PathBuf { user_data().join(".env") }
pub fn data_dir() -> PathBuf { user_data().join("data") }

/// Dev mode (Electron's `--dev`): port 8889, no Cloudflare.
fn dev() -> bool { std::env::args().any(|a| a == "--dev") || std::env::var("CRUNDI_DEV").is_ok_and(|v| v == "1") }

// ─── The setup wizard's .env (setup:save) ───

/// The .env setup:save writes.
pub fn env_text(s: &crundi_shell::local::Setup) -> String {
    let port = if s.tunnel_token.is_empty() { "0" } else { "8888" };
    [
        "# \u{2500}\u{2500}\u{2500} Crundi Configuration \u{2500}\u{2500}\u{2500}".to_string(),
        format!("TELEGRAM_BOT_TOKEN={}", s.bot_token),
        format!("ALLOWED_USERNAME={}", s.username),
        format!("PROJECTS_DIR={}", s.projects_dir),
        format!("WEB_PORT={port}"),
        format!("CLOUDFLARE_TUNNEL_TOKEN={}", s.tunnel_token),
        format!("CLOUDFLARE_TUNNEL_URL={}", s.tunnel_url),
        "DATA_DIR=".to_string(),
    ].join("\n")
}

fn save_setup(s: crundi_shell::local::Setup) {
    let p = env_path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    if let Err(e) = std::fs::write(&p, env_text(&s)) { server::log(&format!("[setup] could not save .env: {e}")); }
    if s.restore { server::with(|st| st.open_restore = true); }
    // The wizard's consent: update checks and launching at startup.
    crundi_shell::lifecycle::first_run_defaults(s.auto_update, s.auto_launch);
    server::log(".env saved \u{2014} starting Crundi...");
    start();
}

// ─── Moving an Electron install over ───

/// Copy `name` from Electron's folder into the client's, if the client has none.
fn take(from: &Path, to: &Path, name: &str) -> bool {
    let (f, t) = (from.join(name), to.join(name));
    if !f.is_file() || t.exists() { return false; }
    std::fs::copy(&f, &t).is_ok_and(|_| {
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&t, std::fs::Permissions::from_mode(0o600)); }
        true
    })
}

/// The one-time import (CONTRACT.md): what was taken.
pub fn import_from(electron: &Path, cfg: &Path) -> Vec<String> {
    let marker = cfg.join("electron-import.json");
    if marker.exists() || !electron.is_dir() { return vec![]; }
    let _ = std::fs::create_dir_all(cfg);
    let mut took = vec![];
    for n in ["update-config.json", "homepool-node.json", "drawer-history.json", "worker.json"] { if take(electron, cfg, n) { took.push(n.to_string()); } }
    // The device's id (hpDeviceId), so the server knows this PC as before.
    let hp: Value = std::fs::read(electron.join("homepool-node.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    if let Some(id) = hp["deviceId"].as_str().filter(|s| s.starts_with("pc-")) {
        if std::fs::write(cfg.join("device-id"), id).is_ok() { took.push("device-id".into()); }
    }
    // client.json (in Electron's data folder): mode, serverUrl, voiceWake.
    let cj: Value = std::fs::read(electron.join("data").join("client.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    if cj.is_object() {
        let p = cfg.join("client.json");
        if !p.exists() {
            let url = cj["serverUrl"].as_str().unwrap_or("").trim_end_matches('/').to_string();
            let mode = if cj["mode"] == "remote" { "remote" } else { "local" };
            let _ = std::fs::write(&p, serde_json::to_string_pretty(&json!({ "mode": mode, "serverUrl": url })).unwrap_or_default());
            took.push("client.json".into());
            // The server to connect to, for remote mode (the session's).
            if mode == "remote" && !url.is_empty() {
                let sp = cfg.join("session.json");
                let mut s: Value = std::fs::read(&sp).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}));
                if s["server"].as_str().unwrap_or("").is_empty() { s["server"] = json!(url); let _ = std::fs::write(&sp, s.to_string()); }
            }
        }
        if let Some(on) = cj["voiceWake"].as_bool() {
            let d = cfg.join("voice-wake");
            if !d.join("settings.json").exists() && std::fs::create_dir_all(&d).is_ok() && std::fs::write(d.join("settings.json"), json!({ "enabled": on }).to_string()).is_ok() { took.push("voiceWake".into()); }
        }
    }
    let _ = std::fs::write(&marker, json!({ "from": electron, "took": took, "at": chrono::Utc::now().to_rfc3339() }).to_string());
    took
}

// ─── Local mode ───

/// Set up at the desktop's init (before the session starts): the import,
/// the edition and mode, the shell's hooks; local mode starts its server.
pub fn init(cx: &mut gpui::App) {
    let took = import_from(&user_data(), &crundi_core::store::config_dir());
    if !took.is_empty() { eprintln!("[lifecycle] Took from Electron's settings: {}", took.join(", ")); }
    let b = bundled();
    let active = b && crundi_shell::local::client_config(b).0 == "local";
    crundi_shell::local::set_hooks(cx, crundi_shell::local::Hooks { start, stop, restart, save_setup, browse });
    crundi_shell::local::update(cx, |l| { l.bundled = b; l.active = active; });
    if active {
        let s = crundi_shell::stores(cx);
        s.session.update(cx, |s, _| s.hold = true);
        start();
    }
    worker::init();
    // Quit leaves nothing running: the server (shutdown, 6 s, the tree) and the worker.
    cx.on_app_quit(|_| { quit(); async {} }).detach();
}

/// Is local mode on (the all-in-one running its own server)?
pub fn active() -> bool { ACTIVE.load(std::sync::atomic::Ordering::Relaxed) }
static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn browse() -> Option<String> {
    rfd::FileDialog::new().set_title("Select Projects Directory").pick_folder().map(|p| p.to_string_lossy().to_string())
}

/// startBot.
pub fn start() {
    ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
    let busy = server::with(|s| s.pid != 0 || s.status == "starting");
    if busy { return; }
    let Some(dir) = server_dir() else { return };
    if !env_path().exists() {
        server::log("No .env found \u{2014} opening setup wizard...");
        WIZARD.store(true, std::sync::atomic::Ordering::Relaxed);
        crundi_webview::wake();
        return;
    }
    server::with(|s| { s.user_stopped = false; s.status = "starting"; });
    server::log("Starting Crundi...");
    let (node, is_bundled) = node();
    let nd = if is_bundled { node.parent().map(Path::to_path_buf) } else { None };
    server::spawn(server::Spawn { node, server_dir: dir, env_path: env_path(), data_dir: data_dir(), device_id: crate::host::device_id(), dev: dev(), bundled_node_dir: nd });
    crundi_webview::wake();
}
static WIZARD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// stopBot.
pub fn stop() {
    let pid = server::with(|s| s.pid);
    if pid == 0 { server::with(|s| s.status = "stopped"); crundi_webview::wake(); return; }
    server::with(|s| s.user_stopped = true);
    server::log("Stopping...");
    std::thread::spawn(|| server::stop_blocking(Duration::from_secs(6)));
}

/// bot:restart: stop, then start again.
pub fn restart() {
    let pid = server::with(|s| s.pid);
    if pid == 0 { return start(); }
    server::with(|s| s.restart_requested = true);
    std::thread::spawn(|| server::stop_blocking(Duration::from_secs(6)));
}

/// gracefulShutdownAndExit's part: the server stopped and its tree gone.
pub fn quit() {
    server::with(|s| s.quitting = true);
    server::log("[lifecycle] Quitting \u{2014} stopping services, terminals, and tunnels...");
    worker::stop_blocking();
    server::stop_blocking(Duration::from_secs(6));
}

/// What the threads saw, acted on (the desktop's pump, with the App).
pub fn pump(cx: &mut gpui::App) {
    let evs = server::take();
    let wiz = WIZARD.swap(false, std::sync::atomic::Ordering::Relaxed);
    worker::pump();
    for r in worker::take_joined() {
        match r { Ok(()) => crate::ctx::toast_ok("This PC is now a worker"), Err(e) => crate::ctx::toast_error(&e) }
    }
    if evs.is_empty() && !wiz { return sync(cx); }
    for e in evs {
        match e {
            server::Ev::Line(l) => {
                server::log(&l);
                let (port, key, ready) = server::parse_line(&l);
                if let Some(p) = port { server::with(|s| s.port = Some(p)); }
                if let Some(k) = key { server::with(|s| s.key = k); }
                if ready { ready_now(cx); }
            }
            server::Ev::Ipc(v) => {
                // The server wants a fresh start (to apply a restored backup).
                if v["type"] == "restart-server" && server::with(|s| s.pid != 0) {
                    server::with(|s| s.restart_requested = true);
                    server::terminate();
                }
            }
            server::Ev::SpawnFailed(_) => server::with(|s| { s.status = "error"; s.pid = 0; }),
            server::Ev::Exit { run, code, signal } => exited(run, code, signal, cx),
        }
    }
    if wiz { crundi_shell::local::update(cx, |l| l.wizard = true); }
    sync(cx);
}

/// "[crundi] Ready": running; the window signs in with the server's key.
fn ready_now(_cx: &mut gpui::App) {
    let (port, key, restore) = server::with(|s| { s.status = "running"; let r = s.open_restore; s.open_restore = false; (s.port, s.key.clone(), r) });
    let Some(port) = port else { return };
    let base = format!("http://localhost:{port}");
    server::log(&format!("[lifecycle] Navigating to {base}"));
    // Electron loads the server's page, which asks /api/auth/methods first: a
    // server with no sign-in method yet shows its first-run setup page
    // (#login-setup), not the app. Asked off the window's thread.
    std::thread::spawn(move || {
        let setup = crundi_core::api::methods(&base).ok().and_then(|m| m.get("setupRequired").and_then(|v| v.as_bool())).unwrap_or(false);
        crate::ctx::main_thread(move || crate::ctx::on_app(move |_, cx| {
            let session = crundi_shell::stores(cx).session;
            if restore || setup {
                // The sign-in page's first-run form (on its restore, when restoring).
                if setup && !restore { server::log("[lifecycle] No sign-in method set up yet: the setup page"); }
                session.update(cx, |s, cx| { s.hold = false; s.check_server(&base, cx) });
                if restore {
                    let b = base.clone();
                    crate::web::later(Duration::from_millis(1500), move || crate::ctx::on_app(move |_, cx| {
                        crundi_shell::local::open_restore(cx, &b);
                    }));
                }
                crundi_shell::local::update(cx, |l| l.active = false);
                RESTORE_SHOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            session.update(cx, |s, cx| s.sign_in_local(base, key, cx));
        }));
    });
}
static RESTORE_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn exited(run: u64, code: Option<i32>, signal: Option<i32>, cx: &mut gpui::App) {
    let current = server::with(|s| s.run == run);
    if !current { return; }
    server::log(&format!("[electron] Process exited: code={} signal={}", code.map(|c| c.to_string()).unwrap_or("null".into()), signal.map(|c| c.to_string()).unwrap_or("null".into())));
    let (quitting, restart, user) = server::with(|s| { s.pid = 0; s.port = None; let r = (s.quitting, s.restart_requested, s.user_stopped); s.restart_requested = false; s.user_stopped = false; r });
    if quitting { return; }
    if restart {
        server::log("[electron] Restarting the server as it asked...");
        server::with(|s| s.status = "starting");
        crate::web::later(Duration::from_millis(400), || { server::with(|s| s.status = "stopped"); start(); });
        return;
    }
    // Back to the status screen (Electron's loadFile(index.html)).
    if RESTORE_SHOWN.swap(false, std::sync::atomic::Ordering::Relaxed) { crundi_shell::local::update(cx, |l| l.active = true); }
    let session = crundi_shell::stores(cx).session;
    session.update(cx, |s, cx| { s.hold = true; s.end_local(cx) });
    if user || code == Some(0) { server::with(|s| s.status = "stopped"); }
    else { server::with(|s| s.status = "error"); server::log("Process crashed \u{2014} check logs above. Click Start to retry."); }
}

/// The shell's copy (status, log) and the tray follow.
fn sync(cx: &mut gpui::App) {
    let (status, logs) = server::with(|s| (s.status, s.logs.iter().cloned().collect::<Vec<_>>()));
    if !active() { return; }
    crundi_shell::local::update(cx, |l| { l.status = status.into(); if l.logs.len() != logs.len() || l.logs.last() != logs.last() { l.logs = logs; } });
    crate::tray::set_server(status);
}

/// Something for the pump.
pub fn waiting() -> bool { server::waiting() || WIZARD.load(std::sync::atomic::Ordering::Relaxed) || worker::waiting() }

/// The driver's look (the state's `areas.desktop.local`).
pub fn props() -> Value {
    let (status, pid, port, run, n) = server::with(|s| (s.status, s.pid, s.port, s.run, s.logs.len()));
    json!({
        "bundled": bundled(), "active": active(), "status": status, "pid": pid, "port": port, "starts": run, "logs": n,
        "serverDir": server_dir(), "userData": user_data(), "envPath": env_path(), "envExists": env_path().exists(),
        "worker": worker::status(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_env_setup_save_writes() {
        let s = crundi_shell::local::Setup { bot_token: "1:a".into(), username: "me".into(), projects_dir: "/p".into(), ..Default::default() };
        assert_eq!(env_text(&s), "# \u{2500}\u{2500}\u{2500} Crundi Configuration \u{2500}\u{2500}\u{2500}\nTELEGRAM_BOT_TOKEN=1:a\nALLOWED_USERNAME=me\nPROJECTS_DIR=/p\nWEB_PORT=0\nCLOUDFLARE_TUNNEL_TOKEN=\nCLOUDFLARE_TUNNEL_URL=\nDATA_DIR=");
        let t = crundi_shell::local::Setup { tunnel_token: "eyJ".into(), ..Default::default() };
        assert!(env_text(&t).contains("WEB_PORT=8888\n"));
    }

    #[test]
    fn electron_s_folders() {
        if cfg!(target_os = "linux") {
            assert!(user_data_for(true).ends_with("crundi-desktop"));
            assert!(user_data_for(false).ends_with("Crundi Client"));
        }
    }

    #[test]
    fn the_import_takes_once() {
        let root = std::env::temp_dir().join(format!("crundi-import-{}", std::process::id()));
        let (e, c) = (root.join("electron"), root.join("native"));
        std::fs::create_dir_all(e.join("data")).unwrap();
        std::fs::write(e.join("homepool-node.json"), r#"{"deviceId":"pc-abc123","server":"http://s","key":"hpn_x","enabled":true}"#).unwrap();
        std::fs::write(e.join("drawer-history.json"), r#"[{"url":"https://a.example","title":"A","at":1,"visits":2}]"#).unwrap();
        std::fs::write(e.join("worker.json"), r#"{"server":"http://s","key":"cw_w0123456789.x","enabled":true}"#).unwrap();
        std::fs::write(e.join("update-config.json"), r#"{"autoUpdate":false,"channel":"dev"}"#).unwrap();
        std::fs::write(e.join("data").join("client.json"), r#"{"mode":"remote","serverUrl":"https://crundi.example/","voiceWake":true}"#).unwrap();
        let took = import_from(&e, &c);
        for n in ["homepool-node.json", "drawer-history.json", "worker.json", "update-config.json", "device-id", "client.json", "voiceWake"] { assert!(took.iter().any(|t| t == n), "{n} in {took:?}"); }
        assert_eq!(std::fs::read_to_string(c.join("device-id")).unwrap(), "pc-abc123");
        let cj: Value = serde_json::from_slice(&std::fs::read(c.join("client.json")).unwrap()).unwrap();
        assert_eq!((cj["mode"].as_str(), cj["serverUrl"].as_str()), (Some("remote"), Some("https://crundi.example")));
        let ss: Value = serde_json::from_slice(&std::fs::read(c.join("session.json")).unwrap()).unwrap();
        assert_eq!(ss["server"], "https://crundi.example");
        // Once: a second start takes nothing, and keeps what the client has since.
        std::fs::write(c.join("device-id"), "pc-new").unwrap();
        assert!(import_from(&e, &c).is_empty());
        assert_eq!(std::fs::read_to_string(c.join("device-id")).unwrap(), "pc-new");
        let _ = std::fs::remove_dir_all(&root);
    }
}

// ─── Settings › This PC as a worker (the web's #pc-worker-on) ───

struct DirInput(gpui::Entity<crundi_widgets::InputState>);
impl gpui::Global for DirInput {}

/// The switch, its status, the work folder and Save (webapp-html.js 15939).
pub fn worker_part(window: &mut gpui::Window, cx: &mut gpui::App) -> gpui::AnyElement {
    use crundi_theme::T;
    use crundi_widgets as w;
    use gpui::*;
    crundi_core::driver::rendered("pc-worker");
    let s = worker::status();
    let input = match cx.try_global::<DirInput>() {
        Some(d) => d.0.clone(),
        None => {
            let e = cx.new(|cx| w::InputState::new(window, cx));
            let dir = s["workDir"].as_str().unwrap_or("").to_string();
            e.update(cx, |i, cx| i.set_value(dir, window, cx));
            cx.set_global(DirInput(e.clone()));
            e
        }
    };
    let (on, avail) = (s["enabled"].as_bool().unwrap_or(false), s["available"].as_bool().unwrap_or(false));
    let hint = |t: &str| div().text_size(T::TEXT_12).text_color(T::text_muted()).child(t.to_string());
    let inp = input.clone();
    div().flex().flex_col().gap(T::SP_10)
        .child(hint("Lets this server use this PC for heavy work: builds, renders, and later whole chats. It shows in the Workers tab, with a terminal on this PC for you. Turn it off at any time."))
        .child(div().flex().flex_wrap().items_center().gap(T::SP_8)
            .child(gpui_component::Disableable::disabled(w::check("pc-worker-on", "Use this PC as a worker", on, move |v: &bool, _, cx| worker_toggle(*v, cx)), !avail && !on))
            .child(div().id("pc-worker-status").text_size(T::TEXT_12).text_color(T::text_muted()).child(worker::words(&s))))
        .child(div().flex().flex_wrap().items_center().gap(T::SP_8)
            .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Work folder"))
            .child(div().flex_1().min_w(T::SP_48 * 4.).child(w::input(&input)))
            .child(w::button("pc-worker-dir", "Save").on_click(move |_, _, cx| worker_dir(inp.read(cx).value().to_string(), cx))))
        .child(hint("Where projects are cloned or copied for work on this PC."))
        .into_any_element()
}

/// The switch (pcWorkerToggle).
pub fn worker_toggle(on: bool, cx: &mut gpui::App) {
    if !on { worker::disable(); crundi_operations::settings_part_changed(cx); return; }
    let s = crundi_shell::stores(cx);
    let Some(api) = s.session.read(cx).api.clone() else { return };
    let base = crundi_core::store::load().server;
    let dir = cx.try_global::<DirInput>().map(|d| d.0.read(cx).value().to_string()).unwrap_or_default();
    worker::join(api, base, dir, worker::joined);
}

/// The work folder's Save (worker:setDir).
pub fn worker_dir(dir: String, _cx: &mut gpui::App) {
    if dir.trim().is_empty() { return; }
    let r = worker::set_dir(&dir);
    crate::ctx::on_app(move |win, cx| {
        match r { Ok(()) => crundi_widgets::toast(win, cx, "Saved", crundi_widgets::Toast::Success), Err(e) => crundi_widgets::toast(win, cx, e, crundi_widgets::Toast::Error) }
        crundi_operations::settings_part_changed(cx);
    });
}
