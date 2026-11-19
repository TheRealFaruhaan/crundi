//! 2.20: a factory's result card (cc-fr-*) and a widget check's screenshots
//! row (specs/cards.md), as the Slint client (app.rs fr_view / fr_* /
//! fr_of, fetch_shots; ui/chat.slint FactoryCard, ShotsRow). Every rule is
//! crundi_core::fresult's.
use crate::LogView;
use crundi_core::{api::{ApiError, ApiResult}, chat::Shown, fresult};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

/// A result card's own state, by entry id: kept across the server's patches.
#[derive(Default)]
pub struct Fr {
    pub files_open: bool,
    pub fu_open: bool,
    pub note_open: bool,
    /// The note of Request changes (made when it first opens; None: empty).
    pub note: Option<Entity<w::TextareaState>>,
    pub diff_loading: bool,
    /// The full diff: its lines and the longest line's length, or the error.
    pub diff: Option<Result<(Vec<String>, usize), String>>,
    pub busy: bool,
    pub armed: bool,
    pub arm_gen: u32,
    pub act_err: String,
    /// The card's state when last seen: a new state drops the loaded diff.
    pub state: String,
    /// The card as last seen: any patch clears a choice's error (F124).
    pub raw: String,
}

/// One screenshot of a widget check: (caption, picture key, width, height).
#[derive(Clone, Debug, PartialEq)]
pub struct Shot { pub caption: String, pub key: String, pub w: f32, pub h: f32 }

