//! Schedules (4.5, 4.6): the pure rules of the web's schedule tab
//! (src/webapp-html.js: defaultSchedule, effectiveWhen, schedWhenSummary,
//! schedActionSummary, schedCondSummary, saveSchedule's checks), unit-tested.

use serde_json::{json, Value};

pub const DAYS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
pub const MODELS: [&str; 5] = ["opus", "sonnet", "haiku", "fable", "mythos"];
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
pub const OPS: [&str; 3] = ["below", "above", "equal"];
pub const TODO_STATUSES: [&str; 2] = ["done", "pending"];

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// A new schedule (the web's defaultSchedule()).
pub fn default_schedule(project: &str) -> Value {
    json!({
        "id": Value::Null, "name": "", "project": project, "enabled": true,
        "action": { "kind": "agent", "agent": "claude", "mode": "normal", "model": "", "effort": "", "session": "new", "sessionId": "", "prompt": "", "command": "", "serviceKey": "", "op": "start" },
        "when": { "mode": "recurring", "time": "09:00", "date": "", "days": [0, 1, 2, 3, 4, 5, 6] },
        "conditions": [],
        "webhook": { "auth": "key", "signatureHeader": "x-crundi-signature", "timestampHeader": "", "idempotency": "" },
    })
}

/// A schedule's time gate, migrating a legacy time-type condition (effectiveWhen).
pub fn effective_when(sch: &Value) -> Option<Value> {
    if let Some(w) = sch.get("when") {
        if !s(w, "time").is_empty() { return Some(w.clone()); }
    }
    let t = sch.get("conditions").and_then(|c| c.as_array()).and_then(|a| a.iter().find(|c| s(c, "type") == "time").cloned())?;
    let at = s(&t, "at");
    if at.is_empty() { return None; }
    Some(json!({ "mode": "recurring", "days": [0, 1, 2, 3, 4, 5, 6], "time": at }))
}

/// The when chip (schedWhenSummary).
pub fn when_summary(w: Option<&Value>) -> String {
    let Some(w) = w else { return "no time set".into() };
    if s(w, "mode") == "webhook" { return "When its webhook is called".into(); }
    let time = s(w, "time");
    if time.is_empty() { return "no time set".into(); }
    if s(w, "mode") == "once" {
        let d = s(w, "date");
        return format!("{} {}", if d.is_empty() { "(no date)".to_string() } else { d }, time);
    }
    let mut days: Vec<i64> = w.get("days").and_then(|d| d.as_array()).map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default();
    if days.is_empty() { return format!("no days \u{00b7} {time}"); }
    if days.len() == 7 { return format!("Daily {time}"); }
    days.sort();
    let names: Vec<&str> = days.iter().filter_map(|d| DAYS.get(*d as usize).copied()).collect();
    format!("{} {}", names.join(","), time)
}

/// The action's line (schedActionSummary); `services` are the GET's {key, name}.
pub fn action_summary(a: &Value, services: &[Value]) -> String {
    match s(a, "kind").as_str() {
        "agent" => {
            let mut bits = vec!["Claude".to_string(), if s(a, "mode") == "skip" { "skip-perms".into() } else { "normal".into() }];
            let m = s(a, "model");
            if !m.is_empty() { bits.push(m); }
            let e = s(a, "effort");
            if !e.is_empty() { bits.push(e); }
            bits.push(match s(a, "session").as_str() {
                "continue" => "continue".into(),
                "resume" => format!("resume {}", s(a, "sessionId").chars().take(8).collect::<String>()),
                _ => "new session".into(),
            });
            format!("Agent \u{00b7} {}", bits.join(" \u{00b7} "))
        }
        "command" => format!("Run: {}", s(a, "command")),
        "service" => {
            let key = s(a, "serviceKey");
            let name = services.iter().find(|x| s(x, "key") == key).map(|x| s(x, "name"))
                .unwrap_or_else(|| if key.is_empty() { "?".into() } else { key.clone() });
            let op = s(a, "op");
            format!("{} service \u{00b7} {}", if op.is_empty() { "start".into() } else { op }, name)
        }
        // The web has no line for a chat job (schedActionSummary returns '').
        _ => String::new(),
    }
}

