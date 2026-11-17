//! Ops area: Info: the server's facts and its log (renderInfo).
//! UI: ui/ops/info.slint (global OpsInfo).
//!
//! The web's renderInfo (Project, Status, Tunnel, Server Logs with
//! copyServerLogs / renderServerLogs / appendServerLog), renderSystemStats
//! (polled every 2 s while Info is on show), loadMaintenance /
//! runMaintenance, and loadCollaborators / collabAction / collabCreate /
//! showInviteModal / offerGitInit.

use super::skills::common::{self as c, ok};
use crate::{areas::AreaEvent, kit, MainWindow};
use crate::{OpsCollabPerson, OpsCollabRow, OpsInfo, OpsInfoCheck, OpsInfoLog, OpsInfoSys, OpsMaintTask};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::RefCell, collections::HashSet, rc::Rc, time::Duration};

/// Lines of the log kept (the web keeps them all; this keeps the newest).
const LOG_KEEP: usize = 2000;

#[derive(Default)]
struct Info {
    shown: bool,
    rendered: bool,
    services: Vec<Value>,
    logs: Vec<(String, String)>,
    maint: Vec<Value>,
    maint_busy: String,
    people: Vec<(String, f64)>,
    picked: HashSet<String>,
    cred_block: String,
    cred: [String; 3],
    git_project: String,
    timer: Option<slint::Timer>,
}

thread_local! {
    static INFO: RefCell<Info> = RefCell::new(Info::default());
    static LOGS: Rc<VecModel<OpsInfoLog>> = kit::model();
    static MAINT: Rc<VecModel<OpsMaintTask>> = kit::model();
    static SYS: Rc<VecModel<OpsInfoSys>> = kit::model();
    static PROJ: Rc<VecModel<OpsInfoCheck>> = kit::model();
}

fn st<R>(f: impl FnOnce(&mut Info) -> R) -> R { INFO.with(|s| f(&mut s.borrow_mut())) }
fn g(f: impl FnOnce(&OpsInfo)) { if let Some(w) = kit::ui() { f(&w.global::<OpsInfo>()); } }

pub fn init(w: &MainWindow) {
    let gl = w.global::<OpsInfo>();
    gl.set_logs(LOGS.with(kit::rc));
    gl.set_maint(MAINT.with(kit::rc));
    gl.set_sys_rows(SYS.with(kit::rc));
    gl.set_inv_projects(PROJ.with(kit::rc));
    gl.on_act(|a, b| act(a.as_str(), b.as_str()));
}

pub fn signed_in() {}

pub fn signed_out() {
    st(|s| { s.timer = None; *s = Info::default(); });
    LOGS.with(|m| kit::sync_rows(m, vec![]));
    g(|g| { g.set_logs_loading(true); g.set_sys_ready(false); g.set_cred_open(false); g.set_git_open(false); g.set_modal(false); });
}

pub fn project(_alias: &str) {
    if st(|s| s.shown) { draw_project(); invite_projects(); }
}

pub fn tab_shown(tab: &str) {
    let on = tab == "info";
    st(|s| s.shown = on);
    if on { render(); poll_start(); } else { st(|s| s.timer = None); }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::State(v) => {
            let sv = v.get("services").and_then(|x| x.as_array()).cloned();
            if let Some(sv) = sv { st(|s| s.services = sv); }
            if st(|s| s.shown) { draw_project(); }
        }
        AreaEvent::Ws("server-log", m) => { if st(|s| s.rendered) { append_log(m); } }
        AreaEvent::Connected(true) => { if st(|s| s.shown) { kit::ws_send(&json!({ "type": "subscribe-logs" })); } }
        _ => {}
    }
}

// ─── Pure rules ───

/// fmtPct: "12.5%", "100%", "—".
pub fn fmt_pct(n: Option<f64>) -> String {
    match n { None => "—".into(), Some(n) if n >= 100.0 => format!("{}%", n.round() as i64), Some(n) => format!("{n:.1}%") }
}

/// fmtDur: "2d 3h", "4h 5m", "6m".
pub fn fmt_dur(sec: Option<f64>) -> String {
    let Some(sec) = sec.filter(|x| x.is_finite()) else { return "—".into() };
    let (d, h, m) = ((sec / 86400.0).floor() as i64, ((sec % 86400.0) / 3600.0).floor() as i64, ((sec % 3600.0) / 60.0).floor() as i64);
    if d > 0 { format!("{d}d {h}h") } else if h > 0 { format!("{h}h {m}m") } else { format!("{m}m") }
}

/// sparkSvg's path: 0..100 wide, 0..20 high, scaled to `max`; empty below two points.
pub fn spark(vals: &[f64], max: f64) -> String {
    if vals.len() < 2 || max <= 0.0 { return String::new(); }
    let n = vals.len();
    vals.iter().enumerate().map(|(i, v)| {
        let x = i as f64 / (n - 1) as f64 * 100.0;
        let y = 19.0 - (v / max).clamp(0.0, 1.0) * 18.0;
        format!("{}{:.2} {:.2}", if i == 0 { "M" } else { "L" }, x, y)
    }).collect()
}

