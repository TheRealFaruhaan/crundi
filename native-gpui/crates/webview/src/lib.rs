//! The CEF bridge (Desktop's, AREAS.md): off-screen (windowless) Chromium
//! browsers whose frames are drawn as GPUI images, with the mouse, wheel and
//! keys passed back, and the annotation strokes drawn over the page.
//!
//! CEF's message loop is pumped from GPUI's main thread (`pump`, every 8 ms
//! from the app), so every CEF callback runs on that thread too. The page's
//! frame lives in a thread-local; the view picks up a new one on its pump.
//! Frames are BGRA (CPU path, as the Slint client's default), which GPUI's
//! images take with no conversion.
//!
//! `cef::*` and `gpui::*` both have Window, Point and KeyEvent: the CEF side
//! lives in its own modules (`engine`, `cef_app`).

use cef::ImplBrowserHost as _;
use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};
use std::{cell::RefCell, sync::Arc, time::Instant};

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
    let cache = std::env::var_os("CRUNDI_CEF_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("crundi-gpui-cef"));
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

thread_local! { static STARTED: RefCell<bool> = const { RefCell::new(false) }; }

/// CEF's work (call every few ms from the main thread).
pub fn pump() { if STARTED.with(|s| *s.borrow()) { cef::do_message_loop_work(); } }

/// Stop CEF (after the window's loop ends).
pub fn shutdown() { if STARTED.with(|s| *s.borrow()) { cef::shutdown(); } }

/// What CEF's callbacks and the view share (main thread only).
#[derive(Default)]
struct Shared {
    browser: Option<cef::Browser>,
    /// The view's size in logical px, and the window's scale.
    w: i32,
    h: i32,
    scale: f32,
    /// The newest frame: BGRA, its size in device px, and a count.
    frame: Option<(Vec<u8>, u32, u32)>,
    frames: u64,
    title: String,
    url: String,
    /// Paint times (ms from on_paint to the frame being in the view's image).
    paint_ms: Vec<f32>,
    painted_at: Option<Instant>,
    can_back: bool,
    can_forward: bool,
    loading: bool,
}

thread_local! {
    static SHARED: RefCell<Shared> = RefCell::new(Shared { scale: 1.0, ..Default::default() });
}

fn with<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    SHARED.with(|s| f(&mut s.borrow_mut()))
}

mod engine {
    //! CEF's side: the handlers and the browser itself.
    use super::with;
    use cef::*;
    use std::time::Instant;

// ─── CEF ───

wrap_render_handler! {
    struct PanelRender {}
    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut cef::Browser>, rect: Option<&mut cef::Rect>) {
            if let Some(r) = rect {
                let (w, h) = with(|s| (s.w, s.h));
                *r = cef::Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) };
            }
        }
        fn screen_info(&self, _browser: Option<&mut cef::Browser>, info: Option<&mut ScreenInfo>) -> ::std::os::raw::c_int {
            let Some(i) = info else { return 0 };
            let (w, h, scale) = with(|s| (s.w, s.h, s.scale));
            i.device_scale_factor = scale;
            i.rect = cef::Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) };
            i.available_rect = i.rect.clone();
            1
        }
        fn on_paint(&self, _browser: Option<&mut cef::Browser>, type_: PaintElementType, _dirty: Option<&[cef::Rect]>, buffer: *const u8, width: ::std::os::raw::c_int, height: ::std::os::raw::c_int) {
            if type_ != PaintElementType::VIEW || buffer.is_null() || width <= 0 || height <= 0 { return; }
            let (w, h) = (width as u32, height as u32);
            let src = unsafe { std::slice::from_raw_parts(buffer, (w * h * 4) as usize) };
            with(|s| {
                s.frame = Some((src.to_vec(), w, h));
                s.frames += 1;
                s.painted_at = Some(Instant::now());
            });
        }
    }
}

wrap_display_handler! {
    struct PanelDisplay {}
    impl DisplayHandler {
        fn on_title_change(&self, _browser: Option<&mut cef::Browser>, title: Option<&CefString>) {
            let t = title.map(|t| t.to_string()).unwrap_or_default();
            with(|s| s.title = t);
        }
        fn on_address_change(&self, _browser: Option<&mut cef::Browser>, frame: Option<&mut cef::Frame>, url: Option<&CefString>) {
            if frame.map(|f| f.is_main() == 0).unwrap_or(false) { return; }
            let u = url.map(|u| u.to_string()).unwrap_or_default();
            with(|s| s.url = u);
        }
    }
}

