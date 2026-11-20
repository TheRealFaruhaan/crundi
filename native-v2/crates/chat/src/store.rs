//! One chat's store (ARCHITECTURE.md, "How server data becomes view
//! state"): the conversation (core::chat::Conversation, the server's
//! entries), its rows as the log draws them (each assistant text cut into
//! the web's markdown blocks, core::webmd: a long one on the background
//! executor, a streamed one only from its last open block), and the log's
//! ListState, spliced only for the rows a batch touched. The small part the head and the composer show (title, state,
//! model, cost, the last error, the text size) is a separate entity,
//! `ChatMeta`, notified only when it changes: a streamed delta redraws the
//! log alone.

use crundi_core::chat::{Conversation, Shown};
use crundi_core::webmd::{self, Block, Inline, Kind};
use crundi_theme::T;
use gpui::*;
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// One row of the log, ready to lay out (render never parses). An assistant
/// text is one row per markdown block (core::webmd), so the log lays out
/// and paints only the blocks on screen, and a streamed delta changes only
/// the last blocks' rows.
#[derive(Clone)]
pub struct Row {
    pub id: String,
    pub shown: Rc<Shown>,
    /// Claude's text: this block, parsed (off the UI thread when long).
    pub blk: Option<Rc<Block>>,
    /// The user's bubble (its text with its links, the web's linkify), a
    /// notice's and an error's text.
    pub plain: Option<Rc<Inline>>,
    /// The texts the log draws and selects in, each piece once (shared).
    pub parts: Vec<SharedString>,
    /// What the row copies as: the block's text; the user's bubble and other
    /// plain text (shared, no copy per frame).
    pub text: SharedString,
    /// An assistant text's block: where it stands among its siblings.
    pub block: Option<BlockRow>,
}

/// A block row's place: its kind and the one before it (the margins the
/// web's CSS gives them collapse between the two), and whether it is last.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockRow { pub ix: usize, pub prev: Option<Kind>, pub kind: Kind, pub last: bool }

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
    /// The plan's status and the permission mode, while the chat has a plan
    /// (the head's PLAN pill: crate::log::plan_status; the web's notifyMeta).
    pub plan: Option<(String, String)>,
    /// For the composer (it never observes the streaming store): the
    /// permission mode and whether Bypass all is in its list (2.6), the
    /// messages handed over while Claude works (2.9), the CLI's commands for
    /// the slash menu (2.11), and whether a plan is shown (2.19).
    pub mode: String,
    pub skip: bool,
    pub slots: Vec<crundi_core::chat::Slot>,
    pub slash: Vec<Value>,
    pub has_plan: bool,
}

pub struct ChatMeta(pub Meta);

