//! The host's views: one off-screen CEF browser per view the app opened,
//! with CEF's handlers turned into events (ipc.rs `Ev`) and the app's
//! commands (`Cmd`) turned into CEF calls. Ported from the in-process
//! engine.rs (drawer pages, hidden browsers, widget panels) and frames.rs
//! (a chat's live frames, Cru's, What's new): one kind of view, `frame`
//! choosing a frame's differences (transparent, no menus or dialogs, new
//! windows to the app, the referrer on its first request).
//!
//! Every CEF callback here runs on the host's main thread (it pumps CEF),
//! apart from the auth and resource handlers (CEF's IO thread), which hold
//! what they need or read a lock.

use crate::host::{emit, emit_fd};
use crate::ipc::{Cmd, Ev, Proxy};
use crate::shm::Segment;
use cef::*;
use serde_json::{json, Value};
use std::{cell::{Cell, RefCell}, collections::HashMap, sync::Mutex};

struct HView {
    browser: Option<Browser>,
    frame: bool,
    w: i32,
    h: i32,
    dpr: f32,
    /// The frame segment, how many were made (a bigger frame makes a new
    /// one), and the handshake: a frame waits for the app's Ack; one that
    /// came meanwhile is drawn again once it does.
    seg: Option<Segment>,
    epoch: u32,
    busy: bool,
    dirty: bool,
    /// The segment no longer holds the page's last frame (new, resized, a
    /// frame was let go): the next one is copied whole, not by its dirty
    /// rects.
    full: bool,
    /// The size of the frame in the segment.
    seg_wh: (u32, u32),
    /// Not on screen (`Cmd::Hidden`): no frames sent.
    hidden: bool,
    _obs: Option<Registration>,
}

/// The host's frame counters (`Ev::Stats`).
#[derive(Default, Clone, PartialEq)]
struct Stats { sent: u64, unchanged: u64, hidden: u64, copied: u64, gpu: u64, error: String }

thread_local! {
    static VIEWS: RefCell<HashMap<u64, HView>> = RefCell::new(HashMap::new());
    /// One profile per network (the drawer's partitions): "client" is CEF's
    /// own; "server", each homepool network and each hidden browser ("mem:")
    /// their own.
    static CONTEXTS: RefCell<HashMap<String, RequestContext>> = RefCell::new(HashMap::new());
    /// Browsers made and not yet gone (on_before_close).
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static STATS: RefCell<(Stats, Stats)> = RefCell::new(Default::default());
    /// The app takes GPU textures (`Hello`).
    static TEXTURES: Cell<bool> = const { Cell::new(false) };
    #[cfg(any(windows, target_os = "macos"))]
    static READER: RefCell<crate::gpu::Reader> = RefCell::new(crate::gpu::Reader::new());
}

fn count(f: impl FnOnce(&mut Stats)) { STATS.with(|s| f(&mut s.borrow_mut().0)); }

pub fn set_textures(on: bool) { TEXTURES.with(|t| t.set(on)); }

/// The counters to the app when they moved.
pub fn send_stats() {
    let now = STATS.with(|s| { let mut s = s.borrow_mut(); if s.0 == s.1 { None } else { s.1 = s.0.clone(); Some(s.0.clone()) } });
    if let Some(s) = now {
        emit(&Ev::Stats { sent: s.sent, unchanged: s.unchanged, hidden: s.hidden, copied: s.copied, gpu: s.gpu, mode: crate::gpu::name(crate::gpu::mode()).into(), error: s.error });
    }
}

/// The app's network proxies' sign-ins, by port (CEF asks on its IO thread).
static PROXIES: Mutex<Vec<(u16, String, String)>> = Mutex::new(Vec::new());

fn with<R>(f: impl FnOnce(&mut HashMap<u64, HView>) -> R) -> R { VIEWS.with(|v| f(&mut v.borrow_mut())) }
fn host_of(id: u64) -> Option<BrowserHost> { with(|v| v.get(&id).and_then(|x| x.browser.as_ref()).and_then(|b| b.host())) }
fn browser_of(id: u64) -> Option<Browser> { with(|v| v.get(&id).and_then(|x| x.browser.clone())) }

/// Browsers still there (the host waits for them to go before it shuts CEF down).
pub fn live() -> usize { LIVE.with(|l| l.get()) }

/// Close every view (the host is leaving).
pub fn close_all() {
    let all: Vec<u64> = with(|v| v.keys().copied().collect());
    for id in all { close(id); }
}

