//! 2.23: parked panes (specs/parked-panes.md). A chat whose process Crundi
//! stopped keeps its pane: its stored conversation darkened behind a card
//! (Resume, Peek, why and when, what Resume does, the heavy warning, the
//! other choices). Peek shows the conversation (Esc or Parked view back);
//! Resume, Compact and resume, New session ask the server
//! (POST /api/panes/:id/resume) and the pane swaps back to the live chat in
//! place when the `state` push lists it live.
//!
//! The chat is parked when the state's terminals list it with kind
//! "parked" (paneKind "ui"). Keeping its pane, the header's "exited" state
//! and × discarding are the chats crate's and the workbench's
//! (shared/requests.md).

use crate::LogView;
use crundi_core::{api::{ApiError, ApiResult}, resume};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};

#[derive(Default)]
pub struct State {
    /// Peek: the conversation readable instead of the card.
    pub peek: bool,
    /// The pressed choice while it runs ("resume" | "compact" | "new"), "" for none.
    pub busy: String,
    /// The stored conversation was asked for (once while parked).
    preview: bool,
    /// The body's height (the card's size variants).
    body_h: f32,
    /// The keyboard while peeking (Esc leaves Peek).
    focus: Option<FocusHandle>,
}

/// The parked item of a chat in the state's terminals (None: the chat is live).
pub fn item(cx: &App, chat: &str) -> Option<Value> {
    let s = crundi_shell::session::get(cx);
    s.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(chat)
        && t.get("kind").and_then(|k| k.as_str()) == Some("parked")
        && t.get("paneKind").and_then(|k| k.as_str()).unwrap_or("ui") == "ui").cloned()
}

/// The chat is parked.
pub fn is_parked(cx: &App, chat: &str) -> bool { item(cx, chat).is_some() }

/// Peeking at a parked chat (the composer then shows; a send resumes first).
pub fn peeking(cx: &App, chat: &str) -> bool {
    crate::view_of(cx, chat).is_some_and(|v| v.read(cx).st.parked.peek) && is_parked(cx, chat)
}

fn b(v: &Value, k: &str) -> bool { v.get(k).and_then(|x| x.as_bool()).unwrap_or(false) }
fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

fn now_secs() -> f64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0) }

