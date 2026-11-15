//! Live frames in the chat log (2.16f, specs/log-rest.md "Live frames in
//! detail"): a show_embed item's page, in its box, as an off-screen CEF
//! view (the panels' bridge, src/browser.rs, one view per frame). A view is
//! made while its box is on the log (the box reports itself several times
//! a second) and released when it stops (scrolled away, the pane gone,
//! parked or hidden) or is more than RELEASE_PX from the view; at most CAP
//! live at once. No JS bindings, no token, no client hooks in a frame.
//!
//! Every CEF callback runs on the main thread (main.rs pumps CEF from
//! Slint's loop): the state is a thread-local.

use cef::*;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::{cell::RefCell, collections::HashMap, time::{Duration, Instant}};

/// Live views at most, across the window (the web has no cap: PLAN.md).
pub const CAP: usize = 6;
/// A box further than this from the log's view releases its view.
pub const RELEASE_PX: f32 = 4500.0;
/// A box that has not reported for this long is off the log (unrealised).
const STALE: Duration = Duration::from_millis(900);

pub struct Fv {
    pub chat: String,
    /// "stored:<name>" (the chat's stored HTML), or the https page.
    pub target: String,
    browser: Option<Browser>,
    /// The view's size (logical px: the box less its 1px edge) and scale.
    w: i32,
    h: i32,
    scale: f32,
    pub image: Option<Image>,
    pub painted: bool,
    pub cursor: i32,
    pub title: String,
    seen: Instant,
    pub on_screen: bool,
    dist: f32,
    hidden: bool,
    pub focused: bool,
    /// The stored page is on its way (or failed: retried next time).
    fetching: bool,
    held: Option<MouseButtonType>,
    /// Where its box is in the window (the test driver clicks through it).
    ax: f32,
    ay: f32,
    /// Clicks: the last press (when, where) and its count (double, triple).
    last_click: Option<(Instant, f32, f32, i32)>,
    /// The page's root scroller: (top, most top, left, most left); None until read.
    scroll: Option<(f64, f64, f64, f64)>,
    scroll_asked: Option<Instant>,
    cdp_next: i32,
    observer: Option<Registration>,
    /// The boxes reporting it lately (a chat full screen is drawn twice: the
    /// full copy and the grid's under it): (w, h, x, y, when). The largest wins.
    reports: Vec<(i32, i32, f32, f32, Instant)>,
}

#[derive(Default)]
struct St {
    frames: HashMap<String, Fv>,
    /// The server's origin (the referrer) and how to fetch stored HTML.
    origin: String,
    fetch: Option<Box<dyn Fn(String, String, String)>>,
    /// Counts paints (the view's images are re-read on it).
    gen: i32,
    on_change: Option<Box<dyn Fn(i32)>>,
    /// The last page a frame asked to open (the test driver reads it).
    pub opened: String,
    sweeping: bool,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St::default());
}

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }

/// Set up: the server's origin, a stored-HTML fetcher (chat, name, done), and
/// what to call when a frame repaints (the paint counter).
/// `fetch(chat, name, key)` asks for a stored page; its answer comes back
/// through [fetched].
pub fn init(origin: String, fetch: impl Fn(String, String, String) + 'static, on_change: impl Fn(i32) + 'static) {
    with(|s| { s.origin = origin; s.fetch = Some(Box::new(fetch)); s.on_change = Some(Box::new(on_change)); });
}

/// The server's origin (a frame's referrer), as signed in now.
pub fn set_origin(o: &str) { with(|s| if s.origin != o { s.origin = o.to_string(); }); }

fn changed() {
    let (g, cb) = with(|s| { s.gen += 1; (s.gen, s.on_change.take()) });
    if let Some(cb) = cb { cb(g); with(|s| s.on_change = Some(cb)); }
}