fn close(id: u64) {
    let Some(v) = with(|v| v.remove(&id)) else { return };
    if let Some(h) = v.browser.and_then(|b| b.host()) { h.close_browser(1); }
    drop(v.seg);
    emit(&Ev::Closed { id });
}

/// One command from the app. False: leave.
pub fn handle(c: Cmd) -> bool {
    match c {
        Cmd::Hello { .. } => {}
        Cmd::Quit => return false,
        Cmd::Open { id, url, frame, network, proxy, origin, w, h, dpr } => open(id, &url, frame, &network, proxy, &origin, w, h, dpr),
        Cmd::Close { id } => close(id),
        Cmd::Size { id, w, h, dpr, screen } => {
            let known = with(|v| v.get_mut(&id).map(|x| { x.w = w.max(1); x.h = h.max(1); x.dpr = dpr.max(0.5); }).is_some());
            if known { with(|v| if let Some(x) = v.get_mut(&id) { x.full = true; }); if let Some(hh) = host_of(id) { if screen { hh.notify_screen_info_changed(); } hh.was_resized(); } }
        }
        Cmd::Navigate { id, url } => if let Some(f) = browser_of(id).and_then(|b| b.main_frame()) { f.load_url(Some(&CefString::from(url.as_str()))); },
        Cmd::History { id, h } => if let Some(b) = browser_of(id) { match h { -1 => b.go_back(), 1 => b.go_forward(), _ => b.reload() } },
        Cmd::Mouse { id, kind, x, y, button, clicks, modifiers } => if let Some(hh) = host_of(id) {
            let ev = MouseEvent { x, y, modifiers };
            let b = match button { 1 => MouseButtonType::MIDDLE, 2 => MouseButtonType::RIGHT, _ => MouseButtonType::LEFT };
            match kind {
                0 => hh.send_mouse_move_event(Some(&ev), 0),
                1 => hh.send_mouse_click_event(Some(&ev), b, 0, clicks.max(1)),
                2 => hh.send_mouse_click_event(Some(&ev), b, 1, clicks.max(1)),
                _ => hh.send_mouse_move_event(Some(&ev), 1),
            }
        },
        Cmd::Wheel { id, x, y, dx, dy } => if let Some(hh) = host_of(id) { hh.send_mouse_wheel_event(Some(&MouseEvent { x, y, modifiers: 0 }), dx, dy); },
        Cmd::Key { id, kind, modifiers, code, ch } => if let Some(hh) = host_of(id) {
            let t = match kind { 1 => KeyEventType::KEYDOWN, 2 => KeyEventType::KEYUP, 3 => KeyEventType::CHAR, _ => KeyEventType::RAWKEYDOWN };
            hh.send_key_event(Some(&KeyEvent { type_: t, modifiers, windows_key_code: code, character: ch, unmodified_character: ch, ..Default::default() }));
        },
        Cmd::Ime { id, text } => if let Some(hh) = host_of(id) { hh.ime_commit_text(Some(&CefString::from(text.as_str())), None, 0); },
        Cmd::Focus { id, on } => if let Some(hh) = host_of(id) { hh.set_focus(on as i32); },
        Cmd::Hidden { id, on } => {
            let ch = with(|v| v.get_mut(&id).map(|x| { let ch = x.hidden != on; x.hidden = on; if on { x.full = true; } ch }).unwrap_or(false));
            if let (true, Some(hh)) = (ch, host_of(id)) {
                // Hidden: Chromium stops drawing (and throttles the page as a
                // background tab); the frame rate down besides. Shown: the
                // rate back and a whole frame at once.
                hh.was_hidden(on as i32);
                hh.set_windowless_frame_rate(if on { 1 } else { FPS });
                if !on { hh.invalidate(PaintElementType::VIEW); }
            }
        }
        Cmd::Cdp { id, call, method, params } => match host_of(id) {
            Some(hh) => { let m = json!({ "id": call, "method": method, "params": params }).to_string(); hh.send_dev_tools_message(Some(m.as_bytes())); }
            None => emit(&Ev::CdpResult { id, call, ok: false, result: json!({ "message": "no browser in this panel yet" }) }),
        },
        Cmd::Js { id, code } => if let Some(f) = browser_of(id).and_then(|b| b.main_frame()) { f.execute_java_script(Some(&CefString::from(code.as_str())), None, 0); },
        Cmd::DevTools { id } => if let Some(hh) = host_of(id) {
            if hh.has_dev_tools() != 0 { hh.close_dev_tools(); } else { {
                // A window of its own, in the pages' (windowless) runtime style: a Chrome-style
                // DevTools for an Alloy page is refused, and no window opened.
                let wi = WindowInfo { runtime_style: RuntimeStyle::ALLOY, bounds: Rect { x: 80, y: 80, width: 1000, height: 700 }, ..WindowInfo::default() };
                hh.show_dev_tools(Some(&wi), None, Some(&BrowserSettings::default()), None);
            } }
        },
        Cmd::Ack { id } => {
            let redraw = with(|v| v.get_mut(&id).map(|x| { x.busy = false; std::mem::take(&mut x.dirty) }).unwrap_or(false));
            if redraw { if let Some(hh) = host_of(id) { hh.invalidate(PaintElementType::VIEW); } }
        }
        Cmd::ClearCookies { site } => clear_cookies(site.as_deref()),
        Cmd::DownloadTo { dl, path, .. } => download_to(dl, &path),
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn open(id: u64, url: &str, frame: bool, network: &str, proxy: Option<Proxy>, origin: &str, w: i32, h: i32, dpr: f32) {
    if with(|v| v.contains_key(&id)) { return; }
    with(|v| v.insert(id, HView { browser: None, frame, w: w.max(1), h: h.max(1), dpr: dpr.max(0.5), seg: None, epoch: 0, busy: false, dirty: false, full: true, seg_wh: (0, 0), hidden: false, _obs: None }));
    let referrer = if frame && !origin.is_empty() { origin.to_string() } else { String::new() };
    let mut client = ViewClient::new(
        ViewRender::new(id), ViewLife::new(id), ViewLoad::new(id), ViewDisplay::new(id),
        ViewDialogs::new(frame), ViewMenu::new(frame), ViewRequests::new(id, frame, ViewResource::new(url.to_string(), referrer)),
        ViewDownloads::new(id),
    );
    let mut window_info = WindowInfo::default().set_as_windowless(Default::default());
    // The GPU path (gpu.rs): frames as shared textures.
    window_info.shared_texture_enabled = (crate::gpu::mode() != crate::gpu::Mode::Off) as i32;
    // A frame is transparent: a page without a background shows its box's colour.
    let settings = BrowserSettings { windowless_frame_rate: FPS, background_color: if frame { 0x0000_0000 } else { 0xffff_ffff }, ..Default::default() };
    let mut ctx = if frame { None } else { context_for(network, proxy.as_ref()) };
    match browser_host_create_browser_sync(Some(&window_info), Some(&mut client), Some(&CefString::from(url)), Some(&settings), None, ctx.as_mut()) {
        Some(b) => {
            let mut obs = ViewDevTools::new(id);
            let reg = b.host().and_then(|h| h.add_dev_tools_message_observer(Some(&mut obs)));
            LIVE.with(|l| l.set(l.get() + 1));
            with(|v| if let Some(x) = v.get_mut(&id) { x.browser = Some(b); x._obs = reg; });
        }
        None => {
            eprintln!("[browser host] view {id}: CEF did not create the browser");
            with(|v| v.remove(&id));
            emit(&Ev::Gone { id, status: -1 });
        }
    }
}

fn context_for(network: &str, proxy: Option<&Proxy>) -> Option<RequestContext> {
    // A hidden browser: a profile of its own, in memory (main.js "mcp-browser-<key>").
    if network.starts_with("mem:") {
        let ctx = request_context_create_context(Some(&RequestContextSettings::default()), None)?;
        CONTEXTS.with(|c| c.borrow_mut().insert(network.to_string(), ctx.clone()));
        return Some(ctx);
    }
    let dir = crate::common::profile_dir(network);
    if dir.is_empty() { return None; }
    if let Some(c) = CONTEXTS.with(|c| c.borrow().get(network).cloned()) { return Some(c); }
    let path = crate::common::cef_root().join(&dir);
    let settings = RequestContextSettings { cache_path: CefString::from(path.to_string_lossy().as_ref()), persist_session_cookies: 1, ..Default::default() };
    let ctx = request_context_create_context(Some(&settings), None)?;
    CONTEXTS.with(|c| c.borrow_mut().insert(network.to_string(), ctx.clone()));
    if let Some(p) = proxy { set_proxy(network, &ctx, p); }
    Some(ctx)
}

/// A network's profile goes out through the app's proxy (net.rs), which
/// asks for the sign-in the app gave (auth_credentials below).
fn set_proxy(network: &str, ctx: &RequestContext, p: &Proxy) {
    {
        let mut ps = PROXIES.lock().unwrap();
        ps.retain(|x| x.0 != p.port);
        ps.push((p.port, p.user.clone(), p.pass.clone()));
    }
    let Some(dict) = dictionary_value_create() else { return };
    let s = |x: &str| CefString::from(x);
    dict.set_string(Some(&s("mode")), Some(&s("fixed_servers")));
    dict.set_string(Some(&s("server")), Some(&s(&format!("http://127.0.0.1:{}", p.port))));
    dict.set_string(Some(&s("bypass_list")), Some(&s("<-loopback>")));
    let Some(v) = value_create() else { return };
    let mut d = dict;
    v.set_dictionary(Some(&mut d));
    let mut v = v;
    let mut err = CefString::default();
    if ctx.set_preference(Some(&s("proxy")), Some(&mut v), Some(&mut err)) == 0 {
        eprintln!("[browser host] the {network} network's proxy was not set: {err}");
    }
}

/// Cookies in every profile made so far, and CEF's global one.
fn clear_cookies(site: Option<&str>) {
    let mut all: Vec<RequestContext> = CONTEXTS.with(|c| c.borrow().values().cloned().collect());
    if let Some(g) = request_context_get_global_context() { all.push(g); }
    for ctx in all {
        if let Some(cm) = ctx.cookie_manager(None) {
            match site {
                None => { cm.delete_cookies(None, None, None); }
                Some(s) => for o in [format!("https://{s}"), format!("http://{s}")] { cm.delete_cookies(Some(&CefString::from(o.as_str())), None, None); },
            }
        }
    }
}

/// A view's frame rate while shown (1 while hidden).
// 60, as Electron's: an animating page ran at 30 (the owner, 18 Nov: embeds
// laggy, low fps). Chromium paints only what changes, so a still page costs
// nothing more; a hidden view drops to 1 (above).
const FPS: i32 = 60;

/// A frame from CEF (the CPU path): into the view's segment, then `Paint`;
/// or, while the app has not taken the last one, noted and drawn again on
/// its Ack. Only the dirty rects are copied when the segment holds the
/// page's last frame, and a frame whose dirty rects hold the same pixels
/// as before is not sent at all. A hidden view's frames are not sent.
fn paint(id: u64, buffer: *const u8, width: i32, height: i32, dirty: &[Rect]) {
    if buffer.is_null() || width <= 0 || height <= 0 { return; }
    let (w, h) = (width as u32, height as u32);
    let need = (w as usize) * (h as usize) * 4;
    with(|v| {
        let Some(x) = v.get_mut(&id) else { return };
        if x.hidden { x.full = true; count(|s| s.hidden += 1); return; }
        if x.busy { x.dirty = true; x.full = true; return; }
        if !segment_for(id, x, need) { return; }
        if x.seg_wh != (w, h) { x.full = true; }
        let Some(seg) = x.seg.as_mut() else { return };
        let src = unsafe { std::slice::from_raw_parts(buffer, need) };
        let dst = &mut seg.bytes_mut()[..need];
        let row = w as usize * 4;
        let mut copied = 0usize;
        let changed = if x.full || dirty.is_empty() {
            dst.copy_from_slice(src);
            copied = need;
            true
        } else {
            let mut changed = false;
            for r in dirty {
                let (x0, y0) = (r.x.clamp(0, width) as usize, r.y.clamp(0, height) as usize);
                let (x1, y1) = ((r.x + r.width).clamp(0, width) as usize, (r.y + r.height).clamp(0, height) as usize);
                if x1 <= x0 || y1 <= y0 { continue; }
                for y in y0..y1 {
                    let (a, b) = (y * row + x0 * 4, y * row + x1 * 4);
                    if dst[a..b] != src[a..b] { dst[a..b].copy_from_slice(&src[a..b]); copied += b - a; changed = true; }
                }
            }
            changed
        };
        x.full = false;
        x.seg_wh = (w, h);
        count(|s| s.copied += copied as u64);
        if !changed { count(|s| s.unchanged += 1); return; }
        x.busy = true;
        count(|s| s.sent += 1);
        emit(&Ev::Paint { id, w, h });
    });
}

/// The view's segment holds `need` bytes (a new one made and sent when it doesn't).
fn segment_for(id: u64, x: &mut HView, need: usize) -> bool {
    if x.seg.as_ref().is_some_and(|s| s.cap() >= need) { return true; }
    x.epoch += 1;
    x.full = true;
    match Segment::create(need, id, x.epoch) {
        Ok((s, fd, name)) => { x.seg = Some(s); emit_fd(&Ev::Shm { id, cap: need, name }, fd); true }
        Err(e) => { eprintln!("[browser host] view {id}: no frame segment: {e}"); x.seg = None; false }
    }
}

/// A frame from CEF's GPU (gpu.rs): read back into the view's segment
/// inside this callback (the texture is CEF's only until it returns), then
/// `Paint`, as a CPU frame; with `Mode::Texture` the texture goes to the
/// app as well.
#[cfg(any(windows, target_os = "macos"))]
fn paint_gpu(id: u64, tex: *mut std::ffi::c_void) {
    if tex.is_null() { return; }
    with(|v| {
        let Some(x) = v.get_mut(&id) else { return };
        if x.hidden { count(|s| s.hidden += 1); return; }
        if x.busy { x.dirty = true; return; }
        let mut placed = false;
        let r = READER.with(|r| unsafe {
            r.borrow_mut().read(tex, &mut |w, h| {
                let need = (w as usize) * (h as usize) * 4;
                if !segment_for(id, x, need) { return None; }
                placed = true;
                x.seg.as_mut().map(|s| &mut s.bytes_mut()[..need] as *mut [u8])
            })
        });
        match r {
            Ok((w, h)) if placed => {
                x.busy = true;
                x.seg_wh = (w, h);
                count(|s| { s.sent += 1; s.gpu += 1; s.copied += (w as u64) * (h as u64) * 4; });
                emit(&Ev::Paint { id, w, h });
                #[cfg(windows)]
                if TEXTURES.with(|t| t.get()) {
                    let parent: u32 = std::env::var("CRUNDI_HOST_PARENT").ok().and_then(|p| p.parse().ok()).unwrap_or(0);
                    match unsafe { crate::gpu::duplicate_into(parent, tex) } {
                        Ok(handle) => emit(&Ev::Texture { id, handle, w, h }),
                        Err(e) => count(|s| s.error = e),
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                if STATS.with(|s| s.borrow().0.error != e) { eprintln!("[browser host] view {id}: a GPU frame was not read: {e}"); }
                count(|s| s.error = e);
            }
        }
    });
}

fn cursor_index(t: CursorType) -> i32 {
    use crate::common::cursor as c;
    match t {
        CursorType::HAND => c::HAND,
        CursorType::IBEAM | CursorType::VERTICALTEXT => c::IBEAM,
        CursorType::CROSS | CursorType::CELL => c::CROSS,
        CursorType::MOVE => c::MOVE,
        CursorType::WAIT | CursorType::PROGRESS => c::WAIT,
        CursorType::NOTALLOWED | CursorType::NODROP => c::NOT_ALLOWED,
        CursorType::EASTRESIZE | CursorType::WESTRESIZE | CursorType::EASTWESTRESIZE | CursorType::COLUMNRESIZE => c::EW,
        CursorType::NORTHRESIZE | CursorType::SOUTHRESIZE | CursorType::NORTHSOUTHRESIZE | CursorType::ROWRESIZE => c::NS,
        CursorType::NORTHEASTRESIZE | CursorType::SOUTHWESTRESIZE | CursorType::NORTHEASTSOUTHWESTRESIZE => c::NESW,
        CursorType::NORTHWESTRESIZE | CursorType::SOUTHEASTRESIZE | CursorType::NORTHWESTSOUTHEASTRESIZE => c::NWSE,
        CursorType::GRAB => c::GRAB,
        CursorType::GRABBING => c::GRABBING,
        CursorType::HELP => c::HELP,
        CursorType::NONE => c::NONE,
        _ => c::ARROW,
    }
}

#[cfg(target_os = "macos")]
type PageCursor = *mut u8;
#[cfg(not(target_os = "macos"))]
type PageCursor = CursorHandle;

fn is_frame(id: u64) -> bool { with(|v| v.get(&id).is_some_and(|x| x.frame)) }

// ─── CEF's handlers ───

wrap_render_handler! {
    pub struct ViewRender {
        id: u64,
    }
    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(r) = rect {
                let (w, h) = with(|v| v.get(&self.id).map(|x| (x.w, x.h)).unwrap_or((1, 1)));
                *r = Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) };
            }
        }
        fn screen_info(&self, _browser: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> ::std::os::raw::c_int {
            match info {
                Some(s) => {
                    let (w, h, k) = with(|v| v.get(&self.id).map(|x| (x.w, x.h, x.dpr)).unwrap_or((1, 1, 1.0)));
                    s.device_scale_factor = k;
                    s.rect = Rect { x: 0, y: 0, width: w.max(1), height: h.max(1) };
                    s.available_rect = s.rect.clone();
                    1
                }
                None => 0,
            }
        }
        fn on_scroll_offset_changed(&self, _browser: Option<&mut Browser>, _x: f64, _y: f64) {
            if is_frame(self.id) { emit(&Ev::Scrolled { id: self.id }); }
        }
        fn on_paint(&self, _browser: Option<&mut Browser>, type_: PaintElementType, dirty: Option<&[Rect]>, buffer: *const u8, width: ::std::os::raw::c_int, height: ::std::os::raw::c_int) {
            if type_ != PaintElementType::VIEW { return; }
            paint(self.id, buffer, width, height, dirty.unwrap_or(&[]));
        }
        fn on_accelerated_paint(&self, _browser: Option<&mut Browser>, type_: PaintElementType, _dirty: Option<&[Rect]>, info: Option<&AcceleratedPaintInfo>) {
            if type_ != PaintElementType::VIEW { return; }
            let Some(_info) = info else { return };
            #[cfg(windows)]
            paint_gpu(self.id, _info.shared_texture_handle as *mut std::ffi::c_void);
            #[cfg(target_os = "macos")]
            paint_gpu(self.id, _info.shared_texture_io_surface);
        }
    }
}

wrap_life_span_handler! {
    pub struct ViewLife {
        id: u64,
    }
    impl LifeSpanHandler {
        // No popup windows off-screen: the app decides (a new drawer tab,
        // the same page, the system's browser).
        fn on_before_popup(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _popup_id: ::std::os::raw::c_int, target_url: Option<&CefString>, _target_frame_name: Option<&CefString>, _target_disposition: WindowOpenDisposition, _user_gesture: ::std::os::raw::c_int, _popup_features: Option<&PopupFeatures>, _window_info: Option<&mut WindowInfo>, _client: Option<&mut Option<Client>>, _settings: Option<&mut BrowserSettings>, _extra_info: Option<&mut Option<DictionaryValue>>, _no_javascript_access: Option<&mut ::std::os::raw::c_int>) -> ::std::os::raw::c_int {
            if let Some(u) = target_url { emit(&Ev::Popup { id: self.id, url: u.to_string() }); }
            1
        }
        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            LIVE.with(|l| l.set(l.get().saturating_sub(1)));
        }
    }
}

