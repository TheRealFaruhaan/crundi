//! The CEF bridge (Desktop's, AREAS.md): off-screen (windowless) Chromium
//! browsers, one per panel, whose frames are drawn as GPUI images, with the
//! mouse, wheel and keys passed back, and the marks drawn over the page.
//!
//! - `engine`: the panels (ported from the Slint client's src/browser.rs):
//!   CEF's handlers, CDP, devices, input, marks, console, network log.
//! - `view`: `PageView`, a GPUI view of one panel.
//! - `net`: the drawer's networks (the server's, through the host's socket).
//! - `shot`: a page's picture with its marks, and clearing browsing data.
//!
//! CEF's message loop is pumped from GPUI's main thread (`pump`, every 8 ms
//! from the app), so every CEF callback runs on that thread too. `later`
//! runs a call on that thread after a while (the pump's timers).
//!
//! `cef::*` and `gpui::*` both have Window, Point and KeyEvent: the CEF side
//! lives in its own modules (`engine`, `net`, `shot`, `cef_app`).

pub mod engine;
pub mod net;
pub mod shot;
mod view;

pub use engine::{Ann, Device, Hooks, DEVICES};
pub use view::PageView;

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

/// CEF's helper processes: CEF runs this same binary again for them
/// (--type=…); for those this never returns (they run and exit). Call first
/// thing in main, before GPUI. On macOS it also loads CEF's framework.
pub fn init_process() {
    #[cfg(target_os = "macos")]
    {
        let loader = mac::load_framework(std::env::args().any(|a| a.starts_with("--type=")));
        std::mem::forget(loader);
    }
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();
    let mut app = cef_app::NativeApp::new();
    let code = cef::execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
    if code >= 0 { std::process::exit(code); }
}

/// Start CEF, once GPUI's app exists (on macOS NSApp must be GPUI's, and
/// conform to CEF's protocol, before CEF starts). False: CEF did not start
/// (the client runs without a browser).
pub fn start() -> bool {
    #[cfg(target_os = "macos")]
    mac::adopt_cef_app_protocol();
    let args = cef::args::Args::new();
    let mut app = cef_app::NativeApp::new();
    // CEF's files sit next to the binary (native-client/scripts/stage-cef.mjs);
    // CRUNDI_CEF_CACHE gives each client running at once a profile of its own.
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    let cache = engine::cef_root();
    let _ = std::fs::create_dir_all(&cache);
    let mut settings = cef::Settings {
        no_sandbox: 1,
        windowless_rendering_enabled: 1,
        multi_threaded_message_loop: 0,
        external_message_pump: 0,
        root_cache_path: cef::CefString::from(cache.to_string_lossy().as_ref()),
        log_severity: cef::LogSeverity::WARNING,
        ..Default::default()
    };
    // On macOS the framework finds its own resources; elsewhere they sit next to the binary.
    if !cfg!(target_os = "macos") {
        settings.resources_dir_path = cef::CefString::from(exe_dir.to_string_lossy().as_ref());
        settings.locales_dir_path = cef::CefString::from(exe_dir.join("locales").to_string_lossy().as_ref());
    }
    let ok = cef::initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut()) == 1;
    if !ok { eprintln!("[gpui] CEF did not start"); }
    STARTED.with(|s| *s.borrow_mut() = ok);
    ok
}

thread_local! {
    static STARTED: RefCell<bool> = const { RefCell::new(false) };
    static TIMERS: RefCell<Vec<(Instant, Box<dyn FnOnce()>)>> = RefCell::new(Vec::new());
}

/// CEF has started (the browser is there).
pub fn started() -> bool { STARTED.with(|s| *s.borrow()) }

/// Run `f` on the main thread after `after` (at the pump after it is due).
pub fn later(after: Duration, f: impl FnOnce() + 'static) {
    TIMERS.with(|t| t.borrow_mut().push((Instant::now() + after, Box::new(f))));
}

/// The pump (every few ms, main thread): the calls deferred from the
/// window's handlers, the timers that are due, then CEF's own work.
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
    if started() { cef::do_message_loop_work(); }
}

/// Stop CEF (after the window's loop ends).
pub fn shutdown() { if started() { cef::shutdown(); } }
