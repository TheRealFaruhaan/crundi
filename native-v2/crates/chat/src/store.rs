//! One chat's store (ARCHITECTURE.md, "How server data becomes view
//! state"): the conversation (core::chat::Conversation, the server's
//! entries), its rows as the log draws them, each assistant text's
//! TextViewState (parsed on the background executor, streamed deltas
//! appended), and the log's ListState, spliced only for the rows a batch
//! touched. The small part the head and the composer show (title, state,
//! model, cost, the last error, the text size) is a separate entity,
//! `ChatMeta`, notified only when it changes: a streamed delta redraws the
//! log alone.

use crundi_core::chat::{Conversation, Shown};
use crundi_theme::T;
use gpui::*;
use gpui_component::text::TextViewState;
use serde_json::Value;
use std::collections::HashMap;

/// One row of the log, ready to lay out (render never parses).
pub struct Row {
    pub id: String,
    pub shown: Shown,
    /// Claude's text (and thinking), parsed off the UI thread.
    pub md: Option<Entity<TextViewState>>,
    /// The user's bubble and other plain text, shared (no copy per frame).
    pub text: SharedString,
}

#[derive(Clone, Default, PartialEq)]
pub struct Meta {
    pub title: String,
    /// idle | working | waiting | needs-input (exited comes from the projects store).
    pub state: String,
    pub model: String,
    pub cost: f64,
    pub session_id: String,
    pub error: String,
    /// The pane's text size step (A- / A+).
    pub zoom: i32,
    pub sending: bool,
}

pub struct ChatMeta(pub Meta);

pub struct ChatStore {
    pub id: String,
    pub conv: Conversation,
    pub rows: Vec<Row>,
    /// The row of each entry id.
    row_of: HashMap<String, usize>,
    pub list: ListState,
    pub meta: Entity<ChatMeta>,
    /// Batches applied (the driver's count) and the last one's row splices.
    pub batches: u64,
    pub spliced: u64,
}

impl ChatStore {
    pub fn new(id: &str, cx: &mut Context<Self>) -> Self {
        let mut conv = Conversation::default();
        conv.id = id.to_string();
        let list = ListState::new(0, ListAlignment::Top, T::LOG_OVERDRAW);
        list.set_follow_mode(FollowMode::Tail);
        let meta = cx.new(|_| ChatMeta(Meta::default()));
        ChatStore { id: id.to_string(), conv, rows: vec![], row_of: HashMap::new(), list, meta, batches: 0, spliced: 0 }
    }

