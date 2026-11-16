//! The pages: off-screen (windowless) CEF browsers, one per panel (a drawer
//! tab's page or a hidden browser), with input, device emulation, CDP, the
//! marks on a page, the console and the network log. Ported from the Slint
//! client's src/browser.rs; what it told the Slint side now goes to the
//! desktop area's hooks (`set_hooks`) and a changed flag the views read.
//!
//! Threading: CEF's message loop is pumped from GPUI's main thread (lib.rs
//! `pump`), so every CEF callback below runs there. Panel state lives in a
//! thread-local and nothing needs a lock.
//!
//! Frames: `on_paint` gives a BGRA buffer of the view (CPU, software
//! compositing), kept as it is: GPUI's images take BGRA.

use base64::Engine;
use cef::*;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap};

/// What the pages tell the desktop area (it sets them at start).
#[derive(Clone, Copy)]
pub struct Hooks {
    /// A panel's state changed (title, address, loading, marks, frame).
    pub changed: fn(usize),
    /// A page's main address changed (the history).
    pub visited: fn(usize, &str),
    /// A link that opens a window: true when the area took it (a new tab).
    pub popup: fn(usize, &str) -> bool,
    /// A main-frame load failed (its words are in `load_error`).
    pub load_failed: fn(usize),
    /// The drawer is annotating (the picker's hover).
    pub annotating: fn() -> bool,
    /// A mark was made: its note field opens (panel, mark).
    pub ask_note: fn(usize, usize),
}

impl Default for Hooks {
    fn default() -> Self {
        Hooks { changed: |_| {}, visited: |_, _| {}, popup: |_, _| false, load_failed: |_| {}, annotating: || false, ask_note: |_, _| {} }
    }
}

thread_local! { static HOOKS: RefCell<Hooks> = RefCell::new(Hooks::default()); }

/// Plug the desktop area in (once, at start).
pub fn set_hooks(h: Hooks) { HOOKS.with(|x| *x.borrow_mut() = h); }
fn hooks() -> Hooks { HOOKS.with(|x| *x.borrow()) }

/// A device the panel can emulate. Index 0 is "Responsive" (no emulation: the
/// page is the panel's own size).
pub struct Device {
    pub name: &'static str,
    pub w: i32,
    pub h: i32,
    pub dpr: f32,
    pub mobile: bool,
    pub ua: &'static str,
}

