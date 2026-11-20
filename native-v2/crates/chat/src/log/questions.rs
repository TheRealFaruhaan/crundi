//! 2.13: question cards (the web's questionNode: AskUserQuestion, one or
//! several questions, multi-select, a note under a choice, Other, Submit,
//! Dismiss, an orphaned card) and permission cards (permissionNode: Allow,
//! Allow always, Deny; escalated: who decides; answered: the tool and its
//! tag).
//!
//! The rules are crundi_core::questions. Each open card's picks are the
//! chat's (`Shared::cards`, the same in a pane and a bubble), kept as the
//! card's answer draft (crundi_core::drafts, per server, in the saved
//! settings) so a half-answered card outlives a restart. Each log keeps the
//! card's text boxes and the radios' focus (`View`). Submit / Dismiss post
//! `respond`, or `answer-closed` for an orphaned card; the card shows its
//! answer at once.

use super::LogView;
use crate::store::{ChatStore, Row};
use crundi_core::{drafts::Drafts, questions::{self as q, Pick, Q}, store as saved};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::collections::HashMap;

/// One open card: its questions and the picks (one per question).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Card { pub qs: Vec<Q>, pub picks: Vec<Pick> }

impl Card {
    fn new(qs: Vec<Q>, mut picks: Vec<Pick>) -> Card { picks.resize(qs.len(), Pick::default()); Card { qs, picks } }
    pub fn can(&self) -> bool { q::can_submit(&self.qs, &self.picks) }
    fn label(&self, qi: usize, oi: usize) -> String { self.qs.get(qi).and_then(|x| x.options.get(oi)).map(|o| o.label.clone()).unwrap_or_default() }
}

/// A card's text boxes (Other, the note, per question) and its radios' focus.
struct CardUi { other: Vec<Entity<w::InputState>>, note: Vec<Entity<w::InputState>>, radios: Vec<Vec<FocusHandle>>, _subs: Vec<Subscription> }

#[derive(Default)]
pub struct View { ui: HashMap<String, CardUi> }

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

fn now_ms() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }

// ─── The answer drafts (per server) ───

#[derive(Default)]
struct QDrafts { server: String, d: Drafts }
impl Global for QDrafts {}

fn server(cx: &App) -> String { crundi_shell::api(cx).map(|a| a.base().to_string()).unwrap_or_default() }

fn drafts(cx: &mut App) -> &mut Drafts {
    let sv = server(cx);
    let g = cx.default_global::<QDrafts>();
    if g.server != sv {
        g.server = sv.clone();
        g.d = Drafts::from_saved(saved::load().drafts.get(&sv).unwrap_or(&Value::Null));
        g.d.prune(now_ms());
    }
    &mut g.d
}

/// A card's picks changed (None: answered, or nothing picked): in memory,
/// and this card's key in the saved settings at once.
fn save_draft(cx: &mut App, chat: &str, entry: &str, picks: Option<Value>) {
    let now = now_ms();
    drafts(cx).set_answer(chat, entry, picks.clone(), now);
    let sv = server(cx);
    if sv.is_empty() { return; }
    saved::update(|st| {
        let mut d = Drafts::from_saved(st.drafts.get(&sv).unwrap_or(&Value::Null));
        d.set_answer(chat, entry, picks, now);
        st.drafts.insert(sv, d.to_saved());
    });
}

fn picks_value(k: &Card) -> Option<Value> { (!q::blank(&k.picks)).then(|| q::to_value(&k.picks)) }

/// The batch changed these entries: a card for each open question (its
/// picks from the draft), none for an answered one.
pub fn follow(st: &mut ChatStore, ids: &[String], cx: &mut Context<ChatStore>) {
    let chat = st.id.clone();
    for id in ids {
        let Some(p) = st.conv.position(id) else { continue };
        let e = &st.conv.entries[p];
        if s(e, "kind") != "question" { continue; }
        if !q::open(&s(e, "status")) {
            if st.log.cards.remove(id).is_some() { save_draft(cx, &chat, id, None); }
            continue;
        }
        let qs = q::parse(e.get("input").unwrap_or(&Value::Null));
        if st.log.cards.get(id).is_some_and(|k| k.qs == qs) { continue; }
        let picks = drafts(cx).answer(&chat, id).map(|d| q::from_value(d, &qs)).unwrap_or_default();
        st.log.cards.insert(id.clone(), Card::new(qs, picks));
    }
}

