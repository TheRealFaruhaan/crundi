//! Factories (4.7, 4.8): the pure rules, words and geometry of the web's
//! app/vendor/crundi-factories.js and tools/factory-flow/src (convert.js,
//! nodes.jsx, Viewer.jsx's describe, Inspector.jsx's texts), unit-tested.
//! Nothing here touches the window or the server.

// A few helpers serve the unit tests and the panel's run summary words only.
#![allow(dead_code)]

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    }
}
pub fn n(v: &Value, k: &str) -> f64 { v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) }
pub fn b(v: &Value, k: &str) -> bool { v.get(k).map(truthy).unwrap_or(false) }
pub fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] { v.get(k).and_then(|x| x.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) }
pub fn truthy(v: &Value) -> bool {
    match v { Value::Null => false, Value::Bool(b) => *b, Value::String(x) => !x.is_empty(), Value::Number(n) => n.as_f64() != Some(0.0), _ => true }
}
fn plural(n: usize, one: &str, many: &str) -> String { if n == 1 { one.into() } else { many.into() } }

/// An ISO date as milliseconds since 1970 (None when it does not parse).
pub fn iso_ms(iso: &str) -> Option<f64> {
    chrono::DateTime::parse_from_rfc3339(iso.trim()).ok().map(|d| d.timestamp_millis() as f64)
}
pub fn now_ms() -> f64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0) }

/// crundi-factories.js ago(): "" for no date.
pub fn ago(iso: &str, now: f64) -> String {
    let Some(t) = iso_ms(iso) else { return String::new() };
    let s = ((now - t) / 1000.0).max(0.0);
    if s < 60.0 { "just now".into() } else if s < 3600.0 { format!("{}m ago", (s / 60.0) as u64) } else if s < 86400.0 { format!("{}h ago", (s / 3600.0) as u64) } else { format!("{}d ago", (s / 86400.0) as u64) }
}

/// crundi-factories.js dur(): "12 min", "2 h 5 min".
pub fn dur(ms: f64) -> String {
    let m = (ms.max(0.0) / 60000.0).floor() as i64;
    if m < 60 { format!("{m} min") } else { format!("{} h {} min", m / 60, m % 60) }
}
/// Viewer.jsx dur() (rounded).
pub fn dur_round(ms: f64) -> String {
    let m = (ms.max(0.0) / 60000.0).round() as i64;
    if m >= 60 { format!("{} h {} min", m / 60, m % 60) } else { format!("{m} min") }
}
/// A local "HH:MM" (Viewer.jsx time()).
pub fn time_of(iso: &str) -> String {
    use chrono::TimeZone;
    let Some(ms) = iso_ms(iso) else { return String::new() };
    chrono::Local.timestamp_millis_opt(ms as i64).single().map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}
/// The first line, at most n characters ("…" when cut).
pub fn first(s: &str, n: usize) -> String {
    let t = s.split('\n').next().unwrap_or("");
    if t.chars().count() > n { t.chars().take(n - 1).collect::<String>() + "…" } else { t.to_string() }
}

// ─── A run's status ───

pub fn status_word(st: &str) -> String {
    match st {
        "starting" => "Starting", "running" => "Running", "waiting" => "Waiting on you", "paused" => "Paused", "crashed" => "Crashed",
        "finishing" => "Done: your decision", "finished" => "Finished", "stopped" => "Stopped", _ => st,
    }.to_string()
}

/// statusChip(): the label and its look ("waiting" when a running run waits on something).
pub fn status_chip(run: &Value) -> (String, String) {
    let st = s(run, "status");
    let wo = s(run, "waitingOn");
    let mut label = status_word(&st);
    if st == "running" && !wo.is_empty() {
        label = match wo.as_str() { "ask" => "A question for you", "cap" => "Reached its limit", "manager" => "The Manager is talking to you", "paused" => "Paused", "usage" => "Paused: usage limit", _ => &label }.to_string();
    }
    let cls = if st == "running" && !wo.is_empty() { "waiting".to_string() } else { st };
    (label, cls)
}

/// The chip's colour family: ok, warn, bad, info or "".
pub fn chip_tone(cls: &str) -> &'static str {
    match cls { "ok" | "running" => "ok", "draft" | "waiting" | "paused" => "warn", "crashed" | "stopped" => "bad", "finishing" | "finished" => "info", _ => "" }
}

/// factoryNeedsYou() (webapp-html.js): the badge counts these.
pub fn needs_you(r: &Value) -> bool {
    let st = s(r, "status");
    st == "crashed" || st == "finishing" || st == "waiting" || !s(r, "waitingOn").is_empty()
}
pub fn badge(runs: &[Value]) -> String { let n = runs.iter().filter(|r| needs_you(r)).count(); if n == 0 { String::new() } else { n.to_string() } }

pub const ACTIVE: [&str; 6] = ["starting", "running", "waiting", "paused", "crashed", "finishing"];
pub fn is_active(r: &Value) -> bool { ACTIVE.contains(&s(r, "status").as_str()) }

/// pausedText(): who paused it and whether it carries on by itself.
pub fn paused_text(p: &Value) -> String {
    let by = s(p, "by");
    if by == "usage" { return "Paused: usage reached the limit set for this run; it carries on by itself after the window resets.".into(); }
    let reason = s(p, "reason");
    let head = if by == "manager" { format!("Paused by the Manager{}", if reason.is_empty() { String::new() } else { format!(": {reason}") }) } else { "Paused".into() };
    if b(p, "resumeOnReset") { format!("{head}; it carries on by itself when the usage window resets.") } else if by == "manager" && !reason.is_empty() { head } else { format!("{head}.") }
}

pub fn cap_text(c: &Value) -> String {
    if s(c, "which") == "maxMinutes" { format!("{} minutes", s(c, "limit")) } else { format!("{} handoffs", s(c, "limit")) }
}

/// The Runs list's third line.
pub fn run_meta(r: &Value, now: f64) -> String {
    let how = r.get("outcome").map(|o| s(o, "how")).unwrap_or_default();
    format!("{} handoffs · {} live sessions · {}{}", n(r, "handoffs") as i64, n(r, "liveSessions") as i64, ago(&s(r, "updatedAt"), now), if how.is_empty() { String::new() } else { format!(" · {how}") })
}
/// The panel's Earlier runs line.
pub fn earlier_meta(r: &Value, now: f64) -> String {
    let how = r.get("outcome").map(|o| s(o, "how")).unwrap_or_default();
    format!("{}{}", ago(&s(r, "updatedAt"), now), if how.is_empty() { String::new() } else { format!(" · {how}") })
}

/// runSteps(): one line per token of the run.
pub fn run_steps(run: &Value) -> Vec<(String, String)> {
    arr(run, "tokens").iter().map(|t| {
        let st = s(t, "status");
        let label = s(t, "label");
        let emp = { let e = s(t, "employee"); if e.is_empty() { "Someone".to_string() } else { e } };
        let what = match st.as_str() { "work" => "working on", "answer" => "deciding", "ask" => "You are asked", "join" => "waiting at", "finished" => "finished", "delegated" => "waiting for a job it handed out at", x => x }.to_string();
        let job = s(t, "job");
        let mut line = if st == "ask" { "You are asked a question".to_string() }
            else if st == "join" { format!("A branch is waiting at {label}") }
            else if st == "wait" { format!("Waiting at {label}") }
            else if !job.is_empty() { format!("{emp} · a job from the Manager: {}", job.chars().take(120).collect::<String>()) }
            else { format!("{emp} · {what} {label}") };
        if b(t, "blocked") { line += " (waiting for a permission answer)"; }
        (st, line)
    }).collect()
}