#[derive(Default)]
pub struct State {
    pub fr: HashMap<String, Fr>,
    /// The last choice's body posted (the driver's fr.last).
    pub last: Value,
    /// Widget checks' screenshots by widget id (asked once each).
    pub shots: HashMap<String, Vec<Shot>>,
    pub shots_asked: HashSet<String>,
    /// Pictures by key: (loading | ready | gone | empty, the picture).
    pub pics: HashMap<String, (String, Option<Arc<Image>>)>,
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// A query parameter, percent-encoded.
pub fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The driver's canned answer as a server's (a delay, a status, a network failure).
fn stub_result(v: Value) -> ApiResult {
    if let Some(ms) = v.get("delayMs").and_then(|x| x.as_u64()) { std::thread::sleep(Duration::from_millis(ms)); }
    match v.get("status").and_then(|x| x.as_u64()) {
        Some(st) if st >= 400 => Err(ApiError::Status(st as u16, s(&v, "error"))),
        _ if v.get("net").is_some() => Err(ApiError::Net(s(&v, "net"))),
        _ => Ok(v),
    }
}

/// A choice's answer as the card's error line (None: accepted).
pub fn choice_error(r: &Result<Value, String>) -> Option<String> {
    match r {
        Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some({ let e = s(v, "error"); if e.is_empty() { "That did not work.".to_string() } else { e } }),
        Ok(_) => None,
        Err(e) if e == "Not supported by this server" => Some("Not supported by this server.".into()),
        Err(e) if e.starts_with("Cannot reach the server") => Some("Could not reach the server.".into()),
        Err(e) => Some(e.clone()),
    }
}

/// A widget check's renders list → its shots (caption "frame" or "frame · state").
pub fn shots_of(v: &Value, project: &str, wid: &str, chat: &str) -> Option<Vec<Shot>> {
    if v.get("ok").and_then(|o| o.as_bool()) != Some(true) { return None; }
    let at = v.pointer("/renders/at").map(|x| match x { Value::String(t) => t.clone(), o => o.to_string() }).unwrap_or_default();
    Some(v.pointer("/renders/shots").and_then(|x| x.as_array()).map(|a| a.iter().map(|sh| {
        let (frame, state) = (s(sh, "frame"), s(sh, "state"));
        let caption = if state.is_empty() || state == "live" { frame.clone() } else { format!("{frame} · {state}") };
        let img = format!("/api/widgets/render-image?project={}&id={}&name={}&t={}", enc(project), enc(wid), enc(&format!("{frame}--{state}")), enc(&at));
        let n = |k: &str| sh.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        Shot { caption, key: format!("{chat}|@render:{img}"), w: n("width"), h: n("height") }
    }).collect()).unwrap_or_default())
}

/// The summary's blocks: (kind p | h | li | code, marker, text).
fn blocks(summary: &str) -> Vec<(&'static str, String, String)> {
    if summary.trim().is_empty() { return vec![]; }
    crate::rows::split_blocks("fr", summary).into_iter().filter_map(|r| match r {
        crate::rows::Row::Prose(_, t, kind, marker, _) => Some((if kind.starts_with("li") { "li" } else if kind.starts_with('h') { "h" } else { "p" }, marker, t.to_string())),
        crate::rows::Row::Code(_, t, _) => Some(("code", String::new(), t)),
        _ => None,
    }).collect()
}

fn fr_entries(v: &LogView, cx: &App) -> Vec<Shown> { v.shown(cx).iter().filter(|e| e.kind == "factory-result").cloned().collect() }

fn note_text(f: &Fr, cx: &App) -> String { f.note.as_ref().map(|n| n.read(cx).value().to_string()).unwrap_or_default() }

/// The conversation changed: each card follows its entry (a new state drops
/// a loaded diff, a running choice, an armed Discard; changes accepted close
/// the note; any patch drops a choice's error), and widget checks ask for
/// their screenshots.
pub fn changed(v: &mut LogView, cx: &mut Context<LogView>) {
    let shown = v.shown(cx);
    for e in shown.iter().filter(|e| e.kind == "factory-result") {
        let f = v.st.cards.fr.entry(e.id.clone()).or_default();
        if f.state != e.status {
            if !f.state.is_empty() { f.diff = None; f.diff_loading = false; }
            f.busy = false; f.armed = false; f.state = e.status.clone();
            if e.status == "changes" { f.note_open = false; f.note = None; }
        }
        let raw = e.raw.to_string();
        if f.raw != raw { if !f.raw.is_empty() { f.act_err.clear(); } f.raw = raw; }
    }
    let wids: Vec<String> = shown.iter().filter(|e| e.kind == "tool" && e.status == "done" && !e.error && !e.widget.is_empty()).map(|e| e.widget.clone()).collect();
    for wid in wids { fetch_shots(v, &wid, cx); }
}

/// A widget check's screenshots: the list once per widget; none (or not ok)
/// draws nothing. Their pictures are fetched as the list arrives.
fn fetch_shots(v: &mut LogView, wid: &str, cx: &mut Context<LogView>) {
    if !v.st.cards.shots_asked.insert(wid.to_string()) { return; }
    let project = crundi_shell::project(cx);
    let chat = v.chat(cx);
    let path = format!("/api/widgets/renders?project={}&id={}", enc(&project), enc(wid));
    let stub = crundi_core::driver::take_stub("renders");
    let me = cx.entity().downgrade();
    let wid = wid.to_string();
    crundi_shell::call(cx, move |api| match stub { Some(v) => stub_result(v), None => api.get(&path) }, move |r, cx| {
        let Some(list) = r.ok().and_then(|v| shots_of(&v, &project, &wid, &chat)) else { return };
        let _ = me.update(cx, |v, cx| {
            for sh in &list { fetch_pic(v, &sh.key, cx); }
            v.st.cards.shots.insert(wid, list);
            cx.notify();
        });
    });
}

fn format_of(b: &[u8]) -> ImageFormat {
    if b.starts_with(b"\x89PNG") { ImageFormat::Png } else if b.starts_with(&[0xff, 0xd8]) { ImageFormat::Jpeg } else if b.starts_with(b"GIF") { ImageFormat::Gif } else if b.len() > 12 && &b[8..12] == b"WEBP" { ImageFormat::Webp } else { ImageFormat::Png }
}

/// A screenshot's picture ("<chat>|@render:<path>"): ready, gone (404 / 410),
/// empty; a network failure is forgotten (asked again on the next draw).
fn fetch_pic(v: &mut LogView, key: &str, cx: &mut Context<LogView>) {
    if v.st.cards.pics.contains_key(key) { return; }
    let Some(path) = key.split_once("|@render:").map(|x| x.1.to_string()) else { return };
    let Some(api) = crundi_shell::api(cx) else { return };
    v.st.cards.pics.insert(key.to_string(), ("loading".into(), None));
    let me = cx.entity().downgrade();
    let key = key.to_string();
    crundi_shell::run(cx, move || api.get_bytes(&path), move |r, cx| {
        let _ = me.update(cx, |v, cx| {
            match r {
                Ok((200, b)) if !b.is_empty() => { let f = format_of(&b); v.st.cards.pics.insert(key, ("ready".into(), Some(Arc::new(Image::from_bytes(f, b))))); }
                Ok((200, _)) => { v.st.cards.pics.insert(key, ("empty".into(), None)); }
                Ok((404, _)) | Ok((410, _)) => { v.st.cards.pics.insert(key, ("gone".into(), None)); }
                _ => { v.st.cards.pics.remove(&key); }
            }
            cx.notify();
        });
    });
}

/// A widget_render call's screenshots as the driver lists them (the Slint
/// client's tools_of "shots": caption, key, state).
pub fn shots_json(v: &LogView, t: &Shown, _cx: &App) -> Value {
    if t.widget.is_empty() || t.status != "done" || t.error { return json!([]); }
    let list = v.st.cards.shots.get(&t.widget).cloned().unwrap_or_default();
    Value::Array(list.iter().map(|sh| json!({ "caption": sh.caption, "key": sh.key, "state": v.st.cards.pics.get(&sh.key).map(|p| p.0.clone()).unwrap_or_default() })).collect())
}

/// A widget check's screenshots row under its widget_render call (None:
/// not one, or no shots): shot-height pictures on the log's colour, each
/// captioned; the row scrolls sideways.
pub fn shots_row(v: &mut LogView, t: &Shown, _window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    if t.widget.is_empty() || t.status != "done" || t.error { return None; }
    fetch_shots(v, &t.widget, cx);
    let list = v.st.cards.shots.get(&t.widget).cloned().unwrap_or_default();
    if list.is_empty() { return None; }
    let shot_h = T::SP_48 * 3. + T::SP_6; // Theme.shot-h (150): asked as a named size (requests.md)
    let mut row = div().id(SharedString::from(format!("shots-{}", t.id))).flex().gap(T::SP_8).pt(T::SP_8).overflow_x_scroll();
    for sh in list {
        let pic = v.st.cards.pics.get(&sh.key).cloned();
        if pic.is_none() { fetch_pic(v, &sh.key, cx); }
        let (iw, ih) = match pic.as_ref().and_then(|p| p.1.as_ref()) {
            Some(_) if sh.h > 0. => (sh.w, sh.h),
            _ if sh.h > 0. => (sh.w, sh.h),
            _ => (1., 1.),
        };
        let ratio = if ih > 0. { iw / ih } else { 1. };
        let mut b = div().flex_none().w(shot_h * ratio).h(shot_h).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg()).overflow_hidden().p(px(1.));
        if let Some((st, Some(im))) = pic { if st == "ready" { b = b.child(img(im).size_full().object_fit(ObjectFit::Contain)); } }
        row = row.child(div().flex().flex_col().gap(T::SP_4).child(b)
            .child(div().font_family(T::MONO).text_size(T::TEXT_10).text_color(T::text_muted()).child(sh.caption.clone())));
    }
    Some(row.into_any_element())
}

