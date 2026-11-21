//! The pages: off-screen browsers, one per panel (a drawer tab's page, a
//! hidden browser, a widget panel), with input, device emulation, CDP, the
//! marks on a page, the console and the network log. The browsers
//! themselves live in the browser host (host/, its own process); this is
//! the app's side of each, with the same API the areas used when CEF ran
//! in the app: the state (`with`), what it tells the desktop area
//! (`set_hooks`), and the calls, now commands to the host (client.rs).
//!
//! Threading: everything here runs on the app's main thread; the host's
//! events come in through the pump (client.rs `pump` → `on_event`,
//! `on_frame`).
//!
//! Frames: the host's BGRA frame, copied out of the view's shared segment
//! straight into a GPUI image (GPUI's images take BGRA), which the panel
//! keeps as its newest frame and its view draws.
//!
//! Hidden panels: a panel no view shows (a tab behind another, the drawer
//! closed, a widget panel not drawn) is hidden in the host at once
//! (`set_shown`, `sync_hidden`; one never shown, as a hidden browser, after
//! HIDE_AFTER): Chromium stops drawing it and no frames come. Shown, it is
//! told at once. Its owner frees it after `unload_grace` (the drawer, the
//! panels). A screenshot (Page.captureScreenshot) wakes a hidden
//! panel for as long as it takes.

use crate::client::{self, Owner};
use crate::ipc::Cmd;
use base64::Engine;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap};

pub use crate::common::{cef_root, now_ms, profile_dir, Device, DEVICES};

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
    /// A console line: true when the area took it (the saved passwords'
    /// page script reports this way); taken lines are not kept, so a
    /// chat's browser_console never reads them.
    pub console: fn(usize, &str) -> bool,
    /// Reload a page whose browser host went away: true when the area made
    /// it again itself (a drawer tab); else the page is made again in place.
    pub reload: fn(usize) -> bool,
}

impl Default for Hooks {
    fn default() -> Self {
        Hooks { changed: |_| {}, visited: |_, _| {}, popup: |_, _| false, load_failed: |_| {}, annotating: || false, ask_note: |_, _| {}, console: |_, _| false, reload: |_| false }
    }
}

thread_local! { static HOOKS: RefCell<Hooks> = RefCell::new(Hooks::default()); }

/// Plug the desktop area in (once, at start).
pub fn set_hooks(h: Hooks) { HOOKS.with(|x| *x.borrow_mut() = h); }
fn hooks() -> Hooks { HOOKS.with(|x| *x.borrow()) }

type CdpReply = Box<dyn FnOnce(bool, Value)>;

#[derive(Default)]
pub struct Panel {
    /// The panel's view in the browser host (None: not made, closed, or
    /// lost when the host went away).
    pub browser: Option<u64>,
    /// The page crashed (its page process, or the whole host): the view
    /// shows "This page crashed" and Reload.
    pub crashed: bool,
    /// The panel's stage, in logical px (the view's size).
    pub stage_w: f32,
    pub stage_h: f32,
    pub device: usize,
    /// A size set by the server (browserResize), instead of a named device.
    pub custom: Option<(i32, i32)>,
    /// The newest frame (BGRA, device px), as the image a view draws.
    pub frame: Option<std::sync::Arc<gpui::RenderImage>>,
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
    /// How long the cursor glides there (ms; 0: it jumps), as the web's transition.
    pub cursor_ms: f32,
    /// The chat's presses so far: each new one rings once where the cursor is.
    pub ripples: u64,
    /// The last main-frame load failure ("Load failed (<code>): <text>").
    pub load_error: Option<String>,
    /// The chat's pointer (browser_mouse): where, and which button is held.
    pub mouse_at: Option<(f32, f32)>,
    pub mouse_down: Option<String>,
    pub touch_on: bool,
    mouse_held: Option<i32>,
    cdp_next: i32,
    cdp_waiters: HashMap<i32, CdpReply>,
    load_waiters: Vec<Box<dyn FnOnce()>>,
    last_ua: String,
    /// The size last told the host.
    sent: (i32, i32, f32),
    /// The page's mouse cursor (common::cursor's kinds).
    pub cursor: i32,
    /// A view shows this panel (PageView `set_panel`).
    pub shown: bool,
    /// Since when no view shows it.
    unshown_at: Option<std::time::Instant>,
    /// What the host was last told (None: nothing yet, so shown).
    pub hidden_sent: Option<bool>,
    /// Screenshots under way: the panel is woken for them.
    waking: u32,
    /// Hidden by its owner although a view holds it (a widget panel whose
    /// box is not drawn).
    pub parked: bool,
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
    /// The page's scale and offset in the stage (for what is drawn over it).
    pub fn fit_scale(&self) -> (f32, f32, f32) { self.fit() }
}

