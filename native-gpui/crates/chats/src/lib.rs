//! A chat pane (Lead's glue, AREAS.md): the chat's log (crundi-chat-log,
//! Operations) over its composer (crundi-chat-composer, Planning), in a
//! workbench pane whose key is "live:<chat id>". The project's running
//! chats each get a pane (new ones to the right), as the Slint client's
//! sync_panes; the live link's chat messages reach their model.
//!
//! The pane header (2.1-2.2) and the rest of the workbench are Workspace's.

use crundi_chat_composer::ComposerView;
use crundi_chat_log::LogView;
use crundi_chat_model::{self as model, ChatModel};
use crundi_shell::{Area, AreaEvent};
use crundi_theme::T;
use crundi_workbench::{self as wb, PaneKind};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{collections::HashMap, rc::Rc};

pub struct ChatPane {
    pub model: Entity<ChatModel>,
    pub log: Entity<LogView>,
    pub composer: Entity<ComposerView>,
}

impl Render for ChatPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A parked chat: the log draws its card over what it was; the composer
        // only while peeking (a send then resumes it first).
        let id = self.model.read(cx).id().to_string();
        let card = crundi_chat_log::parked::is_parked(cx, &id) && !crundi_chat_log::parked::peeking(cx, &id);
        // Files dragged from the system onto any part of the chat attach to it
        // (the dashed outline while over it), not only onto its box (the
        // owner, 18 Nov: dropping files didn't work).
        let chat = id.clone();
        let pane = w::ref_target(div().id(SharedString::from(format!("chat-pane-{id}"))), move |text, _, cx| crundi_chat_composer::attach::insert_text(cx, &chat, text));
        crundi_chat_composer::attach::drop_target(pane, &id)
            .flex().flex_col().size_full().min_h_0().bg(T::bg()).child(self.log.clone()).when(!card, |d| d.child(self.composer.clone()))
    }
}

/// Every chat pane's view, by chat id (made the first time the pane is drawn).
#[derive(Default)]
pub struct Panes(pub HashMap<String, Entity<ChatPane>>);
impl Global for Panes {}

fn pane(id: &str, window: &mut Window, cx: &mut App) -> Entity<ChatPane> {
    if !cx.has_global::<Panes>() { cx.set_global(Panes::default()); }
    if let Some(p) = cx.global::<Panes>().0.get(id) { return p.clone(); }
    let m = model::model(cx, id);
    let log = cx.new(|cx| LogView::new(m.clone(), cx));
    let composer = cx.new(|cx| ComposerView::new(m.clone(), window, cx));
    let p = cx.new(|_| ChatPane { model: m, log, composer });
    cx.global_mut::<Panes>().0.insert(id.to_string(), p.clone());
    p
}

/// The project's running chats (the event stream's terminals of kind "ui").
fn live_chats(cx: &App) -> Vec<Value> {
    let s = crundi_shell::session::get(cx);
    // Running chats, and parked ones (2.23: they keep their pane, showing the card).
    let chat = |t: &Value| match t.get("kind").and_then(|k| k.as_str()) {
        Some("ui") => true,
        Some("parked") => t.get("paneKind").and_then(|k| k.as_str()).unwrap_or("ui") == "ui",
        _ => false,
    };
    let mut out: Vec<Value> = s.terminals.iter().filter(|t| chat(t) && t.get("project").and_then(|p| p.as_str()) == Some(s.project.as_str())).cloned().collect();
    // The safety net (F75/F84): the project's chats from GET /api/ui-sessions
    // that the event stream's state doesn't list (yet): a state event missed
    // late in a long session must not leave a new chat without its pane.
    // Only chats the state has never listed: one it listed is the state's
    // (closed since, it must not come back from a list 10 s old).
    if let Some(p) = cx.try_global::<Polled>() {
        let id = |v: &Value| v.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
        let seen = cx.try_global::<Seen>();
        for c in &p.0 {
            let i = id(c);
            if c.get("project").and_then(|x| x.as_str()) == Some(s.project.as_str()) && !out.iter().any(|o| id(o) == i) && !seen.is_some_and(|x| x.0.contains(&i)) { out.push(c.clone()); }
        }
    }
    out
}

/// Every chat id the state's terminals have listed (this sign-in).
#[derive(Default)]
struct Seen(std::collections::HashSet<String>);
impl Global for Seen {}

fn note_seen(cx: &mut App) {
    let ids: Vec<String> = crundi_shell::session::get(cx).terminals.iter().filter_map(|t| t.get("id").and_then(|i| i.as_str()).map(String::from)).collect();
    if !cx.has_global::<Seen>() { cx.set_global(Seen::default()); }
    cx.global_mut::<Seen>().0.extend(ids);
}

