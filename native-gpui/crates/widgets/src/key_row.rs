//! A menu's row with a key box (the web's .nt-menu button, the Slint client's
//! notes MenuRow): the block type's glyph in a small bordered box, the name,
//! a hint on the right (its Markdown shortcut) or a tick; `active` while the
//! keys are on it; a red hover for a destructive row. Notes' slash, block,
//! language and page menus (Planning's request, shared/requests.md).

use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};

/// One row: `key` the glyph ("" none), `hint` the right-hand words ("" none).
pub fn key_row(id: impl Into<ElementId>, key: impl Into<SharedString>, name: impl Into<SharedString>, hint: impl Into<SharedString>, check: bool, danger: bool, active: bool) -> Stateful<Div> {
    let key: SharedString = key.into();
    let hint: SharedString = hint.into();
    div()
        .id(id).flex().flex_none().items_center().gap(T::SP_10).px(T::SP_8).py(T::SP_6)
        .rounded(T::R_CONTROL).cursor_pointer().text_color(T::text())
        .when(active, |d| d.bg(T::bg_hover()))
        .hover(move |s| if danger { s.bg(T::red_dim()).text_color(T::red()) } else { s.bg(T::bg_hover()) })
        .when(!key.is_empty(), |d| d.child(div().flex_none().size(T::SP_24).rounded(T::R_CHIP).border_1().border_color(T::border())
            .flex().items_center().justify_center().text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(key)))
        .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_13).child(name.into()))
        .when(!hint.is_empty(), |d| d.child(div().flex_none().text_size(T::TEXT_11).text_color(T::text_muted()).child(hint)))
        .when(check, |d| d.child(div().flex_none().text_size(T::TEXT_13).text_color(T::accent_hover()).child("\u{2713}")))
}
