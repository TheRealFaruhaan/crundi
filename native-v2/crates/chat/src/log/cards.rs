//! 2.20: a factory's result card (the web's factoryResultNode, `.cc-fr-*`:
//! the title, the summary, the files and the full diff, the state's
//! message, the usage, the follow-ups from review, the server's error, the
//! buttons, Request changes' note) and a widget check's screenshots row
//! under its widget_render call. Every rule is crundi_core::fresult's. A
//! card's own state (open parts, the diff, a running choice) is the chat's
//! (`Shared::fr`); a change of its height measures its row again.

use super::LogView;
use crate::store::{ChatStore, Row};
use crundi_core::{chat::Shown, fresult};
use crundi_theme::T;
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

/// A result card's own state, kept across the server's patches.
#[derive(Default)]
pub struct Fr {
    pub files_open: bool,
    pub fu_open: bool,
    pub note_open: bool,
    pub note: Option<Entity<w::TextareaState>>,
    pub diff_loading: bool,
    pub diff: Option<Result<(Vec<String>, usize), String>>,
    pub busy: bool,
    pub armed: bool,
    arm_gen: u32,
    pub act_err: String,
    state: String,
    raw: String,
}

/// One screenshot of a widget check: (caption, picture key, width, height).
#[derive(Clone, Debug, PartialEq)]
pub struct Shot { pub caption: String, pub key: String, pub w: f32, pub h: f32 }

/// Widget checks' screenshots by widget id (asked once each) and their pictures.
#[derive(Default)]
struct Shots { shots: HashMap<String, Vec<Shot>>, asked: HashSet<String>, pics: HashMap<String, (String, Option<Arc<Image>>)> }
impl Global for Shots {}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

pub fn enc(v: &str) -> String { v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect() }

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

/// A widget check's renders → its shots (caption "frame" or "frame · state").
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

/// The batch changed these entries: each card follows its entry (a new state
/// drops a loaded diff, a running choice, an armed Discard; changes accepted
/// close the note; any patch drops a choice's error).
pub fn follow(st: &mut ChatStore, ids: &[String]) {
    for id in ids {
        let Some(p) = st.conv.position(id) else { continue };
        let e = &st.conv.entries[p];
        if s(e, "kind") != "factory-result" { continue; }
        let state = s(e, "state");
        let raw = e.to_string();
        let f = st.log.fr.entry(id.clone()).or_default();
        if f.state != state {
            if !f.state.is_empty() { f.diff = None; f.diff_loading = false; }
            f.busy = false; f.armed = false; f.state = state.clone();
            if state == "changes" { f.note_open = false; f.note = None; }
        }
        if f.raw != raw { if !f.raw.is_empty() { f.act_err.clear(); } f.raw = raw; }
    }
}

/// Change a card's state; its row measured again.
fn with(store: &Entity<ChatStore>, id: &str, cx: &mut App, f: impl FnOnce(&mut Fr)) {
    let id = id.to_string();
    store.update(cx, |st, cx| { f(st.log.fr.entry(id.clone()).or_default()); st.remeasure(&id); cx.notify(); });
}

fn entry(store: &Entity<ChatStore>, id: &str, cx: &App) -> Option<Value> { let st = store.read(cx); st.conv.position(id).map(|p| st.conv.entries[p].clone()) }

fn note_state(v: &mut LogView, id: &str, window: &mut Window, cx: &mut Context<LogView>) -> Entity<w::TextareaState> {
    if let Some(n) = v.store.read(cx).log.fr.get(id).and_then(|f| f.note.clone()) { return n; }
    let n = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(3, 6).placeholder("What should change?"));
    let n2 = n.clone();
    with(&v.store, id, cx, |f| f.note = Some(n2));
    n
}