/// A box on the log says where it is: its frame (key), chat and target, the
/// view's size, whether it is in the log's view, and how far from it (px).
pub fn seen(key: &str, chat: &str, target: &str, w: f32, h: f32, ax: f32, ay: f32, scale: f32, on_screen: bool, dist: f32) {
    let (w, h) = ((w.max(1.0)).round() as i32, (h.max(1.0)).round() as i32);
    let mut create = false;
    let mut resize = None;
    let mut hide = None;
    with(|s| {
        let f = s.frames.entry(key.to_string()).or_insert_with(|| Fv {
            chat: chat.into(), target: target.into(), browser: None, w, h, scale, image: None, painted: false, cursor: 0, title: String::new(),
            seen: Instant::now(), on_screen, dist, hidden: false, focused: false, fetching: false, held: None, ax: 0.0, ay: 0.0, reports: vec![], last_click: None, scroll: None, scroll_asked: None, cdp_next: 0, observer: None,
        });
        f.seen = Instant::now();
        f.reports.retain(|r| r.4.elapsed() < Duration::from_millis(600));
        f.reports.push((w, h, ax, ay, Instant::now()));
        let best = f.reports.iter().max_by_key(|r| r.0 * r.1).cloned().unwrap_or((w, h, ax, ay, Instant::now()));
        let (w, h) = (best.0, best.1);
        f.ax = best.2;
        f.ay = best.3;
        f.on_screen = on_screen;
        f.dist = dist;
        if f.target != target { f.target = target.into(); }
        if (f.w, f.h, f.scale) != (w, h, scale) { f.w = w; f.h = h; f.scale = scale; resize = f.browser.as_ref().and_then(|b| b.host()); }
        if f.browser.is_some() && f.hidden == on_screen { f.hidden = !on_screen; hide = f.browser.as_ref().and_then(|b| b.host()).map(|h| (h, !on_screen)); }
        if f.browser.is_none() && !f.fetching && dist <= RELEASE_PX { create = true; }
    });
    if let Some(hst) = resize { hst.notify_screen_info_changed(); hst.was_resized(); }
    if let Some((hst, hidden)) = hide { hst.was_hidden(hidden as i32); if !hidden { hst.invalidate(PaintElementType::VIEW); } }
    if dist > RELEASE_PX { release(key); }
    // Its scroll extent may change as the page grows: asked again now and then.
    if with(|s| s.frames.get(key).is_some_and(|f| f.browser.is_some() && f.scroll_asked.is_none_or(|t| t.elapsed() > Duration::from_secs(1)))) { ask_scroll(key); }
    if create && make_room(key) { start(key); }
    sweep_soon();
}

/// Room for one more view: under the cap, or the live view farthest from
/// its log's view released (never one on screen).
fn make_room(key: &str) -> bool {
    let (n, victim) = with(|s| {
        let live: Vec<(&String, &Fv)> = s.frames.iter().filter(|(k, f)| f.browser.is_some() && *k != key).collect();
        let v = live.iter().filter(|(_, f)| !f.on_screen).max_by(|a, b| a.1.dist.total_cmp(&b.1.dist)).map(|(k, _)| (*k).clone());
        (live.len(), v)
    });
    if n < CAP { return true; }
    match victim { Some(k) => { release(&k); true } None => false }
}

/// Make the view: a stored page is fetched first (a data: URL), an https
/// page loads with the server's origin as its referrer.
fn start(key: &str) {
    let (chat, target) = with(|s| s.frames.get(key).map(|f| (f.chat.clone(), f.target.clone()))).unwrap_or_default();
    if let Some(name) = target.strip_prefix("stored:") {
        with(|s| if let Some(f) = s.frames.get_mut(key) { f.fetching = true; });
        let fetch = with(|s| s.fetch.take());
        if let Some(fetch) = fetch {
            fetch(chat, name.to_string(), key.to_string());
            with(|s| s.fetch = Some(fetch));
        }
    } else if target.starts_with("https://") || loopback(&target) {
        open_view(key, &target, true);
    }
}

/// An http page on this machine (a dev server, the test server) loads too;
/// any other page must be https, as the server sends them.
pub fn loopback(u: &str) -> bool {
    ["http://localhost:", "http://localhost/", "http://127.0.0.1:", "http://127.0.0.1/"].iter().any(|p| u.starts_with(p))
}

