//! The Slint–CEF bridge: off-screen (windowless) CEF browsers whose frames
//! become Slint images, with input, device emulation and CDP.
//!
//! Threading: CEF runs with its message loop pumped from Slint's event loop
//! (main.rs), so every CEF callback below runs on the main thread, the same
//! thread as Slint. Panel state lives in a thread-local and nothing needs a lock.
//!
//! Frames: `on_paint` gives a BGRA buffer of the view (CPU, software
//! compositing). It is copied into a Slint `SharedPixelBuffer` (RGBA). The
//! zero-copy path (CEF `on_accelerated_paint` shared textures imported into
//! Slint's wgpu renderer) is the next step, not in this prototype.

use base64::Engine;
use cef::*;
use serde_json::{json, Value};
use slint::{Image, Model, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc as StdRc};

use crate::{MainWindow, PanelData};

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

pub const DEVICES: &[Device] = &[
    Device { name: "Responsive", w: 0, h: 0, dpr: 1.0, mobile: false, ua: "" },
    Device { name: "iPhone 15 Pro", w: 393, h: 852, dpr: 3.0, mobile: true, ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "Pixel 7", w: 412, h: 915, dpr: 2.625, mobile: true, ua: "Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36" },
    Device { name: "iPad Air", w: 820, h: 1180, dpr: 2.0, mobile: true, ua: "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1" },
    Device { name: "Desktop 1280x800", w: 1280, h: 800, dpr: 1.0, mobile: false, ua: "" },
];

type CdpReply = Box<dyn FnOnce(bool, Value)>;

#[derive(Default)]
pub struct Panel {
    pub browser: Option<Browser>,
    observer: Option<Registration>,
    /// The panel's stage, in logical px (what Slint gives it).
    pub stage_w: f32,
    pub stage_h: f32,
    pub device: usize,
    /// A size set by the server (browserResize), instead of a named device.
    pub custom: Option<(i32, i32)>,
    pub frame: Option<SharedPixelBuffer<Rgba8Pixel>>,
    /// The GPU path's frame: a wgpu texture on Slint's device (src/gpu.rs).
    pub gpu_frame: Option<Image>,
    pub title: String,
    pub url: String,
    /// The server's key for the browser in this panel (who drives it).
    pub key: Option<String>,
    /// Annotation strokes, in page (view) CSS px so they stay on the page as the panel changes.
    pub strokes: Vec<Vec<(f32, f32)>>,
    drawing: bool,
    pub pin: Option<(f32, f32, String)>,
    mouse_held: Option<MouseButtonType>,
    cdp_next: i32,
    cdp_waiters: HashMap<i32, CdpReply>,
    load_waiters: Vec<Box<dyn FnOnce()>>,
    last_ua: String,
    /// The page's mouse cursor, as an index the Slint side maps to its own cursors.
    cursor: i32,
    /// The device dropdown's list. One model for the panel's life: Slint's
    /// ComboBox resets its choice whenever it is handed a different model.
    device_names: StdRc<VecModel<SharedString>>,
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
    /// How the page sits in the stage: scale and offset (Slint's image-fit: contain).
    fn fit(&self) -> (f32, f32, f32) {
        let (vw, vh, ..) = self.view();
        let (vw, vh) = (vw as f32, vh as f32);
        if self.stage_w <= 0.0 || self.stage_h <= 0.0 { return (1.0, 0.0, 0.0); }
        let s = (self.stage_w / vw).min(self.stage_h / vh);
        (s, (self.stage_w - vw * s) / 2.0, (self.stage_h - vh * s) / 2.0)
    }
    fn to_view(&self, x: f32, y: f32) -> (f32, f32) {
        let (s, ox, oy) = self.fit();
        ((x - ox) / s, (y - oy) / s)
    }
    fn to_stage(&self, x: f32, y: f32) -> (f32, f32) {
        let (s, ox, oy) = self.fit();
        (x * s + ox, y * s + oy)
    }
    fn host(&self) -> Option<BrowserHost> { self.browser.as_ref().and_then(|b| b.host()) }
}

pub struct State {
    pub panels: Vec<Panel>,
    pub model: StdRc<VecModel<PanelData>>,
    pub ui: slint::Weak<MainWindow>,
}

thread_local! {
    pub static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("state")))
}

