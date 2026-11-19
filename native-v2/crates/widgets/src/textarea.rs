//! A multi-line text box (gpui-component's Textarea, wrapped once): the
//! composer's message box and the schedule panel's message.

use gpui::*;
pub use gpui_component::input::{Editor, EditorState, Textarea, TextareaState};

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

/// A source file's editor: line numbers and syntax highlighting by the
/// file's extension (rs, js, ts, py, json, md…; unknown ones show plain).
pub fn code_state(file: &str, window: &mut Window, cx: &mut Context<EditorState>) -> EditorState {
    let ext = std::path::Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("text").to_string();
    EditorState::new(window, cx).language(ext)
}

/// The editor over a code_state.
pub fn code_editor(state: &Entity<EditorState>) -> Editor {
    use crundi_theme::T;
    Editor::new(state).bordered(false).text_size(T::TEXT_13)
}
