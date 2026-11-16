//! The one sizing core for every button-like control (the owner, 16 Nov:
//! "use the theme; buttons and such dimensions on the theme"). A control's
//! height, padding, gap, icon, label size and weight and radius all come
//! from the theme's control tokens (T::BTN_*, T::ICON_BTN*); gpui-component's
//! own size presets (.small(), .xsmall()…) are never used for a dimension.
//! The content (icon, label) is laid out here, at the token sizes.

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::{button::Button, Icon, Sizable as _};

/// The button's size class.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ctl {
    /// Every ordinary button (BTN_H).
    Normal,
    /// The big one (Send, a form's main action: BTN_H_LG).
    Large,
}

/// An icon at a token size, tinted by the text colour around it.
pub fn icon_of(icon: IconName, size: Pixels) -> Icon {
    Icon::new(icon).with_size(gpui_component::Size::Size(size))
}

/// Size `b` from the theme and give it its content: the icon (before, or
/// after the label when `trailing`: a dropdown's chevron) and the label.
/// No label: an icon button, square.
pub fn sized(b: Button, label: Option<SharedString>, icon: Option<IconName>, trailing: bool, ctl: Ctl) -> Button {
    let (h, pad, weight) = match ctl { Ctl::Normal => (T::BTN_H, T::BTN_PAD_X, T::BTN_WEIGHT), Ctl::Large => (T::BTN_H_LG, T::BTN_PAD_X_LG, T::BTN_WEIGHT_LG) };
    let b = b.rounded(T::BTN_RADIUS).flex_none();
    match label {
        None => {
            let side = if ctl == Ctl::Large { T::BTN_H_LG } else { T::ICON_BTN };
            b.w(side).h(side).px(px(0.)).when_some(icon, |b, i| b.child(icon_of(i, T::ICON_BTN_ICON)))
        }
        Some(l) => {
            let ic = icon.map(|i| icon_of(i, T::BTN_ICON));
            let text = div().text_size(T::BTN_TEXT).font_weight(weight).whitespace_nowrap().child(l);
            let row = div().flex().items_center().justify_center().gap(T::BTN_GAP).min_w_0();
            let row = if trailing { row.child(text).when_some(ic, |r, i| r.child(i)) } else { row.when_some(ic, |r, i| r.child(i)).child(text) };
            b.h(h).px(pad).child(row)
        }
    }
}

/// The label text alone at a size (Send scales with the pane's text).
pub fn sized_text(b: Button, label: SharedString, text: Pixels, ctl: Ctl) -> Button {
    let (h, pad, weight) = match ctl { Ctl::Normal => (T::BTN_H, T::BTN_PAD_X, T::BTN_WEIGHT), Ctl::Large => (T::BTN_H_LG, T::BTN_PAD_X_LG, T::BTN_WEIGHT_LG) };
    b.rounded(T::BTN_RADIUS).flex_none().h(h).px(pad)
        .child(div().flex().items_center().justify_center().min_w_0().text_size(text).font_weight(weight).whitespace_nowrap().child(label))
}