fn open_view(key: &str, url: &str, referrer: bool) {
    // The page loads at once (a later load_request with a referrer, after
    // about:blank, made Chromium kill the renderer: F132); its first
    // request gets the server's origin as referrer in the request handler.
    let origin = if referrer { with(|s| s.origin.clone()) } else { String::new() };
    let mut client = FrameClient::new(FrameRender::new(key.to_string()), FrameLife::new(key.to_string()), FrameDisplay::new(key.to_string()), FrameDialogs::new(), FrameMenu::new(), FrameLoad::new(key.to_string()), FrameRequests::new(FrameResource::new(url.to_string(), origin)));
    let mut window_info = WindowInfo::default().set_as_windowless(Default::default());
    window_info.shared_texture_enabled = 0;
    // Transparent: a page without a background shows the box's colour.
    let settings = BrowserSettings { windowless_frame_rate: 30, background_color: 0x0000_0000, ..Default::default() };
    let b = browser_host_create_browser_sync(Some(&window_info), Some(&mut client), Some(&CefString::from(url)), Some(&settings), None, None);
    let Some(b) = b else { eprintln!("[native] frame {key}: CEF did not create the view"); return };
    // Reading the page's scroll position (no writes into the page).
    let mut obs = FrameDevTools::new(key.to_string());
    let reg = b.host().and_then(|h| h.add_dev_tools_message_observer(Some(&mut obs)));
    with(|s| if let Some(f) = s.frames.get_mut(key) { f.browser = Some(b); f.observer = reg; f.painted = false; f.hidden = false; f.scroll = None; });
    changed();
}

/// Ask the page where its root scroller is (after a load, a scroll, now
/// and then): the wheel passes on to the log at its end (F134).
fn ask_scroll(key: &str) {
    let msg = with(|s| {
        let f = s.frames.get_mut(key)?;
        f.cdp_next += 1;
        f.scroll_asked = Some(Instant::now());
        Some((f.browser.as_ref().and_then(|b| b.host())?, serde_json::json!({
            "id": f.cdp_next, "method": "Runtime.evaluate",
            "params": { "returnByValue": true, "expression": "(()=>{const e=document.scrollingElement||document.documentElement;return [e.scrollTop,e.scrollHeight-innerHeight,e.scrollLeft,e.scrollWidth-innerWidth]})()" }
        }).to_string()))
    });
    if let Some((h, m)) = msg { h.send_dev_tools_message(Some(m.as_bytes())); }
}

/// Does the page take a wheel step (dx, dy; positive: up / left)? Only
/// while its root can still scroll that way; else the log scrolls.
pub fn wheel_takes(key: &str, dx: f32, dy: f32) -> bool {
    with(|s| {
        let Some(f) = s.frames.get(key) else { return false };
        if f.browser.is_none() { return false; }
        let Some((top, max_top, left, max_left)) = f.scroll else { return false };
        let v = if dy > 0.0 { top > 0.5 } else if dy < 0.0 { top < max_top - 0.5 } else { false };
        let h = if dx > 0.0 { left > 0.5 } else if dx < 0.0 { left < max_left - 0.5 } else { false };
        v || h
    })
}

/// A stored page's HTML came (None: gone or failed; tried again next time).
pub fn fetched(key: &str, html: Option<Vec<u8>>) {
    let still = with(|s| s.frames.get_mut(key).map(|f| { f.fetching = false; f.browser.is_none() && f.seen.elapsed() < STALE }).unwrap_or(false));
    let Some(html) = html else { return };
    // (The cap again: other pages may have been made while this one came.)
    if still && make_room(key) {
        use base64::Engine;
        let url = format!("data:text/html;charset=utf-8;base64,{}", base64::engine::general_purpose::STANDARD.encode(&html));
        open_view(key, &url, false);
    }
}

/// Release a view (its box stays; it comes back fresh).
pub fn release(key: &str) {
    let host = with(|s| s.frames.get_mut(key).and_then(|f| { f.image = None; f.painted = false; f.focused = false; f.observer = None; f.scroll = None; f.browser.take() }).and_then(|b| b.host()));
    if let Some(h) = host { h.close_browser(1); changed(); }
}

