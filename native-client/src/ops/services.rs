//! Ops area: Services (the web's loadServices / renderServices,
//! src/webapp-html.js ~12860-14125): the list, register, start/stop/restart,
//! logs, delete, exposure (port, tunnel, subdomain and path forwards), the
//! forwards no card shows, and the live usage strip (/api/stats every 2s
//! while the tab is on show, as ensureStatsPolling).
//! UI: ui/ops/services.slint (global OpsServices).
//!
//! The native client is the owner's (invitees use the web, DESIGN.md rule 4),
//! so every card can be edited and "Other forwards" is always offered.

use crate::{areas::AreaEvent, kit, MainWindow, OpsServices, OpsSvcCard, OpsSvcForward};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// The sparklines' viewbox (the web's sparkSvg: 100 × 20).
const SPARK_W: f64 = 100.0;
const SPARK_H: f64 = 20.0;
/// The stats poll (ensureStatsPolling: 2s).
const POLL_MS: u64 = 2000;

/// The naming form open in one card (the web's exposeEdit).
#[derive(Clone, Debug, PartialEq)]
struct Edit {
    key: String,
    port: i64,
    mode: String,
    name: String,
    public: bool,
    gen: i32,
}

#[derive(Default)]
struct St {
    shown: bool,
    loaded: bool,
    loading: bool,
    error: String,
    services: Vec<Value>,
    forwards: Vec<Value>,
    opts: Option<Value>,
    /// The last /api/stats "services" map.
    stats: Option<Value>,
    edit: Option<Edit>,
    /// Open logs: key → text.
    logs: HashMap<String, String>,
    timer: Option<slint::Timer>,
    /// What the last "state" said about the services (a change reloads).
    sig: String,
    last_url: String,
    gen: i32,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St::default());
    static CARDS: Rc<VecModel<OpsSvcCard>> = kit::model();
    static OTHERS: Rc<VecModel<OpsSvcForward>> = kit::model();
    static PROJECTS: Rc<VecModel<slint::SharedString>> = kit::model();
}

fn st<R>(f: impl FnOnce(&mut St) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

fn g(w: &MainWindow) -> OpsServices<'_> {
    w.global::<OpsServices>()
}

// ─── The registry ───

pub fn init(w: &MainWindow) {
    let o = g(w);
    o.set_cards(CARDS.with(kit::rc));
    o.set_others(OTHERS.with(kit::rc));
    o.set_projects(PROJECTS.with(kit::rc));
    o.set_spark_w(SPARK_W as f32);
    o.set_spark_h(SPARK_H as f32);
    o.on_reload(load);
    o.on_register_open(register_open);
    o.on_register_submit(register_submit);
    o.on_register_cancel(register_cancel);
    o.on_act(|key, a| act(key.as_str(), a.as_str()));
    o.on_expose(|key, i| expose(key.as_str(), i));
    o.on_edit_name(|key, name| st(|s| if let Some(e) = s.edit.as_mut().filter(|e| e.key == key.as_str()) { e.name = name.to_string(); }));
    o.on_edit_public(|key, on| { st(|s| if let Some(e) = s.edit.as_mut().filter(|e| e.key == key.as_str()) { e.public = on; }); render(); });
    o.on_edit_save(|key| save_expose(key.as_str()));
    o.on_edit_cancel(|| { st(|s| s.edit = None); render(); });
    o.on_fwd_remove(|host| remove_forward(host.as_str()));
    o.on_fwd_project(|host, i| {
        let alias = if i <= 0 { String::new() } else { PROJECTS.with(|m| m.row_data(i as usize)).map(|s| s.to_string()).unwrap_or_default() };
        set_forward_project(host.as_str(), &alias);
    });
    o.on_open_url(|u| open_url(u.as_str()));
}

pub fn signed_in() {
    if st(|s| s.shown) { load(); }
}

pub fn signed_out() {
    st(|s| { let shown = s.shown; *s = St::default(); s.shown = shown; });
    CARDS.with(|m| kit::sync_rows(m, vec![]));
    OTHERS.with(|m| kit::sync_rows(m, vec![]));
    if let Some(w) = kit::ui() {
        let o = g(&w);
        o.set_loaded(false);
        o.set_loading(false);
        o.set_error("".into());
        o.set_reg_open(false);
    }
}

pub fn project(_alias: &str) {
    // The list is filtered by the project (and the title says "all projects"
    // without one); the edit form belongs to a card that may be gone.
    st(|s| s.edit = None);
    render();
    if st(|s| s.shown) { load(); }
}

pub fn tab_shown(tab: &str) {
    let on = tab == "services";
    let was = st(|s| std::mem::replace(&mut s.shown, on));
    if on && !was { load(); }
    polling(on);
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::State(v) => {
            // The web refreshes the cards on its own actions only; the state
            // stream here also brings a service that started, stopped or was
            // registered elsewhere (by Claude, say) onto the open tab.
            let Some(list) = v.get("services").and_then(|x| x.as_array()) else { return };
            let sig = signature(list);
            let changed = st(|s| { let c = s.sig != sig; s.sig = sig; c });
            if changed && st(|s| s.shown && s.loaded) { load(); }
        }
        AreaEvent::Connected(true) => if st(|s| s.shown) { load(); },
        _ => {}
    }
}

// ─── Loading ───

