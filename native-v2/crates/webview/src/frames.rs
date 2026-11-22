//! Live frames (2.16f, specs/log-rest.md "Live frames in detail"): a
//! show_embed item's page in its box, Cru's live screen, What's new's step
//! pages: each an off-screen view of its own in the browser host (host/;
//! `frame: true`). Ported from the Slint client's src/frames.rs, the app's
//! side only: the users decide when a view is made and released (the log
//! its cap), and call these.
//!
//! A view: transparent, 30 frames a second, no JS dialogs, no context menu;
//! new windows go to the system browser (recorded in `opened`, not opened
//! under CRUNDI_TEST_PORT); an https or loopback page's first request
//! carries the server's origin as referrer (`set_origin`). The wheel goes
//! to the page only while its root can still scroll that way (F134).
//!
//! The host's events come in on the main thread (client.rs's pump): the
//! state is a thread-local. Images replaced by newer frames wait in
//! `drop_old` for the window to free them. A view whose host went away (or
//! whose page process crashed) is `crashed`: `overlay` draws "This page
//! crashed" and Reload over it.

use crate::client::{self, Owner};
use crate::ipc::{Cmd, Ev};
use gpui::{CursorStyle, IntoElement, Keystroke, MouseButton, RenderImage};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

struct Fv {
    /// The page's messages to its host (MSG_PREFIX console lines), oldest first.
    messages: Vec<serde_json::Value>,
    /// Loads finished (the host sends what a page wants on load).
    loads: u64,
    /// The view in the host (None: lost when the host went away).
    browser: Option<u64>,
    crashed: bool,
    w: i32,
    h: i32,
    scale: f32,
    /// The newest frame (BGRA, made straight from the segment) and how
    /// many so far.
    frames: u64,
    image: Option<Arc<RenderImage>>,
    /// Off screen (`hidden`): the host sends no frames.
    hidden: bool,
    painted: bool,
    cursor: i32,
    title: String,
    focused: bool,
    held: Option<i32>,
    /// The page's root scroller: (top, most top, left, most left); None until read.
    scroll: Option<(f64, f64, f64, f64)>,
    cdp_next: i32,
    /// The page asked for, and whether its main load failed (a network
    /// error, or an HTTP status of 400 or more).
    url: String,
    failed: bool,
    /// The referrer its first request carries ("" for none).
    referrer: String,
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
/// there already is resized. The browser host starts if it isn't running.
/// False: no browser this run.
pub fn open(key: &str, target: &str, w: f32, h: f32, scale: f32) -> bool {
    if !crate::started() { return false; }
    if with(|s| s.frames.contains_key(key)) { resize(key, w, h, scale); return true; }
    let referrer = if target.starts_with("https://") || loopback(target) { with(|s| s.origin.clone()) } else { String::new() };
    let (w, h, scale) = (w.round().max(1.0) as i32, h.round().max(1.0) as i32, scale.max(1.0));
    with(|s| s.frames.insert(key.to_string(), Fv { messages: vec![], loads: 0, browser: None, crashed: false, w, h, scale, frames: 0, image: None, hidden: false, painted: false, cursor: 0, title: String::new(), focused: false, held: None, scroll: None, cdp_next: 0, url: target.to_string(), failed: false, referrer }));
    if !make(key) { with(|s| s.frames.remove(key)); return false; }
    changed();
    true
}

/// The view in the host for an entry that has none.
fn make(key: &str) -> bool {
    let Some((url, w, h, scale, referrer)) = with(|s| s.frames.get(key).map(|f| (f.url.clone(), f.w, f.h, f.scale, f.referrer.clone()))) else { return false };
    let id = client::open_view(Owner::Frame(key.to_string()), move |id| Cmd::Open { id, url, frame: true, network: String::new(), proxy: None, origin: referrer, w, h, dpr: scale });
    let Some(id) = id else { eprintln!("[gpui] frame {key}: no browser (the browser host can't start)"); return false };
    let hidden = with(|s| s.frames.get_mut(key).map(|f| { f.browser = Some(id); f.crashed = false; f.hidden }).unwrap_or(false));
    if hidden { client::send_to(id, Cmd::Hidden { id, on: true }); }
    true
}

/// Release the view (its segment goes on both sides).
pub fn close(key: &str) {
    let f = with(|s| s.frames.remove(key));
    let Some(f) = f else { return };
    if let Some(img) = f.image { with(|s| s.old.push(img)); }
    if let Some(id) = f.browser { client::close_view(id); }
    changed();
}

fn view(key: &str) -> Option<u64> { with(|s| s.frames.get(key).and_then(|f| f.browser)) }
fn send(key: &str, cmd: impl FnOnce(u64) -> Cmd) { if let Some(id) = view(key) { client::send_to(id, cmd(id)); } }

pub fn resize(key: &str, w: f32, h: f32, scale: f32) {
    let (w, h, scale) = (w.round().max(1.0) as i32, h.round().max(1.0) as i32, scale.max(1.0));
    let ch = with(|s| s.frames.get_mut(key).map(|f| { let c = f.w != w || f.h != h || (f.scale - scale).abs() > 0.01; f.w = w; f.h = h; f.scale = scale; c }).unwrap_or(false));
    if ch { send(key, |id| Cmd::Size { id, w, h, dpr: scale, screen: true }); }
}

/// A view off screen (scrolled away, its card closed): Chromium stops
/// drawing it and the host sends no frames; on screen again, a frame at once.
pub fn hidden(key: &str, on: bool) {
    let ch = with(|s| s.frames.get_mut(key).map(|f| { let ch = f.hidden != on; f.hidden = on; ch }).unwrap_or(false));
    if ch { send(key, |id| Cmd::Hidden { id, on }); }
}

/// Is the view hidden (the test driver)?
pub fn is_hidden(key: &str) -> bool { with(|s| s.frames.get(key).is_some_and(|f| f.hidden)) }

/// Frames the view has had (the test driver).
pub fn frame_count(key: &str) -> u64 { with(|s| s.frames.get(key).map(|f| f.frames).unwrap_or(0)) }

/// The view's newest picture.
pub fn image(key: &str) -> Option<Arc<RenderImage>> { with(|s| s.frames.get(key).and_then(|f| f.image.clone())) }

/// What a view's box draws, for a view's copy (rule 1: taken in its pump,
/// compared, drawn from the copy): frames so far, crashed, waiting for its
/// first frame, the pointer's shape. None: no such view.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Look { pub frames: u64, pub crashed: bool, pub waiting: bool, pub cursor: i32 }
pub fn look(key: &str) -> Option<Look> {
    with(|s| s.frames.get(key).map(|f| Look { frames: f.frames, crashed: f.crashed, waiting: f.browser.is_some() && !f.painted && !f.failed, cursor: f.cursor }))
}
pub fn cursor_of(kind: i32) -> CursorStyle { crate::view::cursor_style(kind) }

/// Free the pictures newer frames replaced (the window's pump).
pub fn drop_old(window: &mut gpui::Window) {
    for img in with(|s| std::mem::take(&mut s.old)) { let _ = window.drop_image(img); }
}

/// The page's root scroller: (top, most top, left, most left); None until read (added by Operations for the log's props, 2.16f).
pub fn scroll(key: &str) -> Option<(f64, f64, f64, f64)> { with(|s| s.frames.get(key).and_then(|f| f.scroll)) }
pub fn painted(key: &str) -> bool { with(|s| s.frames.get(key).is_some_and(|f| f.painted)) }
/// The page the view was opened at.
pub fn url(key: &str) -> String { with(|s| s.frames.get(key).map(|f| f.url.clone()).unwrap_or_default()) }
/// The main load failed: a network error, or an HTTP status of 400 or more.
pub fn failed(key: &str) -> bool { with(|s| s.frames.get(key).is_some_and(|f| f.failed)) }
/// The view's page crashed (its page process, or the browser host).
pub fn crashed(key: &str) -> bool { with(|s| s.frames.get(key).is_some_and(|f| f.crashed)) }
pub fn title(key: &str) -> String { with(|s| s.frames.get(key).map(|f| f.title.clone()).unwrap_or_default()) }
pub fn cursor(key: &str) -> CursorStyle { crate::view::cursor_style(with(|s| s.frames.get(key).map(|f| f.cursor).unwrap_or(0))) }
/// The last page a frame asked to open in a new window.
pub fn opened() -> String { with(|s| s.opened.clone()) }

/// Reload a crashed view: its page process again, or (the host went away)
/// the view made again, starting the host.
pub fn reload(key: &str) {
    let Some(alive) = with(|s| s.frames.get(key).map(|f| f.browser.is_some())) else { return };
    with(|s| if let Some(f) = s.frames.get_mut(key) { f.crashed = false; f.failed = false; f.painted = false; });
    if alive { send(key, |id| Cmd::History { id, h: 0 }); } else { make(key); }
    changed();
}

/// What goes over the view's picture: "This page crashed" with Reload, or,
/// until its first frame, a quiet spinner (None: nothing).
pub fn overlay(key: &str) -> Option<gpui::AnyElement> {
    let (crashed, waiting) = with(|s| s.frames.get(key).map(|f| (f.crashed, f.browser.is_some() && !f.painted && !f.failed)))?;
    if crashed { return Some(crash_overlay(key)); }
    waiting.then(|| crundi_widgets::page_loading().into_any_element())
}

/// "This page crashed" with Reload, for the view (when it has crashed).
pub fn crash_overlay(key: &str) -> gpui::AnyElement {
    let k = key.to_string();
    crundi_widgets::page_crashed(gpui::SharedString::from(format!("frame-reload-{key}")), move |_, _, _| reload(&k)).into_any_element()
}

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
    let code = format!("window.postMessage({}, '*')", m);
    send(key, |id| Cmd::Js { id, code });
}