/// Every view whose box stopped reporting (scrolled off, the pane closed,
/// parked or hidden) is released; forgotten after a while.
fn sweep_soon() {
    if with(|s| std::mem::replace(&mut s.sweeping, true)) { return; }
    let t = slint::Timer::default();
    t.start(slint::TimerMode::Repeated, Duration::from_millis(300), || {
        let stale: Vec<String> = with(|s| s.frames.iter().filter(|(_, f)| f.seen.elapsed() > STALE && (f.browser.is_some() || f.fetching)).map(|(k, _)| k.clone()).collect());
        for k in stale { release(&k); with(|s| if let Some(f) = s.frames.get_mut(&k) { f.fetching = false; }); }
        with(|s| s.frames.retain(|_, f| f.seen.elapsed() < Duration::from_secs(60)));
    });
    std::mem::forget(t);
}

/// Release every view (the window closes).
pub fn release_all() {
    let keys: Vec<String> = with(|s| s.frames.keys().cloned().collect());
    for k in keys { release(&k); }
}

pub fn image(key: &str) -> Option<Image> { with(|s| s.frames.get(key).and_then(|f| f.image.clone())) }
pub fn painted(key: &str) -> bool { with(|s| s.frames.get(key).is_some_and(|f| f.painted)) }
pub fn cursor(key: &str) -> i32 { with(|s| s.frames.get(key).map(|f| f.cursor).unwrap_or(0)) }
pub fn live() -> usize { with(|s| s.frames.values().filter(|f| f.browser.is_some()).count()) }
pub fn opened() -> String { with(|s| s.opened.clone()) }

/// What the test driver reads: every frame known, live or not.
pub fn list() -> serde_json::Value {
    with(|s| serde_json::Value::Array(s.frames.iter().map(|(k, f)| serde_json::json!({
        "key": k, "live": f.browser.is_some(), "painted": f.painted, "title": f.title, "focused": f.focused,
        "onScreen": f.on_screen, "hidden": f.hidden, "w": f.w, "h": f.h, "x": f.ax, "y": f.ay,
        "target": f.target, "scroll": f.scroll.map(|x| vec![x.0, x.1, x.2, x.3]),
    })).collect()))
}

fn host(key: &str) -> Option<BrowserHost> { with(|s| s.frames.get(key).and_then(|f| f.browser.as_ref()).and_then(|b| b.host())) }

const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;
/// Two presses this close (ms, px) are a double click, as the system's.
const DOUBLE_MS: u64 = 500;
const DOUBLE_PX: f32 = 4.0;

/// Pointer in the box (x, y inside its 1px edge): 0 move, 1 down, 2 up, 3 leave.
pub fn pointer(key: &str, kind: i32, x: f32, y: f32, b: i32) {
    let Some(h) = host(key) else { return };
    let held = with(|s| s.frames.get(key).and_then(|f| f.held));
    let btn = match b { 1 => MouseButtonType::MIDDLE, 2 => MouseButtonType::RIGHT, _ => MouseButtonType::LEFT };
    let ev = MouseEvent { x: x.round() as i32, y: y.round() as i32, modifiers: if held.is_some() { EVENTFLAG_LEFT_MOUSE_BUTTON } else { 0 } };
    match kind {
        0 => h.send_mouse_move_event(Some(&ev), 0),
        1 => {
            focus(key, true);
            // Double and triple clicks: the same place, soon after (F133).
            let n = with(|s| s.frames.get_mut(key).map(|f| {
                let n = match f.last_click {
                    Some((t, px, py, c)) if t.elapsed() < Duration::from_millis(DOUBLE_MS) && (px - x).abs() <= DOUBLE_PX && (py - y).abs() <= DOUBLE_PX => (c % 3) + 1,
                    _ => 1,
                };
                f.last_click = Some((Instant::now(), x, y, n));
                f.held = Some(btn);
                n
            }).unwrap_or(1));
            h.send_mouse_click_event(Some(&ev), btn, 0, n);
        }
        2 => {
            let n = with(|s| s.frames.get_mut(key).map(|f| { f.held = None; f.last_click.map(|c| c.3).unwrap_or(1) }).unwrap_or(1));
            h.send_mouse_click_event(Some(&ev), held.unwrap_or(btn), 1, n);
        }
        _ => h.send_mouse_move_event(Some(&ev), 1),
    }
}

