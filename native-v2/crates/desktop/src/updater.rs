//! The app's own updates, as Electron's (app/main.js 955-1450): the
//! settings in update-config.json (crundi_shell::lifecycle), the channels
//! (production: GitHub's latest release, never a prerelease; dev: the newest
//! release of either kind that carries this build's update file), never a
//! downgrade (`version_gt`), a check 8 s after launch and every 24 h when
//! automatic updates are on, the "Download & Install / Later" prompt once
//! per version, the download (full, sha512 checked, as electron-updater's
//! feed files say), the "Restart & Install / Not now" prompt, and the
//! install: Windows runs the new installer once the app has quit (the
//! server stopped with it); Linux installs the .deb with `sudo -n` when no
//! password is needed, else pkexec (after 20 s the terminal offer; after
//! 10 min it gives up), else a terminal, then starts again.
//!
//! The release files are package/CONTRACT.md's: `latest.yml` / `client.yml`
//! on Windows, `latest-linux.yml` / `client-linux.yml` on Linux, in
//! electron-updater's format.
//!
//! The work runs on threads of its own; what needs the window (a prompt,
//! the status in Settings and the tray, quitting) is queued and the area's
//! pump acts on it (`pump`). Nothing polls: the next check is a deadline
//! (`next_due`).

use crundi_shell::lifecycle as lc;
use crundi_widgets as w;
use gpui::{App, Window};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const OWNER: &str = "TheRealFaruhaan";
const REPO: &str = "crundi";
const FIRST_CHECK: Duration = Duration::from_secs(8);
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Electron's updateState.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub enabled: bool,
    pub available: bool,
    pub downloading: bool,
    pub downloaded: bool,
    pub installing: bool,
    pub percent: u32,
    pub version: String,
    pub current: String,
    pub channel: String,
    pub latest_kind: String,
}

impl State {
    pub fn to_json(&self) -> Value {
        json!({ "enabled": self.enabled, "available": self.available, "downloading": self.downloading, "downloaded": self.downloaded,
            "installing": self.installing, "percent": self.percent, "version": self.version, "current": self.current,
            "channel": self.channel, "latestKind": self.latest_kind })
    }
}

/// What a feed file offers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Offer { pub version: String, pub url: String, pub sha512: String, pub size: u64 }

/// What the window must do (queued by the threads, done by the pump).
enum Ui {
    Status,
    PromptDownload(String),
    ConfirmInstall,
    /// pkexec has shown no password prompt for 20 s.
    NoPrompt,
    /// A message box: (message, detail, copy this to the clipboard).
    Message(String, String, Option<String>),
    /// Quit; then (after the app is gone) do this.
    Quit(AfterQuit),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AfterQuit { Nothing, Relaunch, RunInstaller(PathBuf) }

#[derive(Default)]
struct Inner {
    state: State,
    /// The version the download prompt was shown for (once per version).
    prompted: String,
    offer: Option<Offer>,
    /// The downloaded installer.
    file: Option<PathBuf>,
    install_running: bool,
    next_check: Option<Instant>,
    checking: bool,
    after_quit: Option<AfterQuit>,
    /// The pkexec run, while it waits (the terminal offer stops it).
    child: Option<Arc<Mutex<std::process::Child>>>,
    via_terminal: bool,
    /// The last few things it did (the driver's state).
    log: Vec<String>,
}

static INNER: Mutex<Option<Inner>> = Mutex::new(None);
static UI: Mutex<Vec<Ui>> = Mutex::new(Vec::new());
static CMDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn with<R>(f: impl FnOnce(&mut Inner) -> R) -> R {
    let mut g = INNER.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Inner::default))
}

fn ui(u: Ui) { UI.lock().unwrap().push(u); crundi_webview::wake(); }
fn status() { ui(Ui::Status); }

/// appendLog('[update] …'): crundi.log, and the driver's last lines.
fn log(text: &str) {
    let line = format!("[update] {text}");
    lc::log_line(&line);
    with(|i| { i.log.push(line); let n = i.log.len(); if n > 40 { i.log.drain(..n - 40); } });
}

fn testing() -> bool { crundi_core::driver::port().is_some() }

/// The GitHub API and download hosts (a test's own under the driver).
fn api_base() -> String {
    if testing() { if let Ok(u) = std::env::var("CRUNDI_UPDATE_API") { return u.trim_end_matches('/').to_string(); } }
    "https://api.github.com".into()
}
fn download_base() -> String {
    if testing() { if let Ok(u) = std::env::var("CRUNDI_UPDATE_DOWNLOAD").or_else(|_| std::env::var("CRUNDI_UPDATE_API")) { return u.trim_end_matches('/').to_string(); } }
    "https://github.com".into()
}

