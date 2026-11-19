//! Cru's round buttons (crundi-voice.js .cv-x): a filled circle with one
//! icon and a tooltip: ⋯, close, the keyboard, attach, send. `on` is the
//! accent fill (keyboard mode, send); `ring` a state ring round it, as a
//! shadow taking no space (keyboard mode's send: the state's colour).
//! Added by Desktop (AREAS.md, "A new control").

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};

/// A round button `size` across, its icon `icon_size`.
pub fn round_button(id: impl Into<ElementId>, icon: IconName, tip: impl Into<SharedString>, on: bool, ring: Option<Hsla>, size: Pixels, icon_size: Pixels) -> Stateful<Div> {
    let tip: SharedString = tip.into();
    div().id(id).relative().flex().flex_none().items_center().justify_center().size(size).rounded_full().cursor_pointer()
        .bg(if on { T::accent() } else { T::bg_tertiary() })
        .text_color(if on { T::white() } else { T::text_secondary() })
        .text_size(icon_size)
        .when(!on, |d| d.hover(|s| s.bg(T::bg_hover()).text_color(T::text())))
        .when_some(ring, |d, c| d.shadow(vec![BoxShadow { color: c, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: T::SP_2 + px(1.), inset: false }]))
        .child(icon)
        .tooltip(crate::tooltip(tip))
}