/// Run a script in the frame's page (a watcher the host puts in a page it
/// did not make: a video's player).
pub fn run_js(key: &str, code: &str) {
    let code = code.to_string();
    send(key, |id| Cmd::Js { id, code });
}

/// The page's messages since the last call.
pub fn take_messages(key: &str) -> Vec<serde_json::Value> { with(|s| s.frames.get_mut(key).map(|f| std::mem::take(&mut f.messages)).unwrap_or_default()) }

/// How many loads the page finished.
pub fn loads(key: &str) -> u64 { with(|s| s.frames.get(key).map(|f| f.loads).unwrap_or(0)) }
/// Is there a view for this key?
pub fn exists(key: &str) -> bool { with(|s| s.frames.contains_key(key)) }

/// The views live now.
pub fn live() -> usize { with(|s| s.frames.values().filter(|f| f.browser.is_some()).count()) }

/// Ask the page where its root scroller is (after a load, a scroll).
fn ask_scroll(key: &str) {
    let call = with(|s| s.frames.get_mut(key).map(|f| { f.cdp_next += 1; f.cdp_next }));
    let Some(call) = call else { return };
    send(key, |id| Cmd::Cdp { id, call, method: "Runtime.evaluate".into(), params: serde_json::json!({
        "returnByValue": true,
        "expression": "(()=>{const e=document.scrollingElement||document.documentElement;return [e.scrollTop,e.scrollHeight-innerHeight,e.scrollLeft,e.scrollWidth-innerWidth]})()"
    }) });
}

