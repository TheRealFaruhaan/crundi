//! A pill that turns a filter on and off (the web's .mm-proj-chip, the Mind
//! map's project bar): its green dot and name; off: faded, struck through,
//! no dot; `reset` (Show all): dashed, no fill. Added by Planning (4.3).

use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};

pub fn filter_chip(id: impl Into<ElementId>, label: impl Into<SharedString>, off: bool, reset: bool) -> Stateful<Div> {
    div().id(id).flex().flex_none().items_center().gap(T::SP_6).h(T::SP_24).px(T::SP_10).rounded(T::R_PILL).cursor_pointer()
        .border_1().border_color(T::border()).hover(|d| d.border_color(T::accent()))
        .when(reset, |d| d.border_dashed().text_color(T::text_secondary()))
        .when(!reset, |d| d.bg(T::bg_tertiary()))
        .when(off, |d| d.opacity(T::MINDMAP_O_OFF).text_color(T::text_muted()).line_through())
        .when(!off && !reset, |d| d.text_color(T::text()).child(div().flex_none().size(T::SP_6 + px(1.)).rounded_full().bg(T::green())))
        .text_size(T::TEXT_12)
        .child(label.into())
}