/// The card's lines, as the Slint client's pane props name them.
struct Card { why: String, what: &'static str, session: bool, heavy: bool, tokens: String }

fn card_of(it: &Value) -> Card {
    let session = b(it, "hasSession");
    Card {
        why: resume::park_why(&s(it, "reason"), it.get("parkedAt").and_then(|x| x.as_f64()).unwrap_or(0.0), now_secs()),
        what: resume::park_what(session),
        session,
        heavy: b(it, "heavy"),
        tokens: resume::tokens_k(it.get("tokens").and_then(|x| x.as_f64()).unwrap_or(0.0).max(0.0) as u64),
    }
}

/// The size variant of the card for the body's height: 0 full, 1 short (≤ 330), 2 very short (≤ 210).
pub fn variant(body_h: f32) -> u8 { if body_h > 0. && body_h <= 210. { 2 } else if body_h > 0. && body_h <= 330. { 1 } else { 0 } }

/// This chat's "pk" (Null when live) and the header's state.
pub fn props(v: &LogView, cx: &App, o: &mut Map<String, Value>) {
    let chat = v.chat(cx);
    let Some(it) = item(cx, &chat) else { return };
    let c = card_of(&it);
    let p = &v.st.parked;
    o.insert("pk".into(), json!({
        "peek": p.peek, "why": c.why, "what": c.what, "session": c.session, "heavy": c.heavy, "tokens": c.tokens,
        "busy": p.busy, "busyLabel": if p.busy.is_empty() { "" } else { resume::park_busy_label(&p.busy) },
        "variant": variant(p.body_h),
    }));
    o.insert("entries".into(), json!(v.model.read(cx).conv.entries.len()));
}

fn stub_result(v: Value) -> ApiResult {
    if let Some(ms) = v.get("delayMs").and_then(|x| x.as_u64()) { std::thread::sleep(std::time::Duration::from_millis(ms)); }
    match v.get("status").and_then(|x| x.as_u64()) {
        Some(st) if st >= 400 => Err(ApiError::Status(st as u16, s(&v, "error"))),
        _ if v.get("net").is_some() => Err(ApiError::Net(s(&v, "net"))),
        _ => Ok(v),
    }
}

/// Resume (mode "resume" | "compact" | "new"): the server's answer; refused,
/// a toast and the card back. `then` runs once it resumed (a send from Peek).
pub fn resume_chat(v: &mut LogView, mode: &str, then: Option<Box<dyn FnOnce(&mut App)>>, cx: &mut Context<LogView>) {
    if !v.st.parked.busy.is_empty() { return; }
    v.st.parked.busy = mode.to_string();
    cx.notify();
    let id = v.chat(cx);
    let id2 = id.clone();
    let path = format!("/api/panes/{id}/resume");
    let body = json!({ "mode": mode });
    let stub = crundi_core::driver::take_stub("park-resume");
    let me = cx.entity().downgrade();
    crundi_shell::call(cx, move |api| match stub { Some(x) => stub_result(x), None => api.post(&path, &body) }, move |r, cx| {
        let err = match &r {
            Ok(x) if x.get("ok").and_then(|o| o.as_bool()) != Some(true) => Some({ let e = s(x, "error"); if e.is_empty() { "Could not resume".to_string() } else { e } }),
            Ok(_) => None,
            Err(e) => Some(e.clone()),
        };
        let _ = me.update(cx, |v, cx| {
            v.st.parked.busy.clear();
            if err.is_none() { v.st.parked.peek = false; v.st.parked.preview = false; }
            cx.notify();
        });
        match err {
            Some(e) => toast(cx, &e),
            None => {
                // The chat runs again under the same id. The subscription made
                // while it was parked was ignored by the server (no live session
                // then), so ask again: its history and events come from here on.
                // Without this a resumed chat stayed "working" and showed nothing new.
                crundi_shell::subscribe(cx, &id2);
                if let Some(f) = then { f(cx); }
            }
        }
    });
}

fn toast(cx: &mut App, text: &str) {
    let Some(win) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
    let t = text.to_string();
    let _ = win.update(cx, move |_, window, cx| w::toast(window, cx, t, w::Toast::Error));
}

/// The stored conversation, the card's background (fetched once while parked).
fn load_preview(v: &mut LogView, cx: &mut Context<LogView>) {
    if v.st.parked.preview { return; }
    v.st.parked.preview = true;
    let id = v.chat(cx);
    let model = v.model.clone();
    crundi_shell::call(cx, move |api| api.get(&format!("/api/panes/{id}/preview")), move |r, cx| {
        let Ok(x) = r else { return };
        if x.get("ok").and_then(|o| o.as_bool()) != Some(true) { return; }
        model.update(cx, |m, cx| {
            let id = m.id().to_string();
            m.conv.load(&json!({ "id": id, "status": "parked", "state": "idle", "messages": x.get("messages").cloned().unwrap_or(json!([])) }));
            cx.notify();
        });
    });
}

/// The driver: `peek <pane key> on|off`, `park-resume <pane key> resume|compact|new`.
pub fn act(name: &str, args: &[String], v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    let key = args.first().cloned().unwrap_or_default();
    let me = format!("live:{}", v.chat(cx));
    if !key.is_empty() && key != me { return None; }
    match name {
        "peek" => {
            v.st.parked.peek = args.get(1).is_some_and(|a| a == "on");
            // Peeking takes the keyboard (Esc goes back to the card).
            if v.st.parked.peek { let h = v.st.parked.focus.get_or_insert_with(|| cx.focus_handle()).clone(); window.focus(&h, cx); }
            cx.notify();
        }
        "park-resume" => { let mode = args.get(1).cloned().unwrap_or_else(|| "resume".into()); resume_chat(v, &mode, None, cx); }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// A parked chat: the log darkened behind the card, or (Peek) the log with
/// the Parked view pill. `log` is the chat's log as drawn; None when live.
pub fn wrap(v: &mut LogView, log: AnyElement, _window: &mut Window, cx: &mut Context<LogView>) -> Result<AnyElement, AnyElement> {
    let chat = v.chat(cx);
    let Some(it) = item(cx, &chat) else {
        // Live again: Peek and the busy look go.
        if v.st.parked.peek || !v.st.parked.busy.is_empty() || v.st.parked.preview { v.st.parked = State::default(); }
        return Err(log);
    };
    if v.model.read(cx).conv.entries.is_empty() { load_preview(v, cx); }
    let me = cx.entity().downgrade();
    let measure = canvas(move |b, _, cx| {
        let h = f32::from(b.size.height);
        let _ = me.update(cx, |v, cx| if (v.st.parked.body_h - h).abs() > 0.5 { v.st.parked.body_h = h; cx.notify(); });
    }, |_, _, _, _| {}).absolute().size_full();
    let base = div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().child(measure);
    if v.st.parked.peek {
        let pill = w::button("pk-unpeek", "Parked view").icon(IconName::EyeOff).tooltip("Back to the parked view")
            .on_click(cx.listener(|v, _, _, cx| { v.st.parked.peek = false; cx.notify(); }));
        let fh = v.st.parked.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        return Ok(base
            .track_focus(&fh)
            .on_key_down(cx.listener(|v, ev: &KeyDownEvent, _, cx| if ev.keystroke.key == "escape" { v.st.parked.peek = false; cx.notify(); cx.stop_propagation(); }))
            .child(log)
            .child(div().absolute().top(T::SP_10).right(T::SP_12).child(pill))
            .into_any_element());
    }
    let c = card_of(&it);
    let busy = v.st.parked.busy.clone();
    let off = !busy.is_empty();
    let var = variant(v.st.parked.body_h);
    let label = |mode: &str, idle: &str| if busy == mode { resume::park_busy_label(mode).to_string() } else { idle.to_string() };
    let resume_btn = w::disabled(w::primary("pk-resume", label("resume", "Resume")).icon(IconName::Play), off)
        .on_click(cx.listener(|v, _, _, cx| resume_chat(v, "resume", None, cx)));
    let peek_btn = c.session.then(|| w::disabled(w::button("pk-peek", "Peek").icon(IconName::Eye).tooltip("Read the conversation without resuming it"), off)
        .on_click(cx.listener(|v, _, window, cx| { v.st.parked.peek = true; let h = v.st.parked.focus.get_or_insert_with(|| cx.focus_handle()).clone(); window.focus(&h, cx); cx.notify(); })));
    let warn = c.heavy.then(|| {
        let long = " — continuing loads all of it into every turn. Compacting loads it once, then summarises it so later turns stay small.";
        let rest = match var { 0 => long, 1 => ", a long conversation", _ => "" };
        div().w_full().flex().items_center().gap(T::SP_6).px(T::SP_10).py(if var == 0 { T::SP_8 } else { T::SP_4 })
            .rounded(if var == 0 { T::R_CONTROL } else { T::R_PILL }).bg(T::yellow_dim()).border_1().border_color(T::yellow())
            .text_size(T::TEXT_11).line_height(relative(T::LH_UI)).text_color(T::text_secondary())
            .child(div().text_color(T::yellow()).child(IconName::ShieldAlert))
            .child(div().flex_1().min_w_0().child(div().font_weight(T::W_BOLD).text_color(T::yellow()).child(c.tokens.clone())).when(!rest.is_empty(), |d| d.child(rest)))
    });
    let choices = c.session.then(|| {
        if c.heavy {
            div().flex().flex_wrap().justify_center().gap(T::SP_6)
                .child(w::disabled(w::button("pk-compact", label("compact", "Compact and resume")), off).on_click(cx.listener(|v, _, _, cx| resume_chat(v, "compact", None, cx))))
                .child(w::disabled(w::button("pk-new", label("new", "New session")), off).on_click(cx.listener(|v, _, _, cx| resume_chat(v, "new", None, cx))))
        } else {
            div().flex().justify_center()
                .child(w::disabled(w::ghost("pk-new", label("new", "New session")).tooltip("Start a fresh conversation in this pane. The old one stays, and can be resumed from the session list."), off)
                    .on_click(cx.listener(|v, _, _, cx| resume_chat(v, "new", None, cx))))
        }
    });
    let card = div().flex().flex_col().items_center().gap(if var == 0 { T::SP_8 } else { T::SP_6 })
        .p(match var { 0 => T::SP_16, 1 => T::SP_12, _ => T::SP_8 }).w_full().max_w(T::PARK_CARD_W).text_center()
        .when(var == 2, |d| d.flex_row().flex_wrap().justify_center().max_w_full())
        .child(div().flex().flex_wrap().justify_center().gap(T::SP_8).child(resume_btn).children(peek_btn))
        .when(var < 2, |d| d.child(div().text_size(T::TEXT_12).text_color(T::text()).child(c.why.clone())))
        .when(var == 0, |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_secondary()).child(c.what)))
        .children(warn)
        .children(choices);
    Ok(base
        .child(div().absolute().size_full().opacity(0.35).child(log))
        .child(div().id("pk-veil").absolute().size_full().bg(T::scrim()).overflow_y_scroll()
            .child(div().min_h_full().flex().items_center().justify_center().child(card)))
        .into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn size_variants() {
        assert_eq!(variant(0.), 0);
        assert_eq!(variant(700.), 0);
        assert_eq!(variant(330.), 1);
        assert_eq!(variant(211.), 1);
        assert_eq!(variant(210.), 2);
        assert_eq!(variant(76.), 2);
    }

    #[::core::prelude::v1::test]
    fn the_card_lines() {
        let c = card_of(&json!({ "reason": "manual", "parkedAt": now_secs() * 1000.0, "hasSession": true, "heavy": true, "tokens": 138200 }));
        assert_eq!(c.why, "Closed · just now");
        assert_eq!(c.what, "Resume picks up this conversation.");
        assert!(c.heavy && c.session);
        assert_eq!(c.tokens, "138.2k tokens");
    }
}

/// Anything that would change a parked chat while it is peeked at (a send,
/// an answer, a mode change) resumes it first, then goes (specs 4). Not
/// parked: `then` at once. A refused resume: a toast, `then` never runs.
pub fn resume_first(cx: &mut App, chat: &str, then: impl FnOnce(&mut App) + 'static) {
    match crate::view_of(cx, chat).filter(|_| is_parked(cx, chat)) {
        Some(v) => v.update(cx, |v, cx| resume_chat(v, "resume", Some(Box::new(then)), cx)),
        None => then(cx),
    }
}
