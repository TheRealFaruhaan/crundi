//! The chat bubble (6.3; the change request's Desktop list): a chat in a
//! small window of its own that floats over every other window, as the
//! phone's bubble does (specs/desktop/inbox-tray-bubble.md C.1): collapsed,
//! a round head (an unread dot when a message comes in); expanded, the chat
//! itself, its log over its composer (the chat crate's own views, on the
//! chat's own store), with a head row (title, collapse, open in Crundi,
//! close). A press on the head without moving expands or collapses it; a
//! drag moves the window. One bubble at a time; opening another chat moves
//! the bubble to it. (native-gpui's bubble.rs, on v2's chat crate.)
//!
//! **Views (ARCHITECTURE.md rule 3).** `Bubble` is the window's layout view
//! (not cached, no text): the bar (`BubbleBar`) and the round head
//! (`BubbleKnob`) are cached leaves drawn from their own copies, and the chat
//! is `crundi_chat::pane::ChatPane::body`: its LogView and Composer placed as
//! cached leaves, as in a workbench pane. A streamed reply redraws the log
//! alone; the bar redraws only when the title or the state changed, the head
//! only when its dot did.

use crate::ctx;
use crundi_chat::{pane::ChatPane, store::{ChatMeta, ChatStore}};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::Root;
use serde_json::{json, Value};

/// The bar's copy: the chat's title and state, as its pane's head shows them.
#[derive(Clone, PartialEq, Default)]
struct Head { title: String, state: String }

/// The chat's title and state, as its pane's head shows them (the chat
/// crate's `head_info`, F65).
fn head_of(id: &str, meta: &crundi_chat::store::Meta, cx: &App) -> Head {
    let h = crundi_chat::head_info(&crundi_shell::stores(cx), id, meta, cx);
    Head { title: h.title, state: h.state }
}

/// A press on the head or the bar: without moving, expand or collapse; a
/// drag moves the window.
#[derive(Default)]
struct Press(Option<Point<Pixels>>);

impl Press {
    fn down(&mut self, e: &MouseDownEvent) { self.0 = Some(e.position); }
    /// The pointer moved with the button down: past a few px, the window moves.
    fn moved(&mut self, e: &MouseMoveEvent, window: &mut Window) {
        let Some(p) = self.0 else { return };
        if e.pressed_button != Some(MouseButton::Left) { self.0 = None; return; }
        let d = e.position - p;
        if d.x.abs() > T::SP_4 || d.y.abs() > T::SP_4 { self.0 = None; window.start_window_move(); }
    }
    fn up(&mut self) -> bool { self.0.take().is_some() }
}

pub struct Bubble {
    chat: String,
    store: Entity<ChatStore>,
    pane: ChatPane,
    bar: Entity<BubbleBar>,
    knob: Entity<BubbleKnob>,
    expanded: bool,
    seen: usize,
    _subs: Vec<Subscription>,
}

impl Bubble {
    fn new(chat: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Held: kept subscribed whatever project is open (F65), let go with the bubble.
        let store = crundi_chat::hold(cx, chat);
        let seen = store.read(cx).conv.entries.len();
        // Its own list over the chat's rows: the pane's scroll stays where it is.
        let pane = ChatPane::second(store.clone(), window, cx);
        let me = cx.weak_entity();
        let meta = store.read(cx).meta.clone();
        let head = head_of(chat, &meta.read(cx).0, cx);
        let bar = cx.new(|cx| BubbleBar::new(chat, me.clone(), meta, head, cx));
        let knob = cx.new(|_| BubbleKnob { unread: false, bubble: me, press: Press::default() });
        // A new message while collapsed: the head's dot (its copy alone moves).
        let subs = vec![cx.observe(&store, |this: &mut Bubble, s, cx| {
            let n = s.read(cx).conv.entries.len();
            if this.expanded { this.seen = n; } else if n > this.seen { this.knob.update(cx, |k, cx| if !k.unread { k.unread = true; cx.notify(); }); }
        }), cx.on_release(|this: &mut Bubble, cx| crundi_chat::release(cx, &this.chat))];
        Bubble { chat: chat.to_string(), store, pane, bar, knob, expanded: true, seen, _subs: subs }
    }