/// findingsView(): the summary line and one line per finding (`with_by`: the viewer's form, naming who filed it).
pub fn findings(items: &[Value], with_by: bool) -> (String, Vec<String>) {
    let open = items.iter().filter(|i| { let st = s(i, "status"); st == "open" || st == "claimed" }).count();
    let lines = items.iter().map(|i| {
        let st = match s(i, "status").as_str() { "open" => "open".to_string(), "claimed" => "said fixed".into(), "closed" => "verified".into(), "dismissed" => "dismissed".into(), x => x.into() };
        let re = n(i, "reopened") as i64;
        let wh = s(i, "where");
        let by = if with_by { format!(" · {}", s(i, "by")) } else { String::new() };
        format!("{} {st}{}{}{by}: {}{}", s(i, "id"), if b(i, "blocking") { " · blocking" } else { " · nit" }, if re > 0 { format!(" · reopened {re}x") } else { String::new() }, s(i, "text"), if wh.is_empty() { String::new() } else { format!(" ({wh})") })
    }).collect();
    (format!("Review findings: {open} open of {}", items.len()), lines)
}

/// The Viewer's progress line: handoffs · time · turns · open findings.
pub fn progress(run: &Value, now: f64) -> String {
    let h = n(run, "handoffs") as i64;
    let mut bits = vec![format!("{h} handoff{}", if h == 1 { "" } else { "s" })];
    if let Some(t) = iso_ms(&s(run, "createdAt")) { bits.push(dur_round(now - t)); }
    if let Some(u) = run.get("usage") {
        let turns = n(u, "turns") as i64;
        if turns > 0 { let c = n(u, "costUsd"); bits.push(format!("{turns} turns{}", if c > 0.0 { format!(" · ${c:.2}") } else { String::new() })); }
    }
    let f = arr(run, "findings");
    if !f.is_empty() { bits.push(format!("{} open findings", f.iter().filter(|i| { let st = s(i, "status"); st == "open" || st == "claimed" }).count())); }
    bits.join(" · ")
}

/// A wait's time left (WaitCard's left()).
pub fn left(iso: &str, now: f64) -> String {
    let ms = iso_ms(iso).map(|t| t - now).unwrap_or(-1.0);
    if ms <= 0.0 { return "any moment".into(); }
    let m = (ms / 60000.0).floor() as i64;
    if m >= 60 { format!("{} h {} min", m / 60, m % 60) } else if m >= 1 { format!("{m} min") } else { format!("{} s", (ms / 1000.0).ceil() as i64) }
}

/// The head of a wait card (viewer form: "trying"; panel form: "checking").
pub fn wait_head(t: &Value, now: f64, panel: bool) -> String {
    let w = &t["wait"];
    let kind = s(w, "kind");
    let tries = n(w, "tries") as i64;
    let what = if kind == "time" || kind == "until" { format!("{} left", left(&s(w, "deadline"), now)) }
        else if kind == "signal" { let x = s(w, "what"); if x.is_empty() { "waiting for a signal".into() } else { x } }
        else if panel { format!("checking ({tries} tr{} so far)", if tries == 1 { "y" } else { "ies" }) }
        else { format!("trying ({tries} so far)") };
    format!("{}: {what}", s(t, "label"))
}

// ─── Triggers ───

pub const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
pub const COLUMNS: [(&str, &str); 4] = [("backlog", "Backlog"), ("todo", "To do"), ("in_progress", "In progress"), ("done", "Done")];
fn column_label(k: &str) -> String { COLUMNS.iter().find(|c| c.0 == k).map(|c| c.1.to_string()).unwrap_or_else(|| k.to_string()) }

pub fn trigger_when(t: &Value) -> String {
    let kind = s(t, "kind");
    let w = if kind == "schedule" {
        let sc = &t["schedule"];
        let days: Vec<i64> = arr(sc, "days").iter().filter_map(|d| d.as_i64()).collect();
        format!("At {}{}", s(sc, "at"), if !days.is_empty() && days.len() < 7 { format!(" on {}", days.iter().map(|d| DAYS.get(*d as usize).copied().unwrap_or("")).collect::<Vec<_>>().join(", ")) } else { " every day".into() })
    } else if kind == "kanban" { format!("When a card enters {}", column_label(&s(&t["kanban"], "status"))) } else { "When its webhook is called".into() };
    let nc = s(t, "needsCards");
    match COLUMNS.iter().find(|c| c.0 == nc) { Some(c) => format!("{w}, if {} has a card", c.1), None => w }
}

pub fn trigger_last(t: &Value, now: f64) -> String {
    let Some(h) = arr(t, "history").first() else { return String::new() };
    let note = s(h, "note");
    format!("{}: {}{}", ago(&s(h, "at"), now), s(h, "outcome"), if note.is_empty() { String::new() } else { format!(" ({note})") })
}

pub fn trigger_history_line(h: &Value, now: f64) -> String {
    let (t, note) = (s(h, "title"), s(h, "note"));
    format!("{} · {} · {}{}{}", ago(&s(h, "at"), now), s(h, "outcome"), s(h, "source"), if t.is_empty() { String::new() } else { format!(": {t}") }, if note.is_empty() { String::new() } else { format!(" ({note})") })
}

/// The change request box's head in the trigger dialog.
pub fn trigger_cr_head(kind: &str) -> String {
    let ph = match kind {
        "kanban" => "{{card.title}}, {{card.description}}, {{card.todos}} (one per line), {{date}}",
        "webhook" => "{{payload.some.field}}, {{payload.alerts.0.labels.alertname}} (arrays by index), {{payload}} (all of it), {{headers.name}}, {{date}}",
        _ => "{{date}}, {{time}}",
    };
    format!("Filled in when it fires. Placeholders: {ph}. {}", if kind == "webhook" { "Anything a call does not have comes out empty and never stops it; the Manager gets the full request as well." } else { "Only the title has to come out filled; the Manager asks you about anything missing." })
}

// ─── Cleanup ───

pub fn cleanup_title(it: &Value) -> String {
    match s(it, "kind").as_str() { "kept-branch" => format!("Kept branch {}", s(it, "branch")), "orphan" => "Unknown worktree folder".into(), _ => format!("Worktree of a {} run", s(it, "status")) }
}
pub fn cleanup_meta(it: &Value) -> String {
    let (f, p) = (s(it, "factoryName"), s(it, "path"));
    format!("{}{}{}", s(it, "project"), if f.is_empty() { String::new() } else { format!(" · {f}") }, if p.is_empty() { String::new() } else { format!(" · {p}") })
}
pub fn cleanup_warn(it: &Value) -> String {
    let u = n(it, "uncommitted") as i64;
    if u <= 0 { String::new() } else { format!("{u} uncommitted change{} in it: deleting loses them.", if u == 1 { "" } else { "s" }) }
}
/// crundi-factories.js size(): "0", "512 B", "1.2 MB".
pub fn fx_size(bytes: f64) -> String {
    if bytes <= 0.0 { return "0".into(); }
    let u = ["B", "KB", "MB", "GB"];
    let i = ((bytes.ln() / 1024f64.ln()).floor() as usize).min(3);
    let v = bytes / 1024f64.powi(i as i32);
    if i == 0 { format!("{v:.0} {}", u[i]) } else { format!("{v:.1} {}", u[i]) }
}
pub fn cleanup_size(it: &Value) -> String {
    if s(it, "kind") == "kept-branch" { "-".into() } else { format!("{}{}", fx_size(n(it, "bytes")), if b(it, "sizeCapped") { "+" } else { "" }) }
}
pub fn cleanup_age(it: &Value) -> String { format!("{}d", (n(it, "ageMs") / 86400000.0).floor() as i64) }
pub fn cleanup_confirm(picked: &[&Value]) -> String {
    let lose = picked.iter().filter(|i| n(i, "uncommitted") > 0.0 || s(i, "kind") == "kept-branch").count();
    format!("Delete {} item{}?{}", picked.len(), if picked.len() == 1 { "" } else { "s" }, if lose > 0 { format!(" {lose} of them hold work that will be lost.") } else { String::new() })
}

// ─── The library ───