const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;

/// The pointer in the view (x, y in its logical px): 0 move, 1 down, 2 up,
/// 3 leave; `clicks`: 2 a double click, 3 a triple.
pub fn pointer(key: &str, kind: i32, x: f32, y: f32, button: MouseButton, clicks: usize) {
    let Some(id) = view(key) else { return };
    let held = with(|s| s.frames.get(key).and_then(|f| f.held));
    let btn = match button { MouseButton::Middle => 1, MouseButton::Right => 2, _ => 0 };
    let modifiers = if held.is_some() { EVENTFLAG_LEFT_MOUSE_BUTTON } else { 0 };
    let n = clicks.clamp(1, 3) as i32;
    let m = |kind: i32, button: i32| Cmd::Mouse { id, kind, x: x.round() as i32, y: y.round() as i32, button, clicks: n, modifiers };
    match kind {
        0 => client::send_to(id, m(0, 0)),
        1 => { focus(key, true); with(|s| if let Some(f) = s.frames.get_mut(key) { f.held = Some(btn); }); client::send_to(id, m(1, btn)); }
        2 => { with(|s| if let Some(f) = s.frames.get_mut(key) { f.held = None; }); client::send_to(id, m(2, held.unwrap_or(btn))); }
        _ => client::send_to(id, m(3, 0)),
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
    if takes { send(key, |id| Cmd::Wheel { id, x: x as i32, y: y as i32, dx: dx as i32, dy: dy as i32 }); }
    takes
}

/// The frame takes (or gives back) the keyboard; one at a time.
pub fn focus(key: &str, on: bool) {
    let others: Vec<String> = if on { with(|s| s.frames.iter().filter(|(k, f)| *k != key && f.focused).map(|(k, _)| k.clone()).collect()) } else { vec![] };
    for k in others { focus(&k, false); }
    let ch = with(|s| s.frames.get_mut(key).map(|f| { let c = f.focused != on; f.focused = on; c }).unwrap_or(false));
    send(key, |id| Cmd::Focus { id, on });
    if ch { changed(); }
}

pub fn focused() -> Option<String> { with(|s| s.frames.iter().find(|(_, f)| f.focused && f.browser.is_some()).map(|(k, _)| k.clone())) }

/// A key for the frame: pressed and released.
pub fn key(key: &str, ks: &Keystroke) {
    let Some(id) = view(key) else { return };
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
    client::send_to(id, Cmd::Key { id, kind: 0, modifiers, code, ch: 0 });
    let typed = if named == Some(13) { Some('\r') } else if named.is_none() && !m.control && !m.platform { ch } else { None };
    if let Some(c) = typed { let mut buf = [0u16; 2]; client::send_to(id, Cmd::Key { id, kind: 3, modifiers, code, ch: c.encode_utf16(&mut buf)[0] }); }
    client::send_to(id, Cmd::Key { id, kind: 2, modifiers, code, ch: 0 });
}

// ─── What the host tells a frame ───

pub(crate) fn on_frame(key: &str, id: u64, img: Arc<RenderImage>) {
    let took = with(|s| {
        let f = s.frames.get_mut(key).filter(|f| f.browser == Some(id))?;
        let old = f.image.replace(img);
        f.frames += 1;
        f.painted = true;
        if let Some(o) = old { s.old.push(o); }
        Some(())
    }).is_some();
    if took { changed(); }
}

pub(crate) fn on_event(key: &str, id: u64, ev: Ev) {
    if view(key) != Some(id) { return; }
    let set = |f: &mut dyn FnMut(&mut Fv)| with(|s| if let Some(x) = s.frames.get_mut(key) { f(x); });
    match ev {
        Ev::Console { text, .. } => {
            if std::env::var_os("CRUNDI_DEBUG_FRAMES").is_some() { eprintln!("[frame console] {text:?}"); }
            let Some(rest) = text.strip_prefix(MSG_PREFIX) else { return };
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) { set(&mut |f| f.messages.push(v.clone())); changed(); }
        }
        Ev::Title { title, .. } => set(&mut |f| f.title = title.clone()),
        Ev::Cursor { kind, .. } => {
            let ch = with(|s| s.frames.get_mut(key).map(|f| { let ch = f.cursor != kind; f.cursor = kind; ch }).unwrap_or(false));
            if ch { changed(); }
        }
        Ev::Loading { loading, .. } => if !loading { set(&mut |f| f.loads += 1); changed(); ask_scroll(key); },
        Ev::LoadEnd { status, .. } => if status >= 400 { set(&mut |f| f.failed = true); changed(); },
        Ev::LoadError { .. } => { set(&mut |f| f.failed = true); changed(); }
        Ev::Scrolled { .. } => ask_scroll(key),
        Ev::CdpResult { ok, result, .. } => {
            if !ok { return; }
            let Some(a) = result.pointer("/result/value").and_then(|x| x.as_array()) else { return };
            let n = |i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0);
            let sc = (n(0), n(1).max(0.0), n(2), n(3).max(0.0));
            set(&mut |f| f.scroll = Some(sc));
        }
        // New windows (window.open, target=_blank): the system browser.
        Ev::Popup { url, .. } => if url.starts_with("http://") || url.starts_with("https://") {
            with(|s| s.opened = url.clone());
            if std::env::var_os("CRUNDI_TEST_PORT").is_none() { open_system(&url); }
        },
        Ev::Gone { status, .. } => {
            eprintln!("[gpui] frame {key}: its page process ended ({status})");
            set(&mut |f| f.crashed = true);
            changed();
        }
        _ => {}
    }
}

/// The browser host went away: the view shows that it crashed.
pub(crate) fn host_gone(key: &str) {
    let img = with(|s| s.frames.get_mut(key).map(|f| { f.browser = None; f.crashed = true; f.focused = false; f.held = None; f.image.take() }));
    if let Some(Some(img)) = img { with(|s| s.old.push(img)); }
    changed();
}

/// A page in the system's browser.
fn open_system(u: &str) {
    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "windows") { ("cmd", vec!["/C", "start", "", u]) } else if cfg!(target_os = "macos") { ("open", vec![u]) } else { ("xdg-open", vec![u]) };
    let _ = std::process::Command::new(cmd).args(args).spawn();
}
