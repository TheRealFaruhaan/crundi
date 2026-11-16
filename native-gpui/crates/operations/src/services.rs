//! Services and their forwards (the Services tab; the web's loadServices /
//! renderServices, src/webapp-html.js ~12860-14125; Slint:
//! native-client/src/ops/services.rs, ui/ops/services.slint,
//! specs/ops/services.md): the list, register, start/stop/restart, logs,
//! delete, exposure (port, tunnel, subdomain and path forwards), the forwards
//! no card shows, and the live usage strip (/api/stats every 2s while the tab
//! is on show).
//!
//! The native client is the owner's (invitees use the web, DESIGN.md rule 4),
//! so every card can be edited and "Other forwards" is always offered.

pub(crate) mod kit;

use crundi_shell::AreaEvent;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use kit::{n, s, t};
use serde_json::{json, Value};
use std::{collections::HashMap, time::Duration};

/// The stats poll (ensureStatsPolling: 2s).
const POLL: Duration = Duration::from_millis(2000);
const MODES: [&str; 4] = ["off", "tunnel", "subdomain", "path"];

/// The naming form open in one card (the web's exposeEdit); the name is in
/// the form's text box.
#[derive(Clone, Debug, PartialEq)]
struct Edit { key: String, port: i64, mode: String, public: bool }

#[derive(Default)]
pub struct Services {
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
    log_scroll: HashMap<String, ScrollHandle>,
    poll: Option<Task<()>>,
    /// What the last "state" said about the services (a change reloads).
    sig: String,
    last_url: String,
    reg_open: bool,
    /// Name, command, working directory, stop command, tunnel port.
    reg: [Option<Entity<w::InputState>>; 5],
    edit_name: Option<Entity<w::InputState>>,
    subs: Vec<Subscription>,
}

struct Handle(Entity<Services>);
impl Global for Handle {}

fn ent(cx: &mut App) -> Entity<Services> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let e = cx.new(|_| Services::default());
    cx.set_global(Handle(e.clone()));
    e
}

fn upd<R>(cx: &mut App, f: impl FnOnce(&mut Services, &mut Context<Services>) -> R) -> R {
    let e = ent(cx);
    e.update(cx, |s, cx| { let r = f(s, cx); cx.notify(); r })
}

fn read<R>(cx: &mut App, f: impl FnOnce(&Services, &App) -> R) -> R {
    let e = ent(cx);
    f(e.read(cx), cx)
}

// ─── The registry ───

pub fn view(tab: &str, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    if tab != "services" { return None; }
    let e = ent(cx);
    e.update(cx, |s, cx| s.ensure_inputs(window, cx));
    Some(AnyView::from(e).into_any_element())
}

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    match e {
        AreaEvent::State(v) => {
            // The web refreshes the cards on its own actions only; the state
            // stream here also brings a service that started, stopped or was
            // registered elsewhere (by Claude, say) onto the open tab.
            let Some(list) = v.get("services").and_then(|x| x.as_array()) else { return };
            let sig = signature(list);
            let (changed, live) = upd(cx, |s, _| { let c = s.sig != sig; s.sig = sig; (c, s.shown && s.loaded) });
            if changed && live { load(cx); }
        }
        AreaEvent::Connected(true) => if read(cx, |s, _| s.shown) { load(cx); },
        _ => {}
    }
}

pub fn on_project(_alias: &str, cx: &mut App) {
    // The list is filtered by the project (and the title says "all projects"
    // without one); the edit form belongs to a card that may be gone.
    let shown = upd(cx, |s, _| { s.edit = None; s.shown });
    if shown { load(cx); }
}

pub fn on_signed_in(cx: &mut App) {
    if read(cx, |s, _| s.shown) { load(cx); }
}

pub fn on_signed_out(cx: &mut App) {
    upd(cx, |s, _| {
        s.loaded = false; s.loading = false; s.error.clear(); s.services.clear(); s.forwards.clear();
        s.opts = None; s.stats = None; s.edit = None; s.logs.clear(); s.sig.clear(); s.reg_open = false;
    });
}

pub fn on_tab(tab: &str, cx: &mut App) {
    let on = tab == "services";
    let was = upd(cx, |s, _| std::mem::replace(&mut s.shown, on));
    if on && !was { load(cx); }
    polling(on, cx);
}

// ─── Loading ───

/// loadServices: the services, then the forwards (and their options, once).
fn load(cx: &mut App) {
    if crundi_shell::api(cx).is_none() { return; }
    let need_opts = upd(cx, |s, _| { s.loading = true; s.opts.is_none() });
    crundi_shell::call(cx, move |api| {
        let svc = api.get("/api/services")?;
        // The forwards are best effort: the card falls back to tunnel-only.
        let fwd = api.get("/api/forwards").unwrap_or(Value::Null);
        let opts = if need_opts { api.get("/api/forwards/options").unwrap_or(Value::Null) } else { Value::Null };
        let stats = api.get("/api/stats").unwrap_or(Value::Null);
        Ok(json!({ "svc": svc, "fwd": fwd, "opts": opts, "stats": stats }))
    }, |r, cx| {
        let mut failed = None;
        upd(cx, |s, _| {
            s.loading = false;
            match r {
                Ok(mut v) => {
                    s.services = v["svc"].get("services").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    s.loaded = true;
                    s.error.clear();
                    let fwd = v["fwd"].take();
                    if fwd.is_object() { s.forwards = fwd.get("forwards").and_then(|x| x.as_array()).cloned().unwrap_or_default(); }
                    let o = v["opts"].take();
                    if o.is_object() && t(&o, "ok") { s.opts = Some(o); }
                    let st = v["stats"].take();
                    if st.is_object() && st.get("error").is_none() { s.stats = st.get("services").cloned(); }
                }
                Err(e) => {
                    if s.loaded { failed = Some(format!("Failed to load services: {e}")); }
                    s.error = e;
                }
            }
            // Logs of a service that is gone close.
            let keys: Vec<String> = s.services.iter().map(|x| s_(x, "key")).collect();
            s.logs.retain(|k, _| keys.contains(k));
            if s.edit.as_ref().is_some_and(|e| !keys.contains(&e.key)) { s.edit = None; }
        });
        if let Some(e) = failed { kit::err(cx, &e); }
    });
}

fn s_(v: &Value, k: &str) -> String { s(v, k) }