/// An extra condition's chip (schedCondSummary).
pub fn cond_summary(c: &Value) -> String {
    let op = { let o = s(c, "op"); if o.is_empty() { "below".to_string() } else { o } };
    let val = { let v = s(c, "value"); if v.is_empty() { "0".to_string() } else { v } };
    match s(c, "type").as_str() {
        "time" => format!("at {}", { let a = s(c, "at"); if a.is_empty() { "?".into() } else { a } }),
        "usage" => format!("{} usage {} {}%", if s(c, "metric") == "week" { "weekly" } else { "5h" }, op, val),
        "terminals" => format!("open terminals {op} {val}"),
        "kanban" => format!("kanban {} = {}", if s(c, "todoId").is_empty() { "task" } else { "subtask" }, { let st = s(c, "status"); if st.is_empty() { "?".into() } else { st } }),
        _ => String::new(),
    }
}

/// A new extra condition (the web's add-cond defaults).
pub fn new_cond(kind: &str, first_status: &str) -> Value {
    match kind {
        "usage" => json!({ "type": "usage", "metric": "5h", "op": "below", "value": 50 }),
        "usage-week" => json!({ "type": "usage", "metric": "week", "op": "below", "value": 50 }),
        "terminals" => json!({ "type": "terminals", "op": "below", "value": 1 }),
        _ => json!({ "type": "kanban", "taskId": "", "todoId": "", "status": if first_status.is_empty() { "todo" } else { first_status } }),
    }
}

/// "9:5", "09:05", "0905" → "09:05"; anything else "" (the web's time input
/// only ever gives HH:MM or nothing).
pub fn norm_time(t: &str) -> String {
    let t = t.trim();
    let (h, m) = if let Some((h, m)) = t.split_once(':') { (h, m) } else if t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()) { t.split_at(2) } else { return String::new() };
    match (h.trim().parse::<u32>(), m.trim().parse::<u32>()) {
        (Ok(h), Ok(m)) if h < 24 && m < 60 => format!("{h:02}:{m:02}"),
        _ => String::new(),
    }
}

/// "2026-11-20" (and "2026-11-5") → "2026-11-20"; anything else "" (the date input).
pub fn norm_date(d: &str) -> String {
    let p: Vec<&str> = d.trim().split('-').collect();
    if p.len() != 3 { return String::new(); }
    match (p[0].parse::<i32>(), p[1].parse::<u32>(), p[2].parse::<u32>()) {
        (Ok(y), Ok(m), Ok(dd)) if p[0].len() == 4 && (1..=12).contains(&m) && dd >= 1 && dd <= month_days(y, m) => format!("{y:04}-{m:02}-{dd:02}"),
        _ => String::new(),
    }
}

fn month_days(y: i32, m: u32) -> u32 {
    match m {
        2 => if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The board's known columns (the Slint client's board.rs STATUS_LABELS).
pub const STATUS_LABELS: [(&str, &str); 4] = [("backlog", "Backlog"), ("todo", "To Do"), ("in_progress", "In Progress"), ("done", "Done")];

/// A status's label (the key itself when unknown).
pub fn label(st: &str) -> String {
    STATUS_LABELS.iter().find(|(k, _)| *k == st).map(|(_, l)| l.to_string()).unwrap_or_else(|| st.to_string())
}

/// The board's columns' statuses (the server's, or the four known ones).
pub fn statuses(board: &Value) -> Vec<String> {
    board.get("statuses").and_then(|s| s.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<_>>())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| STATUS_LABELS.iter().map(|(k, _)| k.to_string()).collect())
}