/// The drawer's devices (app/vendor/crundi-browser-drawer.js DEVICES), in its order.
pub const DEVICES: &[Device] = &[
    Device { name: "Responsive", w: 0, h: 0, dpr: 1.0, mobile: false, ua: "" },
    Device { name: "iPhone SE", w: 375, h: 667, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPhone 12 Pro", w: 390, h: 844, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPhone 15 Pro", w: 393, h: 852, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPhone 16 Pro", w: 393, h: 852, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPhone 16 Pro Max", w: 430, h: 932, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPad Mini", w: 768, h: 1024, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPad Air", w: 820, h: 1180, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "iPad Pro", w: 1024, h: 1366, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "Pixel 5", w: 393, h: 851, dpr: 2.75, mobile: true, ua: "Mozilla/5.0 (Linux; Android 11; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.91 Mobile Safari/537.36" },
    Device { name: "Pixel 7", w: 412, h: 915, dpr: 2.625, mobile: true, ua: "Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36" },
    Device { name: "Pixel 8 Pro", w: 430, h: 932, dpr: 2.625, mobile: true, ua: "Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36" },
    Device { name: "Galaxy S22", w: 360, h: 800, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (Linux; Android 13; SM-S901B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36" },
    Device { name: "Galaxy S24 Ultra", w: 412, h: 915, dpr: 2.625, mobile: true, ua: "Mozilla/5.0 (Linux; Android 14; SM-S928B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36" },
    Device { name: "Galaxy Z Fold 5", w: 344, h: 882, dpr: 2.8, mobile: true, ua: "Mozilla/5.0 (Linux; Android 13; SM-F946B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36" },
    Device { name: "Galaxy Z Flip 5", w: 264, h: 844, dpr: 2.8, mobile: true, ua: "Mozilla/5.0 (Linux; Android 13; SM-F731B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36" },
    Device { name: "Surface Pro 9", w: 1200, h: 1800, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36" },
    Device { name: "Desktop 1280", w: 1280, h: 800, dpr: 1.0, mobile: false, ua: "" },
    Device { name: "Full HD 1920", w: 1920, h: 1080, dpr: 1.0, mobile: false, ua: "" },
];

impl Device {
    /// The menu's label: "Responsive", or "iPhone SE  375x667".
    pub fn label(&self) -> String { if self.w == 0 { self.name.to_string() } else { format!("{}  {}x{}", self.name, self.w, self.h) } }
}

type CdpReply = Box<dyn FnOnce(bool, Value)>;

#[derive(Default)]
pub struct Panel {
    pub browser: Option<Browser>,
    observer: Option<Registration>,
    /// The panel's stage, in logical px (the view's size).
    pub stage_w: f32,
    pub stage_h: f32,
    pub device: usize,
    /// A size set by the server (browserResize), instead of a named device.
    pub custom: Option<(i32, i32)>,
    /// The newest frame: BGRA, its width and height in device px.
    pub frame: Option<std::rc::Rc<(Vec<u8>, u32, u32)>>,
    /// Frames so far (a view picks up a new one when this moves).
    pub frames: u64,
    pub title: String,
    pub url: String,
    /// The server's key for the browser in this panel (who drives it).
    pub key: Option<String>,
    /// The marks on the page (yours and a chat's), in page (view) CSS px so
    /// they stay on the page as the panel changes (crundi-browser-drawer.js r.ann).
    pub ann: Vec<Ann>,
    /// The mark being drawn (box, pen) and where the drag started.
    drag: Option<(usize, (f32, f32))>,
    /// The element under the pointer while picking: its box and selector.
    pub hover: Option<(f32, f32, f32, f32, String)>,
    hover_busy: bool,
    hover_next: Option<(f32, f32)>,
    /// The drawer tab this panel shows (None: a hidden browser, or a spare).
    pub tab: Option<String>,
    /// The tab's network ("client", "server", "hp:<id>").
    pub network: String,
    /// The browser was closed: the slot can take a new one.
    pub closed: bool,
    pub can_back: bool,
    pub can_fwd: bool,
    pub loading: bool,
    /// The page's console (browser_console), the last MAX_CONSOLE lines.
    pub console: std::collections::VecDeque<Value>,
    /// The page's requests (browser_network), when capture is on.
    pub net_log: std::collections::VecDeque<Value>,
    pub net_on: bool,
    /// The chat's cursor (browser_mouse), in page px, and when it moved.
    pub cursor_at: Option<(f32, f32, std::time::Instant)>,
    /// The last main-frame load failure ("Load failed (<code>): <text>").
    pub load_error: Option<String>,
    /// The chat's pointer (browser_mouse): where, and which button is held.
    pub mouse_at: Option<(f32, f32)>,
    pub mouse_down: Option<String>,
    pub touch_on: bool,
    mouse_held: Option<MouseButtonType>,
    cdp_next: i32,
    cdp_waiters: HashMap<i32, CdpReply>,
    load_waiters: Vec<Box<dyn FnOnce()>>,
    last_ua: String,
    /// The page's mouse cursor (cursor_index's kinds).
    pub cursor: i32,
}

impl Panel {
    /// The page's size (CSS px), its device scale factor, and whether it is mobile.
    pub fn view(&self) -> (i32, i32, f32, bool, &'static str) {
        if let Some((w, h)) = self.custom {
            return (w, h, 1.0, false, "");
        }
        let d = &DEVICES[self.device.min(DEVICES.len() - 1)];
        if d.w == 0 {
            ((self.stage_w.round() as i32).max(1), (self.stage_h.round() as i32).max(1), 1.0, false, "")
        } else {
            (d.w, d.h, d.dpr, d.mobile, d.ua)
        }
    }
    /// How the page sits in the stage: scale and offset (fit: contain).
    pub fn fit(&self) -> (f32, f32, f32) {
        let (vw, vh, ..) = self.view();
        let (vw, vh) = (vw as f32, vh as f32);
        if self.stage_w <= 0.0 || self.stage_h <= 0.0 { return (1.0, 0.0, 0.0); }
        let s = (self.stage_w / vw).min(self.stage_h / vh);
        (s, (self.stage_w - vw * s) / 2.0, (self.stage_h - vh * s) / 2.0)
    }
    pub fn to_view(&self, x: f32, y: f32) -> (f32, f32) {
        let (s, ox, oy) = self.fit();
        ((x - ox) / s, (y - oy) / s)
    }
    pub fn to_stage(&self, x: f32, y: f32) -> (f32, f32) {
        let (s, ox, oy) = self.fit();
        (x * s + ox, y * s + oy)
    }
    pub fn host(&self) -> Option<BrowserHost> { self.browser.as_ref().and_then(|b| b.host()) }
    /// The page's scale and offset in the stage (for what is drawn over it).
    pub fn fit_scale(&self) -> (f32, f32, f32) { self.fit() }
}

/// A mark on the page (crundi-browser-drawer.js: { id, by, kind, x, y, width, height, points, text, selector }).
#[derive(Clone, Default, Debug)]
pub struct Ann {
    pub id: String,
    /// "you" or "claude".
    pub by: String,
    /// "pin", "box", "pen", "element", "arrow", "label".
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub to: (f32, f32),
    pub points: Vec<(f32, f32)>,
    pub text: String,
    pub selector: String,
    /// What the picker read of the element ({ selector, tag, text, html, rect }).
    pub element: Value,
}

pub const MAX_CONSOLE: usize = 500;

#[derive(Default)]
pub struct State {
    pub panels: Vec<Panel>,
    /// The panels changed since the views last looked.
    pub changed: std::collections::BTreeSet<usize>,
    /// The browser-only mode's status line (CRUNDI_SERVER).
    pub status: String,
    /// The drawer's stage size last seen: a page made out of sight (a chat's
    /// tab in the background) takes it, so it is laid out at the size it
    /// will be shown at, not 1x1.
    pub stage: (f32, f32),
}

thread_local! {
    pub static STATE: RefCell<State> = RefCell::new(State::default());
}

pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// A panel's state changed: the views draw it again, and the desktop area hears.
pub fn sync(i: usize) {
    if with(|st| { if i >= st.panels.len() { return false; } st.changed.insert(i); true }) { (hooks().changed)(i); }
}

/// The panels changed since the last call (the views' pump).
/// A drawer page's size before the drawer was ever shown: the drawer's
/// default width, the window's height under the bars.
fn default_stage() -> (f32, f32) {
    use crundi_theme::T;
    (f32::from(T::DRAWER_W), f32::from(T::WINDOW_H - T::TOPBAR_H - T::TABBAR_H - T::DRAWER_BARS_H))
}

pub fn take_changed() -> Vec<usize> { with(|st| std::mem::take(&mut st.changed).into_iter().collect()) }

pub fn status(text: &str) { with(|st| st.status = text.to_string()); }

// ─── CEF handlers ───

wrap_render_handler! {
    pub struct PanelRender {
        index: usize,
    }
    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(r) = rect {
                let (w, h, ..) = with(|st| st.panels[self.index].view());
                *r = Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) };
            }
        }
        fn screen_info(&self, _browser: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> ::std::os::raw::c_int {
            match info {
                Some(s) => {
                    let (w, h, dpr, ..) = with(|st| st.panels[self.index].view());
                    s.device_scale_factor = dpr;
                    s.rect = Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) };
                    s.available_rect = s.rect.clone();
                    1
                }
                None => 0,
            }
        }
        fn on_paint(&self, _browser: Option<&mut Browser>, type_: PaintElementType, _dirty: Option<&[Rect]>, buffer: *const u8, width: ::std::os::raw::c_int, height: ::std::os::raw::c_int) {
            if type_ != PaintElementType::VIEW || buffer.is_null() || width <= 0 || height <= 0 { return; }
            let (w, h) = (width as u32, height as u32);
            let src = unsafe { std::slice::from_raw_parts(buffer, (w * h * 4) as usize) };
            let frame = std::rc::Rc::new((src.to_vec(), w, h));
            with(|st| { let p = &mut st.panels[self.index]; p.frame = Some(frame); p.frames += 1; });
            sync(self.index);
        }
    }
}