// ─── Editing a card ───

fn ensure_ui(v: &mut LogView, entry: &str, window: &mut Window, cx: &mut Context<LogView>) {
    if v.ui.questions.ui.contains_key(entry) { return; }
    let Some(k) = v.store.read(cx).log.cards.get(entry).cloned() else { return };
    let mut subs = vec![];
    let (mut other, mut note) = (vec![], vec![]);
    for (qi, p) in k.picks.iter().enumerate() {
        let (o, n) = (p.other.clone(), p.note.clone());
        let os = cx.new(|cx| { let mut st = w::InputState::new(window, cx).placeholder("Other (type your own answer)…"); st.set_value(o, window, cx); st });
        let ns = cx.new(|cx| { let mut st = w::InputState::new(window, cx).placeholder("Note for Claude about this choice"); st.set_value(n, window, cx); st });
        let e1 = entry.to_string();
        subs.push(cx.subscribe_in(&os, window, move |this: &mut LogView, st, ev: &w::InputEvent, window, cx| match ev {
            w::InputEvent::Change => { let t = st.read(cx).value().to_string(); edit(this, &e1, window, cx, |k| { if let Some(p) = k.picks.get_mut(qi) { q::type_other(p, &t); } }); }
            w::InputEvent::PressEnter { .. } => answer(this, &e1, false, cx),
            _ => {}
        }));
        let e2 = entry.to_string();
        subs.push(cx.subscribe_in(&ns, window, move |this: &mut LogView, st, ev: &w::InputEvent, window, cx| {
            if let w::InputEvent::Change = ev { let t = st.read(cx).value().to_string(); edit(this, &e2, window, cx, |k| { if let Some(p) = k.picks.get_mut(qi) { p.note = t; } }); }
        }));
        subs.extend(w::watch_focus(&os.focus_handle(cx), window, cx));
        subs.extend(w::watch_focus(&ns.focus_handle(cx), window, cx));
        other.push(os);
        note.push(ns);
    }
    let radios = k.qs.iter().map(|x| (0..x.options.len()).map(|_| cx.focus_handle()).collect()).collect();
    v.ui.questions.ui.insert(entry.to_string(), CardUi { other, note, radios, _subs: subs });
}

/// A change to a card's picks: the boxes follow (a pick clears Other…), the
/// draft is saved, the card is measured again (a note row opens or closes).
fn edit(v: &mut LogView, entry: &str, window: &mut Window, cx: &mut Context<LogView>, f: impl FnOnce(&mut Card)) {
    let chat = v.chat(cx);
    let Some(mut k) = v.store.read(cx).log.cards.get(entry).cloned() else { return };
    f(&mut k);
    if let Some(ui) = v.ui.questions.ui.get(entry) {
        for (i, p) in k.picks.iter().enumerate() {
            for (st, want) in [(ui.other.get(i), &p.other), (ui.note.get(i), &p.note)] {
                if let Some(st) = st { if st.read(cx).value().as_ref() != want.as_str() { let t = want.clone(); st.update(cx, |st, cx| st.set_value(t, window, cx)); } }
            }
        }
    }
    let saved = picks_value(&k);
    let e = entry.to_string();
    v.store.update(cx, |st, cx| { st.log.cards.insert(e.clone(), k); st.remeasure(&e); cx.notify(); });
    save_draft(cx, &chat, entry, saved);
}

fn do_pick(v: &mut LogView, entry: &str, qi: usize, oi: usize, window: &mut Window, cx: &mut Context<LogView>) {
    edit(v, entry, window, cx, |k| { let l = k.label(qi, oi); if let (Some(qq), Some(p)) = (k.qs.get(qi).cloned(), k.picks.get_mut(qi)) { q::pick(&qq, p, &l); } });
    if let Some(h) = v.ui.questions.ui.get(entry).and_then(|u| u.radios.get(qi)).and_then(|r| r.get(oi)) { h.focus(window, cx); }
}