/// This feature's fields in the chat's props: "fr" (fr_of's shape).
pub fn props(v: &LogView, cx: &App, o: &mut Map<String, Value>) {
    let cards: Vec<Value> = fr_entries(v, cx).iter().map(|e| {
        let none = Fr::default();
        let f = v.st.cards.fr.get(&e.id).unwrap_or(&none);
        let r = &e.raw;
        let (add, del, n) = fresult::counts(r);
        let (files, more) = fresult::files(r);
        let fus = fresult::followups(r);
        let branch = s(r, "branch");
        let (dl, dh, derr) = match &f.diff { Some(Ok((l, _))) => (l.len(), l.first().cloned().unwrap_or_default(), String::new()), Some(Err(x)) => (0, String::new(), x.clone()), None => (0, String::new(), String::new()) };
        json!({
            "id": e.id, "state": e.status, "title": fresult::title(r), "blocks": blocks(&s(r, "summary")).len(),
            "line": format!("{add} {del} {n}{}", if branch.is_empty() { String::new() } else { format!(" on {branch}") }),
            "filesOpen": f.files_open, "files": files.iter().map(|(a, b, c)| format!("{a} {b} {c}")).collect::<Vec<_>>(), "more": more,
            "canDiff": fresult::can_diff(r), "diffLoading": f.diff_loading, "diffOn": f.diff.is_some(), "diffLines": dl, "diffHead": dh, "diffErr": derr,
            "msg": fresult::message(r), "usage": fresult::usage(r), "fuLine": fresult::followups_line(fus.len()), "fuOpen": f.fu_open,
            "fus": fus.iter().map(|x| json!({ "id": x.id, "blocking": x.blocking, "text": x.text, "where": x.place })).collect::<Vec<_>>(),
            "error": s(r, "error"), "buttons": fresult::buttons(r, f.armed).iter().map(|b| b.label.clone()).collect::<Vec<_>>(), "busy": f.busy,
            "noteOpen": f.note_open && e.status == "ask", "note": note_text(f, cx), "actErr": f.act_err,
        })
    }).collect();
    o.insert("fr".into(), json!({ "cards": cards, "last": v.st.cards.last }));
}