/// loadServices: the services, then the forwards (and their options, once).
pub fn load() {
    if kit::api().is_none() { return; }
    st(|s| s.loading = true);
    if let Some(w) = kit::ui() { g(&w).set_loading(true); }
    let need_opts = st(|s| s.opts.is_none());
    kit::call(
        move |api| {
            let svc = api.get("/api/services")?;
            // The forwards are best effort: the card falls back to tunnel-only.
            let fwd = api.get("/api/forwards").unwrap_or(Value::Null);
            let opts = if need_opts { api.get("/api/forwards/options").unwrap_or(Value::Null) } else { Value::Null };
            let stats = api.get("/api/stats").unwrap_or(Value::Null);
            Ok(json!({ "svc": svc, "fwd": fwd, "opts": opts, "stats": stats }))
        },
        |r| {
            let (svc, fwd, opts, stats) = match r {
                Ok(mut v) => (Ok(v["svc"].take()), Some(v["fwd"].take()).filter(|x| x.is_object()), Some(v["opts"].take()).filter(|x| x.is_object()), Some(v["stats"].take()).filter(|x| x.is_object())),
                Err(e) => (Err(e), None, None, None),
            };
            st(|s| {
                s.loading = false;
                match svc {
                    Ok(v) => {
                        s.services = v.get("services").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                        s.loaded = true;
                        s.error.clear();
                    }
                    Err(e) => {
                        s.error = e.clone();
                        if s.loaded { kit::toast_error(&format!("Failed to load services: {e}")); }
                    }
                }
                if let Some(f) = fwd { s.forwards = f.get("forwards").and_then(|x| x.as_array()).cloned().unwrap_or_default(); }
                if let Some(o) = opts.filter(|o| kit::truthy(o.get("ok").unwrap_or(&Value::Null))) { s.opts = Some(o); }
                if let Some(x) = stats.filter(|x| x.is_object() && x.get("error").is_none()) { s.stats = x.get("services").cloned(); }
                // Logs of a service that is gone close.
                let keys: Vec<String> = s.services.iter().map(|x| kit::s(x, "key")).collect();
                s.logs.retain(|k, _| keys.contains(k));
                if s.edit.as_ref().is_some_and(|e| !keys.contains(&e.key)) { s.edit = None; }
            });
            render();
        },
    );
}

/// The 2s stats poll, only while the tab is on show.
fn polling(on: bool) {
    if !on {
        st(|s| s.timer = None);
        return;
    }
    if st(|s| s.timer.is_some()) { return; }
    let t = slint::Timer::default();
    t.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(POLL_MS), poll_stats);
    st(|s| s.timer = Some(t));
}

fn poll_stats() {
    if kit::api().is_none() { return; }
    kit::call(|api| api.get("/api/stats"), |r| {
        // A refused or failed poll says nothing about the services: keep the strips.
        let Ok(d) = r else { return };
        if !d.is_object() || d.get("error").is_some() { return; }
        st(|s| s.stats = d.get("services").cloned());
        render();
    });
}

/// The services' state in a line, for noticing a change in "state".
fn signature(list: &[Value]) -> String {
    list.iter()
        .map(|x| format!("{}|{}|{}|{}|{}", kit::s(x, "key"), kit::s(x, "status"), kit::s(x, "tunnelPort"), kit::s(x, "tunnelEnabled"), x.pointer("/tunnel/status").and_then(|v| v.as_str()).or_else(|| x.get("tunnelStatus").and_then(|v| v.as_str())).unwrap_or("")))
        .collect::<Vec<_>>()
        .join(";")
}

// ─── Rendering (pure parts first) ───

/// fmtBytes: "—" for nothing, B under a KB, then one decimal under 100.
pub(crate) fn fmt_bytes(n: Option<f64>) -> String {
    let Some(n) = n.filter(|n| n.is_finite()) else { return "\u{2014}".into() };
    if n < 1024.0 { return format!("{} B", n.round() as i64); }
    let units = ["KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (n / 1024.0, 0);
    while v >= 1024.0 && i < units.len() - 1 { v /= 1024.0; i += 1; }
    if v >= 100.0 { format!("{} {}", v.round() as i64, units[i]) } else { format!("{:.1} {}", v, units[i]) }
}

/// fmtPct: "—" for nothing, whole from 100.
pub(crate) fn fmt_pct(n: Option<f64>) -> String {
    match n {
        None => "\u{2014}".into(),
        Some(n) if n >= 100.0 => format!("{}%", n.round() as i64),
        Some(n) => format!("{n:.1}%"),
    }
}

/// lvl: the tone for a load (green until it matters).
pub(crate) fn level(pct: f64) -> &'static str {
    if pct >= 85.0 { "error" } else if pct >= 60.0 { "warn" } else { "ok" }
}

/// sparkSvg: the line and the zero-anchored area in a 100 × 20 viewbox;
/// nothing below two samples (one sample is not a trend).
pub(crate) fn spark(vals: &[f64], max: f64) -> (String, String) {
    if vals.len() < 2 || max <= 0.0 { return (String::new(), String::new()); }
    let n = vals.len();
    let mut d = String::new();
    for (i, v) in vals.iter().enumerate() {
        let x = i as f64 / (n - 1) as f64 * SPARK_W;
        let r = (v / max).clamp(0.0, 1.0);
        let y = (SPARK_H - 1.0) - r * (SPARK_H - 2.0);
        d.push_str(&format!("{}{:.2} {:.2}", if i == 0 { "M" } else { "L" }, x, y));
    }
    let area = format!("{d}L{SPARK_W} {SPARK_H}L0 {SPARK_H}Z");
    (d, area)
}

