//! The chat area (ARCHITECTURE.md; AREAS.md): every open chat's store
//! (`Chats`), the chat pane kind ("live:<chat id>") and its views (pane.rs),
//! the streaming path (the live hub's batch, grouped by chat, to each
//! store's `apply_batch`: one update per chat per frame), keeping the
//! workbench's panes in step with the project's running chats, and the
//! chat's driver actions.

pub mod pane;
pub mod store;

use crundi_core::live::Event;
use crundi_shell::{Area, Stores};
use crundi_workbench::{self as wb, HeadInfo, PaneKind, PaneParts};
use gpui::*;
use pane::ChatPane;
use serde_json::{json, Value};
use std::{collections::HashMap, rc::Rc};
use store::ChatStore;

/// Every open chat's store and pane view, by chat id. Lives in a Global as
/// a handle registry (read in handlers, never in render: views hold their
/// store's handle).
#[derive(Default)]
pub struct Chats {
    stores: HashMap<String, Entity<ChatStore>>,
    panes: HashMap<String, Entity<ChatPane>>,
}
impl Global for Chats {}

/// A chat's store, made (and subscribed on /ws) the first time.
pub fn store(cx: &mut App, id: &str) -> Entity<ChatStore> {
    if let Some(s) = cx.default_global::<Chats>().stores.get(id) { return s.clone(); }
    let s = cx.new(|cx| ChatStore::new(id, cx));
    cx.global_mut::<Chats>().stores.insert(id.to_string(), s.clone());
    crundi_shell::subscribe(cx, id);
    s
}

pub fn find(cx: &App, id: &str) -> Option<Entity<ChatStore>> { cx.try_global::<Chats>().and_then(|c| c.stores.get(id).cloned()) }

fn pane_view(cx: &mut App, id: &str, window: &mut Window) -> Entity<ChatPane> {
    if let Some(p) = cx.default_global::<Chats>().panes.get(id) { return p.clone(); }
    let s = store(cx, id);
    let p = cx.new(|cx| ChatPane::new(s, window, cx));
    cx.global_mut::<Chats>().panes.insert(id.to_string(), p.clone());
    p
}

fn drop_chat(cx: &mut App, id: &str) {
    let c = cx.default_global::<Chats>();
    c.panes.remove(id);
    if c.stores.remove(id).is_some() { crundi_shell::unsubscribe(cx, id); }
}

fn clear(cx: &mut App) {
    let ids: Vec<String> = cx.default_global::<Chats>().stores.keys().cloned().collect();
    for id in ids { drop_chat(cx, &id); }
}

/// The live hub's batch: each chat's messages to its store, in one update.
fn route(cx: &mut App, batch: &[Event]) {
    let mut by: Vec<(String, Vec<Value>)> = vec![];
    for e in batch {
        let Event::Chat(m) = e else { continue };
        let id = m.get("id").and_then(|i| i.as_str()).or_else(|| m.pointer("/session/id").and_then(|i| i.as_str())).unwrap_or("").to_string();
        match by.iter_mut().find(|(i, _)| *i == id) { Some((_, v)) => v.push(m.clone()), None => by.push((id, vec![m.clone()])) }
    }
    for (id, msgs) in by {
        if let Some(s) = find(cx, &id) { s.update(cx, |s, cx| s.apply_batch(&msgs, cx)); }
    }
}

/// The open project's running chats (the event stream's terminals of kind
/// "ui"; a factory's employees are not panes, its Manager is).
fn live_chats(stores: &Stores, cx: &App) -> Vec<Value> {
    let p = stores.projects.read(cx);
    p.terminals.iter().filter(|t| {
        let chat = match t.get("kind").and_then(|k| k.as_str()) {
            Some("ui") => true,
            Some("parked") => t.get("paneKind").and_then(|k| k.as_str()).unwrap_or("ui") == "ui",
            _ => false,
        };
        chat && t.get("project").and_then(|x| x.as_str()) == Some(p.open.as_str())
    }).cloned().collect()
}

