// No console window for the tray app on Windows (headless/CLI use attaches
// to the parent console below).
#![cfg_attr(all(windows, feature = "tray", not(debug_assertions)), windows_subsystem = "windows")]

use crundi_worker::app;
use crundi_worker::config::{default_config_path, Config};
use crundi_worker::join::{absolute, join_and_save, JoinArgs};
use crundi_worker::link::LinkOptions;
use crundi_worker::service;
use std::path::PathBuf;

const USAGE: &str = "\
crundi-worker: lets a Crundi server use this PC (info, pause, a terminal).

Usage:
  crundi-worker                         tray app if there is a desktop, else headless
  crundi-worker --headless              no tray, logs to stderr
  crundi-worker join <server> <code> [--name N] [--work-dir D]
                                        trade a join code for a key and save it
  crundi-worker --key <cw_...> --server <url> [--work-dir D] --embedded
                                        run with a key from the PC client (not saved);
                                        JSON status lines on stdout, exits when stdin closes
  crundi-worker status                  print the config and whether joined
  crundi-worker install-service         Linux: systemd user unit (headless), enabled now
  crundi-worker uninstall-service
  crundi-worker autostart on|off        start the tray app at login

Options:
  --config <path>   config file (default: <config dir>/crundi-worker/config.json)
  --name <name>     name for this PC
  --work-dir <dir>  work folder (default: ~/crundi-work)
  --kind <kind>     kind reported to the server (tray|headless|electron|native)
  -V, --version     print the version
";

#[derive(Default)]
struct Args {
    config: Option<PathBuf>,
    headless: bool,
    embedded: bool,
    key: Option<String>,
    server: Option<String>,
    work_dir: Option<PathBuf>,
    name: Option<String>,
    kind: Option<String>,
    positional: Vec<String>,
}

fn parse() -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut take = |f: &str| -> Result<String, String> {
            match &inline {
                Some(v) => Ok(v.clone()),
                None => val(f),
            }
        };
        match flag.as_str() {
            "--config" => a.config = Some(PathBuf::from(take("--config")?)),
            "--headless" => a.headless = true,
            "--embedded" => a.embedded = true,
            "--key" => a.key = Some(take("--key")?),
            "--server" => a.server = Some(take("--server")?),
            "--work-dir" => a.work_dir = Some(PathBuf::from(take("--work-dir")?)),
            "--name" => a.name = Some(take("--name")?),
            "--kind" => a.kind = Some(take("--kind")?),
            "-h" | "--help" | "help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("crundi-worker {}", crundi_worker::util::VERSION);
                std::process::exit(0);
            }
            f if f.starts_with('-') => return Err(format!("unknown option {f}")),
            _ => a.positional.push(arg),
        }
    }
    Ok(a)
}

fn fail(msg: impl std::fmt::Display, code: i32) -> ! {
    eprintln!("crundi-worker: {msg}");
    std::process::exit(code);
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap_or_else(|e| fail(format!("could not start: {e}"), 1))
}

#[cfg(windows)]
fn attach_console() {
    // With windows_subsystem = "windows" there is no console; attach to the
    // parent's so CLI output shows in the terminal that ran us.
    extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    unsafe {
        AttachConsole(0xFFFF_FFFF);
    }
}