/// The choice an arrow lands on: the next (or previous), wrapping.
fn wrap(oi: i32, d: i32, n: i32) -> usize { (((oi + d) % n + n) % n) as usize }

fn do_move(v: &mut LogView, entry: &str, qi: usize, oi: i32, d: i32, window: &mut Window, cx: &mut Context<LogView>) {
    let n = v.store.read(cx).log.cards.get(entry).and_then(|k| k.qs.get(qi)).map(|x| x.options.len() as i32).unwrap_or(0);
    if n > 0 { do_pick(v, entry, qi, wrap(oi, d, n), window, cx); }
}

fn do_note(v: &mut LogView, entry: &str, qi: usize, oi: usize, window: &mut Window, cx: &mut Context<LogView>) {
    ensure_ui(v, entry, window, cx);
    edit(v, entry, window, cx, |k| { let l = k.label(qi, oi); if let (Some(qq), Some(p)) = (k.qs.get(qi).cloned(), k.picks.get_mut(qi)) { q::open_note(&qq, p, &l); } });
    if let Some(st) = v.ui.questions.ui.get(entry).and_then(|u| u.note.get(qi).cloned()) { st.update(cx, |s, cx| s.focus(window, cx)); }
}

fn do_note_remove(v: &mut LogView, entry: &str, qi: usize, window: &mut Window, cx: &mut Context<LogView>) {
    edit(v, entry, window, cx, |k| { if let Some(p) = k.picks.get_mut(qi) { q::remove_note(p); } });
}

// ─── Answering ───

/// Submit (or Enter in Other) and Dismiss: a live question to `respond`, an
/// orphaned one to `answer-closed`. The card shows its answer at once and
/// its draft goes. A refused `respond` puts the card back with the chat's
/// error line; a refused `answer-closed` adds an error entry under it.
fn answer(v: &mut LogView, entry: &str, dismiss: bool, cx: &mut Context<LogView>) {
    let chat = v.chat(cx);
    let store = v.store.clone();
    let Some(k) = store.read(cx).log.cards.get(entry).cloned() else { return };
    if !dismiss && !k.can() { return; }
    let Some(p) = store.read(cx).conv.position(entry) else { return };
    let prev = store.read(cx).conv.entries[p].clone();
    let status = s(&prev, "status");
    if !q::open(&status) { return; }
    let input = prev.get("input").cloned().unwrap_or(json!({}));
    let orphaned = q::orphaned(&status);
    let (patch, body) = if orphaned {
        let answers = if dismiss { json!({}) } else { q::answers(&k.qs, &k.picks) };
        let text = if dismiss { String::new() } else { q::closed_text(&k.qs, &k.picks) };
        let mut ai = input.clone();
        ai["answers"] = answers.clone();
        (json!({ "status": "answered-late", "answeredInput": ai }), json!({ "entryId": entry, "answers": answers, "text": text }))
    } else if dismiss {
        (json!({ "status": "denied" }), json!({ "requestId": s(&prev, "requestId"), "behavior": "deny", "message": "The user dismissed the question." }))
    } else {
        let upd = q::updated_input(&input, &k.qs, &k.picks);
        (json!({ "status": "allowed", "answeredInput": upd.clone() }), json!({ "requestId": s(&prev, "requestId"), "behavior": "allow", "updatedInput": upd }))
    };
    let keep = picks_value(&k);
    super::shared::local(&store, json!({ "type": "patch", "id": entry, "patch": patch }), cx);
    super::shared::set_error(&store, String::new(), cx);
    v.ui.questions.ui.remove(entry);
    save_draft(cx, &chat, entry, None);
    let path = format!("/api/ui-sessions/{chat}/{}", if orphaned { "answer-closed" } else { "respond" });
    let id = entry.to_string();
    crundi_shell::call(cx, move |api| api.post(&path, &body), move |r, cx| {
        let err = match r {
            Err(e) if orphaned && e == "Not supported by this server" => Some("Answering an old question is not supported by this server".to_string()),
            Ok(x) if x.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(x.get("error").and_then(|e| e.as_str()).unwrap_or("The answer was not taken").to_string()),
            Ok(_) => None,
            Err(e) => Some(e),
        };
        let Some(err) = err else { return };
        if !orphaned {
            // The card back as it was, its picks and draft with it.
            save_draft(cx, &chat, &id, keep);
            let back = json!({ "status": s(&prev, "status"), "answeredInput": Value::Null });
            super::shared::local(&store, json!({ "type": "patch", "id": id, "patch": back }), cx);
            super::shared::set_error(&store, err, cx);
        } else {
            // The server could not pass it on: an error entry under the card.
            let n = store.update(cx, |st, _| { st.log.local_errors += 1; st.log.local_errors });
            super::shared::local(&store, json!({ "type": "entry", "entry": { "id": format!("local-q-error-{n}"), "kind": "error", "text": err } }), cx);
        }
    });
}

