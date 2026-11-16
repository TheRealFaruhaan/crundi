//! 2.13: question cards (AskUserQuestion: one or several questions,
//! multi-select, notes, Other, Submit, Dismiss, an orphaned card;
//! specs/questions.md), and the permission card.
//!
//! The rules are crundi_core::questions; this keeps one `Card` per open
//! question entry (its questions and picks, made without a window so the
//! driver's props see it at once), its text boxes and radios' focus handles
//! (made at the first draw), and the answer draft (crundi_core::drafts,
//! `ans:<chat>:<entry>`, in the saved settings per server) so a
//! half-answered card outlives a client restart. Submit / Dismiss post as
//! the Slint client's q_answer does (app.rs): `respond`, or `answer-closed`
//! for an orphaned card.
use crate::LogView;
use crundi_chat_model::ChatEvent;
use crundi_core::{chat::Shown, drafts::Drafts, questions::{self as q, Pick, Q}, store};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// One open card: its questions, the picks (one per question), and once
/// drawn, its text boxes and the radios' focus handles.
pub struct Card {
    pub qs: Vec<Q>,
    pub picks: Vec<Pick>,
    ui: Option<CardUi>,
}

struct CardUi {
    /// Per question: Other, the note (one note per question).
    other: Vec<Entity<w::InputState>>,
    note: Vec<Entity<w::InputState>>,
    /// Per question, per choice: the keyboard's stop on the choice.
    radios: Vec<Vec<FocusHandle>>,
    _subs: Vec<Subscription>,
}

impl Card {
    fn new(qs: Vec<Q>, mut picks: Vec<Pick>) -> Card {
        picks.resize(qs.len(), Pick::default());
        Card { qs, picks, ui: None }
    }
    pub fn can(&self) -> bool { q::can_submit(&self.qs, &self.picks) }
    fn label(&self, qi: usize, oi: usize) -> String {
        self.qs.get(qi).and_then(|x| x.options.get(oi)).map(|o| o.label.clone()).unwrap_or_default()
    }
}

