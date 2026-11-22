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

/// A round button grown into a pill with its words (crundi-voice.js
/// .cv-ov.full .cv-fullx: "Normal view"): the accent fill, the icon then
/// the label, 12 px semibold white, `size` tall.
pub fn round_pill_button(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>, tip: impl Into<SharedString>, size: Pixels, icon_size: Pixels) -> Stateful<Div> {
    let tip: SharedString = tip.into();
    div().id(id).flex().flex_none().items_center().gap(T::SP_6).h(size).pl(T::SP_10).pr(T::SP_12).rounded(size / 2.).cursor_pointer()
        .bg(T::accent()).text_color(T::white()).text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).whitespace_nowrap()
        .hover(|s| s.bg(T::accent_hover()))
        .child(div().text_size(icon_size).child(icon))
        .child(label.into())
        .tooltip(crate::tooltip(tip))
}

/// A round button in colours of its own (the kid screen's .cv-kid-mic and
/// .cv-mute: a big state-coloured mic, a white mute with an indigo icon),
/// with an optional drop `shadow` under it. Added by Desktop.
#[allow(clippy::too_many_arguments)]
pub fn round_solid(id: impl Into<ElementId>, icon: IconName, size: Pixels, icon_size: Pixels, bg: Hsla, fg: Hsla, shadow: Option<BoxShadow>) -> Stateful<Div> {
    div().id(id).relative().flex().flex_none().items_center().justify_center().size(size).rounded_full().cursor_pointer()
        .bg(bg).text_color(fg).text_size(icon_size)
        .when_some(shadow, |d, s| d.shadow(vec![s]))
        .child(icon)
}
