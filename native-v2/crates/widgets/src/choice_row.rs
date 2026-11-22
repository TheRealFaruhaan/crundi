//! A choice in a dialog's list (the permission-mode picker's `.cc-mode-opt`):
//! a tile with the choice's icon in its colour, the name over a wrapping
//! description. Hover bg-tertiary; selected accent-dim with an accent-line
//! border; the keyboard's focus an accent border.

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::Icon;

pub fn choice_row(id: impl Into<ElementId>, icon: IconName, tone: Hsla, name: impl Into<SharedString>, desc: impl Into<SharedString>, selected: bool, focused: bool) -> Stateful<Div> {
    div().id(id).flex().flex_none().items_center().gap(T::SP_12).w_full().min_h(T::SP_48 + T::SP_4).p(T::SP_10).rounded(T::R_FIELD).border_1().cursor_pointer()
        .border_color(if focused { T::accent() } else if selected { T::accent_edge() } else { transparent_black() })
        .when(selected, |d| d.bg(T::accent_dim()))
        .when(!selected, |d| d.hover(|d| d.bg(T::bg_tertiary())))
        .child(div().flex().flex_none().items_center().justify_center().size(T::SP_32).rounded(T::R_FIELD).bg(T::bg()).border_1().border_color(T::border())
            .child(Icon::new(icon).size(T::ICON_LG).text_color(tone)))
        .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
            .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(name.into()))
            .child(div().text_size(T::TEXT_12).line_height(relative(T::LH_CHAT)).text_color(T::text_secondary()).child(desc.into())))
}