wrap_life_span_handler! {
    pub struct PanelLife {
        index: usize,
    }
    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(b) = browser else { return };
            let b = b.clone();
            let mut obs = PanelDevTools::new(self.index);
            let reg = b.host().and_then(|h| h.add_dev_tools_message_observer(Some(&mut obs)));
            with(|st| { let p = &mut st.panels[self.index]; p.browser = Some(b); p.observer = reg; });
        }
        // Links that open a window load in the same panel (no popup windows in OSR).
        fn on_before_popup(&self, browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _popup_id: ::std::os::raw::c_int, target_url: Option<&CefString>, _target_frame_name: Option<&CefString>, _target_disposition: WindowOpenDisposition, _user_gesture: ::std::os::raw::c_int, _popup_features: Option<&PopupFeatures>, _window_info: Option<&mut WindowInfo>, _client: Option<&mut Option<Client>>, _settings: Option<&mut BrowserSettings>, _extra_info: Option<&mut Option<DictionaryValue>>, _no_javascript_access: Option<&mut ::std::os::raw::c_int>) -> ::std::os::raw::c_int {
            if let (Some(b), Some(u)) = (browser, target_url) {
                let url = u.to_string();
                let i = self.index;
                if !(hooks().popup)(i, &url) {
                    if let Some(f) = b.main_frame() { f.load_url(Some(u)); }
                }
            }
            1
        }
    }
}

wrap_load_handler! {
    pub struct PanelLoad {
        index: usize,
    }
    impl LoadHandler {
        fn on_load_error(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, error_code: Errorcode, error_text: Option<&CefString>, _failed_url: Option<&CefString>) {
            if frame.map(|f| f.is_main() == 0).unwrap_or(false) { return; }
            let code = *error_code.as_ref() as i32;
            if code == -3 { return; } // ERR_ABORTED: a navigation replaced it
            let text = error_text.map(|t| t.to_string()).unwrap_or_default();
            with(|st| st.panels[self.index].load_error = Some(format!("Load failed ({code}): {text}")));
            (hooks().load_failed)(self.index);
        }
        fn on_loading_state_change(&self, _browser: Option<&mut Browser>, is_loading: ::std::os::raw::c_int, back: ::std::os::raw::c_int, fwd: ::std::os::raw::c_int) {
            with(|st| { let p = &mut st.panels[self.index]; p.loading = is_loading != 0; p.can_back = back != 0; p.can_fwd = fwd != 0; });
            sync(self.index);
            if is_loading != 0 { with(|st| st.panels[self.index].load_error = None); }
            if is_loading == 0 {
                let waiters = with(|st| std::mem::take(&mut st.panels[self.index].load_waiters));
                for w in waiters { w(); }
            }
        }
    }
}

#[cfg(target_os = "macos")]
type PageCursor = *mut u8;
#[cfg(not(target_os = "macos"))]
type PageCursor = CursorHandle;


/// CEF's cursor kinds, folded into the ones a view shows.
fn cursor_index(t: CursorType) -> i32 {
    match t {
        CursorType::HAND => 1,
        CursorType::IBEAM | CursorType::VERTICALTEXT => 2,
        CursorType::CROSS | CursorType::CELL => 3,
        CursorType::MOVE => 4,
        CursorType::WAIT | CursorType::PROGRESS => 5,
        CursorType::NOTALLOWED | CursorType::NODROP => 6,
        CursorType::EASTRESIZE | CursorType::WESTRESIZE | CursorType::EASTWESTRESIZE | CursorType::COLUMNRESIZE => 7,
        CursorType::NORTHRESIZE | CursorType::SOUTHRESIZE | CursorType::NORTHSOUTHRESIZE | CursorType::ROWRESIZE => 8,
        CursorType::NORTHEASTRESIZE | CursorType::SOUTHWESTRESIZE | CursorType::NORTHEASTSOUTHWESTRESIZE => 9,
        CursorType::NORTHWESTRESIZE | CursorType::SOUTHEASTRESIZE | CursorType::NORTHWESTSOUTHEASTRESIZE => 10,
        CursorType::GRAB => 11,
        CursorType::GRABBING => 12,
        CursorType::HELP => 13,
        CursorType::NONE => 14,
        _ => 0,
    }
}

wrap_display_handler! {
    pub struct PanelDisplay {
        index: usize,
    }
    impl DisplayHandler {
        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let t = title.map(|t| t.to_string()).unwrap_or_default();
            with(|st| st.panels[self.index].title = t);
            sync(self.index);
        }
        fn on_cursor_change(&self, _browser: Option<&mut Browser>, _cursor: PageCursor, type_: CursorType, _custom_cursor_info: Option<&CursorInfo>) -> ::std::os::raw::c_int {
            let c = cursor_index(type_);
            let changed = with(|st| { let p = &mut st.panels[self.index]; let ch = p.cursor != c; p.cursor = c; ch });
            if changed { sync(self.index); }
            1
        }
        fn on_console_message(&self, _browser: Option<&mut Browser>, level: LogSeverity, message: Option<&CefString>, source: Option<&CefString>, line: ::std::os::raw::c_int) -> ::std::os::raw::c_int {
            let lv = match level { LogSeverity::ERROR | LogSeverity::FATAL => "error", LogSeverity::WARNING => "warning", LogSeverity::VERBOSE => "debug", _ => "log" };
            let m = json!({ "type": lv, "text": message.map(|m| m.to_string()).unwrap_or_default(), "url": source.map(|m| m.to_string()).unwrap_or_default(), "line": line, "time": now_ms() });
            with(|st| { let c = &mut st.panels[self.index].console; c.push_back(m); while c.len() > MAX_CONSOLE { c.pop_front(); } });
            0
        }
        fn on_address_change(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if frame.map(|f| f.is_main() == 0).unwrap_or(false) { return; }
            let u = url.map(|u| u.to_string()).unwrap_or_default();
            let base = |x: &str| x.split('#').next().unwrap_or("").to_string();
            with(|st| { let p = &mut st.panels[self.index]; if base(&p.url) != base(&u) { p.ann.clear(); p.hover = None; } p.url = u.clone(); });
            sync(self.index);
            (hooks().visited)(self.index, &u);
        }
    }
}

wrap_dev_tools_message_observer! {
    pub struct PanelDevTools {
        index: usize,
    }
    impl DevToolsMessageObserver {
        fn on_dev_tools_event(&self, _browser: Option<&mut Browser>, method: Option<&CefString>, params: Option<&[u8]>) {
            let m = method.map(|m| m.to_string()).unwrap_or_default();
            if !m.starts_with("Network.") { return; }
            let v: Value = params.and_then(|r| serde_json::from_slice(r).ok()).unwrap_or(Value::Null);
            net_event(self.index, &m, &v);
        }
        fn on_dev_tools_method_result(&self, _browser: Option<&mut Browser>, message_id: ::std::os::raw::c_int, success: ::std::os::raw::c_int, result: Option<&[u8]>) {
            let v: Value = result.and_then(|r| serde_json::from_slice(r).ok()).unwrap_or(Value::Null);
            let w = with(|st| st.panels[self.index].cdp_waiters.remove(&message_id));
            if let Some(w) = w { w(success != 0, v); }
        }
    }
}

