//! This area's browser lent to the others, through the hooks they set out
//! for it: Operations' Panels and What's new (`crundi_operations::web::Host`:
//! engine panels drawn by a `PageView`, and frames drawn as a picture), and
//! the chat's live frames (`crundi_chat::log::frames::FrameHost`: a show_embed
//! item's page live in its box).
//!
//! **How it is built (ARCHITECTURE.md rules 1-3, 5).** Each page is drawn by
//! a cached view of its own: Panels' by a `PageView` (the drawer's), a chat
//! frame by a `LiveFrame`. Each takes its copy (the newest picture, its look)
//! in the area's pump and notifies itself only when it moved, so a page's
//! new frame redraws that view alone, never the chat or the panel around it
//! (their caches are replayed). The picture is the image the link's reader
//! made from the host's shared segment: no copy on this side.
//!
//! A chat keeps at most `MAX_LIVE` live frames: the one asked for least
//! lately is let go when another comes (it is made again when its box is
//! drawn again).

use crundi_operations::web as ops;
use crundi_webview::{engine, frames};
use gpui::{prelude::FluentBuilder as _, *};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc, time::{Duration, Instant}};

/// How many chat frames are live at once.
const MAX_LIVE: usize = 4;

thread_local! {
    /// Panels' page views (pumped with the drawer's).
    static PAGES: RefCell<Vec<WeakEntity<crundi_webview::PageView>>> = const { RefCell::new(Vec::new()) };
    /// The chat's live frames by key, and when each was last asked for.
    static FRAMES: RefCell<HashMap<String, (Entity<LiveFrame>, Instant)>> = RefCell::new(HashMap::new());
}

/// Set the hooks (the area's init).
pub fn init(cx: &mut App) {
    ops::set_host(cx, Rc::new(OpsHost));
    crundi_chat::log::frames::set_host(cx, Rc::new(ChatHost));
    // Workspace's pages: a PDF or AVIF in a file window, a PDF, video or audio in the media lightbox (F110).
    crundi_workspace::frames::set_host(cx, Rc::new(ChatHost));
}

/// The pump: each page view and live frame takes its copy (a notify only when it moved).
pub fn pump(window: &mut Window, cx: &mut App) {
    let pages: Vec<_> = PAGES.with(|p| { let mut p = p.borrow_mut(); p.retain(|w| w.upgrade().is_some()); p.iter().filter_map(|w| w.upgrade()).collect() });
    for p in pages { p.update(cx, |p, cx| p.pump(window, cx)); }
    let live: Vec<_> = FRAMES.with(|f| f.borrow().values().map(|(e, _)| e.clone()).collect());
    for f in live { f.update(cx, |f, cx| f.pump(cx)); }
    close_left(cx);
    unload_far();
}

/// T10: a chat's embed pages go with the chat. Its pane closed, or another
/// project shown (the workbench's panes are the project's), and the page
/// drawn nowhere: the page is closed in the host (its sound with it). A
/// chat still open keeps its pages, drawn or not (scrolled away, its row
/// dropped by the list): the same page comes back with the row, playing on.
fn close_left(cx: &mut App) {
    let chats: Vec<String> = FRAMES.with(|f| f.borrow().keys().filter(|k| k.starts_with("chat:")).cloned().collect());
    if chats.is_empty() { return; }
    let Some(wb) = crundi_workbench::workbench(cx) else { return };
    let panes = wb.update(cx, |w, _| w.keys());
    for k in chats {
        let chat = k["chat:".len()..].split('|').next().unwrap_or_default();
        if panes.iter().any(|p| p.strip_prefix("live:") == Some(chat)) || drawn(&k) { continue; }
        FRAMES.with(|f| f.borrow_mut().remove(&k));
        SEEN.with(|s| s.borrow_mut().remove(&k));
        FAR.with(|f| f.borrow_mut().remove(&k));
        frames::close(&k);
    }
}

thread_local! {
    /// Whether each live frame was in the window's last frame: the `Seen`
    /// element's token lives in the frame's element state, dropped with it
    /// when the frame is no longer drawn (a cached view reused keeps it).
    static SEEN: RefCell<HashMap<String, (std::rc::Weak<()>, bool)>> = RefCell::new(HashMap::new());
    /// Since when each loaded chat embed has been far off (T10).
    static FAR: RefCell<HashMap<String, (Instant, Instant)>> = RefCell::new(HashMap::new());
}