wrap_life_span_handler! {
    struct PanelLife {}
    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut cef::Browser>) {
            if let Some(b) = browser { let b = b.clone(); with(|s| s.browser = Some(b)); }
        }
    }
}

wrap_load_handler! {
    struct PanelLoad {}
    impl LoadHandler {
        fn on_loading_state_change(&self, _browser: Option<&mut cef::Browser>, is_loading: ::std::os::raw::c_int, back: ::std::os::raw::c_int, fwd: ::std::os::raw::c_int) {
            with(|s| { s.loading = is_loading != 0; s.can_back = back != 0; s.can_forward = fwd != 0; });
        }
    }
}

wrap_client! {
    struct PanelClient {
        render: RenderHandler,
        display: DisplayHandler,
        life: LifeSpanHandler,
        load: LoadHandler,
    }
    impl Client {
        fn load_handler(&self) -> Option<LoadHandler> { Some(self.load.clone()) }
        fn render_handler(&self) -> Option<RenderHandler> { Some(self.render.clone()) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(self.display.clone()) }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(self.life.clone()) }
    }
}

pub fn host() -> Option<BrowserHost> {
    with(|s| s.browser.as_ref().and_then(|b| b.host()))
}

/// Make the panel's browser, off-screen, at [url].
pub fn create(url: &str) {
    let mut client = PanelClient::new(PanelRender::new(), PanelDisplay::new(), PanelLife::new(), PanelLoad::new());
    let window_info = WindowInfo::default().set_as_windowless(Default::default());
    let settings = BrowserSettings { windowless_frame_rate: 60, background_color: 0xffff_ffff, ..Default::default() };
    browser_host_create_browser(Some(&window_info), Some(&mut client), Some(&CefString::from(url)), Some(&settings), None, None);
}

/// Back, forward, reload, or go to an address.
pub fn nav(what: &str, url: &str) {
    let Some(b) = with(|s| s.browser.clone()) else { return };
    match what {
        "back" => b.go_back(),
        "forward" => b.go_forward(),
        "reload" => b.reload(),
        _ => if let Some(f) = b.main_frame() { f.load_url(Some(&CefString::from(url))); },
    }
}

/// Close the tab's browser (the drawer's x).
pub fn close() {
    let b = with(|s| { s.frame = None; s.title.clear(); s.url.clear(); s.browser.take() });
    if let Some(h) = b.and_then(|b| b.host()) { h.close_browser(1); }
}

}

pub use engine::{close, create, host, nav};

/// What the page shows now: its title, address, and whether it can go back and forward.
pub fn page_state() -> (String, String, bool, bool) {
    with(|s| (s.title.clone(), s.url.clone(), s.can_back, s.can_forward))
}

/// The sizes a page can take (the PC client's list): (name, width, height).
/// "Responsive" is the drawer's own size.
pub const DEVICES: &[(&str, i32, i32)] = &[
    ("Responsive", 0, 0),
    ("iPhone 15 Pro", 393, 852),
    ("Pixel 7", 412, 915),
    ("iPad Air", 820, 1180),
    ("Desktop 1280", 1280, 800),
];

/// The page: the browser's newest frame, at its size (a device size scaled
/// to fit and centred), the mouse, wheel and keys passed to it, and the
/// annotation strokes drawn over it while annotating.
pub struct WebPage {
    image: Option<Arc<RenderImage>>,
    seen: u64,
    /// Where the page is in the window (for the mouse), from the last layout.
    bounds: Bounds<Pixels>,
    pub annotate: bool,
    /// Annotation strokes, in page px.
    pub strokes: Vec<Vec<Point<Pixels>>>,
    drawing: bool,
    focus: FocusHandle,
    /// A tab is open (its browser made or being made).
    pub tab: bool,
    created: bool,
    pub home: String,
    pub device: usize,
    /// The page's place in the stage (scaled to fit for a device size), and its scale.
    page_rect: Bounds<Pixels>,
    fit: f32,
}