    /// Expand or collapse: the window takes the chat's size or the head's.
    pub fn set_expanded(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if on == self.expanded { return; }
        self.expanded = on;
        if on {
            self.seen = self.store.read(cx).conv.entries.len();
            self.knob.update(cx, |k, cx| if k.unread { k.unread = false; cx.notify(); });
        }
        window.resize(if on { size(T::BUBBLE_W, T::BUBBLE_H) } else { size(T::BUBBLE_HEAD, T::BUBBLE_HEAD) });
        cx.notify();
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) { let on = !self.expanded; self.set_expanded(on, window, cx); }
}

impl Render for Bubble {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("bubble");
        let root = div().size_full().text_color(T::text()).text_size(T::TEXT_13).font(crundi_theme::ui_font());
        if !self.expanded {
            return root.bg(T::accent()).rounded_full().child(self.knob.clone().cached(StyleRefinement::default().size_full())).into_any_element();
        }
        root.flex().flex_col().bg(T::bg()).border_1().border_color(T::border())
            .child(self.bar.clone().cached(StyleRefinement::default().w_full().h(T::PANE_HEAD).flex_none()))
            .child(div().flex_1().min_h_0().child(self.pane.body(cx)))
            .into_any_element()
    }
}

// ─── The bar: dot, title, collapse, open in Crundi, close ───

pub struct BubbleBar { chat: String, head: Head, bubble: WeakEntity<Bubble>, press: Press, _subs: Vec<Subscription> }

impl BubbleBar {
    fn new(chat: &str, bubble: WeakEntity<Bubble>, meta: Entity<ChatMeta>, head: Head, cx: &mut Context<Self>) -> Self {
        let projects = crundi_shell::stores(cx).projects.clone();
        let m2 = meta.clone();
        // The chat's meta (title, state) and the projects' list (a rename, an
        // exited chat): redrawn only when what the bar shows changed.
        let subs = vec![
            cx.observe(&meta, |this: &mut Self, m, cx| this.refresh(&m, cx)),
            cx.observe(&projects, move |this: &mut Self, _, cx| this.refresh(&m2, cx)),
        ];
        BubbleBar { chat: chat.to_string(), head, bubble, press: Press::default(), _subs: subs }
    }

    fn refresh(&mut self, meta: &Entity<ChatMeta>, cx: &mut Context<Self>) {
        let h = head_of(&self.chat, &meta.read(cx).0, cx);
        if h != self.head { self.head = h; cx.notify(); }
    }
}

impl Render for BubbleBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("bubble-bar");
        let chat = self.chat.clone();
        let (b1, b2) = (self.bubble.clone(), self.bubble.clone());
        w::layer(div().id("bubble-bar").flex().size_full().items_center().gap(T::SP_8).px(T::SP_10)
            .bg(T::bg_secondary()).border_b_1().border_color(T::border()).cursor_grab()
            .child(w::dot(w::state_color(&self.head.state)))
            .child(div().flex_1().min_w_0().truncate().font_weight(T::W_SEMIBOLD).child(self.head.title.clone()))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, _| this.press.down(e)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, _| this.press.moved(e, window)))
            .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| if this.press.up() { let _ = b1.update(cx, |b, cx| b.toggle(window, cx)); }))
            .child(w::icon_button("bubble-collapse", IconName::Minus, "Collapse to the bubble")
                .on_click(move |_, window, cx| { let _ = b2.update(cx, |b, cx| b.set_expanded(false, window, cx)); }))
            .child(w::icon_button("bubble-app", IconName::Maximize2, "Open in Crundi").on_click(move |_, _, _| { ctx::raise(); crate::go_to_chat(&chat); }))
            .child(w::icon_button("bubble-close", IconName::X, "Close the bubble").on_click(|_, window, _| { window.remove_window(); close_state(); })))
    }
}

// ─── The round head (collapsed), with its unread dot ───

pub struct BubbleKnob { unread: bool, bubble: WeakEntity<Bubble>, press: Press }

impl Render for BubbleKnob {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("bubble-head");
        let b = self.bubble.clone();
        w::layer(div().id("bubble-head").relative().size_full().rounded_full().bg(T::accent()).flex().items_center().justify_center()
            .text_color(T::white()).text_size(T::ICON_XL).cursor_pointer().child(IconName::MessageSquare)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, _| this.press.down(e)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, _| this.press.moved(e, window)))
            .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| if this.press.up() { let _ = b.update(cx, |b, cx| b.toggle(window, cx)); }))
            // The dot is drawn over the glyph: a layer of its own (nested layers draw over).
            .when(self.unread, |d| d.child(w::layer(div().absolute().top(T::SP_4).right(T::SP_4).size(T::DOT + T::SP_4).rounded_full().bg(T::red()).border_2().border_color(T::white())))))
    }
}