/// Electron's app.isPackaged: the installed app (its executable is the
/// package's `crundi` / `crundi-client`), not a development build.
pub fn packaged() -> bool {
    if testing() { return std::env::var_os("CRUNDI_UPDATE_API").is_some(); }
    let stem = lc::exe().file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    stem == "crundi" || stem == "crundi-client"
}

/// UPDATE_FILE_SUFFIX: the platform in the update file's name.
pub fn file_suffix() -> &'static str { if cfg!(target_os = "linux") { "-linux" } else if cfg!(target_os = "macos") { "-mac" } else { "" } }

/// This build's update file (`latest-linux.yml`, `client.yml`…).
pub fn feed_file() -> String { format!("{}{}.yml", lc::update_channel_file(), file_suffix()) }

/// a > b, with 1.2.0-dev.3 < 1.2.0-dev.10 < 1.2.0 (main.js versionGt).
pub fn version_gt(a: &str, b: &str) -> bool {
    fn parse(v: &str) -> Option<([u64; 3], Vec<String>)> {
        let v = v.trim().trim_start_matches('v');
        let (core, pre) = match v.find('-') { Some(i) => (&v[..i], Some(&v[i + 1..])), None => (v, None) };
        let core = core.split('+').next().unwrap_or(core);
        let mut it = core.split('.');
        let mut n = [0u64; 3];
        for x in n.iter_mut() { *x = it.next()?.parse().ok()?; }
        let pre = pre.map(|p| p.split('+').next().unwrap_or(p)).filter(|p| !p.is_empty())
            .map(|p| p.split('.').map(str::to_string).collect()).unwrap_or_default();
        Some((n, pre))
    }
    let (Some(x), Some(y)) = (parse(a), parse(b)) else { return false };
    for i in 0..3 { if x.0[i] != y.0[i] { return x.0[i] > y.0[i]; } }
    if x.1.is_empty() || y.1.is_empty() { return x.1.is_empty() && !y.1.is_empty(); }
    for i in 0..x.1.len().max(y.1.len()) {
        let (Some(s), t) = (x.1.get(i), y.1.get(i)) else { return false };
        let Some(t) = t else { return true };
        let (sn, tn) = (s.parse::<u64>().ok(), t.parse::<u64>().ok());
        match (sn, tn) {
            (Some(a), Some(b)) if a != b => return a > b,
            (Some(_), None) => return false,
            (None, Some(_)) => return true,
            _ => if s != t { return s > t; },
        }
    }
    false
}

/// The newest release of either kind carrying `feed` (pickDevRelease):
/// (tag, version, prerelease).
pub fn pick_dev(releases: &Value, feed: &str) -> Option<(String, String, bool)> {
    let mut best: Option<(String, String, bool)> = None;
    for r in releases.as_array()? {
        if r["draft"].as_bool().unwrap_or(false) { continue; }
        if !r["assets"].as_array().is_some_and(|a| a.iter().any(|x| x["name"].as_str() == Some(feed))) { continue; }
        let tag = r["tag_name"].as_str().unwrap_or("").to_string();
        let v = tag.trim_start_matches('v').to_string();
        if best.as_ref().is_none_or(|b| version_gt(&v, &b.1)) { best = Some((tag, v, r["prerelease"].as_bool().unwrap_or(false))); }
    }
    best
}

/// electron-updater's feed file: the version, and the file for this system
/// (the .deb on Linux, the .exe on Windows) with its sha512 and size.
pub fn parse_feed(yml: &str) -> Option<Offer> {
    let unq = |s: &str| s.trim().trim_matches('\'').trim_matches('"').to_string();
    let mut version = String::new();
    let mut files: Vec<Offer> = vec![];
    let (mut top_path, mut top_sha) = (String::new(), String::new());
    let mut in_files = false;
    for line in yml.lines() {
        let t = line.trim_end();
        if t.is_empty() || t.trim_start().starts_with('#') { continue; }
        let indented = t.starts_with(' ') || t.starts_with('-');
        if !indented {
            in_files = t.starts_with("files:");
            if let Some(v) = t.strip_prefix("version:") { version = unq(v); }
            if let Some(v) = t.strip_prefix("path:") { top_path = unq(v); }
            if let Some(v) = t.strip_prefix("sha512:") { top_sha = unq(v); }
            continue;
        }
        if !in_files { continue; }
        let s = t.trim_start();
        let (s, new) = match s.strip_prefix("- ") { Some(r) => (r.trim_start(), true), None => (s, false) };
        if new { files.push(Offer::default()); }
        let Some(f) = files.last_mut() else { continue };
        if let Some(v) = s.strip_prefix("url:") { f.url = unq(v); }
        else if let Some(v) = s.strip_prefix("sha512:") { f.sha512 = unq(v); }
        else if let Some(v) = s.strip_prefix("size:") { f.size = unq(v).parse().unwrap_or(0); }
    }
    if version.is_empty() { return None; }
    let want = if cfg!(target_os = "linux") { ".deb" } else if cfg!(target_os = "windows") { ".exe" } else { "" };
    let mut pick = files.iter().find(|f| !want.is_empty() && f.url.to_lowercase().ends_with(want)).or(files.first()).cloned()
        .unwrap_or(Offer { url: top_path, sha512: top_sha, ..Default::default() });
    if pick.url.is_empty() { return None; }
    pick.version = version;
    Some(pick)
}

