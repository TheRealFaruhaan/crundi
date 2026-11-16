//! The composer's Send / Queue (the Slint client's SendBtn, the web's
//! .cc-btn.primary): the accent button at the `control` height, lined up
//! with the message box, the same size in every state and layout (wide,
//! narrow, recording aside). `scale` is the pane's text scale (its label).

use crundi_theme::T;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants as _};

pub fn send_button(id: impl Into<ElementId>, label: impl Into<SharedString>, scale: f32) -> Button {
    Button::new(id).label(label.into()).primary().flex_none().h(T::CONTROL).min_w(T::SP_48 + T::SP_16).px(T::SP_12)
        .rounded(T::R_CONTROL).text_size(T::TEXT_12 * scale).font_weight(T::W_SEMIBOLD)
}
