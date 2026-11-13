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

/// The long title is shown only on a wide header (≥ 560px) and only when the
/// chat has one; it is capped at min(440, 62% of the header). The short one
/// at 160px.
pub fn title_shown(title: &str, long: &str, head_w: f32) -> (String, f32) {
    if head_w >= 560.0 && !long.is_empty() { (long.to_string(), 440f32.min(head_w * 0.62)) } else { (title.to_string(), 160.0) }
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
    fn titles() {
        assert_eq!(title_shown("Fruit list", "Fruit list and directory listing", 1146.0), ("Fruit list and directory listing".into(), 440.0));
        assert_eq!(title_shown("Fruit list", "Fruit list and directory listing", 600.0).1, 372.0);
        assert_eq!(title_shown("Fruit list", "Fruit list and directory listing", 500.0), ("Fruit list".into(), 160.0));
        assert_eq!(title_shown("Typed", "", 1146.0), ("Typed".into(), 160.0));
        assert_eq!(close_refused(""), "This chat cannot be closed right now.");
    }
}