fn line(k: &str, v: &str) -> OpsInfoSys { OpsInfoSys { kind: "line".into(), k: k.into(), v: v.into(), pct: 0.0 } }
fn bar(p: f64) -> OpsInfoSys { OpsInfoSys { kind: "bar".into(), pct: p.clamp(0.0, 100.0) as f32, ..Default::default() } }
fn gap() -> OpsInfoSys { OpsInfoSys { kind: "gap".into(), ..Default::default() } }

/// The machine's rows after the CPU block (renderSystemStats).
pub fn sys_rows(sys: &Value) -> Vec<OpsInfoSys> {
    let o = |k: &str| sys.get(k).cloned().unwrap_or_default();
    let (mem, disk, host) = (o("mem"), o("disk"), o("host"));
    let f = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_f64());
    let pct = |u: Option<f64>, t: Option<f64>| match (u, t) { (Some(u), Some(t)) if t > 0.0 => u / t * 100.0, _ => 0.0 };
    let b = |v: Option<f64>| v.map(c::fmt_bytes).unwrap_or_else(|| "—".into());
    let mut out = vec![gap(), line("Memory", &format!("{} of {}", b(f(&mem, "used")), b(f(&mem, "total")))), bar(pct(f(&mem, "used"), f(&mem, "total")))];
    if f(&mem, "swapTotal").unwrap_or(0.0) > 0.0 {
        out.extend([gap(), line("Swap", &format!("{} of {}", b(f(&mem, "swapUsed")), b(f(&mem, "swapTotal")))), bar(pct(f(&mem, "swapUsed"), f(&mem, "swapTotal")))]);
    }
    out.extend([gap(), line("Disk", &format!("{} of {}", b(f(&disk, "used")), b(f(&disk, "total")))), bar(pct(f(&disk, "used"), f(&disk, "total"))), gap()]);
    if let Some(l) = sys.get("load").filter(|x| x.is_object()) {
        out.push(line("Load", &format!("{:.2}  {:.2}  {:.2}", kit::n(l, "one"), kit::n(l, "five"), kit::n(l, "fifteen"))));
    }
    if let Some(n) = sys.get("net").filter(|n| n.get("rxPerSec").is_some_and(|x| !x.is_null())) {
        out.push(line("Traffic", &format!("down {}/s   up {}/s", c::fmt_bytes(kit::n(n, "rxPerSec")), c::fmt_bytes(kit::n(n, "txPerSec")))));
    }
    out.push(line("Uptime", &fmt_dur(f(sys, "uptimeSec"))));
    out.push(line("Crundi", &format!("{} · {}", fmt_dur(f(sys, "processUptimeSec")), b(f(sys, "processRssBytes")))));
    let hn = kit::s(&host, "hostname");
    out.push(line("Machine", if hn.is_empty() { "—" } else { &hn }));
    let cm = kit::s(&host, "cpuModel");
    out.push(line("Processor", &format!("{} cores · {}", kit::n(&host, "cores") as i64, if cm.is_empty() { "—" } else { &cm })));
    let ifs = sys.get("interfaces").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    if !ifs.is_empty() {
        out.push(gap());
        for i in &ifs {
            let addrs = i.get("addresses").and_then(|x| x.as_array()).map(|a| a.iter().map(|x| kit::s(x, "address")).collect::<Vec<_>>().join("  ")).unwrap_or_default();
            out.push(line(&kit::s(i, "name"), &addrs));
        }
    }
    out
}

/// A log line: "[3:04:05 PM] words".
fn log_line(l: &Value) -> (String, String) {
    let t = c::ms_of(l, "time").map(c::time).unwrap_or_default();
    (format!("[{t}] {}", kit::s(l, "text")), kit::s(l, "level"))
}

/// The reclaim rows (renderMaintenance) and the total line.
pub fn maint_rows(tasks: &[Value], busy: &str) -> (String, Vec<OpsMaintTask>) {
    let total: f64 = tasks.iter().filter(|t| t.get("available").is_some_and(kit::truthy)).map(|t| kit::n(t, "bytes")).sum();
    let head = if total > 0.0 { format!("{} can be reclaimed", c::fmt_bytes(total)) } else { "Nothing to reclaim right now".into() };
    let rows = tasks.iter().map(|t| {
        let names: Vec<String> = t.get("details").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let avail = t.get("available").is_some_and(kit::truthy);
        let bytes = kit::n(t, "bytes");
        let danger = t.get("danger").is_some_and(kit::truthy);
        let n = names.len();
        let id = kit::s(t, "id");
        OpsMaintTask {
            label: kit::s(t, "label").into(), root: t.get("elevated").is_some_and(kit::truthy), danger,
            desc: kit::s(t, "description").into(),
            detail: if n > 0 { format!("Will remove: {}", names.join(", ")).into() } else { "".into() },
            size: if avail { c::fmt_bytes(bytes).into() } else { "—".into() },
            none: bytes <= 0.0,
            enabled: avail && (bytes > 0.0 || n > 0),
            arm: if n > 0 { format!("Remove {n}{}", if n == 1 { " container?" } else { " containers?" }).into() } else if danger { "Prune it?".into() } else { "Clear it?".into() },
            busy: if busy == id { (if danger { "Removing…" } else { "Clearing…" }).into() } else { "".into() },
            id: id.into(),
        }
    }).collect();
    (head, rows)
}

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

