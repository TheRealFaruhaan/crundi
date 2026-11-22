//! Settings' pure rules (the web's renderSettings and its helpers, turned
//! into plain values): unit-tested below.

use serde_json::{Map, Value};

fn st(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    }
}
fn b(v: &Value, k: &str) -> bool {
    match v.get(k) {
        Some(Value::Bool(x)) => *x,
        Some(Value::String(x)) => !x.is_empty(),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}
fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))
}

// ─── Notifications ───

/// The web's NOTIFY_DEFAULTS_CLIENT.
pub const NOTIFY_DEFAULTS: &[(&str, &str)] = &[
    ("finished", "away"), ("needsInput", "away"),
    ("kanbanTask", "never"), ("kanbanSubtask", "never"),
    ("scheduleRun", "away"),
    ("serviceDown", "always"), ("serviceUp", "away"),
    ("forwardDown", "always"), ("forwardUp", "away"),
    ("updateAvailable", "away"),
    ("scheduledChat", "always"),
    ("mindmapAdd", "never"), ("mindmapDelete", "never"),
    ("browserLaunch", "never"), ("browserStop", "never"),
    ("secretRequest", "always"),
    ("limitReset", "always"),
    ("collabPermission", "away"),
    ("factory", "always"),
];

/// The matrix's groups and rows, in the web's order (NOTIFY_GROUPS).
pub const NOTIFY_GROUPS: &[(&str, &[(&str, &str)])] = &[
    ("Agent", &[("finished", "When agent finishes"), ("needsInput", "When agent needs input")]),
    ("Kanban", &[("kanbanTask", "Main task status change"), ("kanbanSubtask", "Sub task status change")]),
    ("Schedule", &[("scheduleRun", "When a schedule executes"), ("scheduledChat", "Result of a scheduled chat")]),
    ("Services", &[("serviceDown", "When a service crashes / stops"), ("serviceUp", "When a service starts")]),
    ("Forwards", &[("forwardDown", "When a watched forward goes down"), ("forwardUp", "When a watched forward is back")]),
    ("Mindmap", &[("mindmapAdd", "When a node is added"), ("mindmapDelete", "When a node is deleted")]),
    ("MCP Browser", &[("browserLaunch", "When a browser launches"), ("browserStop", "When a browser stops")]),
    ("Security", &[("secretRequest", "Secret access requested")]),
    ("Collaborators", &[("collabPermission", "When a collaborator\u{2019}s Claude asks for permission")]),
    ("Updates", &[("updateAvailable", "When a new Claude Code version is out")]),
    ("Usage", &[("limitReset", "When a usage limit resets")]),
];

/// The prefs: the defaults, then what the server keeps.
pub fn notify_prefs(server: Option<&Value>) -> Map<String, Value> {
    let mut m = Map::new();
    for (k, v) in NOTIFY_DEFAULTS { m.insert((*k).into(), Value::String((*v).into())); }
    if let Some(Value::Object(o)) = server { for (k, v) in o { m.insert(k.clone(), v.clone()); } }
    m
}

/// One matrix row: (category heading, key, label, mode). A heading has a
/// category and nothing else.
#[derive(Debug, Clone, PartialEq)]
pub struct NotifyRow { pub cat: String, pub key: String, pub label: String, pub mode: String }

pub fn notify_rows(prefs: &Map<String, Value>) -> Vec<NotifyRow> {
    let mut out = Vec::new();
    for (cat, evs) in NOTIFY_GROUPS {
        out.push(NotifyRow { cat: cat.to_uppercase(), key: String::new(), label: String::new(), mode: String::new() });
        for (key, label) in *evs {
            let mode = prefs.get(*key).and_then(|v| v.as_str()).unwrap_or("never").to_string();
            out.push(NotifyRow { cat: String::new(), key: (*key).into(), label: (*label).into(), mode });
        }
    }
    out
}

pub fn valid_mode(m: &str) -> bool { matches!(m, "always" | "away" | "never") }