pub fn factory_meta(f: &Value, now: f64) -> String {
    format!("{} people · {} steps · edited {}", n(f, "employeeCount") as i64, n(f, "nodeCount") as i64, ago(&s(f, "updatedAt"), now))
}
pub fn export_name(name: &str) -> String {
    let base = if name.is_empty() { "factory" } else { name };
    let mut out = String::new();
    let mut dash = false;
    for c in base.chars() {
        if c.is_ascii_alphanumeric() || c == '-' { out.push(c.to_ascii_lowercase()); dash = false; } else if !dash { out.push('-'); dash = true; }
    }
    format!("{out}.factory.json")
}
pub fn import_toast(r: &Value) -> String {
    let v = &r["validation"];
    let errs = arr(v, "errors").len();
    format!("Imported \"{}\".{}", s(&r["factory"], "name"), if b(v, "ok") { String::new() } else { format!(" It is a draft: open it to fix {errs} problem{}.", if errs == 1 { "" } else { "s" }) })
}

// ─── The live graph's timeline (Viewer.jsx describe()) ───

pub struct Line { pub k: String, pub main: String, pub sub: String }
fn line(k: &str, main: String, sub: String) -> Option<Line> { Some(Line { k: k.into(), main, sub }) }

pub fn describe(ev: &Value, names: &HashMap<String, String>, nodes: &HashMap<String, Value>) -> Option<Line> {
    let who = |id: &str| names.get(id).cloned().unwrap_or_else(|| id.to_string());
    let by = s(ev, "by");
    match s(ev, "type").as_str() {
        "start" => line("start", "Started".into(), s(ev, "task")),
        "setup" => { let ok = s(ev, "status") == "ok"; line(if ok { "info" } else { "escalate" }, format!("Setup command {}", if ok { "ran" } else { "failed" }), s(ev, "command")) }
        "begin" => line("info", "The Manager got the task".into(), String::new()),
        "handoff" => {
            let p = &ev["payload"];
            if b(p, "finish") { line("finish", format!("{} finished", who(&by)), first(&s(p, "summary"), 160)) }
            else if b(p, "escalate") { line("escalate", format!("{} escalated", who(&by)), first(&s(p, "escalate"), 160)) }
            else { line("handoff", format!("{} handed off", who(&by)), first(&s(p, "summary"), 160)) }
        }
        "escalate" => line("escalate", "Sent to the Manager".into(), first(&s(ev, "reason"), 160)),
        "ask-reply" => line("info", "You replied".into(), s(ev, "note")),
        "wait-done" => {
            let label = nodes.get(&s(ev, "nodeId")).map(|n| s(n, "label")).filter(|x| !x.is_empty()).unwrap_or_else(|| "Wait".into());
            let to = s(ev, "outcome") == "timeout";
            line(if to { "escalate" } else { "info" }, format!("{label}: {}", if to { "gave up waiting" } else { "over" }), first(&s(ev, "detail"), 160))
        }
        "perm-decided" => {
            if !(by == "manager" || by == "deny-list" || by == "gatekeeper") { return None; }
            let d = s(ev, "decision");
            let tail = if by == "deny-list" { " (never allowed)" } else if d == "escalated" || by == "gatekeeper" { " by the Gatekeeper" } else { " by the Manager" };
            let input = &ev["input"];
            let sub = { let r = s(ev, "reason"); if !r.is_empty() { r } else { let c = s(input, "command"); if !c.is_empty() { c } else { s(input, "url") } } };
            line(if d == "allowed" { "info" } else { "escalate" }, format!("{}: {} {}{tail}", s(ev, "employee"), s(ev, "toolName"), if d == "escalated" { "passed to the Manager".into() } else { d.clone() }), first(&sub, 160))
        }
        "secrets-unlocked" => line("info", format!("Secrets unlocked for this run: {}", arr(ev, "secrets").iter().map(|x| s(x, "name")).collect::<Vec<_>>().join(", ")), "Kept in memory only, until the run ends".into()),
        "secret-used" => {
            let nm = |x: &str| match x { "manager" => "the Manager".to_string(), "gatekeeper" => "the Gatekeeper".into(), "person" => "you".into(), "crundi" => "Crundi".into(), o => o.into() };
            let ap = arr(ev, "approvedBy").iter().map(|x| nm(x.as_str().unwrap_or(""))).collect::<Vec<_>>().join(", then ");
            let refused = ev.get("ok") == Some(&Value::Bool(false));
            let emp = { let e = s(ev, "employee"); if e.is_empty() { who(&s(ev, "employeeId")) } else { e } };
            let mut sub = vec![];
            if b(ev, "preauthorised") { sub.push("Pre-authorised".to_string()); }
            if !ap.is_empty() { sub.push(format!("{}{ap}", if refused { "Decided by " } else { "Approved by " })); }
            line(if refused { "escalate" } else { "info" }, format!("{emp} {} the secret \"{}\"", if refused { "was refused" } else { "used" }, s(ev, "secret")), sub.join(" · "))
        }
        "delegate" => line("handoff", format!("The Manager handed {} a job", who(&s(ev, "employeeId"))), first(&s(ev, "task"), 160)),
        "manager-message" => line("info", format!("The Manager {} {}", if b(ev, "hold") { "held" } else if b(ev, "interrupt") { "stopped and told" } else { "told" }, who(&s(ev, "employeeId"))), first(&s(ev, "text"), 160)),
        "trigger-message" => { let k = s(ev, "key"); line("info", format!("Mid-run: trigger \"{}\" fired again; passed to the Manager", s(ev, "name")), format!("{}{}", if k.is_empty() { String::new() } else { format!("[{k}] ") }, s(ev, "title"))) }
        "team-mode" => line("info", if s(ev, "mode") == "bypass" { "The team now runs with bypass".into() } else { "The team asks again".into() }, String::new()),
        "policy" => line("info", "The Manager set its permission policy".into(), first(&s(ev, "text"), 160)),
        "rule" => line("info", "Rule granted".into(), s(ev, "rule")),
        "retire" => line("info", format!("{} stopped for now", who(&s(ev, "employeeId"))), "Not needed by the current work; resumed with their memory if needed again.".into()),
        "pause" => {
            let head = if by == "usage" { "Paused: usage limit" } else if by == "manager" { "Paused by the Manager" } else { "Paused" };
            let sub: Vec<String> = [s(ev, "reason"), if b(ev, "resumeOnReset") { "Carries on by itself when the usage window resets".into() } else { String::new() }].into_iter().filter(|x| !x.is_empty()).collect();
            line("escalate", format!("{head}{}", if s(ev, "mode") == "now" { ", turns in progress stopped" } else { "" }), sub.join(" · "))
        }
        "hold" => line("info", format!("{} woke by itself and was told to hold", who(&s(ev, "employeeId"))), if s(ev, "why") == "held" { "The Manager had told them to wait".into() } else { "The run is paused".into() }),
        "resume" => line("info", "Resumed".into(), String::new()),
        "cap" => line("escalate", format!("Limit reached ({})", cap_text(ev)), String::new()),
        "crash" => line("crash", "Crashed".into(), s(ev, "reason")),
        "recover" => line("info", "Recovered".into(), s(ev, "note")),
        "reopen" => line("escalate", "Back to work".into(), first(&s(ev, "reason"), 160)),
        "merged" => line("merged", "Merged".into(), s(ev, "commit").chars().take(7).collect()),
        "kept" => line("info", "Kept on its branch".into(), String::new()),
        "discarded" => line("info", "Discarded".into(), String::new()),
        "undone" => line("info", "Merge undone".into(), String::new()),
        "stop" => line("crash", "Stopped".into(), String::new()),
        "effect-done" => {
            let r = &ev["result"];
            let c = arr(r, "conflicts");
            if !c.is_empty() { return line("escalate", "Merging the branches conflicted".into(), c.iter().map(|x| arr(x, "files").iter().map(|f| f.as_str().unwrap_or("").to_string()).collect::<Vec<_>>().join(", ")).collect::<Vec<_>>().join("; ")); }
            if !arr(r, "merged").is_empty() { return line("info", "Branches merged back together".into(), String::new()); }
            None
        }
        _ => None,
    }
}

