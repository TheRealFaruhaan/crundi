//! Two small buttons the file windows and the media lightbox use (added by
//! Workspace, round 8; the Architect to review):
//!
//! - `pill_button`: a bordered pill on bg-tertiary, secondary text; hovered,
//!   an accent fill and white text (the web's `.fe-arrange button`, the
//!   floating Arrange bar's Tile / Grid / Cascade).
//! - `scrim_button`: white text on a faint white fill, for a dark scrim
//!   (the web's `.media-lightbox .ml-btn`, Download and Close).

use crundi_theme::T;
use gpui::*;

pub fn pill_button(id: impl Into<ElementId>, label: impl Into<SharedString>, tip: impl Into<SharedString>) -> Stateful<Div> {
    let d = div().id(id).flex().flex_none().items_center().h(T::SP_24).px(T::SP_10).rounded(T::R_PILL).cursor_pointer()
        .border_1().border_color(T::border()).bg(T::bg_tertiary()).text_color(T::text_secondary()).text_size(T::TEXT_12)
        .hover(|d| d.border_color(T::accent()).bg(T::accent()).text_color(T::white()))
        .child(label.into());
    crate::tip(d, tip)
}

pub fn scrim_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).flex().flex_none().items_center().gap(T::SP_4).h(T::CONTROL_SM).px(T::SP_10).rounded(T::R_CONTROL).cursor_pointer()
        .bg(T::white().opacity(T::WS_ML_BTN_O)).text_color(T::white()).text_size(T::TEXT_12)
        .hover(|d| d.bg(T::white().opacity(T::WS_ML_BTN_HOVER_O)))
        .child(label.into())
}

/// A small bordered button in a colour of its own, no fill (the web's diff
/// window Stage: a green edge and green bold text).
pub fn tone_outline(id: impl Into<ElementId>, label: impl Into<SharedString>, color: Hsla) -> Stateful<Div> {
    div().id(id).flex().flex_none().items_center().h(T::SP_16 + px(2.)).px(T::SP_8).rounded(T::SP_4).cursor_pointer()
        .border_1().border_color(color).text_color(color).text_size(T::TEXT_10).font_weight(T::W_SEMIBOLD)
        .hover(|d| d.bg(color.opacity(T::WS_ML_BTN_O)))
        .child(label.into())
}