#[derive(Default)]
pub struct State {
    /// Open cards by entry id.
    cards: HashMap<String, Card>,
    local_errors: u32,
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

// ─── The answer drafts (crundi_core::drafts, per server) ───

/// The drafts of the server signed in to, loaded (and pruned) when it changes.
#[derive(Default)]
struct QDrafts { server: String, d: Drafts }
impl Global for QDrafts {}

fn server(cx: &App) -> String { crundi_shell::api(cx).map(|a| a.base().to_string()).unwrap_or_default() }

fn drafts(cx: &mut App) -> &mut Drafts {
    let sv = server(cx);
    if !cx.has_global::<QDrafts>() { cx.set_global(QDrafts::default()); }
    let g = cx.global_mut::<QDrafts>();
    if g.server != sv {
        g.server = sv.clone();
        g.d = Drafts::from_saved(store::load().drafts.get(&sv).unwrap_or(&Value::Null));
        g.d.prune(now_ms());
    }
    &mut g.d
}

fn draft_of(cx: &App, chat: &str, entry: &str) -> Option<Value> {
    let g = cx.try_global::<QDrafts>()?;
    if g.server != server(cx) { return None; }
    g.d.answer(chat, entry).cloned()
}

/// A card's picks changed (None: answered, or nothing picked): kept in
/// memory and written to the saved settings at once. Only this card's key
/// is changed on disk (the composer's drafts there are left as they are).
fn save_draft(cx: &mut App, chat: &str, entry: &str, picks: Option<Value>) {
    let now = now_ms();
    drafts(cx).set_answer(chat, entry, picks.clone(), now);
    let sv = server(cx);
    if sv.is_empty() { return; }
    store::update(|st| {
        let mut d = Drafts::from_saved(st.drafts.get(&sv).unwrap_or(&Value::Null));
        d.set_answer(chat, entry, picks, now);
        st.drafts.insert(sv, d.to_saved());
    });
}

fn picks_value(k: &Card) -> Option<Value> { (!q::blank(&k.picks)).then(|| q::to_value(&k.picks)) }

// ─── Following the conversation ───

/// The conversation changed: a card for every open question (its picks
/// from the draft), none for an answered one.
pub fn changed(v: &mut LogView, cx: &mut Context<LogView>) {
    let chat = v.chat(cx);
    let qes: Vec<(String, bool, Value)> = v.model.read(cx).conv.entries.iter()
        .filter(|e| s(e, "kind") == "question")
        .map(|e| (s(e, "id"), q::open(&s(e, "status")), e.get("input").cloned().unwrap_or(Value::Null)))
        .collect();
    for (id, open, input) in qes {
        if !open {
            if v.st.questions.cards.remove(&id).is_some() { save_draft(cx, &chat, &id, None); }
            continue;
        }
        let qs = q::parse(&input);
        if v.st.questions.cards.get(&id).is_some_and(|k| k.qs == qs) { continue; }
        let picks = drafts(cx).answer(&chat, &id).map(|d| q::from_value(d, &qs)).unwrap_or_default();
        v.st.questions.cards.insert(id, Card::new(qs, picks));
    }
}

/// The card's text boxes and focus handles, made once (they need the window).
fn ensure_ui(v: &mut LogView, entry: &str, window: &mut Window, cx: &mut Context<LogView>) {
    let Some(k) = v.st.questions.cards.get(entry) else { return };
    if k.ui.is_some() { return; }
    let (others, notes): (Vec<String>, Vec<String>) = k.picks.iter().map(|p| (p.other.clone(), p.note.clone())).unzip();
    let counts: Vec<usize> = k.qs.iter().map(|x| x.options.len()).collect();
    let mut subs = Vec::new();
    let mut other = Vec::new();
    let mut note = Vec::new();
    for (qi, (o, n)) in others.into_iter().zip(notes).enumerate() {
        let os = cx.new(|cx| { let mut st = w::InputState::new(window, cx).placeholder("Other (type your own answer)…"); st.set_value(o, window, cx); st });
        let ns = cx.new(|cx| { let mut st = w::InputState::new(window, cx).placeholder("Note for Claude about this choice"); st.set_value(n, window, cx); st });
        let e1 = entry.to_string();
        subs.push(cx.subscribe_in(&os, window, move |this: &mut LogView, st, ev: &w::InputEvent, window, cx| match ev {
            w::InputEvent::Change => { let t = st.read(cx).value().to_string(); edit(this, &e1, window, cx, |k| { if let Some(p) = k.picks.get_mut(qi) { q::type_other(p, &t); } }); }
            // Enter in Other submits when every question has an answer (the
            // web does nothing: a named difference, specs/questions.md).
            w::InputEvent::PressEnter { .. } => answer(this, &e1, false, cx),
            _ => {}
        }));
        let e2 = entry.to_string();
        subs.push(cx.subscribe_in(&ns, window, move |this: &mut LogView, st, ev: &w::InputEvent, window, cx| {
            if let w::InputEvent::Change = ev { let t = st.read(cx).value().to_string(); edit(this, &e2, window, cx, |k| { if let Some(p) = k.picks.get_mut(qi) { p.note = t; } }); }
        }));
        other.push(os);
        note.push(ns);
    }
    let radios = counts.iter().map(|n| (0..*n).map(|_| cx.focus_handle()).collect()).collect();
    if let Some(k) = v.st.questions.cards.get_mut(entry) { k.ui = Some(CardUi { other, note, radios, _subs: subs }); }
}

/// A change to a card's picks: the boxes follow (a pick clears Other…),
/// the draft is saved, the card drawn again.
fn edit(v: &mut LogView, entry: &str, window: &mut Window, cx: &mut Context<LogView>, f: impl FnOnce(&mut Card)) {
    let chat = v.chat(cx);
    let Some(k) = v.st.questions.cards.get_mut(entry) else { return };
    f(k);
    if let Some(ui) = &k.ui {
        for (i, p) in k.picks.iter().enumerate() {
            for (st, want) in [(ui.other.get(i), &p.other), (ui.note.get(i), &p.note)] {
                if let Some(st) = st { if st.read(cx).value().as_ref() != want.as_str() { let t = want.clone(); st.update(cx, |st, cx| st.set_value(t, window, cx)); } }
            }
        }
    }
    let saved = picks_value(k);
    save_draft(cx, &chat, entry, saved);
    cx.notify();
}

fn focus_radio(v: &LogView, entry: &str, qi: usize, oi: usize, window: &mut Window, cx: &mut App) {
    if let Some(h) = v.st.questions.cards.get(entry).and_then(|k| k.ui.as_ref()).and_then(|u| u.radios.get(qi)).and_then(|r| r.get(oi)) { h.focus(window, cx); }
}

fn focus_input(st: Option<Entity<w::InputState>>, window: &mut Window, cx: &mut Context<LogView>) {
    if let Some(st) = st { st.update(cx, |st, cx| st.focus(window, cx)); }
}

/// A press on a choice (or Space): picked, and the keyboard on it.
fn do_pick(v: &mut LogView, entry: &str, qi: usize, oi: usize, window: &mut Window, cx: &mut Context<LogView>) {
    edit(v, entry, window, cx, |k| { let l = k.label(qi, oi); if let (Some(qq), Some(p)) = (k.qs.get(qi).cloned(), k.picks.get_mut(qi)) { q::pick(&qq, p, &l); } });
    focus_radio(v, entry, qi, oi, window, cx);
}

/// The choice an arrow lands on: the next (or previous), wrapping.
fn wrap(oi: i32, d: i32, n: i32) -> usize { (((oi + d) % n + n) % n) as usize }

/// An arrow in a radio group: to the next (or previous) choice, wrapping, picked.
fn do_move(v: &mut LogView, entry: &str, qi: usize, oi: i32, d: i32, window: &mut Window, cx: &mut Context<LogView>) {
    let n = v.st.questions.cards.get(entry).and_then(|k| k.qs.get(qi)).map(|x| x.options.len() as i32).unwrap_or(0);
    if n == 0 { return; }
    do_pick(v, entry, qi, wrap(oi, d, n), window, cx);
}

/// The note button: the choice picked, its note open, the note's box focused.
fn do_note(v: &mut LogView, entry: &str, qi: usize, oi: usize, window: &mut Window, cx: &mut Context<LogView>) {
    ensure_ui(v, entry, window, cx);
    edit(v, entry, window, cx, |k| { let l = k.label(qi, oi); if let (Some(qq), Some(p)) = (k.qs.get(qi).cloned(), k.picks.get_mut(qi)) { q::open_note(&qq, p, &l); } });
    let st = v.st.questions.cards.get(entry).and_then(|k| k.ui.as_ref()).and_then(|u| u.note.get(qi).cloned());
    focus_input(st, window, cx);
}

fn do_note_remove(v: &mut LogView, entry: &str, qi: usize, window: &mut Window, cx: &mut Context<LogView>) {
    edit(v, entry, window, cx, |k| { if let Some(p) = k.picks.get_mut(qi) { q::remove_note(p); } });
}

// ─── Answering ───

fn post_raw(api: &crundi_core::api::Api, path: &str, body: &Value) -> Result<(u16, Value), String> {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    let (status, text) = api.post_with_progress(path, &bytes, &|_, _| {}).map_err(|e| e.to_string())?;
    if status == 0 { return Err("Network error".into()); }
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

/// Submit (or Enter in Other) and Dismiss. A live question goes to
/// `respond`; an orphaned one to `answer-closed`. The card shows its answer
/// at once (no double answers) and its draft goes. A refused `respond`
/// puts the card back with the chat's error line (the web leaves it looking
/// answered: a named difference); a refused `answer-closed` adds an error
/// entry under the card, which stays answered.
fn answer(v: &mut LogView, entry: &str, dismiss: bool, cx: &mut Context<LogView>) {
    let chat = v.chat(cx);
    let Some(k) = v.st.questions.cards.get(entry) else { return };
    if !dismiss && !k.can() { return; }
    let (qs, picks) = (k.qs.clone(), k.picks.clone());
    let keep = picks_value(k);
    let model = v.model.clone();
    let id = entry.to_string();
    let job = model.update(cx, |m, cx| {
        let e = m.conv.entries.iter_mut().find(|x| s(x, "id") == id)?;
        let status = s(e, "status");
        if !q::open(&status) { return None; }
        let input = e.get("input").cloned().unwrap_or(json!({}));
        let prev = e.clone();
        let orphaned = q::orphaned(&status);
        let body;
        if orphaned {
            let answers = if dismiss { json!({}) } else { q::answers(&qs, &picks) };
            let text = if dismiss { String::new() } else { q::closed_text(&qs, &picks) };
            let mut ai = input.clone();
            ai["answers"] = answers.clone();
            e["status"] = json!("answered-late");
            e["answeredInput"] = ai;
            body = json!({ "entryId": id, "answers": answers, "text": text });
        } else if dismiss {
            e["status"] = json!("denied");
            body = json!({ "requestId": s(e, "requestId"), "behavior": "deny", "message": "The user dismissed the question." });
        } else {
            let upd = q::updated_input(&input, &qs, &picks);
            e["status"] = json!("allowed");
            e["answeredInput"] = upd.clone();
            body = json!({ "requestId": s(e, "requestId"), "behavior": "allow", "updatedInput": upd });
        }
        m.error.clear();
        cx.emit(ChatEvent::Changed);
        cx.notify();
        Some((orphaned, body, prev))
    });
    let Some((orphaned, body, prev)) = job else { return };
    v.st.questions.cards.remove(entry);
    save_draft(cx, &chat, entry, None);
    cx.notify();
    let Some(api) = crundi_shell::api(cx) else { return };
    let path = format!("/api/ui-sessions/{chat}/{}", if orphaned { "answer-closed" } else { "respond" });
    let me = cx.entity().downgrade();
    crundi_shell::run(cx, move || post_raw(&api, &path, &body), move |r, cx| {
        let err = match r {
            Ok((404, _)) if orphaned => Some("Answering an old question is not supported by this server".to_string()),
            Ok((_, v)) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(v.get("error").and_then(|e| e.as_str()).unwrap_or("The answer was not taken").to_string()),
            Ok((st, _)) if st >= 400 => Some(format!("The answer was not taken (HTTP {st})")),
            Ok(_) => None,
            Err(e) => Some(e),
        };
        let Some(err) = err else { return };
        if !orphaned {
            // The card back as it was, its picks and draft with it.
            save_draft(cx, &chat, &id, keep);
            model.update(cx, |m, cx| {
                if let Some(e) = m.conv.entries.iter_mut().find(|x| s(x, "id") == id) {
                    if !q::open(&s(e, "status")) && s(e, "status") != s(&prev, "status") { *e = prev.clone(); }
                }
                m.error = err;
                cx.emit(ChatEvent::Changed);
                cx.notify();
            });
        } else {
            // The server could not pass it on (the session gone): an error
            // entry under the card (the web's appendLocalError).
            let n = me.update(cx, |v, _| { v.st.questions.local_errors += 1; v.st.questions.local_errors }).unwrap_or(1);
            model.update(cx, |m, cx| {
                m.conv.entries.push(json!({ "id": format!("local-q-error-{n}"), "kind": "error", "text": err }));
                cx.emit(ChatEvent::NewEntry);
                cx.notify();
            });
        }
    });
}

/// A permission card's Allow / Allow always / Deny (the Slint client's
/// app::respond).
fn respond(v: &LogView, request_id: String, behavior: &str, always: bool, cx: &mut App) {
    let chat = v.chat(cx);
    let mut body = json!({ "requestId": request_id, "behavior": behavior });
    if always { body["always"] = json!(true); }
    let model = v.model.clone();
    crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{chat}/respond"), &body), move |r, cx| {
        if let Err(e) = r { model.update(cx, |m, cx| { m.error = e; cx.notify(); }); }
    });
}

