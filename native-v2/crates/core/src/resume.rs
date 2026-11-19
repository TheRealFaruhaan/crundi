//! The resume picker (2.22, specs/resume-picker.md) and the parked card's
//! words (2.23, specs/parked-panes.md): the earlier conversations a launch
//! can open, their rows' words, the create call's body, and a parked pane's
//! why, what and warning. As the web's openSessionChooser, renderChatResume,
//! renderHeavyChoice, relTime, launchTerminal and parkedChoicesHtml. Pure.

use serde_json::{json, Value};

/// A conversation this heavy offers compacting first (the server's HEAVY_TOKENS).
pub const HEAVY_TOKENS: u64 = 100_000;

/// One earlier conversation in the list.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Earlier {
    pub id: String,
    pub title: String,
    /// The meta line's plain part: "id8 · age · 38.1k tokens" (or KB).
    pub meta: String,
    pub tokens: u64,
    pub heavy: bool,
    pub in_use: bool,
}

fn st(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// "just now", "5m ago", "2h ago", "3d ago", else the local date.
pub fn rel_time(then: f64, now: f64) -> String {
    if then <= 0.0 { return String::new(); }
    let s = (now - then).max(0.0);
    if s < 60.0 { "just now".into() }
    else if s < 3600.0 { format!("{}m ago", (s / 60.0) as u64) }
    else if s < 86400.0 { format!("{}h ago", (s / 3600.0) as u64) }
    else if s < 2_592_000.0 { format!("{}d ago", (s / 86400.0) as u64) }
    else {
        use chrono::TimeZone;
        match chrono::Local.timestamp_opt(then as i64, 0).single() {
            // en-US's toLocaleDateString, as the web on this server shows it.
            Some(d) => d.format("%-m/%-d/%Y").to_string(),
            None => String::new(),
        }
    }
}

/// "138.2k tokens".
pub fn tokens_k(tokens: u64) -> String { format!("{:.1}k tokens", tokens as f64 / 1000.0) }

/// The list's rows from `GET /api/ui-sessions/resumable` (`now` in seconds).
pub fn rows(v: &Value, now: f64) -> Vec<Earlier> {
    v.get("sessions").and_then(|x| x.as_array()).map(|list| list.iter().map(|c| {
        let id = st(c, "id");
        let tokens = c.get("tokens").and_then(|x| x.as_f64()).unwrap_or(0.0).max(0.0) as u64;
        let size = c.get("sizeBytes").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let then = crate::time::parse_iso(&st(c, "updatedAt")).unwrap_or(0.0);
        let amount = if tokens > 0 { tokens_k(tokens) } else { format!("{} KB", ((size / 1024.0).round() as u64).max(1)) };
        let title = st(c, "title");
        Earlier {
            meta: format!("{} · {} · {}", id.chars().take(8).collect::<String>(), rel_time(then, now), amount),
            title: if title.is_empty() { "(untitled session)".into() } else { title },
            id, tokens,
            heavy: tokens >= HEAVY_TOKENS,
            in_use: c.get("inUse").and_then(|x| x.as_bool()).unwrap_or(false),
        }
    }).collect()).unwrap_or_default()
}

/// The dialog's heading: "Open UI Mode" / "Open UI Mode — Skip Permissions".
pub fn heading(skip: bool) -> String {
    if skip { "Open UI Mode — Skip Permissions".into() } else { "Open UI Mode".into() }
}

/// The heavy choice's intro after the title.
pub fn heavy_intro(tokens: u64) -> String {
    format!("{} — continuing loads all of it into every turn. Compacting loads it once, then summarises it so later turns stay small.", tokens_k(tokens))
}

/// `POST /api/ui-sessions/create`'s body: `mode` new | resume | compact.
pub fn create_body(project: &str, skip: bool, mode: &str, resume_id: &str) -> Value {
    let mut b = json!({ "project": project });
    if skip { b["skipPermissions"] = json!(true); b["title"] = json!("Chat (skip perms)"); }
    b["sessionMode"] = json!(mode);
    if !resume_id.is_empty() && mode != "new" { b["resumeId"] = json!(resume_id); }
    match mode {
        "compact" => b["title"] = json!("Chat (compacted)"),
        "new" => b["title"] = json!("Chat (new)"),
        _ => {}
    }
    b
}

/// A parked pane's lines (2.23): why it was closed (with when), what Resume
/// does, and the warning's tokens.
pub fn park_why(reason: &str, parked_at_ms: f64, now: f64) -> String {
    let why = match reason { "idle" => "Closed while idle", "manual" => "Closed", _ => "Closed when Crundi restarted" };
    let t = rel_time(parked_at_ms / 1000.0, now);
    if t.is_empty() { why.into() } else { format!("{why} · {t}") }
}

pub fn park_what(has_session: bool) -> &'static str {
    if has_session { "Resume picks up this conversation." } else { "Resume starts a new conversation." }
}

