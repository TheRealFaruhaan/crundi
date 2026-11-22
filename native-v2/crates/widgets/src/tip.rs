//! A tooltip, one control with two ways in: `tip(el, text)` puts it on any
//! stateful element ("" for none: the composer's session id, the queue
//! drawers, the mode button); `tooltip(text)` is the builder for
//! `.tooltip(…)` (a card's head, a code block's copy button). Both are
//! gpui-component's Tooltip, wrapped once (Planning's and Operations',
//! unified by the Lead).

use crundi_theme::T;
use gpui::*;
use std::cell::Cell;

thread_local! {
    /// The tip whose element was just pressed: its next showing is empty, so
    /// after a click (a row's menu, a file window, a double-click) the tip
    /// does not come back over what the click opened while the pointer
    /// rests there; it shows again once the pointer has left and come back,
    /// as a browser's title (F160). Pressing another tip's element ends it.
    static PRESSED: Cell<u64> = const { Cell::new(0) };
    /// The pointer's last move was over the pressed tip's element (its
    /// own move listener says so; `pointer_moved`, the window's, reads it
    /// on the next move): when it was not, the pointer left, and the tip
    /// may show again (F213).
    static OVER: Cell<bool> = const { Cell::new(false) };
    /// Where the pointer last moved over a control with its own tooltip
    /// inside a tipped element (`inner`): there the outer tip gives way, so a
    /// row's tip does not cover its More button's (F160).
    static INNER_AT: Cell<Option<Point<Pixels>>> = const { Cell::new(None) };
    /// The last tip drawn: its words, whether they showed, when (the test
    /// driver's `last_drawn`).
    static LAST: std::cell::RefCell<(SharedString, bool, Option<std::time::Instant>)> = const { std::cell::RefCell::new((SharedString::new_static(""), false, None)) };
}

/// The test driver's view of the last tip drawn: its words, whether they
/// showed (false: muted or under an inner control's own tooltip) and how
/// many ms ago.
pub fn last_drawn() -> serde_json::Value {
    LAST.with(|l| { let l = l.borrow(); serde_json::json!({ "text": l.0, "shown": l.1, "ago": l.2.map(|t| t.elapsed().as_millis() as u64) }) })
}

/// A tip's key, from its element's id and its words.
fn key(id: Option<&ElementId>, text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.map(|i| i.to_string()).hash(&mut h);
    text.hash(&mut h);
    h.finish() | 1
}

/// The tooltip's own view, on the theme's tokens. Shown through gpui's
/// native tooltip, which places it at the pointer and flips it above (or to
/// the left) where it would leave the window, as the web's (F43);
/// gpui-component's overlay placed it beside the element instead.
pub struct TipView { text: SharedString, muted: bool }

impl Render for TipView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Muted (shown right after a press) or over an inner control with
        // its own tooltip: nothing.
        let hide = self.muted || INNER_AT.with(|c| c.get()) == Some(window.mouse_position());
        LAST.with(|l| *l.borrow_mut() = (self.text.clone(), !hide, Some(std::time::Instant::now())));
        if hide { return div(); }
        // As wide as the room right of the pointer allows (the text wraps),
        // so at the right edge it stays by the pointer, as the PC client's,
        // rather than jumping to its left.
        let room = window.viewport_size().width - window.mouse_position().x - T::SP_8;
        let max_w = room.min(T::DIALOG_W).max(T::TIP_MIN_W);
        // A clear gap the cursor's height above the box: below the pointer
        // the text clears the cursor (the web's offset), and near the
        // window's bottom the taller box doesn't fit, so gpui flips it
        // above the pointer (F43).
        div().pt(T::SP_20).child(div().max_w(max_w).px(T::SP_8).py(T::SP_4).rounded(T::R_CONTROL).border_1().border_color(T::border())
            .bg(T::bg_secondary()).text_size(T::TEXT_12).text_color(T::text()).font_family(T::UI_FONT)
            .child(self.text.clone()))
    }
}

/// The builder for a tip with key `k` (0: never muted).
fn build(text: SharedString, k: u64) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_, cx| {
        let muted = k != 0 && PRESSED.with(|p| if p.get() == k { p.set(0); true } else { false });
        let text = text.clone();
        cx.new(|_| TipView { text, muted }).into()
    }
}

/// The tooltip's builder, for `.tooltip(w::tooltip("Copy code"))`.
pub fn tooltip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    build(text.into(), 0)
}

/// `el` with `text` as its tooltip ("" for none). A press on `el` keeps
/// the tip away until the pointer leaves it.
pub fn tip<E: StatefulInteractiveElement>(mut el: E, text: impl Into<SharedString>) -> E {
    let text: SharedString = text.into();
    if text.is_empty() { return el; }
    let k = key(el.interactivity().element_id.as_ref(), &text);
    el.capture_any_mouse_down(move |_, _, _| { PRESSED.with(|p| p.set(k)); OVER.with(|o| o.set(true)); })
        .on_mouse_move(move |_, _, _| if PRESSED.with(|p| p.get()) == k { OVER.with(|o| o.set(true)); })
        .tooltip(build(text, k))
}

/// Each move of the pointer in the window (the Shell's root calls it, in
/// the capture phase, before the elements hear the move): if the move before
/// was not over the pressed tip's element, the pointer has left it, and its
/// tip shows again when the pointer next rests there.
pub fn pointer_moved() {
    if PRESSED.with(|p| p.get()) == 0 { return; }
    if !OVER.with(|o| o.replace(false)) { PRESSED.with(|p| p.set(0)); }
}

/// Mark a control with its own tooltip (gpui-component's, as a Button's)
/// that sits inside a tipped element: over it the outer tip gives way.
pub fn inner<E: InteractiveElement>(el: E) -> E {
    el.on_mouse_move(|e: &MouseMoveEvent, _, _| INNER_AT.with(|c| c.set(Some(e.position))))
}