/// A mark on the page (crundi-browser-drawer.js: { id, by, kind, x, y, width, height, points, text, selector }).
#[derive(Clone, Default, Debug, PartialEq)]
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

/// A drawer page's size before the drawer was ever shown: the drawer's
/// default width, the window's height under the bars.
fn default_stage() -> (f32, f32) {
    use crundi_theme::T;
    (f32::from(T::DRAWER_W), f32::from(T::WINDOW_H - T::TOPBAR_H - T::TABBAR_H - T::DRAWER_BARS_H))
}

/// The panels changed since the last call (the views' pump).
pub fn take_changed() -> Vec<usize> { with(|st| std::mem::take(&mut st.changed).into_iter().collect()) }

pub fn status(text: &str) { with(|st| st.status = text.to_string()); }

/// The panel's view in the host, when it has one.
fn view_of(i: usize) -> Option<u64> { with(|st| st.panels.get(i).and_then(|p| p.browser)) }

/// A command for panel [i]'s view (dropped when it has none).
fn to_view(i: usize, cmd: impl FnOnce(u64) -> Cmd) { if let Some(id) = view_of(i) { client::send_to(id, cmd(id)); } }

// ─── What the host tells a panel ───

/// An event for panel [i] from its view [id] (an older view's is dropped).
pub(crate) fn on_event(i: usize, id: u64, ev: crate::ipc::Ev) {
    use crate::ipc::Ev;
    if view_of(i) != Some(id) { return; }
    match ev {
        Ev::Loading { loading, back, fwd, .. } => {
            with(|st| { let p = &mut st.panels[i]; p.loading = loading; p.can_back = back; p.can_fwd = fwd; if loading { p.crashed = false; } });
            sync(i);
            if loading { with(|st| st.panels[i].load_error = None); }
            else {
                let waiters = with(|st| std::mem::take(&mut st.panels[i].load_waiters));
                for w in waiters { w(); }
            }
        }
        Ev::LoadError { code, text, .. } => {
            with(|st| st.panels[i].load_error = Some(format!("Load failed ({code}): {text}")));
            (hooks().load_failed)(i);
        }
        Ev::Title { title, .. } => { with(|st| st.panels[i].title = title); sync(i); }
        Ev::Cursor { kind, .. } => {
            let changed = with(|st| { let p = &mut st.panels[i]; let ch = p.cursor != kind; p.cursor = kind; ch });
            if changed { sync(i); }
        }
        Ev::Console { level, text, source, line, .. } => {
            if (hooks().console)(i, &text) { return; }
            let m = json!({ "type": level, "text": text, "url": source, "line": line, "time": now_ms() });
            with(|st| { let c = &mut st.panels[i].console; c.push_back(m); while c.len() > MAX_CONSOLE { c.pop_front(); } });
        }
        Ev::Address { url, .. } => {
            let base = |x: &str| x.split('#').next().unwrap_or("").to_string();
            with(|st| { let p = &mut st.panels[i]; if base(&p.url) != base(&url) { p.ann.clear(); p.hover = None; } p.url = url.clone(); });
            sync(i);
            (hooks().visited)(i, &url);
        }
        // Links that open a window: the area's (a new tab), else the same panel.
        Ev::Popup { url, .. } => if !(hooks().popup)(i, &url) { client::send_to(id, Cmd::Navigate { id, url }); },
        Ev::Net { method, params, .. } => net_event(i, &method, &params),
        Ev::CdpResult { call, ok, result, .. } => {
            let w = with(|st| st.panels[i].cdp_waiters.remove(&call));
            if let Some(w) = w { w(ok, result); }
        }
        Ev::Gone { status, .. } => {
            eprintln!("[browser] panel {i}: its page process ended ({status})");
            with(|st| { let p = &mut st.panels[i]; p.crashed = true; p.loading = false; });
            fail_waiters(i, "This page crashed");
            sync(i);
        }
        _ => {}
    }
}