/// A permission card's Allow / Allow always / Deny.
fn respond(store: &Entity<ChatStore>, request_id: String, behavior: &str, always: bool, cx: &mut App) {
    let chat = store.read(cx).id.clone();
    let mut body = json!({ "requestId": request_id, "behavior": behavior });
    if always { body["always"] = json!(true); }
    let store = store.clone();
    crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{chat}/respond"), &body), move |r, cx| {
        if let Err(e) = r { super::shared::set_error(&store, e, cx); }
    });
}

// ─── The driver ───

/// The latest question card as the driver sees it.
pub fn state(v: &LogView, cx: &App) -> Value {
    let st = v.store.read(cx);
    let Some(e) = st.conv.entries.iter().rev().find(|e| s(e, "kind") == "question") else { return Value::Null };
    let id = s(e, "id");
    let status = s(e, "status");
    let card = st.log.cards.get(&id);
    let d = q::done(e);
    json!({ "entry": id, "status": status, "open": q::open(&status), "orphaned": q::orphaned(&status),
        "can": card.map(|k| k.can()).unwrap_or(false), "picks": card.map(|k| q::to_value(&k.picks)).unwrap_or(Value::Null),
        "questions": card.map(|k| k.qs.len()).unwrap_or(0), "tags": d.tags, "notes": d.notes, "dismissed": d.dismissed })
}

/// The driver's actions on the chat's latest open question card
/// (q-pick, q-note, q-note-type, q-note-remove, q-other, q-move, q-submit,
/// q-dismiss) and on its latest pending permission (perm-allow, perm-always, perm-deny).
pub fn act(name: &str, args: &[String], v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    if let Some(b) = name.strip_prefix("perm-") {
        let e = v.store.read(cx).conv.entries.iter().rev().find(|e| s(e, "kind") == "permission" && s(e, "status") == "pending").cloned();
        let Some(e) = e else { return Some(json!({ "ok": false, "error": "no pending permission" })) };
        let store = v.store.clone();
        match b { "allow" => respond(&store, s(&e, "requestId"), "allow", false, cx), "always" => respond(&store, s(&e, "requestId"), "allow", true, cx), _ => respond(&store, s(&e, "requestId"), "deny", false, cx) }
        return Some(json!({ "ok": true }));
    }
    let what = name.strip_prefix("q-")?;
    if !["pick", "note", "note-type", "note-remove", "other", "move", "submit", "dismiss"].contains(&what) { return None; }
    let entry = v.store.read(cx).conv.entries.iter().rev().find(|e| s(e, "kind") == "question" && q::open(&s(e, "status"))).map(|e| s(e, "id"));
    let Some(entry) = entry else { return Some(json!({ "ok": false, "error": "no open question" })) };
    ensure_ui(v, &entry, window, cx);
    let n = |i: usize| args.get(i).and_then(|x| x.parse::<i32>().ok()).unwrap_or(0);
    let t = |i: usize| args.get(i).cloned().unwrap_or_default();
    let qi = n(0).max(0) as usize;
    match what {
        "pick" => do_pick(v, &entry, qi, n(1).max(0) as usize, window, cx),
        "note" => do_note(v, &entry, qi, n(1).max(0) as usize, window, cx),
        "note-type" => { let x = t(1); edit(v, &entry, window, cx, |k| { if let Some(p) = k.picks.get_mut(qi) { p.note = x; } }) }
        "note-remove" => do_note_remove(v, &entry, qi, window, cx),
        "other" => { let x = t(1); edit(v, &entry, window, cx, |k| { if let Some(p) = k.picks.get_mut(qi) { q::type_other(p, &x); } }) }
        "move" => do_move(v, &entry, qi, n(1), n(2), window, cx),
        "submit" => answer(v, &entry, false, cx),
        _ => answer(v, &entry, true, cx),
    }
    Some(json!({ "ok": true }))
}

