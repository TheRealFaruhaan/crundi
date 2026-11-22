//! A top-bar action (the web's .topbar-act: a collaborator's Push and Ask
//! to merge): a bordered pill, its icon 13 and a short label 11 semibold,
//! secondary text; brighter with an accent edge on hover; faded while
//! `busy` (it takes no click then).

use crate::icon;
use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};

pub fn topbar_act(id: impl Into<ElementId>, ic: IconName, label: impl Into<SharedString>, busy: bool) -> Stateful<Div> {
    div().id(id).flex().flex_none().items_center().gap(T::SP_4 + px(1.)).py(T::SP_2 + px(1.)).px(T::SP_8 + px(1.)).rounded(T::R_PILL)
        .border_1().border_color(T::border()).bg(T::bg_secondary()).text_color(T::text_secondary())
        .text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).whitespace_nowrap()
        .when(busy, |d| d.opacity(0.55))
        .when(!busy, |d| d.cursor_pointer().hover(|d| d.text_color(T::text()).border_color(T::accent())))
        .child(icon(ic, T::ICON_SM + px(1.)))
        .child(label.into())
}