/// An assistant text's last cut into blocks (the base the next one extends).
#[derive(Clone)]
struct Cut { text: String, cut: webmd::Cut }

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
    /// The log's state for this chat, shared by every log that shows it
    /// (thinking rules, groups, side questions, question picks: crate::log).
    pub log: crate::log::Shared,
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
        ChatStore { id: id.to_string(), conv, rows: vec![], span: HashMap::new(), cuts: HashMap::new(), jobs: HashMap::new(), budget: INLINE_PARSE, list, log: Default::default(), meta, batches: 0, spliced: 0 }
    }

    /// This frame's messages for this chat (ui-history, ui-event), applied in
    /// one go: rows re-derived only for the entries that changed, one notify.
    pub fn apply_batch(&mut self, msgs: &[Value], cx: &mut Context<Self>) {
        let mut any = false;
        // A state from the server (or a whole snapshot) ends the composer's
        // optimistic "working" after a send (Meta::sending).
        let said_state = msgs.iter().any(|m| m.get("type").and_then(|t| t.as_str()) == Some("ui-history")
            || matches!(m.pointer("/event/type").and_then(|t| t.as_str()), Some("state" | "exit" | "history")));
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
        crate::log::follow(self, reloaded, &changed, cx);
        self.sync_meta(said_state, cx);
        cx.notify();
    }

    /// A change the client makes to its own copy of the conversation (the
    /// composer's: a handed-over message's slot, a refused send's error row,
    /// an event played by the driver): the rows it touched spliced, the meta
    /// synced; the log notified only when a row changed.
    pub fn edit_conv(&mut self, f: impl FnOnce(&mut Conversation), cx: &mut Context<Self>) {
        f(&mut self.conv);
        let (reloaded, changed) = self.conv.take_changes();
        let rows = reloaded || !changed.is_empty();
        if reloaded { self.rebuild(cx); } else if rows { self.update_rows(&changed, cx); }
        if rows { crate::log::follow(self, reloaded, &changed, cx); }
        self.sync_meta(false, cx);
        if rows { cx.notify(); }
    }

    /// The blocks of an assistant text: the last cut when the text is the
    /// same; extended from it when only its tail needs parsing again (a
    /// streamed delta, core::webmd::update); parsed here when small;
    /// otherwise parsed on the background executor (one job per entry at a
    /// time, the rows refreshed when it is done) and, until then, the last
    /// cut as it was (or nothing yet, for a text never cut).
    fn blocks_for(&mut self, id: &str, text: &str, cx: &mut Context<Self>) -> Vec<Block> {
        let base = self.cuts.get(id);
        if let Some(c) = base { if c.text == text { return c.cut.blocks.clone(); } }
        let from = base.map(|c| c.cut.resume).filter(|&r| text.get(..r).is_some() && text.get(..r) == c_text(base).get(..r)).unwrap_or(0);
        let work = text.len() - from;
        if work <= self.budget {
            self.budget -= work;
            let cut = match base { Some(c) => webmd::update(&c.cut, &c.text, text), None => webmd::split(text) };
            let blocks = cut.blocks.clone();
            self.cuts.insert(id.to_string(), Cut { text: text.to_string(), cut });
            return blocks;
        }
        let provisional = base.map(|c| c.cut.blocks.clone()).unwrap_or_default();
        if !self.jobs.contains_key(id) {
            let (key, text, base) = (id.to_string(), text.to_string(), base.cloned());
            let task = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let t = text.clone();
                let cut = cx.background_spawn(async move {
                    match &base { Some(c) => webmd::update(&c.cut, &c.text, &t), None => webmd::split(&t) }
                }).await;
                let _ = this.update(cx, |s, cx| s.cut_done(key, text, cut, cx));
            });
            self.jobs.insert(id.to_string(), task);
        }
        provisional
    }

    fn cut_done(&mut self, id: String, text: String, cut: webmd::Cut, cx: &mut Context<Self>) {
        self.jobs.remove(&id);
        // The text moved on meanwhile: the next batch extends this cut.
        self.cuts.insert(id.clone(), Cut { text, cut });
        self.budget = INLINE_PARSE;
        self.update_rows(&[id], cx);
        cx.notify();
    }

    /// An entry's rows, reusing what `old` (its rows as they were) already
    /// has: a row whose block and place did not change is kept as it is (not
    /// spliced). Returns the rows and the first one that changed; None: not
    /// shown.
    fn entry_rows(&mut self, e: &Value, old: Vec<Row>, cx: &mut Context<Self>) -> Option<(Vec<Row>, usize)> {
        let shown = Rc::new(Shown::of(e)?);
        if shown.kind != "assistant" {
            let text = SharedString::from(shown.text.clone());
            let plain = match shown.kind {
                "user" => Some(Rc::new(webmd::plain(&shown.text))),
                "notice" | "error" => Some(Rc::new(Inline { text: shown.text.clone(), spans: vec![] })),
                _ => None,
            };
            let parts = if plain.is_some() { vec![text.clone()] } else { vec![] };
            return Some((vec![Row { id: shown.id.clone(), shown, blk: None, plain, parts, text, block: None }], 0));
        }
        let blocks = self.blocks_for(&shown.id, &shown.text, cx);
        let n = blocks.len();
        let mut old = old.into_iter().map(Some).collect::<Vec<_>>();
        let mut rows = Vec::with_capacity(n);
        let mut first_changed = n;
        for (j, b) in blocks.into_iter().enumerate() {
            let look = BlockRow { ix: j, prev: (j > 0).then(|| rows_kind(&rows, j - 1)).flatten(), kind: b.kind.clone(), last: j + 1 == n };
            let prev = old.get_mut(j).and_then(|o| o.take());
            match prev {
                Some(mut r) if r.block.as_ref() == Some(&look) && r.blk.as_ref().is_some_and(|x| x.parts == b.parts) && !(look.last && r.shown.status != shown.status) => { r.shown = shown.clone(); rows.push(r); }
                _ => {
                    first_changed = first_changed.min(j);
                    let text = SharedString::from(webmd::copy_text(&b));
                    let parts = b.parts.iter().map(|p| SharedString::from(p.text.clone())).collect();
                    rows.push(Row { id: shown.id.clone(), shown: shown.clone(), blk: Some(Rc::new(b)), plain: None, parts, text, block: Some(look) });
                }
            }
        }
        if old.len() > n { first_changed = first_changed.min(n); }
        Some((rows, first_changed))
    }

    /// A new history: every row again (the rows and cuts of entries that are
    /// still there are kept, so nothing is parsed again).
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        // Where the reader was (the web's applyHistory): following the end,
        // it stays there; else on the same block of the same entry, at the
        // same offset (a reset alone would put the log back at its top).
        let first = self.span.is_empty();
        let l = &self.list;
        let top = l.logical_scroll_top();
        let (top_at, following) = (self.rows.get(top.item_ix).map(|r| (r.id.clone(), r.block.as_ref().map_or(0, |b| b.ix))), first || l.is_following_tail());
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
        let l = &self.list;
        l.reset(self.rows.len());
        if following {
            l.set_follow_mode(FollowMode::Tail);
        } else if let Some((s, n)) = top_at.and_then(|(id, b)| self.span.get(&id).map(|&(s, n)| (s + b.min(n.saturating_sub(1)), n))) {
            if n > 0 { l.scroll_to(ListOffset { item_ix: s, offset_in_item: top.offset_in_item }); }
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
                        self.splice(start + j0..start + len.max(j0), n.saturating_sub(j0));
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
                    self.splice(i..i, n);
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

    fn splice(&self, range: std::ops::Range<usize>, n: usize) { self.list.splice(range, n); }

    /// An entry's rows measured again (what a row draws
    /// changed height without its text changing: a thinking block folded, a
    /// side question under it, a call that became an agent's row).
    pub fn remeasure(&mut self, id: &str) {
        if let Some(&(s, n)) = self.span.get(id) { if n > 0 { self.splice(s..s + n, n); self.spliced += n as u64; } }
    }

    /// An entry's rows: (first, how many).
    pub fn span_of(&self, id: &str) -> Option<(usize, usize)> { self.span.get(id).copied() }

    /// The row of an entry's block (the log's selection is kept by entry).
    pub fn row_index(&self, id: &str, blk: usize) -> Option<usize> {
        self.span.get(id).filter(|(_, n)| blk < *n).map(|(s, _)| s + blk)
    }

    /// One row per entry (the driver's "entries": an assistant text's first
    /// block stands for it, with its whole text).
    pub fn entry_rows_shown(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(|r| r.block.as_ref().is_none_or(|b| b.ix == 0))
    }

    fn sync_meta(&mut self, said_state: bool, cx: &mut Context<Self>) {
        let m = {
            let old = &self.meta.read(cx).0;
            let c = &self.conv;
            Meta { title: c.title.clone(), state: c.state.clone(), model: c.model.clone(), cost: c.cost, session_id: c.session_id.clone(),
                mode: c.mode.clone(), skip: c.skip, slots: c.slots.clone(), slash: c.slash.clone(), has_plan: c.plan.is_some(), plan: crate::log::goal::plan_meta(c),
                sending: old.sending && !said_state, ..old.clone() }
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

fn c_text(base: Option<&Cut>) -> &str { base.map_or("", |c| c.text.as_str()) }

/// The kind of the block row `j` of `rows` (an entry's rows being made).
fn rows_kind(rows: &[Row], j: usize) -> Option<Kind> { rows.get(j).and_then(|r| r.block.as_ref()).map(|b| b.kind.clone()) }

/// The text scale of a zoom step (the web's 13px base, 1px a step).
pub fn scale(zoom: i32) -> f32 { (13.0 + zoom as f32) / 13.0 }

#[cfg(test)]
mod tests {
    //! The streaming path, headless: a history then a frame's batch of
    //! deltas is one update, re-deriving only the row that changed.
    use super::ChatStore;
    use gpui::{AppContext as _, TestAppContext};
    use std::rc::Rc;
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
    /// splices only its new blocks and the one that was last (the earlier
    /// rows are kept as they are); the rows are always the blocks
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
        let mut kept: Option<(usize, usize)> = None;
        while sent < full.len() {
            let end = (sent + 23).min(full.len());
            let deltas: Vec<_> = [&full[sent..end]].iter().map(|t| json!({ "type": "ui-event", "id": "c1", "event": { "type": "delta", "id": "a1", "text": t } })).collect();
            let (before, rows_before) = store.read_with(cx, |s, _| (s.spliced, s.rows.len()));
            store.update(cx, |s, cx| s.apply_batch(&deltas, cx));
            sent = end;
            store.read_with(cx, |s, _| {
                let text = &full[..sent];
                let want: Vec<String> = crundi_core::webmd::split(text).blocks.iter().map(crundi_core::webmd::copy_text).collect();
                let got: Vec<&str> = s.rows[1..].iter().map(|r| r.text.as_ref()).collect();
                assert_eq!(got, want, "at {sent}");
                assert_eq!(s.list.item_count(), s.rows.len());
                // The new blocks, and the two that were last before them (a
                // half-sent "2" is a paragraph that closed the list, as in
                // the web: its item before stops being the list's last).
                let added = (s.rows.len() - rows_before) as u64;
                assert!(s.spliced - before <= added + 2, "a batch spliced {} rows, {added} new, at {sent}", s.spliced - before);
                // The first blocks are the same rows all along.
                if s.rows.len() > 3 {
                    let ids = (Rc::as_ptr(s.rows[1].blk.as_ref().unwrap()) as usize, Rc::as_ptr(s.rows[2].blk.as_ref().unwrap()) as usize);
                    if let Some(k) = kept { assert_eq!(k, ids); } else { kept = Some(ids); }
                }
            });
        }
        store.read_with(cx, |s, _| {
            assert_eq!(s.rows.len(), 1 + 302);
            let looks: Vec<_> = s.rows[1..].iter().map(|r| r.block.clone().unwrap()).collect();
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
