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
use gpui::*;
use serde_json::{json, Value};
use std::{collections::HashMap, rc::Rc};

pub struct ChatPane {
    pub model: Entity<ChatModel>,
    pub log: Entity<LogView>,
    pub composer: Entity<ComposerView>,
}

impl Render for ChatPane {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().size_full().min_h_0().bg(T::bg()).child(self.log.clone()).child(self.composer.clone())
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
    s.terminals.iter().filter(|t| t.get("kind").and_then(|k| k.as_str()) == Some("ui") && t.get("project").and_then(|p| p.as_str()) == Some(s.project.as_str())).cloned().collect()
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
}

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
                Some(m) => { let m = m.read(cx); (if m.conv.title.is_empty() { "Chat".into() } else { m.conv.title.clone() }, if m.conv.state.is_empty() { "idle".into() } else { m.conv.state.clone() }) }
                None => ("Chat".into(), "idle".into()),
            }
        }),
    });
    crundi_shell::register(cx, Area::new("chats")
        .on_event(|e, cx| match e {
            AreaEvent::Chat(v) => model::route(cx, v),
            AreaEvent::Connected(on) => model::set_connected(cx, *on),
            AreaEvent::State(_) => sync(cx),
            _ => {}
        })
        .on_project(|_, cx| { model::clear(cx); sync(cx); })
        .on_signed_out(|cx| model::clear(cx))
        .test_ui(|name, args, window, cx| match name {
            // A message to the focused chat, as typed in its box and sent.
            "send" => {
                let id = focused_chat(cx);
                if id.is_empty() { return Some(json!({ "ok": false, "error": "no chat" })); }
                let p = pane(&id, window, cx);
                let comp = p.read(cx).composer.clone();
                let text = args.first().cloned().unwrap_or_default();
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
