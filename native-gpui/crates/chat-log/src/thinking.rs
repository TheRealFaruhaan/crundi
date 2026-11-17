//! 2.15: thinking blocks (specs/thinking.md): a live shimmer while it
//! streams, open while streaming, folding 5 s after it finishes
//! (crundi_core::think's rules, LINGER_MS), the reader's click wins, opening
//! again. As the Slint client's ThinkBlock (ui/chat.slint).
use crate::LogView;
use crundi_core::{chat::Shown, think::{Think, LINGER_MS}};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::time::Duration;

#[derive(Default)]
pub struct State {
    /// When each block is open (the reader's choice, the rules').
    pub think: Think,
    /// Blocks seen streaming (and not yet seen finished).
    pub live: HashSet<String>,
}

/// What changed for the rules: blocks that started streaming now, and
/// blocks that were streaming and are finished now (their linger starts).
pub fn follow(st: &mut State, shown: &[Shown]) -> Vec<String> {
    let mut done = vec![];
    for e in shown.iter().filter(|e| e.kind == "thinking") {
        let streaming = e.status == "streaming";
        if streaming && st.live.insert(e.id.clone()) { st.think.started(&e.id); }
        if !streaming && st.live.remove(&e.id) { done.push(e.id.clone()); }
    }
    done
}

/// The conversation changed: a block that started or finished streaming
/// (a finished one folds LINGER_MS later, unless the reader chose).
pub fn changed(v: &mut LogView, cx: &mut Context<LogView>) {
    let shown = v.shown(cx);
    for id in follow(&mut v.st.thinking, &shown) {
        cx.spawn(async move |this: WeakEntity<LogView>, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(LINGER_MS)).await;
            let _ = this.update(cx, |v, cx| { v.st.thinking.think.linger_over(&id); cx.notify(); });
        }).detach();
    }
}

/// This feature's fields in the chat's props (its rows are in "tools", tools.rs).
pub fn props(_v: &LogView, _cx: &App, _o: &mut Map<String, Value>) {}

/// This feature's driver actions (its heads are tool-toggle's, tools.rs).
pub fn act(_name: &str, _args: &[String], _v: &mut LogView, _window: &mut Window, _cx: &mut Context<LogView>) -> Option<Value> { None }

/// A thinking block: with no text, the "Thought" line; else a bar at the
/// left, the head (caret, "Thinking", shimmering while live) and, open, its
/// text (muted italics, at most a band tall, scrolling; a caret while live).
pub fn row(v: &mut LogView, e: &Shown, open: bool, cx: &mut Context<LogView>) -> AnyElement {
    let fs = crate::prose::fs(v, cx);
    let size = T::TEXT_12 * fs;
    if e.text.is_empty() {
        return div().flex().items_center().gap(T::SP_6)
            .child(w::dot(T::text_muted()).size(T::SP_6))
            .child(div().italic().text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_muted()).child(e.detail.clone()))
            .into_any_element();
    }
    let live = e.status == "streaming";
    let id = e.id.clone();
    let label: AnyElement = if live { w::shimmer(SharedString::from(format!("thl-{}", e.id)), "Thinking").text_color(T::text_secondary()).into_any_element() }
        else { div().child("Thinking").into_any_element() };
    let head = div().id(SharedString::from(format!("thh-{}", e.id))).flex().items_center().gap(T::SP_6).cursor_pointer()
        .text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_secondary())
        .child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM * fs))
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| this.toggle(&id, cx)));
    let text = if live { format!("{} ▎", e.text) } else { e.text.clone() };
    let body = div().id(SharedString::from(format!("thb-{}", e.id))).mt(T::SP_4)
        .max_h(T::COMPOSER_MAX + T::COMPOSER_MAX / 2.).overflow_y_scroll()
        .italic().text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_muted())
        .child(text);
    div().flex()
        .child(div().flex_none().w(T::SP_2).bg(T::border()))
        .child(div().flex().flex_col().flex_1().min_w_0().pl(T::SP_10 - T::SP_2).child(head).when(open, |d| d.child(body)))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn th(id: &str, live: bool) -> Shown { Shown { id: id.into(), kind: "thinking", text: "x".into(), status: if live { "streaming".into() } else { String::new() }, ..Default::default() } }

    #[::core::prelude::v1::test]
    fn started_and_finished() {
        let mut st = State::default();
        // First seen not streaming: folded, no linger.
        assert!(follow(&mut st, &[th("h", false)]).is_empty());
        assert!(!st.think.open("h"));
        // Streaming: open; a delta (still streaming) changes nothing.
        follow(&mut st, &[th("a", true)]);
        assert!(st.think.open("a"));
        assert!(follow(&mut st, &[th("a", true)]).is_empty());
        // A newer block folds it at once.
        follow(&mut st, &[th("a", true), th("b", true)]);
        assert!(!st.think.open("a") && st.think.open("b"));
        // Finished: still open, its linger starts once.
        assert_eq!(follow(&mut st, &[th("a", true), th("b", false)]), vec!["b".to_string()]);
        assert!(st.think.open("b"));
        assert!(follow(&mut st, &[th("a", true), th("b", false)]).is_empty());
    }
}