// ─── Drawing ───

/// The card's frame (.cc-ask): the accent's edge and tint.
fn frame() -> Div { div().flex().flex_col().gap(T::SP_12).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::accent()).bg(T::accent_dim()) }

/// A press inside a card's controls stays there (the log's own press would
/// take the keyboard and start a selection).
fn keep<E: InteractiveElement>(el: E) -> E { el.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()) }

/// A question's card.
pub fn row(v: &mut LogView, r: &Row, fs: f32, window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    let e = &r.shown;
    let status = s(&e.raw, "status");
    if !q::open(&status) {
        // Answered: a tag per answer (or dismissed, or answered), by Cru, each note on its line.
        let d = q::done(&e.raw);
        let mut tags = div().flex().flex_wrap().gap(T::SP_6);
        if d.dismissed { tags = tags.child(w::tag("dismissed", "error")); }
        else if d.tags.is_empty() { tags = tags.child(w::tag("answered", "")); }
        for t in &d.tags { tags = tags.child(w::tag(t.clone(), "ok")); }
        if d.by_cru { tags = tags.child(w::tag("by Cru", "")); }
        return frame().child(tags)
            .when(!d.notes.is_empty(), |f| f.child(div().flex().flex_col().children(d.notes.iter().map(|n| div().text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(n.clone())))))
            .into_any_element();
    }
    ensure_ui(v, &e.id, window, cx);
    let Some(k) = v.store.read(cx).log.cards.get(&e.id).cloned() else { return frame().into_any_element() };
    let Some(ui) = v.ui.questions.ui.get(&e.id) else { return frame().into_any_element() };
    let (others, notes, radios) = (ui.other.clone(), ui.note.clone(), ui.radios.clone());
    let entry = e.id.clone();
    let can = k.can();
    let mut card = frame();
    if q::orphaned(&status) {
        card = card.child(div().text_size(T::TEXT_12 * fs).text_color(T::text_secondary())
            .child("This question outlived the session that asked it. Answering sends your choice as a message."));
    }
    for (qi, qq) in k.qs.iter().enumerate() {
        let p = k.picks.get(qi).cloned().unwrap_or_default();
        let mut col = div().flex().flex_col();
        if !qq.header.is_empty() { col = col.child(div().flex().pb(T::SP_4).child(w::badge(qq.header.to_uppercase()))); }
        col = col.child(div().text_size(T::TEXT_13 * fs).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(qq.question.clone()));
        let mut opts = div().flex().flex_col().gap(T::SP_6).pt(T::SP_8);
        for (oi, o) in qq.options.iter().enumerate() {
            let on = p.chosen.contains(&o.label);
            let note_open = !p.note_opt.is_empty() && p.note_opt == o.label;
            let h = radios.get(qi).and_then(|r| r.get(oi)).cloned().unwrap_or_else(|| cx.focus_handle());
            let focused = h.is_focused(window);
            let hk = h.clone();
            let multi = qq.multi;
            let (e1, e2, e3, e4) = (entry.clone(), entry.clone(), entry.clone(), entry.clone());
            let mut text = div().flex().flex_col().flex_1().min_w_0()
                .child(div().text_size(T::TEXT_13 * fs).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(o.label.clone()))
                .when(!o.desc.is_empty(), |d| d.child(div().text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).child(o.desc.clone())));
            if !o.preview.is_empty() {
                text = text.child(div().mt(T::SP_6).py(T::SP_6).px(T::SP_8).rounded(T::R_CHIP).border_1().border_color(T::border_subtle()).bg(T::bg_secondary())
                    .font_family(T::MONO).text_size(T::TEXT_11 * fs).text_color(T::text_secondary()).child(o.preview.clone()));
            }
            if note_open {
                if let Some(ns) = notes.get(qi) {
                    let focus = ns.focus_handle(cx);
                    text = text.child(keep(div().flex().items_center().gap(T::SP_6).pt(T::SP_8))
                        .child(div().flex_1().min_w_0().child(w::text_box(&focus, w::input(ns), window, cx)))
                        .child(w::icon_button(SharedString::from(format!("qnx-{entry}-{qi}")), IconName::X, "Remove the note")
                            .on_click(cx.listener(move |this, _, window, cx| { cx.stop_propagation(); do_note_remove(this, &e3, qi, window, cx); }))));
                }
            }
            let row = keep(div().id(SharedString::from(format!("qo-{entry}-{qi}-{oi}"))).relative())
                .track_focus(&h)
                .flex().items_start().gap(T::SP_8).py(T::SP_8).pl(T::SP_10).pr(T::SP_32 + T::SP_4)
                .rounded(T::R_CONTROL).border_1().cursor_pointer()
                .border_color(if on { T::accent() } else { T::border() })
                .bg(if on { T::accent_dim() } else { T::bg() })
                .when(!on, |d| d.hover(|d| d.border_color(T::accent_hover())))
                .child(w::choice_mark(multi, on, focused))
                .child(text)
                .child(div().absolute().top(T::SP_4).right(T::SP_4)
                    .child(w::icon_toggle(SharedString::from(format!("qn-{entry}-{qi}-{oi}")), IconName::FileText, "Add a note to this choice", note_open)
                        .on_click(cx.listener(move |this, _, window, cx| { cx.stop_propagation(); do_note(this, &e2, qi, oi, window, cx); }))))
                .on_click(cx.listener(move |this, _, window, cx| do_pick(this, &e1, qi, oi, window, cx)))
                // The keyboard: Space picks (or toggles); in a radio group the arrows move and pick, wrapping.
                .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                    if !hk.is_focused(window) { return; }
                    match ev.keystroke.key.as_str() {
                        "space" | " " => do_pick(this, &e4, qi, oi, window, cx),
                        "down" | "right" if !multi => do_move(this, &e4, qi, oi as i32, 1, window, cx),
                        "up" | "left" if !multi => do_move(this, &e4, qi, oi as i32, -1, window, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }));
            opts = opts.child(row);
        }
        col = col.child(opts);
        if let Some(os) = others.get(qi) {
            let focus = os.focus_handle(cx);
            col = col.child(keep(div().pt(T::SP_4)).child(w::text_box(&focus, w::input(os), window, cx)));
        }
        card = card.child(col);
    }
    let (es, ed) = (entry.clone(), entry.clone());
    card.child(keep(div().flex().flex_wrap().gap(T::SP_8))
        .child(w::disabled(w::primary(SharedString::from(format!("qs-{entry}")), "Submit"), !can).on_click(cx.listener(move |this, _, _, cx| answer(this, &es, false, cx))))
        .child(w::danger(SharedString::from(format!("qd-{entry}")), "Dismiss").on_click(cx.listener(move |this, _, _, cx| answer(this, &ed, true, cx)))))
        .into_any_element()
}