fn nums(v: Option<&Value>) -> Vec<f64> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default()
}

fn max_of(vals: &[f64], floor: f64) -> f64 {
    vals.iter().fold(floor, |m, v| m.max(*v))
}

fn opt_num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| x.as_f64())
}

/// The defaultForwardName: the key's last part, as a hostname.
pub(crate) fn default_forward_name(svc: &Value) -> String {
    let raw = { let n = kit::s(svc, "name"); if n.is_empty() { kit::s(svc, "key") } else { n } };
    let raw = raw.rsplit(':').next().unwrap_or("").to_lowercase();
    let mut out = String::new();
    let mut dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' { out.push(ch); dash = false; }
        else if !dash { out.push('-'); dash = true; }
    }
    let t: String = out.trim_matches('-').chars().take(32).collect();
    if t.is_empty() { "app".into() } else { t }
}

/// The forward name rule (the server's NAME_RE).
pub(crate) fn valid_name(n: &str) -> bool {
    let b = n.as_bytes();
    let ok = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty() && b.len() <= 32 && ok(b[0]) && ok(b[b.len() - 1]) && b.iter().all(|&c| ok(c) || c == b'-')
}

fn mode_index(m: &str) -> i32 {
    match m { "tunnel" => 1, "subdomain" => 2, "path" => 3, _ => 0 }
}

fn port_of(v: &Value, k: &str) -> i64 {
    kit::n(v, k) as i64
}

/// The forward on a port.
fn forward_for(forwards: &[Value], port: i64) -> Option<&Value> {
    if port <= 0 { return None; }
    forwards.iter().find(|f| port_of(f, "port") == port)
}

fn sub_domain(opts: Option<&Value>) -> (bool, String) {
    let Some(o) = opts else { return (false, String::new()) };
    let sub = o.get("choices").and_then(|c| c.as_array()).and_then(|c| c.iter().find(|x| kit::s(x, "id") == "subdomain"));
    let avail = sub.is_some_and(|s| kit::truthy(s.get("available").unwrap_or(&Value::Null)));
    let dom = sub.map(|s| kit::s(s, "suggestedDomain")).filter(|d| !d.is_empty()).unwrap_or_else(|| kit::s(o, "domain"));
    (avail, dom)
}

/// One card from a service, the forwards, the stats, and the open form and logs.
fn card(s: &Value, forwards: &[Value], opts: Option<&Value>, stats: Option<&Value>, edit: Option<&Edit>, logs: Option<&String>) -> OpsSvcCard {
    let key = kit::s(s, "key");
    let status = kit::s(s, "status");
    let running = status == "running";
    let tone = if running { "running" } else if status == "error" { "error" } else { "" };
    let t_port = port_of(s, "tunnelPort");
    let t_enabled = kit::truthy(s.get("tunnelEnabled").unwrap_or(&Value::Null));
    let t_status = kit::s(s, "tunnelStatus");
    let fwd = forward_for(forwards, t_port);
    let mode = fwd.map(|f| kit::s(f, "mode")).unwrap_or_else(|| if t_enabled { "tunnel".into() } else { String::new() });

    let mut c = OpsSvcCard {
        key: key.clone().into(),
        name: { let n = kit::s(s, "name"); if n.is_empty() { key.clone() } else { n } }.into(),
        status: status.to_uppercase().into(),
        tone: tone.into(),
        running,
        command: kit::s(s, "command").into(),
        uptime: { let u = kit::s(s, "uptime"); if u.is_empty() { String::new() } else { format!("\u{b7} up {u}") } }.into(),
        tunnel_btn: !(t_port > 0 || t_enabled),
        expose: t_port > 0 || t_enabled || fwd.is_some(),
        port_label: if t_port > 0 { format!("Port {t_port}") } else { "Set port".into() }.into(),
        mode: mode_index(&mode),
        ..Default::default()
    };

    // The usage strip: the listing's numbers, the poll's where it has the service.
    if running {
        let mut st = json!({ "cpuPct": s.get("cpuPct"), "memory": s.get("memory"), "pid": s.get("pid"), "containers": s.get("containers") });
        if let Some(p) = stats.and_then(|m| m.get(&key)).and_then(|p| p.as_object()) {
            for (k, v) in p { st[k] = v.clone(); }
        }
        let cpu_h = nums(st.get("cpuHistory"));
        let mem_h = nums(st.get("memHistory"));
        let cpu = opt_num(&st, "cpuPct");
        let (cl, ca) = spark(&cpu_h, max_of(&cpu_h, 100.0));
        let (ml, ma) = spark(&mem_h, max_of(&mem_h, 0.0) * 1.25);
        c.stats = true;
        c.cpu = fmt_pct(cpu).into();
        c.cpu_tone = level(cpu.unwrap_or(0.0)).into();
        c.cpu_line = cl.into();
        c.cpu_area = ca.into();
        c.mem = fmt_bytes(opt_num(&st, "memory")).into();
        c.mem_line = ml.into();
        c.mem_area = ma.into();
        c.badge = match st.get("containers").and_then(|x| x.as_array()) {
            Some(a) if a.is_empty() => "no containers".into(),
            Some(a) => format!("{} {}", a.len(), if a.len() == 1 { "container" } else { "containers" }),
            None if kit::truthy(st.get("pid").unwrap_or(&Value::Null)) => format!("pid {}", kit::s(&st, "pid")),
            None => String::new(),
        }.into();
    }

    // The exposure row's address or state word.
    if let Some(f) = fwd {
        c.link = kit::s(f, "url").into();
        c.public = kit::truthy(f.get("public").unwrap_or(&Value::Null));
        c.fwd_host = kit::s(f, "host").into();
    } else if t_enabled && t_status == "active" && !kit::s(s, "tunnelUrl").is_empty() {
        c.link = kit::s(s, "tunnelUrl").into();
    } else {
        let (w, t) = if t_enabled && t_status == "connecting" { ("starting…", "warn") }
            else if t_enabled && t_status == "error" { ("error", "error") }
            else if t_enabled { ("idle", "") }
            else { ("off", "") };
        c.state_word = w.into();
        c.state_tone = t.into();
    }

    if let Some(e) = edit.filter(|e| e.key == key && c.expose) {
        let (_, dom) = sub_domain(opts);
        c.editing = true;
        c.edit_prefix = if e.mode == "path" { "/tunnel/" } else { "" }.into();
        c.edit_suffix = if e.mode == "subdomain" { format!(".{dom}") } else { "/".into() }.into();
        c.edit_name = e.name.clone().into();
        c.edit_public = e.public;
        c.edit_gen = e.gen;
    }
    if let Some(l) = logs {
        c.logs_open = true;
        c.logs = l.clone().into();
    }
    c
}

