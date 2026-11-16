//! A multi-line text box (gpui-component's Textarea, wrapped once): the
//! composer's message box and the schedule panel's message.

use gpui::*;
pub use gpui_component::input::{Textarea, TextareaState};

/// A multi-line box over a TextareaState (create it once and keep it in the view).
pub fn textarea(state: &Entity<TextareaState>) -> Textarea {
    Textarea::new(state)
}

/// The composer's message box: grows from the control height to
/// composer-max, then scrolls; `scale` is the pane's text scale.
pub fn message_box(state: &Entity<TextareaState>, scale: f32) -> Textarea {
    use crundi_theme::T;
    Textarea::new(state).bg(T::bg()).border_color(T::border()).rounded(T::R_CONTROL)
        .text_size(T::TEXT_13 * scale).min_h(T::CONTROL).max_h(T::COMPOSER_MAX)
}
