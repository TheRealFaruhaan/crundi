//! A hover tooltip for any stateful element (a card's head, a code block's
//! copy button): `.tooltip(tip("Copy code"))`. (Made by operations; Lead to review.)
use gpui::{AnyView, App, SharedString, Window};
use gpui_component::tooltip::Tooltip;

pub fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |w, cx| Tooltip::new(text.clone()).build(w, cx)
}
