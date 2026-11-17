//! Markdown text (Claude's replies, side answers, notes): gpui-component's
//! TextView once, on the theme's link colour, selectable, links handed to
//! the caller. (Made by operations; Lead to review.)
use gpui::{App, ClickEvent, ElementId, SharedString, Window};
use gpui_component::text::TextView;

/// A block of markdown; a link's click goes to `on_link` (never opened by
/// the view itself).
pub fn markdown(id: impl Into<ElementId>, text: impl Into<SharedString>, on_link: impl Fn(&SharedString, &mut Window, &mut App) + Send + Sync + 'static) -> TextView {
    TextView::markdown(id, text).selectable(true).on_link_click(move |u, _: &ClickEvent, w, cx| on_link(u, w, cx))
}