/// One card per person (by Telegram name, else by name), their grants under.
pub fn people_of(list: &[Value], now: f64) -> Vec<OpsCollabPerson> {
    let mut order: Vec<(String, Vec<Value>)> = Vec::new();
    for x in list {
        let tg = kit::s(x, "telegram");
        let key = if tg.is_empty() { format!("pc:{}", kit::s(x, "name").to_lowercase()) } else { format!("tg:{tg}") };
        match order.iter().position(|(k, _)| *k == key) { Some(i) => order[i].1.push(x.clone()), None => order.push((key, vec![x.clone()])) }
    }
    order.into_iter().map(|(_, rows)| {
        let first = &rows[0];
        let tg = kit::s(first, "telegram");
        let used: f64 = rows.iter().map(|r| kit::n(r, "tokensUsed")).sum();
        let lim = rows.iter().map(|r| kit::n(r, "tokenLimit")).fold(0.0, f64::max);
        let over = lim > 0.0 && used >= lim;
        let mut sites: Vec<String> = Vec::new();
        for r in &rows { for d in r.get("allowedDomains").and_then(|x| x.as_array()).cloned().unwrap_or_default() { if let Some(d) = d.as_str() { if !sites.iter().any(|s| s == d) { sites.push(d.into()); } } } }
        sites.sort();
        OpsCollabPerson {
            name: kit::s(first, "name").into(),
            tg: if tg.is_empty() { "passcode only".into() } else { format!("@{tg}").into() },
            id: kit::s(first, "id").into(),
            usage: format!("{} tokens{}{}", c::fmt_tok(used), if lim > 0.0 { format!(" of {}", c::fmt_tok(lim)) } else { " · no limit".into() }, if over { " · limit reached" } else { "" }).into(),
            over,
            meter: if lim > 0.0 { (used / lim).min(1.0) as f32 } else { -1.0 },
            limit_btn: if lim > 0.0 { "Change limit".into() } else { "Set limit".into() },
            can_reset: used > 0.0,
            sites: ModelRc::new(VecModel::from(sites.into_iter().map(Into::into).collect::<Vec<slint::SharedString>>())),
            rows: ModelRc::new(VecModel::from(rows.iter().map(|r| OpsCollabRow {
                id: kit::s(r, "id").into(), project: kit::s(r, "project").into(), branch: kit::s(r, "branch").into(),
                exp: if r.get("revoked").is_some_and(kit::truthy) { "revoked".into() } else { time_left(kit::n(r, "expiresAt"), now).into() },
                gone: r.get("expired").is_some_and(kit::truthy), revoked: r.get("revoked").is_some_and(kit::truthy),
            }).collect::<Vec<_>>())),
        }
    }).collect()
}

/// showExpiry: when the access would end, or what is wrong with the amount.
pub fn expiry_text(amount: &str, hours_unit: bool, now: f64, ends: &dyn Fn(f64, bool) -> String) -> String {
    let unit = if hours_unit { "hours" } else { "days" };
    let n = amount.trim().parse::<f64>().ok().map(|x| x.trunc() as i64).unwrap_or(0);
    if n < 1 { return format!("Enter how long, at least 1 {}.", unit.trim_end_matches('s')); }
    let hours = if hours_unit { n } else { n * 24 };
    if hours > 8760 { return "That is over a year. The most is 365 days.".into(); }
    format!("Ends {}. You can extend or revoke it at any time.", ends(now + hours as f64 * 3600000.0, hours < 48))
}

/// "6h", "3d" or "7" (days) → hours (collab-extend's prompt).
pub fn parse_extend(raw: &str) -> Option<f64> {
    let s = raw.trim();
    let num: String = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let n: f64 = num.parse().ok()?;
    if n <= 0.0 { return None; }
    let unit = s[num.len()..].trim_start().chars().next().map(|c| c.to_ascii_lowercase());
    Some(if unit == Some('h') { n } else { n * 24.0 })
}

// ─── Drawing and loading ───

fn render() {
    st(|s| s.rendered = true);
    draw_project();
    invite_projects();
    // Tunnel.
    kit::call(|api| api.get("/api/tunnel"), |r| g(|g| match r {
        Ok(t) => {
            let url = kit::s(&t, "tunnelUrl");
            let tunnels = t.get("tunnels").and_then(|x| x.as_object()).cloned().unwrap_or_default();
            let active = tunnels.values().filter(|x| kit::s(x, "status") == "active").count();
            g.set_has_tunnel(true);
            g.set_t_url(if url.is_empty() { "—".into() } else { url.into() });
            g.set_t_status(if active > 0 { "Connected".into() } else if !tunnels.is_empty() { "Connecting...".into() } else { "Not configured".into() });
            let target = kit::s(&t, "target");
            g.set_t_cf(if t.get("named").is_some_and(kit::truthy) && !target.is_empty() { format!("HTTP → {target}").into() } else { "".into() });
        }
        Err(_) => g.set_has_tunnel(false),
    }));
    // The log, then its live lines.
    kit::call(|api| api.get("/api/server-logs"), |r| {
        if let Ok(d) = r {
            let lines: Vec<(String, String)> = d.get("logs").and_then(|x| x.as_array()).cloned().unwrap_or_default().iter().map(log_line).collect();
            st(|s| s.logs = lines);
            draw_logs();
        }
    });
    kit::ws_send(&json!({ "type": "subscribe-logs" }));
    load_maint();
    load_collab();
    poll();
}