/// otherForwardsHtml: forwards on ports no service records, in this project
/// (or in none).
fn other_forwards(services: &[Value], forwards: &[Value], project: &str) -> Vec<Value> {
    let shown: Vec<i64> = services.iter().map(|s| port_of(s, "tunnelPort")).filter(|p| *p > 0).collect();
    forwards.iter()
        .filter(|f| !shown.contains(&port_of(f, "port")))
        .filter(|f| { let p = kit::s(f, "project"); project.is_empty() || p.is_empty() || p == project })
        .cloned()
        .collect()
}

fn render() {
    let Some(w) = kit::ui() else { return };
    let project = kit::project();
    let aliases: Vec<String> = kit::projects().iter().map(|p| kit::s(p, "alias")).filter(|a| !a.is_empty()).collect();
    let (cards, others, loaded, loading, error) = st(|s| {
        let list: Vec<&Value> = s.services.iter().filter(|x| project.is_empty() || kit::s(x, "alias") == project).collect();
        let cards: Vec<OpsSvcCard> = list.iter().map(|x| {
            let key = kit::s(x, "key");
            card(x, &s.forwards, s.opts.as_ref(), s.stats.as_ref(), s.edit.as_ref(), s.logs.get(&key))
        }).collect();
        let others: Vec<OpsSvcForward> = other_forwards(&s.services, &s.forwards, &project).iter().map(|f| {
            let p = kit::s(f, "project");
            OpsSvcForward {
                host: kit::s(f, "host").into(),
                url: { let u = kit::s(f, "url"); if u.is_empty() { kit::s(f, "host") } else { u } }.into(),
                port: kit::s(f, "port").into(),
                public: kit::truthy(f.get("public").unwrap_or(&Value::Null)),
                project: aliases.iter().position(|a| *a == p).map(|i| i as i32 + 1).unwrap_or(0),
            }
        }).collect();
        (cards, others, s.loaded, s.loading, s.error.clone())
    });
    CARDS.with(|m| kit::sync_rows(m, cards));
    OTHERS.with(|m| kit::sync_rows(m, others));
    let mut opts = vec![slint::SharedString::from("No project")];
    opts.extend(aliases.iter().map(|a| slint::SharedString::from(a.as_str())));
    PROJECTS.with(|m| kit::sync_rows(m, opts));
    let o = g(&w);
    o.set_title(if project.is_empty() { "Services \u{b7} all projects" } else { "Services" }.into());
    o.set_empty(if project.is_empty() { "No services registered" } else { "No services registered for this project" }.into());
    o.set_loaded(loaded);
    o.set_loading(loading);
    o.set_error(error.into());
}

// ─── Actions ───

fn service(key: &str) -> Option<Value> {
    st(|s| s.services.iter().find(|x| kit::s(x, "key") == key).cloned())
}

fn reload_soon(ms: u64) {
    slint::Timer::single_shot(std::time::Duration::from_millis(ms), load);
}

/// A {ok, error} answer: Err(words) when it says no.
fn ok_of(r: Result<Value, String>, fallback: &str) -> Result<Value, String> {
    match r {
        Ok(v) if v.get("ok").and_then(|x| x.as_bool()) == Some(false) => Err({ let e = kit::s(&v, "error"); if e.is_empty() { fallback.into() } else { e } }),
        other => other,
    }
}

pub fn act(key: &str, action: &str) {
    match action {
        "start" | "stop" | "restart" => {
            let (k, a) = (key.to_string(), action.to_string());
            kit::call(
                { let (k, a) = (k.clone(), a.clone()); move |api| api.post(&format!("/api/services/{}/{}", kit::enc(&k), a), &json!({})) },
                move |r| {
                    match ok_of(r, "Failed") {
                        Ok(_) => kit::toast_ok(&format!("{a} {k}")),
                        Err(e) => kit::toast_error(&e),
                    }
                    reload_soon(500);
                },
            );
        }
        "logs" => logs(key),
        "port" => set_port(key),
        "delete" => delete(key),
        _ => {}
    }
}