// ─── Nodes and edges (convert.js, nodes.jsx) ───

pub fn kind_label(t: &str) -> &'static str {
    match t { "start" => "Start", "manager" => "Manager", "employee" => "Employee", "decision" => "Decision", "join" => "Join", "ask" => "Ask me", "wait" => "Wait", "end" => "End", _ => "Employee" }
}
pub fn is_agent(t: &str) -> bool { t == "manager" || t == "employee" }

/// outputsOf() (src/factory-model.js).
pub fn outputs_of(node: &Value) -> Vec<String> {
    let ids = |list: &[Value]| list.iter().map(|o| s(o, "id")).collect::<Vec<_>>();
    match s(node, "type").as_str() {
        "start" | "employee" | "join" => vec!["out".into()],
        "manager" => vec!["out".into(), "finish".into()],
        "decision" => {
            let d = &node["decision"];
            match s(d, "kind").as_str() { "yesno" => vec!["yes".into(), "no".into()], "many" => { let mut v = ids(arr(d, "options")); v.push("none".into()); v } _ => ids(arr(d, "options")) }
        }
        "ask" => ids(arr(&node["ask"], "replies")),
        "wait" => { let k = s(&node["wait"], "kind"); if k == "command" || k == "signal" { vec!["done".into(), "timeout".into()] } else { vec!["out".into()] } }
        _ => vec![],
    }
}

/// handleLabel() (convert.js).
pub fn handle_label(node: &Value, h: &str) -> String {
    if h == "out" { return String::new(); }
    if h == "finish" { return "Finish".into(); }
    let find = |list: &[Value]| list.iter().find(|o| s(o, "id") == h).map(|o| s(o, "label")).unwrap_or_else(|| "?".into());
    match s(node, "type").as_str() {
        "decision" => match h { "yes" => "Yes".into(), "no" => "No".into(), "none" => "None".into(), _ => find(arr(&node["decision"], "options")) },
        "ask" => find(arr(&node["ask"], "replies")),
        "wait" => match h { "done" => "Done".into(), "timeout" => "Timed out".into(), _ => String::new() },
        _ => h.to_string(),
    }
}

/// waitText() (nodes.jsx).
pub fn wait_text(w: &Value) -> String {
    if !w.is_object() { return String::new(); }
    let lim = |m: i64| if m >= 120 && m % 60 == 0 { format!("{} h", m / 60) } else { format!("{m} min") };
    match s(w, "kind").as_str() {
        "time" => format!("For {}", lim(n(w, "minutes") as i64)),
        "until" => format!("Until {}", s(w, "at")),
        "command" => { let c = s(w, "command"); format!("{} · {} max", if c.is_empty() { "No command yet".into() } else { format!("Until `{c}` succeeds") }, lim(n(w, "timeout") as i64)) }
        _ => { let x = s(w, "what"); format!("For a signal{} · {} max", if x.is_empty() { String::new() } else { format!(": {x}") }, lim(n(w, "timeout") as i64)) }
    }
}

pub fn roster_of(f: &Value) -> HashMap<String, Value> { arr(f, "roster").iter().map(|r| (s(r, "id"), r.clone())).collect() }

/// rosterOf() (factory-engine.js): who works a node.
pub fn person_of(node: &Value) -> String {
    match s(node, "type").as_str() { "manager" => "manager".into(), "employee" => s(node, "employeeId"), _ => String::new() }
}

/// The node's body line (nodes.jsx body()).
pub fn node_body(node: &Value, f: &Value) -> String {
    let roster = roster_of(f);
    let emp = match s(node, "type").as_str() { "employee" => roster.get(&s(node, "employeeId")), "manager" => roster.get("manager"), _ => None };
    match s(node, "type").as_str() {
        "start" => "The task comes in".into(),
        "manager" => { let m = emp.map(|e| s(e, "model")).unwrap_or_default(); if m.is_empty() { "Leads the team, decides the end".into() } else { format!("Model: {m}") } }
        "employee" => match emp {
            Some(e) => { let m = s(e, "model"); format!("{}{}", if m.is_empty() { String::new() } else { format!("{m} · ") }, if s(e, "memory") == "fresh" { "fresh each time" } else { "keeps memory" }) }
            None => "Nobody assigned".into(),
        },
        "decision" => {
            let d = &node["decision"];
            let k = match s(d, "kind").as_str() { "yesno" => "Yes / no", "one" => "Pick one", "many" => "Pick any", _ => "" };
            let q = s(d, "question");
            let ab = s(d, "answeredBy");
            let by = if ab.is_empty() { String::new() } else { roster.get(&ab).map(|r| s(r, "name")).unwrap_or_default() };
            format!("{}{} · {k}", if q.is_empty() { "No question yet".into() } else { q }, if by.is_empty() { String::new() } else { format!(" — {by} answers") })
        }
        "join" => {
            let fid = s(&node["join"], "forkId");
            match arr(f, "nodes").iter().find(|x| s(x, "id") == fid && !fid.is_empty()) {
                Some(j) => { let l = s(j, "label"); format!("Waits for the branches of {}", if l.is_empty() { "its decision".into() } else { l }) }
                None => "Choose which decision it joins".into(),
            }
        }
        "ask" => { let p = s(&node["ask"], "prompt"); if p.is_empty() { "No question yet".into() } else { p } }
        "wait" => wait_text(&node["wait"]),
        "end" => "The result, presented by the Manager".into(),
        _ => String::new(),
    }
}

/// The node's title (nodes.jsx title()).
pub fn node_title(node: &Value, f: &Value) -> String {
    let l = s(node, "label");
    if !l.is_empty() { return l; }
    let roster = roster_of(f);
    let t = s(node, "type");
    let name = |id: &str| roster.get(id).map(|r| s(r, "name")).filter(|x| !x.is_empty());
    match t.as_str() {
        "employee" => name(&s(node, "employeeId")).unwrap_or_else(|| "Employee".into()),
        "manager" => name("manager").unwrap_or_else(|| "Manager".into()),
        _ => kind_label(&t).into(),
    }
}

// ─── Geometry (flow units: the web's pixels at zoom 1) ───
// Keep these in step with the node drawing in ui/planning/factories-graph.slint
// (Theme.factories-node-w, -node-w-small and the scale steps it stacks).

pub const NODE_W: f32 = 220.0;
pub const NODE_W_SMALL: f32 = 170.0;
pub const PAD_TOP: f32 = 10.0;   // sp-10
pub const HEAD_H: f32 = 16.0;    // sp-16
pub const TITLE_H: f32 = 20.0;   // sp-20 (+ sp-2 above)
pub const LINE_H: f32 = 16.0;    // sp-16 a body line
pub const BODY_GAP: f32 = 8.0;   // sp-8 under the body
pub const SKILLS_H: f32 = 20.0;  // sp-20
pub const OUTS_H: f32 = 32.0;    // sp-32
pub const CHARS_PER_LINE: usize = 34;