fn draw_project() {
    let alias = kit::project();
    let p = kit::projects().into_iter().find(|x| kit::s(x, "alias") == alias && !alias.is_empty());
    let services = st(|s| s.services.clone());
    g(|g| {
        let Some(p) = p else { g.set_has_project(false); return };
        let a = kit::s(&p, "alias");
        let n = kit::s(&p, "name");
        let path = kit::s(&p, "path");
        g.set_has_project(true);
        g.set_p_alias(a.clone().into());
        g.set_p_name(if n.is_empty() { a.clone().into() } else { n.into() });
        g.set_p_path(if path.is_empty() { "—".into() } else { path.into() });
        let active = kit::terminals().unwrap_or_default().iter().any(|t| kit::s(t, "project") == a && kit::s(t, "kind") != "parked");
        g.set_p_terminal(if active { "Active".into() } else { "Not started".into() });
        let mine: Vec<&Value> = services.iter().filter(|x| kit::s(x, "alias") == a).collect();
        let running = mine.iter().filter(|x| kit::s(x, "status") == "running").count();
        g.set_p_services(format!("{} registered, {running} running", mine.len()).into());
    });
}

fn draw_logs() {
    let rows: Vec<OpsInfoLog> = st(|s| s.logs.iter().map(|(t, l)| OpsInfoLog { text: t.into(), level: l.into() }).collect());
    LOGS.with(|m| kit::sync_rows(m, rows));
    g(|g| { g.set_logs_loading(false); g.set_logs_gen(g.get_logs_gen() + 1); });
}

fn append_log(m: &Value) {
    let (t, l) = log_line(m);
    let drop = st(|s| { s.logs.push((t.clone(), l.clone())); if s.logs.len() > LOG_KEEP { s.logs.remove(0); true } else { false } });
    LOGS.with(|mm| { if drop && slint::Model::row_count(&**mm) > 0 { mm.remove(0); } mm.push(OpsInfoLog { text: t.into(), level: l.into() }); });
    g(|g| { g.set_logs_loading(false); g.set_logs_gen(g.get_logs_gen() + 1); });
}

/// The machine's numbers every 2 s while Info is on show (ensureStatsPolling).
fn poll_start() {
    let t = slint::Timer::default();
    t.start(slint::TimerMode::Repeated, Duration::from_secs(2), poll);
    st(|s| s.timer = Some(t));
}

fn poll() {
    kit::call(|api| api.get("/api/stats"), |r| {
        let Ok(d) = r else { return };
        let Some(sys) = d.get("system").filter(|x| x.is_object()).cloned() else { return };
        let cpu = sys.get("cpu").cloned().unwrap_or_default();
        let overall = cpu.get("overall").and_then(|x| x.as_f64());
        let cores: Vec<f32> = cpu.get("cores").and_then(|x| x.as_array()).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
        let hist: Vec<f64> = sys.pointer("/history/cpu").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
        SYS.with(|m| kit::sync_rows(m, sys_rows(&sys)));
        g(|g| {
            g.set_sys_ready(true);
            g.set_cpu_line(fmt_pct(overall).into());
            g.set_cores(ModelRc::new(VecModel::from(cores)));
            g.set_spark(spark(&hist, 100.0).into());
            g.set_spark_pct(overall.unwrap_or(0.0) as f32);
        });
    });
}

fn load_maint() {
    kit::call(|api| api.get("/api/maintenance"), |r| match r {
        Ok(d) => { st(|s| s.maint = d.get("tasks").and_then(|x| x.as_array()).cloned().unwrap_or_default()); draw_maint(); }
        Err(_) => g(|g| g.set_maint_state("error".into())),
    });
}

fn draw_maint() {
    let (tasks, busy) = st(|s| (s.maint.clone(), s.maint_busy.clone()));
    if tasks.is_empty() { g(|g| g.set_maint_state("empty".into())); return; }
    let (head, rows) = maint_rows(&tasks, &busy);
    MAINT.with(|m| kit::sync_rows(m, rows));
    g(|g| { g.set_maint_total(head.into()); g.set_maint_state("data".into()); });
}

fn load_collab() {
    kit::call(|api| api.get("/api/collaborators"), |r| {
        let d = match r { Ok(d) if ok(&d) => d, _ => { g(|g| g.set_collab_state("error".into())); return; } };
        let list = d.get("collaborators").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let sb = d.get("sandbox").cloned().unwrap_or(json!({ "ok": false, "problems": ["Could not check the sandbox"] }));
        let people = people_of(&list, c::now_ms());
        // Each person's current limit, for the "Change limit" prompt.
        let limits: Vec<(String, f64)> = people.iter().map(|p| {
            let ids: Vec<String> = (0..slint::Model::row_count(&p.rows)).filter_map(|i| slint::Model::row_data(&p.rows, i)).map(|r| r.id.to_string()).collect();
            (p.id.to_string(), list.iter().filter(|x| ids.contains(&kit::s(x, "id"))).map(|r| kit::n(r, "tokenLimit")).fold(0.0, f64::max))
        }).collect();
        st(|s| s.people = limits);
        let problems: Vec<slint::SharedString> = sb.get("problems").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(Into::into)).collect()).unwrap_or_default();
        g(|g| {
            g.set_sb_ok(ok(&sb));
            g.set_sb_problems(ModelRc::new(VecModel::from(problems)));
            g.set_sb_fix(sb.get("canAutoFix").is_some_and(kit::truthy));
            g.set_people(ModelRc::new(VecModel::from(people)));
            g.set_collab_state("data".into());
        });
        expiry();
    });
}

