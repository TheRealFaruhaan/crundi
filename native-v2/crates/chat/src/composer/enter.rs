//! The Enter switch (the web's .cc-toggle, `crundi_chat_enter`): which key
//! sends, in every chat. One setting, saved in the client's settings
//! (`Saved.chat_enter`: "newline" for Ctrl+Enter), held by one entity every
//! composer observes, so a click in one pane switches them all at once (the
//! intent of the web's "Shared by every chat cell"; the web applies it to a
//! pane when the pane is built).

use crundi_core::store;
use crundi_theme::{IconName, T};
use gpui::prelude::FluentBuilder as _;
use gpui_component::Icon;
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
    w::tip(icon_switch(if ctrl { 1 } else { 0 })
        .on_click(move |_, window, cx| { set(cx, !ctrl); refocus(window, cx); }), tip)
        .into_any_element()
}

/// w::icon_switch's look, with the web's hover (.cc-toggle:hover): the word
/// and the option not in use brighten to the primary text (F213).
fn icon_switch(on: usize) -> Stateful<Div> {
    let opts: [&[IconName]; 2] = [&[IconName::CornerDownLeft], &[IconName::ChevronUp, IconName::CornerDownLeft]];
    let mut track = div().flex().items_center().p(T::SP_2).gap(T::SP_2).rounded(T::R_PILL).bg(T::bg_card()).border_1().border_color(T::border());
    for (i, icons) in opts.iter().enumerate() {
        let mut o = div().flex().items_center().px(T::SP_6).py(T::SP_2).rounded(T::R_PILL)
            .when(i == on, |d| d.bg(T::accent_dim()).text_color(T::accent_hover()))
            .when(i != on, |d| d.text_color(T::text_secondary()).group_hover("enter-switch", |d| d.text_color(T::text())));
        for ic in icons.iter() { o = o.child(Icon::new(*ic).size(T::ICON_SM)); }
        track = track.child(o);
    }
    div().id("enter-switch").group("enter-switch").flex().items_center().gap(T::SP_4).cursor_pointer().text_color(T::text_secondary()).hover(|d| d.text_color(T::text()))
        .child(track)
        .child("sends")
}
