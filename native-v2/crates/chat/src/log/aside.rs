//! 2.21: side questions (the web's askAside, `.cc-aside`): `/btw <question>`
//! from the box is asked on the side (POST …/aside), in any state, never
//! queued: a card in the log after the entry that was last when it was
//! asked ("ON THE SIDE", ×), "Asking…" until the answer (markdown, as a
//! reply) or the error comes, then "Not added to the conversation." Local
//! only: × closes a card, a restart loses them. A server without the route
//! (404): the card goes and the text is sent as typed.
//!
//! The composer asks `ask_from_box` before it sends (true: it was a side
//! question, asked here). The cards are the chat's (`Shared::asides`) and
//! are drawn under the last row of the entry they follow.

use super::LogView;
use crate::store::{ChatStore, Row};
use crundi_core::md;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Aside {
    pub id: u32,
    /// The entry that was last when it was asked ("" for an empty log).
    pub anchor: String,
    pub question: String,
    /// "pending" | "done" | "error".
    pub state: String,
    pub text: String,
}

/// What a POST …/aside answer makes of the card: Some((state, text)), or
/// None for a server without side questions (the card goes).
pub fn outcome(r: Result<Value, String>) -> Option<(&'static str, String)> {
    let s = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    match r {
        Err(e) if e == "Not supported by this server" => None,
        Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => Some(("done", s(&v, "answer"))),
        Ok(v) => Some(("error", { let e = s(&v, "error"); if e.is_empty() { "No answer".to_string() } else { e } })),
        Err(e) => Some(("error", e)),
    }
}

/// The composer's send, first: `/btw <question>` is asked here (true: the
/// composer clears its box and sends nothing); anything else is the
/// composer's to send (false).
pub fn ask_from_box(cx: &mut App, chat: &str, text: &str) -> bool {
    let Some(q) = md::btw(text) else { return false };
    let Some(store) = crate::find(cx, chat) else { return false };
    ask(&store, text.trim_end().to_string(), q, cx);
    true
}

/// Ask on the side: the recap dismissed, a card after the last entry, every
/// log of the chat to its end, then POST …/aside.
pub fn ask(store: &Entity<ChatStore>, text: String, question: String, cx: &mut App) {
    let (chat, anchor, n) = store.update(cx, |st, _| {
        st.log.aside_next += 1;
        let anchor = st.conv.entries.last().and_then(|e| e.get("id")).and_then(|i| i.as_str()).unwrap_or("").to_string();
        let n = st.log.aside_next;
        st.log.asides.push(Aside { id: n, anchor: anchor.clone(), question: question.clone(), state: "pending".into(), text: String::new() });
        for l in st.all_lists() { l.set_follow_mode(FollowMode::Tail); }
        (st.id.clone(), anchor, n)
    });
    super::suggest::recap_dismiss(store, cx);
    super::shared::remeasure_after(store, &anchor, cx);
    let path = format!("/api/ui-sessions/{chat}/aside");
    let store = store.clone();
    crundi_shell::call(cx, move |api| api.post(&path, &json!({ "question": question })), move |r, cx| {
        match outcome(r) {
            None => {
                store.update(cx, |st, _| st.log.asides.retain(|x| x.id != n));
                super::shared::remeasure_after(&store, &anchor, cx);
                super::suggest::post_send(&store, text, cx);
            }
            Some((state, out)) => {
                store.update(cx, |st, _| if let Some(x) = st.log.asides.iter_mut().find(|x| x.id == n) { x.state = state.into(); x.text = out; });
                super::shared::remeasure_after(&store, &anchor, cx);
            }
        }
    });
}

/// A card's ×: it goes at once (an answer still coming is dropped).
pub fn close(store: &Entity<ChatStore>, id: u32, cx: &mut App) {
    let anchor = store.update(cx, |st, _| { let a = st.log.asides.iter().find(|x| x.id == id).map(|x| x.anchor.clone()); st.log.asides.retain(|x| x.id != id); a });
    if let Some(a) = anchor { super::shared::remeasure_after(store, &a, cx); }
}

