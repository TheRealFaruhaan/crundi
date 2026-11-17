//! A labelled button whose icon may come after its label (the web's
//! "Skip ×", "Next ›"), at the ordinary or the large control size. The
//! large primary one carries the accent ring the web's focused main
//! action shows (What's new's Next). Sizes and colours are the theme's.

use crate::control::{icon_of, Ctl};
use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::button::{Button, ButtonVariants as _};

/// The button's look.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    /// The quiet, outlined one (Back, Skip).
    Quiet,
    /// The accent one (Next, Done); large: with its ring.
    Primary,
}

/// `icon` before the label, or after it when `trailing`; `large`: the
/// large control size (BTN_H_LG, CONTROL_LG high).
pub fn label_icon_button(id: impl Into<ElementId>, label: impl Into<SharedString>, icon: IconName, trailing: bool, look: Look, large: bool) -> Button {
    let ctl = if large { Ctl::Large } else { Ctl::Normal };
    let (h, pad, weight, text, radius) = match ctl {
        Ctl::Normal => (T::BTN_H, T::BTN_PAD_X, T::BTN_WEIGHT, T::BTN_TEXT, T::BTN_RADIUS),
        Ctl::Large => (T::CONTROL_LG, T::SP_16, T::W_SEMIBOLD, T::TEXT_14, T::R_BUBBLE),
    };
    let ic = icon_of(icon, T::BTN_ICON);
    let label: SharedString = label.into();
    let inner = div().flex().items_center().justify_center().gap(T::BTN_GAP).min_w_0().text_size(text).font_weight(weight).whitespace_nowrap()
        .when(!trailing, |d| d.child(ic.clone()))
        .child(label)
        .when(trailing, |d| d.child(ic));
    let b = Button::new(id);
    let b = if look == Look::Primary { b.primary() } else { b };
    b.rounded(radius).flex_none().h(h).px(pad).child(inner)
        // The large primary (the tour's Next and Done) as the web's .wn-btn.primary:
        // its accent glow under it, and the focus ring it has from opening.
        .when(look == Look::Primary && large, |b| b.shadow(vec![
            BoxShadow { color: T::accent().opacity(0.35), offset: point(px(0.), T::SP_8), blur_radius: T::SP_24, spread_radius: px(0.), inset: false },
            // The focus ring as the PC client shows it (the browser's own):
            // a light ring outside a dark gap.
            BoxShadow { color: T::text(), offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: T::SP_4, inset: false },
            BoxShadow { color: T::bg(), offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: T::SP_2, inset: false },
        ]))
}