/// The driver's actions (the Slint client's FrAct): fr-toggle <chat> <id>
/// files|fu, fr-press <chat> <id> <key>, fr-note <chat> <id> <text>,
/// fr-send <chat> <id>, fr-diff <chat> <id>. The named chat's card (its log
/// when it is not the focused one).
pub fn act(name: &str, args: &[String], v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    if !matches!(name, "fr-toggle" | "fr-press" | "fr-note" | "fr-send" | "fr-diff") { return None; }
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let chat = a(0);
    if !chat.is_empty() && chat != v.chat(cx) {
        let other = crate::view_of(cx, &chat)?;
        let (name, args) = (name.to_string(), args.to_vec());
        return other.update(cx, |o, cx| run_act(&name, &args, o, window, cx));
    }
    run_act(name, args, v, window, cx)
}

fn run_act(name: &str, args: &[String], v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let id = a(1);
    match name {
        "fr-toggle" => toggle(v, &id, &a(2), cx),
        "fr-press" => press(v, &id, &a(2), window, cx),
        "fr-note" => {
            let text = a(2);
            let n = note_state(v, &id, window, cx);
            n.update(cx, |st, cx| st.set_value(text, window, cx));
            cx.notify();
        }
        "fr-send" => send(v, &id, window, cx),
        "fr-diff" => diff(v, &id, cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

fn toggle(v: &mut LogView, id: &str, what: &str, cx: &mut Context<LogView>) {
    let f = v.st.cards.fr.entry(id.to_string()).or_default();
    if what == "fu" { f.fu_open = !f.fu_open; } else { f.files_open = !f.files_open; }
    cx.notify();
}

fn note_state(v: &mut LogView, id: &str, window: &mut Window, cx: &mut Context<LogView>) -> Entity<w::TextareaState> {
    let f = v.st.cards.fr.entry(id.to_string()).or_default();
    if let Some(n) = &f.note { return n.clone(); }
    let n = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(3, 6).placeholder("What should change?"));
    f.note = Some(n.clone());
    n
}

/// A button: Request changes opens the note (and gives it the keyboard);
/// Discard arms for 3 s first; the others (and Close the run) post their choice.
fn press(v: &mut LogView, id: &str, key: &str, window: &mut Window, cx: &mut Context<LogView>) {
    match key {
        "changes" => {
            let open = { let f = v.st.cards.fr.entry(id.to_string()).or_default(); f.note_open = !f.note_open; f.note_open };
            if open { let n = note_state(v, id, window, cx); n.update(cx, |st, cx| st.focus(window, cx)); }
            cx.notify();
        }
        "discard" => {
            let f = v.st.cards.fr.entry(id.to_string()).or_default();
            if f.armed { f.armed = false; return finish(v, id, "discard", None, cx); }
            f.armed = true; f.arm_gen += 1;
            let (g, i) = (f.arm_gen, id.to_string());
            cx.notify();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(3000)).await;
                let _ = this.update(cx, |v, cx| if let Some(f) = v.st.cards.fr.get_mut(&i) { if f.armed && f.arm_gen == g { f.armed = false; cx.notify(); } });
            }).detach();
        }
        "close" => finish(v, id, "discard", None, cx),
        k => finish(v, id, k, None, cx),
    }
}

