//! The running spinner (the web's .cc-spin): a small turning ring in the
//! accent, beside a running tool call or an asking side question. Its own
//! (not gpui-component's Spinner, which turns at the display's rate): at
//! most T::ANIM_FPS frames a second, as Zed caps its decorative animations.
use crundi_theme::{IconName, T};
use gpui::{prelude::*, *};
use gpui_component::{Icon, Sizable as _, Size};
use std::time::Duration;

/// The spinner at icon size `size` (T::ICON_SM in a card's head), in the accent.
pub fn spinner(size: Pixels) -> Spin { Spin { size, color: T::accent() } }

#[derive(IntoElement)]
pub struct Spin { size: Pixels, color: Hsla }

impl Spin {
    /// Another colour than the accent.
    pub fn color(mut self, c: Hsla) -> Self { self.color = c; self }
}

impl RenderOnce for Spin {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let icon: Icon = IconName::LoaderCircle.into();
        div().flex_none().child(
            icon.with_size(Size::Size(self.size)).text_color(self.color).with_animation(
                "spin",
                Animation::new(Duration::from_millis(800)).repeat().with_max_fps(T::ANIM_FPS),
                |this, t| this.transform(Transformation::rotate(percentage(t))),
            ),
        )
    }
}