// No page dialogs (a chat drives these pages): alert dismissed, confirm
// yes, prompt nothing, leaving a page always allowed (main.js's
// window.alert/confirm/prompt overrides and will-prevent-unload).
wrap_jsdialog_handler! {
    pub struct PanelDialogs {}
    impl JsdialogHandler {
        fn on_jsdialog(&self, _browser: Option<&mut Browser>, _origin_url: Option<&CefString>, dialog_type: JsdialogType, _message_text: Option<&CefString>, _default_prompt_text: Option<&CefString>, callback: Option<&mut JsdialogCallback>, suppress_message: Option<&mut ::std::os::raw::c_int>) -> ::std::os::raw::c_int {
            if dialog_type == JsdialogType::ALERT { if let Some(s) = suppress_message { *s = 1; } return 0; }
            if let Some(cb) = callback { cb.cont(if dialog_type == JsdialogType::CONFIRM { 1 } else { 0 }, None); }
            1
        }
        fn on_before_unload_dialog(&self, _browser: Option<&mut Browser>, _message_text: Option<&CefString>, _is_reload: ::std::os::raw::c_int, callback: Option<&mut JsdialogCallback>) -> ::std::os::raw::c_int {
            if let Some(cb) = callback { cb.cont(1, None); }
            1
        }
    }
}

// The drawer's network proxies ask for their sign-in (src/desktop/net.rs).
wrap_request_handler! {
    pub struct PanelRequests {}
    impl RequestHandler {
        fn auth_credentials(&self, _browser: Option<&mut Browser>, _origin_url: Option<&CefString>, is_proxy: ::std::os::raw::c_int, host: Option<&CefString>, port: ::std::os::raw::c_int, _realm: Option<&CefString>, _scheme: Option<&CefString>, callback: Option<&mut AuthCallback>) -> ::std::os::raw::c_int {
            if is_proxy == 0 { return 0; }
            let h = host.map(|h| h.to_string()).unwrap_or_default();
            match (crate::net::credentials(&h, port as u16), callback) {
                (Some((u, p)), Some(cb)) => { cb.cont(Some(&CefString::from(u.as_str())), Some(&CefString::from(p.as_str()))); 1 }
                _ => 0,
            }
        }
    }
}

wrap_client! {
    pub struct PanelClient {
        render: RenderHandler,
        life: LifeSpanHandler,
        load: LoadHandler,
        display: DisplayHandler,
        dialogs: JsdialogHandler,
        requests: RequestHandler,
    }
    impl Client {
        fn request_handler(&self) -> Option<RequestHandler> { Some(self.requests.clone()) }
        fn jsdialog_handler(&self) -> Option<JsdialogHandler> { Some(self.dialogs.clone()) }
        fn render_handler(&self) -> Option<RenderHandler> { Some(self.render.clone()) }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(self.life.clone()) }
        fn load_handler(&self) -> Option<LoadHandler> { Some(self.load.clone()) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(self.display.clone()) }
    }
}

/// Make panel [index]'s browser, off-screen, at [url] (this PC's network).
pub fn create(index: usize, url: &str) {
    create_on(index, url, "client");
}

thread_local! {
    /// One profile per network, as the drawer's partitions (BD PARTITIONS):
    /// "client" is CEF's own; "server" and each homepool network their own.
    static CONTEXTS: RefCell<HashMap<String, RequestContext>> = RefCell::new(HashMap::new());
}

/// The CEF profile folder of a network ("" for this PC's: CEF's global one).
pub fn profile_dir(network: &str) -> String {
    match network {
        "client" | "" => String::new(),
        "server" => "drawer-srv".into(),
        n if n.starts_with("mem:") => String::new(),
        n => format!("drawer-hp-{}", n.trim_start_matches("hp:")),
    }
}

fn context_for(network: &str) -> Option<RequestContext> {
    // A hidden browser: a profile of its own, in memory (main.js "mcp-browser-<key>").
    if network.starts_with("mem:") {
        let settings = RequestContextSettings::default();
        let ctx = request_context_create_context(Some(&settings), None)?;
        CONTEXTS.with(|c| c.borrow_mut().insert(network.to_string(), ctx.clone()));
        return Some(ctx);
    }
    let dir = profile_dir(network);
    if dir.is_empty() { return None; }
    if let Some(c) = CONTEXTS.with(|c| c.borrow().get(network).cloned()) { return Some(c); }
    let path = cef_root().join(&dir);
    let settings = RequestContextSettings { cache_path: CefString::from(path.to_string_lossy().as_ref()), persist_session_cookies: 1, ..Default::default() };
    let ctx = request_context_create_context(Some(&settings), None)?;
    CONTEXTS.with(|c| c.borrow_mut().insert(network.to_string(), ctx.clone()));
    crate::net::context_made(network, &ctx);
    Some(ctx)
}

/// CEF's root cache folder, as main.rs sets it (a network's profile must be inside it).
pub fn cef_root() -> std::path::PathBuf {
    if let Some(c) = std::env::var_os("CRUNDI_CEF_CACHE") { return c.into(); }
    let cache: std::path::PathBuf = std::env::var_os("XDG_CACHE_HOME").map(Into::into)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(Into::into))
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    cache.join("crundi-gpui")
}