// ─── Start, commands, the pump ───

/// At launch (setupAutoUpdate): the settings read, the first check in 8 s.
pub fn start() {
    with(|i| {
        i.state = State { enabled: lc::auto_update(), channel: lc::channel().into(), current: lc::version(), ..Default::default() };
        i.next_check = Some(Instant::now() + FIRST_CHECK);
    });
    // Settings' commands (update:check / install / setEnabled / setChannel).
    lc::set_updater(|cmd| { CMDS.lock().unwrap().push(cmd.to_string()); crundi_webview::wake(); });
    status();
}

/// When the pump must run for the next automatic check.
pub fn next_due() -> Option<Instant> { with(|i| i.next_check) }

/// The area's pump: the due check, Settings' commands, what the threads queued.
pub fn pump(window: &mut Window, cx: &mut App) {
    if with(|i| i.next_check.is_some_and(|t| t <= Instant::now())) {
        with(|i| i.next_check = Some(Instant::now() + EVERY));
        check(false);
    }
    let cmds = std::mem::take(&mut *CMDS.lock().unwrap());
    for c in cmds { command(&c); }
    let todo = std::mem::take(&mut *UI.lock().unwrap());
    for u in todo { act(u, window, cx); }
}

/// A command from Settings or the driver.
pub fn command(cmd: &str) {
    match cmd {
        "check" => check(true),
        "install" => { if with(|i| i.state.downloaded) { ui(Ui::ConfirmInstall) } else { start_download() } }
        "enabled-on" | "enabled-off" => {
            let on = cmd == "enabled-on";
            with(|i| i.state.enabled = on);
            log(&format!("Auto-update {}", if on { "enabled" } else { "disabled" }));
            status();
            if on { check(false); }
        }
        "channel" => {
            let ch = lc::channel().to_string();
            with(|i| { i.state.channel = ch.clone(); i.state.available = false; i.state.version.clear(); i.state.latest_kind.clear(); i.prompted.clear(); i.offer = None; });
            log(&format!("Channel: {ch}"));
            status();
            check(true);
        }
        _ => {}
    }
}

/// The tray's item: Restart to Install Update once downloaded, else Check for Updates….
pub fn tray_pick() { if with(|i| i.state.downloaded) { ui(Ui::ConfirmInstall) } else { check(true) } }

pub fn downloaded() -> bool { with(|i| i.state.downloaded) }

fn act(u: Ui, window: &mut Window, cx: &mut App) {
    match u {
        Ui::Status => {
            let v = with(|i| i.state.to_json());
            crundi_operations::settings::set_update_state(cx, &v);
            crate::tray::refresh();
        }
        Ui::PromptDownload(v) => prompt_download(&v, window, cx),
        Ui::ConfirmInstall => confirm_install(window, cx),
        Ui::NoPrompt => no_prompt(window, cx),
        Ui::Message(m, d, copy) => {
            if let Some(c) = copy { cx.write_to_clipboard(gpui::ClipboardItem::new_string(c)); }
            w::ask_buttons(window, cx, &m, &d, &["OK"], 0, 0, |_, _, _| {});
        }
        Ui::Quit(after) => {
            with(|i| i.after_quit = Some(after));
            cx.quit();
        }
    }
}

/// The modal on detection (promptDownload): once per version.
fn prompt_download(version: &str, window: &mut Window, cx: &mut App) {
    let dev = with(|i| { if i.prompted == version { return None; } i.prompted = version.to_string(); Some(i.state.channel == "dev") });
    let Some(dev) = dev else { return };
    crate::ctx::raise();
    let pre = version.contains('-');
    let message = if dev { if pre { format!("Dev build Crundi {version} is available.") } else { format!("A production release, Crundi {version}, is available.") } } else { format!("Crundi {version} is available.") };
    let detail = format!("{}Download it now? You can watch progress in Settings, and Crundi will ask you to save your work before it restarts to install.", if dev && !pre { "It is newer than the dev build you are on. " } else { "" });
    w::ask_buttons(window, cx, "Update available", &format!("{message}\n\n{detail}"), &["Download & Install", "Later"], 0, 1, |i, _, _| if i == 0 { start_download() });
}