/// svcLogs: the inline logs, a toggle.
fn logs(key: &str) {
    if st(|s| s.logs.remove(key).is_some()) { render(); return; }
    let k = key.to_string();
    kit::call(
        { let k = k.clone(); move |api| api.get(&format!("/api/services/{}/logs", kit::enc(&k))) },
        move |r| {
            let text = match r {
                Ok(v) => {
                    let lines: Vec<String> = v.get("logs").and_then(|x| x.as_array()).map(|a| a.iter().map(|l| l.as_str().map(String::from).unwrap_or_else(|| l.to_string())).collect()).unwrap_or_default();
                    let t = lines.join("\n");
                    if t.is_empty() { "(no logs)".into() } else { t }
                }
                Err(e) => format!("Error: {e}"),
            };
            st(|s| s.logs.insert(k, text));
            render();
        },
    );
}

/// svcTunnelSetPort: the port to expose (0 or blank clears it).
fn set_port(key: &str) {
    let cur = service(key).map(|s| port_of(&s, "tunnelPort")).unwrap_or(0);
    let k = key.to_string();
    kit::prompt(
        "Tunnel port",
        "Local port to expose via Cloudflare. Enter 0 (or blank) to clear.",
        &if cur > 0 { cur.to_string() } else { String::new() },
        "",
        "OK",
        false,
        move |v| {
            let Some(v) = v else { return };
            let port: i64 = parse_int(&v);
            let msg = if port > 0 { format!("Tunnel port set to :{port}") } else { "Tunnel port cleared".into() };
            tunnel_post(&k, json!({ "port": port }), msg);
        },
    );
}

/// parseInt(v, 10) || 0: the leading digits.
pub(crate) fn parse_int(v: &str) -> i64 {
    let t = v.trim();
    let (neg, rest) = match t.strip_prefix('-') { Some(r) => (true, r), None => (false, t.strip_prefix('+').unwrap_or(t)) };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: i64 = digits.parse().unwrap_or(0);
    if neg { -n } else { n }
}

fn tunnel_post(key: &str, body: Value, ok_msg: String) {
    let k = key.to_string();
    kit::call(
        move |api| api.post(&format!("/api/services/{}/tunnel", kit::enc(&k)), &body),
        move |r| {
            match ok_of(r, "Failed") {
                Ok(_) => kit::toast_ok(&ok_msg),
                Err(e) => kit::toast_error(&e),
            }
            reload_soon(600);
        },
    );
}

/// svcDelete: asks first.
fn delete(key: &str) {
    let k = key.to_string();
    let msg = format!("Delete service \"{key}\"?");
    kit::confirm(kit::Confirm { title: "Delete service", message: &msg, ok: "Delete", danger: true }, move |ok| {
        if !ok { return; }
        kit::call(
            { let k = k.clone(); move |api| api.post(&format!("/api/services/{}/delete", kit::enc(&k)), &json!({})) },
            move |r| {
                match ok_of(r, "Failed") {
                    Ok(_) => kit::toast_ok(&format!("Deleted {k}")),
                    Err(e) => kit::toast_error(&e),
                }
                reload_soon(500);
            },
        );
    });
}

/// The switch: Off and Tunnel act at once; Subdomain and Path open the naming form.
pub fn expose(key: &str, i: i32) {
    let mode = ["off", "tunnel", "subdomain", "path"].get(i as usize).copied().unwrap_or("off");
    let port = service(key).map(|s| port_of(&s, "tunnelPort")).unwrap_or(0);
    if mode == "subdomain" || mode == "path" {
        begin_expose(key, port, mode);
    } else {
        st(|s| s.edit = None);
        render();
        expose_now(key, port, mode);
    }
}

/// exposeService: off, or a quick tunnel; whatever is there now goes first.
fn expose_now(key: &str, port: i64, mode: &str) {
    if port <= 0 { kit::toast_error("Set a port first"); return; }
    let existing = st(|s| forward_for(&s.forwards, port).map(|f| kit::s(f, "host")));
    let (k, m) = (key.to_string(), mode.to_string());
    kit::call(
        move |api| {
            if let Some(h) = &existing { let _ = api.delete(&format!("/api/forwards/{}", kit::enc(h))); }
            let path = format!("/api/services/{}/tunnel", kit::enc(&k));
            let off = api.post(&path, &json!({ "enabled": false, "port": port }))?;
            if off.get("ok").and_then(|x| x.as_bool()) == Some(false) {
                let e = kit::s(&off, "error");
                return Err(kit::ApiError::Status(400, if e.is_empty() { "could not turn the tunnel off".into() } else { e }));
            }
            if m == "off" { return Ok(json!({ "ok": true, "off": true })); }
            api.post(&path, &json!({ "enabled": true, "port": port }))
        },
        |r| {
            match r {
                Ok(d) if kit::truthy(d.get("off").unwrap_or(&Value::Null)) => kit::toast_ok("No longer exposed"),
                Ok(d) if kit::truthy(d.get("pendingApproval").unwrap_or(&Value::Null)) => kit::toast_ok("Sent to the owner to approve"),
                Ok(d) if d.get("ok").and_then(|x| x.as_bool()) == Some(false) => kit::toast_error(&{ let e = kit::s(&d, "error"); if e.is_empty() { "Could not start the tunnel".into() } else { e } }),
                Ok(_) => kit::toast_ok("Starting a tunnel"),
                Err(e) => kit::toast_error(&format!("Failed: {e}")),
            }
            load();
        },
    );
}