pub fn node_w(t: &str) -> f32 { if t == "start" || t == "end" { NODE_W_SMALL } else { NODE_W } }
pub fn body_lines(text: &str, w: f32) -> i32 {
    let per = ((CHARS_PER_LINE as f32) * w / NODE_W).max(8.0) as usize;
    if text.chars().count() > per { 2 } else { 1 }
}
pub fn node_h(lines: i32, skills: bool, outs: usize) -> f32 {
    PAD_TOP + HEAD_H + 2.0 + TITLE_H + lines as f32 * LINE_H + BODY_GAP + if skills { SKILLS_H } else { 0.0 } + if outs > 0 { OUTS_H } else { 0.0 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

/// The source handle's point: outputs spread under the node; note: right middle.
pub fn out_point(r: &Rect, outs: &[String], h: &str) -> (f32, f32) {
    if h == "note" { return (r.x + r.w, r.y + r.h / 2.0); }
    let i = outs.iter().position(|x| x == h).unwrap_or(0);
    let c = outs.len().max(1) as f32;
    (r.x + r.w * (i as f32 + 0.5) / c, r.y + r.h)
}
pub fn in_point(r: &Rect, note: bool) -> (f32, f32) { if note { (r.x, r.y + r.h / 2.0) } else { (r.x + r.w / 2.0, r.y) } }

fn off(d: f32, c: f32) -> f32 { if d >= 0.0 { 0.5 * d } else { c * 25.0 * (-d).sqrt() } }

/// getBezierPath() (React Flow), curvature 0.3: bottom to top, or right to left for a note.
pub fn bezier(sx: f32, sy: f32, tx: f32, ty: f32, note: bool) -> (String, f32, f32) {
    let c = 0.3;
    let (scx, scy, tcx, tcy) = if note { (sx + off(tx - sx, c), sy, tx - off(tx - sx, c), ty) } else { (sx, sy + off(ty - sy, c), tx, ty - off(ty - sy, c)) };
    let lx = sx * 0.125 + scx * 0.375 + tcx * 0.375 + tx * 0.125;
    let ly = sy * 0.125 + scy * 0.375 + tcy * 0.375 + ty * 0.125;
    (format!("M {sx:.1} {sy:.1} C {scx:.1} {scy:.1} {tcx:.1} {tcy:.1} {tx:.1} {ty:.1}"), lx, ly)
}

/// loopPath() (nodes.jsx): a connection back up the flow, round the side.
pub fn loop_path(sx: f32, sy: f32, tx: f32, ty: f32, lane: f32) -> (String, f32, f32) {
    let side = lane.max(sx + 40.0).max(tx + 40.0);
    let (r, down, up) = (14.0, sy + 24.0, ty - 24.0);
    let d = format!("M {sx:.1} {sy:.1} L {sx:.1} {:.1} Q {sx:.1} {down:.1} {:.1} {down:.1} L {:.1} {down:.1} Q {side:.1} {down:.1} {side:.1} {:.1} L {side:.1} {:.1} Q {side:.1} {up:.1} {:.1} {up:.1} L {:.1} {up:.1} Q {tx:.1} {up:.1} {tx:.1} {:.1} L {tx:.1} {ty:.1}",
        down - r, sx + r, side - r, down - r, up + r, side - r, tx + r, up + r);
    (d, side, (down + up) / 2.0)
}

/// The closed arrow at an edge's end (pointing down into a step's top, or right into a note handle).
pub fn arrow(tx: f32, ty: f32, note: bool) -> String {
    let (l, h) = (9.0, 5.0);
    if note { format!("M {tx:.1} {ty:.1} L {:.1} {:.1} L {:.1} {:.1} Z", tx - l, ty - h, tx - l, ty + h) }
    else { format!("M {tx:.1} {ty:.1} L {:.1} {:.1} L {:.1} {:.1} Z", tx - h, ty - l, tx + h, ty - l) }
}

/// Lanes for loops (toFlow): edges whose target sits above their source.
pub fn lanes(f: &Value) -> HashMap<String, f32> {
    let nodes = arr(f, "nodes");
    let pos: HashMap<String, (f32, f32)> = nodes.iter().map(|x| (s(x, "id"), (n(&x["position"], "x") as f32, n(&x["position"], "y") as f32))).collect();
    let right = nodes.iter().map(|x| n(&x["position"], "x") as f32 + 230.0).fold(f32::MIN, f32::max).max(0.0);
    let mut backs: Vec<(String, f32)> = arr(f, "edges").iter().filter(|e| s(e, "kind") == "flow").filter_map(|e| {
        let sy = pos.get(&s(e, "source")).map(|p| p.1).unwrap_or(0.0);
        let ty = pos.get(&s(e, "target")).map(|p| p.1).unwrap_or(0.0);
        (ty < sy - 20.0).then(|| (s(e, "id"), sy))
    }).collect();
    backs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    backs.into_iter().enumerate().map(|(i, (id, _))| (id, right + 40.0 + i as f32 * 24.0)).collect()
}

/// The label on an edge (FactoryEdge bits): "" for none.
pub fn edge_label(e: &Value, used: Option<i64>) -> String {
    let note = s(e, "kind") == "note";
    let nl = s(e, "noteLabel");
    let mut bits = vec![];
    if note { bits.push(if nl.is_empty() { "note".into() } else { nl }); } else {
        if b(e, "requireNote") { bits.push(format!("{} required", if nl.is_empty() { "note" } else { &nl })); } else if !nl.is_empty() { bits.push(nl); }
        let mr = n(e, "maxRounds") as i64;
        if mr > 0 { bits.push(match used { Some(u) => format!("round {}/{mr}", u.min(mr)), None => format!("max {mr}") }); }
    }
    bits.join(" · ")
}

/// Issues by node, edge and employee id (issuesBy()): (level, message, fix).
pub type Issue = (String, String, String, Value);
pub fn issues_by(validation: &Value) -> (HashMap<String, Vec<Issue>>, HashMap<String, Vec<Issue>>, HashMap<String, Vec<Issue>>) {
    let (mut nodes, mut edges, mut emps): (HashMap<String, Vec<Issue>>, HashMap<String, Vec<Issue>>, HashMap<String, Vec<Issue>>) = Default::default();
    for (list, level) in [("errors", "error"), ("warnings", "warning")] {
        for it in arr(validation, list) {
            let x: Issue = (level.into(), s(it, "message"), s(it, "fix"), it.clone());
            let nid = s(it, "nodeId");
            if !nid.is_empty() { nodes.entry(nid).or_default().push(x.clone()); }
            for id in arr(it, "nodeIds") { if let Some(id) = id.as_str() { nodes.entry(id.into()).or_default().push(x.clone()); } }
            let eid = s(it, "edgeId");
            if !eid.is_empty() { edges.entry(eid).or_default().push(x.clone()); }
            let mid = s(it, "employeeId");
            if !mid.is_empty() { emps.entry(mid).or_default().push(x.clone()); }
        }
    }
    (nodes, edges, emps)
}

/// The badge on a node: (count, "error"|"warning"), or (0, "").
pub fn badge_of(issues: &[Issue]) -> (i32, String) {
    if issues.is_empty() { return (0, String::new()); }
    let errs = issues.iter().filter(|i| i.0 == "error").count();
    if errs > 0 { (errs as i32, "error".into()) } else { (issues.len() as i32, "warning".into()) }
}

/// The token pill on a node (Tokens()): its words and its look.
pub fn token_pill(tokens: &[Value], blocked: bool) -> (String, String) {
    let Some(t) = tokens.first() else { return (String::new(), String::new()) };
    let st = s(t, "status");
    let label = if blocked { "needs permission".to_string() } else { match st.as_str() { "work" => "working", "answer" => "deciding", "ask" => "waiting on you", "join" | "wait" => "waiting", "delegated" => "waiting on a job", "finished" => "finished", x => x }.to_string() };
    (format!("{label}{}", if tokens.len() > 1 { format!(" ×{}", tokens.len()) } else { String::new() }), if blocked { "blocked".into() } else { st })
}

// ─── Fitting the view ───

/// The zoom and pan that fit a box into a canvas (fitView), with `pad` a share of the canvas.
pub fn fit(bx: Rect, cw: f32, ch: f32, pad: f32, min_z: f32, max_z: f32) -> (f32, f32, f32) {
    if cw <= 0.0 || ch <= 0.0 || bx.w <= 0.0 { return (1.0, 0.0, 0.0); }
    let zx = cw / (bx.w * (1.0 + pad * 2.0));
    let zy = ch / (bx.h.max(1.0) * (1.0 + pad * 2.0));
    let z = zx.min(zy).clamp(min_z, max_z);
    let px = cw / 2.0 - (bx.x + bx.w / 2.0) * z;
    let py = ch / 2.0 - (bx.y + bx.h / 2.0) * z;
    (z, px, py)
}
pub fn union(rs: &[Rect]) -> Option<Rect> {
    let first = rs.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x + first.w, first.y + first.h);
    for r in rs { x0 = x0.min(r.x); y0 = y0.min(r.y); x1 = x1.max(r.x + r.w); y1 = y1.max(r.y + r.h); }
    Some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
}

// ─── The editor's rules (Editor.jsx) ───

/// A new id like genId(): prefix + random base-36.
pub fn gen_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let t = now_ms() as u64 ^ (C.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut x = t.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut out = String::from(prefix);
    for _ in 0..10 { let d = (x % 36) as u32; out.push(std::char::from_digit(d, 36).unwrap_or('0')); x /= 36; if x == 0 { x = t.rotate_left(17) | 1; } }
    out
}

/// newNode(): a step of a kind ("employee:<id>", "decision:yesno", "join", "ask", "wait").
pub fn new_node(kind: &str, x: f64, y: f64) -> Value {
    let (t, a) = kind.split_once(':').unwrap_or((kind, ""));
    let mut base = json!({ "id": gen_id("n_"), "type": t, "label": "", "position": { "x": x, "y": y } });
    match t {
        "employee" => { base["employeeId"] = json!(a); }
        "decision" => {
            let opts = if a == "yesno" { json!([]) } else { json!([{ "id": gen_id("o_"), "label": "Option 1" }, { "id": gen_id("o_"), "label": "Option 2" }]) };
            base["decision"] = json!({ "kind": a, "question": "", "options": opts, "answeredBy": null });
        }
        "join" => { base["join"] = json!({ "forkId": "" }); }
        "ask" => { base["ask"] = json!({ "prompt": "", "replies": [{ "id": gen_id("o_"), "label": "Go ahead" }, { "id": gen_id("o_"), "label": "Stop" }] }); }
        "wait" => { base["wait"] = json!({ "kind": "command", "minutes": 10, "at": "09:00", "command": "", "every": 60, "timeout": 60, "what": "" }); }
        _ => {}
    }
    base
}

/// isValidConnection().
pub fn can_connect(f: &Value, src: &str, sh: &str, tgt: &str, th: &str) -> bool {
    let nodes = arr(f, "nodes");
    let (Some(sn), Some(tn)) = (nodes.iter().find(|x| s(x, "id") == src), nodes.iter().find(|x| s(x, "id") == tgt)) else { return false };
    if src == tgt { return false; }
    let (st, tt) = (s(sn, "type"), s(tn, "type"));
    if sh == "note" { return th == "notein" && is_agent(&st) && is_agent(&tt); }
    if th != "in" || tt == "start" { return false; }
    if sh == "finish" { return tt == "end"; }
    tt != "end"
}

/// Ids reachable from `from` over flow edges.
pub fn reachable(f: &Value, from: &str) -> HashSet<String> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for e in arr(f, "edges") { if s(e, "kind") == "flow" { out.entry(s(e, "source")).or_default().push(s(e, "target")); } }
    let mut seen = HashSet::new();
    let mut stack = vec![from.to_string()];
    while let Some(id) = stack.pop() { if seen.insert(id.clone()) { if let Some(n) = out.get(&id) { stack.extend(n.iter().cloned()); } } }
    seen
}

