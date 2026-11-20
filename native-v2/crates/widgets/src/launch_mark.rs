//! The launcher's "last used" mark (the web's .term-launch .tl-last::after):
//! a 5px accent dot 5px from the top and 6px from the right of the button
//! this project launched last, ringed 2px in the panel's colour. Put it in
//! the button as its last child. Added by Workbench (AREAS.md, "A new control").

use crundi_theme::T;
use gpui::*;

pub fn launch_mark() -> Div {
    // The ring is the box's border, so the box is the dot plus 2px a side.
    div().absolute().top(T::SP_2 + px(1.)).right(T::SP_4).size(T::SP_8 + px(1.)).rounded_full()
        .border_2().border_color(T::bg_secondary()).bg(T::accent_hover())
}