    /// This frame's messages for this chat (ui-history, ui-event), applied in
    /// one go: rows re-derived only for the entries that changed, one notify.
    pub fn apply_batch(&mut self, msgs: &[Value], cx: &mut Context<Self>) {
        let mut any = false;
        for m in msgs {
            any |= match m.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "ui-history" => { if let Some(s) = m.get("session") { self.conv.load(s); true } else { false } }
                "ui-event" => m.get("event").is_some_and(|e| self.conv.apply(e)),
                _ => false,
            };
        }
        if !any { return; }
        self.batches += 1;
        let (reloaded, changed) = self.conv.take_changes();
        if reloaded { self.rebuild(cx); } else { self.update_rows(&changed, cx); }
        self.sync_meta(cx);
        cx.notify();
    }

    fn row(e: &Value, old_md: Option<Entity<TextViewState>>, cx: &mut Context<Self>) -> Option<Row> {
        let shown = Shown::of(e)?;
        let md = matches!(shown.kind, "assistant" | "thinking").then(|| match old_md {
            Some(st) => { let t = shown.text.clone(); st.update(cx, |s, cx| s.set_text(&t, cx)); st }
            None => { let t = shown.text.clone(); cx.new(|cx| TextViewState::markdown(&t, cx).selectable(true)) }
        });
        let text = SharedString::from(shown.text.clone());
        Some(Row { id: shown.id.clone(), shown, md, text })
    }

    /// A new history: every row again (the TextViewStates of entries that
    /// are still there are kept, so their parse is incremental).
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let mut old: HashMap<String, Entity<TextViewState>> = self.rows.drain(..).filter_map(|r| r.md.map(|m| (r.id, m))).collect();
        let entries = std::mem::take(&mut self.conv.entries);
        let rows: Vec<Row> = entries.iter().filter_map(|e| { let id = e.get("id").and_then(|i| i.as_str()).unwrap_or(""); let md = old.remove(id); Self::row(e, md, cx) }).collect();
        self.conv.entries = entries;
        // Where the reader was (the web's applyHistory): following the end,
        // it stays there; else on the same entry, at the same offset (a
        // reset alone would put the log back at its top).
        let first = self.row_of.is_empty();
        let top = self.list.logical_scroll_top();
        let top_id = self.row_of.iter().find(|(_, i)| **i == top.item_ix).map(|(id, _)| id.clone());
        let following = first || self.list.is_following_tail();
        self.rows = rows;
        self.row_of = self.rows.iter().enumerate().map(|(i, r)| (r.id.clone(), i)).collect();
        self.list.reset(self.rows.len());
        if following {
            self.list.set_follow_mode(FollowMode::Tail);
        } else if let Some(ix) = top_id.and_then(|id| self.row_of.get(&id).copied()) {
            self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: top.offset_in_item });
        }
        self.spliced += self.rows.len() as u64;
    }

    /// The entries a batch touched: their rows replaced (or added at the end).
    fn update_rows(&mut self, changed: &[String], cx: &mut Context<Self>) {
        for id in changed {
            let Some(pos) = self.conv.position(id) else { continue };
            let e = self.conv.entries[pos].clone();
            match self.row_of.get(id).copied() {
                Some(i) => {
                    let md = self.rows[i].md.take();
                    match Self::row(&e, md, cx) {
                        Some(r) => { self.rows[i] = r; self.list.splice(i..i + 1, 1); self.spliced += 1; }
                        // Not drawn any more (rare): everything again.
                        None => return self.rebuild(cx),
                    }
                }
                None => {
                    let Some(r) = Self::row(&e, None, cx) else { continue };
                    // A new entry is the last one shown; anything else (an
                    // entry shown late, out of order) rebuilds.
                    let later = self.conv.entries[pos + 1..].iter().any(|x| x.get("id").and_then(|i| i.as_str()).is_some_and(|i| self.row_of.contains_key(i)));
                    if later { return self.rebuild(cx); }
                    let i = self.rows.len();
                    self.row_of.insert(id.clone(), i);
                    self.rows.push(r);
                    self.list.splice(i..i, 1);
                    self.spliced += 1;
                }
            }
        }
    }

    fn sync_meta(&mut self, cx: &mut Context<Self>) {
        let m = {
            let old = &self.meta.read(cx).0;
            Meta { title: self.conv.title.clone(), state: self.conv.state.clone(), model: self.conv.model.clone(), cost: self.conv.cost, session_id: self.conv.session_id.clone(), ..old.clone() }
        };
        self.set_meta(m, cx);
    }

    pub fn set_meta(&self, m: Meta, cx: &mut App) {
        self.meta.update(cx, |x, cx| if x.0 != m { x.0 = m; cx.notify(); });
    }

    pub fn meta(&self, cx: &App) -> Meta { self.meta.read(cx).0.clone() }

    /// Send a message (POST /api/ui-sessions/:id/send); a refusal is shown as the error.
    pub fn send(&mut self, text: String, cx: &mut Context<Self>) {
        if text.trim().is_empty() { return; }
        let mut m = self.meta(cx);
        m.error.clear();
        m.sending = true;
        self.set_meta(m, cx);
        let path = format!("/api/ui-sessions/{}/send", self.id);
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.post(&path, &serde_json::json!({ "text": text })), move |r, cx| {
            let _ = me.update(cx, |s, cx| {
                let mut m = s.meta(cx);
                m.sending = false;
                match r {
                    Err(e) => m.error = e,
                    Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => m.error = v.get("error").and_then(|e| e.as_str()).unwrap_or("Not sent").to_string(),
                    Ok(_) => {}
                }
                s.set_meta(m, cx);
            });
        });
    }

    /// Stop the running turn (POST /api/ui-sessions/:id/stop).
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        let path = format!("/api/ui-sessions/{}/stop", self.id);
        crundi_shell::call(cx, move |api| api.post(&path, &serde_json::json!({})), |_, _| {});
    }

    /// A- / A+ (d = -1 / 1) or reset (0).
    pub fn zoom(&mut self, d: i32, cx: &mut Context<Self>) {
        let mut m = self.meta(cx);
        m.zoom = if d == 0 { 0 } else { (m.zoom + d).clamp(-4, 8) };
        self.set_meta(m, cx);
        // The rows' heights change with the text size.
        self.list.remeasure();
        cx.notify();
    }
}