/// confirmAndInstall: save your work first.
fn confirm_install(window: &mut Window, cx: &mut App) {
    if !with(|i| i.state.downloaded) { start_download(); return; }
    let v = with(|i| i.state.version.clone());
    let tail = if cfg!(target_os = "linux") { "Your system will ask for your password to install it." } else { "It will also install automatically the next time you quit Crundi." };
    let detail = format!("Crundi will close and restart to apply the update. Before continuing:\n\n\u{2022}  Save any unsaved work in your terminals and editors.\n\u{2022}  Make sure any running services are safe to stop \u{2014} they will be shut down.\n\n{tail}");
    crate::ctx::raise();
    w::ask_buttons(window, cx, &format!("Install Crundi {v}"), &format!("Crundi {v} is ready to install.\n\n{detail}"), &["Restart & Install", "Not now"], 1, 1, |i, _, _| if i == 0 { install_now() });
}

/// pkexec has shown nothing for 20 s: a terminal instead, or the command.
fn no_prompt(window: &mut Window, cx: &mut App) {
    let running = with(|i| i.child.as_ref().is_some_and(|c| c.lock().unwrap().try_wait().ok().flatten().is_none()));
    if !running { return; }
    let term = find_terminal().is_some();
    let mut labels: Vec<&str> = vec![];
    if term { labels.push("Install in a terminal"); }
    labels.extend(["Copy the install command", "Keep waiting"]);
    let n = labels.len();
    let owned: Vec<String> = labels.iter().map(|s| s.to_string()).collect();
    w::ask_buttons(window, cx, "No password prompt?", "Your system has not shown a password prompt for the update.\n\nSome desktops have no password agent, or open its prompt out of sight. You can install it in a terminal instead (sudo asks for your password there), then Crundi Client starts again by itself.",
        &labels, 0, n - 1, move |i, _, cx| {
            match owned.get(i).map(String::as_str) {
                Some("Install in a terminal") => with(|x| {
                    if let Some(c) = &x.child { x.via_terminal = true; let _ = c.lock().unwrap().kill(); }
                }),
                Some("Copy the install command") => {
                    let f = with(|x| x.file.clone()).unwrap_or_default();
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(format!("sudo apt install {}", shq(&f.to_string_lossy()))));
                    log("Install command copied");
                }
                _ => {}
            }
        });
}

// ─── Checking ───

fn get(url: &str) -> Result<(u16, String), String> {
    let agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).http_status_as_error(false).build().new_agent();
    let r = agent.get(url).header("Accept", "application/vnd.github+json").header("User-Agent", "crundi").call().map_err(|e| e.to_string())?;
    let st = r.status().as_u16();
    let body = r.into_body().read_to_string().map_err(|e| e.to_string())?;
    Ok((st, body))
}

/// Which release to read the feed of (applyFeed): (tag, kind).
fn pick_release(dev: bool) -> Result<(String, String), String> {
    let api = api_base();
    if dev {
        let (st, body) = get(&format!("{api}/repos/{OWNER}/{REPO}/releases?per_page=30"))?;
        if st != 200 { return Err(format!("GitHub returned {st}")); }
        let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let (tag, _, pre) = pick_dev(&v, &feed_file()).ok_or("No release found")?;
        Ok((tag, if pre { "dev" } else { "production" }.into()))
    } else {
        // GitHub's latest release: never a prerelease, never a draft.
        let (st, body) = get(&format!("{api}/repos/{OWNER}/{REPO}/releases/latest"))?;
        if st != 200 { return Err(format!("GitHub returned {st}")); }
        let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let tag = v["tag_name"].as_str().filter(|t| !t.is_empty()).ok_or("No release found")?.to_string();
        Ok((tag, "production".into()))
    }
}

fn release_url(tag: &str, file: &str) -> String { format!("{}/{OWNER}/{REPO}/releases/download/{tag}/{file}", download_base()) }

