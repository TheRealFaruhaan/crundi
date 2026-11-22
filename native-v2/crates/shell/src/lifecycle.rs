//! The app's own lifecycle, as Electron's (app/main.js), with no GPUI: which
//! edition this is, the update settings (`update-config.json`), launching
//! at startup (one implementation: Settings and the driver both use it),
//! the log file (`crundi.log`) and the single instance.
//!
//! Names are package/CONTRACT.md's, so an Electron install moves over in
//! place: the autostart entry `crundi.desktop` (Linux all-in-one: Electron's
//! app.getName() is its package name; client `Crundi Client.desktop`),
//! the Windows Run value Electron's setLoginItemSettings names
//! `electron.app.Crundi` (client `electron.app.Crundi Client`; read in the CI
//! upgrade test), the update file `latest`/`client`.

use serde_json::{json, Value};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

// ─── The edition ───

/// The client edition: no `server/` beside the executable (CONTRACT.md
/// "Editions"); `CRUNDI_SERVER_DIR` (tests) makes it the all-in-one.
pub fn client_edition() -> bool {
    if let Ok(e) = std::env::var("CRUNDI_EDITION") { return e == "client"; }
    if std::env::var_os("CRUNDI_SERVER_DIR").is_some() { return false; }
    !exe_dir().join("server").join("src").join("index.js").exists()
}

/// Electron's app.getName(): "Crundi", or "Crundi Client".
pub fn app_name() -> &'static str { if client_edition() { "Crundi Client" } else { "Crundi" } }

/// The autostart entry's name: Electron's app.getName() there. The Linux
/// all-in-one's is its package name, "crundi" (CONTRACT.md "Launch at startup").
pub fn launch_name() -> &'static str { if cfg!(target_os = "linux") && !client_edition() { "crundi" } else { app_name() } }

/// The appId / AppUserModelId.
pub fn app_id() -> &'static str { if client_edition() { "com.crundi.client" } else { "com.crundi.app" } }

/// The update file's name stem (app-update.yml's channel): "latest" or "client".
pub fn update_channel_file() -> &'static str { if client_edition() { "client" } else { "latest" } }

pub fn exe() -> PathBuf { std::env::current_exe().unwrap_or_default() }
pub fn exe_dir() -> PathBuf { exe().parent().map(PathBuf::from).unwrap_or_default() }

/// Under the test driver: never the person's own login items or files.
fn testing() -> bool { std::env::var_os("CRUNDI_TEST_PORT").is_some() }

fn home() -> PathBuf { std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default() }

// ─── Update settings (main.js readUpdateConfig / writeUpdateConfig) ───

pub fn update_config_path() -> PathBuf { crundi_core::store::config_dir().join("update-config.json") }

/// The whole file (it holds more than one setting).
pub fn update_config() -> Value {
    std::fs::read_to_string(update_config_path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).filter(|v| v.is_object()).unwrap_or_else(|| json!({}))
}

/// Merged on write: writing one setting never drops the rest.
pub fn write_update_config(patch: Value) {
    let mut c = update_config();
    if let (Some(o), Some(p)) = (c.as_object_mut(), patch.as_object()) { for (k, v) in p { o.insert(k.clone(), v.clone()); } }
    let p = update_config_path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let _ = std::fs::write(p, c.to_string());
}

/// Automatic updates: on unless turned off.
pub fn auto_update() -> bool { update_config().get("autoUpdate").and_then(|v| v.as_bool()).unwrap_or(true) }
pub fn set_auto_update(on: bool) { write_update_config(json!({ "autoUpdate": on })); }

/// "production" (default) or "dev".
pub fn channel() -> &'static str { if update_config().get("channel").and_then(|v| v.as_str()) == Some("dev") { "dev" } else { "production" } }
pub fn set_channel(dev: bool) { write_update_config(json!({ "channel": if dev { "dev" } else { "production" } })); }

/// The updater's commands from Settings ("check", "install", "enabled",
/// "channel"): the desktop area runs the updater and says how to reach it.
static UPDATER: Mutex<Option<Box<dyn Fn(&str) + Send>>> = Mutex::new(None);

