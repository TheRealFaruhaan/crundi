//! A key cap (the web's kbd in the Mind map's shortcut sheet): mono,
//! bordered, with a thicker bottom edge. Added by Planning (4.3).

use crundi_theme::T;
use gpui::*;

pub fn kbd(text: impl Into<SharedString>) -> Div {
    div().flex_none().px(T::SP_6).rounded(T::R_CONTROL).border_1().border_b_2().border_color(T::border()).bg(T::bg())
        .font_family(T::MONO).text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::text())
        .child(text.into())
}
