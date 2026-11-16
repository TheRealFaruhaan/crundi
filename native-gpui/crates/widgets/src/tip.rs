//! A tooltip on any element with an id (gpui-component's Tooltip, wrapped
//! once): the composer's session id, the queue drawers, the mode button.

use gpui::*;

/// `el` with `text` as its tooltip ("" for none).
pub fn tip<E: StatefulInteractiveElement>(el: E, text: impl Into<SharedString>) -> E {
    let text: SharedString = text.into();
    if text.is_empty() { return el; }
    el.tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(text.clone()).build(window, cx))
}
