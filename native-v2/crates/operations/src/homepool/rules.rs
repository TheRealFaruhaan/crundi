//! Homepool's pure rules (the web's helpers in app/vendor/crundi-homepool.js;
//! the Slint client's src/ops/homepool.rs): numbers and times in words, the
//! activity log's words, and the screen's rows built from GET /api/homepool.

use serde_json::Value;

/// A value as text: a string as it is, a number without a needless ".0".
pub fn ss(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}
/// A number (a numeric string too); 0 otherwise.
pub fn n(v: &Value, k: &str) -> f64 {
    match v.get(k) {
        Some(Value::Number(x)) => x.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(Value::Bool(true)) => 1.0,
        _ => 0.0,
    }
}
/// JavaScript's truthiness.
pub fn t(v: &Value, k: &str) -> bool {
    match v.get(k) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(x)) => x.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}
pub fn arr<'a>(v: &'a Value, k: &str) -> Vec<&'a Value> { v.get(k).and_then(|x| x.as_array()).map(|a| a.iter().collect()).unwrap_or_default() }
fn strs(v: &Value, k: &str) -> Vec<String> { arr(v, k).into_iter().map(|x| x.as_str().unwrap_or("").to_string()).collect() }

/// bytes(): "512 B", "12 KB", "3.4 MB", "1.25 GB".
pub fn bytes(b: f64) -> String {
    if b >= 1073741824.0 { format!("{:.2} GB", b / 1073741824.0) }
    else if b >= 1048576.0 { format!("{:.1} MB", b / 1048576.0) }
    else if b >= 1024.0 { format!("{} KB", (b / 1024.0).round() as i64) }
    else { format!("{} B", b as i64) }
}

/// dur(): "2d 3h", "4h 5m", "12m", "30s".
pub fn dur(ms: f64) -> String {
    let s = (ms / 1000.0).floor().max(0.0) as i64;
    if s >= 86400 { format!("{}d {}h", s / 86400, (s % 86400) / 3600) }
    else if s >= 3600 { format!("{}h {}m", s / 3600, (s % 3600) / 60) }
    else if s >= 60 { format!("{}m", s / 60) }
    else { format!("{s}s") }
}

/// ago(): "<dur> ago", or "never".
pub fn ago_at(now: f64, t: f64) -> String { if t > 0.0 { format!("{} ago", dur(now - t)) } else { "never".into() } }

/// day(): a local date and time.
pub fn day(ms: f64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64).map(|d| d.with_timezone(&chrono::Local).format("%-d/%-m/%Y, %H:%M:%S").to_string()).unwrap_or_default()
}

pub fn kind_label(k: &str) -> &str { match k { "chat" => "Chat", "project" => "Project", "software" => "Software", x => x } }

/// The log's words for an event (LOG).
pub fn log_what(w: &str) -> String {
    match w {
        "network-added" => "added network", "network-renamed" => "renamed network", "network-removed" => "removed network",
        "node-added" => "added node", "node-moved" => "moved node", "node-enabled" => "switched node on", "node-disabled" => "switched node off",
        "node-key-reissued" => "gave node a new key", "node-removed" => "removed node", "node-up" => "node connected", "node-down" => "node disconnected",
        "key-added" => "gave access", "key-changed" => "changed access", "key-reissued" => "new secret for", "key-revoked" => "revoked access",
        "request" => "a chat asked for access", "request-approved" => "approved request", "request-refused" => "refused request",
        "refused-login" => "refused a login", "settings-changed" => "changed settings",
        x => x,
    }.to_string()
}

/// whoOf(): who a key is for.
pub fn who_of(k: &Value) -> String {
    match ss(k, "kind").as_str() {
        "chat" => { let p = ss(k, "project"); format!("chat {}{}", ss(k, "chatId").chars().take(8).collect::<String>(), if p.is_empty() { String::new() } else { format!(" in {p}") }) }
        "project" => format!("every chat of {}", ss(k, "project")),
        _ => "software".into(),
    }
}

