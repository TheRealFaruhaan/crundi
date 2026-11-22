//! A pane header's button (the web's .term-head button, the Slint client's
//! HeadBtn): a small bordered box with an icon or a short label, in one of
//! the header's looks. Workspace's request (shared/requests.md): the
//! workbench's header, its narrow "…" menu and the empty pane's buttons.

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};

/// What a header button shows.
pub enum HeadContent {
    Icon(IconName),
    Text(SharedString),
    /// An icon with a count or a word after it (the secrets lock).
    IconText(IconName, SharedString),
}

/// rest | on (yellow: secrets granted) | pinned (accent fill) | lit
/// (accent: full screen, an open menu) | armed (red: "Sure?") | close (the
/// ×: red on hover).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeadLook { Rest, On, Pinned, Lit, Armed, Close }

/// A header button, `w` wide and `h` tall, with its tooltip.
pub fn head_button(id: impl Into<ElementId>, content: HeadContent, look: HeadLook, w: Pixels, h: Pixels, tip: impl Into<SharedString>) -> Stateful<Div> {
    let tip: SharedString = tip.into();
    let (ink, edge, fill) = match look {
        HeadLook::On => (T::yellow(), T::yellow(), T::bg()),
        HeadLook::Armed => (T::red(), T::red(), T::red_dim()),
        HeadLook::Pinned => (T::accent_hover(), T::accent(), T::accent_dim()),
        HeadLook::Lit => (T::accent_hover(), T::accent(), T::bg()),
        HeadLook::Rest | HeadLook::Close => (T::text_secondary(), T::border(), T::bg()),
    };
    let body = match content {
        HeadContent::Icon(i) => div().text_size(T::ICON_SM).child(i),
        HeadContent::Text(t) => div().text_size(if t.as_ref() == "×" { T::TEXT_14 } else { T::TEXT_11 }).child(t),
        HeadContent::IconText(i, t) => div().flex().items_center().gap(T::SP_4).child(div().text_size(T::ICON_SM).child(i)).child(div().text_size(T::TEXT_11).child(t)),
    };
    // The tip stays away after a press until the pointer leaves (w::tip, F213).
    let el = div()
        .id(id).flex().flex_none().items_center().justify_center().w(w).h(h)
        .rounded(T::R_CONTROL).border_1().border_color(edge).bg(fill).text_color(ink)
        .cursor_pointer()
        .when(look == HeadLook::Close, |d| d.hover(|s| s.border_color(T::red()).text_color(T::red())))
        .when(look == HeadLook::Rest, |d| d.hover(|s| s.border_color(T::accent()).text_color(T::text())));
    crate::tip(el, tip).child(body)
}

/// One row of a menu (the "+" menu, a narrow header's "…" menu): its icon,
/// its label, a caret when it opens a submenu; `lit` while that is open.
pub fn menu_item(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>, caret: bool, lit: bool) -> Stateful<Div> {
    div()
        .id(id).flex().flex_none().items_center().gap(T::SP_10).h(T::SP_24 + T::SP_6 + px(1.)).px(T::SP_12)
        .rounded(T::R_CONTROL).text_size(T::TEXT_14).cursor_pointer()
        .text_color(if lit { T::text() } else { T::text_secondary() })
        .when(lit, |d| d.bg(T::bg_hover()))
        .hover(|s| s.bg(T::bg_hover()).text_color(T::text()))
        .child(div().flex_none().text_size(T::ICON_LG).child(icon))
        .child(div().flex_1().whitespace_nowrap().child(label.into()))
        .when(caret, |d| d.child(div().flex_none().text_size(T::ICON_SM).text_color(T::text_muted()).child(IconName::ChevronRight)))
}

/// A menu's frame: the card, its shadow, padding round its rows.
pub fn menu_panel(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id).occlude().flex().flex_col().gap(T::SP_2).p(T::SP_6)
        .bg(T::bg_secondary()).border_1().border_color(T::border()).rounded(T::R_CARD).shadow_lg()
}
