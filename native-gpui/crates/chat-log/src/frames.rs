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
//! The views themselves are the CEF bridge's (crundi_webview::frames,
//! Desktop's): one off-screen view per live box, drawn as its picture in
//! the box; the pointer, the wheel and the keys go to it (a click gives it
//! the keyboard, a press outside takes it back). A stored page is fetched
//! once (/api/chat-media) and opened as a data: page. Without CEF (it did
//! not start), a box draws the spec's fallback: "Open" centred inside.
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
    /// Each box's keyboard (focused while its page has it).
    keys: HashMap<String, FocusHandle>,
    ticking: bool,
}
impl Global for Frames {}

/// While views are live: the logs holding them draw again when a view
/// paints (or its title or cursor change).
fn ticker(cx: &mut App) {
    if std::mem::replace(&mut g(cx).ticking, true) { return; }
    cx.spawn(async move |cx: &mut AsyncApp| {
        let mut last = 0u64;
        loop {
            cx.background_executor().timer(Duration::from_millis(33)).await;
            let go = cx.update(|cx| {
                let now = crundi_webview::frames::generation();
                let chats: Vec<String> = g(cx).boxes.values().filter(|s| s.live).map(|s| s.chat.clone()).collect();
                if chats.is_empty() { g(cx).ticking = false; return false; }
                if now != last {
                    last = now;
                    for c in chats { if let Some(v) = crate::view_of(cx, &c) { v.update(cx, |_, cx| cx.notify()); } }
                }
                true
            });
            if !go { break; }
        }
    }).detach();
}

fn g(cx: &mut App) -> &mut Frames { cx.default_global::<Frames>() }

/// The CEF bridge's per-frame views (crundi_webview::frames). A stored
/// page's HTML is fetched first; until it comes, `open` says no (the box
/// asks again on its next frame).
mod backend {
    use crundi_webview::frames as web;
    use gpui::App;
    use std::collections::HashMap;

    /// Stored pages: (chat, name) -> the HTML (None while it is being fetched).
    #[derive(Default)]
    pub struct Pages(pub HashMap<(String, String), Option<Vec<u8>>>);
    impl gpui::Global for Pages {}

    /// Make a view the box's size at `target` ("stored:<name>" or the page): false when it can't yet.
    pub fn open(cx: &mut App, chat: &str, key: &str, target: &str, w: f32, h: f32, scale: f32) -> bool {
        if !crundi_webview::started() { return false; }
        let url = match target.strip_prefix("stored:") {
            Some(name) => {
                let k = (chat.to_string(), name.to_string());
                match cx.default_global::<Pages>().0.get(&k) {
                    Some(Some(html)) => { use base64::Engine; format!("data:text/html;charset=utf-8;base64,{}", base64::engine::general_purpose::STANDARD.encode(html)) }
                    Some(None) => return false,
                    None => { fetch(cx, k); return false; }
                }
            }
            None if target.starts_with("https://") || web::loopback(target) => target.to_string(),
            None => return false,
        };
        web::open(key, &url, w, h, scale)
    }

    fn fetch(cx: &mut App, k: (String, String)) {
        let Some(api) = crundi_shell::api(cx) else { return };
        cx.default_global::<Pages>().0.insert(k.clone(), None);
        let path = format!("/api/chat-media?session={}&name={}", k.0, k.1);
        crundi_shell::run(cx, move || api.get_bytes(&path).ok().filter(|r| r.0 == 200).map(|r| r.1), move |html, cx| {
            // Failed or gone: forgotten, asked again the next time the box shows.
            match html { Some(h) => { cx.default_global::<Pages>().0.insert(k, Some(h)); } None => { cx.default_global::<Pages>().0.remove(&k); } }
        });
    }
    pub fn close(key: &str) { web::close(key) }
    pub fn painted(key: &str) -> bool { web::painted(key) }
    pub fn title(key: &str) -> String { web::title(key) }
    /// The last page a frame asked to open in a new window (the system browser).
    pub fn opened() -> String { web::opened() }
}

/// Which boxes should hold a view now, from what each reported (pure: the
/// rules). `want`: (key, on screen, distance, live now). Returns (make,
/// release): a box within RELEASE_PX without a view is made while there is
/// room under `cap`, the live one farthest from its view (never on screen)
/// making room; a box beyond RELEASE_PX is released.
#[cfg_attr(not(test), allow(dead_code))]
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
    let was_on = s.on_screen;
    s.on_screen = on_screen;
    s.dist = dist;
    s.at = Instant::now();
    s.target = target.into();
    if s.live { crundi_webview::frames::resize(key, s.w, s.h, scale); }
    // Within reach: one screen of the log either side (the GPUI log draws
    // every row, where the Slint client's list dropped rows off its view).
    let reach = f32::from(view.size.height).max(600.0);
    let (live, on) = (s.live, on_screen);
    if live && dist > reach { drop_view(key, cx); }
    else if live && was_on != on { crundi_webview::frames::hidden(key, !on); }
    else if dist <= reach && make_room(key, cx) {
        let Some(s) = g(cx).boxes.get(key).cloned() else { return };
        if backend::open(cx, &s.chat, key, &s.target, s.w, s.h, scale) { if let Some(s) = g(cx).boxes.get_mut(key) { s.live = true; } ticker(cx); }
    }
    sweep_soon(cx);
}

