//! A live label's shimmer (the web's thinking "Thinking" while it streams):
//! a brighter band sweeping over the text. (Made by operations; Lead to review.)
use crundi_theme::T;
use gpui::{ElementId, SharedString};
use gpui_component::shimmer::ShimmerText;
use std::time::Duration;

/// `text` with a sweeping highlight in the primary ink; style it as text
/// (size, colour) with Styled. `id`: one per label on screen.
pub fn shimmer(id: impl Into<ElementId>, text: impl Into<SharedString>) -> ShimmerText {
    ShimmerText::new(text).id(id).highlight_color(T::text()).duration(Duration::from_millis(1600)).reverse(true)
}
