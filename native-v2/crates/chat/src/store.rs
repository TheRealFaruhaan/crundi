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
use crundi_core::mdblocks::{self, Block, Lead};
use crundi_theme::T;
use gpui::*;
use gpui_component::text::TextViewState;
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// One row of the log, ready to lay out (render never parses). An assistant
/// text is one row per markdown block (core::mdblocks), so the log lays out
/// and paints only the blocks on screen, and a streamed delta changes only
/// the last block's row.
pub struct Row {
    pub id: String,
    pub shown: Rc<Shown>,
    /// Claude's text: this block's, parsed off the UI thread.
    pub md: Option<Entity<TextViewState>>,
    /// The block's source; the user's bubble and other plain text (shared,
    /// no copy per frame).
    pub text: SharedString,
    /// An assistant text's block: where it stands among its siblings.
    pub block: Option<BlockRow>,
}

/// A block row's look: the room above it and below it that the whole
/// document drawn as one would have there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockRow { pub ix: usize, pub lead: Lead, pub gap: bool, pub last: bool }

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

/// An assistant text's last cut into blocks (the base the next one extends).
#[derive(Clone)]
struct Cut { text: String, blocks: Vec<Block> }

/// Parsing done on the UI thread in one update, at most (bytes of markdown
/// cut): about 2 ms. Anything more is cut on the background executor.
const INLINE_PARSE: usize = 4096;

