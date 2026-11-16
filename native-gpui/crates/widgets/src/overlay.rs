//! A modal dialog over the whole window, drawn from inside any view (the
//! web's `.cc-modal-back` + `.cc-modal`): a scrim that takes the pointer
//! (nothing under it hovers) and closes the dialog when pressed; the card
//! centred, `width` wide, never closer than sp-16 to the window's edges,
//! as tall as its content up to dialog-max-h, then scrolling (`scroll`:
//! the header scrolls with the body). The caller gives the card's focus
//! (focus it when it opens) and its key handling (Esc, Tab…).

use crundi_theme::T;
use gpui::*;
use std::rc::Rc;

pub struct Modal {
    pub id: &'static str,
    pub width: Pixels,
    pub focus: FocusHandle,
    pub scroll: ScrollHandle,
    pub on_close: Rc<dyn Fn(&mut Window, &mut App)>,
    pub on_key: Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>,
}

pub fn overlay_modal(m: Modal, window: &Window, children: Vec<AnyElement>) -> impl IntoElement {
    let vp = window.viewport_size();
    let max_h = (vp.height - T::SP_16 * 2.).min(T::DIALOG_MAX_H).max(px(0.));
    let w = m.width.min(vp.width - T::SP_16 * 2.);
    let (close, key) = (m.on_close.clone(), m.on_key.clone());
    deferred(anchored().position(point(px(0.), px(0.))).child(
        div().id((m.id, 0usize)).occlude().w(vp.width).h(vp.height).bg(T::scrim()).flex().items_center().justify_center()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| close(window, cx))
            .child(div().id((m.id, 1usize)).track_focus(&m.focus).on_key_down(move |e, window, cx| key(e, window, cx))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .w(w).max_h(max_h).flex().flex_col().bg(T::bg_secondary()).border_1().border_color(T::border()).rounded(T::R_CARD).p(T::SP_6)
                .child(div().id((m.id, 2usize)).flex().flex_col().w_full().min_h_0().overflow_y_scroll().track_scroll(&m.scroll).children(children))),
    )).with_priority(1)
}

/// A modal's header: the title in capitals and its ×.
pub fn modal_head(id: &'static str, title: impl Into<SharedString>, close_focused: bool, on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    div().flex().flex_none().items_center().gap(T::SP_8).pt(T::SP_8).px(T::SP_8).pb(T::SP_6)
        .child(div().flex_1().text_size(T::TEXT_11).font_weight(T::W_BOLD).text_color(T::text_secondary()).child(title.into().to_uppercase()))
        .child(div().id(id).px(T::SP_8).py(T::SP_4).rounded(T::R_CONTROL).cursor_pointer().text_color(T::text_secondary()).border_1()
            .border_color(if close_focused { T::accent() } else { transparent_black() })
            .hover(|d| d.bg(T::bg_tertiary()).text_color(T::text())).child("×").on_click(on_close))
}
