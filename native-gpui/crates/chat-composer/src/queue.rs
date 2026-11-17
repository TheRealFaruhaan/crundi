//! 2.9: queued messages (specs/queued-messages.md, the web's cc-queue,
//! recallQueued). Sent while the chat works: handed over at once (the
//! server holds it; its slot is the chat model's, crundi_core::chat::Slot)
//! in the slate drawer. Sent while it needs input: held here (the dashed
//! drawer), going out as ONE message the moment it no longer needs input;
//! an exited chat's held lines go back into the box. Held lines belong to
//! the chat (they survive project switches) and are kept with the drafts
//! (a restart puts them in the box). A click on either drawer, or ↑ on the
//! box's first row, takes everything still waiting back into the box.

use crate::ComposerView;
use crundi_core::{api::ApiError, chat::{self as cchat, Handed}};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};

#[derive(Default)]
pub struct State {
    /// The lines held while the chat needs input, one per send.
    pub held: Vec<String>,
}

/// Sent while the chat needs input: held.
pub fn hold(c: &mut ComposerView, text: String, cx: &mut Context<ComposerView>) {
    c.queue.held.push(text);
    sync_held(c, cx);
    cx.notify();
}

fn sync_held(c: &ComposerView, cx: &mut App) {
    let chat = c.chat_id(cx);
    crate::drafts::set_held(cx, &chat, &c.queue.held.join("\n"));
}

/// The chat's state now: held lines go out once it no longer needs input
/// (into the box if it exited).
pub fn on_state(c: &mut ComposerView, st: &str, cx: &mut Context<ComposerView>) {
    if c.queue.held.is_empty() || st == "needs-input" { return; }
    let text = std::mem::take(&mut c.queue.held).join("\n");
    sync_held(c, cx);
    if st == "exited" { c.give_back(&text, cx); } else { c.post(text, true, cx); }
}

/// Anything to take back (held, or a handed-over message still recallable).
pub fn has_any(c: &ComposerView, cx: &App) -> bool {
    !c.queue.held.is_empty() || !c.model.read(cx).conv.recallable().is_empty()
}

/// ↑ in the box: on its first row with nothing selected, takes back.
pub fn up(c: &mut ComposerView, window: &mut Window, cx: &mut Context<ComposerView>) -> bool {
    if !has_any(c, cx) { return false; }
    let (text, r) = { let st = c.input.read(cx); (st.value().to_string(), st.selected_range()) };
    if r.start != r.end { return false; }
    let at = r.start.min(text.len());
    if text.get(..at).is_some_and(|b| b.contains('\n')) { return false; }
    take_back(c, window, cx)
}

/// Take back: the held lines at once, and every recallable handed-over
/// message (POST /cancel-queued each); then the box holds the cancelled
/// texts, the held text, then its own. False: nothing to take back.
pub fn take_back(c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> bool {
    let held = std::mem::take(&mut c.queue.held).join("\n");
    sync_held(c, cx);
    let recall = c.model.read(cx).conv.recallable();
    if held.is_empty() && recall.is_empty() { return false; }
    if recall.is_empty() { taken_back(c, vec![], held, None, cx); return true; }
    cx.notify();
    let Some(api) = crundi_shell::api(cx) else { return true };
    let id = c.chat_id(cx);
    let stubs: Vec<Option<Value>> = recall.iter().map(|_| crundi_core::driver::take_stub("cancel-queued")).collect();
    let me = cx.entity().downgrade();
    crundi_shell::run(cx, move || {
        recall.into_iter().zip(stubs).map(|((uuid, text), stub)| {
            let r = match stub { Some(v) => Ok(v), None => api.post(&format!("/api/ui-sessions/{id}/cancel-queued"), &json!({ "uuid": uuid })) };
            let r = match r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Err(v.get("error").and_then(|e| e.as_str()).filter(|e| !e.is_empty()).unwrap_or("Could not take it back").to_string()),
                Ok(v) => Ok(v.get("cancelled").and_then(|c| c.as_bool()).unwrap_or(false)),
                Err(ApiError::Status(404, _)) => Err("Taking back is not supported by this server".into()),
                Err(e) => Err(e.to_string()),
            };
            (uuid, text, r)
        }).collect::<Vec<_>>()
    }, move |results, cx| {
        let _ = me.update(cx, |c, cx| {
            let mut cancelled = vec![];
            let mut error = None;
            c.model.update(cx, |m, cx| {
                for (uuid, text, r) in results {
                    match r {
                        Ok(true) => { m.conv.slots.retain(|x| x.uuid != uuid); cancelled.push(text); }
                        // Too late: the CLI has it (no error).
                        Ok(false) => { if let Some(x) = m.conv.slots.iter_mut().find(|x| x.uuid == uuid) { x.state = "started".into(); } }
                        Err(e) => { error.get_or_insert(e); }
                    }
                }
                cx.notify();
            });
            taken_back(c, cancelled, held, error, cx);
        });
    });
    true
}