/// A frame for panel [i] from its view [id].
pub(crate) fn on_frame(i: usize, id: u64, img: std::sync::Arc<gpui::RenderImage>) {
    if view_of(i) != Some(id) { return; }
    with(|st| { let p = &mut st.panels[i]; p.frame = Some(img); p.frames += 1; });
    sync(i);
}

/// How long a panel never shown stays shown in the host (made before its
/// view picks it up).
const HIDE_AFTER: std::time::Duration = std::time::Duration::from_millis(500);

/// A view shows panel [i] (or stopped showing it). Either way the host
/// hears at once: a panel no longer shown is hidden now (a panel never
/// shown, made for a view that has not picked it up yet or for the server,
/// waits HIDE_AFTER first).
pub fn set_shown(i: usize, on: bool) {
    let ch = with(|st| st.panels.get_mut(i).map(|p| {
        let ch = p.shown != on;
        p.shown = on;
        p.unshown_at = if on { None } else { std::time::Instant::now().checked_sub(HIDE_AFTER) };
        ch
    }).unwrap_or(false));
    if ch { sync_hidden(); }
}

/// Hidden (or not) by its owner whatever its view: a widget panel whose box
/// is not drawn. At once.
pub fn set_parked(i: usize, on: bool) {
    let ch = with(|st| st.panels.get_mut(i).map(|p| { let ch = p.parked != on; p.parked = on; ch }).unwrap_or(false));
    if ch { sync_hidden(); }
}

/// Is panel [i] shown by a view?
pub fn is_shown(i: usize) -> bool { with(|st| st.panels.get(i).is_some_and(|p| p.shown)) }

/// Is panel [i] hidden in the host now?
pub fn is_hidden(i: usize) -> bool { with(|st| st.panels.get(i).is_some_and(|p| p.hidden_sent == Some(true))) }

/// When a panel no view shows falls due to be hidden (`next_due`).
pub(crate) fn next_due() -> Option<std::time::Instant> {
    with(|st| st.panels.iter().filter(|p| p.browser.is_some() && !p.closed && p.waking == 0 && !p.shown && !p.parked && p.hidden_sent != Some(true))
        .map(|p| p.unshown_at.map(|t| t + HIDE_AFTER).unwrap_or_else(std::time::Instant::now)).min())
}

/// Tell the host which panels are hidden (the pump, and at once on a show).
pub fn sync_hidden() {
    let now = std::time::Instant::now();
    let todo: Vec<(u64, bool)> = with(|st| st.panels.iter_mut().filter_map(|p| {
        let id = p.browser?;
        if p.closed { return None; }
        let want = p.waking == 0 && (p.parked || (!p.shown && p.unshown_at.get_or_insert(now).elapsed() >= HIDE_AFTER));
        // Never told: the host shows a new view, so only a hide is news.
        if p.hidden_sent == Some(want) || (p.hidden_sent.is_none() && !want) { return None; }
        p.hidden_sent = Some(want);
        Some((id, want))
    }).collect());
    for (id, on) in todo {
        // Logged: a page hidden in the host gets no frames but still its
        // title and address, the owner's 18 Nov "blank page, the tab named
        // right" (not reproduced here): this says whether that was why.
        eprintln!("[browser] view {id} {}", if on { "hidden (no frames until shown)" } else { "shown" });
        client::send_to(id, Cmd::Hidden { id, on });
    }
}

/// What is waiting on the page gets its answer now: a failure.
fn fail_waiters(i: usize, why: &str) {
    let (cdp, loads) = with(|st| {
        let p = &mut st.panels[i];
        p.load_error = Some(why.to_string());
        (std::mem::take(&mut p.cdp_waiters), std::mem::take(&mut p.load_waiters))
    });
    for (_, w) in cdp { w(false, json!({ "message": why })); }
    for w in loads { w(); }
}