/// saveSchedule's checks, in the web's order: the toast to show, or Ok.
pub fn validate(d: &Value) -> Result<(), &'static str> {
    let a = &d["action"];
    let w = &d["when"];
    if s(d, "name").trim().is_empty() { return Err("Give the schedule a name"); }
    if s(d, "project").is_empty() { return Err("Pick a project"); }
    let hook = s(w, "mode") == "webhook";
    let kind = s(a, "kind");
    if hook && kind != "chat" && kind != "service" { return Err("A webhook can run a background chat or a service action"); }
    if !hook && s(w, "time").is_empty() { return Err("Set a time"); }
    if s(w, "mode") == "once" && s(w, "date").is_empty() { return Err("Pick a date for the one-time run"); }
    if s(w, "mode") == "recurring" && w.get("days").and_then(|x| x.as_array()).map(|x| x.is_empty()).unwrap_or(true) { return Err("Pick at least one day"); }
    if kind == "agent" && s(a, "prompt").trim().is_empty() { return Err("Agent needs a prompt"); }
    if kind == "chat" && s(a, "prompt").trim().is_empty() { return Err("The chat needs a prompt"); }
    if kind == "command" && s(a, "command").trim().is_empty() { return Err("Enter a command"); }
    if kind == "service" && s(a, "serviceKey").is_empty() { return Err("Pick a service"); }
    Ok(())
}

/// The webhook's URL: `<origin>/api/schedule-hook/<id>`, `?key=` unless signed.
pub fn hook_url(base: &str, id: &str, key: &str, signed: bool) -> String {
    let b = base.trim_end_matches('/');
    if signed { format!("{b}/api/schedule-hook/{id}") } else { format!("{b}/api/schedule-hook/{id}?key={key}") }
}