/// Room for one more view: under CAP, or the live view farthest from its
/// log's view released (never one on screen); as the Slint client's.
fn make_room(key: &str, cx: &mut App) -> bool {
    let live: Vec<(String, bool, f32)> = g(cx).boxes.iter().filter(|(k, s)| s.live && k.as_str() != key).map(|(k, s)| (k.clone(), s.on_screen, s.dist)).collect();
    if live.len() < CAP { return true; }
    match live.iter().filter(|l| !l.1).max_by(|a, b| a.2.total_cmp(&b.2)) { Some(v) => { let k = v.0.clone(); drop_view(&k, cx); true } None => false }
}

fn drop_view(key: &str, cx: &mut App) {
    let f = g(cx);
    if let Some(s) = f.boxes.get_mut(key) { if s.live { s.live = false; backend::close(key); } }
    if f.focused.as_deref() == Some(key) { f.focused = None; }
    f.keys.remove(key);
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

/// What a frame's box holds (inside its edge): the live view's picture,
/// taking the pointer, the wheel and (once clicked) the keys; without CEF,
/// the fallback's "Open". It reports where it is as it is drawn.
pub fn view(v: &mut LogView, key: &str, chat: &str, i: &media::Item, window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    use crundi_webview::frames as web;
    let target = key.split_once('|').map(|x| x.1.to_string()).unwrap_or_default();
    let scroll = v.scroll.clone();
    let (k, c) = (key.to_string(), chat.to_string());
    let report = canvas(move |b, window, cx| { seen(&k, &c, &target, b, scroll.bounds(), window.scale_factor(), cx); }, |_, _, _, _| {})
        .absolute().inset_0().size_full();
    if !crundi_webview::started() {
        let (c2, name, url) = (chat.to_string(), i.name.clone(), i.url.clone());
        return div().absolute().inset_0().flex().items_center().justify_center()
            .child(report)
            .child(w::button(SharedString::from(format!("fopen-{key}")), "Open").on_click(move |_, _, cx| crate::media::open_frame(&c2, &name, &url, cx)))
            .into_any_element();
    }
    let fh = match g(cx).keys.get(key).cloned() { Some(h) => h, None => { let h = cx.focus_handle(); g(cx).keys.insert(key.to_string(), h.clone()); h } };
    // The page lost the keyboard (a click elsewhere, another view took it).
    if !fh.is_focused(window) && web::focused().as_deref() == Some(key) { web::focus(key, false); }
    let origin = |cx: &mut App, k: &str| g(cx).boxes.get(k).map(|s| (s.x, s.y)).unwrap_or((0., 0.));
    let at = move |p: Point<Pixels>, o: (f32, f32)| (f32::from(p.x) - o.0, f32::from(p.y) - o.1);
    let (k1, k2, k3, k4, k5, k6) = (key.to_string(), key.to_string(), key.to_string(), key.to_string(), key.to_string(), key.to_string());
    let fh2 = fh.clone();
    div().id(SharedString::from(format!("frame-{key}"))).absolute().inset_0().overflow_hidden()
        .track_focus(&fh)
        .cursor(web::cursor(key))
        .child(report)
        .children(web::image(key).map(|img| gpui::img(ImageSource::Render(img)).absolute().inset_0().size_full()))
        .on_mouse_down(MouseButton::Left, move |ev, window, cx| {
            window.focus(&fh2, cx);
            let (x, y) = at(ev.position, origin(cx, &k1));
            web::pointer(&k1, 1, x, y, MouseButton::Left, ev.click_count);
            g(cx).focused = Some(k1.clone());
            cx.stop_propagation();
        })
        .on_mouse_up(MouseButton::Left, move |ev, _, cx| { let (x, y) = at(ev.position, origin(cx, &k2)); web::pointer(&k2, 2, x, y, MouseButton::Left, ev.click_count); cx.stop_propagation(); })
        .on_mouse_move(move |ev, _, cx| { let (x, y) = at(ev.position, origin(cx, &k3)); web::pointer(&k3, 0, x, y, MouseButton::Left, 1); })
        // A press anywhere else takes the keyboard back from the page.
        .on_mouse_down_out(move |_, _, cx| { if web::focused().as_deref() == Some(k4.as_str()) { web::focus(&k4, false); } if g(cx).focused.as_deref() == Some(k4.as_str()) { g(cx).focused = None; } })
        .on_scroll_wheel(move |ev, window, cx| {
            let d = ev.delta.pixel_delta(window.line_height());
            let (x, y) = at(ev.position, origin(cx, &k5));
            // The page takes the wheel while it can still scroll that way; else the log scrolls (F134).
            if web::wheel(&k5, x, y, f32::from(d.x), f32::from(d.y)) { cx.stop_propagation(); }
        })
        .on_key_down(move |ev, _, cx| { if web::focused().as_deref() == Some(k6.as_str()) { web::key(&k6, &ev.keystroke); cx.stop_propagation(); } })
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
        "key": k, "live": s.live, "painted": s.live && backend::painted(k), "title": backend::title(k), "focused": crundi_webview::frames::focused().as_deref() == Some(k.as_str()),
        "onScreen": s.on_screen, "hidden": !s.on_screen, "w": s.w.round(), "h": s.h.round(), "x": s.x, "y": s.y, "target": s.target,
        "scroll": crundi_webview::frames::scroll(k).map(|x| json!([x.0, x.1, x.2, x.3])).unwrap_or(Value::Null),
    })).collect();
    let live = crundi_webview::frames::live();
    top.insert("frames".into(), Value::Array(list));
    top.insert("framesLive".into(), json!(live));
    top.insert("frameOpened".into(), json!(backend::opened()));
    top.insert("frameFocused".into(), crundi_webview::frames::focused().map(Value::String).unwrap_or(Value::Null));
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