// ─── Sign-in ───

/// Telegram's line under its switch (authTelegramSettings).
pub fn telegram_status(tb: &Value) -> String {
    if !b(tb, "tokenSet") {
        "Not set up. Create a bot with @BotFather, then enter its token and your Telegram username, and Save.".into()
    } else if b(tb, "restartRequired") {
        "Saved. Restart Crundi to start the bot with these settings; it can be turned on after that.".into()
    } else if st(tb, "status") == "connected" {
        let u = st(tb, "botUsername");
        format!("Bot {}is connected. Sign in with the Telegram account you set here.", if u.is_empty() { String::new() } else { format!("@{u} ") })
    } else {
        "The bot is offline. Check the token, and that no other server is using the same bot.".into()
    }
}

/// The Telegram channel's bot line (telegramChannelExtra): (tone, words).
pub fn telegram_channel_state(t: &Value) -> (&'static str, String) {
    if !b(t, "tokenSet") { ("", "Not set up".into()) }
    else if b(t, "restartRequired") { ("warn", "Saved, waiting for a restart".into()) }
    else if st(t, "botStatus") == "connected" {
        let u = st(t, "botUsername");
        ("ok", format!("Connected{}", if u.is_empty() { String::new() } else { format!(" as @{u}") }))
    } else { ("error", "Offline".into()) }
}

/// Google's or GitHub's block (authOAuthSettings).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OAuthView {
    pub key: String, pub label: String, pub on: bool, pub can_on: bool, pub status: String,
    pub url: String, pub local: bool, pub howto: String, pub acct: String, pub ph: String,
    pub configured: bool, pub secret_set: bool,
    pub client_id: String, pub allowed: String,
}

pub fn oauth_label(k: &str) -> &'static str { if k == "google" { "Google" } else { "GitHub" } }

pub fn oauth_view(auth: &Value, k: &str) -> OAuthView {
    let o = auth.pointer(&format!("/oauth/{k}")).cloned().unwrap_or(Value::Null);
    let label = oauth_label(k);
    let configured = b(&o, "configured");
    let verified = b(&o, "verified");
    let status = if !configured {
        format!("Not set up. Create an OAuth app with {label}, then fill this in, Save and Test.")
    } else if !verified {
        "Saved, not tested since the last change. Run Test before turning it on.".into()
    } else {
        let who = if st(&o, "verifiedAs").is_empty() { st(&o, "allowed") } else { st(&o, "verifiedAs") };
        format!("Tested: signed in as {who}.{}", if b(&o, "idPinned") { " Pinned to that account\u{2019}s id, so a renamed username cannot take it over." } else { "" })
    };
    let url = auth.pointer(&format!("/callbackUrls/{k}")).and_then(|u| u.as_str()).unwrap_or("").to_string();
    let local = url.starts_with("http://localhost:") || url.starts_with("http://localhost/");
    let (acct, ph, howto) = if k == "google" {
        ("Allowed Google email", "you@gmail.com", "Google Cloud console, APIs and Services, Credentials: create an OAuth client ID of type Web application, and add this as an authorised redirect URI.")
    } else {
        ("Allowed GitHub username", "your-username", "GitHub, Settings, Developer settings, OAuth Apps, New OAuth App: use this as the authorisation callback URL.")
    };
    OAuthView {
        key: k.into(), label: label.into(), on: b(auth, k), can_on: configured && verified, status, url, local,
        howto: howto.into(), acct: acct.into(), ph: ph.into(), configured, secret_set: b(&o, "secretSet"),
        client_id: st(&o, "clientId"), allowed: st(&o, "allowed"),
    }
}

pub fn method_label(m: &str) -> String {
    match m { "telegram" => "Telegram", "password" => "Password", "google" => "Google", "github" => "GitHub", _ => m }.to_string()
}

// ─── Updates ───