impl WebPage {
    pub fn new(home: String, cx: &mut Context<Self>) -> Self {
        WebPage { image: None, seen: 0, bounds: Bounds::default(), annotate: false, strokes: Vec::new(), drawing: false, focus: cx.focus_handle(), tab: true, created: false, home, device: 0, page_rect: Bounds::default(), fit: 1.0 }
    }

    /// Open a tab at `home` (the browser is made at the next layout).
    pub fn open_tab(&mut self, cx: &mut Context<Self>) {
        self.tab = true;
        self.created = false;
        cx.notify();
    }

    /// Close the tab and its browser.
    pub fn close_tab(&mut self, cx: &mut Context<Self>) {
        close();
        self.tab = false;
        self.created = false;
        self.annotate = false;
        self.strokes.clear();
        self.image = None;
        cx.notify();
    }

    /// A device size from DEVICES (0: the drawer's own).
    pub fn set_device(&mut self, i: usize, cx: &mut Context<Self>) {
        self.device = i.min(DEVICES.len() - 1);
        if let Some(h) = host() { h.notify_screen_info_changed(); h.was_resized(); }
        cx.notify();
    }

    /// After each CEF pump: a new frame becomes the view's image, and the address follows the page.
    pub fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fresh = with(|s| if s.frames != self.seen { self.seen = s.frames; s.frame.take() } else { None });
        let Some((data, w, h)) = fresh else { return };
        let Some(buf) = image::RgbaImage::from_raw(w, h, data) else { return };
        if let Some(old) = self.image.replace(Arc::new(RenderImage::new([image::Frame::new(buf)]))) {
            let _ = window.drop_image(old);
        }
        with(|s| if let Some(t) = s.painted_at.take() {
            s.paint_ms.push(t.elapsed().as_secs_f32() * 1000.0);
            if s.paint_ms.len() > 240 { s.paint_ms.remove(0); }
        });
        cx.notify();
    }

    /// Frames so far, and the median paint-to-view time in ms.
    pub fn stats() -> (u64, f32) {
        with(|s| {
            let mut v = s.paint_ms.clone();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            (s.frames, v.get(v.len() / 2).copied().unwrap_or(0.0))
        })
    }

    /// The page's size follows the drawer's; the browser is made at the first layout.
    pub fn layout(&mut self, bounds: Bounds<Pixels>, scale: f32) {
        self.bounds = bounds;
        let (sw, sh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        // A device size: the page is that size, scaled down to fit and centred.
        let (_, dw, dh) = DEVICES[self.device];
        let (w, h, fit) = if dw > 0 { (dw, dh, (sw / dw as f32).min(sh / dh as f32).min(1.0)) } else { (sw.round() as i32, sh.round() as i32, 1.0) };
        self.fit = fit;
        let size = size(px(w as f32 * fit), px(h as f32 * fit));
        self.page_rect = Bounds { origin: point((bounds.size.width - size.width) / 2., (bounds.size.height - size.height) / 2.), size };
        let changed = with(|s| {
            let c = s.w != w || s.h != h || s.scale != scale;
            s.w = w; s.h = h; s.scale = scale;
            c
        });
        if self.tab && !self.created && w > 0 && h > 0 {
            self.created = true;
            create(&self.home);
        } else if changed {
            if let Some(h) = host() { h.notify_screen_info_changed(); h.was_resized(); }
        }
    }

    /// Where a point of the window is in the stage (for annotation marks).
    fn page_point(&self, p: Point<Pixels>) -> Point<Pixels> {
        p - self.bounds.origin
    }

    /// Where a point of the window is on the page, in the page's own px.
    fn web_point(&self, p: Point<Pixels>) -> (i32, i32) {
        let q = p - self.bounds.origin - self.page_rect.origin;
        ((f32::from(q.x) / self.fit).round() as i32, (f32::from(q.y) / self.fit).round() as i32)
    }

    fn mouse(&mut self, kind: u8, p: Point<Pixels>, button: MouseButton, cx: &mut Context<Self>) {
        let q = self.page_point(p);
        if self.annotate {
            match kind {
                1 => { self.drawing = true; self.strokes.push(vec![q]); }
                0 if self.drawing => { if let Some(s) = self.strokes.last_mut() { s.push(q); } }
                2 => self.drawing = false,
                _ => return,
            }
            cx.notify();
            return;
        }
        let Some(h) = host() else { return };
        let (x, y) = self.web_point(p);
        let ev = cef::MouseEvent { x, y, modifiers: 0 };
        let b = match button { MouseButton::Right => cef::MouseButtonType::RIGHT, MouseButton::Middle => cef::MouseButtonType::MIDDLE, _ => cef::MouseButtonType::LEFT };
        match kind {
            0 => h.send_mouse_move_event(Some(&ev), 0),
            1 => { h.set_focus(1); h.send_mouse_click_event(Some(&ev), b, 0, 1); }
            _ => h.send_mouse_click_event(Some(&ev), b, 1, 1),
        }
    }

    fn key(&mut self, ks: &Keystroke) {
        let Some(h) = host() else { return };
        let named = match ks.key.as_str() {
            "enter" => Some(13), "backspace" => Some(8), "tab" => Some(9), "escape" => Some(27),
            "left" => Some(37), "up" => Some(38), "right" => Some(39), "down" => Some(40), "delete" => Some(46),
            "home" => Some(36), "end" => Some(35), "pageup" => Some(33), "pagedown" => Some(34),
            _ => None,
        };
        let ch = ks.key_char.as_ref().and_then(|c| c.chars().next());
        let code = named.or_else(|| ch.map(|c| c.to_ascii_uppercase() as i32)).unwrap_or(0);
        let ev = |t: cef::KeyEventType, c: u16| cef::KeyEvent { type_: t, windows_key_code: code, character: c, unmodified_character: c, ..Default::default() };
        h.send_key_event(Some(&ev(cef::KeyEventType::RAWKEYDOWN, 0)));
        if let Some(c) = ch.or(if code == 13 { Some('\r') } else { None }) {
            let mut buf = [0u16; 2];
            h.send_key_event(Some(&ev(cef::KeyEventType::CHAR, c.encode_utf16(&mut buf)[0])));
        }
        h.send_key_event(Some(&ev(cef::KeyEventType::KEYUP, 0)));
    }

    fn page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let me = cx.entity();
        let scale = window.scale_factor();
        let strokes = self.strokes.clone();
        div()
            .id("page").relative().flex_1().min_h_0().bg(if self.device == 0 { gpui::white() } else { T::bg() })
            .track_focus(&self.focus)
            .when(self.annotate, |d| d.cursor_crosshair())
            .when_some(self.image.clone(), |d, img| {
                let r = self.page_rect;
                d.child(gpui::img(ImageSource::Render(img)).absolute().left(r.origin.x).top(r.origin.y).w(r.size.width).h(r.size.height))
            })
            // The annotation layer, and the page's place in the window.
            .child(canvas(
                move |bounds, _, cx| { me.update(cx, |this, _| this.layout(bounds, scale)); bounds },
                move |bounds, _, window, _| {
                    for s in &strokes {
                        if s.len() < 2 { continue; }
                        let mut pb = PathBuilder::stroke(T::ANNOTATE_STROKE);
                        pb.move_to(bounds.origin + s[0]);
                        for p in &s[1..] { pb.line_to(bounds.origin + *p); }
                        if let Ok(path) = pb.build() { window.paint_path(path, T::annotate()); }
                    }
                },
            ).absolute().inset_0().size_full())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| { window.focus(&this.focus, cx); this.mouse(1, e.position, e.button, cx) }))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, e: &MouseDownEvent, _, cx| this.mouse(1, e.position, e.button, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, _, cx| this.mouse(2, e.position, e.button, cx)))
            .on_mouse_up(MouseButton::Right, cx.listener(|this, e: &MouseUpEvent, _, cx| this.mouse(2, e.position, e.button, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| this.mouse(0, e.position, e.pressed_button.unwrap_or(MouseButton::Left), cx)))
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, window, _| {
                let Some(h) = host() else { return };
                let (x, y) = this.web_point(e.position);
                let d = e.delta.pixel_delta(window.line_height());
                h.send_mouse_wheel_event(Some(&cef::MouseEvent { x, y, modifiers: 0 }), f32::from(d.x) as i32, f32::from(d.y) as i32);
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, _| this.key(&e.keystroke)))
            .into_any_element()
    }
}

impl Render for WebPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.page(window, cx)
    }
}
