//! The disclosure at a text scale (the chat log's folding heads, which
//! follow the pane's A- / A+): `fold_chevron`, the disclosure's chevron
//! (right while folded, down while open) for a head that puts it last (a
//! tool card's, a group's), and `disclosure_at`, the disclosure itself with
//! any words (a thinking block's "Thinking", shimmering while it streams).
//! The same look as `disclosure`; only the scale is added. Made by the Chat
//! area (the log, F75).

use crundi_theme::{IconName, T};
use gpui::*;

/// The disclosure's chevron at `scale` (in the current text colour).
pub fn fold_chevron(open: bool, scale: f32) -> Div {
    div().flex_none().child(crate::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM * scale))
}

/// `disclosure` with any words, at `scale`: the chevron, then the words;
/// hovered, they brighten. The caller folds and unfolds on click.
pub fn disclosure_at(id: impl Into<ElementId>, words: impl IntoElement, open: bool, scale: f32) -> Stateful<Div> {
    div().id(id).flex().items_center().gap(T::SP_4).min_w_0().cursor_pointer()
        .text_size(T::TEXT_12 * scale).text_color(T::text_secondary()).hover(|d| d.text_color(T::text()))
        .child(fold_chevron(open, scale))
        .child(div().min_w_0().child(words))
}
