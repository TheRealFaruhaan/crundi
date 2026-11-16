//! 2.8: drafts (specs/attachments-drafts.md, the web's saveDraft /
//! readDraft / pruneDrafts; crundi_core::drafts has the rules). Every change
//! of a box's text is its chat's draft, and the project's copy by the claim
//! rules; they live in the client's settings per server (Saved.drafts),
//! written at most every 350 ms. A composer drawn takes its draft back
//! (its own, else the project's unclaimed copy, with any held lines an
//! earlier run left in front); a chat pane that goes lets its claim go, so
//! a new chat in the project adopts it. Drafts older than 30 days are pruned
//! when loaded.

use crate::ComposerView;
use crundi_core::{chat as cchat, drafts::Drafts, store};
use gpui::*;
use serde_json::{json, Value};
use std::collections::HashSet;

/// One composer's: whether it has taken its draft back since it was drawn.
#[derive(Default)]
pub struct State { pub restored: bool }

/// The drafts of the server signed in to.
#[derive(Default)]
pub struct Store {
    server: String,
    d: Drafts,
    timer: bool,
    dirty: bool,
}
impl Global for Store {}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn server(cx: &App) -> String { crundi_shell::api(cx).map(|a| a.base().to_string()).unwrap_or_default() }

/// The store, loaded (and pruned) for the server signed in to.
fn store(cx: &mut App) -> &mut Store {
    let srv = server(cx);
    if !cx.has_global::<Store>() { cx.set_global(Store::default()); }
    let s = cx.global_mut::<Store>();
    if s.server != srv {
        s.server = srv.clone();
        s.d = Drafts::from_saved(store::load().drafts.get(&srv).unwrap_or(&Value::Null));
        s.d.prune(now_ms());
    }
    s
}

/// A chat's project (the event stream's list, else the open project).
fn alias_of(cx: &App, chat: &str) -> String {
    let s = crundi_shell::session::get(cx);
    s.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(chat))
        .and_then(|t| t.get("project").and_then(|p| p.as_str()).map(String::from))
        .unwrap_or_else(|| s.project.clone())
}

/// The box's text changed (typed, sent, a choice, a give-back).
pub fn changed(c: &ComposerView, text: &str, cx: &mut App) {
    if crundi_shell::api(cx).is_none() || !c.drafts.restored { return; }
    let chat = c.chat_id(cx);
    let alias = alias_of(cx, &chat);
    let s = store(cx);
    s.d.save(&chat, &alias, text, now_ms());
    s.dirty = true;
    write_soon(cx);
}

/// A chat's held lines (2.9), kept with the drafts so they outlive the app.
pub fn set_held(cx: &mut App, chat: &str, text: &str) {
    if crundi_shell::api(cx).is_none() { return; }
    let s = store(cx);
    s.d.set_held(chat, text, now_ms());
    s.dirty = true;
    write_soon(cx);
}

/// Drawn: the composer takes its draft back once (the box empty only).
pub fn restore_if_needed(c: &mut ComposerView, window: &mut Window, cx: &mut Context<ComposerView>) {
    if c.drafts.restored || crundi_shell::api(cx).is_none() { return; }
    c.drafts.restored = true;
    let chat = c.chat_id(cx);
    let alias = alias_of(cx, &chat);
    let held_here = !c.queue.held.is_empty();
    let now = now_ms();
    let found = {
        let s = store(cx);
        let mut found = s.d.restore(&chat, &alias, now);
        if !held_here {
            if let Some(h) = s.d.take_held(&chat) {
                let t = cchat::given_back(&h, found.as_deref().unwrap_or(""));
                s.d.save(&chat, &alias, &t, now);
                found = Some(t);
            }
        }
        s.dirty = true;
        found
    };
    write_soon(cx);
    if let Some(t) = found { if c.text(cx).is_empty() { c.set_text(&t, window, cx); } }
}

/// The chat panes on screen now: a composer whose pane went lets its claim
/// go (and takes its draft back when it is drawn again).
pub fn sync(cx: &mut App) {
    let on: HashSet<String> = crundi_workbench::workbench(cx).map(|w| w.update(cx, |w, cx| w.panes(cx))).unwrap_or_default().iter()
        .filter_map(|p| p.get("key").and_then(|k| k.as_str()).and_then(|k| k.strip_prefix("live:")).map(String::from)).collect();
    let all: Vec<(String, Entity<ComposerView>)> = cx.try_global::<crate::Composers>().map(|c| c.0.iter().filter_map(|(k, w)| w.upgrade().map(|e| (k.clone(), e))).collect()).unwrap_or_default();
    for (chat, e) in all {
        if on.contains(&chat) || !e.read(cx).drafts.restored { continue; }
        e.update(cx, |c, _| c.drafts.restored = false);
        let alias = alias_of(cx, &chat);
        store(cx).d.release(&chat, &alias);
    }
}

fn write_soon(cx: &mut App) {
    let s = store(cx);
    if std::mem::replace(&mut s.timer, true) { return; }
    crate::after(cx, 350, |cx| { store(cx).timer = false; flush(cx); });
}

/// The drafts to disk now (also at quit).
pub fn flush(cx: &mut App) {
    if !cx.has_global::<Store>() { return; }
    let s = cx.global_mut::<Store>();
    if !s.dirty || s.server.is_empty() { return; }
    s.dirty = false;
    let (srv, v) = (s.server.clone(), s.d.to_saved());
    store::update(|x| { x.drafts.insert(srv, v); });
}

pub fn render(_c: &mut ComposerView, _window: &mut Window, _cx: &mut Context<ComposerView>) -> Option<AnyElement> { None }

pub fn test_ui(_c: &mut ComposerView, _name: &str, _args: &[String], _window: &mut Window, _cx: &mut Context<ComposerView>) -> Option<Value> { None }

/// (The box's text, "draft", is the composer's own field.)
pub fn pane_props(_c: &ComposerView, _cx: &App) -> Value { json!({}) }

/// savedDrafts: this server's saved drafts, as written.
pub fn props(_c: &ComposerView, cx: &App) -> Value {
    let v = cx.try_global::<Store>().map(|s| s.d.to_saved()).unwrap_or_else(|| json!({}));
    json!({ "savedDrafts": v })
}