/// Every profile made so far, with its network (clearing reaches them all).
pub fn contexts() -> Vec<(String, RequestContext)> {
    let mut v: Vec<(String, RequestContext)> = CONTEXTS.with(|c| c.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    if let Some(g) = request_context_get_global_context() { v.push(("client".into(), g)); }
    v
}

/// Make panel [index]'s browser on [network]'s profile.
pub fn create_on(index: usize, url: &str, network: &str) {
    let mut client = PanelClient::new(PanelRender::new(index), PanelLife::new(index), PanelLoad::new(index), PanelDisplay::new(index), PanelDialogs::new(), PanelRequests::new());
    let window_info = WindowInfo::default().set_as_windowless(Default::default());
    let settings = BrowserSettings { windowless_frame_rate: 30, background_color: 0xffff_ffff, ..Default::default() };
    let mut ctx = context_for(network);
    let b = browser_host_create_browser_sync(Some(&window_info), Some(&mut client), Some(&CefString::from(url)), Some(&settings), None, ctx.as_mut());
    if b.is_none() { eprintln!("[native] panel {index}: CEF did not create the browser"); }
}

/// A new panel (a drawer tab's page or a hidden browser) at [url] on
/// [network]: a closed slot again, a spare (made at start, never used), or
/// a new one. Returns its index.
pub fn open_panel(url: &str, network: &str) -> usize {
    let spare = with(|st| st.panels.iter().position(|p| !p.closed && p.tab.is_none() && p.key.is_none() && p.browser.is_some() && p.network.is_empty() && (network == "client")));
    if let Some(i) = spare {
        with(|st| { let p = &mut st.panels[i]; p.network = network.to_string(); p.ann.clear(); p.console.clear(); p.title.clear(); });
        navigate(i, url, Box::new(|| {}));
        return i;
    }
    let reuse = with(|st| st.panels.iter().position(|p| p.closed));
    let i = match reuse {
        Some(i) => { with(|st| { let size = (st.panels[i].stage_w, st.panels[i].stage_h); st.panels[i] = Panel::default(); st.panels[i].stage_w = size.0; st.panels[i].stage_h = size.1; }); i }
        None => with(|st| { st.panels.push(Panel::default()); st.panels.len() - 1 }),
    };
    with(|st| { let s = if st.stage.0 > 0.0 { st.stage } else { default_stage() }; let p = &mut st.panels[i]; p.network = network.to_string(); p.url = url.to_string(); if p.stage_w <= 0.0 { p.stage_w = s.0; p.stage_h = s.1; } });
    sync(i);
    create_on(i, url, network);
    i
}

/// Close panel [i]'s browser; its slot is free for the next one.
pub fn close_panel(i: usize) {
    let b = with(|st| {
        let p = st.panels.get_mut(i)?;
        if p.closed { return None; }
        p.closed = true; p.tab = None; p.key = None; p.ann.clear(); p.frame = None; p.title.clear(); p.url.clear();
        let w: Vec<_> = std::mem::take(&mut p.load_waiters);
        let b = p.browser.take();
        p.observer = None;
        Some((b, w))
    });
    let Some((b, waiters)) = b else { return };
    for w in waiters { w(); }
    if let Some(h) = b.and_then(|b| b.host()) { h.close_browser(1); }
    let net = with(|st| std::mem::take(&mut st.panels[i].network));
    if net.starts_with("mem:") { CONTEXTS.with(|c| c.borrow_mut().remove(&net)); }
    sync(i);
}

thread_local! {
    static DEFERRED: RefCell<Vec<Box<dyn FnOnce()>>> = RefCell::new(Vec::new());
}

/// Run a call into CEF that comes from a GPUI handler. On macOS and Windows
/// it waits for the next pump, outside the window's event handling, which
/// CEF must not re-enter; on Linux it runs now.
pub fn defer(f: impl FnOnce() + 'static) {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        DEFERRED.with(|d| d.borrow_mut().push(Box::new(f)));
    } else {
        f();
    }
}

/// The deferred calls, in order (the pump, before CEF's own work).
pub fn run_deferred() {
    loop {
        let batch = DEFERRED.with(|d| std::mem::take(&mut *d.borrow_mut()));
        if batch.is_empty() { break; }
        for f in batch { f(); }
    }
}

// ─── CDP, in-process (no debugging port) ───

/// Send a CDP command to panel [i]'s browser; [reply] gets (ok, result).
pub fn cdp(i: usize, method: &str, params: Value, reply: CdpReply) {
    let sent = with(|st| {
        let p = &mut st.panels[i];
        let Some(h) = p.host() else { return Err(reply) };
        p.cdp_next += 1;
        let id = p.cdp_next;
        let msg = json!({ "id": id, "method": method, "params": params }).to_string();
        p.cdp_waiters.insert(id, reply);
        h.send_dev_tools_message(Some(msg.as_bytes()));
        Ok(())
    });
    if let Err(reply) = sent { reply(false, json!({ "message": "no browser in this panel yet" })); }
}

pub fn eval(i: usize, code: &str, reply: Box<dyn FnOnce(Result<Value, String>)>) {
    cdp(i, "Runtime.evaluate", json!({ "expression": code, "returnByValue": true, "awaitPromise": true }), Box::new(move |ok, v| {
        if !ok { return reply(Err(v.get("message").and_then(|m| m.as_str()).unwrap_or("CDP failed").to_string())); }
        if let Some(e) = v.get("exceptionDetails") {
            let m = e.pointer("/exception/description").or_else(|| e.get("text")).and_then(|m| m.as_str()).unwrap_or("exception").to_string();
            return reply(Err(m));
        }
        reply(Ok(v.pointer("/result/value").cloned().unwrap_or(Value::Null)));
    }));
}

// ─── Device emulation ───

/// Apply panel [i]'s device: the view size and scale factor come from the
/// render handler (view_rect, screen_info); CDP adds mobile, touch and the
/// user agent. A changed user agent reloads the page, as a phone would get it.
pub fn apply_device(i: usize) {
    let (w, h, dpr, mobile, ua, host, responsive) = with(|st| {
        let p = &st.panels[i];
        let (w, h, dpr, mobile, ua) = p.view();
        (w, h, dpr, mobile, ua, p.host(), p.device == 0 && p.custom.is_none())
    });
    let Some(host) = host else { return };
    host.notify_screen_info_changed();
    host.was_resized();
    if responsive {
        cdp(i, "Emulation.clearDeviceMetricsOverride", json!({}), Box::new(|_, _| {}));
    } else {
        cdp(i, "Emulation.setDeviceMetricsOverride", json!({ "width": w, "height": h, "deviceScaleFactor": dpr, "mobile": mobile }), Box::new(|_, _| {}));
    }
    cdp(i, "Emulation.setTouchEmulationEnabled", json!({ "enabled": mobile, "maxTouchPoints": if mobile { 5 } else { 1 } }), Box::new(|_, _| {}));
    let changed = with(|st| { let p = &mut st.panels[i]; let c = p.last_ua != ua; p.last_ua = ua.to_string(); c });
    // Reload once the overrides are in (CDP answers in order): a new UA and
    // touch support only show in a document created after them.
    cdp(i, "Emulation.setUserAgentOverride", json!({ "userAgent": ua }), Box::new(move |_, _| {
        if changed {
            if let Some(b) = with(|st| st.panels[i].browser.clone()) { b.reload(); }
        }
    }));
    sync(i);
}

pub fn set_device(i: usize, d: usize) {
    // The last entry may be "Custom WxH" (the server's size): choosing it keeps that size.
    if d >= DEVICES.len() { return; }
    with(|st| { let p = &mut st.panels[i]; p.device = d; p.custom = None; });
    apply_device(i);
}