// ─── The driver ───

/// This feature's fields in the chat's props: the latest question card as
/// the Slint client's `q_of` (its entry, status, the picks as the draft
/// keeps them, Submit, the answered tags).
pub fn props(v: &LogView, cx: &App, o: &mut Map<String, Value>) {
    let chat = v.chat(cx);
    let m = v.model.read(cx);
    let Some(e) = m.conv.entries.iter().rev().find(|e| s(e, "kind") == "question") else { o.insert("question".into(), Value::Null); return };
    let id = s(e, "id");
    let status = s(e, "status");
    let draft = draft_of(cx, &chat, &id);
    // A card not made yet (the view opened on a loaded chat): as it will be.
    let tmp;
    let card = match v.st.questions.cards.get(&id) {
        Some(k) => Some(k),
        None if q::open(&status) => {
            let qs = q::parse(e.get("input").unwrap_or(&Value::Null));
            let picks = draft.as_ref().map(|d| q::from_value(d, &qs)).unwrap_or_default();
            tmp = Card::new(qs, picks);
            Some(&tmp)
        }
        None => None,
    };
    let d = q::done(e);
    o.insert("question".into(), json!({
        "entry": id, "status": status, "open": q::open(&status), "orphaned": q::orphaned(&status),
        "can": card.map(|k| k.can()).unwrap_or(false),
        "picks": card.map(|k| q::to_value(&k.picks)).unwrap_or(Value::Null),
        "questions": card.map(|k| k.qs.len()).unwrap_or(0),
        "draft": draft.unwrap_or(Value::Null),
        "tags": d.tags, "notes": d.notes, "dismissed": d.dismissed,
    }));
}