fn reload_soon(ms: u64, cx: &mut App) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_millis(ms)).await;
        let _ = cx.update(|cx| load(cx));
    }).detach();
}

/// The 2s stats poll, only while the tab is on show.
fn polling(on: bool, cx: &mut App) {
    if !on { upd(cx, |s, _| s.poll = None); return; }
    if read(cx, |s, _| s.poll.is_some()) { return; }
    let task = cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(POLL).await;
        let _ = cx.update(|cx| poll_stats(cx));
    });
    upd(cx, |s, _| s.poll = Some(task));
}

fn poll_stats(cx: &mut App) {
    if crundi_shell::api(cx).is_none() { return; }
    crundi_shell::call(cx, |api| api.get("/api/stats"), |r, cx| {
        // A refused or failed poll says nothing about the services: keep the strips.
        let Ok(d) = r else { return };
        if !d.is_object() || d.get("error").is_some() { return; }
        upd(cx, |s, _| s.stats = d.get("services").cloned());
    });
}

/// The services' state in a line, for noticing a change in "state".
fn signature(list: &[Value]) -> String {
    list.iter()
        .map(|x| format!("{}|{}|{}|{}|{}", s(x, "key"), s(x, "status"), s(x, "tunnelPort"), s(x, "tunnelEnabled"),
            x.pointer("/tunnel/status").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| s(x, "tunnelStatus"))))
        .collect::<Vec<_>>()
        .join(";")
}

// ─── Pure rules ───

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
    match n.filter(|n| n.is_finite()) {
        None => "\u{2014}".into(),
        Some(n) if n >= 100.0 => format!("{}%", n.round() as i64),
        Some(n) => format!("{n:.1}%"),
    }
}

/// lvl: the tone for a load (green until it matters).
pub(crate) fn level(pct: f64) -> &'static str {
    if pct >= 85.0 { "error" } else if pct >= 60.0 { "warn" } else { "ok" }
}

fn nums(v: Option<&Value>) -> Vec<f64> {
    v.and_then(|x| x.as_array()).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default()
}

fn max_of(vals: &[f64], floor: f64) -> f64 { vals.iter().fold(floor, |m, v| m.max(*v)) }

fn opt_num(v: &Value, k: &str) -> Option<f64> { v.get(k).and_then(|x| x.as_f64()) }

