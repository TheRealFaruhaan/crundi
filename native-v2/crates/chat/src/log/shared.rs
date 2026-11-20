//! What every log of one chat shares (a pane's and a bubble's): kept in the
//! chat's store (`ChatStore::log`) and followed once per batch (`follow`,
//! called by the store after it derived its rows), never in render.
//!
//! - the thinking blocks' rules (2.15, crundi_core::think): a block that
//!   starts streaming opens and folds the others; a finished one folds 5 s
//!   later; the reader's click wins;
//! - each group's calls (2.14: the web's groupMembers), its head being the
//!   call whose id is the group's;
//! - the calls that are an agent's row (2.18: a call whose toolUseId has an
//!   agent record that is not a background command);
//! - the side questions (2.21), the question cards' picks (2.13), the
//!   factory cards' own state (2.20) and the parked card's (2.23).
//!
//! A change that alters a row's height without its text changing (a block
//! folded, a call turned into an agent's row) measures that entry's rows
//! again in every list (`ChatStore::remeasure`).

use crate::store::ChatStore;
use crundi_core::think::{Think, LINGER_MS};
use gpui::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Default)]
pub struct Shared {
    /// 2.15: when each thinking block is open; the blocks seen streaming.
    pub think: Think,
    live: HashSet<String>,
    /// 2.14: each group's calls, in order (the group id is its first call's).
    pub groups: HashMap<String, Vec<String>>,
    /// 2.18: each call's entry by its toolUseId, and the calls drawn as an agent's row.
    tid_entry: HashMap<String, String>,
    pub agent_rows: HashSet<String>,
    /// 2.21: the side questions, in the order asked.
    pub asides: Vec<super::aside::Aside>,
    pub aside_next: u32,
    /// 2.13: each open question card's questions and picks.
    pub cards: HashMap<String, super::questions::Card>,
    pub local_errors: u32,
    /// 2.20: each factory result card's own state.
    pub fr: HashMap<String, super::cards::Fr>,
    pub fr_last: Value,
    /// 2.23: the parked card's running choice and whether its stored
    /// conversation was asked for.
    pub park_busy: String,
    pub park_preview: bool,
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// The store's batch is applied (its rows derived): the log's shared state follows it.
pub fn follow(st: &mut ChatStore, reloaded: bool, changed: &[String], cx: &mut Context<ChatStore>) {
    let ids: Vec<String> = if reloaded { st.conv.entries.iter().map(|e| s(e, "id")).collect() } else { changed.to_vec() };
    if reloaded {
        st.log.groups.clear();
        st.log.tid_entry.clear();
    }
    let mut again: Vec<String> = vec![];
    // Thinking: blocks that started or finished streaming.
    let thinking: Vec<String> = st.conv.entries.iter().filter(|e| s(e, "kind") == "thinking").map(|e| s(e, "id")).collect();
    let before: Vec<bool> = thinking.iter().map(|id| st.log.think.open(id)).collect();
    for id in &ids {
        let Some(pos) = st.conv.position(id) else { continue };
        let e = &st.conv.entries[pos];
        match s(e, "kind").as_str() {
            "thinking" => {
                let streaming = e.get("streaming").and_then(|v| v.as_bool()).unwrap_or(false);
                if streaming && st.log.live.insert(id.clone()) { st.log.think.started(id); }
                if !streaming && st.log.live.remove(id) { linger(id.clone(), cx); }
            }
            "tool" => {
                let g = s(e, "group");
                if !g.is_empty() && !s(e, "groupSummary").is_empty() {
                    let m = st.log.groups.entry(g).or_default();
                    if !m.contains(id) { m.push(id.clone()); }
                }
                let tid = s(e, "toolUseId");
                if !tid.is_empty() { st.log.tid_entry.insert(tid, id.clone()); }
            }
            _ => {}
        }
    }
    for (i, id) in thinking.iter().enumerate() { if st.log.think.open(id) != before[i] { again.push(id.clone()); } }
    // Agent rows: the calls whose record is an agent's (not a command's).
    let rows: HashSet<String> = st.conv.agents.iter().filter(|a| s(a, "kind") != "task")
        .filter_map(|a| st.log.tid_entry.get(&s(a, "toolUseId")).cloned()).collect();
    if rows != st.log.agent_rows {
        again.extend(rows.symmetric_difference(&st.log.agent_rows).cloned());
        st.log.agent_rows = rows;
    }
    super::questions::follow(st, &ids, cx);
    super::cards::follow(st, &ids);
    if !reloaded { for id in again { st.remeasure(&id); } }
}

/// A finished block folds LINGER_MS later (unless the reader chose).
fn linger(id: String, cx: &mut Context<ChatStore>) {
    cx.spawn(async move |this: WeakEntity<ChatStore>, cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_millis(LINGER_MS)).await;
        let _ = this.update(cx, |st, cx| {
            let was = st.log.think.open(&id);
            st.log.think.linger_over(&id);
            if st.log.think.open(&id) != was { st.remeasure(&id); cx.notify(); }
        });
    }).detach();
}

/// The reader opens or closes a thinking block (every log of the chat follows).
pub fn think_toggle(store: &Entity<ChatStore>, id: &str, cx: &mut App) {
    store.update(cx, |st, cx| { st.log.think.toggle(id); st.remeasure(id); cx.notify(); });
}

/// A side question's rows: under the last row of the entry it follows.
pub fn remeasure_after(store: &Entity<ChatStore>, anchor: &str, cx: &mut App) {
    store.update(cx, |st, cx| {
        let id = if st.span_of(anchor).is_some() { anchor.to_string() } else { st.rows.last().map(|r| r.id.clone()).unwrap_or_default() };
        st.remeasure(&id);
        cx.notify();
    });
}

/// A local change to the conversation (an answered card, a dismissed
/// suggestion): played into the store as the server's event would be.
pub fn local(store: &Entity<ChatStore>, event: Value, cx: &mut App) {
    let id = store.read(cx).id.clone();
    store.update(cx, |st, cx| st.apply_batch(&[serde_json::json!({ "type": "ui-event", "id": id, "event": event })], cx));
}

/// The chat's error line (under its composer): set, or cleared with "".
pub fn set_error(store: &Entity<ChatStore>, e: String, cx: &mut App) {
    let mut m = store.read(cx).meta(cx);
    m.error = e;
    let st = store.clone();
    st.update(cx, |s, cx| s.set_meta(m, cx));
}
