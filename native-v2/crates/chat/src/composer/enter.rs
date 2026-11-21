//! The Enter switch (the web's .cc-toggle, `crundi_chat_enter`): which key
//! sends, in every chat. One setting, saved in the client's settings
//! (`Saved.chat_enter`: "newline" for Ctrl+Enter), held by one entity every
//! composer observes, so a click in one pane switches them all at once (the
//! intent of the web's "Shared by every chat cell"; the web applies it to a
//! pane when the pane is built).

use crundi_core::store;
use crundi_theme::IconName;
use crundi_widgets as w;
use gpui::*;

/// false: Enter sends (Shift+Enter a newline); true: Ctrl/Cmd+Enter sends
/// (Enter a newline).
pub struct EnterMode { pub ctrl: bool }

struct Handle(Entity<EnterMode>);
impl Global for Handle {}

/// The setting's entity (made, from the saved settings, the first time).
pub fn mode(cx: &mut App) -> Entity<EnterMode> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let ctrl = store::load().chat_enter == "newline";
    let e = cx.new(|_| EnterMode { ctrl });
    cx.set_global(Handle(e.clone()));
    e
}

/// Switch it: saved, and every composer follows (they observe it).
pub fn set(cx: &mut App, ctrl: bool) {
    let e = mode(cx);
    e.update(cx, |m, cx| if m.ctrl != ctrl { m.ctrl = ctrl; cx.notify(); });
    store::update(|s| s.chat_enter = if ctrl { "newline".into() } else { String::new() });
}

/// The box's placeholder: the hint per mode; narrow, none; exited, "Session ended.".
pub fn placeholder(ctrl: bool, narrow: bool, exited: bool) -> &'static str {
    if exited { "Session ended." }
    else if narrow { "Message Claude…" }
    else if ctrl { "Message Claude…  (Ctrl+Enter to send, Enter for newline)" }
    else { "Message Claude…  (Enter to send, Shift+Enter for newline)" }
}

/// The switch: [ ⏎ | ⌃⏎ ] and "sends"; a click anywhere on it swaps (the
/// web's .cc-toggle is one button: either side, or the word) and puts the
/// focus back in the box (`refocus`).
pub fn switch(ctrl: bool, refocus: impl Fn(&mut Window, &mut App) + 'static) -> AnyElement {
    let tip = if ctrl { "Enter makes a newline, Ctrl+Enter sends — click to swap" } else { "Enter sends, Shift+Enter makes a newline — click to swap" };
    w::tip(w::icon_switch("enter-switch", &[&[IconName::CornerDownLeft], &[IconName::ChevronUp, IconName::CornerDownLeft]], if ctrl { 1 } else { 0 }, Some("sends".into()))
        .on_click(move |_, window, cx| { set(cx, !ctrl); refocus(window, cx); }), tip)
        .into_any_element()
}
