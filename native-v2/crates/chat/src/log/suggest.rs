//! 2.17: the suggestion (`.cc-sug`: "Suggested, tap to send"; a click sends
//! it as it is, its × dismisses it) and the recap (`.cc-recap`: "While you
//! were away", or "Where this chat stands" for a resumed chat; its ×). Both
//! are the conversation's (`conv.suggestion`, `conv.recap`) and float in the
//! dock over the bottom right of the log, under the agent badges
//! (agents.rs draws the dock): the recap, then the suggestion, last.

use super::LogView;
use crate::store::ChatStore;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::*;
use serde_json::json;

/// The recap's caption (drawn in capitals).
pub fn recap_caption(why: &str) -> &'static str { if why == "resumed" { "WHERE THIS CHAT STANDS" } else { "WHILE YOU WERE AWAY" } }

/// The suggestion shows only while the chat is idle.
pub fn shown(st: &ChatStore) -> bool { !st.conv.suggestion.is_empty() && (st.conv.state.is_empty() || st.conv.state == "idle") }

/// POST …/send, a refusal shown as the chat's error (the composer's path;
/// the box untouched).
pub fn post_send(store: &Entity<ChatStore>, text: String, cx: &mut App) {
    let path = format!("/api/ui-sessions/{}/send", store.read(cx).id);
    let store = store.clone();
    crundi_shell::call(cx, move |api| api.post(&path, &json!({ "text": text })), move |r, cx| {
        let err = match r {
            Err(e) => Some(e),
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(v.get("error").and_then(|e| e.as_str()).unwrap_or("Failed to send").to_string()),
            Ok(_) => None,
        };
        if let Some(e) = err { super::shared::set_error(&store, e, cx); }
    });
}

/// The suggestion clicked: sent as it is (the web's sendSuggestion); the log follows its end.
pub fn send(v: &mut LogView, cx: &mut Context<LogView>) {
    let text = v.store.read(cx).conv.suggestion.clone();
    if text.is_empty() { return; }
    // (The web sets the chat working at once: a working chat shows no suggestion.)
    super::shared::local(&v.store, json!({ "type": "state", "state": "working" }), cx);
    v.list.set_follow_mode(FollowMode::Tail);
    post_send(&v.store, text, cx);
    cx.notify();
}

/// The suggestion's ×: gone at once, then POST …/dismiss-suggestion (errors ignored).
pub fn dismiss(store: &Entity<ChatStore>, cx: &mut App) {
    super::shared::local(store, json!({ "type": "suggestion", "suggestion": null }), cx);
    let id = store.read(cx).id.clone();
    crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{id}/dismiss-suggestion"), &json!({})), |_, _| {});
}

/// The recap's × (and a side question asked): gone at once, then POST …/dismiss-recap when there was one.
pub fn recap_dismiss(store: &Entity<ChatStore>, cx: &mut App) {
    if store.read(cx).conv.recap.is_none() { return; }
    super::shared::local(store, json!({ "type": "recap", "recap": null }), cx);
    let id = store.read(cx).id.clone();
    crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{id}/dismiss-recap"), &json!({})), |_, _| {});
}

/// A × in the top right inside its box.
fn close_x(id: &'static str, f: impl Fn(&mut App) + 'static) -> AnyElement {
    div().absolute().top(T::SP_4).right(T::SP_4)
        .child(w::icon_button(id, IconName::X, "Dismiss").on_click(move |_, _, cx| { cx.stop_propagation(); f(cx); }))
        .into_any_element()
}

/// In the dock, under the badges: the recap, then the suggestion (None: neither).
pub fn view(v: &mut LogView, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let fs = v.fs();
    let st = v.store.read(cx);
    let recap = st.conv.recap.clone();
    let sug = shown(st).then(|| st.conv.suggestion.clone());
    if recap.is_none() && sug.is_none() { return None; }
    let mut col = div().flex().flex_col().items_end().gap(T::SP_4).w_full();
    if let Some((text, why)) = recap {
        let store = v.store.clone();
        col = col.child(div().id("recap").occlude().relative().w_full().mt(T::SP_4).pl(T::SP_12).pr(T::SP_24 + T::SP_6).py(T::SP_8)
            .rounded(T::R_FIELD).bg(T::bg_secondary()).border_1().border_color(T::border_subtle()).overflow_hidden().shadow_lg()
            .child(div().absolute().left_0().top_0().bottom_0().w(T::SP_4).bg(T::accent()))
            .child(div().mb(T::SP_2).text_size(T::TEXT_11 * fs).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child(recap_caption(&why)))
            .child(div().text_size(T::TEXT_12 * fs).line_height(relative(T::LH_CHAT)).text_color(T::text_secondary()).child(text))
            .child(close_x("recap-x", move |cx| recap_dismiss(&store, cx))));
    }
    if let Some(text) = sug {
        let store = v.store.clone();
        let spot = v.spot("sug", cx);
        col = col.child(div().flex().flex_col().items_end().gap(T::SP_2).mt(T::SP_4)
            .child(w::suggest_caption(fs))
            .child(w::suggest_bubble("sug", fs)
                .tooltip(w::tooltip("Tap to send"))
                .child(text)
                .children(spot)
                .on_click(cx.listener(|v, _, _, cx| send(v, cx)))
                .child(close_x("sug-x", move |cx| dismiss(&store, cx)))));
    }
    Some(col.into_any_element())
}

#[cfg(test)]
mod tests {
    #[::core::prelude::v1::test]
    fn captions() {
        assert_eq!(super::recap_caption("resumed"), "WHERE THIS CHAT STANDS");
        assert_eq!(super::recap_caption("idle"), "WHILE YOU WERE AWAY");
    }
}