/// The defaultForwardName: the key's last part, as a hostname.
pub(crate) fn default_forward_name(svc: &Value) -> String {
    let raw = { let n = s(svc, "name"); if n.is_empty() { s(svc, "key") } else { n } };
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

/// parseInt(v, 10) || 0: the leading digits.
pub(crate) fn parse_int(v: &str) -> i64 {
    let t = v.trim();
    let (neg, rest) = match t.strip_prefix('-') { Some(r) => (true, r), None => (false, t.strip_prefix('+').unwrap_or(t)) };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: i64 = digits.parse().unwrap_or(0);
    if neg { -n } else { n }
}

fn mode_index(m: &str) -> usize { MODES.iter().position(|x| *x == m).unwrap_or(0) }

fn port_of(v: &Value, k: &str) -> i64 { n(v, k) as i64 }

/// The forward on a port.
fn forward_for(forwards: &[Value], port: i64) -> Option<&Value> {
    if port <= 0 { return None; }
    forwards.iter().find(|f| port_of(f, "port") == port)
}

fn sub_domain(opts: Option<&Value>) -> (bool, String) {
    let Some(o) = opts else { return (false, String::new()) };
    let sub = o.get("choices").and_then(|c| c.as_array()).and_then(|c| c.iter().find(|x| s(x, "id") == "subdomain"));
    let avail = sub.is_some_and(|x| t(x, "available"));
    let dom = sub.map(|x| s(x, "suggestedDomain")).filter(|d| !d.is_empty()).unwrap_or_else(|| s(o, "domain"));
    (avail, dom)
}

/// One card, everything worded (the Slint OpsSvcCard).
#[derive(Clone, Debug, Default, PartialEq)]
struct Card {
    key: String, name: String, status: String, tone: String, running: bool, command: String, uptime: String,
    stats: bool, cpu: String, cpu_tone: String, cpu_hist: Vec<f64>, cpu_max: f64, mem: String, mem_hist: Vec<f64>, mem_max: f64, badge: String,
    tunnel_btn: bool, expose: bool, port_label: String, mode: usize,
    link: String, public: bool, state_word: String, state_tone: String, fwd_host: String,
    editing: bool, edit_prefix: String, edit_suffix: String, edit_public: bool,
    logs_open: bool, logs: String,
}

fn card(x: &Value, forwards: &[Value], opts: Option<&Value>, stats: Option<&Value>, edit: Option<&Edit>, logs: Option<&String>) -> Card {
    let key = s(x, "key");
    let status = s(x, "status");
    let running = status == "running";
    let tone = if running { "running" } else if status == "error" { "error" } else { "" };
    let t_port = port_of(x, "tunnelPort");
    let t_enabled = t(x, "tunnelEnabled");
    let t_status = s(x, "tunnelStatus");
    let fwd = forward_for(forwards, t_port);
    let mode = fwd.map(|f| s(f, "mode")).unwrap_or_else(|| if t_enabled { "tunnel".into() } else { String::new() });
    let mut c = Card {
        key: key.clone(),
        name: { let n = s(x, "name"); if n.is_empty() { key.clone() } else { n } },
        status: status.to_uppercase(),
        tone: tone.into(),
        running,
        command: s(x, "command"),
        uptime: { let u = s(x, "uptime"); if u.is_empty() { String::new() } else { format!("\u{b7} up {u}") } },
        tunnel_btn: !(t_port > 0 || t_enabled),
        expose: t_port > 0 || t_enabled || fwd.is_some(),
        port_label: if t_port > 0 { format!("Port {t_port}") } else { "Set port".into() },
        mode: mode_index(&mode),
        ..Default::default()
    };
    // The usage strip: the listing's numbers, the poll's where it has the service.
    if running {
        let mut st = json!({ "cpuPct": x.get("cpuPct"), "memory": x.get("memory"), "pid": x.get("pid"), "containers": x.get("containers") });
        if let Some(p) = stats.and_then(|m| m.get(&key)).and_then(|p| p.as_object()) {
            for (k, v) in p { st[k] = v.clone(); }
        }
        let cpu = opt_num(&st, "cpuPct");
        c.stats = true;
        c.cpu_hist = nums(st.get("cpuHistory"));
        c.cpu_max = max_of(&c.cpu_hist, 100.0);
        c.mem_hist = nums(st.get("memHistory"));
        c.mem_max = max_of(&c.mem_hist, 0.0) * 1.25;
        c.cpu = fmt_pct(cpu);
        c.cpu_tone = level(cpu.unwrap_or(0.0)).into();
        c.mem = fmt_bytes(opt_num(&st, "memory"));
        c.badge = match st.get("containers").and_then(|x| x.as_array()) {
            Some(a) if a.is_empty() => "no containers".into(),
            Some(a) => format!("{} {}", a.len(), if a.len() == 1 { "container" } else { "containers" }),
            None if t(&st, "pid") => format!("pid {}", s(&st, "pid")),
            None => String::new(),
        };
    }
    // The exposure row's address or state word.
    if let Some(f) = fwd {
        c.link = s(f, "url");
        c.public = t(f, "public");
        c.fwd_host = s(f, "host");
    } else if t_enabled && t_status == "active" && !s(x, "tunnelUrl").is_empty() {
        c.link = s(x, "tunnelUrl");
    } else {
        let (w, tn) = if t_enabled && t_status == "connecting" { ("starting…", "warn") }
            else if t_enabled && t_status == "error" { ("error", "error") }
            else if t_enabled { ("idle", "") } else { ("off", "") };
        c.state_word = w.into();
        c.state_tone = tn.into();
    }
    if let Some(e) = edit.filter(|e| e.key == key && c.expose) {
        let (_, dom) = sub_domain(opts);
        c.editing = true;
        c.edit_prefix = if e.mode == "path" { "/tunnel/" } else { "" }.into();
        c.edit_suffix = if e.mode == "subdomain" { format!(".{dom}") } else { "/".into() };
        c.edit_public = e.public;
    }
    if let Some(l) = logs { c.logs_open = true; c.logs = l.clone(); }
    c
}

/// otherForwardsHtml: forwards on ports no service records, in this project (or in none).
fn other_forwards(services: &[Value], forwards: &[Value], project: &str) -> Vec<Value> {
    let shown: Vec<i64> = services.iter().map(|x| port_of(x, "tunnelPort")).filter(|p| *p > 0).collect();
    forwards.iter()
        .filter(|f| !shown.contains(&port_of(f, "port")))
        .filter(|f| { let p = s(f, "project"); project.is_empty() || p.is_empty() || p == project })
        .cloned()
        .collect()
}

fn aliases(cx: &App) -> Vec<String> {
    crundi_shell::session::get(cx).projects.iter().map(|p| s(p, "alias")).filter(|a| !a.is_empty()).collect()
}

impl Services {
    fn ensure_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit_name.is_some() { return; }
        let ph = ["Service name", "Command (e.g. npm run dev)", "Working directory (optional)", "Stop command (optional, e.g. docker compose down)", "Tunnel port (optional) \u{2014} expose via Cloudflare"];
        for (i, p) in ph.iter().enumerate() { self.reg[i] = Some(kit::input(window, cx, p)); }
        let e = kit::input(window, cx, "name");
        self.subs.push(cx.subscribe_in(&e, window, |this, _, ev: &w::InputEvent, window, cx| {
            if let w::InputEvent::PressEnter { .. } = ev {
                if let Some(k) = this.edit.as_ref().map(|e| e.key.clone()) { window.defer(cx, move |window, cx| save_expose(&k, window, cx)); }
            }
        }));
        self.edit_name = Some(e);
    }

    fn project_services(&self, project: &str) -> Vec<&Value> {
        self.services.iter().filter(|x| project.is_empty() || s(x, "alias") == project).collect()
    }

    fn cards(&self, project: &str) -> Vec<Card> {
        self.project_services(project).into_iter().map(|x| {
            let key = s(x, "key");
            card(x, &self.forwards, self.opts.as_ref(), self.stats.as_ref(), self.edit.as_ref(), self.logs.get(&key))
        }).collect()
    }
}

// ─── Actions ───

fn service(key: &str, cx: &mut App) -> Option<Value> {
    read(cx, |st, _| st.services.iter().find(|x| s(x, "key") == key).cloned())
}

fn act(key: &str, action: &str, window: &mut Window, cx: &mut App) {
    match action {
        "start" | "stop" | "restart" => {
            let (k, a) = (key.to_string(), action.to_string());
            let path = format!("/api/services/{}/{}", kit::enc(&k), a);
            crundi_shell::call(cx, move |api| api.post(&path, &json!({})), move |r, cx| {
                match kit::refused(r, "Failed") {
                    Ok(_) => kit::ok(cx, &format!("{a} {k}")),
                    Err(e) => kit::err(cx, &e),
                }
                reload_soon(500, cx);
            });
        }
        "logs" => logs(key, cx),
        "port" => set_port(key, window, cx),
        "delete" => delete(key, window, cx),
        _ => {}
    }
}

/// svcLogs: the inline logs, a toggle.
fn logs(key: &str, cx: &mut App) {
    if upd(cx, |st, _| st.logs.remove(key).is_some()) { return; }
    let k = key.to_string();
    let path = format!("/api/services/{}/logs", kit::enc(&k));
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
        let text = match r {
            Ok(v) => {
                let t = kit::strs(&v, "logs").join("\n");
                if t.is_empty() { "(no logs)".into() } else { t }
            }
            Err(e) => format!("Error: {e}"),
        };
        upd(cx, |st, _| {
            st.logs.insert(k.clone(), text);
            let h = st.log_scroll.entry(k).or_default().clone();
            h.scroll_to_bottom();
        });
    });
}

/// svcTunnelSetPort: the port to expose (0 or blank clears it).
fn set_port(key: &str, window: &mut Window, cx: &mut App) {
    let cur = service(key, cx).map(|x| port_of(&x, "tunnelPort")).unwrap_or(0);
    let k = key.to_string();
    w::ask(window, cx, w::Ask {
        title: "Tunnel port".into(),
        message: "Local port to expose via Cloudflare. Enter 0 (or blank) to clear.".into(),
        ok: "OK".into(),
        prompt: Some(if cur > 0 { cur.to_string() } else { String::new() }),
        ..Default::default()
    }, move |v, _, cx| {
        let Some(v) = v else { return };
        let port = parse_int(&v);
        let msg = if port > 0 { format!("Tunnel port set to :{port}") } else { "Tunnel port cleared".into() };
        tunnel_post(&k, json!({ "port": port }), msg, cx);
    });
}