pub fn set_updater(f: impl Fn(&str) + Send + 'static) { *UPDATER.lock().unwrap() = Some(Box::new(f)); }

/// Ask the updater (false: there is none in this build).
pub fn updater(cmd: &str) -> bool {
    match UPDATER.lock().unwrap().as_ref() { Some(f) => { f(cmd); true } None => false }
}

// ─── Launch at startup (main.js 1199-1233) ───

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// The Run value's name, Electron's ("electron.app." + app.getName()), so
/// the setting replaces Electron's entry instead of adding a second one.
pub fn run_value_name() -> String { run_value(app_name()) }
fn run_value(app_name: &str) -> String { format!("electron.app.{app_name}") }

/// The file that is the setting (Linux; macOS; every system under the
/// driver): present means on. Under the driver, `$XDG_CONFIG_HOME` the run
/// made, else the client's own config folder.
pub fn launch_entry() -> PathBuf {
    if testing() {
        if cfg!(target_os = "linux") { if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") { return PathBuf::from(x).join("autostart").join(format!("{}.desktop", launch_name())); } }
        return crundi_core::store::config_dir().join("autostart").join(format!("{}.desktop", launch_name()));
    }
    if cfg!(target_os = "macos") { return home().join("Library/LaunchAgents").join(format!("{}.plist", app_id())); }
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"));
    base.join("autostart").join(format!("{}.desktop", launch_name()))
}

/// What starts at login: the AppImage that was started (gone after it
/// exits otherwise), or this executable.
fn launch_exe() -> String {
    std::env::var("APPIMAGE").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| exe().to_string_lossy().to_string())
}

/// The Exec line's quoting (escape " \ ` $).
pub fn quote(p: &str) -> String {
    let mut s = String::from('"');
    for c in p.chars() { if matches!(c, '"' | '\\' | '`' | '$') { s.push('\\'); } s.push(c); }
    s.push('"');
    s
}

/// The autostart entry's text, as Electron writes it.
pub fn desktop_entry(name: &str, exe: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n", quote(exe))
}

fn win_registry() -> bool { cfg!(target_os = "windows") && !testing() }

/// Does the app start at login now? (The system is the source of truth.)
pub fn launch_enabled() -> bool {
    if win_registry() {
        return std::process::Command::new("reg").args(["query", RUN_KEY, "/v", &run_value_name()]).output().is_ok_and(|o| o.status.success());
    }
    launch_entry().exists()
}

/// Turn launching at login on or off; the state read back.
pub fn set_launch(on: bool) -> Result<bool, String> {
    let exe = launch_exe();
    if win_registry() {
        let name = run_value_name();
        // (A value under the appId, from an earlier native build, goes: one entry.)
        let _ = std::process::Command::new("reg").args(["delete", RUN_KEY, "/v", app_id(), "/f"]).output();
        let mut c = std::process::Command::new("reg");
        if on { c.args(["add", RUN_KEY, "/v", &name, "/t", "REG_SZ", "/d", &format!("\"{exe}\""), "/f"]); }
        else { c.args(["delete", RUN_KEY, "/v", &name, "/f"]); }
        let ok = c.output().is_ok_and(|o| o.status.success());
        if !ok && on { return Err("Could not change the startup setting".into()); }
    } else {
        let f = launch_entry();
        if !on {
            match std::fs::remove_file(&f) { Ok(()) => {}, Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}, Err(e) => return Err(e.to_string()) }
        } else {
            if let Some(d) = f.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
            let text = if cfg!(target_os = "macos") && !testing() {
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{exe}</string></array><key>RunAtLoad</key><true/></dict></plist>\n", app_id())
            } else { desktop_entry(launch_name(), &exe) };
            std::fs::write(&f, text).map_err(|e| e.to_string())?;
        }
    }
    log_line(&format!("[startup] Launch at login {}", if on { "enabled" } else { "disabled" }));
    Ok(launch_enabled())
}

