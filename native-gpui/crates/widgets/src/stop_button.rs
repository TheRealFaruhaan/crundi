//! The composer's Stop (`.cc-stop`): a red rounded square on bg-primary, a
//! 1px border, `control` high; hovered, a red border. Armed (the first
//! click): red-dim fill, red border, and "Sure?" beside the square.
//! `scale` is the pane's text scale ("Sure?" grows with it; the box not).

use crate::tip;
use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};

pub fn stop_button(id: impl Into<ElementId>, armed: bool, scale: f32) -> Stateful<Div> {
    let d = div().id(id).flex().flex_none().items_center().justify_center().gap(T::SP_4).h(T::CONTROL).min_w(T::CONTROL).px(T::SP_8)
        .rounded(T::R_CONTROL).border_1().cursor_pointer()
        .bg(if armed { T::red_dim() } else { T::bg() })
        .border_color(if armed { T::red() } else { T::border() })
        .hover(|d| d.border_color(T::red()))
        .child(div().size(T::ICON_SM).rounded(T::R_CHIP / 2.).bg(T::red()))
        .when(armed, |d| d.child(div().text_size(T::TEXT_11 * scale).text_color(T::red()).whitespace_nowrap().child("Sure?")));
    tip(d, if armed { "Click again to interrupt" } else { "Stop this turn (click twice)" })
}