fn tunnel_post(key: &str, body: Value, ok_msg: String, cx: &mut App) {
    let path = format!("/api/services/{}/tunnel", kit::enc(key));
    crundi_shell::call(cx, move |api| api.post(&path, &body), move |r, cx| {
        match kit::refused(r, "Failed") {
            Ok(_) => kit::ok(cx, &ok_msg),
            Err(e) => kit::err(cx, &e),
        }
        reload_soon(600, cx);
    });
}

/// svcDelete: asks first.
fn delete(key: &str, window: &mut Window, cx: &mut App) {
    let k = key.to_string();
    w::ask_confirm(window, cx, w::Ask { title: "Delete service".into(), message: format!("Delete service \"{key}\"?").into(), ok: "Delete".into(), danger: true, ..Default::default() }, move |ok, _, cx| {
        if !ok { return; }
        let k = k.clone();
        let path = format!("/api/services/{}/delete", kit::enc(&k));
        crundi_shell::call(cx, move |api| api.post(&path, &json!({})), move |r, cx| {
            match kit::refused(r, "Failed") {
                Ok(_) => kit::ok(cx, &format!("Deleted {k}")),
                Err(e) => kit::err(cx, &e),
            }
            reload_soon(500, cx);
        });
    });
}

/// The switch: Off and Tunnel act at once; Subdomain and Path open the naming form.
fn expose(key: &str, i: usize, window: &mut Window, cx: &mut App) {
    let mode = MODES.get(i).copied().unwrap_or("off");
    let port = service(key, cx).map(|x| port_of(&x, "tunnelPort")).unwrap_or(0);
    if mode == "subdomain" || mode == "path" {
        begin_expose(key, port, mode, window, cx);
    } else {
        upd(cx, |st, _| st.edit = None);
        expose_now(key, port, mode, cx);
    }
}

/// exposeService: off, or a quick tunnel; whatever is there now goes first.
fn expose_now(key: &str, port: i64, mode: &str, cx: &mut App) {
    if port <= 0 { kit::err(cx, "Set a port first"); return; }
    let existing = read(cx, |st, _| forward_for(&st.forwards, port).map(|f| s(f, "host")));
    let (k, m) = (key.to_string(), mode.to_string());
    crundi_shell::call(cx, move |api| {
        if let Some(h) = &existing { let _ = api.delete(&format!("/api/forwards/{}", kit::enc(h))); }
        let path = format!("/api/services/{}/tunnel", kit::enc(&k));
        let off = api.post(&path, &json!({ "enabled": false, "port": port }))?;
        if off.get("ok").and_then(|x| x.as_bool()) == Some(false) {
            let e = s(&off, "error");
            return Ok(json!({ "ok": false, "error": if e.is_empty() { "could not turn the tunnel off".to_string() } else { e } }));
        }
        if m == "off" { return Ok(json!({ "ok": true, "off": true })); }
        api.post(&path, &json!({ "enabled": true, "port": port }))
    }, |r, cx| {
        match r {
            Ok(d) if t(&d, "off") => kit::ok(cx, "No longer exposed"),
            Ok(d) if t(&d, "pendingApproval") => kit::ok(cx, "Sent to the owner to approve"),
            Ok(d) if d.get("ok").and_then(|x| x.as_bool()) == Some(false) => kit::err(cx, &{ let e = s(&d, "error"); if e.is_empty() { "Could not start the tunnel".into() } else { e } }),
            Ok(_) => kit::ok(cx, "Starting a tunnel"),
            Err(e) => kit::err(cx, &format!("Failed: {e}")),
        }
        load(cx);
    });
}

/// beginExpose: the naming form in the card; nothing is made until Save.
fn begin_expose(key: &str, port: i64, mode: &str, window: &mut Window, cx: &mut App) {
    if port <= 0 { kit::err(cx, "Set a port first"); return; }
    let (avail, _) = read(cx, |st, _| sub_domain(st.opts.as_ref()));
    if mode == "subdomain" && !avail {
        kit::err(cx, "No domain is configured. Set TLS_DOMAIN or FORWARD_DOMAIN, or use Path.");
        return;
    }
    let svc = service(key, cx);
    let (name, input) = upd(cx, |st, _| {
        let existing = forward_for(&st.forwards, port);
        let name = existing.map(|f| s(f, "host")).filter(|h| !h.is_empty())
            .unwrap_or_else(|| svc.as_ref().map(default_forward_name).unwrap_or_else(|| "app".into()));
        // Seeded from the forward: saving must not quietly make a public one private.
        let public = existing.is_some_and(|f| t(f, "public"));
        st.edit = Some(Edit { key: key.into(), port, mode: mode.into(), public });
        (name, st.edit_name.clone())
    });
    kit::set_value(&input, &name, window, cx);
    if let Some(i) = input { i.update(cx, |s, cx| { s.focus(window, cx); s.select_all(window, cx); }); }
}