/// beginExpose: the naming form in the card; nothing is made until Save.
fn begin_expose(key: &str, port: i64, mode: &str) {
    if port <= 0 { kit::toast_error("Set a port first"); return; }
    let (avail, _) = st(|s| sub_domain(s.opts.as_ref()));
    if mode == "subdomain" && !avail {
        kit::toast_error("No domain is configured. Set TLS_DOMAIN or FORWARD_DOMAIN, or use Path.");
        return;
    }
    let svc = service(key);
    st(|s| {
        let existing = forward_for(&s.forwards, port);
        s.gen += 1;
        s.edit = Some(Edit {
            key: key.into(),
            port,
            mode: mode.into(),
            name: existing.map(|f| kit::s(f, "host")).filter(|h| !h.is_empty())
                .unwrap_or_else(|| svc.as_ref().map(default_forward_name).unwrap_or_else(|| "app".into())),
            // Seeded from the forward: saving must not quietly make a public one private.
            public: existing.is_some_and(|f| kit::truthy(f.get("public").unwrap_or(&Value::Null))),
            gen: s.gen,
        });
    });
    render();
}

/// saveExpose: replace what is on the port with the named forward.
pub fn save_expose(key: &str) {
    let Some(e) = st(|s| s.edit.clone().filter(|e| e.key == key)) else { return };
    let name = e.name.trim().to_lowercase();
    if name.is_empty() { kit::toast_error("A name is required"); return; }
    if !valid_name(&name) {
        kit::toast_error("Lowercase letters, digits and hyphens only, not starting or ending with one.");
        return;
    }
    let go = {
        let (k, e, name) = (key.to_string(), e.clone(), name.clone());
        move || {
            let existing = st(|s| forward_for(&s.forwards, e.port).map(|f| kit::s(f, "host")));
            let project = k.split(':').next().unwrap_or("").to_string();
            let body = json!({ "name": name, "port": e.port, "mode": e.mode, "public": e.public, "project": project });
            kit::call(
                move |api| {
                    if let Some(h) = &existing { let _ = api.delete(&format!("/api/forwards/{}", kit::enc(h))); }
                    // No tunnel to stop is fine: the forward is what matters.
                    let _ = api.post(&format!("/api/services/{}/tunnel", kit::enc(&k)), &json!({ "enabled": false, "port": e.port }));
                    api.post("/api/forwards", &body)
                },
                move |r| {
                    match r {
                        Ok(d) if kit::truthy(d.get("pendingApproval").unwrap_or(&Value::Null)) => {
                            st(|s| s.edit = None);
                            kit::toast_ok("Sent to the owner to approve");
                        }
                        Ok(d) if d.get("ok").and_then(|x| x.as_bool()) == Some(false) => {
                            let e = kit::s(&d, "error");
                            kit::toast_error(if e.is_empty() { "Could not create the forward" } else { &e });
                            return;
                        }
                        Ok(d) => {
                            st(|s| s.edit = None);
                            let url = d.pointer("/forward/url").and_then(|u| u.as_str()).unwrap_or(&name).to_string();
                            kit::toast_ok(&format!("Reachable at {url}"));
                        }
                        Err(e) => { kit::toast_error(&format!("Failed: {e}")); return; }
                    }
                    load();
                },
            );
        }
    };
    if e.public {
        let msg = format!("Publish {name}?\n\nAnyone who knows the address reaches it without signing in to Crundi. Only do this for something that has its own sign-in.");
        kit::confirm(kit::Confirm { title: "Publish", message: &msg, ok: "Publish", danger: true }, move |ok| if ok { go(); });
    } else {
        go();
    }
}

/// removeForward.
fn remove_forward(host: &str) {
    if host.is_empty() { return; }
    let h = host.to_string();
    kit::call(
        move |api| api.delete(&format!("/api/forwards/{}", kit::enc(&h))),
        |r| {
            match ok_of(r, "Could not remove it") {
                Ok(_) => kit::toast_ok("Removed"),
                Err(e) => kit::toast_error(&format!("Failed: {e}")),
            }
            load();
        },
    );
}

/// setForwardProject: which project a loose forward serves.
fn set_forward_project(host: &str, project: &str) {
    let (h, p) = (host.to_string(), project.to_string());
    kit::call(
        { let (h, p) = (h.clone(), p.clone()); move |api| api.patch(&format!("/api/forwards/{}", kit::enc(&h)), &json!({ "project": p })) },
        move |r| {
            match ok_of(r, "Could not save that") {
                Ok(_) => kit::toast_ok(&if p.is_empty() { format!("{h} is not linked to a project") } else { format!("{h} now serves {p}") }),
                Err(e) => kit::toast_error(&e),
            }
            load();
        },
    );
}

/// A forward's or tunnel's address in the browser (a path one is on this server).
fn open_url(url: &str) {
    if url.is_empty() { return; }
    let full = if url.starts_with('/') { format!("{}{}", kit::base(), url) } else { url.to_string() };
    st(|s| s.last_url = full.clone());
    if std::env::var_os("CRUNDI_TEST_PORT").is_none() {
        std::thread::spawn(move || { let _ = open::that(&full); });
    }
}

// ─── Register ───

fn register_open() {
    let Some(w) = kit::ui() else { return };
    let o = g(&w);
    // As the web's: a second click leaves the open form as it is.
    if o.get_reg_open() { return; }
    o.set_reg_name("".into());
    o.set_reg_command("".into());
    o.set_reg_cwd("".into());
    o.set_reg_stop("".into());
    o.set_reg_port("".into());
    o.set_reg_gen(o.get_reg_gen() + 1);
    o.set_reg_open(true);
}