pub fn resized(i: usize, w: f32, h: f32) {
    let follow = with(|st| {
        st.stage = (w, h);
        let p = &mut st.panels[i];
        let changed = (p.stage_w - w).abs() > 0.5 || (p.stage_h - h).abs() > 0.5;
        p.stage_w = w; p.stage_h = h;
        changed && p.device == 0 && p.custom.is_none()
    });
    if follow { if let Some(h) = with(|st| st.panels[i].host()) { h.was_resized(); } }
    sync(i);
}

// ─── Input from the views ───

const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;

fn button(b: i32) -> MouseButtonType {
    match b { 1 => MouseButtonType::MIDDLE, 2 => MouseButtonType::RIGHT, _ => MouseButtonType::LEFT }
}

thread_local! {
    /// The annotation tool: "pick", "pin", "box", "pen" (the drawer's bar).
    pub static TOOL: RefCell<String> = RefCell::new("pick".into());
}

fn uid() -> String {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}{:04x}", n / 1_000_000, (n % 65536) as u32)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The elements at page points: { selector, tag, text, html, rect } or null
/// each (app/browser-drawer.js describe).
pub const DESCRIBE_JS: &str = r#"(function(pts){
  function sel(e){ var parts=[]; while(e&&e.nodeType===1&&parts.length<5){ if(e.id){parts.unshift('#'+CSS.escape(e.id));break;}
    var s=e.tagName.toLowerCase(); var c=(typeof e.className==='string'&&e.className.trim())?e.className.trim().split(/\s+/).slice(0,2).map(function(x){return '.'+CSS.escape(x);}).join(''):'';
    var par=e.parentElement; if(par){var same=[].filter.call(par.children,function(k){return k.tagName===e.tagName;}); if(same.length>1) s+=':nth-of-type('+(same.indexOf(e)+1)+')';}
    parts.unshift(s+c); e=par; } return parts.join(' > '); }
  return JSON.stringify(pts.map(function(p){ var el=document.elementFromPoint(p[0],p[1]); if(!el) return null; var r=el.getBoundingClientRect();
    var open=el.outerHTML; var gt=open.indexOf('>'); open=(gt>0?open.slice(0,gt+1):open).slice(0,300);
    return { selector: sel(el), tag: el.tagName.toLowerCase(), text: String(el.innerText||el.value||el.alt||el.getAttribute('aria-label')||'').trim().replace(/\s+/g,' ').slice(0,160),
      html: open, rect: { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) } }; }));
})"#;

/// What is at page points [pts] in panel [i] (each an element or null).
pub fn describe(i: usize, pts: &[(f32, f32)], done: Box<dyn FnOnce(Vec<Value>)>) {
    let arr: Vec<Value> = pts.iter().take(60).map(|&(x, y)| json!([x.round(), y.round()])).collect();
    eval(i, &format!("{DESCRIBE_JS}({})", Value::Array(arr)), Box::new(move |r| {
        let v = match r { Ok(Value::String(s)) => serde_json::from_str::<Vec<Value>>(&s).unwrap_or_default(), _ => vec![] };
        done(v)
    }));
}

fn element_ann(e: &Value) -> Option<Ann> {
    let r = e.get("rect")?;
    let f = |k: &str| r.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    Some(Ann { id: uid(), by: "you".into(), kind: "element".into(), x: f("x"), y: f("y"), w: f("width"), h: f("height"),
        selector: e.get("selector").and_then(|s| s.as_str()).unwrap_or("").to_string(), element: e.clone(), ..Default::default() })
}

/// The picker's hover: one describe at a time, the latest point next.
fn hover_loop(i: usize) {
    let next = with(|st| { let p = &mut st.panels[i]; if p.hover_busy { return None; } let n = p.hover_next.take(); if n.is_some() { p.hover_busy = true; } n });
    let Some(pt) = next else { return };
    describe(i, &[pt], Box::new(move |els| {
        let picking = TOOL.with(|t| t.borrow().as_str() == "pick") && (hooks().annotating)();
        with(|st| {
            let p = &mut st.panels[i];
            p.hover_busy = false;
            p.hover = if picking { els.first().and_then(|e| { let a = element_ann(e)?; Some((a.x, a.y, a.w, a.h, a.selector)) }) } else { None };
        });
        sync(i);
        hover_loop(i);
    }));
}

/// A pointer event on panel [i]'s stage (kind 0 move, 1 down, 2 up).
pub fn pointer(i: usize, kind: i32, x: f32, y: f32, b: i32, annotating: bool) {
    if annotating {
        let tool = TOOL.with(|t| t.borrow().clone());
        let (vx, vy) = with(|st| { let (vx, vy) = st.panels[i].to_view(x, y); (vx.round(), vy.round()) });
        match (kind, tool.as_str()) {
            (1, _) if b != 0 => {}
            (1, "pin") => {
                let n = with(|st| { let p = &mut st.panels[i]; p.ann.push(Ann { id: uid(), by: "you".into(), kind: "pin".into(), x: vx, y: vy, ..Default::default() }); p.ann.len() - 1 });
                sync(i);
                (hooks().ask_note)(i, n);
            }
            (1, "pick") => describe(i, &[(vx, vy)], Box::new(move |els| {
                let Some(a) = els.first().and_then(element_ann) else { return };
                let n = with(|st| { let p = &mut st.panels[i]; p.hover = None; p.ann.push(a); p.ann.len() - 1 });
                sync(i);
                (hooks().ask_note)(i, n);
            })),
            (1, t) => {
                let a = if t == "pen" { Ann { id: uid(), by: "you".into(), kind: "pen".into(), points: vec![(vx, vy)], x: vx, y: vy, ..Default::default() } }
                    else { Ann { id: uid(), by: "you".into(), kind: "box".into(), x: vx, y: vy, w: 1.0, h: 1.0, ..Default::default() } };
                with(|st| { let p = &mut st.panels[i]; p.ann.push(a); p.drag = Some((p.ann.len() - 1, (vx, vy))); });
                sync(i);
            }
            (0, t) => {
                let dragging = with(|st| {
                    let p = &mut st.panels[i];
                    let Some((n, (sx, sy))) = p.drag else { return false };
                    let a = &mut p.ann[n];
                    if a.kind == "pen" { a.points.push((vx, vy)); } else { a.x = sx.min(vx); a.y = sy.min(vy); a.w = (vx - sx).abs(); a.h = (vy - sy).abs(); }
                    true
                });
                if dragging { sync(i); }
                else if t == "pick" { with(|st| st.panels[i].hover_next = Some((vx, vy))); hover_loop(i); }
            }
            (2, _) => {
                let done = with(|st| {
                    let p = &mut st.panels[i];
                    let (n, _) = p.drag.take()?;
                    let a = &mut p.ann[n];
                    // Too small to mean anything: dropped (VD pointerup).
                    if (a.kind == "box" && (a.w < 4.0 || a.h < 4.0)) || (a.kind == "pen" && a.points.len() < 3) { p.ann.remove(n); return Some(None); }
                    if a.kind == "pen" {
                        // Simplified (every second point, at most 400), anchored at its middle.
                        let last = a.points.len() - 1;
                        let pts: Vec<_> = a.points.iter().enumerate().filter(|(j, _)| j % 2 == 0 || *j == last).map(|(_, p)| *p).collect();
                        let pts = pts[pts.len().saturating_sub(400)..].to_vec();
                        let mid = pts[pts.len() / 2];
                        a.points = pts; a.x = mid.0; a.y = mid.1;
                    }
                    Some(Some(n))
                });
                sync(i);
                if let Some(Some(n)) = done { (hooks().ask_note)(i, n); }
            }
            _ => {}
        }
        return;
    }
    let (host, vx, vy, held) = with(|st| { let p = &st.panels[i]; let (vx, vy) = p.to_view(x, y); (p.host(), vx, vy, p.mouse_held) });
    let Some(host) = host else { return };
    let ev = MouseEvent { x: vx.round() as i32, y: vy.round() as i32, modifiers: if held.is_some() { EVENTFLAG_LEFT_MOUSE_BUTTON } else { 0 } };
    match kind {
        0 => host.send_mouse_move_event(Some(&ev), 0),
        1 => { host.set_focus(1); host.send_mouse_click_event(Some(&ev), button(b), 0, 1); with(|st| st.panels[i].mouse_held = Some(button(b))); }
        2 => { host.send_mouse_click_event(Some(&ev), held.unwrap_or(button(b)), 1, 1); with(|st| st.panels[i].mouse_held = None); }
        _ => host.send_mouse_move_event(Some(&ev), 1),
    }
}