/// saveExpose: replace what is on the port with the named forward.
fn save_expose(key: &str, window: &mut Window, cx: &mut App) {
    let Some((e, raw)) = read(cx, |st, cx| st.edit.clone().filter(|e| e.key == key).map(|e| (e, kit::value(&st.edit_name, cx)))) else { return };
    let name = raw.trim().to_lowercase();
    if name.is_empty() { kit::err(cx, "A name is required"); return; }
    if !valid_name(&name) {
        kit::err(cx, "Lowercase letters, digits and hyphens only, not starting or ending with one.");
        return;
    }
    let k = key.to_string();
    let go = move |cx: &mut App| {
        let existing = read(cx, |st, _| forward_for(&st.forwards, e.port).map(|f| s(f, "host")));
        let project = k.split(':').next().unwrap_or("").to_string();
        let body = json!({ "name": name, "port": e.port, "mode": e.mode, "public": e.public, "project": project });
        let (k, port, name) = (k.clone(), e.port, name.clone());
        crundi_shell::call(cx, move |api| {
            if let Some(h) = &existing { let _ = api.delete(&format!("/api/forwards/{}", kit::enc(h))); }
            // No tunnel to stop is fine: the forward is what matters.
            let _ = api.post(&format!("/api/services/{}/tunnel", kit::enc(&k)), &json!({ "enabled": false, "port": port }));
            api.post("/api/forwards", &body)
        }, move |r, cx| {
            match r {
                Ok(d) if t(&d, "pendingApproval") => { upd(cx, |st, _| st.edit = None); kit::ok(cx, "Sent to the owner to approve"); }
                Ok(d) if d.get("ok").and_then(|x| x.as_bool()) == Some(false) => {
                    let e = s(&d, "error");
                    kit::err(cx, if e.is_empty() { "Could not create the forward" } else { &e });
                    return;
                }
                Ok(d) => {
                    upd(cx, |st, _| st.edit = None);
                    let url = d.pointer("/forward/url").and_then(|u| u.as_str()).unwrap_or(&name).to_string();
                    kit::ok(cx, &format!("Reachable at {url}"));
                }
                Err(e) => { kit::err(cx, &format!("Failed: {e}")); return; }
            }
            load(cx);
        });
    };
    if e.public {
        let msg = format!("Publish {}?\n\nAnyone who knows the address reaches it without signing in to Crundi. Only do this for something that has its own sign-in.", raw.trim().to_lowercase());
        let go = std::rc::Rc::new(std::cell::RefCell::new(Some(go)));
        w::ask_confirm(window, cx, w::Ask { title: "Publish".into(), message: msg.into(), ok: "Publish".into(), danger: true, ..Default::default() }, move |ok, _, cx| {
            if ok { if let Some(g) = go.borrow_mut().take() { g(cx); } }
        });
    } else {
        go(cx);
    }
}

/// removeForward.
fn remove_forward(host: &str, cx: &mut App) {
    if host.is_empty() { return; }
    let path = format!("/api/forwards/{}", kit::enc(host));
    crundi_shell::call(cx, move |api| api.delete(&path), |r, cx| {
        match kit::refused(r, "Could not remove it") {
            Ok(_) => kit::ok(cx, "Removed"),
            Err(e) => kit::err(cx, &format!("Failed: {e}")),
        }
        load(cx);
    });
}

/// setForwardProject: which project a loose forward serves.
fn set_forward_project(host: &str, project: &str, cx: &mut App) {
    let (h, p) = (host.to_string(), project.to_string());
    let path = format!("/api/forwards/{}", kit::enc(&h));
    let body = json!({ "project": p });
    crundi_shell::call(cx, move |api| api.patch(&path, &body), move |r, cx| {
        match kit::refused(r, "Could not save that") {
            Ok(_) => kit::ok(cx, &if p.is_empty() { format!("{h} is not linked to a project") } else { format!("{h} now serves {p}") }),
            Err(e) => kit::err(cx, &e),
        }
        load(cx);
    });
}

fn open_url(url: &str, cx: &mut App) {
    let full = kit::open_url(cx, url);
    if !full.is_empty() { upd(cx, |st, _| st.last_url = full); }
}

// ─── Register ───

fn register_open(window: &mut Window, cx: &mut App) {
    // As the web's: a second click leaves the open form as it is.
    let (open, inputs) = read(cx, |st, _| (st.reg_open, st.reg.clone()));
    if open { return; }
    for i in &inputs { kit::set_value(i, "", window, cx); }
    upd(cx, |st, _| st.reg_open = true);
    if let Some(i) = &inputs[0] { i.update(cx, |s, cx| s.focus(window, cx)); }
}

fn register_cancel(cx: &mut App) { upd(cx, |st, _| st.reg_open = false); }

fn register_submit(cx: &mut App) {
    let v: Vec<String> = read(cx, |st, cx| st.reg.iter().map(|i| kit::value(i, cx).trim().to_string()).collect());
    let (name, command) = (v[0].clone(), v[1].clone());
    if name.is_empty() { kit::err(cx, "Service name is required"); return; }
    if command.is_empty() { kit::err(cx, "Command is required"); return; }
    let body = json!({
        "alias": crundi_shell::project(cx), "name": name, "command": command,
        "cwd": v[2], "stopCommand": v[3], "tunnelPort": parse_int(&v[4]).max(0),
    });
    crundi_shell::call(cx, move |api| api.post("/api/services", &body), move |r, cx| {
        match kit::refused(r, "Failed") {
            Ok(_) => kit::ok(cx, &format!("Registered {name}")),
            Err(e) => kit::err(cx, &e),
        }
        // The web's list is drawn again, form and all.
        register_cancel(cx);
        reload_soon(500, cx);
    });
}

// ─── Drawing ───

fn tone_fg(t: &str) -> Hsla { match t { "ok" | "running" => T::green(), "warn" => T::yellow(), "error" => T::red(), _ => T::text_muted() } }
fn tone_bg(t: &str) -> Hsla { match t { "ok" | "running" => T::green_dim(), "warn" => T::yellow_dim(), "error" => T::red_dim(), _ => T::bg_tertiary() } }

fn mono_muted(text: impl Into<SharedString>) -> Div {
    div().flex_none().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(text.into())
}

/// An address that opens in the browser (.tunnel-link).
fn link(id: impl Into<ElementId>, url: String) -> Stateful<Div> {
    let u = url.clone();
    div().id(id).min_w(T::SP_48).truncate().text_size(T::TEXT_12).text_color(T::accent()).cursor_pointer()
        .hover(|d| d.text_color(T::accent_hover()))
        .child(url)
        .on_click(move |_, _, cx| open_url(&u, cx))
}

/// One readout of the usage strip (.stat-cell): the label, the value, the sparkline.
fn stat_cell(label: &'static str, value: String, vals: &[f64], max: f64, stroke: Hsla, fill: Hsla) -> Div {
    div().flex().flex_1().items_center().gap(T::SP_8)
        .child(div().text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(T::text_muted()).child(label))
        .child(div().min_w(T::SERVICES_VALUE_W).font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text()).child(value))
        .child(div().flex_1().min_w(T::SP_32 + T::SP_4).h(T::SERVICES_SPARK_H)
            .child(w::spark(vec![w::Series { values: vals.iter().map(|v| *v as f32).collect(), max: max as f32, stroke, fill }], 0, None).size_full()))
}

