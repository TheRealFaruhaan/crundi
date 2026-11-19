//! A folding section's summary line (the web's <details><summary>): a
//! chevron (right when folded, down when open) and the words; hovered, the
//! words brighten. The caller folds and unfolds on click. (Factories:
//! Earlier runs, Triggers, More tool rules, Usage limits.) Made by Planning.

use crundi_theme::{IconName, T};
use gpui::*;

pub fn disclosure(id: impl Into<ElementId>, text: impl Into<SharedString>, open: bool) -> Stateful<Div> {
    // Long words wrap inside the line's width (the web's <summary> does), the
    // chevron staying on the first line; never past the edge (F71).
    div().id(id).flex().items_start().gap(T::SP_4).min_w_0().w_full().cursor_pointer()
        .text_size(T::TEXT_12).text_color(T::text_secondary()).hover(|d| d.text_color(T::text()))
        .child(div().flex_none().pt(T::SP_2).child(crate::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM)))
        .child(div().flex_1().min_w_0().child(text.into()))
}