/// The setup wizard's consent (setup:save): automatic updates and
/// launching at startup, both on unless the person turned them off there.
pub fn first_run_defaults(auto_update: bool, launch: bool) {
    set_auto_update(auto_update);
    if let Err(e) = set_launch(launch) { log_line(&format!("[startup] error: {e}")); }
}

// ─── The log file (main.js writeLogLine) ───

/// Kept in the client's config folder; trimmed to the last 1000 lines once
/// 2000 were written.
pub const LOG_FILE_MAX: usize = 1000;

pub fn log_path() -> PathBuf { crundi_core::store::config_dir().join("crundi.log") }

static LOG_LINES: Mutex<usize> = Mutex::new(0);

/// One line into crundi.log: `[ISO time] text`.
pub fn log_line(text: &str) {
    let mut n = LOG_LINES.lock().unwrap_or_else(|e| e.into_inner());
    write_log(&log_path(), text, &mut n, chrono::Utc::now());
}

fn write_log(path: &std::path::Path, text: &str, lines: &mut usize, now: chrono::DateTime<chrono::Utc>) {
    let ts = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if let Some(d) = path.parent() { let _ = std::fs::create_dir_all(d); }
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) else { return };
    if writeln!(f, "[{ts}] {text}").is_err() { return; }
    *lines += 1;
    if *lines >= LOG_FILE_MAX * 2 {
        if let Ok(all) = std::fs::read_to_string(path) {
            let v: Vec<&str> = all.split('\n').collect();
            let keep = v[v.len().saturating_sub(LOG_FILE_MAX)..].join("\n");
            let _ = std::fs::write(path, keep + "\n");
            *lines = LOG_FILE_MAX;
        }
    }
}

/// What the logger writes goes to stderr as before, and each line into the file.
struct Tee(Vec<u8>);
impl std::io::Write for Tee {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(b);
        self.0.extend_from_slice(b);
        while let Some(i) = self.0.iter().position(|c| *c == b'\n') {
            let line: Vec<u8> = self.0.drain(..=i).collect();
            log_line(String::from_utf8_lossy(&line[..line.len() - 1]).trim_end());
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { std::io::stderr().flush() }
}

/// The logger (the app's start): env_logger's lines to stderr and to
/// crundi.log, then Electron's lifecycle lines at boot.
pub fn init_log(default_filter: &str) {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_filter))
        .format_timestamp(None)
        .target(env_logger::Target::Pipe(Box::new(Tee(Vec::new()))))
        .try_init();
    log_line("═══════════════════════════════════════");
    log_line(&format!("[lifecycle] {} starting", app_name()));
    log_line(&format!("[lifecycle] version={} exe={}", version(), exe().display()));
    log_line(&format!("[lifecycle] configDir={}", crundi_core::store::config_dir().display()));
    log_line(&format!("[lifecycle] logFile={}", log_path().display()));
    log_line(&format!("[lifecycle] platform={} arch={}", std::env::consts::OS, std::env::consts::ARCH));
}

/// This build's version (the release's, given at package time as
/// CRUNDI_VERSION; the crate's otherwise). The test driver may say another.
pub fn version() -> String {
    if testing() { if let Ok(v) = std::env::var("CRUNDI_TEST_VERSION") { return v; } }
    option_env!("CRUNDI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")).trim_start_matches('v').to_string()
}

// ─── Single instance (requestSingleInstanceLock, second-instance) ───