fn card_view(i: usize, c: &Card, edit_input: Option<&Entity<w::InputState>>, scroll: Option<ScrollHandle>) -> AnyElement {
    let key = c.key.clone();
    let k = |s: &str| (SharedString::from(format!("svc-{s}-{}", c.key)), i);
    let mut body = w::card().id(("svc-card", i)).p(T::SP_12).gap(T::SP_8)
        // The head.
        .child(div().flex().items_center().gap(T::SP_8)
            .child(w::dot(match c.tone.as_str() { "running" => T::green(), "error" => T::red(), _ => T::text_muted() }))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(c.name.clone()))
            .child(w::tag(c.status.clone(), &c.tone)))
        // The command and uptime.
        .child(div().flex().gap(T::SP_4).font_family(T::MONO).text_size(T::TEXT_12)
            .child(div().min_w_0().truncate().text_color(T::text_muted()).child(c.command.clone()))
            .when(!c.uptime.is_empty(), |d| d.child(div().flex_none().text_color(T::text_secondary()).child(c.uptime.clone()))));
    if c.stats {
        body = body.child(div().flex().items_center().gap(T::SP_16).px(T::SP_10).py(T::SP_8).rounded(T::R_CONTROL)
            .bg(T::bg()).border_1().border_color(T::border_subtle())
            .child(stat_cell("CPU", c.cpu.clone(), &c.cpu_hist, c.cpu_max, tone_fg(&c.cpu_tone), tone_bg(&c.cpu_tone)))
            .child(stat_cell("MEMORY", c.mem.clone(), &c.mem_hist, c.mem_max, T::sky(), T::sky_dim()))
            .when(!c.badge.is_empty(), |d| d.child(mono_muted(c.badge.clone()))));
    }
    // The actions.
    let a = |verb: &'static str| { let key = key.clone(); move |_: &ClickEvent, window: &mut Window, cx: &mut App| act(&key, verb, window, cx) };
    let mut row = div().flex().items_center().gap(T::SP_6);
    if c.running {
        row = row.child(w::button(k("stop"), "Stop").icon(IconName::Square).on_click(a("stop")))
            .child(w::button(k("restart"), "Restart").icon(IconName::RefreshCw).on_click(a("restart")));
    } else {
        row = row.child(w::primary(k("start"), "Start").icon(IconName::Play).on_click(a("start")));
    }
    row = row.child(w::button(k("logs"), "Logs").icon(IconName::FileText).on_click(a("logs")));
    if c.tunnel_btn { row = row.child(w::button(k("tunnel"), "Tunnel").icon(IconName::Globe).on_click(a("port"))); }
    row = row.child(w::danger(k("delete"), "Delete").icon(IconName::Trash).on_click(a("delete")));
    body = body.child(row);
    // The exposure row and the naming form.
    if c.expose {
        let kk = key.clone();
        let mut ex = div().flex().items_center().gap(T::SP_6)
            .child(div().flex_none().text_color(T::text_muted()).child(w::icon(IconName::Globe, T::ICON_MD)))
            .child(w::button(k("port"), c.port_label.clone()).on_click(a("port")))
            .child(w::segmented("svc-expose", &["Off", "Tunnel", "Subdomain", "Path"], c.mode, move |i, window, cx| expose(&kk, *i, window, cx)));
        if !c.link.is_empty() {
            ex = ex.child(link(k("link"), c.link.clone()).flex_shrink_1());
            if c.public { ex = ex.child(w::tag("PUBLIC", "warn")); }
        } else {
            ex = ex.child(mono_muted(c.state_word.clone()).text_color(if c.state_tone.is_empty() { T::text_muted() } else { tone_fg(&c.state_tone) }));
        }
        if !c.fwd_host.is_empty() {
            let h = c.fwd_host.clone();
            ex = ex.child(w::icon_button(k("fwd-rm"), IconName::Trash, "Remove this forward").on_click(move |_, _, cx| remove_forward(&h, cx)));
        }
        body = body.child(w::rule()).child(ex);
        if let (true, Some(input)) = (c.editing, edit_input) {
            let (sk, pk) = (key.clone(), key.clone());
            body = body.child(div().flex().flex_col().gap(T::SP_8)
                .on_key_down(|e: &KeyDownEvent, _, cx| if e.keystroke.key == "escape" { upd(cx, |st, _| st.edit = None); })
                .child(div().flex_none().h(px(1.)).w_full().bg(T::border()))
                .child(div().flex().items_center().gap(T::SP_8)
                    .when(!c.edit_prefix.is_empty(), |d| d.child(mono_muted(c.edit_prefix.clone())))
                    .child(div().flex_1().min_w(T::SP_48 + T::SP_40).child(w::input(input)))
                    .child(mono_muted(c.edit_suffix.clone()))
                    .child(w::primary(k("save"), "Save").icon(IconName::Check).on_click(move |_, window, cx| save_expose(&sk, window, cx)))
                    .child(w::button(k("cancel"), "Cancel").on_click(|_, _, cx| { upd(cx, |st, _| st.edit = None); })))
                .child(w::check(k("public"), "Public \u{2014} reachable without signing in to Crundi", c.edit_public, move |on, _, cx| {
                    let on = *on;
                    upd(cx, |st, _| if let Some(e) = st.edit.as_mut().filter(|e| e.key == pk) { e.public = on; });
                })));
        }
    }
    if c.logs_open {
        let sh = scroll.unwrap_or_default();
        body = body.child(div().id(k("logs")).max_h(T::SERVICES_LOGS_H).overflow_y_scroll().track_scroll(&sh)
            .p(T::SP_10).rounded(T::R_CONTROL).bg(T::bg()).border_1().border_color(T::border())
            .font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted())
            .child(c.logs.clone()));
    }
    body.into_any_element()
}

