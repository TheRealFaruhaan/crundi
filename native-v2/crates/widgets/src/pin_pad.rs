//! A PIN keypad: 1-9, a delete key, 0 and an OK key in rows of three. The PC
//! lock's (src/webapp-html.js .pcl-keys: a chevron and a tick, a lighter fill
//! on hover) and the kid screen's (crundi-voice.js .cv-pin-k: "←" and "OK").
//! A key calls `press` with "1".."9", "0", "Back" or "OK".
//! Added by Desktop (AREAS.md, "A new control").

use crundi_theme::IconName;
use gpui::{prelude::FluentBuilder as _, *};
use std::rc::Rc;

/// How the keys look.
#[derive(Clone, Copy)]
pub struct PinLook {
    pub key_w: Pixels,
    pub key_h: Pixels,
    pub gap: Pixels,
    pub radius: Pixels,
    pub bg: Hsla,
    /// The fill under the pointer (None: no hover).
    pub hover: Option<Hsla>,
    pub edge: Hsla,
    pub fg: Hsla,
    pub text: Pixels,
    /// Back and OK as icons (a chevron, a tick) rather than "←" and "OK".
    pub icons: bool,
}

pub const KEYS: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "Back", "0", "OK"];

/// The pad; `id` names its keys (`<id>-<key>`).
pub fn pin_pad(id: &'static str, look: PinLook, press: impl Fn(&'static str, &mut Window, &mut App) + 'static) -> Div {
    let press = Rc::new(press);
    let mut pad = div().flex().flex_col().gap(look.gap);
    for row in KEYS.chunks(3) {
        let mut r = div().flex().gap(look.gap);
        for &key in row {
            let p = press.clone();
            let face: AnyElement = match (key, look.icons) {
                ("Back", true) => IconName::ChevronLeft.into_any_element(),
                ("OK", true) => IconName::Check.into_any_element(),
                ("Back", false) => "\u{2190}".into_any_element(),
                (k, _) => k.into_any_element(),
            };
            r = r.child(div().id(SharedString::from(format!("{id}-{key}"))).w(look.key_w).h(look.key_h).flex().items_center().justify_center()
                .rounded(look.radius).border_1().border_color(look.edge).bg(look.bg).text_color(look.fg).text_size(look.text).cursor_pointer()
                .when_some(look.hover, |d, h| d.hover(move |s| s.bg(h)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, w, cx| { cx.stop_propagation(); p(key, w, cx) })
                .child(face));
        }
        pad = pad.child(r);
    }
    pad
}