fn taken_back(c: &mut ComposerView, cancelled: Vec<String>, held: String, error: Option<String>, cx: &mut Context<ComposerView>) {
    if let Some(e) = &error { c.push_error(e, cx); }
    if cancelled.is_empty() && held.is_empty() { cx.notify(); return; }
    let new = cchat::taken_back(&cancelled, &held, &c.text(cx));
    let me = cx.entity();
    cx.defer(move |cx| crate::in_window(cx, move |window, cx| me.update(cx, |c, cx| { c.set_text(&new, window, cx); c.input.update(cx, |st, cx| st.focus(window, cx)); })));
}

/// The handed-over drawer's words, by its slots (the first that applies).
fn handed_words(h: &Handed) -> (&'static str, &'static str, &'static str, &'static str, &'static str) {
    match h {
        Handed::Recallable => ("Sending", "not read yet", "Click or press ↑ to take it back", "Click to edit — brings it back to the message box", "recallable"),
        Handed::Started => ("Sent", "Claude is reading it", "Claude is reading this one — too late to edit", "Claude has this one; it joins the conversation as it is read", "started"),
        Handed::Waiting => ("Sent", "waiting for Claude to pick it up", "Waiting for the session to confirm", "Claude has this one; it joins the conversation as it is read", "waiting"),
    }
}

/// Four lines at most are shown; more and the hint says it is trimmed.
fn trimmed(body: &str) -> bool { body.lines().count() > 4 }

/// The two drawers above the box: handed over on top, held under it.
pub fn render(c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> Option<AnyElement> {
    let handed = c.model.read(cx).conv.handed();
    if handed.is_none() && c.queue.held.is_empty() { return None; }
    let fs = c.fs(cx);
    let mut col = div().flex().flex_col().gap(crundi_theme::T::SP_8).w_full();
    if let Some((h, body)) = handed {
        let (label, note, hint, tip, _) = handed_words(&h);
        let hint = if trimmed(&body) { format!("Shown trimmed · {hint}") } else { hint.to_string() };
        let recallable = h == Handed::Recallable;
        let d = w::queue_drawer("q-sent", w::Drawer { held: false, label: label.into(), note: note.into(), body: body.into(), hint: hint.into(), tip: tip.into(), clickable: recallable }, fs);
        col = col.child(if recallable { d.on_click(cx.listener(|this, _, window, cx| { take_back(this, window, cx); })) } else { d });
    }
    if !c.queue.held.is_empty() {
        let n = c.queue.held.len();
        let body = c.queue.held.join("\n");
        let hint = if trimmed(&body) { "Shown trimmed · Click or press ↑ to take it back" } else { "Click or press ↑ to take it back" };
        let note = if n == 1 { "1 line · as one message, at the next tool call".to_string() } else { format!("{n} lines · as one message, at the next tool call") };
        col = col.child(w::queue_drawer("q-held", w::Drawer { held: true, label: "Sending".into(), note: note.into(), body: body.into(), hint: hint.into(),
            tip: "Click to edit — brings every queued line back to the message box".into(), clickable: true }, fs)
            .on_click(cx.listener(|this, _, window, cx| { take_back(this, window, cx); })));
    }
    Some(col.into_any_element())
}

/// take-back: as a click on a drawer.
pub fn test_ui(c: &mut ComposerView, name: &str, _args: &[String], window: &mut Window, cx: &mut Context<ComposerView>) -> Option<Value> {
    match name {
        "take-back" => { take_back(c, window, cx); Some(json!({ "ok": true })) }
        _ => None,
    }
}

/// qSent, qSentKind (recallable | started | waiting), qHeld, qHeldN.
pub fn pane_props(c: &ComposerView, cx: &App) -> Value {
    let (sent, kind) = c.model.read(cx).conv.handed().map(|(h, b)| (b, handed_words(&h).4)).unwrap_or_default();
    json!({ "qSent": sent, "qSentKind": kind, "qHeld": c.queue.held.join("\n"), "qHeldN": c.queue.held.len() })
}

pub fn props(_c: &ComposerView, _cx: &App) -> Value { json!({}) }