/// Push a panel's state to the Slint model.
pub fn sync(i: usize) {
    with(|st| {
        let p = &st.panels[i];
        let (vw, vh, ..) = p.view();
        let strokes: Vec<SharedString> = p
            .strokes
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut c = String::new();
                for (n, &(x, y)) in s.iter().enumerate() {
                    let (px, py) = p.to_stage(x, y);
                    c.push_str(&format!("{}{:.1} {:.1} ", if n == 0 { "M" } else { "L" }, px, py));
                }
                if s.len() == 1 { let (px, py) = p.to_stage(s[0].0, s[0].1); c.push_str(&format!("L{:.1} {:.1}", px + 0.5, py)); }
                SharedString::from(c)
            })
            .collect();
        let (pin_x, pin_y, pin_label) = match &p.pin {
            Some((x, y, l)) => { let (px, py) = p.to_stage(*x, *y); (px, py, l.clone()) }
            None => (-1.0, -1.0, String::new()),
        };
        let data = PanelData {
            frame: p.gpu_frame.clone().or_else(|| p.frame.clone().map(Image::from_rgba8)).unwrap_or_default(),
            view_width: vw as f32,
            view_height: vh as f32,
            title: p.title.clone().into(),
            url: p.url.clone().into(),
            // The panel's own device list: the named devices, and the server's size when it set one.
            device: if p.custom.is_some() { DEVICES.len() as i32 } else { p.device as i32 },
            device_names: {
                let mut v: Vec<SharedString> = DEVICES.iter().map(|d| SharedString::from(d.name)).collect();
                if let Some((w, h)) = p.custom { v.push(format!("Custom {w}x{h}").into()); }
                if p.device_names.iter().ne(v.iter().cloned()) { p.device_names.set_vec(v); }
                p.device_names.clone().into()
            },
            cursor: p.cursor,
            driver: p.key.as_ref().map(|k| format!("driven by the server: {k}")).unwrap_or_default().into(),
            strokes: StdRc::new(VecModel::from(strokes)).into(),
            pin_x,
            pin_y,
            pin_label: pin_label.into(),
        };
        st.model.set_row_data(i, data);
    });
}

pub fn status(text: &str) {
    let t = text.to_string();
    with(|st| { if let Some(ui) = st.ui.upgrade() { ui.set_status(t.into()); } });
}

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
            let mut pb = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
            // CEF paints BGRA; Slint wants RGBA.
            for (d, s) in pb.make_mut_bytes().chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                d[0] = s[2]; d[1] = s[1]; d[2] = s[0]; d[3] = s[3];
            }
            with(|st| st.panels[self.index].frame = Some(pb));
            sync(self.index);
        }
        fn on_accelerated_paint(&self, _browser: Option<&mut Browser>, type_: PaintElementType, _dirty: Option<&[Rect]>, info: Option<&AcceleratedPaintInfo>) {
            #[cfg(feature = "gpu")]
            {
                if type_ != PaintElementType::VIEW { return; }
                let Some(info) = info else { return };
                if let Some(img) = crate::gpu::import(info) {
                    with(|st| st.panels[self.index].gpu_frame = Some(img));
                    sync(self.index);
                }
            }
            #[cfg(not(feature = "gpu"))]
            let _ = (type_, info);
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
                if let Some(f) = b.main_frame() { f.load_url(Some(u)); }
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
        fn on_loading_state_change(&self, _browser: Option<&mut Browser>, is_loading: ::std::os::raw::c_int, _back: ::std::os::raw::c_int, _fwd: ::std::os::raw::c_int) {
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

/// CEF's cursor kinds, folded into the ones the panel shows (ui/main.slint, page-cursor).
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
        fn on_address_change(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if frame.map(|f| f.is_main() == 0).unwrap_or(false) { return; }
            let u = url.map(|u| u.to_string()).unwrap_or_default();
            with(|st| { let p = &mut st.panels[self.index]; p.url = u; p.strokes.clear(); p.pin = None; });
            sync(self.index);
        }
    }
}

wrap_dev_tools_message_observer! {
    pub struct PanelDevTools {
        index: usize,
    }
    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(&self, _browser: Option<&mut Browser>, message_id: ::std::os::raw::c_int, success: ::std::os::raw::c_int, result: Option<&[u8]>) {
            let v: Value = result.and_then(|r| serde_json::from_slice(r).ok()).unwrap_or(Value::Null);
            let w = with(|st| st.panels[self.index].cdp_waiters.remove(&message_id));
            if let Some(w) = w { w(success != 0, v); }
        }
    }
}

wrap_client! {
    pub struct PanelClient {
        render: RenderHandler,
        life: LifeSpanHandler,
        load: LoadHandler,
        display: DisplayHandler,
    }
    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> { Some(self.render.clone()) }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(self.life.clone()) }
        fn load_handler(&self) -> Option<LoadHandler> { Some(self.load.clone()) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(self.display.clone()) }
    }
}

