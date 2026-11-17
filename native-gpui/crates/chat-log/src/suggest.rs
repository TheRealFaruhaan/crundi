//! 2.17: suggestions (cc-sug: send, dismiss) and recaps (specs/suggestions-recaps.md).
//!
//! Both come from the conversation (`conv.suggestion`, `conv.recap`: the
//! server's history and its `suggestion` / `recap` events, crundi_core::chat)
//! and float in the pane's dock over the bottom right of the log, under the
//! agent badges (agents.rs draws the dock): the recap, then the suggestion,
//! always last. A click on the suggestion sends it as it is (the box and its
//! draft untouched); each ×, and any message sent, drops it here and on the
//! server.
use crate::LogView;
use crundi_chat_model::ChatModel;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Map, Value};

#[derive(Default)]
pub struct State {}

/// The recap's caption (shown in capitals).
pub fn recap_caption(why: &str) -> &'static str {
    if why == "resumed" { "WHERE THIS CHAT STANDS" } else { "WHILE YOU WERE AWAY" }
}

/// The suggestion shows only while the chat is idle and running.
pub fn shown(m: &ChatModel) -> bool {
    !m.conv.suggestion.is_empty() && m.connected && (m.conv.state.is_empty() || m.conv.state == "idle")
}

/// This feature's fields in the chat's props: the Slint client's `dock`
/// (with the side questions, 2.21).
pub fn props(v: &LogView, cx: &App, o: &mut Map<String, Value>) {
    let m = v.model.read(cx);
    o.insert("dock".into(), json!({
        "suggestion": m.conv.suggestion,
        "recap": m.conv.recap.as_ref().map(|r| json!({ "text": r.0, "why": r.1 })),
        "asides": v.st.aside.list.iter().map(|x| json!({ "id": x.id, "question": x.question, "state": x.state, "text": x.text })).collect::<Vec<_>>(),
    }));
}

/// This feature's driver actions (Some(answer) for a name it knows).
pub fn act(name: &str, _args: &[String], v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    match name {
        "sug-send" => send(v, cx),
        "sug-dismiss" => dismiss(&v.model, cx),
        "recap-dismiss" => recap_dismiss(&v.model, cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// The suggestion clicked: sent as it is (POST …/send, the composer's path;
/// the box untouched); it and the recap go at once; the log follows its end.
pub fn send(v: &mut LogView, cx: &mut Context<LogView>) {
    let text = v.model.update(cx, |m, cx| {
        let t = std::mem::take(&mut m.conv.suggestion);
        m.conv.recap = None;
        m.error.clear();
        cx.notify();
        t
    });
    if text.is_empty() { return; }
    v.stick = 3;
    post_send(&v.model, text, cx);
    cx.notify();
}

/// POST …/send (a test's stub "send" stands in for the server), a refusal
/// shown as the chat's error.
pub fn post_send(model: &Entity<ChatModel>, text: String, cx: &mut App) {
    let path = format!("/api/ui-sessions/{}/send", model.read(cx).conv.id);
    let stub = crundi_core::driver::take_stub("send");
    let me = model.downgrade();
    crundi_shell::call(cx, move |api| match stub { Some(v) => Ok(v), None => api.post(&path, &json!({ "text": text })) }, move |r, cx| {
        let _ = me.update(cx, |m, cx| {
            match r {
                Err(e) => m.error = e,
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => m.error = v.get("error").and_then(|e| e.as_str()).unwrap_or("Failed to send").to_string(),
                Ok(_) => {}
            }
            cx.notify();
        });
    });
}

/// The suggestion's ×: gone at once, then POST …/dismiss-suggestion (errors ignored).
pub fn dismiss(model: &Entity<ChatModel>, cx: &mut App) {
    let id = model.update(cx, |m, cx| { m.conv.suggestion.clear(); cx.notify(); m.conv.id.clone() });
    crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{id}/dismiss-suggestion"), &json!({})), |_, _| {});
}

/// The recap's × (and a side question asked): gone at once, then POST
/// …/dismiss-recap when there was one.
pub fn recap_dismiss(model: &Entity<ChatModel>, cx: &mut App) {
    let (had, id) = model.update(cx, |m, cx| { let had = m.conv.recap.take().is_some(); cx.notify(); (had, m.conv.id.clone()) });
    if !had { return; }
    crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{id}/dismiss-recap"), &json!({})), |_, _| {});
}

/// The dock's × (the suggestion's and the recap's): top right inside its box.
fn close_x(id: &'static str, cx: &mut Context<LogView>, f: fn(&Entity<ChatModel>, &mut App)) -> AnyElement {
    div().absolute().top(T::SP_4).right(T::SP_4)
        .child(w::icon_button(id, IconName::X, "Dismiss").on_click(cx.listener(move |v, _, _, cx| { cx.stop_propagation(); f(&v.model, cx); })))
        .into_any_element()
}

/// In the dock, under the badges: the recap, then the suggestion (None: nothing).
pub fn view(v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let m = v.model.read(cx);
    let z = px(m.zoom as f32);
    let recap = m.conv.recap.clone();
    let sug = if shown(m) { Some(m.conv.suggestion.clone()) } else { None };
    if recap.is_none() && sug.is_none() { return None; }
    let mut col = div().flex().flex_col().items_end().gap(T::SP_4).w_full();
    if let Some((text, why)) = recap {
        col = col.child(div().relative().w_full().mt(T::SP_4).pl(T::SP_12).pr(T::SP_24 + T::SP_6).py(T::SP_8)
            .rounded(T::R_FIELD).bg(T::bg_secondary()).border_1().border_color(T::border_subtle()).overflow_hidden().shadow_lg()
            .child(div().absolute().left_0().top_0().bottom_0().w(T::SP_4).rounded_l(T::R_FIELD).bg(T::accent()))
            .child(div().mb(T::SP_2).text_size(T::TEXT_11 + z).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child(recap_caption(&why)))
            .child(div().text_size(T::TEXT_12 + z).line_height(relative(T::LH_CHAT)).text_color(T::text_secondary()).child(text))
            .child(close_x("recap-x", cx, recap_dismiss)));
    }
    if let Some(text) = sug {
        col = col.child(div().flex().flex_col().items_end().gap(T::SP_2).mt(T::SP_4)
            .child(div().mr(T::SP_4).px(T::SP_4).rounded(T::R_CHIP).bg(T::bg()).text_size(T::TEXT_11 + z).text_color(T::text_muted()).child("Suggested, tap to send"))
            .child(div().id("sug").group("sug").relative().max_w_full().pl(T::SP_12).pr(T::SP_24 + T::SP_6).py(T::SP_8)
                .rounded(T::R_BUBBLE).rounded_br(T::R_CHIP).border_1().border_dashed().border_color(T::accent_edge()).bg(T::bg_secondary()).shadow_lg()
                .text_color(T::text_secondary()).line_height(relative(T::LH_CHAT)).cursor_pointer()
                .hover(|d| d.border_color(T::accent()).bg(T::accent_dim()).text_color(T::text()))
                .tooltip(w::tooltip("Tap to send"))
                .child(text)
                .on_click(cx.listener(|v, _, _, cx| send(v, cx)))
                .child(close_x("sug-x", cx, dismiss))));
    }
    Some(col.into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn captions() {
        assert_eq!(recap_caption("resumed"), "WHERE THIS CHAT STANDS");
        assert_eq!(recap_caption("idle"), "WHILE YOU WERE AWAY");
        assert_eq!(recap_caption(""), "WHILE YOU WERE AWAY");
    }
}
