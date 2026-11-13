//! Crundi's desktop client, native prototype: Rust + Slint for the interface,
//! CEF (Chromium Embedded Framework, through cef-rs) for the browser panels,
//! rendered off-screen so the pages are part of Slint's layout and anything can
//! be drawn over them.
//!
//!   CRUNDI_SERVER=ws://host:port/ws?token=…   control from a Crundi server (/ws, as a native host)
//!   CRUNDI_URL_A / CRUNDI_URL_B                what the two panels open first
//!
//! One binary is also CEF's helper processes (renderer, GPU, utility): CEF
//! starts it again with --type=…, and execute_process() handles those first.

slint::include_modules!();

mod browser;
#[cfg(feature = "gpu")]
mod gpu;
#[cfg(target_os = "macos")]
mod mac;
mod pump;
mod server;

use cef::*;
use std::{rc::Rc, time::Duration};

wrap_app! {
    struct NativeApp;
    impl App {
        fn on_before_command_line_processing(&self, process_type: Option<&CefString>, command_line: Option<&mut CommandLine>) {
            // The browser process decides; the helpers inherit its switches.
            if process_type.map(|t| !t.to_string().is_empty()).unwrap_or(false) { return; }
            let Some(cl) = command_line else { return };
            // The CPU path: software compositing, frames come to on_paint as
            // buffers. The GPU path (feature "gpu", CRUNDI_GPU=1) keeps the GPU
            // and takes frames as shared textures (on_accelerated_paint).
            let switches: &[&str] = if browser::gpu_on() { &[] } else { &["disable-gpu", "disable-gpu-compositing", "enable-begin-frame-scheduling"] };
            for s in switches {
                cl.append_switch(Some(&CefString::from(*s)));
            }
        }
    }
}

fn main() {
    // The API version this crate was built against: must come first.
    // macOS: CEF is a framework in the bundle, loaded by hand (helpers are
    // the same binary, in helper apps three levels deeper).
    #[cfg(target_os = "macos")]
    let _framework = mac::load_framework(std::env::args().any(|a| a.starts_with("--type=")));

    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = args::Args::new();
    let mut app = NativeApp::new();

    // A CEF helper process (renderer, GPU…): run it and leave.
    let code = execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
    if code >= 0 {
        std::process::exit(code);
    }

    // CEF's files sit next to the binary (scripts/stage-cef.sh).
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    let cache = dirs_cache().join("crundi-native");
    let _ = std::fs::create_dir_all(&cache);
    let mut settings = Settings {
        no_sandbox: 1,
        windowless_rendering_enabled: 1,
        // Pumped from Slint's event loop (below), the same way on every OS.
        multi_threaded_message_loop: 0,
        external_message_pump: 0,
        root_cache_path: CefString::from(cache.to_string_lossy().as_ref()),
        log_severity: LogSeverity::WARNING,
        ..Default::default()
    };
    // On macOS the framework finds its own resources; elsewhere they sit next to the binary.
    if !cfg!(target_os = "macos") {
        settings.resources_dir_path = CefString::from(exe_dir.to_string_lossy().as_ref());
        settings.locales_dir_path = CefString::from(exe_dir.join("locales").to_string_lossy().as_ref());
    }

    // The window first: on macOS this is what creates NSApp (winit's), which
    // CEF needs to find implementing its protocol.
    // Slint's renderer: wgpu for the GPU path, else the software renderer
    // (named, so that having wgpu compiled in does not change the default).
    #[cfg(feature = "gpu")]
    let gpu_path = gpu::select_backend();
    #[cfg(not(feature = "gpu"))]
    let gpu_path = false;
    if !gpu_path && std::env::var_os("SLINT_BACKEND").is_none() {
        let _ = slint::BackendSelector::new().backend_name("winit".into()).renderer_name("software".into()).select();
    }

    let ui = MainWindow::new().expect("window");
    #[cfg(feature = "gpu")]
    if gpu_path { gpu::attach(ui.window(), browser::invalidate_all); }
    #[cfg(target_os = "macos")]
    mac::adopt_cef_app_protocol();

    if initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut()) != 1 {
        eprintln!("[native] CEF did not start");
        std::process::exit(1);
    }

    let model = Rc::new(slint::VecModel::from(vec![PanelData::default(), PanelData::default()]));
    ui.set_panels(model.clone().into());
    browser::STATE.with(|s| {
        *s.borrow_mut() = Some(browser::State { panels: vec![browser::Panel::default(), browser::Panel::default()], model, ui: ui.as_weak() });
    });

    // Slint → the bridge (browser::defer: on macOS, after winit's event handling).
    let weak = ui.as_weak();
    ui.on_pointer(move |i, kind, x, y, b, _w, _h| {
        let annotating = weak.upgrade().map(|u| u.get_annotating()).unwrap_or(false);
        browser::defer(move || browser::pointer(i as usize, kind, x, y, b, annotating));
    });
    ui.on_wheel(|i, x, y, dx, dy, _w, _h| browser::defer(move || browser::wheel(i as usize, x, y, dx, dy)));
    ui.on_key(|i, t, d, c, s| browser::defer(move || browser::key(i as usize, t.as_str(), d, c, s)));
    ui.on_resized(|i, w, h| browser::defer(move || browser::resized(i as usize, w, h)));
    ui.on_navigate(|i, u| {
        let u = u.to_string();
        let u = if u.contains("://") || u.starts_with("about:") { u } else { format!("https://{u}") };
        browser::defer(move || browser::navigate(i as usize, &u, Box::new(|| {})));
    });
    ui.on_set_device(|i, d| browser::defer(move || browser::set_device(i as usize, d as usize)));
    ui.on_clear_marks(|i| browser::defer(move || browser::clear_marks(i as usize)));

    let a = std::env::var("CRUNDI_URL_A").unwrap_or_else(|_| "https://example.com".into());
    let b = std::env::var("CRUNDI_URL_B").unwrap_or_else(|_| "https://www.wikipedia.org".into());
    browser::create(0, &a);
    browser::create(1, &b);

    // CEF's message loop (src/pump.rs).
    let _pump = pump::start(8);

    match std::env::var("CRUNDI_SERVER") {
        Ok(url) if !url.is_empty() => server::start(url),
        _ => browser::status("Not connected to a Crundi server (set CRUNDI_SERVER)"),
    }

    ui.run().expect("event loop");

    // Close the browsers, let CEF finish, then shut it down.
    let browsers: Vec<_> = browser::with(|st| st.panels.iter().filter_map(|p| p.browser.clone()).collect());
    for b in &browsers { if let Some(h) = b.host() { h.close_browser(1); } }
    for _ in 0..50 { do_message_loop_work(); std::thread::sleep(Duration::from_millis(10)); }
    drop(browsers);
    shutdown();
}

fn dirs_cache() -> std::path::PathBuf {
    std::env::var_os("XDG_CACHE_HOME").map(Into::into)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(Into::into))
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
}
