//! A meter: a thin rounded track with a filled share (the Slint client's
//! SkBar; the web's .sys-bar, .collab-meter, the usage bars). `frac` is
//! 0..1 (clamped), `fill` its colour, `thick` the track's height. (Made by
//! operations; Lead to review.)

use crundi_theme::T;
use gpui::*;

pub fn meter(frac: f32, fill: Hsla, thick: Pixels) -> Div {
    let f = frac.clamp(0., 1.);
    div().flex_none().w_full().h(thick).rounded(thick / 2.).bg(T::bg_tertiary()).overflow_hidden()
        .child(div().h_full().w(relative(f)).rounded(thick / 2.).bg(fill))
}
