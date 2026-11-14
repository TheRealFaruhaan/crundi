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

// The header's layout numbers, as ui/theme.slint has them (tokens of the same
// name): the fold (pane-fold), the short title's cap (title-cap), the long
// form's cap; and the header's box around its content (the pane's 1px edge on
// each side, padding 2 left and 4 right).
pub const PANE_FOLD: f32 = 560.0;
pub const TITLE_CAP: f32 = 160.0;
pub const LONG_CAP: f32 = 440.0;
pub const LONG_SHARE: f32 = 0.62;
pub const HEAD_INSET: f32 = 8.0;

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

/// A project's initials for the folded rail (the web's projectInitials):
/// the first letters of the first two words (split on spaces, `_`, `.`
/// and `-`), or a one-word name's first two letters; only ASCII letters
/// and digits are kept, topped up from the whole name when short; "?" if none.
pub fn project_initials(name: &str) -> String {
    let alnum = |t: &str| t.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();
    let parts: Vec<&str> = name.split(|c: char| c.is_whitespace() || c == '_' || c == '.' || c == '-').filter(|w| !w.is_empty()).collect();
    let mut ini = match parts.len() {
        0 => String::new(),
        1 => parts[0].chars().take(2).collect(),
        _ => parts[0].chars().take(1).chain(parts[1].chars().take(1)).collect(),
    };
    ini = alnum(&ini);
    if ini.chars().count() < 2 {
        let an = alnum(name);
        if !an.is_empty() { ini = an.chars().take(2).collect(); }
    }
    if ini.is_empty() { "?".into() } else { ini.to_uppercase() }
}

/// The header's content box: the pane less its 1px border each side and the
/// header's padding (2 left, 4 right). The web's header is a size container,
/// so its rules (@container, 62cqw) read this.
pub fn head_content(pane_w: f32) -> f32 {
    pane_w - HEAD_INSET
}

/// The header folds its tools into "…" when its content box is at most
/// 559px (the web's @container (max-width: 559px)): a pane of 567px is
/// narrow, 568px wide (specs/pane-header-narrow.md).
pub fn narrow(pane_w: f32) -> bool {
    head_content(pane_w) <= PANE_FOLD - 1.0
}

/// The long title needs a content box of at least 560px (@container
/// (min-width: 560px)). Between 559 and 560 (a fractional pane) neither rule
/// holds: the web shows the tools inline with the short title.
fn long_fits(pane_w: f32) -> bool {
    head_content(pane_w) >= PANE_FOLD
}

/// No cap: the title takes what the row leaves it.
/// A chat's permission modes, in the picker's order: Bypass all first, and
/// only for a chat launched with skip permissions (the web's buildModes).
pub fn modes(skip: bool) -> Vec<&'static str> {
    let mut m = vec!["default", "acceptEdits", "auto", "plan", "dontAsk"];
    if skip { m.insert(0, "bypassPermissions"); }
    m
}

/// A mode's name and description, as the web chat's MODE_INFO.
pub fn mode_info(m: &str) -> (&'static str, &'static str) {
    match m {
        "acceptEdits" => ("Accept edits", "Edits files without asking. Still asks before running commands."),
        "auto" => ("Auto", "Decides for itself what is safe to do, and asks about the rest."),
        "plan" => ("Plan mode", "Reads and plans only. Changes nothing until you approve the plan."),
        "dontAsk" => ("Don't ask", "Never asks. Anything not already allowed is refused."),
        "bypassPermissions" => ("Bypass all", "Runs everything without asking. Only for a chat launched with skip permissions."),
        _ => ("Ask permissions", "Asks you before editing files or running commands."),
    }
}

/// The mode a chat shows: its own when in its list, otherwise Ask
/// permissions (a snapshot is taken as it comes).
pub fn shown_mode(m: &str, skip: bool) -> &'static str {
    modes(skip).into_iter().find(|x| *x == m).unwrap_or("default")
}

/// What a refused or failed mode change says (a toast).
pub fn mode_error(e: &str) -> String {
    if e == "Not supported by this server" { e.into() } else { format!("Could not switch permission mode: {e}") }
}

/// A chat's state as its pane shows it: the agent's (idle when unknown),
/// or working at once after a send while idle or waiting (the web's
/// optimistic switch), until the server says otherwise. The header keeps
/// "waiting" (its badge); the composer reads it as idle.
pub fn composer_state(state: &str, sending: bool) -> String {
    let st = if state.is_empty() { "idle" } else { state };
    if sending && matches!(st, "idle" | "waiting") { "working".into() } else { st.into() }
}

pub const NO_CAP: f32 = 100_000.0;

/// The title shown and its cap. Wide (content box ≥ 560px): the long form
/// when there is one, at most min(440, 62% of the content box); else the short
/// one at 160px. Narrow: the short one with no cap (it elides when the row
/// runs out).
pub fn title_shown(title: &str, long: &str, pane_w: f32) -> (String, f32) {
    let content = head_content(pane_w);
    if narrow(pane_w) { return (title.to_string(), NO_CAP); }
    if !long.is_empty() && long_fits(pane_w) { (long.to_string(), LONG_CAP.min(content * LONG_SHARE)) } else { (title.to_string(), TITLE_CAP) }
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
    fn initials_as_the_web() {
        for (name, want) in [
            ("Demo", "DE"), ("Other", "OT"), ("Third", "TH"), ("Fourth", "FO"), ("Fifth", "FI"),
            ("crundi factory", "CF"), ("my-project", "MP"), ("a_b.c", "AB"), ("x", "X"),
            ("é chat", "CH"), ("!!", "?"), ("", "?"), ("  web  ui ", "WU"), ("1st", "1S"),
        ] {
            assert_eq!(project_initials(name), want, "{name}");
        }
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

    #[test]
    fn composer_states() {
        assert_eq!(composer_state("", false), "idle");
        assert_eq!(composer_state("idle", true), "working");
        assert_eq!(composer_state("waiting", true), "working");
        assert_eq!(composer_state("waiting", false), "waiting");
        assert_eq!(composer_state("needs-input", true), "needs-input");
        assert_eq!(composer_state("exited", true), "exited");
    }

    #[test]
    fn permission_modes() {
        assert_eq!(modes(false), vec!["default", "acceptEdits", "auto", "plan", "dontAsk"]);
        assert_eq!(modes(true)[0], "bypassPermissions");
        assert_eq!(modes(true).len(), 6);
        assert_eq!(mode_info("bypassPermissions").0, "Bypass all");
        assert_eq!(mode_info("").0, "Ask permissions");
        assert_eq!(shown_mode("bypassPermissions", false), "default");
        assert_eq!(shown_mode("plan", false), "plan");
        assert_eq!(shown_mode("", true), "default");
        assert_eq!(mode_error("No session \"x\""), "Could not switch permission mode: No session \"x\"");
        assert_eq!(mode_error("Not supported by this server"), "Not supported by this server");
    }
}