/// A button: Request changes opens the note; Discard arms for 3 s first; the others post their choice.
fn press(v: &mut LogView, id: &str, key: &str, window: &mut Window, cx: &mut Context<LogView>) {
    let store = v.store.clone();
    match key {
        "changes" => {
            let mut open = false;
            with(&store, id, cx, |f| { f.note_open = !f.note_open; open = f.note_open; });
            if open { let n = note_state(v, id, window, cx); n.update(cx, |st, cx| st.focus(window, cx)); }
        }
        "discard" => {
            let armed = store.read(cx).log.fr.get(id).is_some_and(|f| f.armed);
            if armed { with(&store, id, cx, |f| f.armed = false); return finish(&store, id, "discard", None, cx); }
            let mut g = 0;
            with(&store, id, cx, |f| { f.armed = true; f.arm_gen += 1; g = f.arm_gen; });
            let i = id.to_string();
            cx.spawn(async move |_, cx: &mut AsyncApp| {
                cx.background_executor().timer(Duration::from_millis(3000)).await;
                let _ = cx.update(|cx| { if store.read(cx).log.fr.get(&i).is_some_and(|f| f.armed && f.arm_gen == g) { with(&store, &i, cx, |f| f.armed = false); } });
            }).detach();
        }
        "close" => finish(&store, id, "discard", None, cx),
        k => finish(&store, id, k, None, cx),
    }
}

/// Send to the Manager: an empty note only takes the keyboard back.
fn send(v: &mut LogView, id: &str, window: &mut Window, cx: &mut Context<LogView>) {
    let note = v.store.read(cx).log.fr.get(id).and_then(|f| f.note.as_ref().map(|n| n.read(cx).value().to_string())).unwrap_or_default();
    if note.trim().is_empty() { let n = note_state(v, id, window, cx); n.update(cx, |st, cx| st.focus(window, cx)); return; }
    finish(&v.store.clone(), id, "changes", Some(note), cx);
}

/// Post a choice: the buttons wait; refused: the error line (unless the card
/// says it already). Accepted: the server's patch redraws the card.
fn finish(store: &Entity<ChatStore>, id: &str, choice: &str, note: Option<String>, cx: &mut App) {
    let Some(e) = entry(store, id, cx) else { return };
    let body = fresult::finish_body(&s(&e, "runId"), choice, note.as_deref());
    let b2 = body.clone();
    store.update(cx, |st, _| st.log.fr_last = b2);
    with(store, id, cx, |f| { f.busy = true; f.armed = false; f.act_err.clear(); });
    let (store, i, choice) = (store.clone(), id.to_string(), choice.to_string());
    crundi_shell::call(cx, move |api| api.post("/api/factory-runs", &body), move |r, cx| {
        let err = choice_error(&r);
        let shown = entry(&store, &i, cx).map(|x| s(&x, "error")).unwrap_or_default();
        with(&store, &i, cx, |f| match err {
            Some(e) => { f.busy = false; if shown != e { f.act_err = e; } }
            None if choice == "changes" => { f.note_open = false; f.note = None; }
            None => {}
        });
    });
}

/// Show the full diff: fetched once; a network failure gives the button back.
fn diff(store: &Entity<ChatStore>, id: &str, cx: &mut App) {
    let Some(e) = entry(store, id, cx) else { return };
    if store.read(cx).log.fr.get(id).is_some_and(|f| f.diff_loading || f.diff.is_some()) { return; }
    with(store, id, cx, |f| f.diff_loading = true);
    let path = format!("/api/factory-runs/diff?id={}", enc(&s(&e, "runId")));
    let (store, i) = (store.clone(), id.to_string());
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
        with(&store, &i, cx, |f| {
            f.diff_loading = false;
            match r { Ok(x) => f.diff = Some(fresult::diff_lines(&x)), Err(e) if e.starts_with("Cannot reach the server") => {}, Err(e) => f.diff = Some(Err(e)) }
        });
    });
}

pub fn state(v: &LogView, cx: &App) -> Value {
    let st = v.store.read(cx);
    let cards: Vec<Value> = st.conv.entries.iter().filter(|e| s(e, "kind") == "factory-result").map(|r| {
        let id = s(r, "id");
        let none = Fr::default();
        let f = st.log.fr.get(&id).unwrap_or(&none);
        json!({ "id": id, "state": s(r, "state"), "title": fresult::title(r), "filesOpen": f.files_open, "fuOpen": f.fu_open, "noteOpen": f.note_open,
            "diffOn": f.diff.is_some(), "msg": fresult::message(r), "usage": fresult::usage(r), "busy": f.busy, "actErr": f.act_err,
            "buttons": fresult::buttons(r, f.armed).iter().map(|b| b.label.clone()).collect::<Vec<_>>() })
    }).collect();
    json!({ "cards": cards, "last": st.log.fr_last })
}