fn invite_projects() {
    let picked = st(|s| s.picked.clone());
    let rows = kit::projects().iter().map(|p| {
        let a = kit::s(p, "alias");
        let n = kit::s(p, "name");
        OpsInfoCheck { on: picked.contains(&a), label: if n.is_empty() { a.clone().into() } else { n.into() }, key: a.into() }
    }).collect();
    PROJ.with(|m| kit::sync_rows(m, rows));
}

fn expiry() {
    let Some(w) = kit::ui() else { return };
    let gl = w.global::<OpsInfo>();
    gl.set_inv_expiry(expiry_text(gl.get_inv_amount().as_str(), gl.get_inv_unit() == 1, c::now_ms(), &c::ends).into());
}

// ─── Actions ───

fn act(action: &str, arg: &str) {
    let arg = arg.to_string();
    match action {
        "open-url" => c::open_url(&arg),
        "copy-logs" => {
            let text = st(|s| s.logs.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>().join("\n"));
            if c::copy_text(&text) { kit::toast("Logs copied"); } else { kit::toast_error("Copy failed"); }
        }
        "maint-run" => run_maint(arg),
        "inv-project" => { st(|s| if !s.picked.remove(&arg) { s.picked.insert(arg); }); invite_projects(); }
        "inv-edit" => expiry(),
        "collab-create" => collab_create(),
        "collab-sandbox-setup" => kit::confirm(kit::Confirm { title: "Set up the collaborator sandbox?", message: "This uses sudo on the server to install bubblewrap and socat, and adds an AppArmor profile that lets bubblewrap create sandboxes.", ok: "Set up", danger: false }, |yes| {
            if !yes { return; }
            c::post("/api/collaborators", json!({ "action": "sandboxSetup" }), |r| {
                if ok(&r) { kit::toast_ok("Sandbox ready"); } else { kit::toast_error(&c::err_of(&r, "Setup failed")); }
                load_collab();
            });
        }),
        a if a.starts_with("collab-") => collab_action(a, arg),
        "cred-copy" => {
            let text = st(|s| match arg.as_str() { "url" => s.cred[0].clone(), "name" => s.cred[1].clone(), "pass" => s.cred[2].clone(), _ => s.cred_block.clone() });
            if c::copy_text(&text) {
                g(|g| g.set_copied(arg.clone().into()));
                slint::Timer::single_shot(Duration::from_millis(1400), move || g(|g| if g.get_copied() == arg.as_str() { g.set_copied("".into()); }));
            } else { kit::toast_error("Could not copy — select the text and copy it by hand"); }
        }
        "cred-close" => g(|g| { g.set_cred_open(false); g.set_modal(false); g.set_copied("".into()); }),
        "git-cancel" => g(|g| { g.set_git_open(false); g.set_modal(false); }),
        "git-go" => {
            let project = st(|s| s.git_project.clone());
            g(|g| g.set_git_busy(true));
            c::post("/api/collaborators", json!({ "action": "initGit", "project": project }), move |r| {
                g(|g| g.set_git_busy(false));
                if !ok(&r) { kit::toast_error(&c::err_of(&r, "git init failed")); return; }
                g(|g| { g.set_git_open(false); g.set_modal(false); });
                kit::toast_ok(&format!("Set up git in {project}"));
                collab_create();
            });
        }
        _ => {}
    }
}

fn run_maint(id: String) {
    st(|s| s.maint_busy = id.clone());
    draw_maint();
    kit::call(move |api| c::post_raw(api, "/api/maintenance", &json!({ "id": id })), |r| {
        st(|s| s.maint_busy.clear());
        match r {
            Ok(d) if ok(&d) => kit::toast_ok(&match d.get("freed").and_then(|x| x.as_f64()) { None => "Done.".into(), Some(f) => format!("Freed {}", c::fmt_bytes(f)) }),
            Ok(d) => kit::toast_error(&c::err_of(&d, "The command failed")),
            Err(e) => kit::toast_error(&e),
        }
        load_maint();
        poll();
    });
}