impl Render for Services {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_inputs(window, cx);
        let project = crundi_shell::project(cx);
        let cards = self.cards(&project);
        let mut page = w::page("services-page")
            .child(w::title_bar(if project.is_empty() { "Services \u{b7} all projects" } else { "Services" })
                .child(w::primary("svc-register", "Register service").icon(IconName::Plus).on_click(|_, window, cx| register_open(window, cx))));
        if self.reg_open {
            let mut form = w::card().p(T::SP_12).gap(T::SP_8)
                .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Register a service"));
            for i in self.reg.iter().flatten() { form = form.child(w::input(i)); }
            form = form.child(div().flex().gap(T::SP_6)
                .child(w::primary("svc-reg-ok", "Register").icon(IconName::Check).on_click(|_, _, cx| register_submit(cx)))
                .child(w::button("svc-reg-cancel", "Cancel").on_click(|_, _, cx| register_cancel(cx))));
            page = page.child(form);
        }
        // A failed load never reads as "nothing registered": the error with Retry instead.
        if !self.error.is_empty() && (!self.loaded || cards.is_empty()) {
            page = page.child(w::error_state("svc-retry", self.error.clone(), |_, _, cx| load(cx)));
        } else if self.loading && !self.loaded {
            page = page.child(w::loading("Loading\u{2026}"));
        } else if self.loaded && cards.is_empty() {
            page = page.child(div().flex().flex_col().items_center().pt(T::SP_24).text_color(T::text_muted())
                .child(w::icon(IconName::Server, T::SP_32))
                .child(w::empty(if project.is_empty() { "No services registered" } else { "No services registered for this project" })));
        }
        for (i, c) in cards.iter().enumerate() {
            page = page.child(card_view(i, c, self.edit_name.as_ref(), self.log_scroll.get(&c.key).cloned()));
        }
        let others = other_forwards(&self.services, &self.forwards, &project);
        if !others.is_empty() {
            let al = aliases(cx);
            let mut items: Vec<SharedString> = vec!["No project".into()];
            items.extend(al.iter().map(|a| SharedString::from(a.clone())));
            let mut b = div().flex().flex_col().gap(T::SP_4).px(T::SP_10).py(T::SP_8).rounded(T::R_CONTROL).border_1().border_color(T::border())
                .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child("Other forwards"))
                .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child("Not linked to a service. Pick the project each one serves: outside collaborators on that project can open it."));
            for (i, f) in others.iter().enumerate() {
                let host = s(f, "host");
                let url = { let u = s(f, "url"); if u.is_empty() { host.clone() } else { u } };
                let p = s(f, "project");
                let on = al.iter().position(|a| *a == p).map(|i| i + 1).unwrap_or(0);
                let (h1, h2, al2) = (host.clone(), host.clone(), al.clone());
                b = b.child(w::rule()).child(div().id(("svc-other", i)).flex().items_center().gap(T::SP_6)
                    .child(link(("svc-other-link", i), url).flex_1())
                    .when(t(f, "public"), |d| d.child(w::tag("PUBLIC", "warn")))
                    .child(mono_muted(format!("port {}", s(f, "port"))))
                    .child(w::select(("svc-other-proj", i), items.clone(), Some(on), "No project", false, move |i, _, cx| {
                        let a = if i == 0 { String::new() } else { al2.get(i - 1).cloned().unwrap_or_default() };
                        set_forward_project(&h1, &a, cx);
                    }))
                    .child(w::icon_button(("svc-other-rm", i), IconName::Trash, "Remove this forward").on_click(move |_, _, cx| remove_forward(&h2, cx))));
            }
            page = page.child(b);
        }
        page
    }
}

// ─── The driver ───

