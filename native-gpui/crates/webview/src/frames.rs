//! Live frames in the chat log (2.16f, specs/log-rest.md "Live frames in
//! detail"): a show_embed item's page in its box, as an off-screen CEF view
//! of its own (one per frame). Ported from the Slint client's
//! src/frames.rs, CEF side only: the log (crates/chat-log/src/frames.rs)
//! decides when a view is made and released and the cap, and calls these.
//!
//! A view: transparent, 30 frames a second, no JS dialogs, no context menu;
//! new windows go to the system browser (recorded in `opened`, not opened
//! under CRUNDI_TEST_PORT); an https or loopback page's first request
//! carries the server's origin as referrer (`set_origin`). The wheel goes
//! to the page only while its root can still scroll that way (F134).
//!
//! Every CEF callback runs on the main thread (lib.rs pumps CEF): the state
//! is a thread-local. Images replaced by newer frames wait in `drop_old`
//! for the window to free them.

use cef::*;
use gpui::{CursorStyle, Keystroke, MouseButton, RenderImage};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

struct Fv {
    /// The page's messages to its host (MSG_PREFIX console lines), oldest first.
    messages: Vec<serde_json::Value>,
    /// Loads finished (the host sends what a page wants on load).
    loads: u64,
    browser: Option<Browser>,
    w: i32,
    h: i32,
    scale: f32,
    /// The newest frame (BGRA) and how many so far; the image made from it.
    raw: Option<std::rc::Rc<(Vec<u8>, u32, u32)>>,
    frames: u64,
    image: Option<(u64, Arc<RenderImage>)>,
    painted: bool,
    cursor: i32,
    title: String,
    focused: bool,
    held: Option<MouseButtonType>,
    /// The page's root scroller: (top, most top, left, most left); None until read.
    scroll: Option<(f64, f64, f64, f64)>,
    cdp_next: i32,
    _observer: Option<Registration>,
}

#[derive(Default)]
struct St {
    frames: HashMap<String, Fv>,
    origin: String,
    opened: String,
    generation: u64,
    old: Vec<Arc<RenderImage>>,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St::default()); }
fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }
fn changed() { with(|s| s.generation += 1); }

/// The server's origin, the referrer of a page's first request.
pub fn set_origin(o: &str) { with(|s| s.origin = o.trim_end_matches('/').to_string()); }

/// Moves on every paint, title or cursor change (the log draws again).
pub fn generation() -> u64 { with(|s| s.generation) }

pub fn loopback(u: &str) -> bool {
    ["http://localhost:", "http://localhost/", "http://127.0.0.1:", "http://127.0.0.1/"].iter().any(|p| u.starts_with(p))
}

/// Make the view for `key` at `target` ("data:text/html;base64,…", an
/// https or loopback page), `w` x `h` logical px at `scale`; a view that is
/// there already is resized. False: no browser (CEF did not start, or
/// Chromium refused).
pub fn open(key: &str, target: &str, w: f32, h: f32, scale: f32) -> bool {
    if !crate::started() { return false; }
    if with(|s| s.frames.contains_key(key)) { resize(key, w, h, scale); return true; }
    let referrer = target.starts_with("https://") || loopback(target);
    let origin = if referrer { with(|s| s.origin.clone()) } else { String::new() };
    with(|s| s.frames.insert(key.to_string(), Fv { messages: vec![], loads: 0, browser: None, w: w.round().max(1.0) as i32, h: h.round().max(1.0) as i32, scale: scale.max(1.0), raw: None, frames: 0, image: None, painted: false, cursor: 0, title: String::new(), focused: false, held: None, scroll: None, cdp_next: 0, _observer: None }));
    let mut client = FrameClient::new(FrameRender::new(key.to_string()), FrameLife::new(key.to_string()), FrameDisplay::new(key.to_string()), FrameDialogs::new(), FrameMenu::new(), FrameLoad::new(key.to_string()), FrameRequests::new(FrameResource::new(target.to_string(), origin)));
    let window_info = WindowInfo::default().set_as_windowless(Default::default());
    // Transparent: a page without a background shows the box's colour.
    let settings = BrowserSettings { windowless_frame_rate: 30, background_color: 0x0000_0000, ..Default::default() };
    let b = browser_host_create_browser_sync(Some(&window_info), Some(&mut client), Some(&CefString::from(target)), Some(&settings), None, None);
    let Some(b) = b else { with(|s| s.frames.remove(key)); eprintln!("[gpui] frame {key}: CEF did not create the view"); return false };
    let mut obs = FrameDevTools::new(key.to_string());
    let reg = b.host().and_then(|h| h.add_dev_tools_message_observer(Some(&mut obs)));
    with(|s| if let Some(f) = s.frames.get_mut(key) { f.browser = Some(b); f._observer = reg; });
    changed();
    true
}