/// onConnect(): adds the edge; true when it closed a loop (it gets a limit of 3).
pub fn connect(f: &mut Value, src: &str, sh: &str, tgt: &str) -> bool {
    let edges = f["edges"].as_array_mut().map(std::mem::take).unwrap_or_default();
    if sh == "note" {
        let mut edges = edges;
        if !edges.iter().any(|e| s(e, "kind") == "note" && s(e, "source") == src && s(e, "target") == tgt) {
            edges.push(json!({ "id": gen_id("e_"), "kind": "note", "source": src, "sourceHandle": "note", "target": tgt, "requireNote": false, "noteLabel": "", "maxRounds": 0 }));
        }
        f["edges"] = Value::Array(edges);
        return false;
    }
    let mut edges: Vec<Value> = edges.into_iter().filter(|e| !(s(e, "kind") == "flow" && s(e, "source") == src && s(e, "sourceHandle") == sh)).collect();
    f["edges"] = Value::Array(edges.clone());
    let closes = reachable(f, tgt).contains(src);
    edges.push(json!({ "id": gen_id("e_"), "kind": "flow", "source": src, "sourceHandle": sh, "target": tgt, "requireNote": false, "noteLabel": "", "maxRounds": if closes { 3 } else { 0 } }));
    f["edges"] = Value::Array(edges);
    closes
}

/// Delete steps (never Start, Manager or End), their edges, and joins' links to them.
pub fn remove_nodes(f: &mut Value, ids: &[String]) {
    let gone: HashSet<String> = arr(f, "nodes").iter().filter(|x| ids.contains(&s(x, "id")) && !["start", "manager", "end"].contains(&s(x, "type").as_str())).map(|x| s(x, "id")).collect();
    let nodes: Vec<Value> = arr(f, "nodes").iter().filter(|x| !gone.contains(&s(x, "id"))).cloned().map(|mut x| {
        if s(&x, "type") == "join" && gone.contains(&s(&x["join"], "forkId")) { x["join"]["forkId"] = json!(""); }
        x
    }).collect();
    let edges: Vec<Value> = arr(f, "edges").iter().filter(|e| !gone.contains(&s(e, "source")) && !gone.contains(&s(e, "target"))).cloned().collect();
    f["nodes"] = Value::Array(nodes);
    f["edges"] = Value::Array(edges);
}
pub fn remove_edge(f: &mut Value, id: &str) {
    let edges: Vec<Value> = arr(f, "edges").iter().filter(|e| s(e, "id") != id || s(e, "sourceHandle") == "finish").cloned().collect();
    f["edges"] = Value::Array(edges);
}

/// layoutEdges() (factory-model.js): flow edges that read top to bottom.
pub fn layout_edges(f: &Value) -> Vec<(String, String)> {
    let nodes = arr(f, "nodes");
    let mut out: HashMap<String, Vec<Value>> = nodes.iter().map(|x| (s(x, "id"), vec![])).collect();
    for e in arr(f, "edges") { if s(e, "kind") != "note" && out.contains_key(&s(e, "source")) && out.contains_key(&s(e, "target")) { out.get_mut(&s(e, "source")).unwrap().push(e.clone()); } }
    for l in out.values_mut() { l.sort_by_key(|e| (n(e, "maxRounds") > 0.0) as u8); }
    let mut state: HashMap<String, u8> = HashMap::new();
    let mut keep = vec![];
    fn visit(id: &str, out: &HashMap<String, Vec<Value>>, state: &mut HashMap<String, u8>, keep: &mut Vec<(String, String)>) {
        state.insert(id.into(), 1);
        for e in out.get(id).cloned().unwrap_or_default() {
            let t = s(&e, "target");
            if state.get(&t) == Some(&1) { continue; }
            keep.push((id.to_string(), t.clone()));
            if !state.contains_key(&t) { visit(&t, out, state, keep); }
        }
        state.insert(id.into(), 2);
    }
    if let Some(st) = nodes.iter().find(|x| s(x, "type") == "start") { visit(&s(st, "id"), &out, &mut state, &mut keep); }
    for x in nodes { let id = s(x, "id"); if !state.contains_key(&id) { visit(&id, &out, &mut state, &mut keep); } }
    keep
}