wrap_load_handler! {
    pub struct ViewLoad {
        id: u64,
    }
    impl LoadHandler {
        fn on_load_error(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, error_code: Errorcode, error_text: Option<&CefString>, _failed_url: Option<&CefString>) {
            if frame.map(|f| f.is_main() == 0).unwrap_or(false) { return; }
            let code = *error_code.as_ref() as i32;
            if code == -3 { return; } // ERR_ABORTED: a navigation replaced it
            emit(&Ev::LoadError { id: self.id, code, text: error_text.map(|t| t.to_string()).unwrap_or_default() });
        }
        fn on_loading_state_change(&self, _browser: Option<&mut Browser>, is_loading: ::std::os::raw::c_int, back: ::std::os::raw::c_int, fwd: ::std::os::raw::c_int) {
            emit(&Ev::Loading { id: self.id, loading: is_loading != 0, back: back != 0, fwd: fwd != 0 });
        }
        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, http_status_code: ::std::os::raw::c_int) {
            if frame.map(|f| f.is_main() == 0).unwrap_or(false) { return; }
            emit(&Ev::LoadEnd { id: self.id, status: http_status_code });
        }
    }
}

wrap_display_handler! {
    pub struct ViewDisplay {
        id: u64,
    }
    impl DisplayHandler {
        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            emit(&Ev::Title { id: self.id, title: title.map(|t| t.to_string()).unwrap_or_default() });
        }
        fn on_cursor_change(&self, _browser: Option<&mut Browser>, _cursor: PageCursor, type_: CursorType, _custom_cursor_info: Option<&CursorInfo>) -> ::std::os::raw::c_int {
            emit(&Ev::Cursor { id: self.id, kind: cursor_index(type_) });
            1
        }
        fn on_console_message(&self, _browser: Option<&mut Browser>, level: LogSeverity, message: Option<&CefString>, source: Option<&CefString>, line: ::std::os::raw::c_int) -> ::std::os::raw::c_int {
            let lv = match level { LogSeverity::ERROR | LogSeverity::FATAL => "error", LogSeverity::WARNING => "warning", LogSeverity::VERBOSE => "debug", _ => "log" };
            emit(&Ev::Console { id: self.id, level: lv.into(), text: message.map(|m| m.to_string()).unwrap_or_default(), source: source.map(|m| m.to_string()).unwrap_or_default(), line });
            1
        }
        fn on_address_change(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if frame.map(|f| f.is_main() == 0).unwrap_or(false) { return; }
            emit(&Ev::Address { id: self.id, url: url.map(|u| u.to_string()).unwrap_or_default() });
        }
    }
}