pub fn wheel(i: usize, x: f32, y: f32, dx: f32, dy: f32) {
    let (host, vx, vy) = with(|st| { let p = &st.panels[i]; let (vx, vy) = p.to_view(x, y); (p.host(), vx, vy) });
    if let Some(h) = host {
        h.send_mouse_wheel_event(Some(&MouseEvent { x: vx as i32, y: vy as i32, modifiers: 0 }), dx as i32, dy as i32);
    }
}

/// Named keys as text (private-use characters, as the Slint client's) to Windows key codes.
fn named_key(c: char) -> Option<i32> {
    Some(match c {
        '\u{8}' => 8, '\t' => 9, '\n' | '\r' => 13, '\u{1b}' => 27, '\u{7f}' => 46,
        '\u{F700}' => 38, '\u{F701}' => 40, '\u{F702}' => 37, '\u{F703}' => 39,
        '\u{F729}' => 36, '\u{F72B}' => 35, '\u{F72C}' => 33, '\u{F72D}' => 34,
        _ => return None,
    })
}

pub fn key(i: usize, text: &str, down: bool, ctrl: bool, shift: bool) {
    if std::env::var_os("CRUNDI_DEBUG_KEYS").is_some() { eprintln!("[native] key panel {i} {:?} down={down}", text); }
    let Some(host) = with(|st| st.panels[i].host()) else { return };
    let Some(c) = text.chars().next() else { return };
    let modifiers = (if shift { 1 << 1 } else { 0 }) | (if ctrl { 1 << 2 } else { 0 });
    let code = named_key(c).unwrap_or_else(|| c.to_ascii_uppercase() as i32);
    let ev = |t: KeyEventType, ch: u16| KeyEvent { type_: t, modifiers, windows_key_code: code, character: ch, unmodified_character: ch, ..Default::default() };
    if down {
        host.send_key_event(Some(&ev(KeyEventType::RAWKEYDOWN, 0)));
        // Printable (and Enter): the character itself.
        if named_key(c).is_none() || c == '\n' || c == '\r' {
            let ch = if c == '\n' { '\r' } else { c };
            let mut buf = [0u16; 2];
            host.send_key_event(Some(&ev(KeyEventType::CHAR, ch.encode_utf16(&mut buf)[0])));
        }
    } else {
        host.send_key_event(Some(&ev(KeyEventType::KEYUP, 0)));
    }
}

/// Clear: the person's marks only (VD "Remove your marks").
pub fn clear_marks(i: usize) {
    with(|st| { let p = &mut st.panels[i]; p.ann.retain(|a| a.by == "claude"); p.drag = None; });
    sync(i);
}

/// Every mark, a chat's too (browserDrawerAnnotate clear).
pub fn clear_all_marks(i: usize) {
    with(|st| { let p = &mut st.panels[i]; p.ann.clear(); p.drag = None; p.hover = None; });
    sync(i);
}

/// A mark's note (the note field: Enter keeps, at most 500 characters).
pub fn set_note(i: usize, n: usize, text: &str) {
    with(|st| if let Some(a) = st.panels.get_mut(i).and_then(|p| p.ann.get_mut(n)) { a.text = text.trim().chars().take(500).collect(); });
    sync(i);
}

// ─── What the server asks (server.rs) ───

pub fn navigate(i: usize, url: &str, done: Box<dyn FnOnce()>) {
    let b = with(|st| { let p = &mut st.panels[i]; p.load_waiters.push(done); p.browser.clone() });
    match b.and_then(|b| b.main_frame()) {
        Some(f) => f.load_url(Some(&CefString::from(url))),
        None => { let w = with(|st| std::mem::take(&mut st.panels[i].load_waiters)); for w in w { w(); } }
    }
}

/// The panel's current frame as a PNG (what the person sees, at device pixels), base64.
pub fn screenshot(i: usize) -> Option<String> {
    let f = with(|st| st.panels.get(i).and_then(|p| p.frame.clone()))?;
    png_base64(&f.0, f.1, f.2)
}

/// The whole screen the window is on, as a base64 PNG (X11's xwd: Xvfb in
/// the tests). None elsewhere.
pub fn window_snapshot() -> Option<String> {
    let out = std::process::Command::new("sh").arg("-c").arg("xwd -silent -root | convert xwd:- png:-").output().ok()?;
    if !out.status.success() || out.stdout.is_empty() { return None; }
    Some(base64::engine::general_purpose::STANDARD.encode(out.stdout))
}

