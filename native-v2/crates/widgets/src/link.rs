//! A link-looking button (the web's .lr-link and inline "Open" links):
//! accent text, brighter and underlined on hover. (Made by operations;
//! Lead to review.)

use crundi_theme::T;
use gpui::*;

pub fn link(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    let t: SharedString = text.into();
    div().id(id).flex_none().cursor_pointer().text_size(T::TEXT_12).text_color(T::accent_hover())
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