fn main() {
    #[cfg(windows)]
    attach_console();
    crundi_worker::util::install_crypto();
    let a = parse().unwrap_or_else(|e| fail(format!("{e}\n\n{USAGE}"), 64));
    let config_path = a.config.clone().map(|p| absolute(&p)).unwrap_or_else(default_config_path);
    let custom_config = a.config.as_ref().map(|_| config_path.as_path());

    match a.positional.first().map(String::as_str) {
        Some("join") => {
            let (Some(server), Some(code)) = (a.positional.get(1), a.positional.get(2)) else {
                fail("usage: crundi-worker join <server-url> <code> [--name N] [--work-dir D]", 64)
            };
            let kind = a.kind.clone().unwrap_or_else(|| default_kind(a.headless).into());
            match join_and_save(
                &config_path,
                JoinArgs { server, code, name: a.name.as_deref(), work_dir: a.work_dir.clone(), kind: &kind },
            ) {
                Ok(cfg) => {
                    println!(
                        "Joined {} as \"{}\" (worker {}). Saved to {}.",
                        cfg.server.as_deref().unwrap_or(""),
                        cfg.name.as_deref().unwrap_or(""),
                        cfg.id.as_deref().unwrap_or(""),
                        config_path.display()
                    );
                    println!("Work folder: {}", cfg.work_dir.display());
                    println!("Run it now with: crundi-worker{}", if cfg!(feature = "tray") { "" } else { " --headless" });
                }
                Err(e) => fail(format!("{e:#}"), 1),
            }
        }
        Some("status") => {
            let cfg = Config::load(&config_path).unwrap_or_else(|e| fail(format!("{e:#}"), 1));
            println!("config:      {}", config_path.display());
            println!("joined:      {}", if cfg.joined() { "yes" } else { "no" });
            println!("server:      {}", cfg.server.as_deref().unwrap_or("-"));
            println!("worker id:   {}", cfg.id.as_deref().unwrap_or("-"));
            println!("key:         {}", cfg.key.as_deref().map(mask_key).unwrap_or_else(|| "-".into()));
            println!("name:        {}", cfg.display_name());
            println!("work folder: {}", cfg.work_dir.display());
            println!("paused:      {}", cfg.paused);
            println!("allow:       chats {}, commands {}", cfg.allow_chats, cfg.allow_commands);
            println!("start at login: {}", if service::autostart_enabled() { "on" } else { "off" });
            std::process::exit(if cfg.joined() { 0 } else { 1 });
        }
        Some("install-service") => match service::install_service(custom_config) {
            Ok(m) => println!("{m}"),
            Err(e) => fail(format!("{e:#}"), 1),
        },
        Some("uninstall-service") => match service::uninstall_service() {
            Ok(m) => println!("{m}"),
            Err(e) => fail(format!("{e:#}"), 1),
        },
        Some("autostart") => {
            let on = match a.positional.get(1).map(String::as_str) {
                Some("on") => true,
                Some("off") => false,
                _ => fail("usage: crundi-worker autostart on|off", 64),
            };
            match service::set_autostart(on, custom_config) {
                Ok(m) => println!("{m}"),
                Err(e) => fail(format!("{e:#}"), 1),
            }
        }
        Some(other) => fail(format!("unknown command {other}\n\n{USAGE}"), 64),
        None => std::process::exit(run(a, config_path)),
    }
}

fn mask_key(k: &str) -> String {
    match k.split_once('.') {
        Some((id, _)) => format!("{id}.****"),
        None => "****".into(),
    }
}

fn default_kind(headless: bool) -> &'static str {
    if headless || !cfg!(feature = "tray") {
        "headless"
    } else {
        "tray"
    }
}

/// Run the worker (no subcommand).
fn run(a: Args, config_path: PathBuf) -> i32 {
    // A key given on the command line: run with it, never save it.
    if a.key.is_some() || a.embedded {
        let (Some(key), Some(server)) = (a.key.clone(), a.server.clone()) else {
            fail("--embedded needs --key and --server", 64)
        };
        let saved = Config::load(&config_path).unwrap_or_default();
        let server = crundi_worker::util::normalize_server(&server).unwrap_or_else(|e| fail(e, 64));
        let cfg = Config {
            server: Some(server),
            key: Some(key.clone()),
            id: key.strip_prefix("cw_").and_then(|r| r.split_once('.')).map(|(id, _)| id.to_string()),
            name: a.name.clone().or(saved.name),
            work_dir: a.work_dir.as_deref().map(absolute).unwrap_or(saved.work_dir),
            device_id: saved.device_id,
            ..Config::default()
        };
        let kind = a.kind.clone().unwrap_or_else(|| if a.embedded { "electron".into() } else { "headless".into() });
        let opts = LinkOptions::new(&kind, None);
        let rt = runtime();
        return if a.embedded {
            rt.block_on(app::embedded(cfg, opts))
        } else {
            rt.block_on(app::headless(cfg, opts))
        };
    }

    let mut cfg = Config::load(&config_path).unwrap_or_else(|e| fail(format!("{e:#}"), 1));
    if let Some(wd) = &a.work_dir {
        cfg.work_dir = absolute(wd);
        let _ = cfg.save(&config_path);
    }

    #[cfg(feature = "tray")]
    if !a.headless && crundi_worker::tray::desktop_available() {
        let kind = a.kind.clone().unwrap_or_else(|| "tray".into());
        match crundi_worker::tray::run(config_path.clone(), a.config.is_some(), kind) {
            Ok(code) => return code,
            Err(e) => eprintln!("crundi-worker: no tray ({e:#}); running headless"),
        }
    }

    let kind = a.kind.clone().unwrap_or_else(|| "headless".into());
    let opts = LinkOptions::new(&kind, Some(config_path));
    runtime().block_on(app::headless(cfg, opts))
}
