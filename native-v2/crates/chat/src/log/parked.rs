//! 2.23: parked panes (the web's parkedChoicesHtml): a chat whose process
//! Crundi stopped keeps its pane: its stored conversation darkened behind a
//! card (Resume, Peek, why and when, what Resume does, the heavy warning,
//! the other choices). Peek shows the conversation (with its composer; Esc
//! or "Parked view" goes back); Resume, Compact and resume, New session ask
//! the server (POST /api/panes/:id/resume) and the pane is the live chat
//! again when the state lists it live.
//!
//! The chat is parked when the projects' terminals list it with kind
//! "parked" (paneKind "ui"). While parked and not peeked at, the pane hides
//! its composer (`LogView::hides_composer`, asked by the pane's body).

use super::LogView;
use crundi_core::resume;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

#[derive(Default)]
pub struct View {
    pub parked: bool,
    pub peek: bool,
    /// The log's height (the card's size variants).
    body_h: f32,
    focus: Option<FocusHandle>,
}

fn b(v: &Value, k: &str) -> bool { v.get(k).and_then(|x| x.as_bool()).unwrap_or(false) }
fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn now_secs() -> f64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0) }

/// The chat's parked item in the projects' terminals (None: live).
pub fn item(cx: &App, chat: &str) -> Option<Value> {
    crundi_shell::stores(cx).projects.read(cx).terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(chat)
        && t.get("kind").and_then(|k| k.as_str()) == Some("parked")
        && t.get("paneKind").and_then(|k| k.as_str()).unwrap_or("ui") == "ui").cloned()
}

/// The card's size for the log's height: 0 full, 1 short (≤ 330), 2 very short (≤ 210).
pub fn variant(h: f32) -> u8 { if h > 0. && h <= 210. { 2 } else if h > 0. && h <= 330. { 1 } else { 0 } }

/// The projects changed: parked or live again (Peek and the busy look go).
pub fn follow(v: &mut LogView, cx: &mut Context<LogView>) {
    let chat = v.chat(cx);
    let parked = item(cx, &chat).is_some();
    if parked == v.ui.parked.parked { return; }
    v.ui.parked.parked = parked;
    if !parked {
        v.ui.parked.peek = false;
        v.store.update(cx, |st, _| { st.log.park_busy.clear(); st.log.park_preview = false; });
    } else if v.store.read(cx).conv.entries.is_empty() { load_preview(v, cx); }
    cx.notify();
}

/// Resume (mode resume | compact | new): refused, a toast and the card back.
pub fn resume_chat(v: &mut LogView, mode: &str, cx: &mut Context<LogView>) {
    if !v.store.read(cx).log.park_busy.is_empty() { return; }
    let m = mode.to_string();
    v.store.update(cx, |st, cx| { st.log.park_busy = m; cx.notify(); });
    let id = v.chat(cx);
    let path = format!("/api/panes/{id}/resume");
    let body = json!({ "mode": mode });
    let store = v.store.clone();
    let me = cx.entity().downgrade();
    crundi_shell::call(cx, move |api| api.post(&path, &body), move |r, cx| {
        let err = match &r {
            Ok(x) if x.get("ok").and_then(|o| o.as_bool()) != Some(true) => Some({ let e = s(x, "error"); if e.is_empty() { "Could not resume".to_string() } else { e } }),
            Ok(_) => None,
            Err(e) => Some(e.clone()),
        };
        store.update(cx, |st, cx| { st.log.park_busy.clear(); if err.is_none() { st.log.park_preview = false; } cx.notify(); });
        match err {
            Some(e) => { if let Some(win) = cx.active_window() { let _ = win.update(cx, move |_, window, cx| w::toast(window, cx, e, w::Toast::Error)); } }
            None => {
                let _ = me.update(cx, |v, cx| { v.ui.parked.peek = false; cx.notify(); });
                // The chat runs again under the same id: subscribe again (the
                // one made while parked was ignored by the server).
                crundi_shell::subscribe(cx, &id);
            }
        }
    });
}