/// Send to the Manager: an empty note only takes the keyboard back.
fn send(v: &mut LogView, id: &str, window: &mut Window, cx: &mut Context<LogView>) {
    let note = v.st.cards.fr.get(id).map(|f| note_text(f, cx)).unwrap_or_default();
    if note.trim().is_empty() {
        let n = note_state(v, id, window, cx);
        n.update(cx, |st, cx| st.focus(window, cx));
        return cx.notify();
    }
    finish(v, id, "changes", Some(note), cx);
}

fn entry(v: &LogView, id: &str, cx: &App) -> Option<Value> { fr_entries(v, cx).into_iter().find(|e| e.id == id).map(|e| e.raw) }

/// Post a choice: the buttons wait; refused, failed or unsupported: the
/// error line (unless the card already says it, F125), the buttons back.
/// Accepted: the server's patch redraws the card; changes accepted close the note.
fn finish(v: &mut LogView, id: &str, choice: &str, note: Option<String>, cx: &mut Context<LogView>) {
    let Some(e) = entry(v, id, cx) else { return };
    let body = fresult::finish_body(&s(&e, "runId"), choice, note.as_deref());
    { let f = v.st.cards.fr.entry(id.to_string()).or_default(); f.busy = true; f.armed = false; f.act_err.clear(); }
    v.st.cards.last = body.clone();
    cx.notify();
    let stub = crundi_core::driver::take_stub("fr-finish");
    let me = cx.entity().downgrade();
    let (i, choice) = (id.to_string(), choice.to_string());
    crundi_shell::call(cx, move |api| match stub { Some(v) => stub_result(v), None => api.post("/api/factory-runs", &body) }, move |r, cx| {
        let err = choice_error(&r);
        let _ = me.update(cx, |v, cx| {
            let shown = entry(v, &i, cx).map(|x| s(&x, "error")).unwrap_or_default();
            let f = v.st.cards.fr.entry(i.clone()).or_default();
            match err {
                Some(e) => { f.busy = false; if shown != e { f.act_err = e; } }
                None if choice == "changes" => { f.note_open = false; f.note = None; }
                None => {}
            }
            cx.notify();
        });
    });
}

