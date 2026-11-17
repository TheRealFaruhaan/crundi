//! A tooltip, one control with two ways in: `tip(el, text)` puts it on any
//! stateful element ("" for none: the composer's session id, the queue
//! drawers, the mode button); `tooltip(text)` is the builder for
//! `.tooltip(…)` (a card's head, a code block's copy button). Both are
//! gpui-component's Tooltip, wrapped once (Planning's and Operations',
//! unified by the Lead).

use crundi_theme::T;
use gpui::*;

/// The tooltip's own view, on the theme's tokens. Shown through gpui's
/// native tooltip, which places it at the pointer and flips it above (or to
/// the left) where it would leave the window, as the web's (F43);
/// gpui-component's overlay placed it beside the element instead.
pub struct TipView(SharedString);

impl Render for TipView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(self.0.clone()))
    }
}

/// The tooltip's builder, for `.tooltip(w::tooltip("Copy code"))`.
pub fn tooltip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |_, cx| cx.new(|_| TipView(text.clone())).into()
}

/// `el` with `text` as its tooltip ("" for none).
pub fn tip<E: StatefulInteractiveElement>(el: E, text: impl Into<SharedString>) -> E {
    let text: SharedString = text.into();
    if text.is_empty() { return el; }
    el.tooltip(tooltip(text))
}