/// The stored conversation, the card's background (asked for once while parked).
fn load_preview(v: &mut LogView, cx: &mut Context<LogView>) {
    if v.store.read(cx).log.park_preview { return; }
    v.store.update(cx, |st, _| st.log.park_preview = true);
    let id = v.chat(cx);
    let store = v.store.clone();
    let path = format!("/api/panes/{id}/preview");
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
        let Ok(x) = r else { return };
        if x.get("ok").and_then(|o| o.as_bool()) != Some(true) { return; }
        let msgs = x.get("messages").cloned().unwrap_or(json!([]));
        store.update(cx, |st, cx| st.apply_batch(&[json!({ "type": "ui-history", "id": id, "session": { "id": id, "status": "parked", "state": "idle", "messages": msgs } })], cx));
    });
}

fn peek(v: &mut LogView, on: bool, window: &mut Window, cx: &mut Context<LogView>) {
    v.ui.parked.peek = on;
    if on { let h = v.ui.parked.focus.get_or_insert_with(|| cx.focus_handle()).clone(); window.focus(&h, cx); }
    cx.notify();
}

pub fn state(v: &LogView, cx: &App) -> Value {
    let chat = v.chat(cx);
    let Some(it) = item(cx, &chat) else { return Value::Null };
    let busy = v.store.read(cx).log.park_busy.clone();
    json!({ "peek": v.ui.parked.peek, "why": resume::park_why(&s(&it, "reason"), it.get("parkedAt").and_then(|x| x.as_f64()).unwrap_or(0.), now_secs()),
        "what": resume::park_what(b(&it, "hasSession")), "heavy": b(&it, "heavy"), "busy": busy, "variant": variant(v.ui.parked.body_h), "hidesComposer": v.hides_composer() })
}

