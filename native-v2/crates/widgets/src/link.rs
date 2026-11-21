//! A link-looking button (the web's .lr-link and inline "Open" links):
//! accent text, brighter and underlined on hover. (Made by operations;
//! Lead to review.)

use crundi_theme::T;
use gpui::*;

pub fn link(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> { link_sized(id, text, T::TEXT_12) }

/// `link` at a text size of the caller's (a chat's text follows its zoom;
/// a frame's centred "Open" is 13 px).
pub fn link_sized(id: impl Into<ElementId>, text: impl Into<SharedString>, size: Pixels) -> Stateful<Div> {
    let t: SharedString = text.into();
    div().id(id).flex_none().cursor_pointer().text_size(size).text_color(T::accent_hover())
        .hover(|d| d.text_color(T::accent()).underline())
        .child(t.clone()).child(crate::find::mark(t))
}

/// A quiet link (the sign-in page's "I was invited to a project" and "Back
/// to the normal sign-in", #collab-login-link): muted, accent on hover.
pub fn quiet_link(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    let t: SharedString = text.into();
    div().id(id).flex_none().cursor_pointer().text_size(T::TEXT_12).text_color(T::text_muted())
        .hover(|d| d.text_color(T::accent_hover()))
        .child(t.clone()).child(crate::find::mark(t))
}

/// A muted link that goes to the primary text on hover (the chat's agent
/// sheet's "‹ Agents" back link, .cc-agpanel-back: 12 px times the chat's
/// zoom, 6 px of room on its right).
pub fn muted_link(id: impl Into<ElementId>, text: impl Into<SharedString>, size: Pixels) -> Stateful<Div> {
    let t: SharedString = text.into();
    div().id(id).flex_none().cursor_pointer().whitespace_nowrap().pr(T::SP_6).text_size(size).text_color(T::text_muted())
        .hover(|d| d.text_color(T::text()))
        .child(t.clone()).child(crate::find::mark(t))
}
