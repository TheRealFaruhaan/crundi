//! The turn's activity row (the web's .cc-activity, claude-chat.js
//! syncActivity): while Claude works, a spinner and "Working…" (or
//! "Compacting the conversation… this can take a while on a long session"
//! after a /compact) pinned at the log's end, under the newest row, so a
//! turn that streams nothing for a while (a /compact, a long think) never
//! looks frozen. Mini's Working scene goes and points at it (`spot`).
//!
//! Its spinner turns at T::ANIM_FPS, and a frame of it must not redraw the
//! log (ARCHITECTURE.md rules 3 and 6; a view's notify redraws its
//! ancestors). So, as the streaming tail (tail.rs), while the log follows
//! its end and its rows reach the foot, the row is drawn by a view of its
//! own (ActivityView), a sibling of the LogView in the chat's body, over a
//! room of its height the list keeps under its last row. Otherwise (a
//! short log whose rows sit at its top, a reader up the log, a second log)
//! the row is drawn in the list under the last row, as the web's.

use super::LogView;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::*;
use serde_json::Value;

pub const WORKING: &str = "Working…";
pub const COMPACTING: &str = "Compacting the conversation… this can take a while on a long session";

/// The row's height at the pane's text scale (12px italic and padding 2 0).
pub fn height(fs: f32) -> Pixels { px((22. * fs).round()) }

/// The words: after the newest message, if it was a /compact (looking back
/// past nothing Claude wrote or ran since).
pub fn label(entries: &[Value]) -> &'static str {
    for e in entries.iter().rev() {
        match e.get("kind").and_then(|k| k.as_str()) {
            Some("user") => {
                let t = e.get("text").and_then(|t| t.as_str()).unwrap_or("");
                return if t.trim().starts_with("/compact") { COMPACTING } else { WORKING };
            }
            Some("assistant-text" | "tool") => break,
            _ => {}
        }
    }
    WORKING
}

/// Whether the chat is working (the web's state === 'working', set at a send).
pub(crate) fn shown(v: &LogView, cx: &App) -> bool {
    let m = &v.store.read(cx).meta.read(cx).0;
    m.state == "working" || m.sending
}

/// The row: the spinner and the words, at the log's padding.
pub(crate) fn row(v: &LogView, cx: &mut App) -> AnyElement {
    let fs = v.fs();
    let label = label(&v.store.read(cx).conv.entries);
    let id = v.chat(cx);
    let spots = v.spots.then(|| {
        let g = format!("act:{id}");
        (crundi_shell::spots::spot(cx, &g, format!("activity:{id}")), crundi_shell::spots::spot(cx, &g, format!("activity-w:{id}")))
    });
    let (sr, sw) = match spots { Some((a, b)) => (Some(a), Some(b)), None => (None, None) };
    div().id("cc-activity").relative().w_full().h(height(fs)).px(T::SP_16).flex().items_center().gap(T::SP_8)
        .text_size(T::TEXT_12 * fs).italic().text_color(T::text_muted())
        .child(w::spinner(px(11.) * fs))
        .child(div().relative().child(label).children(sw))
        .children(sr)
        .into_any_element()
}

/// The activity row's spots begin again (the LogView's render and the
/// ActivityView's: whichever draws it notes it; when neither does, it is gone).
pub(crate) fn begin(v: &LogView, cx: &mut App) {
    if v.spots { let g = format!("act:{}", v.chat(cx)); crundi_shell::spots::begin(cx, &g); }
}

impl LogView {
    /// The activity row while a turn runs: Some(true) drawn by its own view
    /// at the log's foot, Some(false) in the list under the last row.
    pub(crate) fn activity(&self, cx: &App) -> Option<bool> {
        if !shown(self, cx) { return None; }
        let rows = self.store.read(cx).rows.len();
        Some(rows > 0 && self.tail.watched && !self.tail.boxed && self.tail.act_view.is_some()
            && self.list.is_following_tail() && self.tail.at_foot())
    }

    /// For the chat's body: the activity's view and its height, while it
    /// draws itself (its bottom is the list's room under it: the dock's).
    pub fn activity_place(&self, cx: &App) -> Option<(Entity<ActivityView>, Pixels, Pixels)> {
        if self.activity(cx) != Some(true) { return None; }
        Some((self.tail.act_view.clone()?, height(self.fs()), self.tail.dock_room))
    }
}

/// The activity row's view (cached by the chat's body at the log's foot).
pub struct ActivityView { pub(crate) log: WeakEntity<LogView>, pub(crate) _sub: Subscription }

impl Render for ActivityView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("log-activity");
        let el = self.log.upgrade().and_then(|l| l.update(cx, |l, cx| {
            if l.activity(cx) != Some(true) { return None; }
            begin(l, cx);
            Some(row(l, cx))
        }));
        div().size_full().children(el)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[::core::prelude::v1::test]
    fn the_words_follow_the_newest_message() {
        assert_eq!(label(&[]), WORKING);
        assert_eq!(label(&[json!({ "kind": "user", "text": "hi" })]), WORKING);
        assert_eq!(label(&[json!({ "kind": "user", "text": "  /compact keep the plan" })]), COMPACTING);
        // Something Claude wrote since: the /compact is over.
        assert_eq!(label(&[json!({ "kind": "user", "text": "/compact" }), json!({ "kind": "assistant-text", "text": "ok" })]), WORKING);
        // A notice in between does not count.
        assert_eq!(label(&[json!({ "kind": "user", "text": "/compact" }), json!({ "kind": "notice", "text": "…" })]), COMPACTING);
    }
}
