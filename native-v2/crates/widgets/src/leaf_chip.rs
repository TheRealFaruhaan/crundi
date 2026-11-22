//! An empty pane's controls (the web's .mosaic-empty): the launch chips, pills
//! with an accent icon that light up in the accent on hover (.me-chip), and
//! the small actions under them (.me-acts button: Split ▸, Split ▾, ×).
//! Added by Workbench (AREAS.md, "A new control").

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};

/// A launch chip: padding 5 / 10, a pill, 11.5px, the icon 13px in the
/// accent; hover: accent border, primary text, accent wash. `tip`: "Add …".
pub fn leaf_chip(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>, tip: impl Into<SharedString>) -> Stateful<Div> {
    let tip: SharedString = tip.into();
    div().id(id).flex().flex_none().items_center().gap(T::LEAF_CHIP_GAP).py(T::LEAF_CHIP_PY).px(T::SP_10).rounded(T::R_PILL).cursor_pointer()
        .border_1().border_color(T::border()).bg(T::bg()).text_color(T::text_secondary()).text_size(T::LEAF_CHIP_TEXT)
        .hover(|s| s.border_color(T::accent()).text_color(T::text()).bg(T::accent_dim()))
        .child(div().flex_none().text_size(T::LEAF_ICON).text_color(T::accent_hover()).child(icon))
        .child(label.into())
        .tooltip(crate::tooltip(tip))
}

/// An action under the chips: padding 5 / 9, the small radius, 0.72rem (11.5px), a
/// 13px icon then its label (none for ×); hover: accent border, primary text.
pub fn leaf_act(id: impl Into<ElementId>, icon: IconName, label: Option<&'static str>, tip: impl Into<SharedString>) -> Stateful<Div> {
    let tip: SharedString = tip.into();
    div().id(id).flex().flex_none().items_center().gap(T::SP_4).py(T::LEAF_CHIP_PY).px(T::LEAF_ACT_PX).rounded(T::R_CONTROL).cursor_pointer()
        .border_1().border_color(T::border()).bg(T::bg_tertiary()).text_color(T::text_secondary()).text_size(T::LEAF_CHIP_TEXT)
        .hover(|s| s.border_color(T::accent()).text_color(T::text()))
        .child(div().flex_none().text_size(T::LEAF_ICON).child(icon))
        .when_some(label, |d, l| d.child(l))
        .tooltip(crate::tooltip(tip))
}