/// Release the view.
pub fn close(key: &str) {
    let f = with(|s| s.frames.remove(key));
    let Some(f) = f else { return };
    if let Some((_, img)) = f.image { with(|s| s.old.push(img)); }
    if let Some(h) = f.browser.and_then(|b| b.host()) { h.close_browser(1); }
    changed();
}

pub fn resize(key: &str, w: f32, h: f32, scale: f32) {
    let (w, h, scale) = (w.round().max(1.0) as i32, h.round().max(1.0) as i32, scale.max(1.0));
    let ch = with(|s| s.frames.get_mut(key).map(|f| { let c = f.w != w || f.h != h || (f.scale - scale).abs() > 0.01; f.w = w; f.h = h; f.scale = scale; c }).unwrap_or(false));
    if ch { if let Some(hh) = host(key) { hh.notify_screen_info_changed(); hh.was_resized(); } }
}

/// Hidden views stop painting (and use less).
pub fn hidden(key: &str, on: bool) { if let Some(h) = host(key) { h.was_hidden(on as i32); } }

/// The view's newest picture (made once per frame).
pub fn image(key: &str) -> Option<Arc<RenderImage>> {
    with(|s| {
        let f = s.frames.get_mut(key)?;
        if let Some((n, img)) = &f.image { if *n == f.frames { return Some(img.clone()); } }
        let raw = f.raw.clone()?;
        let buf = image::RgbaImage::from_raw(raw.1, raw.2, raw.0.clone())?;
        let img = Arc::new(RenderImage::new([image::Frame::new(buf)]));
        if let Some((_, old)) = f.image.replace((f.frames, img.clone())) { s.old.push(old); }
        Some(img)
    })
}

/// Free the pictures newer frames replaced (the window's pump).
pub fn drop_old(window: &mut gpui::Window) {
    for img in with(|s| std::mem::take(&mut s.old)) { let _ = window.drop_image(img); }
}

/// The page's root scroller: (top, most top, left, most left); None until read (added by Operations for the log's props, 2.16f).
pub fn scroll(key: &str) -> Option<(f64, f64, f64, f64)> { with(|s| s.frames.get(key).and_then(|f| f.scroll)) }
pub fn painted(key: &str) -> bool { with(|s| s.frames.get(key).is_some_and(|f| f.painted)) }
pub fn title(key: &str) -> String { with(|s| s.frames.get(key).map(|f| f.title.clone()).unwrap_or_default()) }
pub fn cursor(key: &str) -> CursorStyle { crate::view::cursor_style(with(|s| s.frames.get(key).map(|f| f.cursor).unwrap_or(0))) }
/// The last page a frame asked to open in a new window.
pub fn opened() -> String { with(|s| s.opened.clone()) }

/// The message bridge (a page's window.postMessage to its host, and back).
/// A top-level page's `parent` is itself: what it posts to "parent" reaches
/// its own window, where `BRIDGE_JS` (put first in a page the host makes)
/// passes the host's kinds on as console lines with this prefix.
pub const MSG_PREFIX: &str = "\u{2063}crundi-msg:";

/// The page's side of the bridge: messages of the given kinds (`cru` values,
/// or `type` values) posted to the window go to the host.
pub fn bridge_js(cru: &[&str], types: &[&str]) -> String {
    let j = |v: &[&str]| serde_json::to_string(v).unwrap_or_default();
    format!("<script>(function(){{var P={},C={},T={};addEventListener('message',function(e){{var d=e.data;if(e.source!==window||!d||typeof d!=='object')return;if(C.indexOf(d.cru)<0&&T.indexOf(d.type)<0)return;try{{console.debug(P+JSON.stringify(d));}}catch(x){{}}}});}})();</script>",
        serde_json::to_string(MSG_PREFIX).unwrap_or_default(), j(cru), j(types))
}