fn collab_action(action: &str, id: String) {
    let send = move |body: Value| {
        c::post("/api/collaborators", body.clone(), move |r| {
            if !ok(&r) { kit::toast_error(&c::err_of(&r, "That did not work")); return; }
            let pass = kit::s(&r, "passcode");
            if !pass.is_empty() {
                let name = kit::s(&r, "name");
                show_invite("New passcode", if name.is_empty() { "them" } else { &name }, &pass, &[], "");
            }
            if kit::s(&body, "action") == "merge" { kit::toast_ok(&format!("Merged {} into {}", kit::s(&r, "merged"), kit::s(&r, "into"))); } else { kit::toast_ok("Done"); }
            load_collab();
        });
    };
    match action {
        "collab-extend" => kit::prompt("Extend access", "Extend access by how long? e.g. 6h, 3d, or 7 for seven days", "7d", "", "Extend", false, move |raw| {
            let Some(raw) = raw else { return };
            match parse_extend(&raw) { Some(h) => send(json!({ "id": id, "action": "update", "hours": h })), None => kit::toast_error("Try something like 6h or 3d") }
        }),
        "collab-revoke" => send(json!({ "id": id, "action": "revoke" })),
        "collab-unrevoke" => send(json!({ "id": id, "action": "update", "revoked": false })),
        "collab-limit" => {
            let cur = st(|s| s.people.iter().find(|(p, _)| *p == id).map(|x| x.1).unwrap_or(0.0));
            kit::prompt("Token limit", "Token limit for this person, across all their projects. e.g. 2M or 500k. Leave empty for no limit.", &if cur > 0.0 { c::fmt_tok(cur) } else { String::new() }, "", "Save", false, move |raw| {
                let Some(raw) = raw else { return };
                let n = if raw.trim().is_empty() { Some(0.0) } else { c::parse_tok(&raw).filter(|x| *x > 0.0) };
                match n { Some(n) => send(json!({ "id": id, "action": "setLimit", "tokenLimit": n })), None => kit::toast_error("Try something like 2M or 500k") }
            });
        }
        "collab-domain-remove" => {
            let (pid, host) = id.split_once('\n').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or_default();
            send(json!({ "id": pid, "action": "removeDomain", "host": host }));
        }
        "collab-reset-usage" => kit::confirm(kit::Confirm { title: "Reset usage", message: "Reset their token count to zero?", ok: "Reset", danger: false }, move |yes| if yes { send(json!({ "id": id, "action": "resetUsage" })); }),
        "collab-reissue" => send(json!({ "id": id, "action": "reissue" })),
        "collab-merge-owner" => send(json!({ "id": id, "action": "merge" })),
        "collab-remove" => kit::confirm(kit::Confirm { title: "Remove this access?", message: "Their working copy is deleted. Their branch and commits are kept.", ok: "Remove", danger: true }, move |yes| if yes { send(json!({ "id": id, "action": "remove" })); }),
        _ => {}
    }
}

fn collab_create() {
    let Some(w) = kit::ui() else { return };
    let gl = w.global::<OpsInfo>();
    let name = gl.get_inv_name().to_string();
    let tg = gl.get_inv_tg().to_string();
    let amount = gl.get_inv_amount().trim().parse::<f64>().ok().map(|x| x.trunc() as i64).filter(|x| *x != 0).unwrap_or(7);
    let hours = if gl.get_inv_unit() == 1 { amount } else { amount * 24 };
    let chosen: Vec<String> = kit::projects().iter().map(|p| kit::s(p, "alias")).filter(|a| st(|s| s.picked.contains(a))).collect();
    if name.trim().is_empty() { kit::toast_error("Give them a name"); return; }
    if chosen.is_empty() { kit::toast_error("Pick at least one project"); return; }
    let lim_raw = gl.get_inv_limit().to_string();
    let limit = if lim_raw.trim().is_empty() { 0.0 } else { match c::parse_tok(&lim_raw).filter(|x| *x > 0.0) { Some(n) => n, None => { kit::toast_error("Token limit: try something like 2M or 500k"); return; } } };
    let body = json!({ "action": "create", "name": name, "telegram": tg, "hours": hours, "projects": chosen, "tokenLimit": limit });
    c::post("/api/collaborators", body, move |r| {
        if !ok(&r) {
            let needs: Vec<String> = r.get("needsGit").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            if needs.len() == 1 { offer_git_init(needs[0].clone()); return; }
            kit::toast_error(&c::err_of(&r, "Could not create that"));
            return;
        }
        let made = r.get("collaborators").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let pass = kit::s(&r, "passcode");
        if !pass.is_empty() {
            let projects: Vec<String> = made.iter().map(|x| kit::s(x, "project")).collect();
            let ends = made.first().and_then(|m| c::ms_of(m, "expiresAt")).map(c::ends_long).unwrap_or_default();
            show_invite(&format!("Access created for {name}"), &name, &pass, &projects, &ends);
        } else if r.get("sharesExisting").is_some_and(kit::truthy) {
            kit::toast_ok("Added to their existing access — their passcode is unchanged");
        }
        let failed: Vec<String> = r.get("failed").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        if !failed.is_empty() { kit::toast_error(&failed.join("; ")); }
        load_collab();
    });
}

