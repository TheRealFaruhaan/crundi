//! A new pane's launch buttons (the web's .term-launch buttons): Empty
//! Shell, UI Mode (the hero: the indigo gradient, its block caret), Terminal
//! and the two Skip Permissions shields (icon only at rest; open, they name
//! themselves while their row-mate gives up its label, so the row's width
//! stays the same). Added by Workbench (AREAS.md, "A new control").
//!
//! UI Mode's gradient is drawn still, and its caret does not blink: the
//! web's drift (9 s, forever) would draw frames while nothing changes
//! (ARCHITECTURE.md rule 6); a question for the owner (PLAN.md).

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LaunchLook { Shell, Chat, Normal, Shield }

/// One launch button: `label` shown or folded away (a shield at rest, a
/// main button while its shield is open); `busy` spins in place of its
/// icon; `off`: another launch is under way.
pub fn launch_button(id: impl Into<ElementId>, look: LaunchLook, icon: Option<IconName>, label: &'static str, show_label: bool, busy: bool, off: bool) -> Stateful<Div> {
    let base = div().id(id).flex().items_center().justify_center().overflow_hidden().whitespace_nowrap()
        .py(T::LAUNCH_PY).rounded(T::R_CONTROL).border_1().text_size(T::TEXT_14)
        .when(!off, |d| d.cursor_pointer())
        .when(off && !busy, |d| d.opacity(0.5));
    let b = match look {
        LaunchLook::Shell => base.min_w(T::LAUNCH_MIN_W).px(T::SP_24).border_color(T::border()).text_color(T::text_secondary())
            .when(!off, |d| d.hover(|s| s.text_color(T::text()).border_color(T::accent()))),
        LaunchLook::Chat => base.flex_1().min_w_0().px(T::SP_12).border_color(T::launch_chat_edge()).text_color(T::white()).font_weight(T::W_SEMIBOLD)
            .bg(linear_gradient(110., linear_color_stop(T::accent_deep(), 0.), linear_color_stop(T::accent_hover(), 1.)))
            .when(!off, |d| d.hover(|s| s.border_color(T::accent_hover()))),
        LaunchLook::Normal => base.flex_1().min_w_0().px(T::SP_12).border_color(T::accent_edge()).bg(T::launch_normal_bg()).text_color(T::text())
            .when(!off, |d| d.hover(|s| s.border_color(T::accent()).bg(T::accent_dim()))),
        LaunchLook::Shield => base.flex_none().w(if show_label { T::LAUNCH_SHIELD_OPEN } else { T::LAUNCH_SHIELD }).border_color(T::border()).bg(T::bg_tertiary()).text_color(T::text())
            .when(!off, |d| d.hover(|s| s.border_color(T::accent()).bg(T::accent_dim()))),
    };
    let mark: Option<AnyElement> = if busy { Some(crate::spinner(T::ICON_MD).into_any_element()) }
        else if look == LaunchLook::Chat { Some(div().font_family(T::MONO).child("\u{258d}").into_any_element()) }
        else { icon.map(|i| div().flex_none().child(crate::icon(i, T::ICON_MD)).into_any_element()) };
    let gap = if look == LaunchLook::Chat { T::LAUNCH_CHAT_GAP } else { T::LAUNCH_GAP };
    b.children(mark).when(show_label, |d| d.child(div().when(look != LaunchLook::Shell, |d| d.ml(gap)).child(label)))
}
