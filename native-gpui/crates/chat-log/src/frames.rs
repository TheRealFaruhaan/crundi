//! 2.16f: live frames (a show_embed item's page in its box, PLAN.md 2.16f,
//! specs/log-rest.md "Live frames in detail"), as the Slint client's
//! src/frames.rs and LiveFrame (ui/chat.slint).
//!
//! The log's side is here: every frame box reports where it is each frame
//! (its place in the window, on the log's view or not, how far from it),
//! and the rules decide which boxes hold a live view: made within reach,
//! released beyond RELEASE_PX or when its box stops being drawn (scrolled
//! off, the pane closed, parked or hidden), at most CAP across the window
//! (the live one farthest from its log's view goes, never one on screen).
//!
//! The views themselves are the CEF bridge's (crundi-webview, Desktop's).
//! It has one browser today (the drawer's) and no per-frame views yet, so
//! until it does (shared/requests.md: `crundi_webview::frames`) a box
//! draws the spec's "CEF did not start" fallback: the box, its caption and
//! Open link under it, and "Open" centred inside (stored HTML opens in the
//! system browser through a temp file). `Backend` below is the one place
//! the views plug in.
use crate::LogView;
use crundi_core::media;
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct State {}

/// Live views at most, across the window (the web has no cap: PLAN.md).
pub const CAP: usize = 6;
/// A box further than this from its log's view releases its view.
pub const RELEASE_PX: f32 = 4500.0;
/// A box that has not reported for this long is off the log (not drawn).
const STALE: Duration = Duration::from_millis(900);

/// A frame box as last drawn.
#[derive(Clone, Debug)]
pub struct Seen {
    pub chat: String,
    /// "stored:<name>" (the chat's stored HTML), or the page's address.
    pub target: String,
    /// The view's place in the window and size (the box less its 1px edge).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub on_screen: bool,
    pub dist: f32,
    pub live: bool,
    at: Instant,
}

#[derive(Default)]
pub struct Frames {
    pub boxes: HashMap<String, Seen>,
    /// The frame that has the keyboard (a click in it; a click elsewhere takes it back).
    pub focused: Option<String>,
    sweeping: bool,
}
impl Global for Frames {}

fn g(cx: &mut App) -> &mut Frames { cx.default_global::<Frames>() }

/// The CEF bridge's per-frame views (none yet: see the module's note).
/// Each returns what "no view" means, so the rules and the props stay true.
mod backend {
    /// Make a view the box's size at `target` (a data: URL of stored HTML, or the page): false when it can't.
    pub fn open(_key: &str, _target: &str, _w: f32, _h: f32, _scale: f32) -> bool { false }
    pub fn close(_key: &str) {}
    pub fn painted(_key: &str) -> bool { false }
    pub fn title(_key: &str) -> String { String::new() }
    /// The last page a frame asked to open in a new window (the system browser).
    pub fn opened() -> String { String::new() }
}

/// Which boxes should hold a view now, from what each reported (pure: the
/// rules). `want`: (key, on screen, distance, live now). Returns (make,
/// release): a box within RELEASE_PX without a view is made while there is
/// room under `cap`, the live one farthest from its view (never on screen)
/// making room; a box beyond RELEASE_PX is released.
pub fn plan(want: &[(String, bool, f32, bool)], cap: usize) -> (Vec<String>, Vec<String>) {
    let mut release: Vec<String> = want.iter().filter(|w| w.3 && w.2 > RELEASE_PX).map(|w| w.0.clone()).collect();
    let mut live: Vec<(String, bool, f32)> = want.iter().filter(|w| w.3 && w.2 <= RELEASE_PX).map(|w| (w.0.clone(), w.1, w.2)).collect();
    let mut make = vec![];
    // Nearest first: on screen, then by distance.
    let mut wait: Vec<&(String, bool, f32, bool)> = want.iter().filter(|w| !w.3 && w.2 <= RELEASE_PX).collect();
    wait.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.total_cmp(&b.2)));
    for w in wait {
        if live.len() >= cap {
            let victim = live.iter().enumerate().filter(|(_, l)| !l.1 && l.2 > w.2).max_by(|a, b| a.1 .2.total_cmp(&b.1 .2)).map(|(i, _)| i);
            match victim { Some(i) => release.push(live.remove(i).0), None => continue }
        }
        live.push((w.0.clone(), w.1, w.2));
        make.push(w.0.clone());
    }
    (make, release)
}

/// A box reports itself (each frame it is drawn): where, on the log's view
/// or not, how far from it; then the rules run.
fn seen(key: &str, chat: &str, target: &str, b: Bounds<Pixels>, view: Bounds<Pixels>, scale: f32, cx: &mut App) {
    let (top, bot) = (f32::from(b.origin.y), f32::from(b.origin.y + b.size.height));
    let (vt, vb) = (f32::from(view.origin.y), f32::from(view.origin.y + view.size.height));
    let on_screen = view.size.height > px(0.) && bot > vt && top < vb;
    let dist = if on_screen { 0.0 } else if bot <= vt { vt - bot } else { top - vb };
    let f = g(cx);
    let s = f.boxes.entry(key.to_string()).or_insert_with(|| Seen { chat: chat.into(), target: target.into(), x: 0., y: 0., w: 0., h: 0., on_screen, dist, live: false, at: Instant::now() });
    // (The bounds are inside the box's 1px edge already.)
    s.x = f32::from(b.origin.x);
    s.y = top;
    s.w = f32::from(b.size.width).max(1.0);
    s.h = f32::from(b.size.height).max(1.0);
    s.on_screen = on_screen;
    s.dist = dist;
    s.at = Instant::now();
    s.target = target.into();
    let want: Vec<(String, bool, f32, bool)> = f.boxes.iter().filter(|(_, s)| s.at.elapsed() < STALE).map(|(k, s)| (k.clone(), s.on_screen, s.dist, s.live)).collect();
    let (make, release) = plan(&want, CAP);
    for k in release { drop_view(&k, cx); }
    for k in make {
        let Some(s) = g(cx).boxes.get(&k).cloned() else { continue };
        if backend::open(&k, &s.target, s.w, s.h, scale) { if let Some(s) = g(cx).boxes.get_mut(&k) { s.live = true; } }
    }
    sweep_soon(cx);
}