/// showInviteModal: the passcode, shown once.
fn show_invite(title: &str, name: &str, pass: &str, projects: &[String], ends: &str) {
    let url = kit::base();
    let mut block = vec![format!("Crundi access for {name}"), String::new(), format!("Sign in: {url}"), format!("Name: {name}"), format!("Passcode: {pass}")];
    if !projects.is_empty() { block.push(format!("Projects: {}", projects.join(", "))); }
    if !ends.is_empty() { block.push(format!("Access ends: {ends}")); }
    block.push(String::new());
    block.push("Open the link, choose \"I was invited to a project\", and use the name and passcode above.".into());
    let block: Vec<String> = block.into_iter().filter(|l| !l.is_empty()).collect();
    st(|s| { s.cred_block = block.join("\n"); s.cred = [url.clone(), name.to_string(), pass.to_string()]; });
    g(|g| {
        g.set_cred_title(title.into()); g.set_cred_url(url.into()); g.set_cred_name(name.into()); g.set_cred_pass(pass.into());
        g.set_cred_projects(projects.join(", ").into()); g.set_cred_ends(if projects.is_empty() { "".into() } else { ends.into() });
        g.set_copied("".into()); g.set_cred_open(true); g.set_modal(true);
    });
}

/// offerGitInit: what making the project a repository would commit.
fn offer_git_init(project: String) {
    let p2 = project.clone();
    c::post("/api/collaborators", json!({ "action": "gitPreview", "project": project }), move |p| {
        if !ok(&p) { kit::toast_error(&c::err_of(&p, "Could not inspect that project")); return; }
        if p.get("alreadyGit").is_some_and(kit::truthy) { collab_create(); return; }
        let (commit, kept, risky, tail) = git_words(&p);
        st(|s| s.git_project = p2.clone());
        g(|g| {
            g.set_git_title(format!("{p2} is not a git repository yet").into());
            g.set_git_commit(commit.into()); g.set_git_kept(kept.into()); g.set_git_risky(risky.into()); g.set_git_tail(tail.into());
            g.set_git_busy(false); g.set_git_open(true); g.set_modal(true);
        });
    });
}

/// The git dialog's lines: would commit, kept out, looks sensitive, the .gitignore.
pub fn git_words(p: &Value) -> (String, String, String, String) {
    let mb = kit::n(p, "bytes") / 1048576.0;
    let mut commit = format!("Would commit: {} files, {mb:.1} MB", kit::n(p, "files") as i64);
    if p.get("truncated").is_some_and(kit::truthy) { commit += " (stopped counting at 20,000)"; }
    let ign: Vec<String> = p.get("ignoring").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if !ign.is_empty() { commit += &format!("\nSkipping: {}", ign.join(", ")); }
    let list = |k: &str| p.get(k).and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let ex = list("excluded");
    let kept = if ex.is_empty() { String::new() } else { format!("Kept out of git:\n{}", ex.iter().map(|x| format!("{} ({})", kit::s(x, "path"), kit::s(x, "why"))).collect::<Vec<_>>().join("\n")) };
    let rk = list("risky");
    let risky = if rk.is_empty() { String::new() } else { format!("These would be committed and look sensitive:\n{}\n\nMove or delete them first if that is not what you want.", rk.iter().map(|x| format!("{} ({})", kit::s(x, "path"), kit::s(x, "why"))).collect::<Vec<_>>().join("\n")) };
    let tail = if p.get("wouldWriteIgnore").is_some_and(kit::truthy) { "A .gitignore will be written first, keeping dependencies, build output and secrets out. You can edit it afterwards." } else { "This project already has a .gitignore, so git decides what is included." };
    (commit, kept, risky, tail.into())
}

// ─── The driver ───

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
///   info-act <action> <arg>    a button (OpsInfo.act), as a click
///   info-field <name> <text>   type into the invite form
///   info-log <text> [level]    a live log line, as /ws would bring it
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "info-act" => { act(&a(0), &a(1)); Some(json!({ "ok": true })) }
        "info-log" => { append_log(&json!({ "time": c::now_ms(), "level": if a(1).is_empty() { "info".into() } else { a(1) }, "text": a(0) })); Some(json!({ "ok": true })) }
        "info-field" => {
            let w = kit::ui()?;
            let g = w.global::<OpsInfo>();
            let v: slint::SharedString = a(1).into();
            match a(0).as_str() {
                "name" => g.set_inv_name(v), "tg" => g.set_inv_tg(v), "amount" => g.set_inv_amount(v),
                "unit" => g.set_inv_unit(a(1).parse().unwrap_or(0)), "limit" => g.set_inv_limit(v),
                _ => return Some(json!({ "ok": false, "error": "unknown field" })),
            }
            expiry();
            Some(json!({ "ok": true }))
        }
        _ => None,
    }
}