/// Post a message into the frame's page (window.postMessage(m, '*')).
pub fn post(key: &str, m: &serde_json::Value) {
    let Some(b) = with(|s| s.frames.get(key).and_then(|f| f.browser.clone())) else { return };
    if let Some(f) = b.main_frame() {
        let code = format!("window.postMessage({}, '*')", m);
        f.execute_java_script(Some(&CefString::from(code.as_str())), None, 0);
    }
}

/// The page's messages since the last call.
pub fn take_messages(key: &str) -> Vec<serde_json::Value> { with(|s| s.frames.get_mut(key).map(|f| std::mem::take(&mut f.messages)).unwrap_or_default()) }

/// How many loads the page finished.
pub fn loads(key: &str) -> u64 { with(|s| s.frames.get(key).map(|f| f.loads).unwrap_or(0)) }
/// Is there a view for this key?
pub fn exists(key: &str) -> bool { with(|s| s.frames.contains_key(key)) }

/// The views live now.
pub fn live() -> usize { with(|s| s.frames.values().filter(|f| f.browser.is_some()).count()) }

fn host(key: &str) -> Option<BrowserHost> { with(|s| s.frames.get(key).and_then(|f| f.browser.as_ref()).and_then(|b| b.host())) }

/// Ask the page where its root scroller is (after a load, a scroll).
fn ask_scroll(key: &str) {
    let msg = with(|s| {
        let f = s.frames.get_mut(key)?;
        f.cdp_next += 1;
        Some((f.browser.as_ref().and_then(|b| b.host())?, serde_json::json!({
            "id": f.cdp_next, "method": "Runtime.evaluate",
            "params": { "returnByValue": true, "expression": "(()=>{const e=document.scrollingElement||document.documentElement;return [e.scrollTop,e.scrollHeight-innerHeight,e.scrollLeft,e.scrollWidth-innerWidth]})()" }
        }).to_string()))
    });
    if let Some((h, m)) = msg { h.send_dev_tools_message(Some(m.as_bytes())); }
}

const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;

/// The pointer in the view (x, y in its logical px): 0 move, 1 down, 2 up,
/// 3 leave; `clicks`: 2 a double click, 3 a triple.
pub fn pointer(key: &str, kind: i32, x: f32, y: f32, button: MouseButton, clicks: usize) {
    let Some(h) = host(key) else { return };
    let held = with(|s| s.frames.get(key).and_then(|f| f.held));
    let btn = match button { MouseButton::Middle => MouseButtonType::MIDDLE, MouseButton::Right => MouseButtonType::RIGHT, _ => MouseButtonType::LEFT };
    let ev = MouseEvent { x: x.round() as i32, y: y.round() as i32, modifiers: if held.is_some() { EVENTFLAG_LEFT_MOUSE_BUTTON } else { 0 } };
    let n = clicks.clamp(1, 3) as i32;
    match kind {
        0 => h.send_mouse_move_event(Some(&ev), 0),
        1 => { focus(key, true); with(|s| if let Some(f) = s.frames.get_mut(key) { f.held = Some(btn); }); h.send_mouse_click_event(Some(&ev), btn, 0, n); }
        2 => { with(|s| if let Some(f) = s.frames.get_mut(key) { f.held = None; }); h.send_mouse_click_event(Some(&ev), held.unwrap_or(btn), 1, n); }
        _ => h.send_mouse_move_event(Some(&ev), 1),
    }
}

