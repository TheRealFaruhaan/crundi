//! Dragging a reference between panels (the web's data-drag-ref): a Files
//! row (later Git rows, Kanban subtasks, media) carries "[File Path: …]",
//! dropped into a chat's box or a terminal's input. The owner, 18 Nov: no
//! drag and drop between panels.

use crundi_theme::T;
use gpui::*;

/// What is dragged: the text dropped in, and the label the ghost shows.
#[derive(Clone)]
pub struct RefDrag { pub text: SharedString, pub label: SharedString }

/// The web's labels (DRAG_LABELS) around a value: "[File Path: /a/b]".
pub fn ref_text(kind: &str, value: &str) -> String {
    let label = match kind {
        "file" => "File Path", "folder" => "Folder Path", "kanban-task" => "Kanban Task ID",
        "kanban-subtask" => "Kanban Subtask ID", "mindmap" => "Mindmap Idea ID", "media" => "Media Path",
        _ => return value.to_string(),
    };
    format!("[{label}: {value}]")
}

/// The ghost under the pointer while dragging: the name, in a pill.
pub struct RefGhost(SharedString);
impl Render for RefGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px(T::SP_8).py(T::SP_4).rounded(T::R_PILL).bg(T::bg_tertiary()).border_1().border_color(T::accent())
            .text_size(T::TEXT_12).text_color(T::text()).child(self.0.clone())
    }
}

/// Make `el` a drag source for `kind` and `value` (the ghost says `label`).
pub fn drag_ref(el: Stateful<Div>, kind: &str, value: &str, label: impl Into<SharedString>) -> Stateful<Div> {
    let d = RefDrag { text: ref_text(kind, value).into(), label: label.into() };
    el.on_drag(d, |d, _, _, cx| cx.new(|_| RefGhost(d.label.clone())))
}

/// Accept a dragged reference (`put` gets its text), the accent outline while over.
pub fn ref_target(el: Stateful<Div>, put: impl Fn(&str, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    el.drag_over::<RefDrag>(|s, _, _, _| s.border_2().border_color(T::accent()))
        .on_drop(move |d: &RefDrag, window, cx| { put(&d.text, window, cx); cx.stop_propagation(); })
}

#[cfg(test)]
mod tests {
    #[test]
    fn labels() {
        assert_eq!(super::ref_text("file", "/p/a.rs"), "[File Path: /p/a.rs]");
        assert_eq!(super::ref_text("folder", "/p/src"), "[Folder Path: /p/src]");
        assert_eq!(super::ref_text("other", "x"), "x");
    }
}