fn drawn(key: &str) -> bool { SEEN.with(|s| s.borrow().get(key).is_some_and(|w| w.0.strong_count() > 0)) }
/// Drawn, and within two viewport heights of the view (T10).
fn near_now(key: &str) -> bool { SEEN.with(|s| s.borrow().get(key).is_some_and(|w| w.0.strong_count() > 0 && w.1)) }

/// How far off an embed may be and stay loaded: this many heights of the
/// list's visible area above or below it.
const NEAR_VIEWPORTS: f32 = 2.0;
/// A far-off embed that is playing keeps running this long (T10, the owner's rule).
const PLAYING_GRACE: Duration = Duration::from_secs(5 * 60);
fn playing_grace() -> Duration {
    // The test driver's override (CRUNDI_TEST_PORT runs only).
    if std::env::var_os("CRUNDI_TEST_PORT").is_some() { if let Some(s) = std::env::var("CRUNDI_TEST_EMBED_GRACE_SECS").ok().and_then(|v| v.parse::<f64>().ok()) { return Duration::from_secs_f64(s); } }
    PLAYING_GRACE
}

/// Within NEAR_VIEWPORTS of the clip it is drawn in.
fn near(bounds: Bounds<Pixels>, window: &Window) -> bool {
    let m = window.content_mask().bounds;
    let reach = m.size.height * NEAR_VIEWPORTS;
    bounds.bottom() >= m.top() - reach && bounds.top() <= m.bottom() + reach
}

/// T10: a loaded chat embed far off (out of the near zone, or its row
/// dropped) is unloaded when it plays nothing, or after PLAYING_GRACE when
/// it plays; back near before then, it runs on. Unloaded, its box keeps its
/// size and the page loads again when it comes near.
fn unload_far() {
    let keys: Vec<String> = FRAMES.with(|f| f.borrow().keys().filter(|k| k.starts_with("chat:")).cloned().collect());
    let now = Instant::now();
    let mut waiting = false;
    for k in keys {
        if near_now(&k) || !frames::exists(&k) { FAR.with(|f| f.borrow_mut().remove(&k)); continue; }
        let (since, asked) = FAR.with(|f| *f.borrow_mut().entry(k.clone()).or_insert((now, now - Duration::from_secs(10))));
        let playing = frames::playing(&k);
        let unload = match playing { Some(false) => asked > since, Some(true) => now.duration_since(since) >= playing_grace(), None => false };
        if unload {
            FAR.with(|f| f.borrow_mut().remove(&k));
            frames::close(&k);
            continue;
        }
        if now.duration_since(asked) >= Duration::from_millis(900) {
            frames::ask_media(&k);
            FAR.with(|f| if let Some(e) = f.borrow_mut().get_mut(&k) { e.1 = now; });
        }
        waiting = true;
    }
    if waiting && !CHECK.with(|c| c.replace(true)) {
        crundi_webview::later(Duration::from_secs(1), || CHECK.with(|c| c.set(false)));
    }
}