/// A wheel step (dx, dy; positive: up / left): true when the page took it
/// (its root can still scroll that way); false: the log scrolls instead.
pub fn wheel(key: &str, x: f32, y: f32, dx: f32, dy: f32) -> bool {
    let takes = with(|s| {
        let Some(f) = s.frames.get(key) else { return false };
        if f.browser.is_none() { return false; }
        let Some((top, max_top, left, max_left)) = f.scroll else { return false };
        let v = if dy > 0.0 { top > 0.5 } else if dy < 0.0 { top < max_top - 0.5 } else { false };
        let hz = if dx > 0.0 { left > 0.5 } else if dx < 0.0 { left < max_left - 0.5 } else { false };
        v || hz
    });
    if takes { if let Some(h) = host(key) { h.send_mouse_wheel_event(Some(&MouseEvent { x: x as i32, y: y as i32, modifiers: 0 }), dx as i32, dy as i32); } }
    takes
}

/// The frame takes (or gives back) the keyboard; one at a time.
pub fn focus(key: &str, on: bool) {
    let others: Vec<String> = if on { with(|s| s.frames.iter().filter(|(k, f)| *k != key && f.focused).map(|(k, _)| k.clone()).collect()) } else { vec![] };
    for k in others { focus(&k, false); }
    let ch = with(|s| s.frames.get_mut(key).map(|f| { let c = f.focused != on; f.focused = on; c }).unwrap_or(false));
    if let Some(h) = host(key) { h.set_focus(on as i32); }
    if ch { changed(); }
}

pub fn focused() -> Option<String> { with(|s| s.frames.iter().find(|(_, f)| f.focused && f.browser.is_some()).map(|(k, _)| k.clone())) }

/// A key for the frame: pressed and released.
pub fn key(key: &str, ks: &Keystroke) {
    let Some(h) = host(key) else { return };
    let named = match ks.key.as_str() {
        "enter" => Some(13), "backspace" => Some(8), "tab" => Some(9), "escape" => Some(27), "delete" => Some(46),
        "up" => Some(38), "down" => Some(40), "left" => Some(37), "right" => Some(39),
        "home" => Some(36), "end" => Some(35), "pageup" => Some(33), "pagedown" => Some(34),
        _ => None,
    };
    let m = &ks.modifiers;
    let modifiers = (if m.shift { 1 << 1 } else { 0 }) | (if m.control { 1 << 2 } else { 0 }) | (if m.alt { 1 << 3 } else { 0 }) | (if m.platform { 1 << 7 } else { 0 });
    let ch = ks.key_char.as_ref().and_then(|c| c.chars().next()).or_else(|| if ks.key.chars().count() == 1 { ks.key.chars().next() } else { None });
    let code = named.or_else(|| ch.map(|c| c.to_ascii_uppercase() as i32)).unwrap_or(0);
    let ev = |t: KeyEventType, c: u16| KeyEvent { type_: t, modifiers, windows_key_code: code, character: c, unmodified_character: c, ..Default::default() };
    h.send_key_event(Some(&ev(KeyEventType::RAWKEYDOWN, 0)));
    let typed = if named == Some(13) { Some('\r') } else if named.is_none() && !m.control && !m.platform { ch } else { None };
    if let Some(c) = typed { let mut buf = [0u16; 2]; h.send_key_event(Some(&ev(KeyEventType::CHAR, c.encode_utf16(&mut buf)[0]))); }
    h.send_key_event(Some(&ev(KeyEventType::KEYUP, 0)));
}

// ─── CEF handlers ───

wrap_render_handler! {
    pub struct FrameRender {
        key: String,
    }
    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(r) = rect {
                let (w, h) = with(|s| s.frames.get(&self.key).map(|f| (f.w, f.h)).unwrap_or((1, 1)));
                *r = Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) };
            }
        }
        fn screen_info(&self, _browser: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> ::std::os::raw::c_int {
            match info {
                Some(i) => {
                    let (w, h, k) = with(|s| s.frames.get(&self.key).map(|f| (f.w, f.h, f.scale)).unwrap_or((1, 1, 1.0)));
                    i.device_scale_factor = k;
                    i.rect = Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) };
                    i.available_rect = i.rect.clone();
                    1
                }
                None => 0,
            }
        }
        fn on_scroll_offset_changed(&self, _browser: Option<&mut Browser>, _x: f64, _y: f64) {
            ask_scroll(&self.key);
        }
        fn on_paint(&self, _browser: Option<&mut Browser>, type_: PaintElementType, _dirty: Option<&[Rect]>, buffer: *const u8, width: ::std::os::raw::c_int, height: ::std::os::raw::c_int) {
            if type_ != PaintElementType::VIEW || buffer.is_null() || width <= 0 || height <= 0 { return; }
            let (w, h) = (width as u32, height as u32);
            let src = unsafe { std::slice::from_raw_parts(buffer, (w * h * 4) as usize) };
            // BGRA as GPUI's images take it.
            let raw = std::rc::Rc::new((src.to_vec(), w, h));
            with(|s| if let Some(f) = s.frames.get_mut(&self.key) { if f.browser.is_some() { f.raw = Some(raw); f.frames += 1; f.painted = true; } });
            changed();
        }
    }
}