/// Make panel [index]'s browser, off-screen, at [url].
pub fn create(index: usize, url: &str) {
    let mut client = PanelClient::new(PanelRender::new(index), PanelLife::new(index), PanelLoad::new(index), PanelDisplay::new(index));
    let mut window_info = WindowInfo::default().set_as_windowless(Default::default());
    // The GPU path: frames as shared textures (on_accelerated_paint), not buffers.
    window_info.shared_texture_enabled = gpu_on() as _;
    let settings = BrowserSettings { windowless_frame_rate: 30, background_color: 0xffff_ffff, ..Default::default() };
    let b = browser_host_create_browser_sync(Some(&window_info), Some(&mut client), Some(&CefString::from(url)), Some(&settings), None, None);
    if b.is_none() { eprintln!("[native] panel {index}: CEF did not create the browser"); }
}

thread_local! {
    static DEFERRED: RefCell<Vec<Box<dyn FnOnce()>>> = RefCell::new(Vec::new());
}

/// Run a call into CEF that comes from a Slint callback. On macOS and Windows
/// it waits for the next pump (src/pump.rs), outside winit's event handling,
/// which CEF must not re-enter; on Linux it runs now.
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

/// Is the GPU path in use (src/gpu.rs)?
pub fn gpu_on() -> bool {
    #[cfg(feature = "gpu")]
    return crate::gpu::on();
    #[cfg(not(feature = "gpu"))]
    false
}

/// Ask every panel for a new frame (the GPU path, once Slint's device is ready).
#[cfg_attr(not(feature = "gpu"), allow(dead_code))]
pub fn invalidate_all() {
    let hosts: Vec<_> = with(|st| st.panels.iter().filter_map(|p| p.host()).collect());
    for h in hosts { h.invalidate(PaintElementType::VIEW); }
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
        let p = &mut st.panels[i];
        let changed = (p.stage_w - w).abs() > 0.5 || (p.stage_h - h).abs() > 0.5;
        p.stage_w = w; p.stage_h = h;
        changed && p.device == 0 && p.custom.is_none()
    });
    if follow { if let Some(h) = with(|st| st.panels[i].host()) { h.was_resized(); } }
    sync(i);
}

// ─── Input from Slint ───

const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;

fn button(b: i32) -> MouseButtonType {
    match b { 1 => MouseButtonType::MIDDLE, 2 => MouseButtonType::RIGHT, _ => MouseButtonType::LEFT }
}

/// A pointer event on panel [i]'s stage (kind 0 move, 1 down, 2 up).
pub fn pointer(i: usize, kind: i32, x: f32, y: f32, b: i32, annotating: bool) {
    if annotating {
        with(|st| {
            let p = &mut st.panels[i];
            let (vx, vy) = p.to_view(x, y);
            match kind {
                1 => { p.strokes.push(vec![(vx, vy)]); p.drawing = true; }
                0 if p.drawing => { if let Some(s) = p.strokes.last_mut() { s.push((vx, vy)); } }
                2 => { p.drawing = false; }
                _ => {}
            }
        });
        if kind != 0 || with(|st| st.panels[i].drawing) { sync(i); }
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

/// Slint's key text for named keys (private-use characters) to Windows key codes.
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

pub fn clear_marks(i: usize) {
    with(|st| { let p = &mut st.panels[i]; p.strokes.clear(); p.pin = None; });
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

/// The panel's current frame as a PNG (what the person sees, at device pixels).
pub fn screenshot(i: usize) -> Option<String> {
    let pb = with(|st| st.panels[i].frame.clone())?;
    png_base64(&pb)
}

/// The whole window as Slint draws it (panels, toolbar, annotations), as a
/// base64 PNG: a screenshot that needs nothing from the OS.
pub fn window_snapshot() -> Option<String> {
    use slint::ComponentHandle;
    let ui = with(|st| st.ui.upgrade())?;
    let pb = ui.window().take_snapshot().ok()?;
    png_base64(&pb)
}

fn png_base64(pb: &SharedPixelBuffer<Rgba8Pixel>) -> Option<String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, pb.width(), pb.height());
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().ok()?;
        w.write_image_data(pb.as_bytes()).ok()?;
    }
    Some(base64::engine::general_purpose::STANDARD.encode(out))
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
    with(|st| st.panels[i].pin = Some((x, y, label.to_string())));
    sync(i);
}

/// The marks on panel [i], in page coordinates (what a chat reads back).
pub fn marks(i: usize) -> Value {
    with(|st| {
        let p = &st.panels[i];
        let (w, h, ..) = p.view();
        json!({
            "viewport": { "width": w, "height": h },
            "strokes": p.strokes.iter().map(|s| s.iter().map(|&(x, y)| json!([x.round(), y.round()])).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "pin": p.pin.as_ref().map(|(x, y, l)| json!({ "x": x, "y": y, "label": l })),
        })
    })
}