/// The browser host went away (crashed, or was killed): the page shows
/// that it crashed, and Reload makes it again (`reload`).
pub(crate) fn host_gone(i: usize) {
    let had = with(|st| {
        let Some(p) = st.panels.get_mut(i) else { return false };
        if p.closed { return false; }
        p.browser = None; p.crashed = true; p.frame = None; p.hidden_sent = None; p.loading = false; p.can_back = false; p.can_fwd = false;
        p.hover = None; p.hover_busy = false; p.mouse_held = None; p.sent = (0, 0, 0.0); p.last_ua.clear();
        true
    });
    if !had { return; }
    fail_waiters(i, "The browser stopped");
    sync(i);
}

/// Has panel [i]'s page crashed?
pub fn crashed(i: usize) -> bool { with(|st| st.panels.get(i).is_some_and(|p| p.crashed && !p.closed)) }

/// Reload a crashed page: its page process again (the host is up), or,
/// when the host went away, the page made again (a drawer tab by the
/// desktop area, else in place), starting the host.
pub fn reload(i: usize) {
    let Some((alive, url, network, sized)) = with(|st| st.panels.get(i).filter(|p| !p.closed).map(|p| (p.browser.is_some(), p.url.clone(), p.network.clone(), p.device != 0 || p.custom.is_some()))) else { return };
    if alive {
        with(|st| { let p = &mut st.panels[i]; p.crashed = false; p.load_error = None; });
        to_view(i, |id| Cmd::History { id, h: 0 });
        sync(i);
        return;
    }
    if (hooks().reload)(i) { return; }
    let url = if url.is_empty() { "about:blank".to_string() } else { url };
    with(|st| { let p = &mut st.panels[i]; p.crashed = false; p.load_error = None; p.frames = 0; });
    create_on(i, &url, &network);
    if sized { crate::later(std::time::Duration::from_millis(300), move || apply_device(i)); }
    sync(i);
}

/// Make panel [index]'s browser, off-screen, at [url] (this PC's network).
pub fn create(index: usize, url: &str) {
    create_on(index, url, "client");
}

/// Make panel [index]'s browser on [network]'s profile (in the host,
/// started if it isn't).
pub fn create_on(index: usize, url: &str, network: &str) {
    let (w, h, dpr) = with(|st| { let (w, h, dpr, ..) = st.panels[index].view(); (w, h, dpr) });
    let proxy = crate::net::proxy_for(network);
    let (u, n) = (url.to_string(), network.to_string());
    let id = client::open_view(Owner::Panel(index), move |id| Cmd::Open { id, url: u, frame: false, network: n, proxy, origin: String::new(), w, h, dpr });
    with(|st| { let p = &mut st.panels[index]; p.browser = id; p.sent = (w, h, dpr); p.crashed = false; p.hidden_sent = None; });
    if id.is_none() { eprintln!("[native] panel {index}: no browser (the browser host can't start)"); }
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

/// Close panel [i]'s browser; its slot is free for the next one (and its
/// view and frame segment go in the host).
pub fn close_panel(i: usize) {
    let b = with(|st| {
        let p = st.panels.get_mut(i)?;
        if p.closed { return None; }
        p.closed = true; p.tab = None; p.key = None; p.ann.clear(); p.frame = None; p.title.clear(); p.url.clear(); p.crashed = false;
        let w: Vec<_> = std::mem::take(&mut p.load_waiters);
        let c: Vec<_> = std::mem::take(&mut p.cdp_waiters).into_values().collect();
        Some((p.browser.take(), w, c))
    });
    let Some((b, waiters, cdp)) = b else { return };
    for w in waiters { w(); }
    for w in cdp { w(false, json!({ "message": "The page was closed" })); }
    if let Some(id) = b { client::close_view(id); }
    with(|st| st.panels[i].network.clear());
    sync(i);
}

thread_local! {
    static DEFERRED: RefCell<Vec<Box<dyn FnOnce()>>> = RefCell::new(Vec::new());
}

/// Run a call that comes from a GPUI handler. On macOS and Windows it waits
/// for the next pump, outside the window's event handling (as when CEF ran
/// in the app, which it must not re-enter); on Linux it runs now.
pub fn defer(f: impl FnOnce() + 'static) {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        DEFERRED.with(|d| d.borrow_mut().push(Box::new(f)));
        crate::wake();
    } else {
        f();
    }
}