/// fr-toggle <id> files|fu, fr-press <id> <key>, fr-note <id> <text>, fr-send <id>, fr-diff <id>.
pub fn act(name: &str, args: &[String], v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let id = a(0);
    let store = v.store.clone();
    match name {
        "fr-toggle" => { let what = a(1); with(&store, &id, cx, |f| if what == "fu" { f.fu_open = !f.fu_open } else { f.files_open = !f.files_open }); }
        "fr-press" => press(v, &id, &a(1), window, cx),
        "fr-note" => { let t = a(1); let n = note_state(v, &id, window, cx); n.update(cx, |st, cx| st.set_value(t, window, cx)); }
        "fr-send" => send(v, &id, window, cx),
        "fr-diff" => diff(&store, &id, cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

// ─── Drawing ───

fn hl(color: Hsla) -> HighlightStyle { HighlightStyle { color: Some(color), ..Default::default() } }

fn spans(parts: &[(String, HighlightStyle)]) -> StyledText {
    let mut text = String::new();
    let mut runs = vec![];
    for (t, st) in parts { let a = text.len(); text.push_str(t); runs.push((a..text.len(), *st)); }
    StyledText::new(text).with_highlights(runs)
}

/// A factory's result card.
pub fn row(v: &mut LogView, r: &Row, fs: f32, window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    let raw = r.shown.raw.clone();
    let id = r.shown.id.clone();
    let store = v.store.clone();
    let st = store.read(cx);
    let none = Fr::default();
    let f = st.log.fr.get(&id).unwrap_or(&none);
    let (add, del, n) = fresult::counts(&raw);
    let (files, more) = fresult::files(&raw);
    let fus = fresult::followups(&raw);
    let branch = s(&raw, "branch");
    let msg = fresult::message(&raw);
    let usage = fresult::usage(&raw);
    let err = s(&raw, "error");
    let btns = fresult::buttons(&raw, f.armed);
    let (busy, files_open, fu_open, note_open) = (f.busy, f.files_open, f.fu_open, f.note_open && r.shown.status == "ask");
    let (diff_loading, dv, act_err, note) = (f.diff_loading, f.diff.clone(), f.act_err.clone(), f.note.clone());
    let keep = |d: Div| d.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    let mut card = div().flex().flex_col().rounded(T::R_CARD).border_1().border_color(T::accent()).bg(T::accent_dim()).p(T::SP_12)
        .child(div().text_size(T::TEXT_13 * fs).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(fresult::title(&raw)));
    let summary = s(&raw, "summary");
    if summary.trim().is_empty() { card = card.child(div().h(T::SP_4)); } else {
        let b = v.ui.md.blocks(&format!("fr-{id}"), &summary);
        card = card.child(div().flex().flex_col().pt(T::SP_6).pb(T::SP_8).children(super::md::draw(&b, fs, T::text())));
    }
    let mut parts = vec![(add, hl(T::green())), (" ".into(), hl(T::text_secondary())), (del, hl(T::red())), (format!(" {n}"), hl(T::text_secondary()))];
    if !branch.is_empty() { parts.push((format!(" on {branch}"), hl(T::text_muted()))); }
    let s2 = store.clone();
    let i2 = id.clone();
    card = card.child(w::disclosure_at(SharedString::from(format!("fr-files-{id}")), spans(&parts), files_open, fs).on_click(move |_, _, cx| with(&s2, &i2, cx, |f| f.files_open = !f.files_open)));
    if files_open {
        let mut list = div().flex().flex_col().gap(T::SP_2).pt(T::SP_6);
        for (name, a, d) in files {
            list = list.child(div().flex().gap(T::SP_10).font_family(T::MONO).text_size(T::TEXT_11 * fs)
                .child(div().flex_1().min_w_0().truncate().text_color(T::text()).child(name))
                .child(div().flex().gap(T::SP_4).child(div().text_color(T::green()).child(a)).child(div().text_color(T::red()).child(d))));
        }
        if !more.is_empty() { list = list.child(div().text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).child(more)); }
        if fresult::can_diff(&raw) && dv.is_none() {
            let (s3, i3) = (store.clone(), id.clone());
            list = list.child(keep(div().pt(T::SP_4)).child(w::disabled(w::button(SharedString::from(format!("fr-diff-{id}")), "Show the full diff"), diff_loading).w_full().on_click(move |_, _, cx| diff(&s3, &i3, cx))));
        }
        match &dv {
            Some(Err(x)) => { list = list.child(div().text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(x.clone())); }
            Some(Ok((lines, _))) => {
                let half = window.viewport_size().height / 2.;
                let mut d = div().id(SharedString::from(format!("fr-dv-{id}"))).mt(T::SP_6).max_h(half).overflow_scroll().font_family(T::MONO).text_size(T::TEXT_11 * fs).text_color(T::text());
                for l in lines { d = d.child(div().whitespace_nowrap().child(if l.is_empty() { " ".to_string() } else { l.clone() })); }
                list = list.child(d);
            }
            None => {}
        }
        card = card.child(list);
    }
    card = card.child(div().h(T::SP_8));
    if !msg.is_empty() { let b = v.ui.md.blocks(&format!("frm-{id}"), &msg); card = card.child(div().pb(T::SP_8).flex().flex_col().children(super::md::draw(&b, fs * 12. / 13., T::text()))); }
    if !usage.is_empty() { card = card.child(div().pt(T::SP_2).pb(T::SP_6).text_size(T::TEXT_11 * fs).text_color(T::text_secondary()).child(usage)); }
    if !fus.is_empty() {
        let (s4, i4) = (store.clone(), id.clone());
        card = card.child(w::disclosure_at(SharedString::from(format!("fr-fu-{id}")), fresult::followups_line(fus.len()), fu_open, fs).on_click(move |_, _, cx| with(&s4, &i4, cx, |f| f.fu_open = !f.fu_open)));
        if fu_open {
            let mut list = div().flex().flex_col().gap(T::SP_2).pt(T::SP_6).text_size(T::TEXT_11 * fs);
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
    if !err.is_empty() { card = card.child(div().pb(T::SP_8).text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(err)); }
    if !btns.is_empty() {
        let mut bw = keep(div().flex().flex_wrap().gap(T::SP_8));
        for b in btns {
            let (i5, key) = (id.clone(), b.key);
            let bid = SharedString::from(format!("fr-b-{id}-{}", b.key));
            let btn = if b.primary { w::primary(bid, b.label) } else if b.danger { w::danger(bid, b.label) } else { w::button(bid, b.label) };
            bw = bw.child(w::disabled(btn, busy).on_click(cx.listener(move |v, _, window, cx| press(v, &i5, key, window, cx))));
        }
        card = card.child(bw);
    }
    if note_open {
        if let Some(nt) = note {
            let (i6, i7) = (id.clone(), id.clone());
            card = card.child(keep(div().flex().flex_col().gap(T::SP_6).pt(T::SP_8))
                .child(div().capture_key_down(cx.listener(move |v, ev: &KeyDownEvent, window, cx| {
                    if ev.keystroke.key == "enter" && (ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform) { send(v, &i6, window, cx); cx.stop_propagation(); }
                })).child(w::textarea(&nt)))
                .child(w::disabled(w::primary(SharedString::from(format!("fr-send-{id}")), "Send to the Manager"), busy).w_full().on_click(cx.listener(move |v, _, window, cx| send(v, &i7, window, cx)))));
        }
    }
    if !act_err.is_empty() { card = card.child(div().pt(T::SP_8).text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(act_err)); }
    card.into_any_element()
}

// ─── A widget check's screenshots ───

fn format_of(b: &[u8]) -> ImageFormat {
    if b.starts_with(b"\x89PNG") { ImageFormat::Png } else if b.starts_with(&[0xff, 0xd8]) { ImageFormat::Jpeg } else if b.starts_with(b"GIF") { ImageFormat::Gif } else if b.len() > 12 && &b[8..12] == b"WEBP" { ImageFormat::Webp } else { ImageFormat::Png }
}

fn redraw(chat: &str, cx: &mut App) { if let Some(s) = crate::find(cx, chat) { s.update(cx, |_, cx| cx.notify()); } }

/// The screenshots list once per widget (fetched the first time its call is drawn).
fn fetch_shots(chat: &str, wid: &str, cx: &mut App) {
    if !cx.default_global::<Shots>().asked.insert(wid.to_string()) { return; }
    let project = crundi_shell::project(cx);
    let path = format!("/api/widgets/renders?project={}&id={}", enc(&project), enc(wid));
    let (chat, wid) = (chat.to_string(), wid.to_string());
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
        let Some(list) = r.ok().and_then(|v| shots_of(&v, &project, &wid, &chat)) else { return };
        for sh in &list { fetch_pic(&chat, &sh.key, cx); }
        cx.default_global::<Shots>().shots.insert(wid, list);
        // The row appears under its call: measured again.
        if let Some(s) = crate::find(cx, &chat) {
            s.update(cx, |st, cx| { let ids: Vec<String> = st.rows.iter().filter(|r| !r.shown.widget.is_empty()).map(|r| r.id.clone()).collect(); for i in ids { st.remeasure(&i); } cx.notify(); });
        }
    });
}

fn fetch_pic(chat: &str, key: &str, cx: &mut App) {
    if cx.default_global::<Shots>().pics.contains_key(key) { return; }
    let Some(path) = key.split_once("|@render:").map(|x| x.1.to_string()) else { return };
    let Some(api) = crundi_shell::api(cx) else { return };
    cx.default_global::<Shots>().pics.insert(key.to_string(), ("loading".into(), None));
    let (key, chat) = (key.to_string(), chat.to_string());
    crundi_shell::run(cx, move || api.get_bytes(&path), move |r, cx| {
        let p = &mut cx.default_global::<Shots>().pics;
        match r {
            Ok((200, b)) if !b.is_empty() => { let f = format_of(&b); p.insert(key, ("ready".into(), Some(Arc::new(Image::from_bytes(f, b))))); }
            Ok((200, _)) => { p.insert(key, ("empty".into(), None)); }
            Ok((404, _)) | Ok((410, _)) => { p.insert(key, ("gone".into(), None)); }
            _ => { p.remove(&key); }
        }
        redraw(&chat, cx);
    });
}

/// Under a done widget_render call: its screenshots, shot-height, captioned (None: none).
pub fn shots_row(v: &mut LogView, t: &Shown, cx: &mut Context<LogView>) -> Option<AnyElement> {
    if t.widget.is_empty() || t.status != "done" || t.error { return None; }
    let chat = v.chat(cx);
    fetch_shots(&chat, &t.widget, cx);
    let list = cx.default_global::<Shots>().shots.get(&t.widget).cloned().unwrap_or_default();
    if list.is_empty() { return None; }
    let mut row = div().id(SharedString::from(format!("shots-{}", t.id))).flex().gap(T::SP_8).pt(T::SP_8).overflow_x_scroll();
    for sh in list {
        let pic = cx.default_global::<Shots>().pics.get(&sh.key).cloned();
        let ratio = if sh.h > 0. { sh.w / sh.h } else { 1. };
        let mut b = div().flex_none().w(T::SHOT_H * ratio).h(T::SHOT_H).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg()).overflow_hidden().p(px(1.));
        if let Some((st, Some(im))) = pic { if st == "ready" { b = b.child(img(im).size_full().object_fit(ObjectFit::Contain)); } }
        row = row.child(div().flex().flex_col().gap(T::SP_4).child(b).child(div().font_family(T::MONO).text_size(T::TEXT_10).text_color(T::text_muted()).child(sh.caption.clone())));
    }
    Some(row.into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn shots_and_errors() {
        let v = json!({ "ok": true, "renders": { "at": 1, "shots": [{ "frame": "inline", "state": "live", "width": 600, "height": 300 }, { "frame": "mobile", "state": "dark", "width": 390, "height": 844 }] } });
        let l = shots_of(&v, "p q", "w-check", "c1").unwrap();
        assert_eq!(l[1].caption, "mobile · dark");
        assert_eq!(l[1].key, "c1|@render:/api/widgets/render-image?project=p%20q&id=w-check&name=mobile--dark&t=1");
        assert_eq!(choice_error(&Ok(json!({ "ok": false }))).unwrap(), "That did not work.");
        assert!(choice_error(&Ok(json!({ "ok": true }))).is_none());
    }
}
