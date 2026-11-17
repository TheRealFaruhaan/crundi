//! 2.21: side questions (cc-aside, asked while a turn runs; specs/side-questions.md).
//!
//! `/btw <question>` from the box is asked on the side (POST …/aside), in
//! any state, never queued: a card in the log after the entry that was last
//! when it was asked (rows::rows_of places it), "Asking…" until the answer
//! (markdown) or the error comes. Local only: nothing reaches the
//! conversation, × closes a card, a restart loses them. A server without the
//! route (404): the card goes and the text is sent as typed.
//!
//! The composer calls `ask_from_box` before it sends (shared/requests.md).
use crate::{rows::{split_blocks, Aside, Row}, suggest, LogView};
use crundi_core::{api::ApiError, md};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Map, Value};

#[derive(Default)]
pub struct State {
    /// The chat's side questions, in the order asked.
    pub list: Vec<Aside>,
    /// The last card's number (ids are per chat, from 1).
    pub next: u32,
}

/// What a POST …/aside answer makes of the card: Some((state, text)), or
/// None for a server without side questions (404: the card goes).
pub fn outcome(r: Result<(u16, Value), String>) -> Option<(&'static str, String)> {
    let s = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    match r {
        Ok((404, _)) => None,
        Ok((_, v)) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => Some(("done", s(&v, "answer"))),
        Ok((_, v)) => Some(("error", { let e = s(&v, "error"); if e.is_empty() { "No answer".to_string() } else { e } })),
        Err(e) => Some(("error", e)),
    }
}

/// The composer's send, first: a side question (`/btw <question>`) is asked
/// here (true: the composer clears its box and sends nothing); anything else
/// is the composer's to send (false).
pub fn ask_from_box(cx: &mut App, chat: &str, text: &str) -> bool {
    let Some(q) = md::btw(text) else { return false };
    let Some(v) = crate::view_of(cx, chat) else { return false };
    let text = text.trim_end().to_string();
    v.update(cx, |v, cx| ask(v, text, q, cx));
    true
}

/// Ask on the side: the recap dismissed, a card after the last entry, the
/// log to its end, then POST …/aside (a test's stub "aside": {status, body}).
pub fn ask(v: &mut LogView, text: String, question: String, cx: &mut Context<LogView>) {
    v.st.aside.next += 1;
    let n = v.st.aside.next;
    let (chat, anchor) = {
        let m = v.model.read(cx);
        (m.conv.id.clone(), m.conv.entries.last().and_then(|e| e.get("id")).and_then(|i| i.as_str()).unwrap_or("").to_string())
    };
    v.st.aside.list.push(Aside { id: n, anchor, question: question.clone(), state: "pending".into(), text: String::new() });
    suggest::recap_dismiss(&v.model, cx);
    v.stick = 3;
    cx.notify();
    let stub = crundi_core::driver::take_stub("aside");
    let api = crundi_shell::api(cx);
    let path = format!("/api/ui-sessions/{chat}/aside");
    let me = cx.entity().downgrade();
    crundi_shell::run(cx, move || -> Result<(u16, Value), String> {
        if let Some(s) = stub {
            return Ok((s.get("status").and_then(|x| x.as_u64()).unwrap_or(200) as u16, s.get("body").cloned().unwrap_or(Value::Null)));
        }
        let Some(api) = api else { return Err("Not signed in".into()) };
        match api.post(&path, &json!({ "question": question })) {
            Ok(v) => Ok((200, v)),
            Err(ApiError::Status(404, _)) => Ok((404, Value::Null)),
            Err(ApiError::Status(code, words)) => serde_json::from_str::<Value>(&words).map(|v| (code, v)).or(Err(words)),
            Err(e) => Err(e.to_string()),
        }
    }, move |r, cx| {
        let _ = me.update(cx, |v, cx| {
            match outcome(r) {
                None => {
                    v.st.aside.list.retain(|x| x.id != n);
                    suggest::post_send(&v.model, text, cx);
                }
                Some((state, out)) => if let Some(x) = v.st.aside.list.iter_mut().find(|x| x.id == n) {
                    x.state = state.into();
                    x.text = out;
                    v.stick = 3;
                },
            }
            cx.notify();
        });
    });
}

