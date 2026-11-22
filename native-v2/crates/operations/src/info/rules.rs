//! Info's pure rules (the Slint client's src/ops/info.rs and the helpers it
//! took from skills/common.rs): the web's fmtBytes / fmtTok / parseTok,
//! fmtPct / fmtDur, sparkSvg, renderSystemStats' rows, renderMaintenance,
//! the collaborators' cards, showExpiry, the extend prompt, offerGitInit's
//! words, and the en-US local dates the PC client shows.

use chrono::{Datelike, Local, NaiveDateTime, TimeZone, Timelike};
use serde_json::Value;

// ─── Reading the server's JSON ───

pub fn s(v: &Value, k: &str) -> String {
    match v.get(k) { Some(Value::String(x)) => x.clone(), Some(Value::Null) | None => String::new(), Some(o) => o.to_string() }
}
pub fn n(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0)
}
pub fn truthy(v: &Value) -> bool {
    match v { Value::Null => false, Value::Bool(b) => *b, Value::String(x) => !x.is_empty(), Value::Number(x) => x.as_f64().is_some_and(|f| f != 0.0), _ => true }
}
pub fn flag(v: &Value, k: &str) -> bool { v.get(k).is_some_and(truthy) }
pub fn ok(d: &Value) -> bool { d.get("ok").and_then(|x| x.as_bool()).unwrap_or(false) }
/// The web's `{ ok:false, error }` → its words (or `fallback`).
pub fn err_of(d: &Value, fallback: &str) -> String { let e = s(d, "error"); if e.is_empty() { fallback.into() } else { e } }
fn strs(v: &Value, k: &str) -> Vec<String> {
    v.get(k).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

/// A time field as ms: a number (ms) or an ISO date.
pub fn ms_of(v: &Value, k: &str) -> Option<f64> {
    match v.get(k)? {
        Value::Number(x) => x.as_f64(),
        Value::String(t) => t.parse::<f64>().ok().or_else(|| crundi_core::time::parse_iso(t).map(|x| x * 1000.0)),
        _ => None,
    }
}

// ─── Numbers ───

/// fmtBytes: "512 B", "1.5 KB", "120 MB".
pub fn fmt_bytes(x: f64) -> String {
    if !x.is_finite() { return "—".into(); }
    if x < 1024.0 { return format!("{} B", x.round() as i64); }
    let u = ["KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (x / 1024.0, 0);
    while v >= 1024.0 && i < u.len() - 1 { v /= 1024.0; i += 1; }
    if v >= 100.0 { format!("{} {}", v.round() as i64, u[i]) } else { format!("{:.1} {}", v, u[i]) }
}

/// fmtTok: 2M, 500k, 950.
pub fn fmt_tok(x: f64) -> String {
    if x >= 1e6 { let m = (x / 1e5).round() / 10.0; if m.fract() == 0.0 { format!("{}M", m as i64) } else { format!("{m}M") } }
    else if x >= 1e3 { format!("{}k", (x / 1e3).round() as i64) }
    else { format!("{}", x as i64) }
}

/// parseTok: "2M", "500k", "1,500,000" → tokens.
pub fn parse_tok(raw: &str) -> Option<f64> {
    let t = raw.trim().to_lowercase().replace(',', "");
    let (num, mul) = if let Some(x) = t.strip_suffix('m') { (x.trim(), 1e6) } else if let Some(x) = t.strip_suffix('k') { (x.trim(), 1e3) } else { (t.as_str(), 1.0) };
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit() || c == '.') || num.matches('.').count() > 1 || num.starts_with('.') || num.ends_with('.') { return None; }
    num.parse::<f64>().ok().map(|v| (v * mul).round())
}

/// fmtPct: "12.5%", "100%", "—".
pub fn fmt_pct(x: Option<f64>) -> String {
    match x { None => "—".into(), Some(x) if x >= 100.0 => format!("{}%", x.round() as i64), Some(x) => format!("{x:.1}%") }
}

/// fmtDur: "2d 3h", "4h 5m", "6m".
pub fn fmt_dur(sec: Option<f64>) -> String {
    let Some(sec) = sec.filter(|x| x.is_finite()) else { return "—".into() };
    let (d, h, m) = ((sec / 86400.0).floor() as i64, ((sec % 86400.0) / 3600.0).floor() as i64, ((sec % 3600.0) / 60.0).floor() as i64);
    if d > 0 { format!("{d}d {h}h") } else if h > 0 { format!("{h}h {m}m") } else { format!("{m}m") }
}

/// sparkSvg's points: x 0..100, y 0..20 (1 at the top for `max`, 19 at 0);
/// empty below two points.
pub fn spark(vals: &[f64], max: f64) -> Vec<(f32, f32)> {
    if vals.len() < 2 || max <= 0.0 { return vec![]; }
    let k = vals.len();
    vals.iter().enumerate().map(|(i, v)| ((i as f64 / (k - 1) as f64 * 100.0) as f32, (19.0 - (v / max).clamp(0.0, 1.0) * 18.0) as f32)).collect()
}

/// lvl: green until it matters, 60 % amber (1), 85 % red (2).
pub fn level(pct: f64) -> u8 { if pct >= 85.0 { 2 } else if pct >= 60.0 { 1 } else { 0 } }

// ─── Dates (en-US, as the PC client shows them) ───

const MON: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAY: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

fn local(ms: f64) -> Option<NaiveDateTime> { Local.timestamp_millis_opt(ms as i64).single().map(|d| d.naive_local()) }
fn h12(t: &NaiveDateTime) -> (u32, &'static str) { let h = t.hour(); (if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" }) }

/// toLocaleTimeString(): "3:04:05 PM".
pub fn time_at(t: &NaiveDateTime) -> String { let (h, ap) = h12(t); format!("{}:{:02}:{:02} {}", h, t.minute(), t.second(), ap) }
pub fn time(ms: f64) -> String { local(ms).map(|t| time_at(&t)).unwrap_or_default() }

/// The invite's "Ends …": under two days the clock ("Sat 3:04 PM"), else the day ("Sat, Nov 21").
pub fn ends_at(t: &NaiveDateTime, short: bool) -> String {
    let wd = DAY[t.weekday().num_days_from_sunday() as usize];
    if short { let (h, ap) = h12(t); format!("{wd} {h}:{:02} {ap}", t.minute()) } else { format!("{wd}, {} {}", MON[t.month0() as usize], t.day()) }
}
pub fn ends(ms: f64, short: bool) -> String { local(ms).map(|t| ends_at(&t, short)).unwrap_or_default() }

/// The passcode dialog's "Access ends": "Sat, Nov 21, 3:04 PM".
pub fn ends_long(ms: f64) -> String {
    local(ms).map(|t| { let (h, ap) = h12(&t); format!("{}, {} {}, {}:{:02} {}", DAY[t.weekday().num_days_from_sunday() as usize], MON[t.month0() as usize], t.day(), h, t.minute(), ap) }).unwrap_or_default()
}

// ─── This machine ───

/// A row of the machine panel after the CPU block.
#[derive(Clone, Debug, PartialEq)]
pub enum SysRow { Line(String, String), Bar(f64), Gap }

/// renderSystemStats' rows.
pub fn sys_rows(sys: &Value) -> Vec<SysRow> {
    use SysRow::*;
    let o = |k: &str| sys.get(k).cloned().unwrap_or_default();
    let (mem, disk, host) = (o("mem"), o("disk"), o("host"));
    let f = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_f64());
    let pct = |u: Option<f64>, t: Option<f64>| match (u, t) { (Some(u), Some(t)) if t > 0.0 => (u / t * 100.0).clamp(0.0, 100.0), _ => 0.0 };
    let b = |v: Option<f64>| v.map(fmt_bytes).unwrap_or_else(|| "—".into());
    let line = |k: &str, v: String| Line(k.into(), v);
    let mut out = vec![Gap, line("Memory", format!("{} of {}", b(f(&mem, "used")), b(f(&mem, "total")))), Bar(pct(f(&mem, "used"), f(&mem, "total")))];
    if f(&mem, "swapTotal").unwrap_or(0.0) > 0.0 {
        out.extend([Gap, line("Swap", format!("{} of {}", b(f(&mem, "swapUsed")), b(f(&mem, "swapTotal")))), Bar(pct(f(&mem, "swapUsed"), f(&mem, "swapTotal")))]);
    }
    out.extend([Gap, line("Disk", format!("{} of {}", b(f(&disk, "used")), b(f(&disk, "total")))), Bar(pct(f(&disk, "used"), f(&disk, "total"))), Gap]);
    if let Some(l) = sys.get("load").filter(|x| x.is_object()) {
        out.push(line("Load", format!("{:.2}  {:.2}  {:.2}", n(l, "one"), n(l, "five"), n(l, "fifteen"))));
    }
    if let Some(x) = sys.get("net").filter(|x| x.get("rxPerSec").is_some_and(|v| !v.is_null())) {
        out.push(line("Traffic", format!("down {}/s   up {}/s", fmt_bytes(n(x, "rxPerSec")), fmt_bytes(n(x, "txPerSec")))));
    }
    out.push(line("Uptime", fmt_dur(f(sys, "uptimeSec"))));
    out.push(line("Crundi", format!("{} · {}", fmt_dur(f(sys, "processUptimeSec")), b(f(sys, "processRssBytes")))));
    let hn = s(&host, "hostname");
    out.push(line("Machine", if hn.is_empty() { "—".into() } else { hn }));
    let cm = s(&host, "cpuModel");
    out.push(line("Processor", format!("{} cores · {}", n(&host, "cores") as i64, if cm.is_empty() { "—" } else { &cm })));
    let ifs = sys.get("interfaces").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    if !ifs.is_empty() {
        out.push(Gap);
        for i in &ifs {
            let addrs = i.get("addresses").and_then(|x| x.as_array()).map(|a| a.iter().map(|x| s(x, "address")).collect::<Vec<_>>().join("  ")).unwrap_or_default();
            out.push(line(&s(i, "name"), addrs));
        }
    }
    out
}

/// A log line: "[3:04:05 PM] words" and its level.
pub fn log_line(l: &Value) -> (String, String) {
    let t = ms_of(l, "time").map(time).unwrap_or_default();
    (format!("[{t}] {}", s(l, "text")), s(l, "level"))
}

// ─── Reclaim disk space ───

#[derive(Clone, Debug, Default)]
pub struct Task { pub id: String, pub label: String, pub root: bool, pub danger: bool, pub desc: String, pub detail: String, pub size: String, pub none: bool, pub enabled: bool, pub arm: String, pub busy: String }

/// renderMaintenance: the total line and one row per task.
pub fn maint_rows(tasks: &[Value], busy: &str) -> (String, Vec<Task>) {
    let total: f64 = tasks.iter().filter(|t| flag(t, "available")).map(|t| n(t, "bytes")).sum();
    let head = if total > 0.0 { format!("{} can be reclaimed", fmt_bytes(total)) } else { "Nothing to reclaim right now".into() };
    let rows = tasks.iter().map(|t| {
        let names = strs(t, "details");
        let (avail, bytes, danger, k, id) = (flag(t, "available"), n(t, "bytes"), flag(t, "danger"), names.len(), s(t, "id"));
        Task {
            label: s(t, "label"), root: flag(t, "elevated"), danger, desc: s(t, "description"),
            detail: if k > 0 { format!("Will remove: {}", names.join(", ")) } else { String::new() },
            size: if avail { fmt_bytes(bytes) } else { "—".into() },
            none: bytes <= 0.0,
            enabled: avail && (bytes > 0.0 || k > 0),
            arm: if k > 0 { format!("Remove {k}{}", if k == 1 { " container?" } else { " containers?" }) } else if danger { "Prune it?".into() } else { "Clear it?".into() },
            busy: if busy == id { (if danger { "Removing…" } else { "Clearing…" }).into() } else { String::new() },
            id,
        }
    }).collect();
    (head, rows)
}

// ─── Outside collaborators ───

/// collabTimeLeft.
pub fn time_left(ms: f64, now: f64) -> String {
    let left = ms - now;
    if left <= 0.0 { return "expired".into(); }
    let d = (left / 86400000.0).floor() as i64;
    if d >= 1 { return format!("{d}{}", if d == 1 { " day left" } else { " days left" }); }
    let h = (left / 3600000.0).floor() as i64;
    if h >= 1 { return format!("{h}{}", if h == 1 { " hour left" } else { " hours left" }); }
    let m = ((left / 60000.0).round() as i64).max(1);
    format!("{m}{}", if m == 1 { " minute left" } else { " minutes left" })
}

#[derive(Clone, Debug, Default)]
pub struct Grant { pub id: String, pub project: String, pub branch: String, pub exp: String, pub gone: bool, pub revoked: bool }

#[derive(Clone, Debug, Default)]
pub struct Person {
    pub name: String, pub tg: String, pub id: String, pub usage: String, pub over: bool,
    /// The token meter 0..1 (None: no limit).
    pub meter: Option<f32>,
    pub limit_btn: String, pub can_reset: bool,
    /// The highest limit of their grants (the "Change limit" prompt's value).
    pub limit: f64,
    pub sites: Vec<String>, pub rows: Vec<Grant>,
}

/// One card per person (by Telegram name, else by name), their grants under.
pub fn people_of(list: &[Value], now: f64) -> Vec<Person> {
    let mut order: Vec<(String, Vec<Value>)> = Vec::new();
    for x in list {
        let tg = s(x, "telegram");
        let key = if tg.is_empty() { format!("pc:{}", s(x, "name").to_lowercase()) } else { format!("tg:{tg}") };
        match order.iter().position(|(k, _)| *k == key) { Some(i) => order[i].1.push(x.clone()), None => order.push((key, vec![x.clone()])) }
    }
    order.into_iter().map(|(_, rows)| {
        let first = &rows[0];
        let tg = s(first, "telegram");
        let used: f64 = rows.iter().map(|r| n(r, "tokensUsed")).sum();
        let lim = rows.iter().map(|r| n(r, "tokenLimit")).fold(0.0, f64::max);
        let over = lim > 0.0 && used >= lim;
        let mut sites: Vec<String> = Vec::new();
        for r in &rows { for d in strs(r, "allowedDomains") { if !sites.contains(&d) { sites.push(d); } } }
        sites.sort();
        Person {
            name: s(first, "name"),
            tg: if tg.is_empty() { "passcode only".into() } else { format!("@{tg}") },
            id: s(first, "id"),
            usage: format!("{} tokens{}{}", fmt_tok(used), if lim > 0.0 { format!(" of {}", fmt_tok(lim)) } else { " · no limit".into() }, if over { " · limit reached" } else { "" }),
            over,
            meter: (lim > 0.0).then(|| (used / lim).min(1.0) as f32),
            limit_btn: if lim > 0.0 { "Change limit".into() } else { "Set limit".into() },
            can_reset: used > 0.0,
            limit: lim,
            sites,
            rows: rows.iter().map(|r| Grant {
                id: s(r, "id"), project: s(r, "project"), branch: s(r, "branch"),
                exp: if flag(r, "revoked") { "revoked".into() } else { time_left(n(r, "expiresAt"), now) },
                gone: flag(r, "expired"), revoked: flag(r, "revoked"),
            }).collect(),
        }
    }).collect()
}

/// showExpiry: when the access would end, or what is wrong with the amount.
pub fn expiry_text(amount: &str, hours_unit: bool, now: f64, ends: &dyn Fn(f64, bool) -> String) -> String {
    let unit = if hours_unit { "hours" } else { "days" };
    let k = amount.trim().parse::<f64>().ok().map(|x| x.trunc() as i64).unwrap_or(0);
    if k < 1 { return format!("Enter how long, at least 1 {}.", unit.trim_end_matches('s')); }
    let hours = if hours_unit { k } else { k * 24 };
    if hours > 8760 { return "That is over a year. The most is 365 days.".into(); }
    format!("Ends {}. You can extend or revoke it at any time.", ends(now + hours as f64 * 3600000.0, hours < 48))
}

/// "6h", "3d" or "7" (days) → hours (collab-extend's prompt).
pub fn parse_extend(raw: &str) -> Option<f64> {
    let t = raw.trim();
    let num: String = t.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let x: f64 = num.parse().ok()?;
    if x <= 0.0 { return None; }
    let unit = t[num.len()..].trim_start().chars().next().map(|c| c.to_ascii_lowercase());
    Some(if unit == Some('h') { x } else { x * 24.0 })
}

/// showInviteModal's "Copy everything" block.
pub fn invite_block(url: &str, name: &str, pass: &str, projects: &[String], ends: &str) -> String {
    let mut b = vec![format!("Crundi access for {name}"), format!("Sign in: {url}"), format!("Name: {name}"), format!("Passcode: {pass}")];
    if !projects.is_empty() { b.push(format!("Projects: {}", projects.join(", "))); }
    if !ends.is_empty() { b.push(format!("Access ends: {ends}")); }
    b.push("Open the link, choose \"I was invited to a project\", and use the name and passcode above.".into());
    b.join("\n")
}

/// The git dialog's lines: would commit, kept out, looks sensitive, the .gitignore.
pub fn git_words(p: &Value) -> (String, String, String, String) {
    let mb = n(p, "bytes") / 1048576.0;
    let mut commit = format!("Would commit: {} files, {mb:.1} MB", n(p, "files") as i64);
    if flag(p, "truncated") { commit += " (stopped counting at 20,000)"; }
    let ign = strs(p, "ignoring");
    if !ign.is_empty() { commit += &format!("\nSkipping: {}", ign.join(", ")); }
    let list = |k: &str| p.get(k).and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let paths = |v: &[Value]| v.iter().map(|x| format!("{} ({})", s(x, "path"), s(x, "why"))).collect::<Vec<_>>().join("\n");
    let ex = list("excluded");
    let kept = if ex.is_empty() { String::new() } else { format!("Kept out of git:\n{}", paths(&ex)) };
    let rk = list("risky");
    let risky = if rk.is_empty() { String::new() } else { format!("These would be committed and look sensitive:\n{}\n\nMove or delete them first if that is not what you want.", paths(&rk)) };
    let tail = if flag(p, "wouldWriteIgnore") { "A .gitignore will be written first, keeping dependencies, build output and secrets out. You can edit it afterwards." } else { "This project already has a .gitignore, so git decides what is included." };
    (commit, kept, risky, tail.into())
}

#[cfg(test)]
mod tests {
    use super::{expiry_text, fmt_bytes, fmt_dur, fmt_pct, fmt_tok, git_words, invite_block, level, maint_rows, parse_extend, parse_tok, people_of, spark, sys_rows, time_left, SysRow};
    use serde_json::json;

    #[::core::prelude::v1::test]
    fn numbers() {
        assert_eq!(fmt_pct(Some(12.345)), "12.3%");
        assert_eq!(fmt_pct(Some(150.0)), "150%");
        assert_eq!(fmt_pct(None), "—");
        assert_eq!(fmt_dur(Some(2.0 * 86400.0 + 3.0 * 3600.0)), "2d 3h");
        assert_eq!(fmt_dur(Some(3700.0)), "1h 1m");
        assert_eq!(fmt_dur(Some(59.0)), "0m");
        assert_eq!(fmt_bytes(512.0), "512 B");
        assert_eq!(fmt_bytes(1536.0), "1.5 KB");
        assert_eq!(fmt_tok(2_000_000.0), "2M");
        assert_eq!(fmt_tok(500_000.0), "500k");
        assert_eq!(parse_tok("2M"), Some(2e6));
        assert_eq!(parse_tok("1,500"), Some(1500.0));
        assert_eq!(parse_tok("x"), None);
        assert_eq!((level(10.0), level(60.0), level(85.0)), (0, 1, 2));
    }

    #[::core::prelude::v1::test]
    fn sparkline() {
        assert!(spark(&[50.0], 100.0).is_empty());
        assert_eq!(spark(&[0.0, 100.0], 100.0), vec![(0.0, 19.0), (100.0, 1.0)]);
    }

    #[::core::prelude::v1::test]
    fn machine_rows() {
        let r = sys_rows(&json!({ "mem": { "used": 1024, "total": 2048, "swapTotal": 0 }, "disk": { "used": 1, "total": 4 }, "uptimeSec": 60, "host": { "hostname": "box", "cores": 4, "cpuModel": "X" }, "interfaces": [{ "name": "eth0", "addresses": [{ "address": "10.0.0.2" }] }] }));
        let lines: Vec<(String, String)> = r.iter().filter_map(|x| if let SysRow::Line(k, v) = x { Some((k.clone(), v.clone())) } else { None }).collect();
        assert_eq!(lines[0], ("Memory".into(), "1.0 KB of 2.0 KB".into()));
        assert!(lines.iter().all(|l| l.0 != "Swap" && l.0 != "Load" && l.0 != "Traffic"));
        assert!(lines.contains(&("Processor".into(), "4 cores · X".into())));
        assert_eq!(lines.last().unwrap(), &("eth0".to_string(), "10.0.0.2".to_string()));
        let bars: Vec<f64> = r.iter().filter_map(|x| if let SysRow::Bar(p) = x { Some(*p) } else { None }).collect();
        assert_eq!(bars, vec![50.0, 25.0]);
    }

    #[::core::prelude::v1::test]
    fn reclaim() {
        let (head, rows) = maint_rows(&[
            json!({ "id": "a", "label": "Cache", "available": true, "bytes": 2048, "description": "d" }),
            json!({ "id": "b", "label": "Containers", "available": true, "bytes": 0, "danger": true, "details": ["x", "y"] }),
            json!({ "id": "c", "label": "Gone", "available": false, "bytes": 0 }),
        ], "a");
        assert_eq!(head, "2.0 KB can be reclaimed");
        assert_eq!(rows[0].busy, "Clearing…");
        assert_eq!(rows[1].arm, "Remove 2 containers?");
        assert!(rows[1].enabled && rows[1].detail == "Will remove: x, y");
        assert!(!rows[2].enabled && rows[2].size == "—");
        assert_eq!(maint_rows(&[], "").0, "Nothing to reclaim right now");
    }

    #[::core::prelude::v1::test]
    fn collaborators() {
        let now = 1_000_000_000.0;
        assert_eq!(time_left(now - 1.0, now), "expired");
        assert_eq!(time_left(now + 2.5 * 86400000.0, now), "2 days left");
        assert_eq!(time_left(now + 3600000.0 * 1.5, now), "1 hour left");
        assert_eq!(time_left(now + 1000.0, now), "1 minute left");
        let p = people_of(&[
            json!({ "id": "1", "name": "Sam", "project": "a", "expiresAt": now + 86400000.0 * 3.0, "tokensUsed": 600000, "tokenLimit": 500000, "allowedDomains": ["b.com", "a.com"] }),
            json!({ "id": "2", "name": "sam", "project": "b", "revoked": true }),
            json!({ "id": "3", "name": "Ann", "telegram": "ann", "project": "a" }),
        ], now);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].usage, "600k tokens of 500k · limit reached");
        assert!(p[0].over && p[0].meter == Some(1.0) && p[0].limit == 500000.0);
        assert_eq!(p[0].rows.len(), 2);
        assert_eq!(p[0].sites[0], "a.com");
        assert_eq!(p[0].rows[1].exp, "revoked");
        assert_eq!(p[1].tg, "@ann");
        assert_eq!(p[1].usage, "0 tokens · no limit");
        assert_eq!(p[1].limit_btn, "Set limit");
    }

    #[::core::prelude::v1::test]
    fn invite_words() {
        let ends = |ms: f64, short: bool| format!("{}{}", ms as i64, if short { "s" } else { "l" });
        assert_eq!(expiry_text("0", false, 0.0, &ends), "Enter how long, at least 1 day.");
        assert_eq!(expiry_text("0", true, 0.0, &ends), "Enter how long, at least 1 hour.");
        assert_eq!(expiry_text("400", false, 0.0, &ends), "That is over a year. The most is 365 days.");
        assert_eq!(expiry_text("4", true, 0.0, &ends), "Ends 14400000s. You can extend or revoke it at any time.");
        assert_eq!(expiry_text("7", false, 0.0, &ends), "Ends 604800000l. You can extend or revoke it at any time.");
        assert_eq!(parse_extend("6h"), Some(6.0));
        assert_eq!(parse_extend("3d"), Some(72.0));
        assert_eq!(parse_extend("7"), Some(168.0));
        assert_eq!(parse_extend("x"), None);
        let (c1, k, r, t) = git_words(&json!({ "files": 12, "bytes": 1048576, "excluded": [{ "path": ".env", "why": "secrets" }], "wouldWriteIgnore": true }));
        assert_eq!(c1, "Would commit: 12 files, 1.0 MB");
        assert_eq!(k, "Kept out of git:\n.env (secrets)");
        assert!(r.is_empty() && t.starts_with("A .gitignore"));
        let b = invite_block("https://x", "Sam", "p1", &["a".into()], "");
        assert_eq!(b.lines().count(), 6);
        assert!(b.contains("Passcode: p1") && b.contains("Projects: a"));
    }
}