wrap_dev_tools_message_observer! {
    pub struct ViewDevTools {
        id: u64,
    }
    impl DevToolsMessageObserver {
        fn on_dev_tools_event(&self, _browser: Option<&mut Browser>, method: Option<&CefString>, params: Option<&[u8]>) {
            let m = method.map(|m| m.to_string()).unwrap_or_default();
            if !m.starts_with("Network.") { return; }
            let v: Value = params.and_then(|r| serde_json::from_slice(r).ok()).unwrap_or(Value::Null);
            emit(&Ev::Net { id: self.id, method: m, params: v });
        }
        fn on_dev_tools_method_result(&self, _browser: Option<&mut Browser>, message_id: ::std::os::raw::c_int, success: ::std::os::raw::c_int, result: Option<&[u8]>) {
            let v: Value = result.and_then(|r| serde_json::from_slice(r).ok()).unwrap_or(Value::Null);
            emit(&Ev::CdpResult { id: self.id, call: message_id, ok: success != 0, result: v });
        }
    }
}

// No page dialogs. A drawer page (a chat drives it): alert dismissed,
// confirm yes, prompt nothing, leaving a page always allowed (main.js's
// overrides). A frame: none at all.
wrap_jsdialog_handler! {
    pub struct ViewDialogs {
        frame: bool,
    }
    impl JsdialogHandler {
        fn on_jsdialog(&self, _browser: Option<&mut Browser>, _origin_url: Option<&CefString>, dialog_type: JsdialogType, _message_text: Option<&CefString>, _default_prompt_text: Option<&CefString>, callback: Option<&mut JsdialogCallback>, suppress_message: Option<&mut ::std::os::raw::c_int>) -> ::std::os::raw::c_int {
            if self.frame || dialog_type == JsdialogType::ALERT { if let Some(s) = suppress_message { *s = 1; } return 0; }
            if let Some(cb) = callback { cb.cont(if dialog_type == JsdialogType::CONFIRM { 1 } else { 0 }, None); }
            1
        }
        fn on_before_unload_dialog(&self, _browser: Option<&mut Browser>, _message_text: Option<&CefString>, _is_reload: ::std::os::raw::c_int, callback: Option<&mut JsdialogCallback>) -> ::std::os::raw::c_int {
            if let Some(cb) = callback { cb.cont(1, None); }
            1
        }
    }
}