/// The deferred calls, in order (the pump, before the host's events).
pub fn run_deferred() {
    loop {
        let batch = DEFERRED.with(|d| std::mem::take(&mut *d.borrow_mut()));
        if batch.is_empty() { break; }
        for f in batch { f(); }
    }
}

// ─── CDP (in the host, no debugging port) ───

/// Send a CDP command to panel [i]'s browser; [reply] gets (ok, result).
pub fn cdp(i: usize, method: &str, params: Value, reply: CdpReply) {
    // A screenshot of a hidden panel: shown in the host until it is taken
    // (a hidden page draws nothing to capture).
    let wake = method == "Page.captureScreenshot";
    let reply: CdpReply = if wake {
        Box::new(move |ok, v| { with(|st| if let Some(p) = st.panels.get_mut(i) { p.waking = p.waking.saturating_sub(1); p.unshown_at = Some(std::time::Instant::now()); }); reply(ok, v) })
    } else { reply };
    let sent = with(|st| {
        let Some(p) = st.panels.get_mut(i) else { return Err(reply) };
        let Some(id) = p.browser else { return Err(reply) };
        p.cdp_next += 1;
        let call = p.cdp_next;
        p.cdp_waiters.insert(call, reply);
        let unhide = wake && p.hidden_sent == Some(true);
        if wake { p.waking += 1; }
        if unhide { p.hidden_sent = Some(false); }
        Ok((id, call, unhide))
    });
    match sent {
        Ok((id, call, unhide)) => {
            if unhide { client::send_to(id, Cmd::Hidden { id, on: false }); }
            client::send_to(id, Cmd::Cdp { id, call, method: method.to_string(), params })
        }
        Err(reply) => reply(false, json!({ "message": "no browser in this panel yet" })),
    }
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

/// Tell the host the panel's page size, when it changed (or `screen`: its
/// scale may have, always).
fn push_size(i: usize, screen: bool) {
    let (id, w, h, dpr, changed) = with(|st| {
        let p = &mut st.panels[i];
        let (w, h, dpr, ..) = p.view();
        let ch = p.sent != (w, h, dpr);
        p.sent = (w, h, dpr);
        (p.browser, w, h, dpr, ch)
    });
    if let Some(id) = id { if changed || screen { client::send_to(id, Cmd::Size { id, w, h, dpr, screen }); } }
}

/// Apply panel [i]'s device: the view size and scale factor go to the host
/// (its render handler's view_rect, screen_info); CDP adds mobile, touch and
/// the user agent. A changed user agent reloads the page, as a phone would get it.
pub fn apply_device(i: usize) {
    let (w, h, dpr, mobile, ua, alive, responsive) = with(|st| {
        let p = &st.panels[i];
        let (w, h, dpr, mobile, ua) = p.view();
        (w, h, dpr, mobile, ua, p.browser.is_some(), p.device == 0 && p.custom.is_none())
    });
    if !alive { return; }
    push_size(i, true);
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
        if changed { to_view(i, |id| Cmd::History { id, h: 0 }); }
    }));
    sync(i);
}

pub fn set_device(i: usize, d: usize) {
    // The last entry may be "Custom WxH" (the server's size): choosing it keeps that size.
    if d >= DEVICES.len() { return; }
    with(|st| { let p = &mut st.panels[i]; p.device = d; p.custom = None; });
    apply_device(i);
}

pub fn resized(i: usize, w: f32, h: f32) { resized_as(i, w, h, true) }

/// A panel that is not a drawer page (a widget panel's card or chip) took
/// a size: the drawer's stage size is left as it is.
pub fn resized_panel(i: usize, w: f32, h: f32) { resized_as(i, w, h, false) }

fn resized_as(i: usize, w: f32, h: f32, drawer: bool) {
    let follow = with(|st| {
        if drawer { st.stage = (w, h); }
        let p = &mut st.panels[i];
        let changed = (p.stage_w - w).abs() > 0.5 || (p.stage_h - h).abs() > 0.5;
        p.stage_w = w; p.stage_h = h;
        changed && p.device == 0 && p.custom.is_none()
    });
    if follow { push_size(i, false); }
    sync(i);
}

// ─── Input from the views ───

const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;

thread_local! {
    /// The annotation tool: "pick", "pin", "box", "pen" (the drawer's bar).
    pub static TOOL: RefCell<String> = RefCell::new("pick".into());
}

