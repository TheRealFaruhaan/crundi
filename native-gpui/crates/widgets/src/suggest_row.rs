//! A row of a suggestion list under the caret (the slash menu's `.cc-slash`
//! items): the name in mono, never wrapped, and a muted description that
//! elides; selected accent-dim, hovered bg-tertiary. Rows touch (no gap).

use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};

pub fn suggest_row(id: impl Into<ElementId>, name: impl Into<SharedString>, desc: impl Into<SharedString>, selected: bool, scale: f32) -> Stateful<Div> {
    let desc: SharedString = desc.into();
    div().id(id).flex().flex_none().items_center().gap(T::SP_10).w_full().px(T::SP_10).py(T::SP_6).cursor_pointer().overflow_hidden()
        .when(selected, |d| d.bg(T::accent_dim()))
        .when(!selected, |d| d.hover(|d| d.bg(T::bg_tertiary())))
        .child(div().flex_none().whitespace_nowrap().font_family(T::MONO).text_size(T::TEXT_12 * scale)
            .text_color(if selected { T::accent_hover() } else { T::text() }).child(name.into()))
        .when(!desc.is_empty(), |d| d.child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_11 * scale).text_color(T::text_muted()).child(desc)))
}