fn drop_view(key: &str, cx: &mut App) {
    let f = g(cx);
    if let Some(s) = f.boxes.get_mut(key) { if s.live { s.live = false; backend::close(key); } }
    if f.focused.as_deref() == Some(key) { f.focused = None; }
}

/// Every view whose box stopped reporting (scrolled off, the pane closed,
/// parked or hidden) is released; boxes are forgotten after a minute.
fn sweep_soon(cx: &mut App) {
    if std::mem::replace(&mut g(cx).sweeping, true) { return; }
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(Duration::from_millis(300)).await;
        cx.update(|cx| {
            let stale: Vec<String> = g(cx).boxes.iter().filter(|(_, s)| s.live && s.at.elapsed() > STALE).map(|(k, _)| k.clone()).collect();
            for k in stale { drop_view(&k, cx); }
            g(cx).boxes.retain(|_, s| s.at.elapsed() < Duration::from_secs(60));
        });
    }).detach();
}

/// Release a chat's views (its pane closed): its boxes stop reporting
/// anyway; this is at once.
pub fn release_chat(chat: &str, cx: &mut App) {
    let keys: Vec<String> = g(cx).boxes.iter().filter(|(_, s)| s.chat == chat).map(|(k, _)| k.clone()).collect();
    for k in keys { drop_view(&k, cx); }
}

/// What a frame's box holds (inside its edge): the live view when there is
/// one, else the fallback's "Open"; it reports where it is as it is drawn.
pub fn view(v: &mut LogView, key: &str, chat: &str, i: &media::Item, _window: &mut Window, _cx: &mut Context<LogView>) -> AnyElement {
    let target = key.split_once('|').map(|x| x.1.to_string()).unwrap_or_default();
    let scroll = v.scroll.clone();
    let (k, c) = (key.to_string(), chat.to_string());
    let report = canvas(move |b, window, cx| { seen(&k, &c, &target, b, scroll.bounds(), window.scale_factor(), cx); }, |_, _, _, _| {})
        .absolute().inset_0().size_full();
    let (c2, name, url) = (chat.to_string(), i.name.clone(), i.url.clone());
    div().absolute().inset_0().flex().items_center().justify_center()
        .child(report)
        .child(w::button(SharedString::from(format!("fopen-{key}")), "Open").on_click(move |_, _, cx| crate::media::open_frame(&c2, &name, &url, cx)))
        .into_any_element()
}

/// This feature's fields in the chat's props (none: the frames are window-wide).
pub fn props(_v: &LogView, _cx: &App, _o: &mut Map<String, Value>) {}

/// The window-wide fields (top level, the Slint client's names): frames
/// (every box known: key, live, painted, title, focused, onScreen, place),
/// framesLive (views held), frameOpened, frameFocused.
pub fn global_props(cx: &mut App, top: &mut Map<String, Value>) {
    let f = g(cx);
    let list: Vec<Value> = f.boxes.iter().map(|(k, s)| json!({
        "key": k, "live": s.live, "painted": s.live && backend::painted(k), "title": backend::title(k), "focused": f.focused.as_deref() == Some(k.as_str()),
        "onScreen": s.on_screen, "hidden": !s.on_screen, "w": s.w.round(), "h": s.h.round(), "x": s.x, "y": s.y, "target": s.target, "scroll": Value::Null,
    })).collect();
    let live = f.boxes.values().filter(|s| s.live).count();
    top.insert("frames".into(), Value::Array(list));
    top.insert("framesLive".into(), json!(live));
    top.insert("frameOpened".into(), json!(backend::opened()));
    top.insert("frameFocused".into(), f.focused.clone().map(Value::String).unwrap_or(Value::Null));
}

/// This feature's driver actions (none of its own: the frames take the
/// window's pointer and keys).
pub fn act(_name: &str, _args: &[String], _v: &mut LogView, _window: &mut Window, _cx: &mut Context<LogView>) -> Option<Value> { None }

#[cfg(test)]
mod tests {
    use super::*;

    fn w(k: &str, on: bool, d: f32, live: bool) -> (String, bool, f32, bool) { (k.into(), on, d, live) }

    #[::core::prelude::v1::test]
    fn made_within_reach_released_beyond() {
        let (make, rel) = plan(&[w("a", true, 0.0, false), w("b", false, 5000.0, true), w("c", false, 5000.0, false)], CAP);
        assert_eq!(make, ["a"]);
        assert_eq!(rel, ["b"]);
    }

    #[::core::prelude::v1::test]
    fn the_cap() {
        // Eight boxes on screen: six views, never one on screen released.
        let all: Vec<_> = (0..8).map(|i| w(&format!("f{i}"), true, 0.0, false)).collect();
        let (make, rel) = plan(&all, CAP);
        assert_eq!((make.len(), rel.len()), (6, 0));
        // Six live (one far off screen), a new one on screen: the far one goes.
        let mut six: Vec<_> = (0..5).map(|i| w(&format!("l{i}"), true, 0.0, true)).collect();
        six.push(w("far", false, 3000.0, true));
        six.push(w("new", true, 0.0, false));
        let (make, rel) = plan(&six, CAP);
        assert_eq!((make, rel), (vec!["new".to_string()], vec!["far".to_string()]));
    }
}
