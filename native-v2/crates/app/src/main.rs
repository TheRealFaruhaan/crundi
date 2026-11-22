//! Crundi's native PC client v2 (ARCHITECTURE.md): makes the shared stores,
//! registers every area, opens the window and answers the test driver.
//! Thin: everything else is in the crates. Nothing here polls: the live
//! link and the driver each wake the UI thread through a channel it awaits.
//!
//!   CRUNDI_CONFIG_DIR=<dir>   where the login is saved (crundi_core::store)
//!   CRUNDI_TEST_PORT=<port>   the test driver (driver.rs)
//!   ZED_MEASUREMENTS=1        gpui prints each drawn frame's duration on
//!                             stderr ("frame duration: …"): tools/frames.mjs

mod driver;
#[cfg(feature = "frame-journal")]
mod journal;

use crundi_theme::T;
use futures::StreamExt as _;
use gpui::*;
use gpui_component::Root;

fn main() {
    // Started as the browser host or a CEF helper: become the host (never returns).
    crundi_webview::init_process();
    // The log: stderr and crundi.log in the config folder (Electron's), with the boot lines.
    crundi_shell::lifecycle::init_log("info,wgpu_core=warn,wgpu_hal=warn,naga=warn,zbus=off");
    // One copy at a time (requestSingleInstanceLock): a second launch shows the first's window and goes.
    if !crundi_shell::lifecycle::claim() {
        crundi_shell::lifecycle::log_line("[lifecycle] Another instance is running; showing it");
        return;
    }
    // The app quits when asked (Quit, the × without a tray), never because its
    // window closed: on Wayland the × closes it to hide it in the tray (F127).
    gpui_platform::application().with_assets(crundi_theme::AllAssets).with_quit_mode(QuitMode::Explicit).run(move |cx: &mut App| {
        gpui_component::init(cx);
        #[cfg(feature = "frame-journal")]
        journal::start(cx);
        // A chat code block's colours: the syntax set loads off the UI thread.
        crundi_core::syntax::warm();
        crundi_theme::init(cx);
        crundi_widgets::init(cx);
        let stores = crundi_shell::init(cx);
        // The areas, before the session starts (they hear sign-in).
        crundi_workbench::init(cx);
        crundi_chat::init(cx);
        crundi_planning::init(cx);
        crundi_workspace::init(cx);
        crundi_operations::init(cx);
        // The desktop area (the browser drawer, Inbox, tray, Cru…). The
        // browser host is not started here: it starts with the first page.
        crundi_webview::start(&crundi_core::store::config_dir());
        crundi_desktop::init(cx);

        // Centred on the display when there is one; Xvfb reports none, so a
        // window centred on "nothing" would come out 10x10: place it instead.
        let wsize = size(T::WINDOW_W, T::WINDOW_H);
        let bounds = if cx.primary_display().is_some() { Bounds::centered(None, wsize, cx) } else { Bounds { origin: point(px(0.), px(0.)), size: wsize } };
        let handle = open_main(&stores, bounds, cx);
        // In front with the keys, as Electron's mainWindow.show() and focus().
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        // The tray's Open after Wayland's hide: the window again, on the same entities.
        let s = stores.clone();
        crundi_desktop::set_reopen(cx, move |b, cx| { let h = open_main(&s, b, cx); let _ = h.update(cx, |_, window, _| window.activate_window()); });
        stores.session.update(cx, |s, cx| s.start(cx));

        // The test driver: its commands arrive on a channel the UI thread
        // awaits, and are answered in the window.
        if let Some(port) = crundi_core::driver::port() {
            let (tx, mut rx) = futures::channel::mpsc::unbounded::<crundi_core::driver::Request>();
            crundi_core::driver::start(port, move |r| { let _ = tx.unbounded_send(r); });
            let _ = handle;
            let s = stores.clone();
            cx.spawn(async move |cx: &mut AsyncApp| {
                while let Some(r) = rx.next().await {
                    let s = s.clone();
                    // The window there is now (another after Wayland's hide opened it
                    // again); none while hidden that way: the driver answers for the tray.
                    match cx.update(|cx| cx.windows().into_iter().next()) {
                        Some(w) => { let _ = cx.update_window(w, move |_, window, cx| driver::handle(r, &s, window, cx)); }
                        None => driver::windowless(r),
                    }
                }
            }).detach();
        }
    });
    crundi_webview::shutdown();
    // An update installed: the new version starts (or its installer runs) now the app is gone.
    crundi_desktop::updater::after_quit();
    crundi_shell::lifecycle::log_line("[lifecycle] Quit");
}

/// Open the window (at start, and again after Wayland's hide closed it):
/// the Shell over the shared stores, the desktop area's views.
fn open_main(stores: &crundi_shell::Stores, bounds: Bounds<Pixels>, cx: &mut App) -> WindowHandle<Root> {
    // Frameless, as the PC client: the top bar draws the window's buttons and moves it.
    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(T::WINDOW_MIN_W, T::WINDOW_MIN_H)),
        titlebar: None,
        window_decorations: Some(WindowDecorations::Client),
        app_id: Some("crundi".into()),
        icon: app_icon(),
        ..Default::default()
    };
    let s = stores.clone();
    cx.open_window(opts, move |window, cx| {
        // The system's close goes the ×'s way (crundi_shell::close_window).
        window.on_window_should_close(cx, |window, cx| { crundi_shell::close_window(window, cx); false });
        let shell = cx.new(|cx| crundi_shell::Shell::new(s.clone(), window, cx));
        crundi_desktop::init_window(&shell, window, cx);
        cx.new(|cx| Root::new(shell, window, cx))
    }).expect("window")
}

/// The app's icon, 256 px (assets/icon-256.png, the PC client's).
fn app_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    let png = include_bytes!("../assets/icon-256.png");
    image::load_from_memory_with_format(png, image::ImageFormat::Png).ok().map(|i| std::sync::Arc::new(i.to_rgba8()))
}
