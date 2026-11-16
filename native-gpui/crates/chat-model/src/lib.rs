//! One chat as a GPUI entity (Lead's, AREAS.md): the conversation
//! (crundi_core::chat::Conversation, the Slint client's model, unchanged)
//! with what the window adds (connected, the last error, the text size).
//! The chat's log (crundi-chat-log, Operations) and composer
//! (crundi-chat-composer, Planning) both read it and change it only through
//! these functions; the live link's messages reach it through `route`.

use crundi_core::chat::Conversation;
use gpui::*;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct ChatModel {
    pub conv: Conversation,
    pub connected: bool,
    /// The last error to show (a refused send…), "" for none.
    pub error: String,
    /// The pane's text size step (A- / A+).
    pub zoom: i32,
}

/// What a chat model tells the views (they re-render on notify anyway).
pub enum ChatEvent { Changed, NewEntry }
impl EventEmitter<ChatEvent> for ChatModel {}

impl ChatModel {
    pub fn new(id: &str) -> Self {
        let mut conv = Conversation::default();
        conv.id = id.to_string();
        ChatModel { conv, connected: true, error: String::new(), zoom: 0 }
    }

    pub fn id(&self) -> &str { &self.conv.id }

    /// A ui-history or ui-event message for this chat.
    pub fn apply(&mut self, msg: &Value, cx: &mut Context<Self>) {
        let before = self.conv.entries.len();
        let changed = match msg.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "ui-history" => { if let Some(s) = msg.get("session") { self.conv.load(s); true } else { false } }
            "ui-event" => msg.get("event").is_some_and(|e| self.conv.apply(e)),
            _ => false,
        };
        if changed {
            cx.emit(if self.conv.entries.len() > before { ChatEvent::NewEntry } else { ChatEvent::Changed });
            cx.notify();
        }
    }

    /// Send a message (POST /api/ui-sessions/:id/send); a refusal is shown as the error.
    pub fn send(&mut self, text: String, cx: &mut Context<Self>) {
        if text.trim().is_empty() { return; }
        self.error.clear();
        let path = format!("/api/ui-sessions/{}/send", self.conv.id);
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.post(&path, &json!({ "text": text })), move |r, cx| {
            let _ = me.update(cx, |m, cx| {
                match r {
                    Err(e) => m.error = e,
                    Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => m.error = v.get("error").and_then(|e| e.as_str()).unwrap_or("Not sent").to_string(),
                    Ok(_) => {}
                }
                cx.notify();
            });
        });
        cx.notify();
    }

    /// Stop the running turn (POST /api/ui-sessions/:id/stop).
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        let path = format!("/api/ui-sessions/{}/stop", self.conv.id);
        crundi_shell::call(cx, move |api| api.post(&path, &json!({})), |_, _| {});
    }
}

/// Every open chat model, by chat id.
#[derive(Default)]
pub struct Chats(pub HashMap<String, Entity<ChatModel>>);
impl Global for Chats {}

/// The model of a chat, made (and subscribed on the live link) the first time.
pub fn model(cx: &mut App, id: &str) -> Entity<ChatModel> {
    if !cx.has_global::<Chats>() { cx.set_global(Chats::default()); }
    if let Some(m) = cx.global::<Chats>().0.get(id) { return m.clone(); }
    let m = cx.new(|_| ChatModel::new(id));
    cx.global_mut::<Chats>().0.insert(id.to_string(), m.clone());
    crundi_shell::subscribe(cx, id);
    m
}

/// The model of a chat, if it is open.
pub fn find(cx: &App, id: &str) -> Option<Entity<ChatModel>> {
    cx.try_global::<Chats>().and_then(|c| c.0.get(id).cloned())
}

/// Forget a chat (its pane closed): unsubscribe.
pub fn drop_chat(cx: &mut App, id: &str) {
    if cx.has_global::<Chats>() && cx.global_mut::<Chats>().0.remove(id).is_some() { crundi_shell::unsubscribe(cx, id); }
}

/// Forget every chat (signed out, another project).
pub fn clear(cx: &mut App) {
    let ids: Vec<String> = cx.try_global::<Chats>().map(|c| c.0.keys().cloned().collect()).unwrap_or_default();
    for id in ids { drop_chat(cx, &id); }
}

/// A live-link message for a chat: to its model, by the message's id.
pub fn route(cx: &mut App, msg: &Value) {
    let id = msg.get("id").and_then(|i| i.as_str()).or_else(|| msg.pointer("/session/id").and_then(|i| i.as_str())).unwrap_or("").to_string();
    if let Some(m) = find(cx, &id) { m.update(cx, |m, cx| m.apply(msg, cx)); }
}

/// Every open chat's connection follows the live link.
pub fn set_connected(cx: &mut App, on: bool) {
    let ms: Vec<Entity<ChatModel>> = cx.try_global::<Chats>().map(|c| c.0.values().cloned().collect()).unwrap_or_default();
    for m in ms { m.update(cx, |m, cx| { m.connected = on; cx.notify(); }); }
}

/// A side question from the composer's box (`/btw <question>`): the chat's
/// log asks it on the side (2.21). The log registers the handler; the
/// composer calls `aside` first on send (true: handled, send nothing).
#[derive(Default)]
pub struct Aside(pub Option<std::rc::Rc<dyn Fn(&mut App, &str, &str) -> bool>>);
impl Global for Aside {}

pub fn register_aside(cx: &mut App, f: impl Fn(&mut App, &str, &str) -> bool + 'static) {
    cx.set_global(Aside(Some(std::rc::Rc::new(f))));
}

pub fn aside(cx: &mut App, chat: &str, text: &str) -> bool {
    let Some(f) = cx.try_global::<Aside>().and_then(|a| a.0.clone()) else { return false };
    f(cx, chat, text)
}
