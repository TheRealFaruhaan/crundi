//! Crundi's native PC client on GPUI (AREAS.md): starts the window,
//! registers every part, and pumps the browser host's events and the live
//! link from the main thread. Thin: everything else is in the crates.
//!
//!   CRUNDI_CONFIG_DIR=<dir>   where the login is saved (crundi_core::store)
//!   CRUNDI_CEF_CACHE=<dir>    CEF's profile (one per client running at once)
//!   CRUNDI_TEST_PORT=<port>   the test driver (driver.rs; the Slint client's protocol)
//!   CRUNDI_URL=<url>          what the browser drawer opens first
//!   CRUNDI_BROWSER_IDLE_SECS=<s>  how long the browser host stays with no page (60)
//!
//! Chromium runs in a process of its own, this same binary started with
//! --browser-host, when a page is first needed (crundi_webview); CEF starts
//! it again for its helper processes (--type=…). crundi_webview::init_process()
//! runs those first, and they never reach the window.

mod driver;

use crundi_theme::T;
use gpui::*;
use gpui_component::Root;
use std::{sync::mpsc, time::Duration};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn,zbus=off")).init();
    crundi_webview::init_process();

    gpui_platform::application().with_assets(crundi_theme::AllAssets).run(move |cx: &mut App| {
        let cef = crundi_webview::start(&crundi_core::store::config_dir());
        gpui_component::init(cx);
        crundi_theme::init(cx);
        // The parts, before the session starts (they hear sign-in).
        crundi_workbench::init(cx);
        crundi_chats::init(cx);
        crundi_workspace::init(cx);
        crundi_planning::init(cx);
        crundi_operations::init(cx);
        crundi_shell::init(cx);

        // Centred on the display when there is one; Xvfb reports none, so a
        // window centred on "nothing" would come out 10x10: place it instead.
        let wsize = size(T::WINDOW_W, T::WINDOW_H);
        let bounds = if cx.primary_display().is_some() { Bounds::centered(None, wsize, cx) } else { Bounds { origin: point(px(0.), px(0.)), size: wsize } };
        // Frameless, as the PC client: the top bar draws the window's buttons and moves it.
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(T::WINDOW_MIN_W, T::WINDOW_MIN_H)),
            titlebar: None,
            window_decorations: Some(WindowDecorations::Client),
            app_id: Some("crundi".into()),
            ..Default::default()
        };
        let handle = cx.open_window(opts, |window, cx| {
            let shell = cx.new(|cx| crundi_shell::Shell::new(window, cx));
            if cef { crundi_desktop::init(window, cx); }
            cx.new(|cx| Root::new(shell, window, cx))
        }).expect("window");

        // The test driver: commands come in on its threads, are answered here.
        let (tx, rx) = mpsc::channel::<crundi_core::driver::Request>();
        if let Some(port) = crundi_core::driver::port() {
            crundi_core::driver::start(port, move |r| { let _ = tx.send(r); });
        }

        // One pump for everything from outside the UI: CEF's work (its
        // callbacks run here), the live link's events, the driver's commands.
        // (The window, not its Root view: the parts and the driver read Root,
        // which a WindowHandle<Root>::update would hold.)
        let any: AnyWindowHandle = handle.into();
        cx.spawn(async move |cx| loop {
            cx.background_executor().timer(Duration::from_millis(8)).await;
            let ok = cx.update_window(any, |_, window, cx| {
                crundi_webview::pump();
                crundi_desktop::pump(window, cx);
                crundi_shell::session::pump(cx);
                while let Ok(r) = rx.try_recv() { driver::handle(r, window, cx); }
            });
            if ok.is_err() { break; }
        }).detach();
    });
    crundi_webview::shutdown();
}
