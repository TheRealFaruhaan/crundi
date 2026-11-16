//! The chat bubble (6.3; the change request's Desktop list): a chat in a
//! small window of its own that floats over every other window, as the
//! phone's bubble does (specs/desktop/inbox-tray-bubble.md C.1): collapsed,
//! a round head (an unread dot when a message comes in); expanded, the chat
//! itself, its log over its composer (the same views as a pane: chat-log,
//! chat-composer, on the chat's own model), with a head row (title, collapse,
//! open in Crundi, close). A press on the head without moving expands or
//! collapses it; a drag moves the window.
//!
//! One bubble at a time; opening another chat moves the bubble to it.

use crate::ctx;
use crundi_chat_composer::ComposerView;
use crundi_chat_log::LogView;
use crundi_chat_model::{self as model, ChatModel};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::Root;
use serde_json::{json, Value};

pub struct Bubble {
    chat: String,
    model: Entity<ChatModel>,
    log: Entity<LogView>,
    composer: Entity<ComposerView>,
    expanded: bool,
    unread: bool,
    seen: usize,
    press: Option<Point<Pixels>>,
    _sub: Subscription,
}

impl Bubble {
    fn new(chat: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let m = model::model(cx, chat);
        let seen = m.read(cx).conv.entries.len();
        let log = cx.new(|cx| LogView::new(m.clone(), cx));
        let composer = cx.new(|cx| ComposerView::new(m.clone(), window, cx));
        let sub = cx.observe(&m, |this: &mut Bubble, m, cx| {
            let n = m.read(cx).conv.entries.len();
            if n > this.seen && !this.expanded { this.unread = true; }
            if this.expanded { this.seen = n; }
            cx.notify();
        });
        Bubble { chat: chat.to_string(), model: m, log, composer, expanded: true, unread: false, seen, press: None, _sub: sub }
    }

    fn title(&self, cx: &App) -> (String, String) {
        let m = self.model.read(cx);
        (if m.conv.title.is_empty() { "Chat".into() } else { m.conv.title.clone() }, if m.conv.state.is_empty() { "idle".into() } else { m.conv.state.clone() })
    }

    /// Expand or collapse: the window takes the chat's size or the head's.
    pub fn set_expanded(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.expanded = on;
        if on { self.unread = false; self.seen = self.model.read(cx).conv.entries.len(); }
        window.resize(if on { size(T::BUBBLE_W, T::BUBBLE_H) } else { size(T::BUBBLE_HEAD, T::BUBBLE_HEAD) });
        cx.notify();
    }

    /// The head: a press without moving toggles; a drag moves the window.
    fn head_events(d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, _| this.press = Some(e.position)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, _| {
                let Some(p) = this.press else { return };
                if e.pressed_button != Some(MouseButton::Left) { this.press = None; return; }
                let d = e.position - p;
                if f32::from(d.x).abs() > f32::from(T::SP_4) || f32::from(d.y).abs() > f32::from(T::SP_4) { this.press = None; window.start_window_move(); }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, window, cx| {
                if this.press.take().is_some() { let on = !this.expanded; this.set_expanded(on, window, cx); }
            }))
    }
}

impl Render for Bubble {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, state) = self.title(cx);
        let root = div().size_full().bg(T::bg()).text_color(T::text()).text_size(T::TEXT_13).font(crundi_theme::ui_font());
        if !self.expanded {
            let head = div().id("bubble-head").size_full().rounded_full().bg(T::accent()).flex().items_center().justify_center()
                .text_color(T::white()).text_size(T::ICON_XL).cursor_pointer().child(IconName::MessageSquare)
                .when(self.unread, |d| d.child(div().absolute().top(T::SP_4).right(T::SP_4).size(T::DOT + T::SP_4).rounded_full().bg(T::red()).border_2().border_color(T::white())));
            return root.bg(T::accent()).rounded_full().relative().child(Self::head_events(head, cx)).into_any_element();
        }
        let chat = self.chat.clone();
        let head = div().id("bubble-bar").flex().flex_none().items_center().gap(T::SP_8).h(T::PANE_HEAD).px(T::SP_10)
            .bg(T::bg_secondary()).border_b_1().border_color(T::border()).cursor_grab()
            .child(w::dot(w::state_color(&state)))
            .child(div().flex_1().min_w_0().truncate().font_weight(T::W_SEMIBOLD).child(title));
        let head = Self::head_events(head, cx)
            .child(w::icon_button("bubble-collapse", IconName::Minus, "Collapse to the bubble").on_click(cx.listener(|this, _, window, cx| this.set_expanded(false, window, cx))))
            .child(w::icon_button("bubble-app", IconName::Maximize2, "Open in Crundi").on_click(move |_, _, _| { ctx::raise(); crate::go_to_chat(&chat); }))
            .child(w::icon_button("bubble-close", IconName::X, "Close the bubble").on_click(|_, window, _| { window.remove_window(); close_state(); }));
        root.flex().flex_col().border_1().border_color(T::border())
            .child(head)
            .child(div().flex().flex_col().flex_1().min_h_0().child(self.log.clone()).child(self.composer.clone()))
            .into_any_element()
    }
}

/// The bubble's window and view (one at a time).
struct Open { window: AnyWindowHandle, view: Entity<Bubble> }
thread_local! { static OPEN: std::cell::RefCell<Option<Open>> = const { std::cell::RefCell::new(None) }; }

fn close_state() { OPEN.with(|o| *o.borrow_mut() = None); }

/// Open the bubble for a chat (or move the bubble to it), expanded.
pub fn open(chat: &str, cx: &mut App) {
    if chat.is_empty() { return ctx::toast_error("This app cannot open a bubble"); }
    let cur = OPEN.with(|o| o.borrow().as_ref().map(|x| (x.window, x.view.clone())));
    if let Some((win, view)) = cur {
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
    let Some((win, view)) = OPEN.with(|o| o.borrow().as_ref().map(|x| (x.window, x.view.clone()))) else { return };
    let _ = win.update(cx, |_, window, cx| view.update(cx, |b, cx| { let on = !b.expanded; b.set_expanded(on, window, cx); }));
}

pub fn close(cx: &mut App) {
    let Some(win) = OPEN.with(|o| o.borrow().as_ref().map(|x| x.window)) else { return };
    let _ = win.update(cx, |_, window, _| window.remove_window());
    close_state();
}

/// Type and send in the bubble's composer (the test driver).
pub fn send(text: &str, cx: &mut App) {
    let Some((win, view)) = OPEN.with(|o| o.borrow().as_ref().map(|x| (x.window, x.view.clone()))) else { return };
    let comp = view.read(cx).composer.clone();
    let t = text.to_string();
    let _ = win.update(cx, |_, window, cx| comp.update(cx, |c, cx| { c.set_text(&t, window, cx); c.send(window, cx); }));
}

/// The whole screen with the bubble on it, as a PNG (X11: xwd), for the test driver.
pub fn snap(path: &str) -> bool {
    std::process::Command::new("sh").arg("-c").arg(format!("xwd -silent -root | convert xwd:- png:{path}")).status().is_ok_and(|s| s.success())
}

pub fn props(cx: &App) -> Value {
    OPEN.with(|o| match o.borrow().as_ref() {
        None => json!({ "open": false }),
        Some(x) => {
            let b = x.view.read(cx);
            let (title, state) = b.title(cx);
            json!({ "open": true, "chat": b.chat, "expanded": b.expanded, "unread": b.unread, "title": title, "state": state, "entries": b.model.read(cx).conv.entries.len() })
        }
    })
}
