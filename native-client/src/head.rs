//! The pane header's rules (specs/pane-header-wide.md), as pure functions:
//! renaming, the text size, the secrets lock's count and words, and what a
//! refused close says. The web UI's versions are in src/webapp-html.js
//! (renameTerminal, setTermFont, paintChatSecretBtns, closeTerminal).

use crate::api::ApiError;
use serde_json::Value;

/// The chat text size the web starts at, and its limits.
pub const FONT_BASE: i32 = 13;
pub const FONT_MIN: i32 = 9;
pub const FONT_MAX: i32 = 24;

/// What a rename saves: trimmed, "Terminal" when empty; None when nothing
/// changed (nothing is sent).
pub fn rename_value(input: &str, current: &str) -> Option<String> {
    let t = input.trim();
    let t = if t.is_empty() { "Terminal" } else { t };
    if t == current { None } else { Some(t.to_string()) }
}

/// A- / A+: one px smaller or larger, within 9…24.
pub fn font_step(px: i32, delta: i32) -> i32 {
    (px + delta).clamp(FONT_MIN, FONT_MAX)
}

/// The scale the chat's text is drawn at (the web's --cc-fs).
pub fn font_scale(px: i32) -> f32 {
    px as f32 / FONT_BASE as f32
}

/// How many of a chat's grants are still good at `now` (seconds since 1970):
/// one with `expiresAt` in the past has run out; the rest (no expiry, or
/// `untilChatEnds`) count.
pub fn grant_count(grants: Option<&Value>, now: f64) -> usize {
    let Some(list) = grants.and_then(|g| g.as_array()) else { return 0 };
    list.iter()
        .filter(|g| match g.get("expiresAt").and_then(|e| e.as_str()) {
            Some(at) if !at.is_empty() => crate::app::parse_iso(at).map(|t| t > now).unwrap_or(true),
            _ => true,
        })
        .count()
}

/// The lock's tooltip for this many grants.
pub fn secrets_tip(n: usize) -> String {
    match n {
        0 => "Pre-authorise secrets for this chat".into(),
        1 => "1 secret pre-authorised for this chat".into(),
        n => format!("{n} secrets pre-authorised for this chat"),
    }
}

/// The toast when the server refuses to close a chat (409: a factory
/// Manager's chat while its run is going).
pub fn close_refused(msg: &str) -> String {
    if msg.is_empty() { "This chat cannot be closed right now.".into() } else { msg.to_string() }
}

/// What came of asking the server to close a chat.
#[derive(Debug, PartialEq)]
pub enum Closed {
    /// It closed: the pane goes.
    Yes,
    /// It didn't: the pane stays, and the toast says this.
    No(String),
    SignedOut,
}

/// The server's answer to POST /api/ui-sessions/:id/close, as the web UI
/// reads it (closeTerminal): 409 is a refusal with the server's words; any
/// other answer, even an error status, means it closed; no answer at all is
/// "Failed to close terminal: …".
pub fn close_outcome(r: Result<Value, ApiError>) -> Closed {
    match r {
        Ok(_) => Closed::Yes,
        Err(ApiError::SignedOut) => Closed::SignedOut,
        Err(ApiError::Status(409, m)) => Closed::No(close_refused(&m)),
        Err(ApiError::Status(_, _)) => Closed::Yes,
        Err(e) => Closed::No(format!("Failed to close terminal: {e}")),
    }
}

fn field<'a>(c: &'a Value, k: &str) -> &'a str { c.get(k).and_then(|v| v.as_str()).unwrap_or("") }

/// A chat's state, as the web draws it (2.4): `exited` wins over
/// everything; a pending card is needs input; else its agent state (idle
/// when it has none yet).
pub fn chat_state(c: &Value) -> String {
    if field(c, "status") == "exited" { return "exited".into(); }
    if c.get("pending").and_then(|p| p.as_array()).is_some_and(|p| !p.is_empty()) { return "needs-input".into(); }
    let st = field(c, "agentState");
    if st.is_empty() { "idle".into() } else { st.to_string() }
}

/// A project's dot (renderProjects), over its `running` chats and Claude
/// terminals: needs-input > working > waiting > running > none.
pub fn project_dot(list: &[Value], alias: &str) -> &'static str {
    let mine: Vec<String> = list.iter().filter(|c| field(c, "project") == alias && field(c, "status") == "running").map(chat_state).collect();
    if mine.iter().any(|x| x == "needs-input") { "needs-input" }
    else if mine.iter().any(|x| x == "working") { "working" }
    else if mine.iter().any(|x| x == "waiting") { "waiting" }
    else if !mine.is_empty() { "running" }
    else { "" }
}

/// The header's content box: the pane less its 1px border each side and the
/// header's padding (2 left, 4 right). The web's header is a size container,
/// so its rules (@container, 62cqw) read this.
pub fn head_content(pane_w: f32) -> f32 {
    pane_w - 8.0
}

/// The header folds its tools into "…" when its content box is at most
/// 559px (the web's @container (max-width: 559px)): a pane of 567px is
/// narrow, 568px wide (specs/pane-header-narrow.md).
pub fn narrow(pane_w: f32) -> bool {
    head_content(pane_w) <= 559.0
}

/// The long title needs a content box of at least 560px (@container
/// (min-width: 560px)). Between 559 and 560 (a fractional pane) neither rule
/// holds: the web shows the tools inline with the short title.
fn long_fits(pane_w: f32) -> bool {
    head_content(pane_w) >= 560.0
}

/// No cap: the title takes what the row leaves it.
pub const NO_CAP: f32 = 100_000.0;