/// The pressed choice's label while it runs.
pub fn park_busy_label(mode: &str) -> &'static str {
    match mode { "new" => "Starting…", "compact" => "Compacting…", _ => "Resuming…" }
}

/// The picker's error: the reason alone (the line says "Could not load
/// earlier conversations: " before it, as the web's).
pub fn list_error(e: &str) -> String {
    e.strip_prefix("Cannot reach the server: ").unwrap_or(e).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        let now = 2_000_000_000.0;
        assert_eq!(rel_time(now - 5.0, now), "just now");
        assert_eq!(rel_time(now - 125.0, now), "2m ago");
        assert_eq!(rel_time(now - 7300.0, now), "2h ago");
        assert_eq!(rel_time(now - 3.0 * 86400.0, now), "3d ago");
        assert!(rel_time(now - 40.0 * 86400.0, now).contains('/'));
        assert_eq!(rel_time(0.0, now), "");
    }

    #[test]
    fn list_rows() {
        let v = serde_json::json!({ "ok": true, "sessions": [
            { "id": "aaaa1111-2222", "title": "Refactor the billing", "updatedAt": "2033-05-18T01:33:20.000Z", "sizeBytes": 900000, "tokens": 138200, "inUse": false },
            { "id": "f3a8c698-x", "title": "", "updatedAt": "2033-05-18T03:33:10.000Z", "sizeBytes": 300, "tokens": 0, "inUse": true },
        ] });
        let now = 2_000_000_000.0; // 2033-05-18T03:33:20Z
        let r = rows(&v, now);
        assert_eq!(r[0].meta, "aaaa1111 · 2h ago · 138.2k tokens");
        assert!(r[0].heavy && !r[0].in_use);
        assert_eq!(r[1].meta, "f3a8c698 · just now · 1 KB");
        assert_eq!(r[1].title, "(untitled session)");
        assert!(r[1].in_use && !r[1].heavy);
        assert!(rows(&serde_json::json!({}), now).is_empty());
    }

    #[test]
    fn bodies() {
        assert_eq!(create_body("demo", false, "new", ""), json!({ "project": "demo", "sessionMode": "new", "title": "Chat (new)" }));
        assert_eq!(create_body("demo", false, "resume", "abc"), json!({ "project": "demo", "sessionMode": "resume", "resumeId": "abc" }));
        assert_eq!(create_body("demo", false, "compact", "abc"), json!({ "project": "demo", "sessionMode": "compact", "resumeId": "abc", "title": "Chat (compacted)" }));
        assert_eq!(create_body("demo", true, "resume", "abc"), json!({ "project": "demo", "skipPermissions": true, "title": "Chat (skip perms)", "sessionMode": "resume", "resumeId": "abc" }));
        assert_eq!(create_body("demo", true, "new", "")["title"], "Chat (new)");
        assert_eq!(heading(true), "Open UI Mode — Skip Permissions");
        assert!(heavy_intro(138200).starts_with("138.2k tokens — continuing"));
    }

    #[test]
    fn parked_words() {
        let now = 2_000_000_000.0;
        assert_eq!(park_why("manual", now * 1000.0 - 3000.0, now), "Closed · just now");
        assert_eq!(park_why("idle", now * 1000.0 - 600_000.0, now), "Closed while idle · 10m ago");
        assert_eq!(park_why("restart", 0.0, now), "Closed when Crundi restarted");
        assert_eq!(park_what(false), "Resume starts a new conversation.");
        assert_eq!(park_busy_label("new"), "Starting…");
        assert_eq!(park_busy_label("compact"), "Compacting…");
        assert_eq!(park_busy_label("resume"), "Resuming…");
        assert_eq!(list_error("Cannot reach the server: Failed to fetch"), "Failed to fetch");
        assert_eq!(list_error("Unknown project"), "Unknown project");
    }
}