/// This screen's part of the area's props (under "info").
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return Value::Null };
    let g = w.global::<OpsInfo>();
    let (logs, last) = st(|s| (s.logs.len(), s.logs.last().map(|x| x.0.clone()).unwrap_or_default()));
    let sys: Vec<Value> = SYS.with(|m| (0..slint::Model::row_count(&**m)).filter_map(|i| slint::Model::row_data(&**m, i)).filter(|r| r.kind == "line").map(|r| json!([r.k.as_str(), r.v.as_str()])).collect());
    let maint: Vec<Value> = MAINT.with(|m| (0..slint::Model::row_count(&**m)).filter_map(|i| slint::Model::row_data(&**m, i)).map(|t| json!({ "id": t.id.as_str(), "label": t.label.as_str(), "size": t.size.as_str(), "enabled": t.enabled })).collect());
    let projects: Vec<Value> = PROJ.with(|m| (0..slint::Model::row_count(&**m)).filter_map(|i| slint::Model::row_data(&**m, i)).map(|p| json!({ "key": p.key.as_str(), "on": p.on })).collect());
    json!({
        "shown": st(|s| s.shown), "polling": st(|s| s.timer.is_some()),
        "project": if g.get_has_project() { json!({ "alias": g.get_p_alias().as_str(), "name": g.get_p_name().as_str(), "path": g.get_p_path().as_str(), "terminal": g.get_p_terminal().as_str(), "services": g.get_p_services().as_str() }) } else { Value::Null },
        "sys": { "ready": g.get_sys_ready(), "cpu": g.get_cpu_line().as_str(), "cores": slint::Model::row_count(&g.get_cores()), "spark": !g.get_spark().is_empty(), "rows": sys },
        "maint": { "state": g.get_maint_state().as_str(), "total": g.get_maint_total().as_str(), "tasks": maint },
        "collab": { "state": g.get_collab_state().as_str(), "sandboxOk": g.get_sb_ok(), "people": slint::Model::row_count(&g.get_people()), "expiry": g.get_inv_expiry().as_str(), "projects": projects },
        "tunnel": if g.get_has_tunnel() { json!({ "url": g.get_t_url().as_str(), "status": g.get_t_status().as_str(), "cf": g.get_t_cf().as_str() }) } else { Value::Null },
        "logs": logs, "lastLog": last, "logsLoading": g.get_logs_loading(),
        "cred": g.get_cred_open(), "git": g.get_git_open(), "copied": g.get_copied().as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(fmt_pct(Some(12.345)), "12.3%");
        assert_eq!(fmt_pct(Some(150.0)), "150%");
        assert_eq!(fmt_pct(None), "—");
        assert_eq!(fmt_dur(Some(2.0 * 86400.0 + 3.0 * 3600.0)), "2d 3h");
        assert_eq!(fmt_dur(Some(3700.0)), "1h 1m");
        assert_eq!(fmt_dur(Some(59.0)), "0m");
    }

    #[test]
    fn sparkline() {
        assert_eq!(spark(&[50.0], 100.0), "");
        assert_eq!(spark(&[0.0, 100.0], 100.0), "M0.00 19.00L100.00 1.00");
    }

    #[test]
    fn machine_rows() {
        let r = sys_rows(&json!({ "mem": { "used": 1024, "total": 2048, "swapTotal": 0 }, "disk": { "used": 1, "total": 4 }, "uptimeSec": 60, "host": { "hostname": "box", "cores": 4, "cpuModel": "X" }, "interfaces": [{ "name": "eth0", "addresses": [{ "address": "10.0.0.2" }] }] }));
        let lines: Vec<(String, String)> = r.iter().filter(|x| x.kind == "line").map(|x| (x.k.to_string(), x.v.to_string())).collect();
        assert_eq!(lines[0], ("Memory".into(), "1.0 KB of 2.0 KB".into()));
        assert!(lines.iter().all(|l| l.0 != "Swap" && l.0 != "Load" && l.0 != "Traffic"));
        assert!(lines.contains(&("Processor".into(), "4 cores · X".into())));
        assert_eq!(lines.last().unwrap(), &("eth0".to_string(), "10.0.0.2".to_string()));
        let bars: Vec<f32> = r.iter().filter(|x| x.kind == "bar").map(|x| x.pct).collect();
        assert_eq!(bars, vec![50.0, 25.0]);
    }

    #[test]
    fn reclaim() {
        let (head, rows) = maint_rows(&[
            json!({ "id": "a", "label": "Cache", "available": true, "bytes": 2048, "description": "d" }),
            json!({ "id": "b", "label": "Containers", "available": true, "bytes": 0, "danger": true, "details": ["x", "y"] }),
            json!({ "id": "c", "label": "Gone", "available": false, "bytes": 0 }),
        ], "a");
        assert_eq!(head, "2.0 KB can be reclaimed");
        assert_eq!(rows[0].busy.as_str(), "Clearing…");
        assert_eq!(rows[1].arm.as_str(), "Remove 2 containers?");
        assert!(rows[1].enabled && rows[1].detail.as_str() == "Will remove: x, y");
        assert!(!rows[2].enabled && rows[2].size.as_str() == "—");
    }

    #[test]
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
        assert_eq!(p[0].usage.as_str(), "600k tokens of 500k · limit reached");
        assert!(p[0].over && p[0].meter == 1.0);
        assert_eq!(slint::Model::row_count(&p[0].rows), 2);
        assert_eq!(slint::Model::row_data(&p[0].sites, 0).unwrap().as_str(), "a.com");
        assert_eq!(slint::Model::row_data(&p[0].rows, 1).unwrap().exp.as_str(), "revoked");
        assert_eq!(p[1].tg.as_str(), "@ann");
        assert_eq!(p[1].usage.as_str(), "0 tokens · no limit");
        assert_eq!(p[1].limit_btn.as_str(), "Set limit");
    }

    #[test]
    fn invite_words() {
        let ends = |ms: f64, short: bool| format!("{}{}", ms as i64, if short { "s" } else { "l" });
        assert_eq!(expiry_text("0", false, 0.0, &ends), "Enter how long, at least 1 day.");
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
    }
}