/// checkForUpdates: automatic ones only while turned on; a manual one always.
pub fn check(manual: bool) {
    if !packaged() {
        log("Skipped (dev build \u{2014} packaged app only)");
        if manual { ui(Ui::Message("Update checks run in the installed app only.".into(), String::new(), None)); }
        return;
    }
    let (enabled, dev, busy) = with(|i| (i.state.enabled, i.state.channel == "dev", i.checking || i.state.downloading || i.install_running));
    if !enabled && !manual { return; }
    if busy { return; }
    with(|i| i.checking = true);
    log("Checking for updates\u{2026}");
    std::thread::Builder::new().name("update-check".into()).spawn(move || {
        let r = (|| -> Result<(Offer, String, String), String> {
            let (tag, kind) = pick_release(dev)?;
            let feed = feed_file();
            let (st, body) = get(&release_url(&tag, &feed))?;
            if st != 200 { return Err(format!("Cannot find {feed} in the latest release artifacts ({st})")); }
            let mut o = parse_feed(&body).ok_or_else(|| format!("Cannot parse {feed}"))?;
            if !o.url.starts_with("http://") && !o.url.starts_with("https://") { o.url = release_url(&tag, &o.url); }
            Ok((o, tag, kind))
        })();
        with(|i| i.checking = false);
        match r {
            Ok((o, _, kind)) => {
                let cur = with(|i| i.state.current.clone());
                if version_gt(&o.version, &cur) {
                    log(&format!("Available: {}", o.version));
                    let v = o.version.clone();
                    let same = with(|i| i.offer.as_ref().is_some_and(|x| x.version == v) && (i.state.downloaded || i.state.downloading));
                    with(|i| {
                        i.state.available = true; i.state.version = v.clone(); i.state.latest_kind = kind;
                        if !same { i.state.downloading = false; i.state.downloaded = false; i.state.percent = 0; i.offer = Some(o); i.file = None; }
                    });
                    status();
                    ui(Ui::PromptDownload(v));
                } else {
                    log("Up to date");
                    with(|i| { i.state.available = false; i.state.downloading = false; i.state.downloaded = false; i.state.percent = 0; i.state.latest_kind = kind; });
                    status();
                }
            }
            Err(e) => { log(&format!("check failed: {e}")); status(); }
        }
    }).ok();
}

// ─── Downloading ───

pub fn updates_dir() -> PathBuf { crundi_core::store::config_dir().join("updates") }

/// startDownload: the whole file (never a differential one), its sha512 checked.
pub fn start_download() {
    let (offer, downloaded, downloading) = with(|i| (i.offer.clone(), i.state.downloaded, i.state.downloading));
    if downloaded { ui(Ui::ConfirmInstall); return; }
    if downloading { return; }
    let Some(o) = offer else { return };
    log("Downloading update\u{2026}");
    with(|i| { i.state.downloading = true; i.state.percent = 0; });
    status();
    std::thread::Builder::new().name("update-download".into()).spawn(move || {
        match download(&o) {
            Ok(f) => {
                log(&format!("Downloaded {}", o.version));
                with(|i| { i.file = Some(f); i.state.available = true; i.state.downloaded = true; i.state.downloading = false; i.state.percent = 100; i.state.version = o.version.clone(); });
                status();
                ui(Ui::ConfirmInstall);
            }
            Err(e) => {
                log(&format!("download error: {e}"));
                with(|i| i.state.downloading = false);
                status();
            }
        }
    }).ok();
}

fn download(o: &Offer) -> Result<PathBuf, String> {
    use sha2::Digest as _;
    use std::io::{Read as _, Write as _};
    let dir = updates_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = o.url.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("update").replace(['\\', ':'], "_");
    let to = dir.join(&name);
    let part = dir.join(format!("{name}.part"));
    let agent = ureq::Agent::config_builder().timeout_connect(Some(Duration::from_secs(20))).build().new_agent();
    let r = agent.get(&o.url).header("User-Agent", "crundi").call().map_err(|e| e.to_string())?;
    let total = r.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).filter(|n| *n > 0).unwrap_or(o.size);
    let mut body = r.into_body();
    let mut reader = body.as_reader();
    let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut h = sha2::Sha512::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut got = 0u64;
    let mut shown = 0u32;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        h.update(&buf[..n]);
        f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        if total > 0 {
            let p = ((got * 100) / total).min(100) as u32;
            if p != shown { shown = p; with(|i| i.state.percent = p); log(&format!("Downloading {p}%")); status(); }
        }
    }
    drop(f);
    if !o.sha512.is_empty() {
        use base64::Engine as _;
        let sum = base64::engine::general_purpose::STANDARD.encode(h.finalize());
        if sum != o.sha512 { let _ = std::fs::remove_file(&part); return Err(format!("sha512 checksum mismatch, expected {}, got {sum}", o.sha512)); }
    }
    std::fs::rename(&part, &to).map_err(|e| e.to_string())?;
    Ok(to)
}

