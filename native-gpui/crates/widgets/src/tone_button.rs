//! A small bordered button in a tone of its own (the composer's mode
//! button, `.cc-mode`): an icon and an optional label in `fg`, a 1px `edge`
//! border on bg-primary, r-control; hovered, the border goes accent (unless
//! `keep_edge`: Plan mode and Bypass all keep theirs). `scale` is the
//! pane's text scale (its height, icon and text grow with it).

use crate::tip;
use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::Icon;

pub fn tone_button(id: impl Into<ElementId>, icon: IconName, label: Option<SharedString>, fg: Hsla, edge: Hsla, keep_edge: bool, tooltip: impl Into<SharedString>, scale: f32) -> Stateful<Div> {
    let d = div().id(id).flex().flex_none().items_center().justify_center().gap(T::SP_4).h(T::SP_24 * scale).min_w(T::CONTROL_SM * scale)
        .px(T::SP_6).rounded(T::R_CONTROL).border_1().border_color(edge).bg(T::bg()).cursor_pointer().text_color(fg)
        .text_size(T::TEXT_11 * scale)
        .when(!keep_edge, |d| d.hover(|d| d.border_color(T::accent())))
        .child(Icon::new(icon).size(T::ICON_MD * scale).text_color(fg))
        .when_some(label, |d, l| d.child(div().whitespace_nowrap().child(l)));
    tip(d, tooltip)
}