/// Show the full diff: fetched once; a network failure gives the button back.
fn diff(v: &mut LogView, id: &str, cx: &mut Context<LogView>) {
    let Some(e) = entry(v, id, cx) else { return };
    { let f = v.st.cards.fr.entry(id.to_string()).or_default(); if f.diff_loading || f.diff.is_some() { return; } f.diff_loading = true; }
    cx.notify();
    let path = format!("/api/factory-runs/diff?id={}", enc(&s(&e, "runId")));
    let stub = crundi_core::driver::take_stub("fr-diff");
    let me = cx.entity().downgrade();
    let i = id.to_string();
    crundi_shell::call(cx, move |api| match stub { Some(v) => stub_result(v), None => api.get(&path) }, move |r, cx| {
        let _ = me.update(cx, |v, cx| {
            let f = v.st.cards.fr.entry(i).or_default();
            f.diff_loading = false;
            match r {
                Ok(x) => f.diff = Some(fresult::diff_lines(&x)),
                Err(e) if e.starts_with("Cannot reach the server") => {}
                Err(e) => f.diff = Some(Err(e)),
            }
            cx.notify();
        });
    });
}

// ─── Drawing ───

fn hl(color: Hsla) -> HighlightStyle { HighlightStyle { color: Some(color), ..Default::default() } }

/// Pieces of text in one wrapping line, each with its look.
fn spans(parts: &[(String, HighlightStyle)]) -> StyledText {
    let mut text = String::new();
    let mut runs = vec![];
    for (t, st) in parts {
        let a = text.len();
        text.push_str(t);
        runs.push((a..text.len(), *st));
    }
    StyledText::new(text).with_highlights(runs)
}

/// A disclosure line (files, follow-ups): the chevron, then its text.
fn disclosure(id: String, open: bool, text: impl IntoElement, on: impl Fn(&mut LogView, &mut Context<LogView>) + 'static, cx: &mut Context<LogView>) -> Stateful<Div> {
    div().id(SharedString::from(id)).flex().items_start().gap(T::SP_4).cursor_pointer().text_color(T::text_secondary())
        .child(div().pt(T::SP_2).child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM).text_color(T::text_secondary())))
        .child(div().flex_1().min_w_0().child(text))
        .on_click(cx.listener(move |this, _, _, cx| on(this, cx)))
}