/// The Server section's body (renderServerUpdate).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerView {
    pub version: String, pub sub: String, pub act: String, pub latest: String, pub channel: i32,
    pub dev_note: String, pub blocker: String, pub notes_url: String, pub log: String, pub can_restart: bool,
    pub applying: bool,
}

pub fn server_view(d: &Value, now_secs: f64) -> ServerView {
    let u = d.get("update").cloned().unwrap_or(Value::Null);
    let applying = b(&u, "applying");
    let available = b(&u, "available");
    let dev = st(&u, "channel") == "dev";
    let latest = st(&u, "latest");
    let sub = if applying {
        "Updating… the server will restart on its own.".to_string()
    } else if available {
        if dev {
            if st(&u, "latestKind") == "production" { format!("Production release {latest} is available.") } else { format!("Dev build {latest} is available.") }
        } else { format!("Version {latest} is available.") }
    } else if !st(&u, "error").is_empty() {
        format!("Last check failed: {}", st(&u, "error"))
    } else {
        let checked = num(&u, "checkedAt").filter(|t| *t > 0.0).map(|t| format!(" \u{b7} checked {}", rel_time(t, now_secs))).unwrap_or_default();
        format!("Up to date{checked}.")
    };
    let blocker = st(&u, "blocker");
    let act = if applying { "applying" } else if available && blocker.is_empty() { "update" } else { "check" };
    let dev_note = if !dev && b(&u, "currentIsDev") && !available {
        format!("This server runs a dev build. Production updates resume once a production release is newer than {}.", st(&u, "current"))
    } else { String::new() };
    let log = st(d, "log");
    let log = if applying && !log.is_empty() { tail_chars(&log, 1200) } else { String::new() };
    ServerView {
        version: st(&u, "current"), sub, act: act.into(), latest, channel: if dev { 1 } else { 0 },
        dev_note, blocker: if available { blocker } else { String::new() },
        notes_url: if available { st(&u, "notesUrl") } else { String::new() },
        log, can_restart: b(d, "canRestart"), applying,
    }
}

fn tail_chars(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n { s.to_string() } else { s.chars().skip(count - n).collect() }
}

/// The Claude Code section's body (renderClaudeUpdate).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClaudeView {
    pub version: String, pub sub: String, pub act: String, pub latest: String, pub beyond: bool,
    pub tested: String, pub tested_warn: String, pub blocker: String, pub sudo: String, pub last: String,
}

pub fn claude_view(c: &Value) -> ClaudeView {
    let installed = st(c, "installed");
    let latest = st(c, "latest");
    let available = b(c, "available");
    let can = b(c, "canUpdate");
    let sub = if b(c, "applying") { "Installing…".to_string() }
        else if installed.is_empty() { "Not installed, or not on the server\u{2019}s PATH.".into() }
        else if !st(c, "error").is_empty() { format!("Last check failed: {}", st(c, "error")) }
        else if available { format!("Version {latest} is available.") }
        else { "Up to date.".into() };
    let act = if b(c, "applying") { "applying" } else if available && can { "update" } else { "check" };
    let tested_v = st(c, "tested");
    let (tested, tested_warn) = if tested_v.is_empty() { (String::new(), String::new()) } else {
        let mut w = Vec::new();
        if b(c, "beyondTested") && available { w.push(format!("{latest} is newer than that \u{2014} it will ask first.")); }
        if b(c, "runningBeyondTested") { w.push(format!("You are already running {installed}, past it.")); }
        (format!("This Crundi build was tested with {tested_v}."), w.join(" "))
    };
    let (blocker, sudo) = if !can && !st(c, "blocker").is_empty() { (st(c, "blocker"), String::new()) }
        else if b(c, "needsSudo") { (String::new(), "Installed system-wide, so updating uses sudo \u{2014} which is also why Claude Code cannot update itself here.".into()) }
        else { (String::new(), String::new()) };
    ClaudeView {
        version: if installed.is_empty() { "\u{2014}".into() } else { installed }, sub, act: act.into(), latest,
        beyond: b(c, "beyondTested"), tested, tested_warn, blocker, sudo, last: st(c, "lastResult"),
    }
}