// ─── Opening, closing, the driver ───

/// The bubble's window and view (one at a time).
struct Open { window: AnyWindowHandle, view: Entity<Bubble> }
thread_local! { static OPEN: std::cell::RefCell<Option<Open>> = const { std::cell::RefCell::new(None) }; }

fn close_state() { OPEN.with(|o| *o.borrow_mut() = None); }

fn current() -> Option<(AnyWindowHandle, Entity<Bubble>)> { OPEN.with(|o| o.borrow().as_ref().map(|x| (x.window, x.view.clone()))) }

/// Open the bubble for a chat (or move the bubble to it), expanded.
pub fn open(chat: &str, cx: &mut App) {
    if chat.is_empty() { return ctx::toast_error("This app cannot open a bubble"); }
    if let Some((win, view)) = current() {
        if view.read(cx).chat == chat {
            let _ = win.update(cx, |_, window, cx| view.update(cx, |b, cx| b.set_expanded(true, window, cx)));
            return;
        }
        let _ = win.update(cx, |_, window, _| window.remove_window());
        close_state();
    }
    // Bottom right of the display (the expanded chat fits above and left of it).
    let size = size(T::BUBBLE_W, T::BUBBLE_H);
    let origin = match cx.primary_display() {
        Some(d) => { let b = d.bounds(); point(b.origin.x + b.size.width - size.width - T::SP_24, b.origin.y + b.size.height - size.height - T::SP_24) }
        None => point(T::WINDOW_W - size.width - T::SP_24, T::WINDOW_H - size.height - T::SP_24),
    };
    let opts = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds { origin, size })),
        titlebar: None,
        kind: WindowKind::PopUp,
        is_movable: true,
        focus: true,
        window_decorations: Some(WindowDecorations::Client),
        app_id: Some("crundi-bubble".into()),
        ..Default::default()
    };
    let id = chat.to_string();
    let mut made: Option<Entity<Bubble>> = None;
    let r = cx.open_window(opts, |window, cx| {
        let b = cx.new(|cx| Bubble::new(&id, window, cx));
        made = Some(b.clone());
        cx.new(|cx| Root::new(b, window, cx))
    });
    match (r, made) {
        (Ok(h), Some(view)) => OPEN.with(|o| *o.borrow_mut() = Some(Open { window: h.into(), view })),
        _ => ctx::toast_error("This app cannot open a bubble"),
    }
}

/// Expand or collapse the open bubble.
pub fn toggle(cx: &mut App) {
    let Some((win, view)) = current() else { return };
    let _ = win.update(cx, |_, window, cx| view.update(cx, |b, cx| b.toggle(window, cx)));
}

pub fn close(cx: &mut App) {
    let Some((win, _)) = current() else { return };
    let _ = win.update(cx, |_, window, _| window.remove_window());
    close_state();
}

/// Type and send in the bubble's composer (the test driver), as typed and sent.
pub fn send(text: &str, cx: &mut App) {
    let Some((win, view)) = current() else { return };
    let comp = view.read(cx).pane.composer.clone();
    let t = text.to_string();
    let _ = win.update(cx, |_, window, cx| comp.update(cx, |c, cx| { c.set_text(&t, window, cx); c.send(window, cx); }));
}

/// The whole screen with the bubble on it, as a PNG (X11: xwd), for the test driver.
pub fn snap(path: &str) -> bool {
    std::process::Command::new("sh").arg("-c").arg("xwd -silent -root | convert xwd:- \"png:$0\"").arg(path).status().is_ok_and(|s| s.success())
}

pub fn props(cx: &App) -> Value {
    let Some((_, view)) = current() else { return json!({ "open": false }) };
    let b = view.read(cx);
    let head = &b.bar.read(cx).head;
    json!({
        "open": true, "chat": b.chat, "expanded": b.expanded, "unread": b.knob.read(cx).unread,
        "title": head.title, "state": head.state, "entries": b.store.read(cx).conv.entries.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::Press;
    use gpui::{point, px};

    #[test]
    fn a_press_without_moving_toggles() {
        let mut p = Press::default();
        assert!(!p.up());
        p.0 = Some(point(px(10.), px(10.)));
        assert!(p.up());
        assert!(!p.up());
    }
}