fn uid() -> String {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}{:04x}", n / 1_000_000, (n % 65536) as u32)
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
    let (id, vx, vy, held) = with(|st| { let p = &st.panels[i]; let (vx, vy) = p.to_view(x, y); (p.browser, vx, vy, p.mouse_held) });
    let Some(id) = id else { return };
    let (vx, vy) = (vx.round() as i32, vy.round() as i32);
    let modifiers = if held.is_some() { EVENTFLAG_LEFT_MOUSE_BUTTON } else { 0 };
    let m = |kind: i32, button: i32| Cmd::Mouse { id, kind, x: vx, y: vy, button, clicks: 1, modifiers };
    match kind {
        0 => client::send_to(id, m(0, 0)),
        1 => {
            client::send_to(id, Cmd::Focus { id, on: true });
            client::send_to(id, m(1, b));
            with(|st| st.panels[i].mouse_held = Some(b));
        }
        2 => { client::send_to(id, m(2, held.unwrap_or(b))); with(|st| st.panels[i].mouse_held = None); }
        _ => client::send_to(id, m(3, 0)),
    }
}

pub fn wheel(i: usize, x: f32, y: f32, dx: f32, dy: f32) {
    let (vx, vy) = with(|st| st.panels[i].to_view(x, y));
    to_view(i, |id| Cmd::Wheel { id, x: vx as i32, y: vy as i32, dx: dx as i32, dy: dy as i32 });
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
    let Some(id) = view_of(i) else { return };
    let Some(c) = text.chars().next() else { return };
    let modifiers = (if shift { 1 << 1 } else { 0 }) | (if ctrl { 1 << 2 } else { 0 });
    let code = named_key(c).unwrap_or_else(|| c.to_ascii_uppercase() as i32);
    if down {
        client::send_to(id, Cmd::Key { id, kind: 0, modifiers, code, ch: 0 });
        // Printable (and Enter): the character itself.
        if named_key(c).is_none() || c == '\n' || c == '\r' {
            let ch = if c == '\n' { '\r' } else { c };
            let mut buf = [0u16; 2];
            client::send_to(id, Cmd::Key { id, kind: 3, modifiers, code, ch: ch.encode_utf16(&mut buf)[0] });
        }
    } else {
        client::send_to(id, Cmd::Key { id, kind: 2, modifiers, code, ch: 0 });
    }
}

/// Text an input method committed, into the page.
pub fn ime_commit(i: usize, text: &str) { let t = text.to_string(); to_view(i, move |id| Cmd::Ime { id, text: t }); }

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
    let b = with(|st| { let p = &mut st.panels[i]; p.load_waiters.push(done); p.browser });
    match b {
        Some(id) => client::send_to(id, Cmd::Navigate { id, url: url.to_string() }),
        None => { let w = with(|st| std::mem::take(&mut st.panels[i].load_waiters)); for w in w { w(); } }
    }
}

/// The panel's current frame as a PNG (what the person sees, at device pixels), base64.
pub fn screenshot(i: usize) -> Option<String> {
    let f = with(|st| st.panels.get(i).and_then(|p| p.frame.clone()))?;
    let sz = f.size(0);
    png_base64(f.as_bytes(0)?, sz.width.0 as u32, sz.height.0 as u32)
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
pub fn history(i: usize, h: i32) { to_view(i, |id| Cmd::History { id, h }); }

/// Real mouse input at page (CSS px) coordinates, as browser_mouse does.
pub fn mouse(i: usize, x: f32, y: f32, action: &str) -> Result<(), String> {
    let id = view_of(i).ok_or("no browser in this panel yet")?;
    let m = |kind: i32| Cmd::Mouse { id, kind, x: x.round() as i32, y: y.round() as i32, button: 0, clicks: 1, modifiers: 0 };
    match action {
        "move" => client::send_to(id, m(0)),
        "down" => client::send_to(id, m(1)),
        "up" => client::send_to(id, m(2)),
        "click" | "" => { client::send_to(id, m(0)); client::send_to(id, m(1)); client::send_to(id, m(2)); }
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

/// DevTools for panel [i], in a window of its own (BD devtools: detached;
/// the host's window), or closed when open.
pub fn devtools(i: usize) { to_view(i, |id| Cmd::DevTools { id }); }

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