wrap_life_span_handler! {
    pub struct FrameLife {
        key: String,
    }
    impl LifeSpanHandler {
        // New windows (window.open, target=_blank): the system browser.
        fn on_before_popup(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _popup_id: ::std::os::raw::c_int, target_url: Option<&CefString>, _target_frame_name: Option<&CefString>, _target_disposition: WindowOpenDisposition, _user_gesture: ::std::os::raw::c_int, _popup_features: Option<&PopupFeatures>, _window_info: Option<&mut WindowInfo>, _client: Option<&mut Option<Client>>, _settings: Option<&mut BrowserSettings>, _extra_info: Option<&mut Option<DictionaryValue>>, _no_javascript_access: Option<&mut ::std::os::raw::c_int>) -> ::std::os::raw::c_int {
            let u = target_url.map(|u| u.to_string()).unwrap_or_default();
            if u.starts_with("http://") || u.starts_with("https://") {
                with(|s| s.opened = u.clone());
                if std::env::var_os("CRUNDI_TEST_PORT").is_none() { open_system(&u); }
            }
            let _ = &self.key;
            1
        }
    }
}

wrap_display_handler! {
    pub struct FrameDisplay {
        key: String,
    }
    impl DisplayHandler {
        fn on_console_message(&self, _browser: Option<&mut Browser>, _level: LogSeverity, message: Option<&CefString>, _source: Option<&CefString>, _line: ::std::os::raw::c_int) -> ::std::os::raw::c_int {
            let m = message.map(|m| m.to_string()).unwrap_or_default();
            if std::env::var_os("CRUNDI_DEBUG_FRAMES").is_some() { eprintln!("[frame console] {m:?}"); }
            let Some(rest) = m.strip_prefix(MSG_PREFIX) else { return 0 };
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) { with(|s| if let Some(f) = s.frames.get_mut(&self.key) { f.messages.push(v); }); changed(); }
            1
        }
        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let t = title.map(|t| t.to_string()).unwrap_or_default();
            with(|s| if let Some(f) = s.frames.get_mut(&self.key) { f.title = t; });
        }
        fn on_cursor_change(&self, _browser: Option<&mut Browser>, _cursor: PageCursor, type_: CursorType, _custom_cursor_info: Option<&CursorInfo>) -> ::std::os::raw::c_int {
            let c = crate::engine::cursor_kind(type_);
            let ch = with(|s| s.frames.get_mut(&self.key).map(|f| { let ch = f.cursor != c; f.cursor = c; ch }).unwrap_or(false));
            if ch { changed(); }
            1
        }
    }
}

#[cfg(target_os = "macos")]
pub type PageCursor = *mut u8;
#[cfg(not(target_os = "macos"))]
pub type PageCursor = CursorHandle;

wrap_jsdialog_handler! {
    pub struct FrameDialogs {}
    impl JsdialogHandler {
        // alert / confirm / prompt: a windowless view shows none (PLAN.md).
        fn on_jsdialog(&self, _browser: Option<&mut Browser>, _origin_url: Option<&CefString>, _dialog_type: JsdialogType, _message_text: Option<&CefString>, _default_prompt_text: Option<&CefString>, _callback: Option<&mut JsdialogCallback>, suppress_message: Option<&mut ::std::os::raw::c_int>) -> ::std::os::raw::c_int {
            if let Some(s) = suppress_message { *s = 1; }
            0
        }
    }
}