fn arg(args: &[String], i: usize) -> &str { args.get(i).map(|s| s.as_str()).unwrap_or("") }

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    if !name.starts_with("services-") { return None; }
    ent(cx).update(cx, |s, cx| s.ensure_inputs(window, cx));
    match name {
        "services-load" => load(cx),
        "services-register-open" => register_open(window, cx),
        "services-register-cancel" => register_cancel(cx),
        // name, command, cwd, stop command, tunnel port; then Register.
        "services-register" => {
            register_open(window, cx);
            let inputs = read(cx, |st, _| st.reg.clone());
            for (i, inp) in inputs.iter().enumerate() { kit::set_value(inp, arg(args, i), window, cx); }
            register_submit(cx);
        }
        // start | stop | restart | logs | port | delete, key.
        "services-act" => act(arg(args, 1), arg(args, 0), window, cx),
        // key, off | tunnel | subdomain | path.
        "services-expose" => expose(arg(args, 0), mode_index(arg(args, 1)), window, cx),
        "services-edit-name" => { let i = read(cx, |st, _| st.edit_name.clone()); kit::set_value(&i, arg(args, 0), window, cx); }
        "services-edit-public" => { let on = arg(args, 0) == "true"; upd(cx, |st, _| if let Some(e) = st.edit.as_mut() { e.public = on; }); }
        "services-edit-save" => if let Some(k) = read(cx, |st, _| st.edit.as_ref().map(|e| e.key.clone())) { save_expose(&k, window, cx); },
        "services-edit-cancel" => { upd(cx, |st, _| st.edit = None); }
        "services-fwd-remove" => remove_forward(arg(args, 0), cx),
        // host, alias ("" none).
        "services-fwd-project" => set_forward_project(arg(args, 0), arg(args, 1), cx),
        "services-open-url" => open_url(arg(args, 0), cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// The driver's view of this screen (the Slint client's names).
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    let project = crundi_shell::project(cx);
    let al = aliases(cx);
    read(cx, |st, cx| {
        let edit_name = kit::value(&st.edit_name, cx);
        let cards: Vec<Value> = st.cards(&project).iter().map(|c| { let edit_name = if c.editing { edit_name.clone() } else { String::new() }; json!({
            "key": c.key, "name": c.name, "status": c.status, "tone": c.tone,
            "running": c.running, "command": c.command, "uptime": c.uptime,
            "stats": if c.stats { json!({ "cpu": c.cpu, "mem": c.mem, "badge": c.badge, "cpuLine": c.cpu_hist.len() >= 2 }) } else { Value::Null },
            "tunnelBtn": c.tunnel_btn, "expose": c.expose, "portLabel": c.port_label, "mode": c.mode,
            "link": c.link, "public": c.public, "stateWord": c.state_word, "fwdHost": c.fwd_host,
            "editing": c.editing, "editPrefix": c.edit_prefix, "editSuffix": c.edit_suffix,
            "editName": edit_name.clone(), "editPublic": c.edit_public,
            "logsOpen": c.logs_open, "logs": c.logs.chars().take(400).collect::<String>(),
        }) }).collect();
        let others: Vec<Value> = other_forwards(&st.services, &st.forwards, &project).iter().map(|f| {
            let p = s(f, "project");
            let url = { let u = s(f, "url"); if u.is_empty() { s(f, "host") } else { u } };
            let proj = if al.contains(&p) { p } else { String::new() };
            json!({ "host": s(f, "host"), "url": url, "port": s(f, "port"), "public": t(f, "public"), "project": proj })
        }).collect();
        let mut projects = vec!["No project".to_string()];
        projects.extend(al.iter().cloned());
        json!({
            "shown": st.shown, "loaded": st.loaded, "loading": st.loading, "error": st.error,
            "title": if project.is_empty() { "Services \u{b7} all projects" } else { "Services" },
            "empty": if project.is_empty() { "No services registered" } else { "No services registered for this project" },
            "regOpen": st.reg_open, "cards": cards, "others": others, "polling": st.poll.is_some(),
            "lastUrl": st.last_url, "projects": projects,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn bytes_and_percent() {
        assert_eq!(fmt_bytes(None), "\u{2014}");
        assert_eq!(fmt_bytes(Some(512.0)), "512 B");
        assert_eq!(fmt_bytes(Some(2048.0)), "2.0 KB");
        assert_eq!(fmt_bytes(Some(150.0 * 1024.0 * 1024.0)), "150 MB");
        assert_eq!(fmt_pct(None), "\u{2014}");
        assert_eq!(fmt_pct(Some(3.14159)), "3.1%");
        assert_eq!(fmt_pct(Some(201.4)), "201%");
    }

    #[::core::prelude::v1::test]
    fn levels() {
        assert_eq!(level(10.0), "ok");
        assert_eq!(level(60.0), "warn");
        assert_eq!(level(85.0), "error");
    }

    #[::core::prelude::v1::test]
    fn forward_names() {
        assert_eq!(default_forward_name(&json!({ "key": "playground:Test App" })), "test-app");
        assert_eq!(default_forward_name(&json!({ "name": "!!!" })), "app");
        assert_eq!(default_forward_name(&json!({ "name": "demo:web_dev" })), "web-dev");
        assert!(valid_name("my-app1") && valid_name("a"));
        assert!(!valid_name("-app") && !valid_name("app-") && !valid_name("App") && !valid_name(""));
    }

    #[::core::prelude::v1::test]
    fn ints() {
        assert_eq!(parse_int(" 4561 "), 4561);
        assert_eq!(parse_int("80abc"), 80);
        assert_eq!(parse_int(""), 0);
        assert_eq!(parse_int(" "), 0);
        assert_eq!(parse_int("x"), 0);
    }

    #[::core::prelude::v1::test]
    fn card_states() {
        let fwds = vec![json!({ "host": "web", "port": 4561, "mode": "path", "url": "/tunnel/web/", "public": true })];
        let x = json!({ "key": "demo:web", "name": "web", "status": "running", "command": "sleep 600", "uptime": "2m", "pid": 42, "memory": 2048, "cpuPct": 1.5, "tunnelPort": 4561 });
        let c = card(&x, &fwds, None, None, None, None);
        assert_eq!(c.status, "RUNNING");
        assert_eq!(c.uptime, "\u{b7} up 2m");
        assert!(c.stats && c.expose && !c.tunnel_btn);
        assert_eq!(c.badge, "pid 42");
        assert_eq!((c.mode, c.link.as_str(), c.public, c.fwd_host.as_str(), c.port_label.as_str()), (3, "/tunnel/web/", true, "web", "Port 4561"));

        let c2 = card(&json!({ "key": "demo:w", "status": "stopped", "tunnelPort": 0 }), &[], None, None, None, None);
        assert!(!c2.stats && !c2.expose && c2.tunnel_btn && c2.mode == 0);
        assert_eq!(c2.name, "demo:w");

        let x3 = json!({ "key": "demo:t", "status": "stopped", "tunnelPort": 3000, "tunnelEnabled": true, "tunnelStatus": "connecting" });
        let c3 = card(&x3, &[], None, None, None, None);
        assert_eq!((c3.mode, c3.state_word.as_str(), c3.state_tone.as_str()), (1, "starting…", "warn"));

        let e = Edit { key: "demo:t".into(), port: 3000, mode: "path".into(), public: false };
        let c4 = card(&x3, &[], None, None, Some(&e), Some(&"line".to_string()));
        assert!(c4.editing && c4.logs_open);
        assert_eq!((c4.edit_prefix.as_str(), c4.edit_suffix.as_str()), ("/tunnel/", "/"));
        let opts = json!({ "choices": [{ "id": "subdomain", "available": true, "suggestedDomain": "x.dev" }] });
        let e5 = Edit { mode: "subdomain".into(), ..e };
        assert_eq!(card(&x3, &[], Some(&opts), None, Some(&e5), None).edit_suffix, ".x.dev");
    }

    #[::core::prelude::v1::test]
    fn containers_badge() {
        let x = json!({ "key": "k", "status": "running", "containers": [] });
        assert_eq!(card(&x, &[], None, None, None, None).badge, "no containers");
        let stats = json!({ "k": { "containers": [{ "name": "a" }, { "name": "b" }], "cpuPct": 70.0, "cpuHistory": [1, 2, 3] } });
        let c = card(&x, &[], None, Some(&stats), None, None);
        assert_eq!(c.badge, "2 containers");
        assert_eq!(c.cpu_tone, "warn");
        assert_eq!(c.cpu_hist.len(), 3);
    }

    #[::core::prelude::v1::test]
    fn loose_forwards() {
        let svcs = vec![json!({ "tunnelPort": 3000 })];
        let fwds = vec![json!({ "host": "a", "port": 3000 }), json!({ "host": "b", "port": 4000, "project": "other" }), json!({ "host": "c", "port": 5000 })];
        let hosts = |p: &str| other_forwards(&svcs, &fwds, p).iter().map(|f| s(f, "host")).collect::<Vec<_>>();
        assert_eq!(hosts(""), vec!["b", "c"]);
        assert_eq!(hosts("demo"), vec!["c"]);
    }

    #[::core::prelude::v1::test]
    fn signatures() {
        let a = signature(&[json!({ "key": "k", "status": "running", "tunnelPort": 1 })]);
        let b = signature(&[json!({ "key": "k", "status": "stopped", "tunnelPort": 1 })]);
        assert_ne!(a, b);
    }
}