/// This feature's driver actions (the Slint client's q_drive): on the
/// chat's latest open question card.
pub fn act(name: &str, args: &[String], v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    let what = name.strip_prefix("q-")?;
    if !["pick", "note", "note-type", "note-remove", "other", "other-focus", "move", "submit", "dismiss"].contains(&what) { return None; }
    changed(v, cx);
    let entry = v.model.read(cx).conv.entries.iter().rev().find(|e| s(e, "kind") == "question" && q::open(&s(e, "status"))).map(|e| s(e, "id"));
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
        // The keyboard to a question's Other (as a click in it).
        "other-focus" => { let st = v.st.questions.cards.get(&entry).and_then(|k| k.ui.as_ref()).and_then(|u| u.other.get(qi).cloned()); focus_input(st, window, cx); }
        "move" => do_move(v, &entry, qi, n(1), n(2), window, cx),
        "submit" => answer(v, &entry, false, cx),
        "dismiss" => answer(v, &entry, true, cx),
        _ => {}
    }
    Some(json!({ "ok": true }))
}

// ─── Drawing ───

/// A question's or a permission's card.
pub fn row(v: &mut LogView, e: &Shown, window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    if e.kind == "permission" { return Some(permission(v, e, cx)); }
    let status = s(&e.raw, "status");
    let frame = || div().flex().flex_col().gap(T::SP_12).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::accent()).bg(T::accent_dim());
    if !q::open(&status) {
        // Answered: a tag per answer (or dismissed, or answered), by Cru,
        // each note on its own line.
        let d = q::done(&e.raw);
        let mut tags = div().flex().flex_wrap().gap(T::SP_6);
        if d.dismissed { tags = tags.child(w::tag("dismissed", "error")); }
        else if d.tags.is_empty() { tags = tags.child(w::tag("answered", "")); }
        for t in &d.tags { tags = tags.child(w::tag(t.clone(), "ok")); }
        if d.by_cru { tags = tags.child(w::tag("by Cru", "")); }
        return Some(frame().child(tags)
            .when(!d.notes.is_empty(), |f| f.child(div().flex().flex_col().children(d.notes.iter().map(|n| div().text_size(T::TEXT_11).text_color(T::text_muted()).child(n.clone())))))
            .into_any_element());
    }
    if !v.st.questions.cards.contains_key(&e.id) { changed(v, cx); }
    ensure_ui(v, &e.id, window, cx);
    let Some(k) = v.st.questions.cards.get(&e.id) else { return None };
    let Some(ui) = k.ui.as_ref() else { return None };
    let entry = e.id.clone();
    let can = k.can();
    let (qs, picks) = (k.qs.clone(), k.picks.clone());
    let (others, notes, radios) = (ui.other.clone(), ui.note.clone(), ui.radios.clone());
    let mut card = frame();
    if q::orphaned(&status) {
        card = card.child(div().text_size(T::TEXT_12).text_color(T::text_secondary())
            .child("This question outlived the session that asked it. Answering sends your choice as a message."));
    }
    for (qi, qq) in qs.iter().enumerate() {
        let p = picks.get(qi).cloned().unwrap_or_default();
        let mut col = div().flex().flex_col();
        if !qq.header.is_empty() { col = col.child(div().flex().pb(T::SP_4).child(w::badge(qq.header.to_uppercase()))); }
        col = col.child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(qq.question.clone()));
        let mut opts = div().flex().flex_col().gap(T::SP_6).pt(T::SP_8);
        for (oi, o) in qq.options.iter().enumerate() {
            let on = p.chosen.contains(&o.label);
            let note_open = !p.note_opt.is_empty() && p.note_opt == o.label;
            let h = radios.get(qi).and_then(|r| r.get(oi)).cloned().unwrap_or_else(|| cx.focus_handle());
            let focused = h.is_focused(window);
            let multi = qq.multi;
            let (e1, e2, e3, e4) = (entry.clone(), entry.clone(), entry.clone(), entry.clone());
            let mut text = div().flex().flex_col().flex_1().min_w_0()
                .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(o.label.clone()))
                .when(!o.desc.is_empty(), |d| d.child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(o.desc.clone())));
            if !o.preview.is_empty() {
                text = text.child(div().mt(T::SP_6).py(T::SP_6).px(T::SP_8).rounded(T::R_CHIP).border_1().border_color(T::border_subtle()).bg(T::bg_secondary())
                    .font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_secondary()).child(o.preview.clone()));
            }
            if note_open {
                if let Some(ns) = notes.get(qi) {
                    // A press in the note does not toggle the row.
                    text = text.child(div().flex().items_center().gap(T::SP_6).pt(T::SP_8)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(div().flex_1().min_w_0().text_size(T::TEXT_12).child(w::input(ns)))
                        .child(w::icon_button(SharedString::from(format!("qnx-{entry}-{qi}")), IconName::X, "Remove the note")
                            .on_click(cx.listener(move |this, _, window, cx| { cx.stop_propagation(); do_note_remove(this, &e3, qi, window, cx); }))));
                }
            }
            let row = div().id(SharedString::from(format!("qo-{entry}-{qi}-{oi}"))).relative()
                .track_focus(&h)
                .flex().items_start().gap(T::SP_8).py(T::SP_8).pl(T::SP_10).pr(T::SP_32 + T::SP_4)
                .rounded(T::R_CONTROL).border_1().cursor_pointer()
                .border_color(if on { T::accent() } else { T::border() })
                .bg(if on { T::accent_dim() } else { T::bg() })
                .when(!on, |d| d.hover(|d| d.border_color(T::accent_hover())))
                .child(w::choice_mark(multi, on, focused))
                .child(text)
                .child(div().absolute().top(T::SP_4).right(T::SP_4)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(w::icon_toggle(SharedString::from(format!("qn-{entry}-{qi}-{oi}")), IconName::FileText, "Add a note to this choice", note_open)
                        .on_click(cx.listener(move |this, _, window, cx| { cx.stop_propagation(); do_note(this, &e2, qi, oi, window, cx); }))))
                .on_click(cx.listener(move |this, _, window, cx| do_pick(this, &e1, qi, oi, window, cx)))
                // The keyboard: Space picks (or toggles); in a radio group
                // the arrows move to and pick the next choice, wrapping.
                .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
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
        if let Some(os) = others.get(qi) { col = col.child(div().pt(T::SP_4).text_size(T::TEXT_12).child(w::input(os))); }
        card = card.child(col);
    }
    let (es, ed) = (entry.clone(), entry.clone());
    card = card.child(div().flex().flex_wrap().gap(T::SP_8)
        .child(w::disabled(w::primary(SharedString::from(format!("qs-{entry}")), "Submit"), !can)
            .on_click(cx.listener(move |this, _, _, cx| answer(this, &es, false, cx))))
        .child(w::danger(SharedString::from(format!("qd-{entry}")), "Dismiss")
            .on_click(cx.listener(move |this, _, _, cx| answer(this, &ed, true, cx)))));
    Some(card.into_any_element())
}

