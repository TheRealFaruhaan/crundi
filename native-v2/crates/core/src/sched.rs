//! Scheduled messages (2.10, specs/scheduled-messages.md): the form's rules
//! (its default time, what each trigger says, what Schedule checks) and how
//! the list reads. Pure; the app (src/app.rs) talks to the server.

use chrono::{DateTime, Duration, Local, NaiveDateTime, SecondsFormat, TimeZone, Timelike, Utc};
use serde_json::Value;

/// The time field's format (as the web's datetime-local shows it, en-US).
const FIELD: &str = "%m/%d/%Y, %I:%M %p";

/// The time field when the panel opens: now + 30 minutes, seconds dropped.
pub fn default_at(now: DateTime<Local>) -> String {
    let t = now + Duration::minutes(30);
    t.with_second(0).unwrap_or(t).format(FIELD).to_string()
}

/// The field's text as a moment (local time); None when it can't be read.
pub fn parse_at(text: &str) -> Option<DateTime<Local>> {
    let t = text.trim();
    for f in [FIELD, "%m/%d/%Y %I:%M %p", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(t, f) { return Local.from_local_datetime(&n).earliest(); }
    }
    None
}

/// A date and time picked in the two popups, as the field's text.
pub fn picked(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Option<String> {
    let n = chrono::NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, 0)?;
    Some(Local.from_local_datetime(&n).earliest()?.format(FIELD).to_string())
}

/// The note a trigger shows while it is chosen.
pub fn trigger_note(trigger: &str) -> &'static str {
    match trigger {
        "turn-end" => "Sends as soon as the current turn finishes. If nothing is running, it waits for the next turn to end.",
        "limit-reset" => "Sends when the rolling 5-hour usage window rolls over.",
        _ => "",
    }
}

/// Schedule pressed: the request's trigger, or the note that says why not.
pub fn check(text: &str, trigger: &str, at: &str) -> Result<Value, &'static str> {
    if text.trim().is_empty() { return Err("Write a message first."); }
    if trigger == "time" {
        let Some(t) = parse_at(at) else { return Err("Pick a time.") };
        return Ok(serde_json::json!({ "type": "time", "at": t.with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Millis, true) }));
    }
    Ok(serde_json::json!({ "type": trigger }))
}

/// An item's when-label (the web's trigLabel): the time in local
/// toLocaleString form, or what it waits for.
pub fn when_label(item: &Value) -> String {
    let t = item.pointer("/trigger/type").and_then(|v| v.as_str()).unwrap_or("");
    match t {
        "time" => item.pointer("/trigger/at").and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Local).format("%-m/%-d/%Y, %-I:%M:%S %p").to_string())
            .unwrap_or_default(),
        "turn-end" => "when the turn ends".into(),
        "limit-reset" => "when the 5h window resets".into(),
        o => o.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form() {
        let now = Local.with_ymd_and_hms(2026, 11, 14, 11, 21, 40).unwrap();
        assert_eq!(default_at(now), "11/14/2026, 11:51 AM");
        assert_eq!(parse_at("11/14/2026, 3:30 PM").map(|d| (d.hour(), d.minute())), Some((15, 30)));
        assert_eq!(parse_at("11/14/2026, 03:30 PM").map(|d| d.hour()), Some(15));
        assert!(parse_at("").is_none() && parse_at("soon").is_none());
        assert_eq!(picked(2026, 11, 15, 9, 0).as_deref(), Some("11/15/2026, 09:00 AM"));
        assert_eq!(check("  ", "time", "11/14/2026, 3:30 PM"), Err("Write a message first."));
        assert_eq!(check("hi", "time", ""), Err("Pick a time."));
        assert_eq!(check("hi", "turn-end", ""), Ok(serde_json::json!({ "type": "turn-end" })));
        let t = check("hi", "time", "11/14/2026, 3:30 PM").unwrap();
        assert!(t["at"].as_str().unwrap().ends_with(":30:00.000Z"));
        assert_eq!(trigger_note("time"), "");
        assert!(trigger_note("limit-reset").starts_with("Sends when the rolling 5-hour"));
    }

    #[test]
    fn list_labels() {
        assert_eq!(when_label(&serde_json::json!({ "trigger": { "type": "turn-end" } })), "when the turn ends");
        assert_eq!(when_label(&serde_json::json!({ "trigger": { "type": "limit-reset" } })), "when the 5h window resets");
        assert_eq!(when_label(&serde_json::json!({ "trigger": { "type": "later" } })), "later");
        let at = Local.with_ymd_and_hms(2026, 11, 14, 15, 30, 0).unwrap().with_timezone(&Utc).to_rfc3339();
        assert_eq!(when_label(&serde_json::json!({ "trigger": { "type": "time", "at": at } })), "11/14/2026, 3:30:00 PM");
    }
}