/// A permission card (the web's permissionNode): yellow; pending: its title,
/// description, reason, the input, then Allow / Allow always / Deny (or,
/// escalated, who decides); answered: the tool's name and its tag.
pub fn permission(v: &mut LogView, r: &Row, fs: f32, _cx: &mut Context<LogView>) -> AnyElement {
    let e = &r.shown;
    let mut c = div().flex().flex_col().gap(T::SP_8).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::yellow()).bg(T::yellow_dim()).text_size(T::TEXT_12 * fs);
    let tag = e.group_summary.clone();
    if e.status == "pending" {
        c = c.child(div().text_size(T::TEXT_13 * fs).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(e.title.clone()))
            .when(!e.detail.is_empty(), |d| d.child(div().text_color(T::text_secondary()).child(e.detail.clone())))
            .when(!e.note.is_empty(), |d| d.child(div().text_color(T::text_muted()).child(e.note.clone())))
            .child(div().id(SharedString::from(format!("pp-{}", e.id))).max_h(T::SP_48 * 4.).overflow_y_scroll().py(T::SP_8).px(T::SP_10)
                .rounded(T::R_CONTROL).border_1().border_color(T::border_subtle()).bg(T::bg())
                .font_family(T::MONO).text_color(T::text_secondary()).child(e.args.clone()));
        if !tag.is_empty() {
            c = c.child(div().text_color(T::text_muted()).child(if tag == "manager" { "Sent to the Manager of this factory, who decides. You will see the answer here." } else { "Sent to the project owner. You will see the answer here." }));
            if tag == "escalated" {
                let id = e.group.clone();
                c = c.child(keep(div().flex()).child(w::button(SharedString::from(format!("pc-{}", e.id)), "Carry on without this").on_click(move |_, _, cx| {
                    let body = json!({ "id": id });
                    crundi_shell::call(cx, move |api| api.post("/api/collab/cancel", &body), |_, _| {});
                })));
            }
        } else {
            let store = v.store.clone();
            let btn = |b: gpui_component::button::Button, behavior: &'static str, always: bool, rid: String, store: Entity<ChatStore>| b.on_click(move |_, _, cx| respond(&store, rid.clone(), behavior, always, cx));
            let rid = e.request_id.clone();
            c = c.child(keep(div().flex().flex_wrap().gap(T::SP_8))
                .child(btn(w::primary(SharedString::from(format!("pa-{}", e.id)), "Allow"), "allow", false, rid.clone(), store.clone()))
                .when(!e.options.is_empty(), |d| d.child(btn(w::button(SharedString::from(format!("pA-{}", e.id)), "Allow always"), "allow", true, rid.clone(), store.clone())))
                .child(btn(w::danger(SharedString::from(format!("pd-{}", e.id)), "Deny"), "deny", false, rid.clone(), store.clone())));
        }
    } else {
        c = c.child(div().flex().items_center().gap(T::SP_6)
            .child(div().text_size(T::TEXT_13 * fs).font_weight(T::W_BOLD).text_color(T::text()).child(e.text.clone()))
            .child(w::tag(tag, if e.error { "error" } else { "ok" })))
            .when(!e.group.is_empty(), |d| d.child(div().text_color(T::text_secondary()).child(e.group.clone())));
    }
    c.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn arrows_wrap() {
        assert_eq!(wrap(1, 1, 2), 0);
        assert_eq!(wrap(0, -1, 2), 1);
    }

    #[::core::prelude::v1::test]
    fn a_card_from_its_draft() {
        let qs = q::parse(&json!({ "questions": [
            { "question": "A?", "options": [{ "label": "x" }, { "label": "y" }] },
            { "question": "B?", "multiSelect": true, "options": [{ "label": "u" }, { "label": "v" }] },
        ] }));
        let mut k = Card::new(qs.clone(), vec![]);
        assert!(picks_value(&k).is_none() && !k.can());
        let (q0, l) = (k.qs[0].clone(), k.label(0, 1));
        q::pick(&q0, &mut k.picks[0], &l);
        q::type_other(&mut k.picks[1], "mine");
        assert!(k.can());
        let back = Card::new(qs.clone(), q::from_value(&picks_value(&k).unwrap(), &qs));
        assert_eq!(back.picks, k.picks);
    }
}
