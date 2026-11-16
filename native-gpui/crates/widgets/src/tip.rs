//! A tooltip, one control with two ways in: `tip(el, text)` puts it on any
//! stateful element ("" for none: the composer's session id, the queue
//! drawers, the mode button); `tooltip(text)` is the builder for
//! `.tooltip(…)` (a card's head, a code block's copy button). Both are
//! gpui-component's Tooltip, wrapped once (Planning's and Operations',
//! unified by the Lead).

use gpui::*;

/// The tooltip's builder, for `.tooltip(w::tooltip("Copy code"))`.
pub fn tooltip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |w, cx| gpui_component::tooltip::Tooltip::new(text.clone()).build(w, cx)
}

/// `el` with `text` as its tooltip ("" for none).
pub fn tip<E: StatefulInteractiveElement>(el: E, text: impl Into<SharedString>) -> E {
    let text: SharedString = text.into();
    if text.is_empty() { return el; }
    el.tooltip(tooltip(text))
}
