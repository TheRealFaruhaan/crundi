//! A modal dialog over the whole window, drawn from inside any view (the
//! web's dialogs: `#sched-modal` + `.sched-modal-card`, `.input-modal`,
//! `#add-project-modal`…): a scrim that takes the pointer (nothing under it
//! hovers) and closes the dialog when pressed (the web's `e.target ===
//! modal` click); the card centred, `width` wide, never closer than sp-16
//! to the window's edges and at most 90% of the window tall (the web's
//! `max-height: 90vh`). Esc does not close it (the web's dialogs ignore
//! it); a dialog that does (Add project, a help sheet) says so in its own
//! `on_key`.
//!
//! Two ways to fill it:
//! - `overlay_modal(m, window, children)`: the children in the padded card,
//!   scrolling as a whole when they are taller than the card;
//! - `overlay_dialog(m, window, head, body, foot)` (and `dialog(…)` for a
//!   view's own body in `modal_pinned`): the web's `.sm-head` / `.sm-bodyscroll`
//!   / `.sm-foot`: the head and the footer stay pinned, only the body scrolls.

use crundi_theme::T;
use gpui::*;
use std::rc::Rc;

thread_local! { static HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

/// While the PC lock or the kid screen is on, a dialog's keys do nothing and an
/// ask takes no focus (F228; the web drops every key outside the lock).
pub fn hold_keys(on: bool) { HELD.with(|h| h.set(on)); }
pub fn keys_held() -> bool { HELD.with(|h| h.get()) }

pub struct Modal {
    pub id: &'static str,
    pub width: Pixels,
    pub focus: FocusHandle,
    pub scroll: ScrollHandle,
    pub on_close: Rc<dyn Fn(&mut Window, &mut App)>,
    pub on_key: Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>,
}

/// The card's tallest: 90% of the window, never closer than sp-16 to its edges.
fn max_h(window: &Window) -> Pixels {
    let vp = window.viewport_size();
    (vp.height * 0.9).min(vp.height - T::SP_16 * 2.).max(px(0.))
}

fn frame(m: Modal, window: &Window, padded: bool, inner: AnyElement) -> impl IntoElement {
    let vp = window.viewport_size();
    let w = m.width.min(vp.width - T::SP_16 * 2.);
    let (close, key) = (m.on_close.clone(), m.on_key.clone());
    let card = div().id((m.id, 1usize)).track_focus(&m.focus).on_key_down(move |e, window, cx| if !keys_held() { key(e, window, cx) })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .w(w).max_h(max_h(window)).flex().flex_col().overflow_hidden().bg(T::bg_secondary()).border_1().border_color(T::border()).rounded(T::R_CARD);
    let card = if padded { card.p(T::SP_6) } else { card };
    deferred(anchored().position(point(px(0.), px(0.))).child(
        div().id((m.id, 0usize)).occlude().w(vp.width).h(vp.height).bg(T::scrim()).flex().items_center().justify_center()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| close(window, cx))
            .child(card.child(inner)),
    )).with_priority(1)
}

/// The children in the padded card, scrolling as a whole.
pub fn overlay_modal(m: Modal, window: &Window, children: Vec<AnyElement>) -> impl IntoElement {
    let inner = div().id((m.id, 2usize)).flex().flex_col().w_full().min_h_0().overflow_y_scroll().track_scroll(&m.scroll).children(children).into_any_element();
    frame(m, window, true, inner)
}

/// The web's pinned dialog: `head` (`modal_head`) on top, `body` scrolling
/// (`m.scroll`), `foot` (`modal_foot`) at the bottom, always in view.
pub fn overlay_dialog(m: Modal, window: &Window, head: impl IntoElement, body: impl IntoElement, foot: Option<AnyElement>) -> impl IntoElement {
    let inner = dialog_parts((m.id, 3usize), Some(&m.scroll), head, body, foot).into_any_element();
    frame(m, window, false, inner)
}

/// The pinned layer's card around a view that draws `dialog(…)` itself.
pub(crate) fn overlay_dialog_view(m: Modal, window: &Window, inner: AnyElement) -> impl IntoElement {
    frame(m, window, false, inner)
}

/// A view's own body for `modal_pinned` (the view draws its head, body and
/// footer; the body alone scrolls).
pub fn dialog(id: impl Into<ElementId>, head: impl IntoElement, body: impl IntoElement, foot: Option<AnyElement>) -> Div {
    dialog_parts(id, None, head, body, foot)
}

fn dialog_parts(id: impl Into<ElementId>, scroll: Option<&ScrollHandle>, head: impl IntoElement, body: impl IntoElement, foot: Option<AnyElement>) -> Div {
    let mut mid = div().id(id).flex_1().min_h_0().flex().flex_col().overflow_y_scroll();
    if let Some(s) = scroll { mid = mid.track_scroll(s); }
    let mut d = div().flex().flex_col().w_full().min_h_0().max_h_full()
        .child(div().flex_none().border_b_1().border_color(T::border()).child(head))
        .child(mid.child(body));
    if let Some(f) = foot { d = d.child(div().flex_none().child(f)); }
    d
}

/// A dialog's footer (the web's `.sm-foot`): its buttons on the right, a
/// hairline above.
pub fn modal_foot() -> Div {
    div().flex().flex_none().justify_end().items_center().gap(T::SP_8).px(T::SP_16).py(T::SP_12).border_t_1().border_color(T::border())
}

/// A dialog's head (the web's `.sm-head`: the title as written, its ×).
pub fn modal_head(id: &'static str, title: impl Into<SharedString>, close_focused: bool, on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    div().flex().flex_none().items_center().justify_between().gap(T::SP_8).py(T::SP_12).px(T::SP_16)
        .child(div().flex_1().min_w_0().text_size(T::TEXT_16).font_weight(T::W_BOLD).text_color(T::text()).child(title.into()))
        .child(div().id(id).px(T::SP_4).rounded(T::R_CONTROL).cursor_pointer().text_color(T::text_muted()).border_1()
            .border_color(if close_focused { T::accent() } else { transparent_black() })
            .hover(|d| d.text_color(T::text())).child(crate::control::icon_of(crundi_theme::IconName::X, T::ICON_MD)).on_click(on_close))
}