// ─── Installing ───

/// The shell quoting of one argument.
pub fn shq(v: &str) -> String { format!("'{}'", v.replace('\'', "'\\''")) }

/// The first terminal emulator on PATH, and how to run a command in it (findTerminal).
pub fn find_terminal() -> Option<(String, Vec<String>)> {
    let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into());
    let has = |b: &str| path.split(':').any(|d| !d.is_empty() && Path::new(d).join(b).exists());
    let list: [(&str, &[&str]); 9] = [
        ("x-terminal-emulator", &["-e", "bash", "-c"]), ("gnome-terminal", &["--", "bash", "-c"]), ("konsole", &["-e", "bash", "-c"]),
        ("xfce4-terminal", &["-x", "bash", "-c"]), ("kitty", &["bash", "-c"]), ("alacritty", &["-e", "bash", "-c"]),
        ("foot", &["bash", "-c"]), ("wezterm", &["start", "--", "bash", "-c"]), ("xterm", &["-e", "bash", "-c"]),
    ];
    list.iter().find(|(b, _)| has(b)).map(|(b, a)| (b.to_string(), a.iter().map(|s| s.to_string()).collect()))
}

fn pkexec_path() -> PathBuf {
    if testing() { if let Ok(p) = std::env::var("CRUNDI_TEST_PKEXEC") { return p.into(); } }
    "/usr/bin/pkexec".into()
}

/// Can sudo run without asking (NOPASSWD)? Then no prompt is needed at all.
fn sudo_without_password() -> bool {
    let Ok(mut c) = std::process::Command::new("sudo").args(["-n", "true"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() else { return false };
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match c.try_wait() {
            Ok(Some(s)) => return s.success(),
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(50)),
            _ => { let _ = c.kill(); let _ = c.wait(); return false; }
        }
    }
}

/// dpkg first; if dependencies are missing, apt pulls them in: (ok, code, output).
fn install_deb(file: &Path, via_sudo: bool) -> (bool, i32, String) {
    let f = file.to_string_lossy().to_string();
    let args = ["/bin/sh", "-c", "dpkg -i \"$1\" || apt-get install -f -y", "crundi-update", &f];
    let mut cmd = if via_sudo { let mut c = std::process::Command::new("sudo"); c.arg("-n").args(args); c } else { let mut c = std::process::Command::new(pkexec_path()); c.args(args); c };
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let child = match cmd.spawn() { Ok(c) => c, Err(e) => return (false, -1, e.to_string()) };
    let out = Arc::new(Mutex::new(String::new()));
    let mut child = child;
    let readers: Vec<_> = [child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)]
        .into_iter().flatten().map(|mut r| { let o = out.clone(); std::thread::spawn(move || { let mut b = String::new(); let _ = r.read_to_string(&mut b); o.lock().unwrap().push_str(&b); }) }).collect();
    let child = Arc::new(Mutex::new(child));
    if !via_sudo { with(|i| { i.child = Some(child.clone()); i.via_terminal = false; }); }
    let started = Instant::now();
    let mut offered = false;
    let code = loop {
        if let Some(s) = child.lock().unwrap().try_wait().ok().flatten() { break s.code().unwrap_or(-1); }
        if !via_sudo && !offered && started.elapsed() >= offer_after() { offered = true; ui(Ui::NoPrompt); }
        if started.elapsed() >= Duration::from_secs(600) {
            log("No answer from pkexec after 10 minutes; giving up");
            let _ = child.lock().unwrap().kill();
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    for r in readers { let _ = r.join(); }
    with(|i| i.child = None);
    let o = out.lock().unwrap().clone();
    let tail = if o.len() > 4000 { o[o.len() - 4000..].to_string() } else { o };
    (code == 0, code, tail.trim().to_string())
}

/// 20 s (a test may say less).
fn offer_after() -> Duration {
    if testing() { if let Some(ms) = std::env::var("CRUNDI_TEST_PKEXEC_OFFER_MS").ok().and_then(|v| v.parse().ok()) { return Duration::from_millis(ms); } }
    Duration::from_secs(20)
}

/// Install in a terminal (sudo asks there), then start the new version.
fn install_in_terminal(file: &Path) -> bool {
    let Some((bin, mut args)) = find_terminal() else { return false };
    let exe = std::env::var("APPIMAGE").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| lc::exe().to_string_lossy().to_string());
    let script = format!("echo \"Installing the Crundi Client update. sudo asks for your password.\"; sudo apt install -y {} && echo \"Installed. Starting Crundi Client\u{2026}\" || echo \"The install did not finish.\"; (nohup {} >/dev/null 2>&1 &); sleep 3", shq(&file.to_string_lossy()), shq(&exe));
    args.push(script);
    match std::process::Command::new(&bin).args(&args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        Ok(_) => { log(&format!("Installing in a terminal ({bin})")); true }
        Err(e) => { log(&format!("Could not open a terminal: {e}")); false }
    }
}