/// A BGRA frame as a base64 PNG.
pub fn png_base64(bgra: &[u8], w: u32, h: u32) -> Option<String> {
    let mut rgba = bgra.to_vec();
    for px in rgba.chunks_exact_mut(4) { px.swap(0, 2); }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().ok()?;
        wr.write_image_data(&rgba).ok()?;
    }
    Some(base64::engine::general_purpose::STANDARD.encode(out))
}

/// The address bar's back (-1), forward (1) and reload (0).
pub fn history(i: usize, h: i32) {
    let Some(b) = with(|st| st.panels[i].browser.clone()) else { return };
    match h {
        -1 => b.go_back(),
        1 => b.go_forward(),
        _ => b.reload(),
    }
}

/// Real mouse input at page (CSS px) coordinates, as browser_mouse does.
pub fn mouse(i: usize, x: f32, y: f32, action: &str) -> Result<(), String> {
    let host = with(|st| st.panels[i].host()).ok_or("no browser in this panel yet")?;
    let ev = MouseEvent { x: x.round() as i32, y: y.round() as i32, modifiers: 0 };
    match action {
        "move" => host.send_mouse_move_event(Some(&ev), 0),
        "down" => host.send_mouse_click_event(Some(&ev), MouseButtonType::LEFT, 0, 1),
        "up" => host.send_mouse_click_event(Some(&ev), MouseButtonType::LEFT, 1, 1),
        "click" | "" => {
            host.send_mouse_move_event(Some(&ev), 0);
            host.send_mouse_click_event(Some(&ev), MouseButtonType::LEFT, 0, 1);
            host.send_mouse_click_event(Some(&ev), MouseButtonType::LEFT, 1, 1);
        }
        a => return Err(format!("mouse action {a} is not in this prototype")),
    }
    Ok(())
}

pub fn resize(i: usize, w: i32, h: i32) {
    with(|st| { let p = &mut st.panels[i]; p.custom = Some((w.clamp(200, 3840), h.clamp(200, 3840))); });
    apply_device(i);
}

pub fn point(i: usize, x: f32, y: f32, label: &str) {
    with(|st| st.panels[i].ann.push(Ann { id: uid(), by: "claude".into(), kind: "pin".into(), x, y, text: label.to_string(), ..Default::default() }));
    sync(i);
}

/// A mark's anchor (VD anchorOf).
pub fn anchor_of(a: &Ann) -> (f32, f32) {
    match a.kind.as_str() { "box" | "element" => (a.x + a.w / 2.0, a.y + a.h / 2.0), "arrow" => a.to, _ => (a.x, a.y) }
}

/// The marks as a chat reads them (VD annList), numbered from 1.
pub fn ann_list(i: usize) -> Vec<Value> {
    with(|st| st.panels[i].ann.iter().enumerate().map(|(n, a)| ann_json(n + 1, a)).collect())
}

pub fn ann_json(n: usize, a: &Ann) -> Value {
    let (ax, ay) = anchor_of(a);
    let mut o = json!({ "n": n, "by": if a.by == "claude" { "claude" } else { "person" }, "kind": a.kind, "text": a.text, "x": a.x, "y": a.y, "anchor": [ax, ay] });
    if a.kind == "box" || a.kind == "element" { o["width"] = json!(a.w); o["height"] = json!(a.h); }
    if a.kind == "element" && !a.element.is_null() { o["picked"] = a.element.clone(); } else if !a.selector.is_empty() { o["picked"] = json!({ "selector": a.selector }); }
    if a.kind == "arrow" { o["toX"] = json!(a.to.0); o["toY"] = json!(a.to.1); }
    if a.kind == "pen" { o["points"] = json!(a.points.len()); }
    o
}

/// The marks on panel [i], in page coordinates (what a chat reads back).
pub fn marks(i: usize) -> Value {
    let (w, h) = with(|st| { let (w, h, ..) = st.panels[i].view(); (w, h) });
    let anns = ann_list(i);
    let pin = with(|st| st.panels[i].ann.iter().rev().find(|a| a.kind == "pin" && a.by == "claude").map(|a| json!({ "x": a.x, "y": a.y, "label": a.text })));
    json!({ "viewport": { "width": w, "height": h }, "annotations": anns, "pin": pin })
}

/// DevTools for panel [i], in a window of its own (BD devtools: detached),
/// or closed when open.
pub fn devtools(i: usize) {
    let Some(h) = with(|st| st.panels.get(i).and_then(|p| p.host())) else { return };
    if h.has_dev_tools() != 0 { h.close_dev_tools(); return; }
    let wi = WindowInfo::default();
    let settings = BrowserSettings::default();
    h.show_dev_tools(Some(&wi), None, Some(&settings), None);
}

pub const MAX_NET: usize = 1000;

/// A CDP Network event, kept while capture is on (main.js webRequest hooks:
/// the same field names).
fn net_event(i: usize, method: &str, v: &Value) {
    with(|st| {
        let p = &mut st.panels[i];
        if !p.net_on { return; }
        let id = v.get("requestId").cloned().unwrap_or(Value::Null);
        match method {
            "Network.requestWillBeSent" => {
                let r = &v["request"];
                p.net_log.push_back(json!({ "id": id, "url": r["url"], "method": r["method"], "resourceType": v.get("type").cloned().unwrap_or(json!("Other")), "requestHeaders": r["headers"], "timestamp": now_ms() }));
                while p.net_log.len() > MAX_NET { p.net_log.pop_front(); }
            }
            "Network.responseReceived" => if let Some(e) = p.net_log.iter_mut().rev().find(|e| e["id"] == id) {
                let r = &v["response"];
                e["statusCode"] = r["status"].clone();
                e["statusLine"] = json!(format!("HTTP {} {}", r["status"], r["statusText"].as_str().unwrap_or("")));
                e["responseHeaders"] = r["headers"].clone();
                e["fromCache"] = json!(r["fromDiskCache"].as_bool().unwrap_or(false) || r["fromServiceWorker"].as_bool().unwrap_or(false));
            },
            "Network.loadingFinished" => if let Some(e) = p.net_log.iter_mut().rev().find(|e| e["id"] == id) { e["completedAt"] = json!(now_ms()); },
            "Network.loadingFailed" => if let Some(e) = p.net_log.iter_mut().rev().find(|e| e["id"] == id) {
                e["error"] = v["errorText"].clone();
                e["completedAt"] = json!(now_ms());
            },
            _ => {}
        }
    });
}