pub fn wheel(key: &str, x: f32, y: f32, dx: f32, dy: f32) {
    if let Some(h) = host(key) { h.send_mouse_wheel_event(Some(&MouseEvent { x: x as i32, y: y as i32, modifiers: 0 }), dx as i32, dy as i32); }
}

/// The frame takes (or gives back) the keyboard.
pub fn focus(key: &str, on: bool) {
    let others: Vec<String> = if on { with(|s| s.frames.iter().filter(|(k, f)| *k != key && f.focused).map(|(k, _)| k.clone()).collect()) } else { vec![] };
    for k in others { focus(&k, false); }
    let changed_now = with(|s| s.frames.get_mut(key).map(|f| { let c = f.focused != on; f.focused = on; c }).unwrap_or(false));
    if let Some(h) = host(key) { h.set_focus(on as i32); }
    if changed_now { changed(); }
}

pub fn focused() -> Option<String> { with(|s| s.frames.iter().find(|(_, f)| f.focused && f.browser.is_some()).map(|(k, _)| k.clone())) }

/// A key for the focused frame (Slint's key text; Ctrl and Shift held?).
pub fn key(key: &str, text: &str, down: bool, ctrl: bool, shift: bool, alt: bool, meta: bool) {
    let Some(h) = host(key) else { return };
    let Some(c) = text.chars().next() else { return };
    let named = |c: char| -> Option<i32> {
        Some(match c {
            '\u{8}' => 8, '\t' => 9, '\n' | '\r' => 13, '\u{1b}' => 27, '\u{7f}' => 46,
            '\u{F700}' => 38, '\u{F701}' => 40, '\u{F702}' => 37, '\u{F703}' => 39,
            '\u{F729}' => 36, '\u{F72B}' => 35, '\u{F72C}' => 33, '\u{F72D}' => 34,
            _ => return None,
        })
    };
    // EVENTFLAG_SHIFT_DOWN, _CONTROL_DOWN, _ALT_DOWN, _COMMAND_DOWN.
    let modifiers = (if shift { 1 << 1 } else { 0 }) | (if ctrl { 1 << 2 } else { 0 }) | (if alt { 1 << 3 } else { 0 }) | (if meta { 1 << 7 } else { 0 });
    let code = named(c).unwrap_or_else(|| c.to_ascii_uppercase() as i32);
    let ev = |t: KeyEventType, ch: u16| KeyEvent { type_: t, modifiers, windows_key_code: code, character: ch, unmodified_character: ch, ..Default::default() };
    if down {
        h.send_key_event(Some(&ev(KeyEventType::RAWKEYDOWN, 0)));
        if named(c).is_none() || c == '\n' || c == '\r' {
            let ch = if c == '\n' { '\r' } else { c };
            let mut buf = [0u16; 2];
            h.send_key_event(Some(&ev(KeyEventType::CHAR, ch.encode_utf16(&mut buf)[0])));
        }
    } else {
        h.send_key_event(Some(&ev(KeyEventType::KEYUP, 0)));
    }
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
            let mut pb = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
            // CEF paints BGRA (premultiplied); Slint wants RGBA.
            for (d, s) in pb.make_mut_bytes().chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                d[0] = s[2]; d[1] = s[1]; d[2] = s[0]; d[3] = s[3];
            }
            with(|s| if let Some(f) = s.frames.get_mut(&self.key) { if f.browser.is_some() { f.image = Some(Image::from_rgba8_premultiplied(pb)); f.painted = true; } });
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
                if std::env::var_os("CRUNDI_TEST_PORT").is_none() { let _ = open::that(&u); }
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
        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let t = title.map(|t| t.to_string()).unwrap_or_default();
            with(|s| if let Some(f) = s.frames.get_mut(&self.key) { f.title = t; });
        }
        fn on_cursor_change(&self, _browser: Option<&mut Browser>, _cursor: crate::frames::PageCursor, type_: CursorType, _custom_cursor_info: Option<&CursorInfo>) -> ::std::os::raw::c_int {
            let c = crate::browser::cursor_kind(type_);
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
            if is_loading == 0 { ask_scroll(&self.key); }
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