/// "Tidy the layout": layers by longest path over layoutEdges (ranksep 70, nodesep 50), each centred; End last.
pub fn tidy(f: &Value, heights: &HashMap<String, f32>) -> HashMap<String, (f64, f64)> {
    let nodes = arr(f, "nodes");
    let mut dag: HashMap<String, Vec<String>> = nodes.iter().map(|x| (s(x, "id"), vec![])).collect();
    for (a, b2) in layout_edges(f) { dag.get_mut(&a).unwrap().push(b2); }
    let mut order = vec![];
    let mut seen = HashSet::new();
    fn topo(id: &str, dag: &HashMap<String, Vec<String>>, seen: &mut HashSet<String>, order: &mut Vec<String>) {
        if !seen.insert(id.to_string()) { return; }
        for t in dag.get(id).cloned().unwrap_or_default() { topo(&t, dag, seen, order); }
        order.push(id.to_string());
    }
    for x in nodes { topo(&s(x, "id"), &dag, &mut seen, &mut order); }
    order.reverse();
    let mut layer: HashMap<String, i64> = HashMap::new();
    for id in &order { layer.entry(id.clone()).or_insert(0); }
    for id in &order { let l = layer[id]; for t in &dag[id] { let e = layer.entry(t.clone()).or_insert(0); *e = (*e).max(l + 1); } }
    if let Some(end) = nodes.iter().find(|x| s(x, "type") == "end") {
        let id = s(end, "id");
        let max = layer.values().copied().max().unwrap_or(0);
        let others = layer.iter().filter(|(k, _)| **k != id).map(|(_, v)| *v).max().unwrap_or(0);
        layer.insert(id, if layer.get(&s(end, "id")).copied().unwrap_or(0) == max && max > others { max } else { others + 1 });
    }
    let mut rows: Vec<(i64, Vec<String>)> = vec![];
    for x in nodes { let id = s(x, "id"); let l = layer.get(&id).copied().unwrap_or(0); match rows.iter_mut().find(|r| r.0 == l) { Some(r) => r.1.push(id), None => rows.push((l, vec![id])) } }
    rows.sort_by_key(|r| r.0);
    let ty: HashMap<String, String> = nodes.iter().map(|x| (s(x, "id"), s(x, "type"))).collect();
    let mut pos = HashMap::new();
    let mut y = 0.0f64;
    for (_, ids) in rows {
        let ws: Vec<f64> = ids.iter().map(|id| node_w(ty.get(id).map(|x| x.as_str()).unwrap_or("")) as f64).collect();
        let total: f64 = ws.iter().sum::<f64>() + 50.0 * (ids.len() as f64 - 1.0);
        let mut x = -total / 2.0;
        let mut row_h = 0.0f64;
        for (i, id) in ids.iter().enumerate() {
            pos.insert(id.clone(), (x.round(), y.round()));
            x += ws[i] + 50.0;
            row_h = row_h.max(heights.get(id).copied().unwrap_or(100.0) as f64);
        }
        y += row_h + 70.0;
    }
    pos
}

/// The Team list's meta: "Manager" or "2 steps", plus "· 1 to check".
pub fn person_meta(f: &Value, id: &str, issues: usize) -> String {
    let used = steps_of(f, id);
    format!("{}{}", if id == "manager" { "Manager".to_string() } else { format!("{used} step{}", if used == 1 { "" } else { "s" }) }, if issues > 0 { format!(" · {issues} to check") } else { String::new() })
}
pub fn steps_of(f: &Value, id: &str) -> usize { arr(f, "nodes").iter().filter(|x| s(x, "type") == "employee" && s(x, "employeeId") == id).count() }

/// newEmployee(): from a persona template (or blank), a name not yet taken.
pub fn new_employee(tpl: Option<&Value>, f: &Value, skills: Option<&[String]>) -> Value {
    let base = tpl.map(|t| s(t, "name")).unwrap_or_else(|| "New employee".into());
    let taken = |nm: &str| arr(f, "roster").iter().any(|r| s(r, "name").to_lowercase() == nm.to_lowercase());
    let mut name = base.clone();
    let mut i = 2;
    while taken(&name) { name = format!("{base} {i}"); i += 1; }
    let tskills: Vec<Value> = tpl.map(|t| arr(t, "skills").iter().filter(|k| skills.map(|l| l.iter().any(|x| Some(x.as_str()) == k.as_str())).unwrap_or(true)).cloned().collect()).unwrap_or_default();
    json!({ "id": gen_id("emp_"), "name": name, "persona": tpl.map(|t| s(t, "persona")).unwrap_or_default(), "model": "", "effort": "",
        "memory": tpl.map(|t| s(t, "memory")).filter(|m| !m.is_empty()).unwrap_or_else(|| "keep".into()),
        "tools": { "allow": [], "deny": tpl.map(|t| t["deny"].clone()).filter(|d| d.is_array()).unwrap_or(json!([])) },
        "skills": tskills, "presets": tpl.map(|t| t["presets"].clone()).filter(|d| d.is_array()).unwrap_or(json!([])) })
}

/// /skill-name mentions in a text (findMentions, factory-skills.js).
pub fn mentions(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let before_ok = i == 0 || " \t\n([{,;\"'`".contains(chars[i - 1]);
        if chars[i] == '/' && before_ok {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_lowercase() || chars[j].is_ascii_digit() || chars[j] == ':' || chars[j] == '-') { j += 1; }
            if j > i + 1 && chars.get(i + 1).map(|c| c.is_ascii_alphanumeric()).unwrap_or(false) {
                let name: String = chars[i + 1..j].iter().collect::<String>().trim_end_matches(['-', ':']).to_string();
                let after_ok = j >= chars.len() || !(chars[j] == '/' || chars[j].is_alphanumeric());
                if after_ok && !name.is_empty() && !out.contains(&name) { out.push(name); }
            }
            i = j.max(i + 1);
        } else { i += 1; }
    }
    out
}

/// The start form's precheck lines: (kind, text) with kind "err" | "note" | "warn" | "open".
pub fn precheck_lines(pre: &Value) -> Vec<(String, String)> {
    let mut out = vec![];
    if let Some(a) = pre.get("active").filter(|a| a.is_object()) {
        out.push(("err".into(), format!("A factory is already {} here (\"{}\"). One at a time per project.", if s(a, "status") == "crashed" { "waiting to be recovered" } else { "running" }, s(a, "factoryName"))));
        out.push(("open".into(), s(a, "id")));
        return out;
    }
    if !b(pre, "ok") { out.push(("err".into(), { let e = s(pre, "error"); if e.is_empty() { "Could not check the project.".into() } else { e } })); return out; }
    let br = s(pre, "branch");
    out.push(("note".into(), format!("The team works in its own worktree, on a new branch cut from {br}. Your checkout is not touched.")));
    if b(pre, "dirty") { out.push(("warn".into(), format!("You have uncommitted changes. The run starts from the last commit on {br}, so it will not see them."))); }
    out
}

pub fn missing_skills_text(list: &[String]) -> String {
    let one = list.len() == 1;
    format!("This factory names skill{} the project does not have: {}. You can install {} in Settings, under Skills, or start without.", if one { "" } else { "s" }, list.iter().map(|x| format!("/{x}")).collect::<Vec<_>>().join(", "), if one { "it" } else { "them" })
}

pub fn pin_sub(pass: bool) -> String {
    format!("This factory may use this secret. Its {} is kept in memory for this run only, so the team does not stop to ask; the value is never shown. Skip to be asked when it is first used.", if pass { "passphrase" } else { "PIN" })
}