/// The last GET /api/ui-sessions (the safety net above).
#[derive(Default)]
struct Polled(Vec<Value>);
impl Global for Polled {}

/// Poll the chat list every 10 s while signed in: one poller for the
/// app's life, started at the first sign-in. It re-arms whether or not a
/// session is up yet, so an early tick can't end it (F84).
fn poll(cx: &mut App) {
    if cx.has_global::<Polling>() { return; }
    cx.set_global(Polling);
    tick(cx);
}
struct Polling;
impl Global for Polling {}

fn tick(cx: &mut App) {
    fn again(cx: &mut App) {
        let t = cx.background_executor().timer(std::time::Duration::from_secs(10));
        cx.spawn(async move |cx: &mut AsyncApp| { t.await; let _ = cx.update(tick); }).detach();
    }
    if !crundi_shell::signed_in(cx) { return again(cx); }
    crundi_shell::call(cx, |api| api.get("/api/ui-sessions"), |r, cx| {
        if let Ok(v) = r {
            let list = v.get("sessions").and_then(|s| s.as_array()).cloned().or_else(|| v.as_array().cloned()).unwrap_or_default();
            // Only chats that are running (or parked): an ended one is the state's to show.
            // As the server's state: a factory's employees are not panes (its Manager is).
            let live: Vec<Value> = list.into_iter().filter(|c| {
                let employee = c.get("factory").is_some_and(|f| !f.is_null() && f.get("role").and_then(|r| r.as_str()) != Some("manager"));
                !employee && !c.get("background").and_then(|b| b.as_bool()).unwrap_or(false)
                    && !matches!(c.get("status").and_then(|x| x.as_str()), Some("exited" | "ended" | "closed"))
            }).collect();
            cx.set_global(Polled(live));
            sync(cx);
        }
        again(cx);
    });
}

/// Every running chat has a pane; panes of chats that are gone close.
pub fn sync(cx: &mut App) {
    let Some(w) = wb::workbench(cx) else { return };
    let ids: Vec<String> = live_chats(cx).iter().filter_map(|c| c.get("id").and_then(|i| i.as_str()).map(String::from)).collect();
    w.update(cx, |w, cx| {
        let keep = ids.clone();
        w.retain(move |k| !k.starts_with("live:") || keep.iter().any(|i| k == format!("live:{i}")), cx);
        for id in &ids { let key = format!("live:{id}"); if w.mosaic().leaf_of(&key).is_none() { w.place(&key, cx); } }
    });
    for id in &ids { let _ = model::model(cx, id); }
    // A chat that was parked and runs again (resumed here, from another
    // device or the web) needs subscribing again: the server ignored the
    // subscription made while it was parked.
    let parked: std::collections::HashSet<String> = ids.iter().filter(|i| crundi_chat_log::parked::is_parked(cx, i)).cloned().collect();
    let before = cx.try_global::<WasParked>().map(|w| w.0.clone()).unwrap_or_default();
    for id in ids.iter().filter(|i| before.contains(*i) && !parked.contains(*i)) { crundi_shell::subscribe(cx, id); }
    cx.set_global(WasParked(parked));
}

/// The chats that were parked at the last sync.
#[derive(Default)]
struct WasParked(std::collections::HashSet<String>);
impl Global for WasParked {}

/// The chat in the focused pane (else the first), for the driver's "send".
pub fn focused_chat(cx: &mut App) -> String {
    let panes = wb::workbench(cx).map(|w| w.update(cx, |w, cx| w.panes(cx))).unwrap_or_default();
    let chat = |p: &Value| p.get("key").and_then(|k| k.as_str()).and_then(|k| k.strip_prefix("live:")).map(String::from);
    panes.iter().find(|p| p.get("focused") == Some(&json!(true))).and_then(chat).or_else(|| panes.iter().find_map(chat)).unwrap_or_default()
}