/// The title shown and its cap. Wide (content box ≥ 560px): the long form
/// when there is one, at most min(440, 62% of the content box); else the short
/// one at 160px. Narrow: the short one with no cap (it elides when the row
/// runs out).
pub fn title_shown(title: &str, long: &str, pane_w: f32) -> (String, f32) {
    let content = head_content(pane_w);
    if narrow(pane_w) { return (title.to_string(), NO_CAP); }
    if !long.is_empty() && long_fits(pane_w) { (long.to_string(), 440f32.min(content * 0.62)) } else { (title.to_string(), 160.0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rename() {
        assert_eq!(rename_value("  New name ", "Old"), Some("New name".into()));
        assert_eq!(rename_value("   ", "Old"), Some("Terminal".into()));
        assert_eq!(rename_value("Old", "Old"), None);
        assert_eq!(rename_value("", "Terminal"), None);
    }

    #[test]
    fn font() {
        assert_eq!(font_step(13, 1), 14);
        assert_eq!(font_step(24, 1), 24);
        assert_eq!(font_step(9, -1), 9);
        assert_eq!(font_step(30, -1), 24);
        assert!((font_scale(26) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn grants() {
        let now = 1_800_000_000.0;
        let g = json!([
            { "secretId": "a", "expiresAt": null, "untilChatEnds": true },
            { "secretId": "b", "expiresAt": "2027-01-15T08:00:00.000Z" },
            { "secretId": "c", "expiresAt": "2020-01-01T00:00:00Z" },
        ]);
        let t = crate::app::parse_iso("2027-01-15T08:00:00.000Z").unwrap();
        assert_eq!(grant_count(Some(&g), t - 1.0), 2);
        assert_eq!(grant_count(Some(&g), t + 1.0), 1);
        assert_eq!(grant_count(None, now), 0);
        assert_eq!(secrets_tip(0), "Pre-authorise secrets for this chat");
        assert_eq!(secrets_tip(1), "1 secret pre-authorised for this chat");
        assert_eq!(secrets_tip(2), "2 secrets pre-authorised for this chat");
    }

    #[test]
    fn close_answers() {
        let msg = "This factory Manager's chat can't be closed while its run is going.";
        assert_eq!(close_outcome(Err(ApiError::Status(409, msg.into()))), Closed::No(msg.into()));
        assert_eq!(close_outcome(Err(ApiError::Status(409, String::new()))), Closed::No("This chat cannot be closed right now.".into()));
        assert_eq!(close_outcome(Ok(json!({ "ok": true }))), Closed::Yes);
        assert_eq!(close_outcome(Err(ApiError::Status(404, String::new()))), Closed::Yes);
        assert_eq!(close_outcome(Err(ApiError::SignedOut)), Closed::SignedOut);
        assert_eq!(close_outcome(Err(ApiError::Net("refused".into()))), Closed::No("Failed to close terminal: Cannot reach the server: refused".into()));
    }

    #[test]
    fn live_states() {
        // Exited wins over the last agent state; a pending card is needs input.
        assert_eq!(chat_state(&json!({ "status": "exited", "agentState": "idle" })), "exited");
        assert_eq!(chat_state(&json!({ "status": "running", "agentState": null })), "idle");
        assert_eq!(chat_state(&json!({ "status": "running", "agentState": "working", "pending": [{}] })), "needs-input");
        assert_eq!(chat_state(&json!({ "status": "running", "agentState": "waiting" })), "waiting");
        // The project dot: running ones only (not exited, parked or another
        // project's); needs-input > working > waiting > running.
        let list = vec![
            json!({ "project": "demo", "status": "running", "agentState": "waiting" }),
            json!({ "project": "demo", "status": "running", "agentState": "working" }),
            json!({ "project": "demo", "status": "exited", "agentState": "needs-input" }),
            json!({ "project": "demo", "status": "parked", "agentState": "needs-input" }),
            json!({ "project": "other", "status": "running", "agentState": "needs-input" }),
            json!({ "project": "idle", "status": "running", "agentState": "idle", "kind": "terminal" }),
        ];
        assert_eq!(project_dot(&list, "demo"), "working");
        assert_eq!(project_dot(&list, "other"), "needs-input");
        assert_eq!(project_dot(&list, "idle"), "running");
        assert_eq!(project_dot(&list, "none"), "");
        assert_eq!(project_dot(&list[..1], "demo"), "waiting");
    }

    #[test]
    fn titles() {
        // A 1148px pane: wide, the long form at 440.
        assert_eq!(title_shown("Fruit list", "Fruit list and directory listing", 1148.0), ("Fruit list and directory listing".into(), 440.0));
        // 62% of the content box (600 − 8).
        assert!((title_shown("Fruit list", "Fruit list and directory listing", 600.0).1 - 367.04).abs() < 0.01);
        // The edge: 567 is narrow (short form, no cap), 568 is wide.
        assert_eq!(title_shown("Fruit list", "Fruit list and directory listing", 567.0), ("Fruit list".into(), NO_CAP));
        assert_eq!(title_shown("Fruit list", "Fruit list and directory listing", 568.0).0, "Fruit list and directory listing");
        assert!(narrow(567.0) && !narrow(568.0));
        // Between the two rules (content 559–560): tools inline, short title.
        assert!(!narrow(567.5));
        assert_eq!(title_shown("Fruit list", "Fruit list and directory listing", 567.5), ("Fruit list".into(), 160.0));
        assert_eq!(title_shown("Typed", "", 1148.0), ("Typed".into(), 160.0));
        assert_eq!(title_shown("Fruit list and directory", "", 378.0), ("Fruit list and directory".into(), NO_CAP));
        assert_eq!(close_refused(""), "This chat cannot be closed right now.");
    }
}
