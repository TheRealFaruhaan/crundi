//! The browser bridge (Desktop's, AREAS.md): off-screen Chromium browsers
//! (CEF), one per page, whose frames are drawn as GPUI images, with the
//! mouse, wheel and keys passed back, and the marks drawn over the page.
//!
//! Chromium runs in a process of its own, the browser host: this same
//! binary started with `--browser-host` (host/), lazily, when a page is
//! first needed, and stopped when no page has been open for a while
//! (client.rs). The app keeps the same API as when CEF ran inside it:
//!
//! - `engine`: the panels (drawer tabs, hidden browsers, widget panels):
//!   their state, devices, input, marks, console, network log, CDP;
//! - `frames`: live frames (a chat's embeds, Cru's screen, What's new);
//! - `view`: `PageView`, a GPUI view of one panel;
//! - `net`: the drawer's networks (the server's, through the host's socket);
//! - `shot`: a page's picture with its marks, and clearing browsing data;
//! - `client`, `ipc`, `shm`, `proc`: the host process, its link, the frame
//!   segments, and keeping it tied to the app's life.
//!
//! The host's events are handed out from GPUI's main thread (`pump`, every
//! 8 ms from the app). `later` runs a call on that thread after a while.
//!
//! `cef::*` and `gpui::*` both have Window, Point and KeyEvent: CEF's code
//! lives in its own modules (`host`, `cef_app`, `mac`).

mod client;
pub mod common;
pub mod engine;
pub mod frames;
mod host;
pub mod ipc;
pub mod net;
pub mod proc;
pub mod shm;
pub mod shot;
mod view;

pub use engine::{Ann, Device, Hooks, DEVICES};
pub use view::PageView;
pub use client::{idle_grace, set_keep_alive};

use std::{cell::RefCell, time::{Duration, Instant}};

mod cef_app {
    use cef::*;
    wrap_app! {
        pub struct NativeApp;
        impl App {
            fn on_before_command_line_processing(&self, process_type: Option<&CefString>, command_line: Option<&mut CommandLine>) {
                if process_type.map(|t| !t.to_string().is_empty()).unwrap_or(false) { return; }
                let Some(cl) = command_line else { return };
                // The CPU path, as the Slint client's default: software
                // compositing, frames to on_paint as buffers.
                for s in ["disable-gpu", "disable-gpu-compositing", "enable-begin-frame-scheduling"] {
                    cl.append_switch(Some(&CefString::from(s)));
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod mac;

/// First thing in main, before GPUI. CEF's helper processes (this binary
/// again, `--type=…`) run and exit here; the browser host
/// (`--browser-host`) runs here and never returns. The app itself goes on,
/// without CEF loaded or started.
pub fn init_process() {
    let host = std::env::args().any(|a| a == proc::FLAG);
    let helper = std::env::args().any(|a| a.starts_with("--type="));
    if !host && !helper { return; }
    #[cfg(target_os = "macos")]
    {
        let loader = mac::load_framework(helper);
        std::mem::forget(loader);
    }
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();
    let mut app = cef_app::NativeApp::new();
    let code = cef::execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
    if code >= 0 { std::process::exit(code); }
    if host { host::run(); }
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
}

/// The browser host's state for the test driver (props "browserHost").
pub fn host_props() -> serde_json::Value { client::props() }

/// Stop the browser host now (the test driver; the idle stop does this by itself).
pub fn stop_host() { client::stop() }

/// Stop the browser host (after the window's loop ends).
pub fn shutdown() { client::shutdown(); }