pub fn init(cx: &mut App) {
    wb::register_kind(cx, PaneKind {
        prefix: "live:",
        render: Rc::new(|key, window, cx| pane(key.trim_start_matches("live:"), window, cx).into_any_element()),
        title: Rc::new(|key, cx| {
            let id = key.trim_start_matches("live:");
            match model::find(cx, id) {
                Some(m) => {
                    let parked = crundi_chat_log::parked::is_parked(cx, id);
                    let m = m.read(cx);
                    (if m.conv.title.is_empty() { "Chat".into() } else { m.conv.title.clone() }, if parked { "exited".into() } else if m.conv.state.is_empty() { "idle".into() } else { m.conv.state.clone() })
                }
                None => ("Chat".into(), "idle".into()),
            }
        }),
    });
    crundi_shell::register(cx, Area::new("chats")
        .on_event(|e, cx| match e {
            AreaEvent::Chat(v) => model::route(cx, v),
            AreaEvent::Connected(on) => model::set_connected(cx, *on),
            AreaEvent::State(_) => { note_seen(cx); sync(cx) }
            _ => {}
        })
        .on_project(|_, cx| { model::clear(cx); sync(cx); })
        .on_signed_in(poll)
        .on_signed_out(|cx| { cx.set_global(Polled::default()); cx.set_global(Seen::default()); model::clear(cx) })
        .test_ui(|name, args, window, cx| match name {
            // A message to the focused chat, as typed in its box and sent.
            "send" => {
                let id = focused_chat(cx);
                if id.is_empty() { return Some(json!({ "ok": false, "error": "no chat" })); }
                let p = pane(&id, window, cx);
                let comp = p.read(cx).composer.clone();
                let text = args.first().cloned().unwrap_or_default();
                // A parked chat (peeking) resumes first, then sends (2.23).
                if crundi_chat_log::parked::is_parked(cx, &id) {
                    let win = window.window_handle();
                    crundi_chat_log::parked::resume_first(cx, &id, move |cx| {
                        let _ = win.update(cx, |_, window, cx| comp.update(cx, |c, cx| { c.set_text(&text, window, cx); c.send(window, cx); }));
                    });
                    return Some(json!({ "ok": true }));
                }
                comp.update(cx, |c, cx| { c.set_text(&text, window, cx); c.send(window, cx); });
                Some(json!({ "ok": true }))
            }
            // An approval's answer (the Slint client's app::respond): allow / deny, always.
            "respond" => {
                let id = focused_chat(cx);
                let mut body = json!({ "requestId": args.first().cloned().unwrap_or_default(), "behavior": args.get(1).cloned().unwrap_or_default() });
                if args.get(2).is_some_and(|a| a == "true") { body["always"] = json!(true); }
                let me = model::find(cx, &id);
                crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{id}/respond"), &body), move |r, cx| {
                    if let (Err(e), Some(m)) = (r, me) { m.update(cx, |m, cx| { m.error = e; cx.notify(); }); }
                });
                Some(json!({ "ok": true }))
            }
            // Stop, as its button: the composer's arming Stop (one click "Sure?"
            // for 3 s, a second interrupts; 2.5). A chat whose composer isn't
            // drawn yet stops at once (planning, F29).
            "stop" => {
                let id = focused_chat(cx);
                match crundi_chat_composer::composer(cx, &id) {
                    Some(c) => c.update(cx, |c, cx| c.stop(cx)),
                    None => if let Some(m) = model::find(cx, &id) { m.update(cx, |m, cx| m.stop(cx)); },
                }
                Some(json!({ "ok": true }))
            }
            // A new chat in the project (the launcher's UI mode comes with 2.3).
            "new-chat" => {
                let project = crundi_shell::project(cx);
                crundi_shell::call(cx, move |api| api.post("/api/ui-sessions/create", &json!({ "project": project })), |r, cx| {
                    // The new chat's pane, focused (the event stream's state follows).
                    if let Some(id) = r.ok().and_then(|v| v.get("id").and_then(|i| i.as_str()).map(String::from)) {
                        let _ = model::model(cx, &id);
                        if let Some(w) = wb::workbench(cx) { w.update(cx, |w, cx| { w.place(&format!("live:{id}"), cx); }); }
                    }
                    sync(cx);
                });
                Some(json!({ "ok": true }))
            }
            _ => None,
        })
        .state(|_, cx| {
            // The focused chat's rows as the log draws them (crundi_core::chat), and the project's chats.
            let id = focused_chat(cx);
            let entries: Vec<Value> = model::find(cx, &id).map(|m| m.read(cx).conv.view().into_iter().map(|e| json!({
                "id": e.id, "kind": e.kind, "text": e.text, "title": e.title, "detail": e.detail, "status": e.status, "error": e.error, "requestId": e.request_id,
            })).collect()).unwrap_or_default();
            let st = model::find(cx, &id).map(|m| m.read(cx).conv.state.clone()).unwrap_or_default();
            let chats: Vec<Value> = live_chats(cx).iter().map(|c| json!({ "id": c.get("id"), "title": c.get("title"), "state": c.get("state") })).collect();
            json!({ "chat": id, "entries": entries, "chatState": st, "chats": chats })
        }));
}