// ─── Sessions ───

/// The web's relTime, from milliseconds since the epoch.
pub fn rel_time(ms: f64, now_secs: f64) -> String {
    if ms <= 0.0 { return String::new(); }
    let s = (now_secs - ms / 1000.0).max(0.0);
    if s < 60.0 { "just now".into() }
    else if s < 3600.0 { format!("{}m ago", (s / 60.0) as u64) }
    else if s < 86400.0 { format!("{}h ago", (s / 3600.0) as u64) }
    else if s < 2_592_000.0 { format!("{}d ago", (s / 86400.0) as u64) }
    else { date_of(ms) }
}

fn date_of(ms: f64) -> String {
    // A civil date from days since the epoch (Howard Hinnant's algorithm).
    let z = (ms / 86_400_000.0).floor() as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{d}/{m}/{y}")
}

/// A device from its user agent (describeAgent). This client says
/// "CrundiNative/…": the desktop app.
pub fn describe_agent(ua: &str) -> String {
    if ua.is_empty() { return "Unknown device".into(); }
    let has = |n: &str| ua.contains(n);
    let lower = ua.to_lowercase();
    let os = if lower.contains("android") { "Android" }
        else if has("iPhone") || has("iPad") || has("iOS") { "iOS" }
        else if lower.contains("windows") { "Windows" }
        else if has("Mac OS X") || has("Macintosh") { "macOS" }
        else if lower.contains("linux") { "Linux" } else { "" };
    let browser = if has("Edg/") { "Edge" } else if has("OPR/") { "Opera" }
        else if has("Electron/") || has("CrundiNative/") { "Desktop app" }
        else if has("Chrome/") { "Chrome" } else if has("Firefox/") { "Firefox" }
        else if has("Safari/") { "Safari" } else { "" };
    if !browser.is_empty() && !os.is_empty() { return format!("{browser} on {os}"); }
    if !browser.is_empty() { browser.into() } else if !os.is_empty() { os.into() } else { "Unknown device".into() }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow { pub id: String, pub label: String, pub current: bool, pub desc: String }

pub fn session_rows(list: &[Value], now_secs: f64) -> Vec<SessionRow> {
    list.iter().map(|x| {
        let dev = x.pointer("/device/name").and_then(|n| n.as_str()).unwrap_or("");
        let label = if dev.is_empty() { describe_agent(&st(x, "userAgent")) } else { format!("{dev} (Crundi app)") };
        let mut desc = String::new();
        if !st(x, "ip").is_empty() { desc.push_str(&st(x, "ip")); desc.push_str(" \u{b7} "); }
        desc.push_str("signed in ");
        desc.push_str(&rel_time(time_of(x.get("createdAt")), now_secs));
        let last = time_of(x.get("lastSeenAt"));
        if last > 0.0 { desc.push_str(" \u{b7} last used "); desc.push_str(&rel_time(last, now_secs)); }
        SessionRow { id: st(x, "id"), label, current: b(x, "current"), desc }
    }).collect()
}

/// A time the server sends as ms or as an ISO string, in ms.
fn time_of(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.parse::<f64>().ok().or_else(|| crundi_core::time::parse_iso(s).map(|t| t * 1000.0)).unwrap_or(0.0),
        _ => 0.0,
    }
}

// ─── Terminal ───

/// Close idle panes after: Ok(minutes) when 0 or at least `min`.
pub fn autopark(text: &str, min: i64) -> Result<i64, String> {
    let n = text.trim().parse::<f64>().ok().filter(|n| n.is_finite()).map(|n| n.round() as i64);
    match n {
        Some(n) if n == 0 || (n > 0 && n >= min) => Ok(n),
        _ => Err(format!("Use 0 to turn it off, or {min} minutes or more")),
    }
}

// ─── Phone ───

