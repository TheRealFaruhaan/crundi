//! The browser bridge (Desktop's, AREAS.md): off-screen Chromium browsers
//! (CEF), one per page, whose frames are drawn as GPUI images, with the
//! mouse, wheel and keys passed back, and the marks drawn over the page.
//!
//! Chromium runs in a process of its own, the browser host: its own
//! executable, `crundi-browser-host` (crates/browser-host), next to the
//! app's, started lazily when a page is first needed and stopped when no
//! page has been open for a while (client.rs). Only the host links libcef:
//! the app never maps it. The app keeps the same API as when CEF ran inside
//! it:
//!
//! - `engine`: the panels (drawer tabs, hidden browsers, widget panels):
//!   their state, devices, input, marks, console, network log, CDP;
//! - `frames`: live frames (a chat's embeds, Cru's screen, What's new);
//! - `view`: `PageView`, a GPUI view of one panel;
//! - `net`: the drawer's networks (the server's, through the host's socket);
//! - `shot`: a page's picture with its marks, and clearing browsing data;
//! - `client`: the host process and its link; `ipc`, `shm`, `proc`,
//!   `common` come from crates/browser-link (shared with the host).
//!
//! The host's events are handed out from GPUI's main thread (`pump`, every
//! 8 ms from the app). `later` runs a call on that thread after a while.

mod client;
pub mod engine;
pub mod frames;
pub mod net;
pub mod shot;
mod view;
pub use crundi_browser_link::{common, gpu, ipc, proc, shm};

pub use engine::{Ann, Device, Hooks, DEVICES};
pub use view::PageView;
pub use client::{idle_grace, set_keep_alive, unload_grace};

use std::{cell::RefCell, time::{Duration, Instant}};

/// First thing in main, before GPUI. The app is never CEF: when it is
/// started as the browser host (`--browser-host`, as before the host had an
/// executable of its own) or as a CEF helper (`--type=…`), it becomes
/// `crundi-browser-host` with the same arguments (exec: the same PID on
/// Linux and macOS) and never returns. Otherwise nothing happens.
pub fn init_process() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let host = args.iter().any(|a| a == proc::FLAG);
    let helper = args.iter().any(|a| a.to_string_lossy().starts_with("--type="));
    if !host && !helper { return; }
    let exe = match proc::host_exe() {
        Ok(e) => e,
        Err(e) => { eprintln!("[browser] {e}"); std::process::exit(1) }
    };
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(&args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec();
        eprintln!("[browser] {} did not start: {e}", exe.display());
        std::process::exit(1)
    }
    #[cfg(not(unix))]
    {
        let code = cmd.status().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
        std::process::exit(code)
    }
}

/// At the app's start (once GPUI's app exists): a browser host a crashed
/// run left is reaped, and stale CEF locks go. The host is not started (it
/// starts with the first page). `config_dir`: where its PID is kept. False:
/// no browser this run (CRUNDI_NO_BROWSER).
pub fn start(config_dir: &std::path::Path) -> bool {
    let ok = client::start(config_dir.to_path_buf());
    STARTED.with(|s| *s.borrow_mut() = ok);
    ok
}

thread_local! {
    static STARTED: RefCell<bool> = const { RefCell::new(false) };
    static TIMERS: RefCell<Vec<(Instant, Box<dyn FnOnce()>)>> = RefCell::new(Vec::new());
}

/// The browser is there to use (its host may not be running yet: it starts
/// on the first page). False when it can't start this run.
pub fn started() -> bool { STARTED.with(|s| *s.borrow()) && client::available() }

/// Run `f` on the main thread after `after` (at the pump after it is due).
pub fn later(after: Duration, f: impl FnOnce() + 'static) {
    TIMERS.with(|t| t.borrow_mut().push((Instant::now() + after, Box::new(f))));
}

/// The pump (every few ms, main thread): the calls deferred from the
/// window's handlers, the timers that are due, then the host's events.
pub fn pump() {
    engine::run_deferred();
    let now = Instant::now();
    let due: Vec<Box<dyn FnOnce()>> = TIMERS.with(|t| {
        let mut t = t.borrow_mut();
        let mut due = vec![];
        let mut i = 0;
        while i < t.len() { if t[i].0 <= now { due.push(t.remove(i).1); } else { i += 1; } }
        due
    });
    for f in due { f(); }
    client::pump();
    engine::sync_hidden();
}

/// The browser host's state for the test driver (props "browserHost").
pub fn host_props() -> serde_json::Value { client::props() }

/// Stop the browser host now (the test driver; the idle stop does this by itself).
pub fn stop_host() { client::stop() }

/// Stop the browser host (after the window's loop ends).
pub fn shutdown() { client::shutdown(); }
