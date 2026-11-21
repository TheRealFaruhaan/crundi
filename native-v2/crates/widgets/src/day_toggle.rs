//! A weekday toggle (the web's .sm-day in the Schedule dialog,
//! src/webapp-html.js:2785-2786): a T::DAY_W x T::DAY_H bordered button on
//! bg-tertiary; on, the accent edge on accent-dim with the accent-hover ink.
//! The web gives it no hover rule, so neither does this. A Stateful<Div>:
//! the caller adds the click.

use crundi_theme::T;
use gpui::*;

pub fn day_toggle(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    let (bg, edge, ink) = if on { (T::accent_dim(), T::accent(), T::accent_hover()) } else { (T::bg_tertiary(), T::border(), T::text_secondary()) };
    div().id(id).flex().flex_none().items_center().justify_center().w(T::DAY_W).h(T::DAY_H).cursor_pointer()
        .rounded(T::R_CONTROL).border_1().border_color(edge).bg(bg).text_color(ink)
        .text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).child(label.into())
}