/// The push box's first line (renderPush).
pub fn push_line(s: &Value) -> String {
    let n = s.get("devices").and_then(|d| d.as_array()).map(|a| a.len()).unwrap_or(0);
    if b(s, "hasKey") {
        format!("Push key {} is set. {}", st(s, "keyHint"),
            if n > 0 { format!("{n} phone{} registered.", if n == 1 { "" } else { "s" }) } else { "No phone has registered for notifications yet.".into() })
    } else {
        format!("Notifications reach the phone app through a relay, which needs a key for this server. {}", st(s, "keyHelp"))
    }
}

pub fn relay_summary(s: &Value) -> String {
    let u = st(s, "relayUrl");
    let u = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).unwrap_or(&u).to_string();
    format!("{u}{}", if b(s, "customRelay") { " (custom)" } else { "" })
}

/// "Code expires in 4:05".
pub fn expires_in(left: i64) -> String { format!("Code expires in {}:{:02}", left.max(0) / 60, left.max(0) % 60) }

// ─── Cru (showVoiceServer) ───

/// The list with the current value added when missing, sorted.
pub fn choices(list: &[f64], cur: f64) -> Vec<f64> {
    let mut l = list.to_vec();
    if !l.iter().any(|x| (x - cur).abs() < 1e-9) { l.push(cur); }
    l.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    l
}

pub fn index_of(list: &[f64], cur: f64) -> i32 {
    list.iter().position(|x| (x - cur).abs() < 1e-9).map(|i| i as i32).unwrap_or(-1)
}

/// A number as JavaScript prints it (1.5, 2, 0.85).
pub fn js_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { let s = format!("{n}"); s }
}

pub fn rate_label(r: f64) -> String {
    if (r - 1.0).abs() < 0.001 { "Normal".into() } else { format!("{}x", js_num((r * 100.0).round() / 100.0)) }
}

pub fn sec_label(n: i64) -> String {
    if n < 60 { format!("{n} seconds") }
    else if n % 60 != 0 { format!("{} min {} s", n / 60, n % 60) }
    else { format!("{}{}", n / 60, if n == 60 { " minute" } else { " minutes" }) }
}

pub fn effort_label(e: &str) -> String {
    match e { "low" => "Low (quickest)", "medium" => "Medium", "high" => "High", "xhigh" => "Extra high", "max" => "Max (most thorough)", _ => e }.to_string()
}

/// The thinking effort's hint for the chosen model.
pub fn effort_hint(model: &Value) -> String {
    let label = st(model, "label");
    let label = if label.is_empty() { "This model".to_string() } else { label };
    let def = st(model, "apiDefaultEffort");
    format!("How hard Cru thinks before it answers: lower is faster and cheaper, higher is more thorough. {label} takes every level{}. You can also ask Cru to change it. It applies from the next answer.",
        if def.is_empty() { String::new() } else { format!(" (its own default is {def}; Cru runs at low unless you choose)") })
}

/// The Spending line.
pub fn spend_line(s: &Value) -> String {
    let sp = s.get("spend").cloned().unwrap_or(Value::Null);
    let money = |k: &str| format!("{:.2}", num(&sp, k).unwrap_or(0.0));
    format!("Claude API: ${} of ${}, {} requests, restarts {}{}. Deepgram: ${} of ${}, restarts {}{}.",
        money("claudeUsd"), js_num(num(s, "monthlyLimitUsd").unwrap_or(0.0)), num(&sp, "turns").unwrap_or(0.0) as i64,
        st(s, "nextReset"), if b(s, "limitReached") { " (limit reached)" } else { "" },
        money("deepgramUsd"), js_num(num(s, "deepgramLimitUsd").unwrap_or(0.0)), st(s, "deepgramNextReset"),
        if b(s, "deepgramLimitReached") { " (limit reached)" } else { "" })
}

