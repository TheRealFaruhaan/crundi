//! The authenticator's 6-digit code box (the web's #login-code): the text
//! centred, mono. Its InputState takes digits only, at most 6
//! (`digits_state`, the web's maxlength 6 and pattern [0-9]{6}).

use crate::{input, Input, InputState};
use crundi_theme::T;
use gpui::*;

/// A code box's state: digits only, six at most.
pub fn digits_state(state: InputState) -> InputState {
    state.validate(|s, _| s.len() <= 6 && s.chars().all(|c| c.is_ascii_digit()))
}

pub fn code_input(state: &Entity<InputState>) -> Input {
    input(state).text_center().font_family(T::MONO)
}