pub fn cr_summary(cr: &Value) -> String {
    let n = arr(cr, "attachments").len();
    format!("Change request{}", if n > 0 { format!(" · {n} attachment{}", plural(n, "", "s")) } else { String::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chips_and_badge() {
        let r = json!({ "status": "running", "waitingOn": "ask" });
        assert_eq!(status_chip(&r), ("A question for you".to_string(), "waiting".to_string()));
        assert_eq!(chip_tone("waiting"), "warn");
        assert_eq!(status_chip(&json!({ "status": "finishing" })).0, "Done: your decision");
        let runs = vec![r, json!({ "status": "running", "waitingOn": "" }), json!({ "status": "crashed" }), json!({ "status": "finished" })];
        assert_eq!(badge(&runs), "2");
        assert_eq!(badge(&[]), "");
    }

    #[test]
    fn words() {
        assert_eq!(dur(5.0 * 60000.0), "5 min");
        assert_eq!(dur(125.0 * 60000.0), "2 h 5 min");
        assert_eq!(paused_text(&json!({ "by": "manager", "reason": "lunch" })), "Paused by the Manager: lunch");
        assert_eq!(paused_text(&json!({ "by": "you" })), "Paused.");
        assert_eq!(paused_text(&json!({ "by": "you", "resumeOnReset": true })), "Paused; it carries on by itself when the usage window resets.");
        assert_eq!(first("abc\ndef", 160), "abc");
        assert_eq!(first("abcdef", 4), "abc…");
        assert_eq!(fx_size(0.0), "0");
        assert_eq!(fx_size(2048.0), "2.0 KB");
        assert_eq!(export_name("My Team!"), "my-team-.factory.json");
        assert_eq!(cleanup_title(&json!({ "kind": "kept-branch", "branch": "x" })), "Kept branch x");
        assert_eq!(cleanup_warn(&json!({ "uncommitted": 1 })), "1 uncommitted change in it: deleting loses them.");
    }

    #[test]
    fn triggers() {
        let t = json!({ "kind": "schedule", "schedule": { "at": "09:00", "days": [1, 3] }, "needsCards": "todo" });
        assert_eq!(trigger_when(&t), "At 09:00 on Mon, Wed, if To do has a card");
        assert_eq!(trigger_when(&json!({ "kind": "kanban", "kanban": { "status": "in_progress" } })), "When a card enters In progress");
        assert_eq!(trigger_when(&json!({ "kind": "webhook" })), "When its webhook is called");
        assert_eq!(trigger_when(&json!({ "kind": "schedule", "schedule": { "at": "07:30", "days": [] } })), "At 07:30 every day");
    }

    #[test]
    fn steps_and_findings() {
        let run = json!({ "tokens": [{ "status": "work", "employee": "Dev", "label": "Build" }, { "status": "ask", "label": "Ship?" }, { "status": "work", "employee": "", "label": "x", "job": "Fix it", "blocked": true }] });
        let s = run_steps(&run);
        assert_eq!(s[0].1, "Dev · working on Build");
        assert_eq!(s[1].1, "You are asked a question");
        assert_eq!(s[2].1, "Someone · a job from the Manager: Fix it (waiting for a permission answer)");
        let (head, lines) = findings(&[json!({ "id": "F1", "status": "open", "blocking": true, "text": "bad", "where": "a.rs" }), json!({ "id": "F2", "status": "closed", "text": "ok" })], false);
        assert_eq!(head, "Review findings: 1 open of 2");
        assert_eq!(lines[0], "F1 open · blocking: bad (a.rs)");
        assert_eq!(lines[1], "F2 verified · nit: ok");
    }

    #[test]
    fn timeline() {
        let names: HashMap<String, String> = [("manager".to_string(), "Manager".to_string())].into();
        let nodes = HashMap::new();
        let l = describe(&json!({ "type": "handoff", "by": "manager", "payload": { "finish": true, "summary": "All done\nmore" } }), &names, &nodes).unwrap();
        assert_eq!((l.k.as_str(), l.main.as_str(), l.sub.as_str()), ("finish", "Manager finished", "All done"));
        assert!(describe(&json!({ "type": "session" }), &names, &nodes).is_none());
        assert_eq!(describe(&json!({ "type": "cap", "which": "maxMinutes", "limit": 60 }), &names, &nodes).unwrap().main, "Limit reached (60 minutes)");
        assert_eq!(describe(&json!({ "type": "pause", "by": "usage", "mode": "now" }), &names, &nodes).unwrap().main, "Paused: usage limit, turns in progress stopped");
    }

    #[test]
    fn nodes_and_edges() {
        let f = json!({ "roster": [{ "id": "manager", "name": "Boss" }, { "id": "e1", "name": "Dev", "memory": "fresh", "model": "opus" }],
            "nodes": [{ "id": "a", "type": "employee", "employeeId": "e1", "position": { "x": 0, "y": 0 } }, { "id": "d", "type": "decision", "label": "Ok?", "decision": { "kind": "many", "options": [{ "id": "o1", "label": "UI" }] }, "position": { "x": 0, "y": 200 } }],
            "edges": [{ "id": "back", "kind": "flow", "source": "d", "target": "a", "maxRounds": 3 }] });
        let a = &f["nodes"][0];
        assert_eq!(node_title(a, &f), "Dev");
        assert_eq!(node_body(a, &f), "opus · fresh each time");
        assert_eq!(outputs_of(&f["nodes"][1]), vec!["o1", "none"]);
        assert_eq!(handle_label(&f["nodes"][1], "o1"), "UI");
        assert_eq!(lanes(&f).get("back"), Some(&270.0));
        assert_eq!(edge_label(&f["edges"][0], Some(5)), "round 3/3");
        assert_eq!(edge_label(&f["edges"][0], None), "max 3");
        assert_eq!(wait_text(&json!({ "kind": "command", "command": "npm test", "timeout": 120 })), "Until `npm test` succeeds · 2 h max");
        let (p, lx, ly) = bezier(0.0, 0.0, 0.0, 100.0, false);
        assert!(p.starts_with("M 0.0 0.0 C"));
        assert_eq!((lx, ly), (0.0, 50.0));
    }

    #[test]
    fn editing() {
        let mut f = json!({ "nodes": [{ "id": "start", "type": "start" }, { "id": "m", "type": "manager" }, { "id": "a", "type": "employee" }, { "id": "end", "type": "end" }], "edges": [] });
        assert!(can_connect(&f, "start", "out", "m", "in"));
        assert!(!can_connect(&f, "m", "out", "end", "in"));
        assert!(can_connect(&f, "m", "finish", "end", "in"));
        assert!(!can_connect(&f, "a", "out", "start", "in"));
        assert!(can_connect(&f, "m", "note", "a", "notein"));
        assert!(!connect(&mut f, "m", "out", "a"));
        assert!(connect(&mut f, "a", "out", "m"));
        assert_eq!(f["edges"][1]["maxRounds"], 3);
        remove_nodes(&mut f, &["a".to_string(), "m".to_string()]);
        assert_eq!(arr(&f, "nodes").len(), 3);
        assert_eq!(arr(&f, "edges").len(), 0);
        let n = new_node("decision:one", 16.0, 32.0);
        assert_eq!(arr(&n["decision"], "options").len(), 2);
        assert_eq!(mentions("use /frontend-design and /run, not a/b"), vec!["frontend-design", "run"]);
    }

    #[test]
    fn layout_and_fit() {
        let f = json!({ "nodes": [{ "id": "s", "type": "start" }, { "id": "m", "type": "manager" }, { "id": "a", "type": "employee" }, { "id": "b", "type": "employee" }, { "id": "e", "type": "end" }],
            "edges": [{ "kind": "flow", "source": "s", "target": "m" }, { "kind": "flow", "source": "m", "target": "a" }, { "kind": "flow", "source": "m", "target": "b" }, { "kind": "flow", "source": "a", "target": "m", "maxRounds": 2 }, { "kind": "flow", "source": "m", "target": "e" }] });
        let pos = tidy(&f, &HashMap::new());
        assert!(pos["s"].1 < pos["m"].1 && pos["m"].1 < pos["a"].1 && pos["a"].1 == pos["b"].1 && pos["e"].1 > pos["a"].1);
        let (z, px, _) = fit(Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 }, 200.0, 200.0, 0.0, 0.1, 1.0);
        assert_eq!((z, px), (1.0, 50.0));
    }
}