pub fn admin_hint(s: &Value) -> String {
    format!("Cru can run any command on this server as the Crundi user, with no confirmation, including on its own (the heartbeat, its tickers and triggers) to look after the server while you are away, every command logged. A spoken request is obeyed only in your voice. On its own it never restarts Crundi.{}",
        if s.get("adminCapable") == Some(&Value::Bool(false)) { " It needs Sonnet or above: choose another model first." } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[::core::prelude::v1::test]
    fn notify_matrix_follows_the_groups_and_the_server() {
        let p = notify_prefs(Some(&json!({ "finished": "always" })));
        let rows = notify_rows(&p);
        assert_eq!(rows[0].cat, "AGENT");
        assert_eq!(rows[1].key, "finished");
        assert_eq!(rows[1].mode, "always");
        assert_eq!(rows[2].mode, "away");
        assert_eq!(rows.iter().filter(|r| !r.cat.is_empty()).count(), NOTIFY_GROUPS.len());
        assert!(valid_mode("away") && !valid_mode("sometimes"));
    }

    #[::core::prelude::v1::test]
    fn telegram_lines() {
        assert!(telegram_status(&json!({})).starts_with("Not set up."));
        assert!(telegram_status(&json!({ "tokenSet": true, "restartRequired": true })).starts_with("Saved. Restart"));
        assert_eq!(telegram_status(&json!({ "tokenSet": true, "status": "connected", "botUsername": "cb" })), "Bot @cb is connected. Sign in with the Telegram account you set here.");
        assert_eq!(telegram_channel_state(&json!({ "tokenSet": true, "botStatus": "connected", "botUsername": "cb" })), ("ok", "Connected as @cb".into()));
        assert_eq!(telegram_channel_state(&json!({ "tokenSet": true })).0, "error");
    }

    #[::core::prelude::v1::test]
    fn oauth_states() {
        let a = json!({ "google": false, "oauth": { "google": { "configured": true, "verified": true, "verifiedAs": "me@x.com", "clientId": "id" } },
            "callbackUrls": { "google": "http://localhost:4566/api/auth/oauth/google/callback" } });
        let g = oauth_view(&a, "google");
        assert!(g.can_on && g.local && g.configured);
        assert_eq!(g.status, "Tested: signed in as me@x.com.");
        assert_eq!(g.client_id, "id");
        let h = oauth_view(&a, "github");
        assert!(!h.can_on);
        assert!(h.status.starts_with("Not set up. Create an OAuth app with GitHub"));
    }

    #[::core::prelude::v1::test]
    fn server_update_view() {
        let d = json!({ "update": { "current": "1.2.0", "latest": "1.3.0", "available": true, "channel": "production", "notesUrl": "u" }, "canRestart": true });
        let v = server_view(&d, 0.0);
        assert_eq!(v.act, "update");
        assert_eq!(v.sub, "Version 1.3.0 is available.");
        assert_eq!(v.notes_url, "u");
        let d = json!({ "update": { "current": "1.2.0-dev.1", "latest": "1.2.0-dev.1", "available": false, "channel": "production", "currentIsDev": true, "checkedAt": 1000 } });
        let v = server_view(&d, 61.0);
        assert_eq!(v.act, "check");
        assert_eq!(v.sub, "Up to date \u{b7} checked 1m ago.");
        assert!(v.dev_note.contains("1.2.0-dev.1"));
        let d = json!({ "update": { "available": true, "blocker": "Not a git checkout", "channel": "dev", "latestKind": "dev", "latest": "2" } });
        let v = server_view(&d, 0.0);
        assert_eq!(v.act, "check");
        assert_eq!(v.sub, "Dev build 2 is available.");
        assert_eq!(v.blocker, "Not a git checkout");
        assert_eq!(v.channel, 1);
    }

    #[::core::prelude::v1::test]
    fn claude_update_view() {
        let c = json!({ "installed": "2.1", "latest": "2.2", "tested": "2.1", "available": true, "beyondTested": true, "canUpdate": true, "needsSudo": true });
        let v = claude_view(&c);
        assert_eq!(v.act, "update");
        assert!(v.beyond);
        assert_eq!(v.tested, "This Crundi build was tested with 2.1.");
        assert!(v.tested_warn.starts_with("2.2 is newer"));
        assert!(v.sudo.starts_with("Installed system-wide"));
        let v = claude_view(&json!({ "installed": "", "canUpdate": false, "blocker": "no npm" }));
        assert_eq!(v.version, "\u{2014}");
        assert_eq!(v.blocker, "no npm");
        assert_eq!(v.act, "check");
    }

    #[::core::prelude::v1::test]
    fn devices_and_times() {
        assert_eq!(describe_agent("Mozilla/5.0 (Windows NT 10.0) Chrome/120 Safari/537"), "Chrome on Windows");
        assert_eq!(describe_agent("Mozilla/5.0 (X11; Linux x86_64) Edg/120 Chrome/120"), "Edge on Linux");
        assert_eq!(describe_agent("CrundiNative/0.1.0"), "Desktop app");
        assert_eq!(describe_agent("node"), "Unknown device");
        assert_eq!(describe_agent(""), "Unknown device");
        assert_eq!(rel_time(1000.0 * 1000.0, 1000.0 + 7200.0), "2h ago");
        assert_eq!(date_of(0.0), "1/1/1970");
        let rows = session_rows(&[json!({ "id": "a", "current": true, "userAgent": "node", "ip": "1.2.3.4", "createdAt": 0, "lastSeenAt": 0 }),
            json!({ "id": "b", "device": { "name": "Pixel" }, "createdAt": 1000 })], 100.0);
        assert_eq!(rows[0].desc, "1.2.3.4 \u{b7} signed in ");
        assert_eq!(rows[1].label, "Pixel (Crundi app)");
        assert!(rows[0].current && !rows[1].current);
    }

    #[::core::prelude::v1::test]
    fn autopark_rules() {
        assert_eq!(autopark("0", 10), Ok(0));
        assert_eq!(autopark(" 15 ", 10), Ok(15));
        assert!(autopark("5", 10).is_err());
        assert!(autopark("x", 10).is_err());
        assert_eq!(autopark("5", 10).unwrap_err(), "Use 0 to turn it off, or 10 minutes or more");
    }

    #[::core::prelude::v1::test]
    fn phone_lines() {
        let s = json!({ "hasKey": true, "keyHint": "…ab12", "devices": [{}], "relayUrl": "https://relay.x", "customRelay": true });
        assert_eq!(push_line(&s), "Push key …ab12 is set. 1 phone registered.");
        assert_eq!(relay_summary(&s), "relay.x (custom)");
        assert!(push_line(&json!({ "keyHelp": "Ask." })).ends_with("for this server. Ask."));
        assert_eq!(expires_in(245), "Code expires in 4:05");
    }

    #[::core::prelude::v1::test]
    fn cru_choices_and_labels() {
        let c = choices(&[5.0, 10.0, 30.0], 20.0);
        assert_eq!(c, vec![5.0, 10.0, 20.0, 30.0]);
        assert_eq!(index_of(&c, 20.0), 2);
        assert_eq!(rate_label(1.0), "Normal");
        assert_eq!(rate_label(1.15), "1.15x");
        assert_eq!(js_num(1.5), "1.5");
        assert_eq!(js_num(2.0), "2");
        assert_eq!(sec_label(45), "45 seconds");
        assert_eq!(sec_label(60), "1 minute");
        assert_eq!(sec_label(90), "1 min 30 s");
        assert_eq!(sec_label(300), "5 minutes");
        let s = json!({ "spend": { "claudeUsd": 1.234, "turns": 3 }, "monthlyLimitUsd": 50, "nextReset": "2026-12-01", "deepgramLimitUsd": 200, "deepgramNextReset": "2026-12-01" });
        assert_eq!(spend_line(&s), "Claude API: $1.23 of $50, 3 requests, restarts 2026-12-01. Deepgram: $0.00 of $200, restarts 2026-12-01.");
        assert!(admin_hint(&json!({ "adminCapable": false })).ends_with("choose another model first."));
    }
}
