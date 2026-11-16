//! A multi-line text box (gpui-component's Textarea, wrapped once): the
//! composer's message box and the schedule panel's message.

use gpui::*;
pub use gpui_component::input::{Textarea, TextareaState};

/// A multi-line box over a TextareaState (create it once and keep it in the view).
pub fn textarea(state: &Entity<TextareaState>) -> Textarea {
    Textarea::new(state)
}