/// The cards under row `ix`: those after its entry, when it is the entry's
/// last row (and, under the log's last row, those whose entry is gone).
pub fn under(v: &mut LogView, ix: usize, r: &Row, window: &mut Window, cx: &mut Context<LogView>) -> Vec<AnyElement> {
    let st = v.store.read(cx);
    if st.log.asides.is_empty() { return vec![]; }
    let last_of_entry = st.span_of(&r.id).is_some_and(|(s, n)| ix + 1 == s + n);
    let last = ix + 1 == st.rows.len();
    let list: Vec<Aside> = st.log.asides.iter().filter(|a| (last_of_entry && a.anchor == r.id) || (last && !a.anchor.is_empty() && st.span_of(&a.anchor).is_none())).cloned().collect();
    list.iter().map(|a| card(v, a, window, cx)).collect()
}

/// The cards asked in an empty log (no row to follow).
pub fn orphans(v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Vec<AnyElement> {
    let list: Vec<Aside> = v.store.read(cx).log.asides.iter().filter(|a| a.anchor.is_empty()).cloned().collect();
    if list.is_empty() { return vec![]; }
    vec![div().absolute().top_0().left_0().right_0().p(T::SP_16).flex().flex_col().gap(T::SP_10).children(list.iter().map(|a| card(v, a, window, cx))).into_any_element()]
}

/// A side question's card: its head (ON THE SIDE, ×), the question, then
/// "Asking…", the answer or the error box; the note.
fn card(v: &mut LogView, a: &Aside, _window: &mut Window, _cx: &mut Context<LogView>) -> AnyElement {
    let fs = v.fs();
    let id = a.id;
    let answer = match a.state.as_str() {
        "done" => { let b = v.ui.md.blocks(&format!("aside-{id}"), &a.text); div().flex().flex_col().children(super::md::draw(&b, fs, T::text_secondary())).into_any_element() }
        "error" => super::prose::error_box(fs).child(a.text.clone()).into_any_element(),
        _ => div().flex().items_center().gap(T::SP_6).text_size(T::TEXT_13 * fs).text_color(T::text_secondary()).child(w::spinner(T::ICON_SM * fs)).child("Asking…").into_any_element(),
    };
    let store = v.store.clone();
    div().flex().flex_col().px(T::SP_10).py(T::SP_8).rounded(T::R_CONTROL).border_1().border_dashed().border_color(T::border()).bg(T::bg_secondary())
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().flex_1().min_w_0().text_size(T::TEXT_11 * fs).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child("ON THE SIDE"))
            .child(w::icon_button(SharedString::from(format!("aside-x-{id}")), IconName::X, "Close").on_click(move |_, _, cx| close(&store, id, cx))))
        .child(div().mt(T::SP_4).mb(T::SP_6).text_size(T::TEXT_13 * fs).text_color(T::text()).line_height(relative(T::LH_CHAT)).child(a.question.clone()))
        .child(answer)
        .child(div().mt(T::SP_6).text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child("Not added to the conversation."))
        .into_any_element()
}

pub fn state(v: &LogView, cx: &App) -> Value {
    json!(v.store.read(cx).log.asides.iter().map(|x| json!({ "id": x.id, "anchor": x.anchor, "question": x.question, "state": x.state, "text": x.text })).collect::<Vec<_>>())
}

pub fn act(name: &str, args: &[String], v: &mut LogView, cx: &mut Context<LogView>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "aside-close" => close(&v.store.clone(), a(0).parse().unwrap_or(0), cx),
        "aside-ask" => {
            let t = a(0);
            let Some(q) = md::btw(&t) else { return Some(json!({ "ok": false, "error": "not a side question" })) };
            ask(&v.store.clone(), t.trim_end().to_string(), q, cx);
        }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn outcomes() {
        assert_eq!(outcome(Ok(json!({ "ok": true, "answer": "Writing." }))), Some(("done", "Writing.".into())));
        assert_eq!(outcome(Ok(json!({ "ok": false }))), Some(("error", "No answer".into())));
        assert_eq!(outcome(Err("Not supported by this server".into())), None);
        assert_eq!(outcome(Err("Failed to fetch".into())), Some(("error", "Failed to fetch".into())));
    }
}