thread_local! { static CHECK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

/// No space, nothing painted: it keeps the frame's token while drawn.
struct Seen(String);
impl IntoElement for Seen { type Element = Self; fn into_element(self) -> Self { self } }
impl Element for Seen {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut st = Style::default();
        st.position = Position::Absolute;
        st.inset.top = px(0.).into();
        st.inset.left = px(0.).into();
        st.size.width = relative(1.).into();
        st.size.height = relative(1.).into();
        (window.request_layout(st, None, cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, _: &mut App) {
        let key = self.0.clone();
        let nb = near(bounds, window);
        window.with_global_id(ElementId::Name("frame-seen".into()), |gid, window| {
            window.with_element_state::<Rc<()>, _>(gid, |st, _| {
                let t = st.unwrap_or_else(|| Rc::new(()));
                SEEN.with(|s| s.borrow_mut().insert(key, (Rc::downgrade(&t), nb)));
                ((), t)
            })
        });
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), _: &mut Window, _: &mut App) {}
}

// ─── Operations: Panels and What's new ───

struct OpsHost;

impl ops::Host for OpsHost {
    fn started(&self) -> bool { crundi_webview::started() }
    fn unload_grace(&self) -> Duration { crundi_webview::unload_grace() }
    fn panel_open(&self, url: &str) -> usize { engine::open_panel(url, "client") }
    fn panel_navigate(&self, i: usize, url: &str) { engine::navigate(i, url, Box::new(|| {})); }
    fn panel_close(&self, i: usize) { engine::close_panel(i); }
    fn panel_resized(&self, i: usize, w: f32, h: f32) { engine::resized_panel(i, w, h); }
    fn panel_parked(&self, i: usize, parked: bool) { engine::set_parked(i, parked); }
    fn panel_state(&self, i: usize) -> ops::PanelState {
        engine::with(|st| st.panels.get(i).filter(|p| !p.closed).map(|p| ops::PanelState { live: p.browser.is_some(), painted: p.frames > 0, title: p.title.clone() })).unwrap_or_default()
    }
    fn page_view(&self, cx: &mut App) -> ops::PageView {
        let view = cx.new(crundi_webview::PageView::new);
        PAGES.with(|p| p.borrow_mut().push(view.downgrade()));
        let weak = view.downgrade();
        let set_panel: Rc<dyn Fn(Option<usize>, &mut App)> = Rc::new(move |i, cx| { let _ = weak.update(cx, |p, cx| p.set_panel(i, cx)); crundi_webview::wake(); });
        ops::PageView { view: view.into(), set_panel }
    }
    fn frame_open(&self, key: &str, url: &str, w: f32, h: f32, scale: f32) -> bool { frames::open(key, url, w, h, scale) }
    fn frame_resize(&self, key: &str, w: f32, h: f32, scale: f32) { frames::resize(key, w, h, scale); }
    fn frame_close(&self, key: &str) { frames::close(key); }
    fn frame_exists(&self, key: &str) -> bool { frames::exists(key) }
    fn frame_generation(&self) -> u64 { frames::generation() }
    fn frame_failed(&self, key: &str) -> bool { frames::failed(key) }
    fn frame_painted(&self, key: &str) -> bool { frames::painted(key) }
    fn frame_crashed(&self, key: &str) -> bool { frames::crashed(key) }
    fn frame_image(&self, key: &str) -> Option<Arc<RenderImage>> { frames::image(key) }
    fn frame_url(&self, key: &str) -> String { frames::url(key) }
    fn frame_title(&self, key: &str) -> String { frames::title(key) }
    fn frame_loads(&self, key: &str) -> u64 { frames::loads(key) }
    fn crash_overlay(&self, key: &str) -> AnyElement { frames::crash_overlay(key) }
}

// ─── The chat's live frames ───

struct ChatHost;

impl crundi_chat::log::frames::FrameHost for ChatHost {
    fn started(&self) -> bool { crundi_webview::started() }
    fn frame(&self, key: &str, url: &str, _window: &mut Window, cx: &mut App) -> AnyElement { live_frame(format!("chat:{key}"), url, cx) }
}

/// Workspace's pages are drawn as the chat's frames are (their own keys).
impl crundi_workspace::frames::PageHost for ChatHost {
    fn started(&self) -> bool { crundi_webview::started() }
    fn frame(&self, key: &str, url: &str, _window: &mut Window, cx: &mut App) -> AnyElement { live_frame(format!("ws:{key}"), url, cx) }
}

/// A page live in its box, its view kept by key (at most MAX_LIVE at once).
fn live_frame(key: String, url: &str, cx: &mut App) -> AnyElement {
    {
        let have = FRAMES.with(|f| f.borrow_mut().get_mut(&key).map(|(e, at)| { *at = Instant::now(); e.clone() }));
        let e = match have {
            Some(e) => e,
            None => {
                // At most MAX_LIVE: the one asked for least lately goes.
                let old = FRAMES.with(|f| {
                    let f = f.borrow();
                    if f.len() < MAX_LIVE { return None; }
                    f.iter().min_by_key(|(_, (_, at))| *at).map(|(k, _)| k.clone())
                });
                if let Some(k) = old { FRAMES.with(|f| f.borrow_mut().remove(&k)); SEEN.with(|s| s.borrow_mut().remove(&k)); frames::close(&k); }
                let e = cx.new(|cx| LiveFrame::new(key.clone(), cx));
                FRAMES.with(|f| f.borrow_mut().insert(key.clone(), (e.clone(), Instant::now())));
                e
            }
        };
        let u = url.to_string();
        e.update(cx, |f, cx| if f.url != u { f.url = u; f.size = (0., 0.); cx.notify(); });
        e.cached(StyleRefinement::default().size_full()).into_any_element()
    }
}

/// One chat frame: the page made at its box's size and kept to it, its
/// newest picture, the pointer, wheel and keys passed to it (a click gives
/// it the keyboard). A wheel the page cannot take scrolls the chat.
pub struct LiveFrame {
    key: String,
    url: String,
    focus: FocusHandle,
    origin: Point<Pixels>,
    size: (f32, f32),
    look: Option<frames::Look>,
    image: Option<Arc<RenderImage>>,
}

impl LiveFrame {
    fn new(key: String, cx: &mut Context<Self>) -> Self {
        LiveFrame { key, url: String::new(), focus: cx.focus_handle(), origin: Point::default(), size: (0., 0.), look: None, image: None }
    }

    /// The page moved (painted, loaded, crashed): draw again.
    fn pump(&mut self, cx: &mut Context<Self>) {
        let look = frames::look(&self.key);
        if look == self.look { return; }
        if look.map(|l| l.frames) != self.look.map(|l| l.frames) { self.image = frames::image(&self.key); }
        if look.is_none() { self.size = (0., 0.); self.image = None; }
        self.look = look;
        cx.notify();
    }

    fn at(&self, p: Point<Pixels>) -> (f32, f32) { let q = p - self.origin; (f32::from(q.x), f32::from(q.y)) }
}

impl Render for LiveFrame {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("chat-frame");
        let me = cx.entity();
        let (key, url, look) = (self.key.clone(), self.url.clone(), self.look);
        let over = frames::overlay(&key);
        div().id(SharedString::from(format!("live-{key}"))).relative().size_full().overflow_hidden()
            .track_focus(&self.focus)
            .cursor(frames::cursor_of(look.map(|l| l.cursor).unwrap_or(0)))
            .when_some(self.image.clone(), |d, i| d.child(gpui::img(ImageSource::Render(i)).absolute().inset_0().size_full()))
            // The box's size: the page is made (or resized) to it as it is laid out.
            .child(canvas(move |bounds, window, cx| {
                let scale = window.scale_factor();
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let nb = near(bounds, window);
                me.update(cx, |this, _| {
                    this.origin = bounds.origin;
                    if url.is_empty() || w < 1. || h < 1. { return; }
                    // A chat embed far off is not loaded (T10): it loads as it comes near.
                    if this.key.starts_with("chat:") && !frames::exists(&this.key) && !nb { return; }
                    if !frames::exists(&this.key) || (this.size.0 - w).abs() > 0.5 || (this.size.1 - h).abs() > 0.5 {
                        this.size = (w, h);
                        frames::open(&this.key, &url, w, h, scale);
                        crundi_webview::wake();
                    }
                });
            }, |_, _, _, _| {}).absolute().top_0().left_0().size_full())
            .children(over)
            .child(Seen(key.clone()))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| { window.focus(&this.focus, cx); let (x, y) = this.at(e.position); frames::pointer(&this.key, 1, x, y, e.button, e.click_count); cx.stop_propagation(); }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, _, _| { let (x, y) = this.at(e.position); frames::pointer(&this.key, 2, x, y, e.button, e.click_count); }))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, _| { let (x, y) = this.at(e.position); frames::pointer(&this.key, 0, x, y, e.pressed_button.unwrap_or(MouseButton::Left), 1); }))
            .on_scroll(cx.listener(|this, e: &ScrollEvent, window, cx| {
                let (x, y) = this.at(e.position);
                let d = e.delta.pixel_delta(window.line_height());
                // The page takes the wheel while it can still scroll that way; else the chat does.
                if frames::wheel(&this.key, x, y, f32::from(d.x), f32::from(d.y)) { cx.stop_propagation(); }
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| { frames::key(&this.key, &e.keystroke); cx.stop_propagation(); }))
    }
}

/// The chat frames live now (the driver's).
pub fn props() -> serde_json::Value {
    FRAMES.with(|f| serde_json::Value::Array(f.borrow().keys().map(|k| serde_json::json!({ "key": k, "frames": frames::frame_count(k), "hidden": frames::is_hidden(k), "page": frames::page_id(k), "drawn": drawn(k), "near": near_now(k), "playing": frames::playing(k) })).collect()))
}