/// Every running chat has a pane; panes of chats that are gone close.
pub fn sync(stores: &Stores, cx: &mut App) {
    let Some(w) = wb::workbench(cx) else { return };
    let ids: Vec<String> = live_chats(stores, cx).iter().filter_map(|c| c.get("id").and_then(|i| i.as_str()).map(String::from)).collect();
    w.update(cx, |w, cx| {
        let keep = ids.clone();
        w.retain(move |k| !k.starts_with("live:") || keep.iter().any(|i| k == format!("live:{i}")), cx);
        for id in &ids { w.place(&format!("live:{id}"), cx); }
    });
    for id in &ids { let _ = store(cx, id); }
    let gone: Vec<String> = cx.default_global::<Chats>().stores.keys().filter(|k| !ids.contains(k)).cloned().collect();
    for id in gone { drop_chat(cx, &id); }
}

/// The chat in the focused pane (else the first).
pub fn focused_chat(cx: &mut App) -> String {
    wb::workbench(cx).and_then(|w| w.update(cx, |w, _| w.focused_key())).and_then(|k| k.strip_prefix("live:").map(String::from)).unwrap_or_default()
}

/// The chat's head: its title and long title from the event stream's list
/// (a rename reaches every client there, live, as native-gpui's title_of);
/// its state exited from that list, else its own (the chat's events).
fn head_info(stores: &Stores, id: &str, meta: &store::Meta, cx: &App) -> HeadInfo {
    let p = stores.projects.read(cx);
    let listed = p.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(id));
    let field = |k: &str| listed.and_then(|t| t.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut title = field("title");
    if title.is_empty() { title = meta.title.clone(); }
    if title.is_empty() { title = "Chat".into(); }
    let state = match listed {
        Some(t) if crundi_core::head::chat_state(t) == "exited" => "exited".into(),
        _ => crundi_core::head::composer_state(&meta.state, meta.sending),
    };
    HeadInfo { title, long: field("longTitle"), state }
}