wrap_context_menu_handler! {
    pub struct ViewMenu {
        frame: bool,
    }
    impl ContextMenuHandler {
        // A frame: no menu on a right click (Electron shows none in a frame).
        fn on_before_context_menu(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _params: Option<&mut ContextMenuParams>, model: Option<&mut MenuModel>) {
            if self.frame { if let Some(m) = model { m.clear(); } }
        }
    }
}

// A frame's first request of an https or loopback page carries the server's
// origin as its referrer (strict-origin-when-cross-origin), as the web's
// iframe sends it: YouTube's player refuses a load without one. On CEF's IO
// thread: the handler holds what it needs.
wrap_resource_request_handler! {
    pub struct ViewResource {
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
    pub struct ViewRequests {
        id: u64,
        frame: bool,
        resource: ResourceRequestHandler,
    }
    impl RequestHandler {
        fn resource_request_handler(&self, _browser: Option<&mut Browser>, _frame: Option<&mut Frame>, _request: Option<&mut Request>, _is_navigation: ::std::os::raw::c_int, _is_download: ::std::os::raw::c_int, _request_initiator: Option<&CefString>, _disable_default_handling: Option<&mut ::std::os::raw::c_int>) -> Option<ResourceRequestHandler> {
            if self.frame { Some(self.resource.clone()) } else { None }
        }
        // The drawer's network proxies ask for their sign-in (net.rs).
        fn auth_credentials(&self, _browser: Option<&mut Browser>, _origin_url: Option<&CefString>, is_proxy: ::std::os::raw::c_int, host: Option<&CefString>, port: ::std::os::raw::c_int, _realm: Option<&CefString>, _scheme: Option<&CefString>, callback: Option<&mut AuthCallback>) -> ::std::os::raw::c_int {
            if is_proxy == 0 { return 0; }
            if host.map(|h| h.to_string()).as_deref() != Some("127.0.0.1") { return 0; }
            let creds = PROXIES.lock().unwrap().iter().find(|p| p.0 as i32 == port).map(|p| (p.1.clone(), p.2.clone()));
            match (creds, callback) {
                (Some((u, p)), Some(cb)) => { cb.cont(Some(&CefString::from(u.as_str())), Some(&CefString::from(p.as_str()))); 1 }
                _ => 0,
            }
        }
        fn on_render_process_terminated(&self, _browser: Option<&mut Browser>, status: TerminationStatus, _error_code: ::std::os::raw::c_int, _error_string: Option<&CefString>) {
            emit(&Ev::Gone { id: self.id, status: *status.as_ref() as i32 });
        }
    }
}

// ─── Downloads (app/main.js wireDownloads: one Save dialog per download) ───

thread_local! {
    /// Downloads waiting for the app's answer (where to save), by CEF's id.
    static PENDING: RefCell<HashMap<u32, BeforeDownloadCallback>> = RefCell::new(HashMap::new());
}

/// The app's answer: continue into `path` without CEF's own dialog, or
/// cancel ("": the callback let go unrun cancels the download).
fn download_to(dl: u32, path: &str) {
    let Some(cb) = PENDING.with(|p| p.borrow_mut().remove(&dl)) else { return };
    if !path.is_empty() { cb.cont(Some(&CefString::from(path)), 0); }
}

wrap_download_handler! {
    pub struct ViewDownloads {
        id: u64,
    }
    impl DownloadHandler {
        fn can_download(&self, _browser: Option<&mut Browser>, _url: Option<&CefString>, _request_method: Option<&CefString>) -> ::std::os::raw::c_int { 1 }
        // Held: the app asks the person where (Ev::Download), then Cmd::DownloadTo.
        fn on_before_download(&self, _browser: Option<&mut Browser>, download_item: Option<&mut DownloadItem>, suggested_name: Option<&CefString>, callback: Option<&mut BeforeDownloadCallback>) -> ::std::os::raw::c_int {
            let (Some(item), Some(cb)) = (download_item, callback) else { return 0 };
            let dl = item.id();
            let url = CefString::from(&item.url()).to_string();
            let name = suggested_name.map(|n| n.to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "download".into());
            PENDING.with(|p| p.borrow_mut().insert(dl, cb.clone()));
            emit(&Ev::Download { id: self.id, dl, name, url });
            1
        }
        fn on_download_updated(&self, _browser: Option<&mut Browser>, download_item: Option<&mut DownloadItem>, _callback: Option<&mut DownloadItemCallback>) {
            let Some(item) = download_item else { return };
            let done = item.is_complete() != 0;
            if done || item.is_canceled() != 0 || item.is_interrupted() != 0 {
                let path = CefString::from(&item.full_path()).to_string();
                emit(&Ev::Downloaded { id: self.id, dl: item.id(), path, ok: done });
            }
        }
    }
}

wrap_client! {
    pub struct ViewClient {
        render: RenderHandler,
        life: LifeSpanHandler,
        load: LoadHandler,
        display: DisplayHandler,
        dialogs: JsdialogHandler,
        menu: ContextMenuHandler,
        requests: RequestHandler,
        downloads: DownloadHandler,
    }
    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> { Some(self.render.clone()) }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(self.life.clone()) }
        fn load_handler(&self) -> Option<LoadHandler> { Some(self.load.clone()) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(self.display.clone()) }
        fn jsdialog_handler(&self) -> Option<JsdialogHandler> { Some(self.dialogs.clone()) }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> { Some(self.menu.clone()) }
        fn request_handler(&self) -> Option<RequestHandler> { Some(self.requests.clone()) }
        fn download_handler(&self) -> Option<DownloadHandler> { Some(self.downloads.clone()) }
    }
}