/// installUpdateNow.
pub fn install_now() {
    if with(|i| std::mem::replace(&mut i.install_running, true)) { return; }
    let file = with(|i| i.file.clone()).filter(|f| f.exists());
    let Some(file) = file else { with(|i| i.install_running = false); return };
    let deb = cfg!(target_os = "linux") && file.extension().is_some_and(|e| e.eq_ignore_ascii_case("deb"));
    if !deb {
        // Windows: the new installer runs silently once the app (and the server with it) has quit, and starts it again.
        log("Installing \u{2014} stopping server first\u{2026}");
        ui(Ui::Quit(AfterQuit::RunInstaller(file)));
        return;
    }
    std::thread::Builder::new().name("update-install".into()).spawn(move || install_linux(&file)).ok();
}

fn installing(on: bool) { with(|i| i.state.installing = on); status(); }

fn install_linux(file: &Path) {
    let shown = file.to_string_lossy().to_string();
    if sudo_without_password() {
        log(&format!("Installing {shown} with sudo (no password needed)\u{2026}"));
        installing(true);
        let (ok, code, out) = install_deb(file, true);
        installing(false);
        if ok { log("Installed; restarting\u{2026}"); ui(Ui::Quit(AfterQuit::Relaunch)); return; }
        log(&format!("sudo install did not finish (exit {code}): {out}; trying pkexec"));
    }
    if pkexec_path().exists() {
        log(&format!("Installing {shown} with pkexec\u{2026}"));
        installing(true);
        let (ok, code, out) = install_deb(file, false);
        installing(false);
        if with(|i| std::mem::replace(&mut i.via_terminal, false)) {
            if install_in_terminal(file) { ui(Ui::Quit(AfterQuit::Nothing)); return; }
            with(|i| i.install_running = false);
            ui(Ui::Message("No terminal found.".into(), format!("Install it yourself with:\n\nsudo apt install {shown}"), None));
            return;
        }
        if !ok {
            with(|i| i.install_running = false);
            log(&format!("Install did not finish (exit {code}): {out}"));
            // 126: the password prompt was dismissed; 127: not authorised.
            let cancelled = code == 126 || code == 127;
            let (m, d) = if cancelled {
                ("The update was not installed.".to_string(), "The password prompt was closed. Crundi carries on as it was; choose Restart & Install again whenever you like.".to_string())
            } else {
                let what = if out.is_empty() { format!("exit code {code}") } else { out.clone() };
                let tail: String = what.chars().rev().take(1200).collect::<Vec<_>>().into_iter().rev().collect();
                ("The update could not be installed.".to_string(), format!("Crundi carries on as it was. What the installer said:\n\n{tail}"))
            };
            ui(Ui::Message(m, d, None));
            return;
        }
        log("Installed; restarting\u{2026}");
        ui(Ui::Quit(AfterQuit::Relaunch));
        return;
    }
    // No pkexec either: the terminal.
    if install_in_terminal(file) { ui(Ui::Quit(AfterQuit::Nothing)); return; }
    with(|i| i.install_running = false);
    ui(Ui::Message("Install the update in a terminal.".into(), format!("sudo apt install {shown}"), Some(format!("sudo apt install {}", shq(&shown)))));
}

/// After the app has quit (main, once the window and the server are gone):
/// start the new version, or run the new installer.
pub fn after_quit() {
    let Some(a) = with(|i| i.after_quit.take()) else { return };
    // The next copy must not find this one still holding the single instance.
    lc::release();
    let detached = |c: &mut std::process::Command| { c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()); c.spawn() };
    match a {
        AfterQuit::Nothing => {}
        AfterQuit::Relaunch => {
            lc::log_line("[update] Relaunching");
            // (A moment later: this process lets go of its sockets as it exits.)
            let mut c = std::process::Command::new("/bin/sh");
            c.arg("-c").arg("sleep 1; exec \"$0\" \"$@\"").arg(lc::exe()).args(std::env::args_os().skip(1));
            let _ = detached(&mut c);
        }
        AfterQuit::RunInstaller(f) => {
            // electron-updater's quitAndInstall(true, true): silent, run after.
            lc::log_line(&format!("[update] Running {}", f.display()));
            let _ = detached(std::process::Command::new(&f).args(["--updated", "/S", "--force-run"]));
        }
    }
}