fn register_cancel() {
    if let Some(w) = kit::ui() { g(&w).set_reg_open(false); }
}

fn register_submit() {
    let Some(w) = kit::ui() else { return };
    let o = g(&w);
    let name = o.get_reg_name().trim().to_string();
    let command = o.get_reg_command().trim().to_string();
    if name.is_empty() { kit::toast_error("Service name is required"); return; }
    if command.is_empty() { kit::toast_error("Command is required"); return; }
    let body = json!({
        "alias": kit::project(), "name": name, "command": command,
        "cwd": o.get_reg_cwd().trim(), "stopCommand": o.get_reg_stop().trim(),
        "tunnelPort": parse_int(&o.get_reg_port()).max(0),
    });
    kit::call(
        move |api| api.post("/api/services", &body),
        move |r| {
            match ok_of(r, "Failed") {
                Ok(_) => kit::toast_ok(&format!("Registered {name}")),
                Err(e) => kit::toast_error(&e),
            }
            // The web's list is drawn again, form and all.
            register_cancel();
            reload_soon(500);
        },
    );
}

// ─── The driver ───

fn arg(args: &[String], i: usize) -> &str {
    args.get(i).map(|s| s.as_str()).unwrap_or("")
}

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let ok = Some(json!({ "ok": true }));
    match name {
        "services-load" => { load(); ok }
        "services-register-open" => { register_open(); ok }
        "services-register-cancel" => { register_cancel(); ok }
        // name, command, cwd, stop command, tunnel port; then Register.
        "services-register" => {
            register_open();
            let w = kit::ui()?;
            let o = g(&w);
            o.set_reg_name(arg(args, 0).into());
            o.set_reg_command(arg(args, 1).into());
            o.set_reg_cwd(arg(args, 2).into());
            o.set_reg_stop(arg(args, 3).into());
            o.set_reg_port(arg(args, 4).into());
            register_submit();
            ok
        }
        // start | stop | restart | logs | port | delete, key.
        "services-act" => { act(arg(args, 1), arg(args, 0)); ok }
        // key, off | tunnel | subdomain | path.
        "services-expose" => {
            let i = ["off", "tunnel", "subdomain", "path"].iter().position(|m| *m == arg(args, 1)).unwrap_or(0);
            expose(arg(args, 0), i as i32);
            ok
        }
        "services-edit-name" => { st(|s| if let Some(e) = s.edit.as_mut() { e.name = arg(args, 0).into(); }); render(); ok }
        "services-edit-public" => { if let Some(k) = st(|s| s.edit.as_ref().map(|e| e.key.clone())) { if let Some(w) = kit::ui() { g(&w).invoke_edit_public(k.into(), arg(args, 0) == "true"); } } ok }
        "services-edit-save" => { if let Some(k) = st(|s| s.edit.as_ref().map(|e| e.key.clone())) { save_expose(&k); } ok }
        "services-edit-cancel" => { st(|s| s.edit = None); render(); ok }
        "services-fwd-remove" => { remove_forward(arg(args, 0)); ok }
        // host, alias ("" none).
        "services-fwd-project" => { set_forward_project(arg(args, 0), arg(args, 1)); ok }
        "services-open-url" => { open_url(arg(args, 0)); ok }
        _ => None,
    }
}