/// A factory's result card (.cc-ask.cc-fr): the title, the summary, the
/// files (and the full diff), the state's message, the usage, the
/// follow-ups, the server's error, the buttons, the note of Request
/// changes, and a choice's error.
pub fn row(v: &mut LogView, e: &Shown, window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let r = e.raw.clone();
    let id = e.id.clone();
    let zoom = px(v.model.read(cx).zoom as f32);
    let fs = |t: Pixels| t + zoom;
    let note_input = { let f = v.st.cards.fr.entry(id.clone()).or_default(); if f.note_open && f.note.is_none() { true } else { false } };
    if note_input { let _ = note_state(v, &id, window, cx); }
    let f = v.st.cards.fr.get(&id)?;
    let (add, del, n) = fresult::counts(&r);
    let (files, more) = fresult::files(&r);
    let fus = fresult::followups(&r);
    let branch = s(&r, "branch");
    let msg = fresult::message(&r);
    let usage = fresult::usage(&r);
    let err = s(&r, "error");
    let btns = fresult::buttons(&r, f.armed);
    let (busy, files_open, fu_open, note_open) = (f.busy, f.files_open, f.fu_open, f.note_open && e.status == "ask");
    let (diff_loading, dv) = (f.diff_loading, f.diff.clone());
    let act_err = f.act_err.clone();
    let note = f.note.clone();
    let mut card = div().flex().flex_col().rounded(T::R_CARD).border_1().border_color(T::accent()).bg(T::accent_dim()).p(T::SP_12)
        .child(div().text_size(fs(T::TEXT_13)).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(fresult::title(&r)));
    // The summary: the reply's blocks.
    let bl = blocks(&s(&r, "summary"));
    if bl.is_empty() { card = card.child(div().h(T::SP_4)); } else {
        let mut sum = div().flex().flex_col().gap(T::SP_4).pt(T::SP_6).pb(T::SP_8).text_size(fs(T::TEXT_13)).text_color(T::text());
        for (k, (kind, marker, t)) in bl.into_iter().enumerate() {
            let mid = SharedString::from(format!("fr-md-{id}-{k}"));
            let md = |t: String| w::markdown(mid.clone(), t, |u, _, cx| if !u.is_empty() && std::env::var_os("CRUNDI_TEST_PORT").is_none() { cx.open_url(u) });
            sum = sum.child(match kind {
                "li" => div().flex().child(div().w(T::SP_20).flex_none().text_center().child(marker)).child(div().flex_1().min_w_0().child(md(t))).into_any_element(),
                "code" => div().font_family(T::MONO).text_size(fs(T::TEXT_12)).child(t).into_any_element(),
                "h" => div().text_size(fs(T::TEXT_14)).child(md(t)).into_any_element(),
                _ => div().child(md(t)).into_any_element(),
            });
        }
        card = card.child(sum);
    }
    // The files: "+A −R in N files on <branch>", a disclosure.
    let mut parts = vec![(add, hl(T::green())), (" ".into(), hl(T::text_secondary())), (del, hl(T::red())), (format!(" {n}"), hl(T::text_secondary()))];
    if !branch.is_empty() { parts.push((format!(" on {branch}"), hl(T::text_muted()))); }
    let i2 = id.clone();
    card = card.child(div().text_size(fs(T::TEXT_12)).child(disclosure(format!("fr-files-{id}"), files_open, spans(&parts), move |v, cx| toggle(v, &i2, "files", cx), cx)));
    if files_open {
        let mut list = div().flex().flex_col().gap(T::SP_2).pt(T::SP_6);
        for (name, a, d) in files {
            list = list.child(div().flex().gap(T::SP_10).font_family(T::MONO).text_size(fs(T::TEXT_11))
                .child(div().flex_1().min_w_0().truncate().text_color(T::text()).child(name))
                .child(div().flex().gap(T::SP_4).child(div().text_color(T::green()).child(a)).child(div().text_color(T::red()).child(d))));
        }
        if !more.is_empty() { list = list.child(div().text_size(fs(T::TEXT_12)).text_color(T::text_secondary()).child(more)); }
        if fresult::can_diff(&r) && dv.is_none() {
            let i3 = id.clone();
            list = list.child(div().pt(T::SP_4).child(w::disabled(w::button(SharedString::from(format!("fr-diff-{id}")), "Show the full diff"), diff_loading).w_full()
                .on_click(cx.listener(move |v, _, _, cx| diff(v, &i3, cx)))));
        }
        match &dv {
            Some(Err(x)) => { list = list.child(div().text_size(fs(T::TEXT_11)).text_color(T::text_muted()).child(x.clone())); }
            Some(Ok((lines, _))) => {
                let half = window.viewport_size().height / 2.;
                let mut d = div().id(SharedString::from(format!("fr-dv-{id}"))).mt(T::SP_6).max_h(half).overflow_scroll().font_family(T::MONO).text_size(fs(T::TEXT_11)).text_color(T::text());
                for l in lines { d = d.child(div().whitespace_nowrap().child(if l.is_empty() { " ".to_string() } else { l.clone() })); }
                list = list.child(d);
            }
            None => {}
        }
        card = card.child(list);
    }
    card = card.child(div().h(T::SP_8));
    if !msg.is_empty() {
        card = card.child(div().pb(T::SP_8).text_size(fs(T::TEXT_12)).text_color(T::text()).child(w::markdown(SharedString::from(format!("fr-msg-{id}")), msg, |_, _, _| {})));
    }
    if !usage.is_empty() { card = card.child(div().pt(T::SP_2).pb(T::SP_6).text_size(fs(T::TEXT_11)).text_color(T::text_secondary()).child(usage)); }
    // The follow-ups from review, a disclosure.
    if !fus.is_empty() {
        let i4 = id.clone();
        card = card.child(div().text_size(fs(T::TEXT_12)).child(disclosure(format!("fr-fu-{id}"), fu_open, fresult::followups_line(fus.len()), move |v, cx| toggle(v, &i4, "fu", cx), cx)));
        if fu_open {
            let mut list = div().flex().flex_col().gap(T::SP_2).pt(T::SP_6).text_size(fs(T::TEXT_11));
            for x in &fus {
                let mut p = vec![(x.id.clone(), HighlightStyle { color: Some(T::text()), font_weight: Some(T::W_BOLD), ..Default::default() })];
                if x.blocking { p.push((" blocking".into(), hl(T::red()))); }
                p.push((format!(" {}", x.text), hl(T::text())));
                if !x.place.is_empty() { p.push((format!(" ({})", x.place), hl(T::text_muted()))); }
                list = list.child(div().child(spans(&p)));
            }
            card = card.child(list);
        }
        card = card.child(div().h(T::SP_8));
    }
    if !err.is_empty() { card = card.child(div().pb(T::SP_8).text_size(fs(T::TEXT_11)).text_color(T::text_muted()).child(err)); }
    if !btns.is_empty() {
        let mut bw = div().flex().flex_wrap().gap(T::SP_8);
        for b in btns {
            let (i5, key) = (id.clone(), b.key);
            let bid = SharedString::from(format!("fr-b-{id}-{}", b.key));
            let btn = if b.primary { w::primary(bid, b.label) } else if b.danger { w::danger(bid, b.label) } else { w::button(bid, b.label) };
            bw = bw.child(w::disabled(btn, busy).on_click(cx.listener(move |v, _, window, cx| press(v, &i5, key, window, cx))));
        }
        card = card.child(bw);
    }
    // Request changes: the note (Ctrl/Cmd+Enter sends) and Send to the Manager.
    if note_open {
        if let Some(n) = note {
            let (i6, i7) = (id.clone(), id.clone());
            card = card.child(div().flex().flex_col().gap(T::SP_6).pt(T::SP_8)
                .child(div().text_size(fs(T::TEXT_12)).capture_key_down(cx.listener(move |v, ev: &KeyDownEvent, window, cx| {
                    if ev.keystroke.key == "enter" && (ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform) { send(v, &i6, window, cx); cx.stop_propagation(); }
                })).child(w::textarea(&n)))
                .child(w::disabled(w::primary(SharedString::from(format!("fr-send-{id}")), "Send to the Manager"), busy).w_full()
                    .on_click(cx.listener(move |v, _, window, cx| send(v, &i7, window, cx)))));
        }
    }
    if !act_err.is_empty() { card = card.child(div().pt(T::SP_8).text_size(fs(T::TEXT_11)).text_color(T::text_muted()).child(act_err)); }
    Some(card.into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn shots_and_errors() {
        let v = json!({ "ok": true, "renders": { "at": 1, "shots": [{ "frame": "inline", "state": "live", "width": 600, "height": 300 }, { "frame": "mobile", "state": "dark", "width": 390, "height": 844 }] } });
        let l = shots_of(&v, "p q", "w-check", "c1").unwrap();
        assert_eq!(l[0].caption, "inline");
        assert_eq!(l[1].caption, "mobile · dark");
        assert_eq!(l[1].key, "c1|@render:/api/widgets/render-image?project=p%20q&id=w-check&name=mobile--dark&t=1");
        assert!(shots_of(&json!({ "ok": false }), "p", "w", "c").is_none());
        assert_eq!(choice_error(&Ok(json!({ "ok": false }))).unwrap(), "That did not work.");
        assert_eq!(choice_error(&Err("Not supported by this server".into())).unwrap(), "Not supported by this server.");
        assert_eq!(choice_error(&Err("Cannot reach the server: x".into())).unwrap(), "Could not reach the server.");
        assert!(choice_error(&Ok(json!({ "ok": true }))).is_none());
        assert_eq!(blocks("## Done\n\nThe **site**.\n\n- a\n- b").len(), 4);
        assert!(blocks("  ").is_empty());
    }
}