/// Called when a second launch asks this one to show itself.
static ON_SECOND: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);
/// Asked before anyone listened (the window was not up yet).
static ASKED: Mutex<bool> = Mutex::new(false);
/// The lock, held for the process's life.
static LOCK: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Claim the single instance (before the window opens). True: this is the
/// one copy (it now listens for a second launch). False: another copy runs
/// and was asked to show itself: exit.
pub fn claim() -> bool {
    let dir = crundi_core::store::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let lock_path = dir.join("instance.lock");
    let port_path = dir.join("instance.port");
    let Ok(f) = std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&lock_path) else { return true };
    if f.try_lock().is_err() {
        // The other copy: tell it (its port), then go.
        if let Some(port) = std::fs::read_to_string(&port_path).ok().and_then(|p| p.trim().parse::<u16>().ok()) {
            if let Ok(mut s) = std::net::TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], port)), std::time::Duration::from_secs(3)) {
                let _ = s.write_all(b"second-instance\n");
            }
        }
        return false;
    }
    *LOCK.lock().unwrap() = Some(f);
    if let Ok(l) = std::net::TcpListener::bind(("127.0.0.1", 0)) {
        if let Ok(a) = l.local_addr() { let _ = std::fs::write(&port_path, a.port().to_string()); }
        std::thread::Builder::new().name("single-instance".into()).spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { continue };
                let mut buf = [0u8; 32];
                let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let n = std::io::Read::read(&mut s, &mut buf).unwrap_or(0);
                if !buf[..n].starts_with(b"second-instance") { continue; }
                log_line("[lifecycle] second-instance: showing the window");
                match ON_SECOND.lock().unwrap().as_ref() { Some(f) => f(), None => *ASKED.lock().unwrap() = true }
            }
        }).ok();
    }
    true
}

/// Let go of the single instance (before starting the next copy: an update's relaunch).
pub fn release() {
    if let Some(f) = LOCK.lock().unwrap().take() { let _ = f.unlock(); }
    let _ = std::fs::remove_file(crundi_core::store::config_dir().join("instance.port"));
}

/// What a second launch does (show the window): set once the window is up.
pub fn on_second_instance(f: impl Fn() + Send + 'static) {
    let asked = std::mem::replace(&mut *ASKED.lock().unwrap(), false);
    if asked { f(); }
    *ON_SECOND.lock().unwrap() = Some(Box::new(f));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_value_is_electrons() {
        // Seen in the CI upgrade test (windows.ps1): Electron 1.21.7's values.
        assert_eq!(super::run_value("Crundi"), "electron.app.Crundi");
        assert_eq!(super::run_value("Crundi Client"), "electron.app.Crundi Client");
    }

    #[test]
    fn the_entry() {
        assert_eq!(quote("/a b/c$\"d"), "\"/a b/c\\$\\\"d\"");
        assert_eq!(quote("a`b\\c"), "\"a\\`b\\\\c\"");
        let e = desktop_entry("Crundi", "/opt/Crundi/crundi");
        assert_eq!(e, "[Desktop Entry]\nType=Application\nName=Crundi\nExec=\"/opt/Crundi/crundi\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n");
        assert!(desktop_entry("Crundi Client", "/x").contains("Name=Crundi Client\n"));
    }

    #[test]
    fn the_log_is_trimmed_as_electrons() {
        let d = std::env::temp_dir().join(format!("crundi-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let p = d.join("crundi.log");
        let mut n = 0;
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-22T10:00:00.123Z").unwrap().with_timezone(&chrono::Utc);
        for i in 0..1999 { write_log(&p, &format!("line {i}"), &mut n, now); }
        let t = std::fs::read_to_string(&p).unwrap();
        assert_eq!(t.lines().count(), 1999);
        assert_eq!(t.lines().next().unwrap(), "[2026-11-22T10:00:00.123Z] line 0");
        write_log(&p, "line 1999", &mut n, now);
        let t = std::fs::read_to_string(&p).unwrap();
        // Electron's split('\n').slice(-1000) keeps the piece after the last newline: 999 lines and a blank one.
        assert_eq!(t.lines().count(), LOG_FILE_MAX);
        assert_eq!(t.lines().next().unwrap(), "[2026-11-22T10:00:00.123Z] line 1001");
        assert_eq!(t.lines().nth(998).unwrap(), "[2026-11-22T10:00:00.123Z] line 1999");
        assert_eq!(t.lines().last().unwrap(), "");
        assert_eq!(n, LOG_FILE_MAX);
        let _ = std::fs::remove_dir_all(&d);
    }
}
