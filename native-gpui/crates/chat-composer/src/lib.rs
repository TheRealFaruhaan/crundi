//! A chat's composer (Planning's, phase 2: 2.5-2.12b and voice 2.24;
//! AREAS.md): the message box, Send, and the meta row (state, the
//! permission mode, the Enter switch, schedule, session id, model, cost).
//!
//! The foundation's version: the box (Enter or Ctrl+Enter sends, the switch
//! works), Send, and the meta row's parts. Stop arming, the mode picker,
//! attachments, drafts, the queue, scheduled messages, the slash menu, the
//! secrets dialog and dock, and voice input are 2.5-2.12b and 2.24.

use crundi_chat_model::ChatModel;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::input::{InputEvent, Textarea, TextareaState};

pub struct ComposerView {
    model: Entity<ChatModel>,
    input: Entity<TextareaState>,
    /// Enter sends (true) or Ctrl+Enter sends.
    pub enter_sends: bool,
    _sub: Subscription,
}

impl ComposerView {
    pub fn new(model: Entity<ChatModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 8).placeholder("Message Claude…"));
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { shift: false, secondary } = ev { if this.enter_sends != *secondary { this.send(window, cx); } }
        });
        ComposerView { model, input, enter_sends: true, _sub: sub }
    }

    /// Send what is in the box (the Send button, Enter, the test driver's "send").
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if text.is_empty() { return; }
        self.input.update(cx, |st, cx| st.set_value("", window, cx));
        self.model.update(cx, |m, cx| m.send(text, cx));
    }

    /// Put text in the box (the driver, drafts).
    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |st, cx| st.set_value(text.to_string(), window, cx));
    }

    pub fn text(&self, cx: &App) -> String { self.input.read(cx).value().to_string() }
}

/// The permission mode as the meta row's chip says it.
pub fn mode_label(m: &str) -> &'static str {
    match m {
        "bypassPermissions" => "Bypass all",
        "acceptEdits" => "Accept edits",
        "plan" => "Plan",
        "auto" => "Auto",
        "dontAsk" => "Don't ask",
        _ => "Ask permissions",
    }
}

impl Render for ComposerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let m = self.model.read(cx);
        let state = if !m.connected { "offline".to_string() } else if m.conv.state.is_empty() { "idle".into() } else { m.conv.state.clone() };
        let (mode, sid, model, cost, err) = (m.conv.mode.clone(), m.conv.session_id.clone(), m.conv.model.clone(), m.conv.cost, m.error.clone());
        div().flex().flex_col().gap(T::SP_6).p(T::SP_8).border_t_1().border_color(T::border())
            .child(div().flex().items_end().gap(T::SP_8)
                .child(div().flex_1().child(Textarea::new(&self.input)))
                .child(w::primary("send", "Send").on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))))
            .child(div().flex().items_center().gap(T::SP_8).text_size(T::TEXT_11).text_color(T::text_muted())
                .child(state)
                .child(w::chip(IconName::ShieldCheck, mode_label(&mode), mode == "bypassPermissions"))
                .child(w::segmented("enter", &["Enter", "Ctrl+Enter"], if self.enter_sends { 0 } else { 1 }, cx.listener(|this, i: &usize, _, cx| { this.enter_sends = *i == 0; cx.notify(); })))
                .child("sends")
                .child(w::icon_button("schedule", IconName::Clock, "Schedule a message"))
                .when(!err.is_empty(), |d| d.child(div().text_color(T::error_text()).child(err)))
                .child(div().flex_1())
                .when(!sid.is_empty(), |d| d.child(div().font_family(T::MONO).child(sid.chars().take(8).collect::<String>())))
                .when(!model.is_empty(), |d| d.child(model))
                .when(cost > 0.0, |d| d.child(format!("${cost:.4}"))))
    }
}