/// The driver's view.
pub fn props() -> Value {
    with(|i| {
        let mut v = i.state.to_json();
        v["prompted"] = json!(i.prompted);
        v["file"] = json!(i.file.as_ref().map(|f| f.to_string_lossy().to_string()));
        v["nextCheckMs"] = json!(i.next_check.map(|t| t.saturating_duration_since(Instant::now()).as_millis() as u64));
        v["log"] = json!(i.log);
        v["feed"] = json!(feed_file());
        v["packaged"] = json!(packaged());
        v["pid"] = json!(std::process::id());
        v
    })
}

/// The driver: a check due now (instead of 8 s or a day from now).
pub fn test_due_now() { with(|i| i.next_check = Some(Instant::now())); crundi_webview::wake(); }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_order_as_electrons() {
        assert!(version_gt("1.2.0-dev.10", "1.2.0-dev.3"));
        assert!(!version_gt("1.2.0-dev.3", "1.2.0-dev.10"));
        assert!(version_gt("1.2.0", "1.2.0-dev.10"));
        assert!(!version_gt("1.2.0-dev.10", "1.2.0"));
        assert!(version_gt("v1.10.0", "1.9.9"));
        assert!(!version_gt("1.2.0", "1.2.0"));
        assert!(version_gt("1.2.1-dev.1", "1.2.0"));
        assert!(version_gt("1.2.0-dev.1.1", "1.2.0-dev.1"));
        assert!(version_gt("1.2.0-dev.a", "1.2.0-dev.9"));
        assert!(!version_gt("nonsense", "1.0.0"));
    }

    #[test]
    fn dev_picks_the_newest_with_its_feed_file() {
        let feed = "latest-linux.yml";
        let rel = json!([
            { "tag_name": "v1.3.0-dev.2", "prerelease": true, "draft": false, "assets": [{ "name": "latest-linux.yml" }] },
            { "tag_name": "v1.3.0-dev.9", "prerelease": true, "draft": true, "assets": [{ "name": "latest-linux.yml" }] },
            { "tag_name": "v1.3.0-dev.5", "prerelease": true, "draft": false, "assets": [{ "name": "latest.yml" }] },
            { "tag_name": "v1.2.0", "prerelease": false, "draft": false, "assets": [{ "name": "latest-linux.yml" }] },
        ]);
        assert_eq!(pick_dev(&rel, feed), Some(("v1.3.0-dev.2".into(), "1.3.0-dev.2".into(), true)));
        let rel2 = json!([
            { "tag_name": "v1.3.0-dev.2", "prerelease": true, "assets": [{ "name": "latest-linux.yml" }] },
            { "tag_name": "v1.3.0", "prerelease": false, "assets": [{ "name": "latest-linux.yml" }] },
        ]);
        assert_eq!(pick_dev(&rel2, feed), Some(("v1.3.0".into(), "1.3.0".into(), false)));
        assert_eq!(pick_dev(&json!([]), feed), None);
    }

    #[test]
    fn the_feed_file() {
        let yml = "version: 1.22.0\nfiles:\n  - url: Crundi-1.22.0-linux-x64.AppImage\n    sha512: AAA\n    size: 10\n  - url: Crundi-1.22.0-linux-x64.deb\n    sha512: 'BBB=='\n    size: 2048\npath: Crundi-1.22.0-linux-x64.AppImage\nsha512: AAA\nreleaseDate: '2026-11-20T10:00:00.000Z'\n";
        let o = parse_feed(yml).unwrap();
        assert_eq!(o.version, "1.22.0");
        if cfg!(target_os = "linux") { assert_eq!((o.url.as_str(), o.sha512.as_str(), o.size), ("Crundi-1.22.0-linux-x64.deb", "BBB==", 2048)); }
        let win = "version: 1.22.0\nfiles:\n  - url: Crundi-Setup-1.22.0.exe\n    sha512: CCC\n    size: 5\npath: Crundi-Setup-1.22.0.exe\nsha512: CCC\n";
        assert_eq!(parse_feed(win).unwrap().url, "Crundi-Setup-1.22.0.exe");
        assert_eq!(parse_feed("path: x.exe\nsha512: Z\nversion: 2.0.0\n").unwrap().url, "x.exe");
        assert!(parse_feed("nothing: here\n").is_none());
    }

    #[test]
    fn names_and_quoting() {
        assert_eq!(shq("a'b"), "'a'\\''b'");
        assert!(feed_file().ends_with(if cfg!(target_os = "linux") { "-linux.yml" } else if cfg!(target_os = "macos") { "-mac.yml" } else { ".yml" }));
    }
}