/// A local "YYYY-MM-DD HH:MM" (or with a T) as ms since 1970; None when empty or unreadable.
pub fn parse_local(s: &str) -> Option<f64> {
    let s = s.trim().replace('T', " ");
    if s.is_empty() { return None; }
    let n = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M").ok()?;
    use chrono::TimeZone;
    chrono::Local.from_local_datetime(&n).single().map(|d| d.timestamp_millis() as f64)
}

pub fn show_local(ms: f64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
}

/// The QR's dark runs (x, y, width; one module high) and its size in
/// modules, from the server's SVG (src/qr.js toSvg: "M{x} {y}h{run}v1h-{run}z").
pub fn qr_runs(svg: &str) -> Option<(Vec<(f32, f32, f32)>, f32)> {
    let vb = svg.split("viewBox=\"").nth(1)?.split('"').next()?;
    let size: f32 = vb.split_whitespace().nth(2)?.parse().ok()?;
    let d = svg.split(" d=\"").nth(1)?.split('"').next()?;
    let mut runs = vec![];
    for part in d.split('M').filter(|p| !p.is_empty()) {
        let (xy, rest) = part.split_once('h')?;
        let mut it = xy.split_whitespace();
        let x: f32 = it.next()?.parse().ok()?;
        let y: f32 = it.next()?.parse().ok()?;
        let w: f32 = rest.split('v').next()?.parse().ok()?;
        runs.push((x, y, w));
    }
    if runs.is_empty() { return None; }
    Some((runs, size))
}

// ─── The rows ───