pub fn init(cx: &mut App) {
    wb::register_kind(cx, PaneKind {
        prefix: "live:",
        chat: true,
        make: Rc::new(|key, stores, window, cx| {
            let id = key.trim_start_matches("live:").to_string();
            let view = pane_view(cx, &id, window);
            let s = store(cx, &id);
            let meta = s.read(cx).meta.clone();
            let (m2, stores2, id2) = (meta.clone(), stores.clone(), id.clone());
            let projects = stores.projects.clone();
            PaneParts {
                body: view.into(),
                info: Rc::new(move |cx| {
                    let m = m2.read(cx).0.clone();
                    head_info(&stores2, &id2, &m, cx)
                }),
                // The head redraws on the chat's meta and on the projects
                // store (an exited chat), never on its streamed text.
                watch: Box::new(move |cx| vec![cx.observe(&meta, |_, _, cx| cx.notify()), cx.observe(&projects, |_, _, cx| cx.notify())]),
            }
        }),
        close: Some(Rc::new(|key, cx, done| {
            let id = key.trim_start_matches("live:").to_string();
            let path = format!("/api/ui-sessions/{id}/close");
            let Some(api) = crundi_shell::api(cx) else { return done(false, cx) };
            crundi_shell::run(cx, move || api.post(&path, &json!({})), move |r, cx| match crundi_core::head::close_outcome(r) {
                crundi_core::head::Closed::Yes => { drop_chat(cx, &id); done(true, cx) }
                crundi_core::head::Closed::SignedOut => { let s = crundi_shell::stores(cx); s.session.update(cx, |s, cx| s.sign_out("Your sign-in ended. Sign in again.", cx)); }
                crundi_core::head::Closed::No(why) => { log::warn!("close refused: {why}"); done(false, cx) }
            });
        })),
        zoom: Some(Rc::new(|key, d, cx| { if let Some(s) = find(cx, key.trim_start_matches("live:")) { s.update(cx, |s, cx| s.zoom(d, cx)); } })),
    });
    crundi_shell::register(cx, Area::new("chat")
        .on_batch(|stores, batch, cx| {
            route(cx, batch);
            if batch.iter().any(|e| matches!(e, Event::State(_))) { sync(stores, cx); }
        })
        .on_project(|stores, _, cx| { clear(cx); sync(stores, cx); })
        .action("workbench-ready", |stores, _, cx| sync(stores, cx))
        .on_signed_out(|_, cx| clear(cx))
        .test_ui(|stores, name, args, window, cx| match name {
            // A message to the focused chat, as typed in its box and sent.
            "send" => {
                let id = focused_chat(cx);
                if id.is_empty() { return Some(json!({ "ok": false, "error": "no chat" })); }
                let p = pane_view(cx, &id, window);
                let comp = p.read(cx).composer.clone();
                let text = args.first().cloned().unwrap_or_default();
                comp.update(cx, |c, cx| { c.set_text(&text, window, cx); c.send(window, cx); });
                Some(json!({ "ok": true }))
            }
            "stop" => {
                let id = focused_chat(cx);
                if let Some(s) = find(cx, &id) { s.update(cx, |s, cx| s.stop(cx)); }
                Some(json!({ "ok": true }))
            }
            // A new chat in the project; its pane is placed and focused.
            "new-chat" => {
                let project = stores.projects.read(cx).open.clone();
                let s = stores.clone();
                crundi_shell::call(cx, move |api| api.post("/api/ui-sessions/create", &json!({ "project": project })), move |r, cx| {
                    if let Some(id) = r.ok().and_then(|v| v.get("id").and_then(|i| i.as_str()).map(String::from)) {
                        let _ = store(cx, &id);
                        if let Some(w) = wb::workbench(cx) { w.update(cx, |w, cx| { let k = format!("live:{id}"); w.place(&k, cx); w.focus_key(&k, cx); }); }
                    }
                    sync(&s, cx);
                });
                Some(json!({ "ok": true }))
            }
            // Scroll the focused chat's log by dy px (the frame harness).
            "log-scroll" => {
                let id = focused_chat(cx);
                let dy: f32 = args.first().and_then(|a| a.parse().ok()).unwrap_or(0.);
                if id.is_empty() { return Some(json!({ "ok": false, "error": "no chat" })); }
                // The pane's log draws the list: it alone is notified (a
                // wheel does the same through the list's own handler).
                let log = pane_view(cx, &id, window).read(cx).log.clone();
                let l = store(cx, &id).read(cx).list.clone();
                l.scroll_by(px(dy));
                log.update(cx, |_, cx| cx.notify());
                Some(json!({ "ok": true }))
            }
            _ => None,
        })
        .state(|stores, _, cx| {
            let id = focused_chat(cx);
            let (entries, st, batches, spliced) = match find(cx, &id) {
                Some(s) => {
                    let s = s.read(cx);
                    let e: Vec<Value> = s.rows.iter().map(|r| json!({ "id": r.id, "kind": r.shown.kind, "text": r.shown.text, "title": r.shown.title, "status": r.shown.status, "error": r.shown.error })).collect();
                    (e, s.conv.state.clone(), s.batches, s.spliced)
                }
                None => (vec![], String::new(), 0, 0),
            };
            let chats: Vec<Value> = live_chats(stores, cx).iter().map(|c| json!({ "id": c.get("id"), "title": c.get("title"), "state": c.get("state") })).collect();
            // The focused chat's head as drawn, and whether its log follows the end.
            let (head, following) = match find(cx, &id) {
                Some(s) => { let s = s.read(cx); let h = head_info(stores, &id, &s.meta.read(cx).0, cx); (json!({ "title": h.title, "long": h.long, "state": h.state }), s.list.is_following_tail()) }
                None => (Value::Null, false),
            };
            json!({ "chat": id, "entries": entries, "chatState": st, "chats": chats, "batches": batches, "spliced": spliced, "head": head, "logFollowing": following })
        }));
}