pub struct ChatStore {
    pub id: String,
    pub conv: Conversation,
    pub rows: Vec<Row>,
    /// The rows of each entry id: (first row, how many).
    span: HashMap<String, (usize, usize)>,
    /// Each assistant text's blocks, and the background cuts under way.
    cuts: HashMap<String, Cut>,
    jobs: HashMap<String, Task<()>>,
    /// What is left of this update's INLINE_PARSE.
    budget: usize,
    pub list: ListState,
    pub meta: Entity<ChatMeta>,
    /// Batches applied (the driver's count) and the rows spliced.
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
        ChatStore { id: id.to_string(), conv, rows: vec![], span: HashMap::new(), cuts: HashMap::new(), jobs: HashMap::new(), budget: INLINE_PARSE, list, meta, batches: 0, spliced: 0 }
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
        self.budget = INLINE_PARSE;
        let (reloaded, changed) = self.conv.take_changes();
        if reloaded { self.rebuild(cx); } else { self.update_rows(&changed, cx); }
        self.sync_meta(cx);
        cx.notify();
    }

    /// The blocks of an assistant text: the last cut when the text is the
    /// same; extended from it when only its tail needs cutting again (a
    /// streamed delta); cut here when small; otherwise cut on the background
    /// executor (one job per entry at a time, the rows refreshed when it is
    /// done) and, until then, the last cut with its last block running to
    /// the end (or nothing yet, for a text never cut).
    fn blocks_for(&mut self, id: &str, text: &str, cx: &mut Context<Self>) -> Vec<Block> {
        let base = self.cuts.get(id);
        if let Some(c) = base { if c.text == text { return c.blocks.clone(); } }
        // From where an update would cut again (core::mdblocks::update).
        let tail_from = base.and_then(|c| (c.blocks.len() >= 3).then(|| c.blocks[c.blocks.len() - 2].range.start))
            .filter(|&f| text.len() >= f && c_prefix(base, text, f));
        let work = match tail_from { Some(f) => text.len() - f, None => text.len() };
        if work <= self.budget {
            self.budget -= work;
            let blocks = match base { Some(c) if tail_from.is_some() => mdblocks::update(&c.blocks, &c.text, text), _ => mdblocks::split(text) };
            self.cuts.insert(id.to_string(), Cut { text: text.to_string(), blocks: blocks.clone() });
            return blocks;
        }
        let provisional = match base {
            Some(c) => match c.blocks.last() {
                Some(l) if text.len() >= l.range.start && c_prefix(base, text, l.range.start) => {
                    let mut b = c.blocks.clone();
                    let last = b.last_mut().unwrap();
                    last.range.end = text.len();
                    b
                }
                _ => vec![],
            },
            None => vec![],
        };
        if !self.jobs.contains_key(id) {
            let (key, text, base) = (id.to_string(), text.to_string(), base.cloned());
            let task = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let t = text.clone();
                let blocks = cx.background_spawn(async move {
                    match &base { Some(c) => mdblocks::update(&c.blocks, &c.text, &t), None => mdblocks::split(&t) }
                }).await;
                let _ = this.update(cx, |s, cx| s.cut_done(key, text, blocks, cx));
            });
            self.jobs.insert(id.to_string(), task);
        }
        provisional
    }

    fn cut_done(&mut self, id: String, text: String, blocks: Vec<Block>, cx: &mut Context<Self>) {
        self.jobs.remove(&id);
        self.cuts.insert(id.clone(), Cut { text, blocks });
        self.budget = INLINE_PARSE;
        self.update_rows(&[id], cx);
        cx.notify();
    }

    /// An entry's rows, reusing what `old` (its rows as they were) already
    /// has: a row whose block and look did not change is kept as it is (not
    /// spliced); a changed block's TextViewState gets the new text (an
    /// extension is appended, the parse runs on the background executor).
    /// Returns the rows and the first one that changed; None: not shown.
    fn entry_rows(&mut self, e: &Value, old: Vec<Row>, cx: &mut Context<Self>) -> Option<(Vec<Row>, usize)> {
        let shown = Rc::new(Shown::of(e)?);
        if shown.kind != "assistant" {
            let text = SharedString::from(shown.text.clone());
            return Some((vec![Row { id: shown.id.clone(), shown, md: None, text, block: None }], 0));
        }
        let blocks = self.blocks_for(&shown.id, &shown.text, cx);
        let n = blocks.len();
        let mut old = old.into_iter().map(Some).collect::<Vec<_>>();
        let mut rows = Vec::with_capacity(n);
        let mut first_changed = n;
        for (j, b) in blocks.iter().enumerate() {
            let src = shown.text.get(b.range.clone()).unwrap_or("");
            let look = BlockRow { ix: j, lead: b.lead, gap: b.gap, last: j + 1 == n };
            let prev = old.get_mut(j).and_then(|o| o.take());
            match prev {
                Some(mut r) if r.block == Some(look) && r.text.as_ref() == src => { r.shown = shown.clone(); rows.push(r); }
                prev => {
                    first_changed = first_changed.min(j);
                    let md = match prev.and_then(|r| r.md) {
                        Some(st) => { st.update(cx, |s, cx| s.set_text(src, cx)); st }
                        None => cx.new(|cx| TextViewState::markdown(src, cx).selectable(true)),
                    };
                    rows.push(Row { id: shown.id.clone(), shown: shown.clone(), md: Some(md), text: SharedString::from(src.to_string()), block: Some(look) });
                }
            }
        }
        if old.len() > n { first_changed = first_changed.min(n); }
        Some((rows, first_changed))
    }

    /// A new history: every row again (the rows and TextViewStates of
    /// entries that are still there are kept, so their parse is incremental).
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        // Where the reader was (the web's applyHistory): following the end,
        // it stays there; else on the same block of the same entry, at the
        // same offset (a reset alone would put the log back at its top).
        let first = self.span.is_empty();
        let top = self.list.logical_scroll_top();
        let top_at = self.rows.get(top.item_ix).map(|r| (r.id.clone(), r.block.map_or(0, |b| b.ix)));
        let following = first || self.list.is_following_tail();
        let mut old: HashMap<String, Vec<Row>> = HashMap::new();
        for r in self.rows.drain(..) { old.entry(r.id.clone()).or_default().push(r); }
        let entries = std::mem::take(&mut self.conv.entries);
        let mut rows = vec![];
        let mut span = HashMap::new();
        for e in &entries {
            let id = e.get("id").and_then(|i| i.as_str()).unwrap_or("");
            let prev = old.remove(id).unwrap_or_default();
            if let Some((r, _)) = self.entry_rows(e, prev, cx) { span.insert(id.to_string(), (rows.len(), r.len())); rows.extend(r); }
        }
        self.conv.entries = entries;
        self.cuts.retain(|id, _| span.contains_key(id));
        self.rows = rows;
        self.span = span;
        self.list.reset(self.rows.len());
        if following {
            self.list.set_follow_mode(FollowMode::Tail);
        } else if let Some((s, n)) = top_at.and_then(|(id, b)| self.span.get(&id).map(|&(s, n)| (s + b.min(n.saturating_sub(1)), n))) {
            if n > 0 { self.list.scroll_to(ListOffset { item_ix: s, offset_in_item: top.offset_in_item }); }
        }
        self.spliced += self.rows.len() as u64;
    }

    /// The entries a batch touched: their changed rows replaced (or added at
    /// the end).
    fn update_rows(&mut self, changed: &[String], cx: &mut Context<Self>) {
        for id in changed {
            let Some(pos) = self.conv.position(id) else { continue };
            let e = self.conv.entries[pos].clone();
            match self.span.get(id).copied() {
                Some((start, len)) => {
                    let old: Vec<Row> = self.rows.drain(start..start + len).collect();
                    let Some((new, j0)) = self.entry_rows(&e, old, cx) else {
                        // Not drawn any more (rare): everything again.
                        self.span.remove(id);
                        return self.rebuild(cx);
                    };
                    let n = new.len();
                    self.rows.splice(start..start, new);
                    if n != len {
                        self.span.insert(id.clone(), (start, n));
                        self.respan();
                    }
                    if j0 < n.max(len) {
                        self.list.splice(start + j0..start + len.max(j0), n.saturating_sub(j0));
                        self.spliced += n.saturating_sub(j0).max(1) as u64;
                    }
                }
                None => {
                    // A new entry is the last one shown; anything else (an
                    // entry shown late, out of order) rebuilds.
                    let later = self.conv.entries[pos + 1..].iter().any(|x| x.get("id").and_then(|i| i.as_str()).is_some_and(|i| self.span.contains_key(i)));
                    if later { return self.rebuild(cx); }
                    let Some((new, _)) = self.entry_rows(&e, vec![], cx) else { continue };
                    let (i, n) = (self.rows.len(), new.len());
                    self.span.insert(id.clone(), (i, n));
                    self.rows.extend(new);
                    self.list.splice(i..i, n);
                    self.spliced += n as u64;
                }
            }
        }
    }

    /// Every entry's first row again, in the conversation's order (after an
    /// entry's row count changed).
    fn respan(&mut self) {
        let mut at = 0;
        for e in &self.conv.entries {
            let Some(id) = e.get("id").and_then(|i| i.as_str()) else { continue };
            if let Some(v) = self.span.get_mut(id) { v.0 = at; at += v.1; }
        }
    }

    /// One row per entry (the driver's "entries": an assistant text's first
    /// block stands for it, with its whole text).
    pub fn entry_rows_shown(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(|r| r.block.is_none_or(|b| b.ix == 0))
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

/// Whether `text` starts with the cut's text up to `at`.
fn c_prefix(base: Option<&Cut>, text: &str, at: usize) -> bool {
    base.is_some_and(|c| c.text.get(..at).is_some_and(|p| text.get(..at) == Some(p)))
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
        store.read_with(cx, |s, _| { assert_eq!(s.batches, 2); assert_eq!(s.spliced, 4); assert_eq!(s.rows[1].shown.text, "Hello!"); assert_eq!(s.rows[1].text.as_ref(), "Hello!"); });
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

    /// F10: a long reply is one row per markdown block. Streamed, each batch
    /// splices only its new blocks and the one that was last (the earlier rows and their
    /// TextViewStates are kept as they are); the rows are always the blocks
    /// a whole cut of the text gives.
    #[gpui::test]
    fn a_streamed_reply_splices_only_its_last_blocks(cx: &mut TestAppContext) {
        let store = cx.new(|cx| ChatStore::new("c1", cx));
        let history = json!({ "type": "ui-history", "id": "c1", "session": { "id": "c1", "state": "working", "messages": [
            { "id": "u1", "kind": "user", "text": "count" },
            { "id": "a1", "kind": "assistant-text", "text": "", "streaming": true },
        ] } });
        store.update(cx, |s, cx| s.apply_batch(&[history], cx));
        let full: String = std::iter::once("Here they are:\n\n".to_string()).chain((1..=300).map(|i| format!("{i}. number {i}\n"))).chain(std::iter::once("\nDone.".to_string())).collect();
        let mut sent = 0;
        let mut kept: Option<(gpui::EntityId, gpui::EntityId)> = None;
        while sent < full.len() {
            let end = (sent + 23).min(full.len());
            let deltas: Vec<_> = [&full[sent..end]].iter().map(|t| json!({ "type": "ui-event", "id": "c1", "event": { "type": "delta", "id": "a1", "text": t } })).collect();
            let (before, rows_before) = store.read_with(cx, |s, _| (s.spliced, s.rows.len()));
            store.update(cx, |s, cx| s.apply_batch(&deltas, cx));
            sent = end;
            store.read_with(cx, |s, _| {
                let text = &full[..sent];
                let want = crundi_core::mdblocks::split(text);
                let got: Vec<&str> = s.rows[1..].iter().map(|r| r.text.as_ref()).collect();
                assert_eq!(got, crundi_core::mdblocks::texts(text, &want), "at {sent}");
                assert_eq!(s.list.item_count(), s.rows.len());
                // The new blocks, and the one that was last before them.
                let added = (s.rows.len() - rows_before) as u64;
                assert!(s.spliced - before <= added + 1, "a batch spliced {} rows, {added} new, at {sent}", s.spliced - before);
                // The first blocks' states are the same entities all along.
                if s.rows.len() > 3 {
                    let ids = (s.rows[1].md.as_ref().unwrap().entity_id(), s.rows[2].md.as_ref().unwrap().entity_id());
                    if let Some(k) = kept { assert_eq!(k, ids); } else { kept = Some(ids); }
                }
            });
        }
        store.read_with(cx, |s, _| {
            assert_eq!(s.rows.len(), 1 + 302);
            let looks: Vec<_> = s.rows[1..].iter().map(|r| r.block.unwrap()).collect();
            assert!(looks[..301].iter().all(|b| !b.last) && looks[301].last);
            assert_eq!(s.entry_rows_shown().count(), 2);
        });
    }

    /// A long reply in a history is cut on the background executor: until it
    /// is, the entry draws nothing; then its blocks are its rows.
    #[gpui::test]
    fn a_long_history_reply_is_cut_off_the_ui_thread(cx: &mut TestAppContext) {
        let text: String = (1..=400).map(|i| format!("{i}. number {i}\n")).collect();
        assert!(text.len() > super::INLINE_PARSE);
        let store = cx.new(|cx| ChatStore::new("c1", cx));
        let history = json!({ "type": "ui-history", "id": "c1", "session": { "id": "c1", "state": "idle", "messages": [
            { "id": "u1", "kind": "user", "text": "count" },
            { "id": "a1", "kind": "assistant-text", "text": text },
            { "id": "u2", "kind": "user", "text": "thanks" },
        ] } });
        store.update(cx, |s, cx| s.apply_batch(&[history.clone()], cx));
        store.read_with(cx, |s, _| { assert_eq!(s.rows.len(), 2); assert_eq!(s.jobs.len(), 1); });
        cx.run_until_parked();
        store.read_with(cx, |s, _| {
            assert_eq!(s.rows.len(), 402);
            assert_eq!(s.list.item_count(), 402);
            assert_eq!(s.rows[401].text.as_ref(), "thanks");
            assert!(s.jobs.is_empty());
        });
        // The same history again (a reconnect): nothing is cut again.
        store.update(cx, |s, cx| s.apply_batch(&[history], cx));
        store.read_with(cx, |s, _| { assert_eq!(s.rows.len(), 402); assert!(s.jobs.is_empty()); });
    }
}