pub fn act(name: &str, args: &[String], v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    match name {
        "peek" => peek(v, args.first().is_some_and(|a| a == "on"), window, cx),
        "park-resume" => { let mode = args.first().cloned().unwrap_or_else(|| "resume".into()); resume_chat(v, &mode, cx); }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// A parked chat: the log darkened behind the card, or (Peek) the log with
/// the "Parked view" pill. A live chat's log as it is.
pub fn wrap(v: &mut LogView, log: AnyElement, _window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    if !v.ui.parked.parked { return log; }
    let chat = v.chat(cx);
    let Some(it) = item(cx, &chat) else { return log };
    let me = cx.entity().downgrade();
    let measure = canvas(move |bd, _, cx| {
        let h = f32::from(bd.size.height);
        let me = me.clone();
        cx.defer(move |cx| { let _ = me.update(cx, |v, cx| if (v.ui.parked.body_h - h).abs() > 0.5 { v.ui.parked.body_h = h; cx.notify(); }); });
    }, |_, _, _, _| {}).absolute().size_full();
    let base = div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().child(measure);
    if v.ui.parked.peek {
        let pill = w::button("pk-unpeek", "Parked view").icon(IconName::EyeOff).tooltip("Back to the parked view")
            .on_click(cx.listener(|v, _, w, cx| peek(v, false, w, cx)));
        let fh = v.ui.parked.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        return base.track_focus(&fh)
            .on_key_down(cx.listener(|v, ev: &KeyDownEvent, w, cx| if ev.keystroke.key == "escape" { peek(v, false, w, cx); cx.stop_propagation(); }))
            .child(log)
            .child(div().absolute().top(T::SP_10).right(T::SP_12).child(pill))
            .into_any_element();
    }
    let session = b(&it, "hasSession");
    let heavy = b(&it, "heavy");
    let tokens = resume::tokens_k(it.get("tokens").and_then(|x| x.as_f64()).unwrap_or(0.0).max(0.0) as u64);
    let why = resume::park_why(&s(&it, "reason"), it.get("parkedAt").and_then(|x| x.as_f64()).unwrap_or(0.), now_secs());
    let what = resume::park_what(session);
    let busy = v.store.read(cx).log.park_busy.clone();
    let off = !busy.is_empty();
    let var = variant(v.ui.parked.body_h);
    let label = |mode: &str, idle: &str| if busy == mode { resume::park_busy_label(mode).to_string() } else { idle.to_string() };
    let resume_btn = w::disabled(w::primary("pk-resume", label("resume", "Resume")).icon(IconName::Play), off).on_click(cx.listener(|v, _, _, cx| resume_chat(v, "resume", cx)));
    let peek_btn = session.then(|| w::disabled(w::button("pk-peek", "Peek").icon(IconName::Eye).tooltip("Read the conversation without resuming it"), off)
        .on_click(cx.listener(|v, _, w, cx| peek(v, true, w, cx))));
    let warn = heavy.then(|| {
        let long = " — continuing loads all of it into every turn. Compacting loads it once, then summarises it so later turns stay small.";
        let rest = match var { 0 => long, 1 => ", a long conversation", _ => "" };
        div().w_full().flex().items_center().gap(T::SP_6).px(T::SP_10).py(if var == 0 { T::SP_8 } else { T::SP_4 })
            .rounded(if var == 0 { T::R_CONTROL } else { T::R_PILL }).bg(T::yellow_dim()).border_1().border_color(T::yellow())
            .text_size(T::TEXT_11).line_height(relative(T::LH_UI)).text_color(T::text_secondary())
            .child(div().text_color(T::yellow()).child(w::icon(IconName::ShieldAlert, T::ICON_SM)))
            .child(div().flex_1().min_w_0().child(div().font_weight(T::W_BOLD).text_color(T::yellow()).child(tokens.clone())).when(!rest.is_empty(), |d| d.child(rest)))
    });
    let choices = session.then(|| {
        if heavy {
            div().flex().flex_wrap().justify_center().gap(T::SP_6)
                .child(w::disabled(w::button("pk-compact", label("compact", "Compact and resume")), off).on_click(cx.listener(|v, _, _, cx| resume_chat(v, "compact", cx))))
                .child(w::disabled(w::button("pk-new", label("new", "New session")), off).on_click(cx.listener(|v, _, _, cx| resume_chat(v, "new", cx))))
        } else {
            div().flex().justify_center()
                .child(w::disabled(w::ghost("pk-new", label("new", "New session")).tooltip("Start a fresh conversation in this pane. The old one stays, and can be resumed from the session list."), off)
                    .on_click(cx.listener(|v, _, _, cx| resume_chat(v, "new", cx))))
        }
    });
    let card = div().flex().flex_col().items_center().gap(if var == 0 { T::SP_8 } else { T::SP_6 })
        .p(match var { 0 => T::SP_16, 1 => T::SP_12, _ => T::SP_8 }).w_full().max_w(T::PARK_CARD_W).text_center()
        .when(var == 2, |d| d.flex_row().flex_wrap().justify_center().max_w_full())
        .child(div().flex().flex_wrap().justify_center().gap(T::SP_8).child(resume_btn).children(peek_btn))
        .when(var < 2, |d| d.child(div().text_size(T::TEXT_12).text_color(T::text()).child(why)))
        .when(var == 0, |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_secondary()).child(what)))
        .children(warn)
        .children(choices);
    base.child(div().absolute().size_full().flex().flex_col().opacity(0.35).child(log))
        .child(div().id("pk-veil").occlude().absolute().size_full().bg(T::scrim()).overflow_y_scroll()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().min_h_full().flex().items_center().justify_center().child(card)))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #[::core::prelude::v1::test]
    fn size_variants() {
        assert_eq!(super::variant(700.), 0);
        assert_eq!(super::variant(330.), 1);
        assert_eq!(super::variant(210.), 2);
    }
}