wrap_context_menu_handler! {
    pub struct FrameMenu {}
    impl ContextMenuHandler {
        // Right-click: no menu (Electron shows none in a frame).
        fn on_before_context_menu(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _params: Option<&mut ContextMenuParams>, model: Option<&mut MenuModel>) {
            if let Some(m) = model { m.clear(); }
        }
    }
}

wrap_load_handler! {
    pub struct FrameLoad {
        key: String,
    }
    impl LoadHandler {
        fn on_loading_state_change(&self, _browser: Option<&mut Browser>, is_loading: ::std::os::raw::c_int, _back: ::std::os::raw::c_int, _fwd: ::std::os::raw::c_int) {
            if is_loading == 0 { with(|s| if let Some(f) = s.frames.get_mut(&self.key) { f.loads += 1; }); changed(); ask_scroll(&self.key); }
        }
    }
}

wrap_dev_tools_message_observer! {
    pub struct FrameDevTools {
        key: String,
    }
    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(&self, _browser: Option<&mut Browser>, _message_id: ::std::os::raw::c_int, success: ::std::os::raw::c_int, result: Option<&[u8]>) {
            if success == 0 { return; }
            let v: serde_json::Value = result.and_then(|r| serde_json::from_slice(r).ok()).unwrap_or(serde_json::Value::Null);
            let Some(a) = v.pointer("/result/value").and_then(|x| x.as_array()) else { return };
            let n = |i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0);
            with(|s| if let Some(f) = s.frames.get_mut(&self.key) { f.scroll = Some((n(0), n(1).max(0.0), n(2), n(3).max(0.0))); });
        }
    }
}

// The first request of an https page carries the server's origin as its
// referrer (strict-origin-when-cross-origin), as the web's iframe sends it:
// YouTube's player refuses a load without one. On CEF's IO thread: the
// handler holds what it needs (no thread-local state).
wrap_resource_request_handler! {
    pub struct FrameResource {
        url: String,
        origin: String,
    }
    impl ResourceRequestHandler {
        fn on_before_resource_load(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, request: Option<&mut Request>, _callback: Option<&mut Callback>) -> ReturnValue {
            if let (Some(fr), Some(req)) = (frame, request) {
                let main = fr.is_main() != 0;
                let url = CefString::from(&req.url()).to_string();
                let has = !CefString::from(&req.referrer_url()).to_string().is_empty();
                if main && !self.origin.is_empty() && !has && url == self.url {
                    req.set_referrer(Some(&CefString::from(format!("{}/", self.origin).as_str())), ReferrerPolicy::REDUCE_REFERRER_GRANULARITY_ON_TRANSITION_CROSS_ORIGIN);
                }
            }
            ReturnValue::CONTINUE
        }
    }
}

wrap_request_handler! {
    pub struct FrameRequests {
        resource: ResourceRequestHandler,
    }
    impl RequestHandler {
        fn resource_request_handler(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _request: Option<&mut Request>, _is_navigation: ::std::os::raw::c_int, _is_download: ::std::os::raw::c_int, _request_initiator: Option<&CefString>, _disable_default_handling: Option<&mut ::std::os::raw::c_int>) -> Option<ResourceRequestHandler> {
            Some(self.resource.clone())
        }
    }
}

wrap_client! {
    pub struct FrameClient {
        render: RenderHandler,
        life: LifeSpanHandler,
        display: DisplayHandler,
        dialogs: JsdialogHandler,
        menu: ContextMenuHandler,
        load: LoadHandler,
        requests: RequestHandler,
    }
    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> { Some(self.render.clone()) }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(self.life.clone()) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(self.display.clone()) }
        fn jsdialog_handler(&self) -> Option<JsdialogHandler> { Some(self.dialogs.clone()) }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> { Some(self.menu.clone()) }
        fn load_handler(&self) -> Option<LoadHandler> { Some(self.load.clone()) }
        fn request_handler(&self) -> Option<RequestHandler> { Some(self.requests.clone()) }
    }
}

/// A page in the system's browser.
fn open_system(u: &str) {
    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "windows") { ("cmd", vec!["/C", "start", "", u]) } else if cfg!(target_os = "macos") { ("open", vec![u]) } else { ("xdg-open", vec![u]) };
    let _ = std::process::Command::new(cmd).args(args).spawn();
}