/// A line of "how to use" or a dialog: "quiet" words, "warn", or "code" (with its Copy).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Use { pub label: String, pub kind: &'static str, pub text: String }
pub fn use_line(label: &str, kind: &'static str, text: &str) -> Use { Use { label: label.into(), kind, text: text.into() } }

#[derive(Clone, Debug, Default)]
pub struct Tab { pub key: &'static str, pub label: &'static str, pub count: String, pub alert: bool }
#[derive(Clone, Debug, Default)]
pub struct Node { pub id: String, pub name: String, pub dot: &'static str, pub state: String, pub tone: &'static str, pub meta: String, pub net: usize, pub on: bool }
#[derive(Clone, Debug, Default)]
pub struct Net { pub id: String, pub name: String, pub kind: &'static str, pub chip: String, pub tone: &'static str, pub on: bool, pub body: String, pub err: String, pub nodes: Vec<Node> }
#[derive(Clone, Debug, Default)]
pub struct Key { pub id: String, pub name: String, pub nets: Vec<String>, pub state: String, pub tone: &'static str, pub on: bool, pub meta: String, pub open: bool, pub software: bool, pub uses: Vec<Use> }
#[derive(Clone, Debug, Default)]
pub struct Req { pub id: String, pub title: String, pub nets: Vec<String>, pub status: String, pub tone: &'static str, pub pending: bool, pub meta: String, pub reason: String }
#[derive(Clone, Debug, Default)]
pub struct Log { pub at: String, pub what: String, pub subj: String }

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub tabs: Vec<Tab>,
    pub sum: Vec<(String, String)>,
    pub nets: Vec<Net>,
    pub loose: Vec<Node>,
    /// A node's network select: no network, then the owner's networks.
    pub move_opts: Vec<String>,
    pub keys: Vec<Key>,
    pub reqs: Vec<Req>,
    pub log: Vec<Log>,
}

pub fn net_name(d: &Value, id: &str) -> String {
    if id.is_empty() { return "No network".into(); }
    arr(d, "networks").into_iter().find(|n| ss(n, "id") == id).map(|n| ss(n, "name")).unwrap_or_else(|| id.to_string())
}

/// The owner's own networks (not the server, not the bridge).
pub fn home_nets(d: &Value) -> Vec<Value> {
    arr(d, "networks").into_iter().filter(|n| { let i = ss(n, "id"); i != "server" && i != "pool" }).cloned().collect()
}

fn node_row(d: &Value, nd: &Value, now: f64) -> Node {
    let i = nd.get("info").cloned().unwrap_or_default();
    let enabled = nd.get("enabled") != Some(&Value::Bool(false));
    let (up, paused) = (t(nd, "up"), t(nd, "paused"));
    let dot = if !enabled { "" } else if up { if paused { "paused" } else { "up" } } else { "" };
    let reason = ss(&i, "pauseReason");
    let (state, tone) = if !enabled { ("switched off".to_string(), "") } else if up {
        if paused { (format!("paused{}", if reason.is_empty() { String::new() } else { format!(": {reason}") }), "warn") } else { ("connected".into(), "ok") }
    } else { ("offline".into(), "") };
    let mut meta = Vec::new();
    let plat = { let p = ss(&i, "platform"); if p.is_empty() { ss(nd, "platform") } else { p } };
    meta.push(format!("{}{}", if ss(nd, "kind") == "crundi-app" { "Crundi app" } else { "Standalone node" }, if plat.is_empty() { String::new() } else { format!(" · {plat}") }));
    if up {
        if !ss(&i, "network").is_empty() { meta.push(ss(&i, "network")); }
        if !ss(&i, "publicIp").is_empty() { meta.push(format!("IP {}", ss(&i, "publicIp"))); }
        meta.push(format!("up {}", dur(now - n(nd, "connectedAt"))));
        meta.push(format!("{} open", n(nd, "streams") as i64));
    } else {
        meta.push(format!("last seen {}", ago_at(now, n(nd, "lastSeenAt"))));
    }
    let slots = n(nd, "slots") as i64;
    meta.push(format!("{slots} slot{}", if slots == 1 { "" } else { "s" }));
    meta.push(format!("{} today", bytes(nd.get("usage").map(|u| n(u, "bytes")).unwrap_or(0.0))));
    if n(&i, "mobileCapMb") > 0.0 { meta.push(format!("mobile {} of {} MB", bytes(n(&i, "mobileBytesToday")), ss(&i, "mobileCapMb"))); }
    let homes: Vec<String> = home_nets(d).iter().map(|x| ss(x, "id")).collect();
    let net = homes.iter().position(|x| *x == ss(nd, "network")).map(|p| p + 1).unwrap_or(0);
    Node { id: ss(nd, "id"), name: ss(nd, "name"), dot, state, tone, meta: meta.join(" · "), net, on: enabled }
}

/// The screen's rows from GET /api/homepool, at `now` (ms), with key
/// `open_key`'s "How to use" open.
pub fn model(d: &Value, now: f64, open_key: &str) -> Model {
    let (nodes, keys, reqs) = (arr(d, "nodes"), arr(d, "keys"), arr(d, "requests"));
    let pending = reqs.iter().filter(|r| ss(r, "status") == "pending").count();
    let tabs = vec![
        Tab { key: "networks", label: "Networks", count: nodes.len().to_string(), alert: false },
        Tab { key: "access", label: "Access", count: keys.len().to_string(), alert: false },
        Tab { key: "requests", label: "Requests", count: pending.to_string(), alert: pending > 0 },
        Tab { key: "activity", label: "Activity", count: String::new(), alert: false },
    ];
    let up = nodes.iter().filter(|x| t(x, "up") && !t(x, "paused")).count();
    let today: f64 = keys.iter().map(|k| k.get("usage").map(|u| n(u, "bytes")).unwrap_or(0.0)).sum();
    let sum = vec![
        (format!("{up}/{}", nodes.len()), "NODES UP".to_string()),
        (keys.iter().filter(|k| ss(k, "refusal").is_empty()).count().to_string(), "KEYS IN USE".into()),
        (bytes(today), "DATA TODAY".into()),
        (pending.to_string(), "WAITING FOR YOU".into()),
    ];
    let mut move_opts = vec!["No network".to_string()];
    move_opts.extend(home_nets(d).iter().map(|x| ss(x, "name")));
    let nets: Vec<Net> = arr(d, "networks").into_iter().map(|x| {
        let id = ss(x, "id");
        let mut net = Net { id: id.clone(), name: ss(x, "name"), ..Default::default() };
        if id == "server" {
            net.kind = "server"; net.chip = "always up".into(); net.tone = "ok";
            net.body = "This server\u{2019}s own connection. The default for every key.".into();
        } else if id == "pool" {
            let p = x.get("pool").cloned().unwrap_or_default();
            let on = x.get("enabled") != Some(&Value::Bool(false));
            net.kind = "pool"; net.on = on;
            let (c, tn) = if !on { ("bridge off".to_string(), "") } else if t(&p, "up") { (format!("{} node(s) up", n(&p, "liveCount") as i64), "ok") } else { ("down".into(), "warn") };
            net.chip = c; net.tone = tn;
            let ups: Vec<String> = arr(&p, "nodes").iter().map(|y| { let nm = ss(y, "name"); if nm.is_empty() { ss(y, "id") } else { nm } }).collect();
            net.body = format!("The standalone homepool (homepool-relay) as it runs today. A key on this network keeps the same slots, so the same identities keep the same home lines.{}", if ups.is_empty() { String::new() } else { format!(" Up: {}.", ups.join(", ")) });
            net.err = ss(&p, "error");
        } else {
            let live = n(x, "live") as i64;
            net.kind = "own"; net.chip = format!("{live} up"); net.tone = if live > 0 { "ok" } else { "" };
            net.nodes = nodes.iter().filter(|y| ss(y, "network") == id).map(|y| node_row(d, y, now)).collect();
            if net.nodes.is_empty() { net.body = "No node in this network yet. Add one, or move one here.".into(); }
        }
        net
    }).collect();
    let net_ids: Vec<String> = arr(d, "networks").into_iter().map(|x| ss(x, "id")).collect();
    let loose = nodes.iter().filter(|x| { let nw = ss(x, "network"); nw.is_empty() || !net_ids.contains(&nw) }).map(|x| node_row(d, x, now)).collect();
    let keys_rows: Vec<Key> = keys.iter().map(|k| {
        let u = k.get("usage").cloned().unwrap_or_default();
        let s = k.get("status").cloned().unwrap_or_default();
        let refusal = ss(k, "refusal");
        let (state, tone) = if !refusal.is_empty() { (refusal, "bad") } else if t(&s, "egressAvailable") {
            ((if t(&s, "fallbackInUse") { "leaving from the server (fallback)" } else { "exit up" }).to_string(), "ok")
        } else { ("no exit up".into(), "warn") };
        let kind = ss(k, "kind");
        let mut meta = vec![
            if kind == "software" { (if t(k, "portBlock") { "Software, port block" } else { "Software, own login" }).to_string() } else { format!("{}: {}", kind_label(&kind), who_of(k)) },
            format!("{} today", bytes(n(&u, "bytes"))),
            format!("{} connections", n(&u, "conns") as i64),
            format!("{} refused", n(&u, "refused") as i64),
            format!("used {}", ago_at(now, n(&u, "lastUsedAt"))),
        ];
        if n(k, "open") > 0.0 { meta.push(format!("{} open now", n(k, "open") as i64)); }
        if n(k, "dailyCapMb") > 0.0 { meta.push(format!("cap {} MB/day", ss(k, "dailyCapMb"))); }
        if n(k, "expiresAt") > 0.0 { meta.push(format!("expires {}", day(n(k, "expiresAt")))); }
        if t(k, "fallbackToServer") { meta.push("falls back to the server".into()); }
        if t(k, "portBlock") { if let Some(p) = k.get("ports").filter(|p| p.is_object()) {
            meta.push(format!("ports {}-{} from {}", ss(p, "base"), ss(p, "status"), strs(k, "allowFrom").join(", ")));
        } }
        let open = !open_key.is_empty() && open_key == ss(k, "id");
        let mut uses = Vec::new();
        if let Some(us) = k.get("use").filter(|x| x.is_object() && open) {
            if kind == "software" {
                uses.push(use_line("", "quiet", &format!("Login: {}", ss(us, "login"))));
                uses.push(use_line("", "code", &ss(us, "socks")));
                uses.push(use_line("", "code", &ss(us, "http")));
                uses.push(use_line("", "quiet", &format!("Status for the software to check before it starts: {}", ss(us, "status"))));
                if !ss(us, "fromDocker").is_empty() { uses.push(use_line("", "quiet", &format!("From a container: {}", ss(us, "fromDocker")))); }
            } else {
                uses.push(use_line("", "quiet", "Chats use it through the MCP tools homepool_proxy and homepool_fetch."));
            }
            if let Some(b) = us.get("block").filter(|b| b.is_object()) {
                uses.push(use_line("", "quiet", &format!("Port block (no login; only from {}): any slot {}, slots {}. For docker compose:", strs(b, "from").join(", "), ss(b, "any"), ss(b, "slots"))));
                uses.push(use_line("", "code", &strs(b, "compose").join("\n")));
            }
        }
        Key {
            id: ss(k, "id"), name: ss(k, "name"),
            nets: strs(k, "networks").iter().map(|x| net_name(d, x)).collect(),
            state, tone, on: k.get("enabled") != Some(&Value::Bool(false)),
            meta: meta.join(" · "), open, software: kind == "software", uses,
        }
    }).collect();
    let reqs_rows: Vec<Req> = reqs.iter().map(|r| {
        let status = ss(r, "status");
        let pend = status == "pending";
        let (name, p) = (ss(r, "name"), ss(r, "project"));
        Req {
            id: ss(r, "id"),
            title: format!("{}{}", kind_label(&ss(r, "kind")), if name.is_empty() { String::new() } else { format!(" \"{name}\"") }),
            nets: strs(r, "networks").iter().map(|x| net_name(d, x)).collect(),
            tone: if pend { "warn" } else if status == "approved" { "ok" } else { "bad" },
            status, pending: pend,
            meta: format!("From chat {}{}, {}", ss(r, "chatId").chars().take(8).collect::<String>(), if p.is_empty() { String::new() } else { format!(" in {p}") }, ago_at(now, n(r, "createdAt"))),
            reason: ss(r, "reason"),
        }
    }).collect();
    let names = |id: &str| nodes.iter().chain(keys.iter()).find(|x| ss(x, "id") == id).map(|x| ss(x, "name")).unwrap_or_else(|| id.to_string());
    let log = arr(d, "log").iter().map(|e| {
        let what = ss(e, "what");
        let mut subj = if !ss(e, "node").is_empty() { names(&ss(e, "node")) } else if !ss(e, "key").is_empty() { names(&ss(e, "key")) }
            else if !ss(e, "network").is_empty() { net_name(d, &ss(e, "network")) } else if !ss(e, "request").is_empty() { ss(e, "request") }
            else if !ss(e, "user").is_empty() { format!("{}{}", ss(e, "user"), { let f = ss(e, "from"); if f.is_empty() { String::new() } else { format!(" from {f}") } }) } else { String::new() };
        if what == "node-moved" { subj.push_str(&format!(" \u{2192} {}", net_name(d, &ss(e, "to")))); }
        Log { at: day(n(e, "at")), what: log_what(&what), subj }
    }).collect();
    Model { tabs, sum, nets, loose, move_opts, keys: keys_rows, reqs: reqs_rows, log }
}

/// A chip's tone in the widgets' names (tag): ok, warn, bad → error.
pub fn tag_tone(t: &str) -> &'static str { match t { "ok" => "ok", "warn" => "warn", "bad" => "error", _ => "" } }

#[cfg(test)]
mod tests {
    use super::{ago_at, bytes, dur, log_what, model, parse_local, qr_runs, show_local, who_of};
    use serde_json::json;

    #[::core::prelude::v1::test]
    fn formats() {
        assert_eq!(bytes(500.0), "500 B");
        assert_eq!(bytes(2048.0), "2 KB");
        assert_eq!(bytes(3.5 * 1048576.0), "3.5 MB");
        assert_eq!(bytes(2.0 * 1073741824.0), "2.00 GB");
        assert_eq!(dur(30_000.0), "30s");
        assert_eq!(dur(125_000.0), "2m");
        assert_eq!(dur(3_725_000.0), "1h 2m");
        assert_eq!(dur(90_000_000.0), "1d 1h");
        assert_eq!(ago_at(100_000.0, 0.0), "never");
        assert_eq!(ago_at(100_000.0, 40_000.0), "1m ago");
    }

    #[::core::prelude::v1::test]
    fn words() {
        assert_eq!(log_what("node-moved"), "moved node");
        assert_eq!(log_what("custom"), "custom");
        assert_eq!(who_of(&json!({ "kind": "chat", "chatId": "abcdef123456", "project": "demo" })), "chat abcdef12 in demo");
        assert_eq!(who_of(&json!({ "kind": "project", "project": "demo" })), "every chat of demo");
        assert_eq!(who_of(&json!({ "kind": "software" })), "software");
    }

    #[::core::prelude::v1::test]
    fn times_and_qr() {
        let ms = parse_local("2026-11-15 10:30").unwrap();
        assert_eq!(show_local(ms), "2026-11-15 10:30");
        assert_eq!(parse_local("2026-11-15T10:30"), Some(ms));
        assert_eq!(parse_local(""), None);
        assert_eq!(parse_local("soon"), None);
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 29 29" shape-rendering="crispEdges"><rect width="29" height="29" fill="#fff"/><path d="M4 4h7v1h-7zM12 5h1v1h-1z" fill="#000"/></svg>"##;
        assert_eq!(qr_runs(svg), Some((vec![(4.0, 4.0, 7.0), (12.0, 5.0, 1.0)], 29.0)));
        assert_eq!(qr_runs(""), None);
    }

    #[::core::prelude::v1::test]
    fn rows() {
        let d = json!({
            "networks": [{ "id": "server", "name": "Server" }, { "id": "pool", "name": "Standalone homepool", "enabled": false }, { "id": "home", "name": "Home", "live": 0 }],
            "nodes": [{ "id": "n1", "name": "PC", "network": "home", "slots": 1, "enabled": false }],
            "keys": [{ "id": "k1", "name": "s", "kind": "software", "networks": ["server", "home"], "dailyCapMb": 100, "fallbackToServer": true }],
            "requests": [{ "id": "r1", "kind": "chat", "status": "pending", "chatId": "abcdefghij" }],
            "log": [{ "what": "node-moved", "node": "n1", "to": "home", "at": 0 }],
        });
        let m = model(&d, 0.0, "");
        assert_eq!(m.tabs[2].count, "1");
        assert!(m.tabs[2].alert);
        assert_eq!(m.nets[0].chip, "always up");
        assert_eq!(m.nets[1].chip, "bridge off");
        assert_eq!(m.nets[2].nodes[0].state, "switched off");
        assert_eq!(m.nets[2].nodes[0].net, 1);
        assert!(m.nets[2].nodes[0].meta.starts_with("Standalone node · last seen never · 1 slot · 0 B today"));
        assert_eq!(m.move_opts, vec!["No network", "Home"]);
        assert_eq!(m.keys[0].nets, vec!["Server", "Home"]);
        assert!(m.keys[0].meta.contains("cap 100 MB/day") && m.keys[0].meta.ends_with("falls back to the server"));
        assert_eq!(m.reqs[0].title, "Chat");
        assert_eq!(m.log[0].subj, "PC \u{2192} Home");
        assert_eq!(m.sum[3], ("1".to_string(), "WAITING FOR YOU".to_string()));
    }
}