/// The text scale of a zoom step (the web's 13px base, 1px a step).
pub fn scale(zoom: i32) -> f32 { (13.0 + zoom as f32) / 13.0 }

#[cfg(test)]
mod tests {
    //! The streaming path, headless: a history then a frame's batch of
    //! deltas is one update, re-deriving only the row that changed.
    use super::ChatStore;
    use gpui::{AppContext as _, TestAppContext};
    use serde_json::json;

    #[gpui::test]
    fn a_batch_splices_only_what_changed(cx: &mut TestAppContext) {
        let store = cx.new(|cx| ChatStore::new("c1", cx));
        let history = json!({ "type": "ui-history", "id": "c1", "session": { "id": "c1", "title": "T", "state": "working", "messages": [
            { "id": "u1", "kind": "user", "text": "hi" },
            { "id": "a1", "kind": "assistant-text", "text": "He", "streaming": true },
            { "id": "t1", "kind": "tool", "name": "Bash", "input": { "command": "ls" }, "status": "running" },
        ] } });
        store.update(cx, |s, cx| s.apply_batch(&[history], cx));
        store.read_with(cx, |s, _| { assert_eq!(s.rows.len(), 3); assert_eq!(s.batches, 1); assert_eq!(s.spliced, 3); assert_eq!(s.list.item_count(), 3); });
        // Many deltas in one frame: one batch, one row spliced.
        let deltas: Vec<_> = ["l", "l", "o", "!"].iter().map(|t| json!({ "type": "ui-event", "id": "c1", "event": { "type": "delta", "id": "a1", "text": t } })).collect();
        store.update(cx, |s, cx| s.apply_batch(&deltas, cx));
        store.read_with(cx, |s, _| { assert_eq!(s.batches, 2); assert_eq!(s.spliced, 4); assert_eq!(s.rows[1].shown.text, "Hello!"); });
        // A new entry goes at the end; the meta follows the state.
        let more = vec![
            json!({ "type": "ui-event", "id": "c1", "event": { "type": "entry", "entry": { "id": "r1", "kind": "result", "durationMs": 1200 } } }),
            json!({ "type": "ui-event", "id": "c1", "event": { "type": "state", "state": "idle" } }),
        ];
        store.update(cx, |s, cx| s.apply_batch(&more, cx));
        store.read_with(cx, |s, cx| { assert_eq!(s.rows.len(), 4); assert_eq!(s.list.item_count(), 4); assert_eq!(s.meta(cx).state, "idle"); assert_eq!(s.meta(cx).title, "T"); });
        // Nothing this chat shows: no update at all.
        store.update(cx, |s, cx| s.apply_batch(&[json!({ "type": "ui-event", "id": "c1", "event": { "type": "delta", "id": "nope", "text": "x" } })], cx));
        store.read_with(cx, |s, _| assert_eq!(s.batches, 3));
    }

    /// A history again (a reconnect): following the end, the log stays
    /// there; scrolled up, it stays on the entry the reader was at (the
    /// web's applyHistory), never back at its top.
    #[gpui::test]
    fn a_new_history_keeps_the_readers_place(cx: &mut TestAppContext) {
        let hist = |ids: &[&str]| json!({ "type": "ui-history", "id": "c1", "session": { "id": "c1", "state": "idle",
            "messages": ids.iter().map(|i| json!({ "id": i, "kind": "user", "text": i })).collect::<Vec<_>>() } });
        let store = cx.new(|cx| ChatStore::new("c1", cx));
        store.update(cx, |s, cx| s.apply_batch(&[hist(&["a", "b", "c"])], cx));
        store.read_with(cx, |s, _| assert!(s.list.is_following_tail()));
        store.update(cx, |s, cx| s.apply_batch(&[hist(&["a", "b", "c", "d"])], cx));
        store.read_with(cx, |s, _| { assert!(s.list.is_following_tail()); assert_eq!(s.list.item_count(), 4); });
        // Scrolled up to "b" (a person's wheel): following stops.
        store.read_with(cx, |s, _| s.list.scroll_to(gpui::ListOffset { item_ix: 1, offset_in_item: gpui::px(7.) }));
        store.update(cx, |s, cx| s.apply_batch(&[hist(&["z", "a", "b", "c", "d", "e"])], cx));
        store.read_with(cx, |s, _| {
            assert!(!s.list.is_following_tail());
            let top = s.list.logical_scroll_top();
            assert_eq!((top.item_ix, top.offset_in_item), (2, gpui::px(7.)));
        });
    }
}
