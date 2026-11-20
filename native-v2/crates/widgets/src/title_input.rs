//! A pane title's rename box (the web's .term-title-input): the title's
//! 12px semibold in a box with the accent border, at most the short
//! title's 160px. Added by Workbench (AREAS.md, "A new control").

use crate::{Input, InputState};
use crundi_theme::T;
use gpui::*;

pub fn title_input(state: &Entity<InputState>) -> Input {
    Input::new(state).h(T::SP_20).w(T::TITLE_CAP).px(T::SP_6).text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD)
        .bg(T::bg()).border_1().border_color(T::accent()).rounded(T::R_CONTROL)
}