/// The local IANA zone ("Europe/London"), as the web sends
/// Intl.DateTimeFormat().resolvedOptions().timeZone; "" when unknown (the
/// server then reads the time on its own clock, as for an older browser).
pub fn local_tz() -> String {
    let ok = |z: &str| !z.is_empty() && z.len() <= 64 && z.chars().all(|c| c.is_ascii_alphanumeric() || "_+-/".contains(c));
    if let Ok(z) = std::env::var("TZ") {
        let z = z.trim_start_matches(':').to_string();
        if ok(&z) && z.contains('/') { return z; }
    }
    if let Ok(z) = std::fs::read_to_string("/etc/timezone") {
        let z = z.trim();
        if ok(z) { return z.to_string(); }
    }
    if let Ok(p) = std::fs::read_link("/etc/localtime") {
        let p = p.to_string_lossy().to_string();
        if let Some(i) = p.find("zoneinfo/") {
            let z = &p[i + 9..];
            if ok(z) { return z.to_string(); }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    // Names, not a glob: a glob would pull gpui's `test` macro over std's.
    use super::{action_summary, cond_summary, default_schedule, effective_when, hook_url, label, norm_date, norm_time, statuses, validate, when_summary};
    use serde_json::{json, Value};

    #[test]
    fn when_lines() {
        assert_eq!(when_summary(None), "no time set");
        assert_eq!(when_summary(Some(&json!({ "mode": "webhook" }))), "When its webhook is called");
        assert_eq!(when_summary(Some(&json!({ "mode": "once", "time": "10:00", "date": "2026-12-01" }))), "2026-12-01 10:00");
        assert_eq!(when_summary(Some(&json!({ "mode": "once", "time": "10:00", "date": "" }))), "(no date) 10:00");
        assert_eq!(when_summary(Some(&json!({ "mode": "recurring", "time": "09:00", "days": [0, 1, 2, 3, 4, 5, 6] }))), "Daily 09:00");
        assert_eq!(when_summary(Some(&json!({ "mode": "recurring", "time": "09:00", "days": [5, 1, 3] }))), "Mo,We,Fr 09:00");
        assert_eq!(when_summary(Some(&json!({ "mode": "recurring", "time": "09:00", "days": [] }))), "no days \u{00b7} 09:00");
    }

    #[test]
    fn legacy_time_condition() {
        let s = json!({ "conditions": [{ "type": "time", "at": "07:30" }] });
        assert_eq!(when_summary(effective_when(&s).as_ref()), "Daily 07:30");
        assert!(effective_when(&json!({})).is_none());
    }

    #[test]
    fn action_lines() {
        let svcs = vec![json!({ "key": "demo:web", "name": "web" })];
        assert_eq!(action_summary(&json!({ "kind": "agent", "mode": "skip", "model": "opus", "session": "resume", "sessionId": "8441d374-fd1a" }), &svcs), "Agent \u{00b7} Claude \u{00b7} skip-perms \u{00b7} opus \u{00b7} resume 8441d374");
        assert_eq!(action_summary(&json!({ "kind": "agent", "mode": "normal", "session": "new" }), &svcs), "Agent \u{00b7} Claude \u{00b7} normal \u{00b7} new session");
        assert_eq!(action_summary(&json!({ "kind": "command", "command": "npm test" }), &svcs), "Run: npm test");
        assert_eq!(action_summary(&json!({ "kind": "service", "serviceKey": "demo:web", "op": "stop" }), &svcs), "stop service \u{00b7} web");
        assert_eq!(action_summary(&json!({ "kind": "service", "serviceKey": "" }), &svcs), "start service \u{00b7} ?");
        assert_eq!(action_summary(&json!({ "kind": "chat" }), &svcs), "");
    }

    #[test]
    fn cond_lines() {
        assert_eq!(cond_summary(&json!({ "type": "usage", "metric": "week", "op": "above", "value": "80" })), "weekly usage above 80%");
        assert_eq!(cond_summary(&json!({ "type": "usage", "metric": "5h", "value": 50 })), "5h usage below 50%");
        assert_eq!(cond_summary(&json!({ "type": "terminals", "op": "equal", "value": 2 })), "open terminals equal 2");
        assert_eq!(cond_summary(&json!({ "type": "kanban", "todoId": "x", "status": "done" })), "kanban subtask = done");
        assert_eq!(cond_summary(&json!({ "type": "kanban", "status": "" })), "kanban task = ?");
    }

    #[test]
    fn checks_in_order() {
        let mut d = default_schedule("demo");
        assert_eq!(validate(&d), Err("Give the schedule a name"));
        d["name"] = json!("Nightly");
        assert_eq!(validate(&d), Err("Agent needs a prompt"));
        d["project"] = json!("");
        assert_eq!(validate(&d), Err("Pick a project"));
        d["project"] = json!("demo");
        d["when"]["mode"] = json!("webhook");
        assert_eq!(validate(&d), Err("A webhook can run a background chat or a service action"));
        d["action"]["kind"] = json!("chat");
        assert_eq!(validate(&d), Err("The chat needs a prompt"));
        d["action"]["prompt"] = json!("Look");
        assert_eq!(validate(&d), Ok(()));
        d["when"]["mode"] = json!("once");
        assert_eq!(validate(&d), Err("Pick a date for the one-time run"));
        d["when"]["time"] = json!("");
        assert_eq!(validate(&d), Err("Set a time"));
        d["when"] = json!({ "mode": "recurring", "time": "09:00", "days": [] });
        assert_eq!(validate(&d), Err("Pick at least one day"));
        d["when"]["days"] = json!([1]);
        d["action"]["kind"] = json!("command");
        assert_eq!(validate(&d), Err("Enter a command"));
        d["action"]["kind"] = json!("service");
        assert_eq!(validate(&d), Err("Pick a service"));
    }

    #[test]
    fn times_and_dates() {
        assert_eq!(norm_time("9:5"), "09:05");
        assert_eq!(norm_time("21:30"), "21:30");
        assert_eq!(norm_time("0730"), "07:30");
        assert_eq!(norm_time("25:00"), "");
        assert_eq!(norm_time("soon"), "");
        assert_eq!(norm_date("2026-11-5"), "2026-11-05");
        assert_eq!(norm_date("2026-02-30"), "");
        assert_eq!(norm_date("tomorrow"), "");
        assert_eq!(norm_date("2028-02-29"), "2028-02-29");
        assert_eq!(norm_date("2026-13-01"), "");
    }

    #[test]
    fn board_statuses() {
        assert_eq!(statuses(&Value::Null), vec!["backlog", "todo", "in_progress", "done"]);
        assert_eq!(statuses(&json!({ "statuses": ["a", "b"] })), vec!["a", "b"]);
        assert_eq!(label("in_progress"), "In Progress");
        assert_eq!(label("qa"), "qa");
    }

    #[test]
    fn hook_urls() {
        assert_eq!(hook_url("https://h/", "abc", "K", false), "https://h/api/schedule-hook/abc?key=K");
        assert_eq!(hook_url("https://h", "abc", "K", true), "https://h/api/schedule-hook/abc");
    }
}