/// This screen's part of the area's props (under "services").
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return json!({}) };
    let o = g(&w);
    let cards: Vec<Value> = CARDS.with(|m| m.iter().map(|c| json!({
        "key": c.key.as_str(), "name": c.name.as_str(), "status": c.status.as_str(), "tone": c.tone.as_str(),
        "running": c.running, "command": c.command.as_str(), "uptime": c.uptime.as_str(),
        "stats": if c.stats { json!({ "cpu": c.cpu.as_str(), "mem": c.mem.as_str(), "badge": c.badge.as_str(), "cpuLine": !c.cpu_line.is_empty() }) } else { Value::Null },
        "tunnelBtn": c.tunnel_btn, "expose": c.expose, "portLabel": c.port_label.as_str(), "mode": c.mode,
        "link": c.link.as_str(), "public": c.public, "stateWord": c.state_word.as_str(), "fwdHost": c.fwd_host.as_str(),
        "editing": c.editing, "editPrefix": c.edit_prefix.as_str(), "editSuffix": c.edit_suffix.as_str(), "editName": c.edit_name.as_str(), "editPublic": c.edit_public,
        "logsOpen": c.logs_open, "logs": c.logs.chars().take(400).collect::<String>(),
    })).collect());
    let projects: Vec<String> = PROJECTS.with(|m| m.iter().map(|s| s.to_string()).collect());
    let others: Vec<Value> = OTHERS.with(|m| m.iter().map(|f| json!({
        "host": f.host.as_str(), "url": f.url.as_str(), "port": f.port.as_str(), "public": f.public,
        "project": projects.get(f.project as usize).filter(|_| f.project > 0).cloned().unwrap_or_default(),
    })).collect());
    st(|s| json!({
        "shown": s.shown, "loaded": o.get_loaded(), "loading": o.get_loading(), "error": o.get_error().as_str(),
        "title": o.get_title().as_str(), "empty": o.get_empty().as_str(),
        "regOpen": o.get_reg_open(), "cards": cards, "others": others, "polling": s.timer.is_some(),
        "lastUrl": s.last_url, "projects": projects,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_and_percent() {
        assert_eq!(fmt_bytes(None), "\u{2014}");
        assert_eq!(fmt_bytes(Some(512.0)), "512 B");
        assert_eq!(fmt_bytes(Some(2048.0)), "2.0 KB");
        assert_eq!(fmt_bytes(Some(150.0 * 1024.0 * 1024.0)), "150 MB");
        assert_eq!(fmt_pct(None), "\u{2014}");
        assert_eq!(fmt_pct(Some(3.14159)), "3.1%");
        assert_eq!(fmt_pct(Some(201.4)), "201%");
    }

    #[test]
    fn levels() {
        assert_eq!(level(10.0), "ok");
        assert_eq!(level(60.0), "warn");
        assert_eq!(level(85.0), "error");
    }

    #[test]
    fn sparkline() {
        assert_eq!(spark(&[5.0], 100.0), (String::new(), String::new()));
        assert_eq!(spark(&[1.0, 2.0], 0.0), (String::new(), String::new()));
        let (l, a) = spark(&[0.0, 100.0], 100.0);
        assert_eq!(l, "M0.00 19.00L100.00 1.00");
        assert_eq!(a, "M0.00 19.00L100.00 1.00L100 20L0 20Z");
    }

    #[test]
    fn forward_names() {
        assert_eq!(default_forward_name(&json!({ "key": "playground:Test App" })), "test-app");
        assert_eq!(default_forward_name(&json!({ "name": "!!!" })), "app");
        assert_eq!(default_forward_name(&json!({ "name": "demo:web_dev" })), "web-dev");
        assert!(valid_name("my-app1"));
        assert!(!valid_name("-app") && !valid_name("app-") && !valid_name("App") && !valid_name(""));
        assert!(valid_name("a"));
    }

    #[test]
    fn ints() {
        assert_eq!(parse_int(" 4561 "), 4561);
        assert_eq!(parse_int("80abc"), 80);
        assert_eq!(parse_int(""), 0);
        assert_eq!(parse_int("x"), 0);
    }

    #[test]
    fn card_states() {
        let fwds = vec![json!({ "host": "web", "port": 4561, "mode": "path", "url": "/tunnel/web/", "public": true })];
        let s = json!({ "key": "demo:web", "name": "web", "status": "running", "command": "sleep 600", "uptime": "2m", "pid": 42, "memory": 2048, "cpuPct": 1.5, "tunnelPort": 4561 });
        let c = card(&s, &fwds, None, None, None, None);
        assert_eq!(c.status.as_str(), "RUNNING");
        assert_eq!(c.uptime.as_str(), "\u{b7} up 2m");
        assert!(c.stats && c.expose && !c.tunnel_btn);
        assert_eq!(c.badge.as_str(), "pid 42");
        assert_eq!(c.mode, 3);
        assert_eq!(c.link.as_str(), "/tunnel/web/");
        assert!(c.public);
        assert_eq!(c.fwd_host.as_str(), "web");
        assert_eq!(c.port_label.as_str(), "Port 4561");

        let s2 = json!({ "key": "demo:w", "status": "stopped", "tunnelPort": 0 });
        let c2 = card(&s2, &[], None, None, None, None);
        assert!(!c2.stats && !c2.expose && c2.tunnel_btn && c2.mode == 0);
        assert_eq!(c2.name.as_str(), "demo:w");

        let s3 = json!({ "key": "demo:t", "status": "stopped", "tunnelPort": 3000, "tunnelEnabled": true, "tunnelStatus": "connecting" });
        let c3 = card(&s3, &[], None, None, None, None);
        assert_eq!((c3.mode, c3.state_word.as_str(), c3.state_tone.as_str()), (1, "starting…", "warn"));

        let e = Edit { key: "demo:t".into(), port: 3000, mode: "path".into(), name: "t".into(), public: false, gen: 1 };
        let c4 = card(&s3, &[], None, None, Some(&e), Some(&"line".to_string()));
        assert!(c4.editing && c4.logs_open);
        assert_eq!((c4.edit_prefix.as_str(), c4.edit_suffix.as_str()), ("/tunnel/", "/"));
    }

    #[test]
    fn containers_badge() {
        let s = json!({ "key": "k", "status": "running", "containers": [] });
        assert_eq!(card(&s, &[], None, None, None, None).badge.as_str(), "no containers");
        let stats = json!({ "k": { "containers": [{ "name": "a" }, { "name": "b" }], "cpuPct": 70.0, "cpuHistory": [1, 2, 3] } });
        let c = card(&s, &[], None, Some(&stats), None, None);
        assert_eq!(c.badge.as_str(), "2 containers");
        assert_eq!(c.cpu_tone.as_str(), "warn");
        assert!(!c.cpu_line.is_empty());
    }

    #[test]
    fn loose_forwards() {
        let svcs = vec![json!({ "tunnelPort": 3000 })];
        let fwds = vec![json!({ "host": "a", "port": 3000 }), json!({ "host": "b", "port": 4000, "project": "other" }), json!({ "host": "c", "port": 5000 })];
        let hosts = |p: &str| other_forwards(&svcs, &fwds, p).iter().map(|f| kit::s(f, "host")).collect::<Vec<_>>();
        assert_eq!(hosts(""), vec!["b", "c"]);
        assert_eq!(hosts("demo"), vec!["c"]);
    }
}