/// A permission card (the web's permissionNode): yellow; pending: its
/// title, description, reason, the input, then Allow / Allow always / Deny
/// (or, escalated, who decides); answered: the tool's name and its tag.
fn permission(v: &mut LogView, e: &Shown, cx: &mut Context<LogView>) -> AnyElement {
    let mut c = div().flex().flex_col().gap(T::SP_8).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::yellow()).bg(T::yellow_dim()).text_size(T::TEXT_12);
    let tag = e.group_summary.clone();
    if e.status == "pending" {
        c = c.child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(e.title.clone()))
            .when(!e.detail.is_empty(), |d| d.child(div().text_color(T::text_secondary()).child(e.detail.clone())))
            .when(!e.note.is_empty(), |d| d.child(div().text_color(T::text_muted()).child(e.note.clone())))
            .child(div().id(SharedString::from(format!("pp-{}", e.id))).max_h(T::SP_48 * 4.).overflow_y_scroll().py(T::SP_8).px(T::SP_10)
                .rounded(T::R_CONTROL).border_1().border_color(T::border_subtle()).bg(T::bg())
                .font_family(T::MONO).text_color(T::text_secondary()).child(e.args.clone()));
        if !tag.is_empty() {
            c = c.child(div().text_color(T::text_muted()).child(if tag == "manager" { "Sent to the Manager of this factory, who decides. You will see the answer here." } else { "Sent to the project owner. You will see the answer here." }));
            if tag == "escalated" {
                let id = e.group.clone();
                c = c.child(div().flex().child(w::button(SharedString::from(format!("pc-{}", e.id)), "Carry on without this").on_click(move |_, _, cx| {
                    let body = json!({ "id": id });
                    crundi_shell::call(cx, move |api| api.post("/api/collab/cancel", &body), |_, _| {});
                })));
            }
        } else {
            let me = cx.entity().downgrade();
            let btn = |b: gpui_component::button::Button, behavior: &'static str, always: bool, rid: String, me: WeakEntity<LogView>| b.on_click(move |_, _, cx| {
                let rid = rid.clone();
                let _ = me.update(cx, |v, cx| respond(v, rid, behavior, always, cx));
            });
            let rid = e.request_id.clone();
            c = c.child(div().flex().flex_wrap().gap(T::SP_8)
                .child(btn(w::primary(SharedString::from(format!("pa-{}", e.id)), "Allow"), "allow", false, rid.clone(), me.clone()))
                .when(!e.options.is_empty(), |d| d.child(btn(w::button(SharedString::from(format!("pA-{}", e.id)), "Allow always"), "allow", true, rid.clone(), me.clone())))
                .child(btn(w::danger(SharedString::from(format!("pd-{}", e.id)), "Deny"), "deny", false, rid.clone(), me.clone())));
        }
    } else {
        c = c.child(div().flex().items_center().gap(T::SP_6)
            .child(div().text_size(T::TEXT_13).font_weight(T::W_BOLD).text_color(T::text()).child(e.text.clone()))
            .child(w::tag(tag, if e.error { "error" } else { "ok" })))
            .when(!e.group.is_empty(), |d| d.child(div().text_color(T::text_secondary()).child(e.group.clone())));
    }
    let _ = v;
    c.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{wrap, picks_value, Card, Q};
    use crundi_core::questions as q;
    use serde_json::json;

    fn qs() -> Vec<Q> {
        q::parse(&json!({ "questions": [
            { "question": "A?", "options": [{ "label": "x" }, { "label": "y" }] },
            { "question": "B?", "multiSelect": true, "options": [{ "label": "u" }, { "label": "v" }] },
        ] }))
    }

    #[test]
    fn arrows_wrap() {
        assert_eq!(wrap(1, 1, 2), 0);
        assert_eq!(wrap(0, -1, 2), 1);
        assert_eq!(wrap(0, 1, 3), 1);
    }

    #[test]
    fn a_card_from_its_draft() {
        let mut k = Card::new(qs(), vec![]);
        assert_eq!(k.picks.len(), 2);
        assert!(picks_value(&k).is_none());
        assert!(!k.can());
        let q0 = k.qs[0].clone();
        let label = k.label(0, 1);
        q::pick(&q0, &mut k.picks[0], &label);
        q::type_other(&mut k.picks[1], "mine");
        assert!(k.can());
        let saved = picks_value(&k).unwrap();
        let back = Card::new(qs(), q::from_value(&saved, &qs()));
        assert_eq!(back.picks, k.picks);
    }
}