/// A card's ×: it goes at once (an answer still coming is dropped); no call.
pub fn close(v: &mut LogView, id: u32, cx: &mut Context<LogView>) {
    v.st.aside.list.retain(|x| x.id != id);
    cx.notify();
}

/// This feature's fields in the chat's props (the side questions are in
/// `dock.asides`, suggest.rs, as the Slint client's).
pub fn props(_v: &LogView, _cx: &App, _o: &mut Map<String, Value>) {}

/// This feature's driver actions (Some(answer) for a name it knows).
/// "aside-ask <text>" is the GPUI client's own stand-in for the composer's
/// `/btw` until the composer calls `ask_from_box`.
pub fn act(name: &str, args: &[String], v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "aside-close" => close(v, a(0).parse().unwrap_or(0), cx),
        "aside-ask" => {
            let t = a(0);
            let Some(q) = md::btw(&t) else { return Some(json!({ "ok": false, "error": "not a side question" })) };
            ask(v, t.trim_end().to_string(), q, cx);
        }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// A side question's card: the head (ON THE SIDE, ×), the question, then
/// "Asking…", the answer (markdown, as a reply) or the error box; the note.
pub fn row(v: &mut LogView, a: &Aside, cx: &mut Context<LogView>) -> AnyElement {
    let z = px(v.model.read(cx).zoom as f32);
    let id = a.id;
    let answer = match a.state.as_str() {
        "done" => {
            let blocks: Vec<AnyElement> = split_blocks(&format!("aside-{}", a.id), &a.text).into_iter().filter_map(|r| match r {
                Row::Prose(bid, t, kind, marker, last) => Some(crate::prose::block(&bid, &t, kind, &marker, last, cx)),
                Row::Code(bid, c, last) => Some(crate::prose::code(v, &bid, &c, last, cx)),
                _ => None,
            }).collect();
            div().flex().flex_col().gap(T::SP_6).text_color(T::text_secondary()).children(blocks).into_any_element()
        }
        "error" => div().px(T::SP_10).py(T::SP_6).rounded(T::R_CONTROL).border_1().border_color(T::red()).bg(T::red_dim())
            .text_size(T::TEXT_12 + z).text_color(T::red()).child(a.text.clone()).into_any_element(),
        _ => div().flex().items_center().gap(T::SP_6).text_color(T::text_secondary())
            .child(w::spinner(T::ICON_SM)).child("Asking…").into_any_element(),
    };
    div().flex().flex_col().px(T::SP_10).py(T::SP_8).rounded(T::R_CONTROL).border_1().border_dashed().border_color(T::border()).bg(T::bg_secondary())
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().flex_1().min_w_0().text_size(T::TEXT_11 + z).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child("ON THE SIDE"))
            .child(w::icon_button(SharedString::from(format!("aside-x-{id}")), IconName::X, "Close").on_click(cx.listener(move |v, _, _, cx| close(v, id, cx)))))
        .child(div().mt(T::SP_4).mb(T::SP_6).text_color(T::text()).line_height(relative(T::LH_CHAT)).child(a.question.clone()))
        .child(answer)
        .child(div().mt(T::SP_6).text_size(T::TEXT_11 + z).text_color(T::text_muted()).child("Not added to the conversation."))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn outcomes() {
        assert_eq!(outcome(Ok((200, json!({ "ok": true, "answer": "Writing **design.md**." })))), Some(("done", "Writing **design.md**.".into())));
        assert_eq!(outcome(Ok((200, json!({ "ok": false, "error": "E3" })))), Some(("error", "E3".into())));
        assert_eq!(outcome(Ok((200, json!({ "ok": false })))), Some(("error", "No answer".into())));
        assert_eq!(outcome(Ok((404, Value::Null))), None);
        assert_eq!(outcome(Err("Failed to fetch".into())), Some(("error", "Failed to fetch".into())));
    }
}
