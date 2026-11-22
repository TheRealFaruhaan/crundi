//! A live label (the web's thinking "Thinking" while it streams): the text
//! breathing between muted and full. Not gpui-component's ShimmerText,
//! which asks for a frame at the display's rate with no cap: this one asks
//! for at most T::ANIM_FPS (each frame draws the pane again).
use crundi_theme::T;
use gpui::{prelude::*, *};
use std::time::Duration;

/// `text`, pulsing; style it as text (size, colour) with Styled. `id`: one
/// per label on screen.
pub fn shimmer(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Shimmer {
    Shimmer { id: id.into(), text: text.into(), style: StyleRefinement::default() }
}

#[derive(IntoElement)]
pub struct Shimmer { id: ElementId, text: SharedString, style: StyleRefinement }

impl Styled for Shimmer {
    fn style(&mut self) -> &mut StyleRefinement { &mut self.style }
}

impl RenderOnce for Shimmer {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut d = div().min_w_0();
        *d.style() = self.style;
        d.child(self.text).with_animation(
            self.id,
            Animation::new(Duration::from_millis(1600)).repeat().with_max_fps(T::ANIM_FPS),
            |d, t| d.opacity(0.55 + 0.45 * (0.5 + 0.5 * (t * std::f32::consts::TAU).cos())),
        )
    }
}
