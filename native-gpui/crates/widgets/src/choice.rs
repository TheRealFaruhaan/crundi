//! A choice's mark: the radio (round) or the checkbox (square) drawn at the
//! left of a choice row whose whole row is the press target (a question
//! card's choices, 2.13: .cc-opt). `w::check` is a box with its own label
//! and click; this is the mark alone, for rows that handle the press.
use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};

/// The mark: `multi` a checkbox, else a radio; `on` picked; `focused` the
/// keyboard is on it (the accent ring round the mark itself).
pub fn choice_mark(multi: bool, on: bool, focused: bool) -> Div {
    let mark = div().flex().flex_none().items_center().justify_center().size(T::ICON_SM).border_1()
        .rounded(if multi { T::R_CHIP } else { T::R_PILL })
        .border_color(if on { T::accent() } else { T::text_muted() })
        .bg(if on && multi { T::accent() } else { T::bg() })
        .when(on && !multi, |d| d.child(div().size(T::SP_6).rounded(T::R_PILL).bg(T::accent())))
        .when(on && multi, |d| d.text_size(T::TEXT_10).text_color(T::white()).child(IconName::Check));
    div().flex_none().p(T::SP_2).rounded(if multi { T::R_CONTROL } else { T::R_PILL })
        .when(focused, |d| d.bg(T::accent_edge()))
        .child(mark)
}
