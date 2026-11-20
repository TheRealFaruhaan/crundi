//! Info (the Info tab: the server's facts and its log; Slint: src/ops/info.rs,
//! ui/ops/info.slint, specs/ops/info.md; the web's renderInfo,
//! renderSystemStats, loadMaintenance, loadCollaborators).
//!
//! In the web's order: Project and Status (with a project open), This
//! machine (GET /api/stats every 2 s while Info is on show), Reclaim disk
//! space (GET/POST /api/maintenance), Outside collaborators (GET/POST
//! /api/collaborators, the invite form, the new-passcode and "not a git
//! repository yet" dialogs), Tunnel (GET /api/tunnel), Server Logs (GET
//! /api/server-logs, then live over /ws `server-log` after `subscribe-logs`,
//! sent again on reconnect; Copy). The screen is one entity in a Global.
//!
//! Driver ("ops:<name>", the Slint client's):
//!   info-act <action> <arg>    a button, as a click
//!   info-field <name> <text>   the invite form: name, tg, amount, unit (0 days, 1 hours), limit
//!   info-log <text> [level]    a live log line, as /ws brings it

mod rules;

use crundi_core::api::{Api, ApiError, ApiResult};
use crate::compat::{session, AreaEvent};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use rules::{SysRow, Person};
use serde_json::{json, Value};
use std::{collections::HashSet, rc::Rc, time::Duration};

/// Lines of the log kept (the web keeps them all; this keeps the newest).
const LOG_KEEP: usize = 2000;
/// A two-click button stays armed this long (the Slint confirm-arm).
const ARM: Duration = Duration::from_secs(3);

#[derive(PartialEq)]
struct Sys { cpu: String, overall: f64, cores: Vec<f64>, hist: Vec<(f32, f32)>, rows: Vec<SysRow> }
struct Tunnel { url: String, status: String, cf: String }
struct Cred { title: String, url: String, name: String, pass: String, projects: String, ends: String, block: String }
struct Git { project: String, title: String, commit: String, kept: String, risky: String, tail: String, busy: bool }
struct Inputs { name: Entity<w::InputState>, tg: Entity<w::InputState>, amount: Entity<w::InputState>, limit: Entity<w::InputState>, _subs: Vec<Subscription> }

pub struct InfoView {
    shown: bool,
    rendered: bool,
    services: Vec<Value>,
    /// What the page shows of the session, kept here (rule 1): the open
    /// project's facts and whether the server is reachable (`sync`).
    facts: Option<(String, String, String, String, String)>,
    online: bool,
    logs: Vec<(String, String)>,
    logs_loading: bool,
    /// The server log, virtualised (only the lines on screen are laid out;
    /// an appended line is one splice), following its end.
    log_list: ListState,
    /// The page's sections as a virtual list (`kinds`, `relist`).
    page_list: ListState,
    page_kinds: Vec<&'static str>,
    sys: Option<Sys>,
    maint: Vec<Value>,
    maint_state: String,
    maint_busy: String,
    armed: String,
    arm_gen: u64,
    collab_state: String,
    sb_ok: bool,
    sb_problems: Vec<String>,
    sb_fix: bool,
    people: Vec<Person>,
    picked: HashSet<String>,
    inv_name: String,
    inv_tg: String,
    inv_amount: String,
    inv_unit: usize,
    inv_limit: String,
    inputs: Option<Inputs>,
    tunnel: Option<Tunnel>,
    cred: Option<Cred>,
    git: Option<Git>,
    copied: String,
    poll: Option<Task<()>>,
    focus: FocusHandle,
    dlg_scroll: ScrollHandle,
}

struct Handle(Entity<InfoView>);
impl Global for Handle {}

fn fresh(cx: &mut Context<InfoView>) -> InfoView {
    InfoView {
        shown: false, rendered: false, services: vec![], logs: vec![], logs_loading: true, page_list: ListState::new(0, ListAlignment::Top, T::INFO_LOGS_H), page_kinds: vec![], log_list: { let l = ListState::new(0, ListAlignment::Top, T::INFO_LOGS_H); l.set_follow_mode(FollowMode::Tail); l },
        facts: None, online: true, sys: None, maint: vec![], maint_state: "loading".into(), maint_busy: String::new(), armed: String::new(), arm_gen: 0,
        collab_state: "loading".into(), sb_ok: false, sb_problems: vec![], sb_fix: false, people: vec![], picked: HashSet::new(),
        inv_name: String::new(), inv_tg: String::new(), inv_amount: "7".into(), inv_unit: 0, inv_limit: String::new(), inputs: None,
        tunnel: None, cred: None, git: None, copied: String::new(), poll: None, focus: cx.focus_handle(), dlg_scroll: ScrollHandle::new(),
    }
}

fn ent(cx: &mut App) -> Entity<InfoView> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let e = cx.new(fresh);
    cx.set_global(Handle(e.clone()));
    e
}

/// The session's part of the page again; a redraw only when it changed.
fn sync(cx: &mut App) {
    if !cx.has_global::<Handle>() { return; }
    let services = ent(cx).read(cx).services.clone();
    let facts = project_facts(&services, cx);
    let online = session::get(cx).online;
    let same = { let v = ent(cx); let v = v.read(cx); v.facts == facts && v.online == online };
    if !same { with(cx, |s, _| { s.facts = facts; s.online = online; }); }
}

/// Change what the page does not draw (no redraw).
fn hush<R>(cx: &mut App, f: impl FnOnce(&mut InfoView) -> R) -> R { let e = ent(cx); e.update(cx, |v, _| f(v)) }

fn with<R>(cx: &mut App, f: impl FnOnce(&mut InfoView, &mut Context<InfoView>) -> R) -> R {
    let e = ent(cx);
    e.update(cx, |v, cx| { let r = f(v, cx); v.relist(); cx.notify(); r })
}

/// A toast from a server's answer (no window at hand: the window's).
fn toast(cx: &mut App, text: impl Into<SharedString>, kind: w::Toast) {
    let text: SharedString = text.into();
    let Some(h) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
    let _ = cx.update_window(h, |_, window, cx| w::toast(window, cx, text, kind));
}

/// POST JSON and keep the answer whatever its status (a 400 carries its words, a 409 what to ask next).
fn post_raw(api: &Api, path: &str, body: &Value) -> ApiResult {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    let (st, text) = api.post_with_progress(path, &bytes, &|_, _| {})?;
    if st == 404 { return Err(ApiError::Status(404, String::new())); }
    if st == 0 { return Err(ApiError::Net("Could not reach the server".into())); }
    Ok(serde_json::from_str(&text).unwrap_or_else(|_| json!({ "ok": false, "error": format!("The server answered {st}") })))
}

fn post(cx: &mut App, path: &'static str, body: Value, done: impl FnOnce(Value, &mut App) + 'static) {
    crundi_shell::call(cx, move |api| post_raw(api, path, &body), move |r, cx| done(r.unwrap_or_else(|e| json!({ "ok": false, "error": e })), cx));
}

// ─── The hooks ───

/// This screen's tab: its page.
pub fn view(tab: &str, window: &mut Window, cx: &mut App) -> Option<AnyView> {
    if tab != "info" { return None; }
    let e = ent(cx);
    e.update(cx, |v, cx| v.ensure_inputs(window, cx));
    Some(e.into())
}

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    match e {
        AreaEvent::State(v) => {
            if !cx.has_global::<Handle>() { return; }
            if let Some(sv) = v.get("services").and_then(|x| x.as_array()).cloned() { hush(cx, |s| s.services = sv); }
            sync(cx);
        }
        AreaEvent::Ws("server-log", m) => {
            let m = (*m).clone();
            if cx.has_global::<Handle>() && ent(cx).read(cx).rendered { with(cx, |s, _| s.append_log(&m)); }
        }
        AreaEvent::Connected(true) => {
            sync(cx);
            if cx.has_global::<Handle>() && ent(cx).read(cx).shown { session::ws_send(cx, &json!({ "type": "subscribe-logs" })); render_all(cx); }
        }
        AreaEvent::Connected(false) => sync(cx),
        _ => {}
    }
}

pub fn on_project(_alias: &str, cx: &mut App) {
    // Project and Status show the open project.
    sync(cx);
}

pub fn on_signed_in(_cx: &mut App) {}

pub fn on_signed_out(cx: &mut App) {
    if !cx.has_global::<Handle>() { return; }
    with(cx, |s, cx| {
        let inputs = s.inputs.take();
        *s = fresh(cx);
        s.inputs = inputs;
    });
}

pub fn on_tab(tab: &str, cx: &mut App) {
    let on = tab == "info";
    if !on && !cx.has_global::<Handle>() { return; }
    hush(cx, |s| { s.shown = on; if !on { s.poll = None; } });
    if on { sync(cx); render_all(cx); poll_start(cx); }
}

// ─── Loading ───

fn render_all(cx: &mut App) {
    with(cx, |s, _| s.rendered = true);
    crundi_shell::call(cx, |api| api.get("/api/tunnel"), |r, cx| with(cx, |s, _| {
        s.tunnel = r.ok().map(|t| {
            let url = rules::s(&t, "tunnelUrl");
            let tunnels = t.get("tunnels").and_then(|x| x.as_object()).cloned().unwrap_or_default();
            let active = tunnels.values().filter(|x| rules::s(x, "status") == "active").count();
            let target = rules::s(&t, "target");
            Tunnel {
                url: if url.is_empty() { "—".into() } else { url },
                status: if active > 0 { "Connected".into() } else if !tunnels.is_empty() { "Connecting...".into() } else { "Not configured".into() },
                cf: if rules::flag(&t, "named") && !target.is_empty() { format!("HTTP → {target}") } else { String::new() },
            }
        });
    }));
    crundi_shell::call(cx, |api| api.get("/api/server-logs"), |r, cx| {
        if let Ok(d) = r {
            let mut lines: Vec<(String, String)> = d.get("logs").and_then(|x| x.as_array()).cloned().unwrap_or_default().iter().map(rules::log_line).collect();
            if lines.len() > LOG_KEEP { lines.drain(..lines.len() - LOG_KEEP); }
            with(cx, |s, _| { s.logs = lines; s.logs_loading = false; s.log_list.reset(s.logs.len()); s.log_list.scroll_to_end(); });
        }
    });
    session::ws_send(cx, &json!({ "type": "subscribe-logs" }));
    load_maint(cx);
    load_collab(cx);
    poll(cx);
}

/// The machine's numbers every 2 s while Info is on show (ensureStatsPolling).
fn poll_start(cx: &mut App) {
    let task = cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(Duration::from_secs(2)).await;
        cx.update(|cx| poll(cx));
    });
    hush(cx, |s| s.poll = Some(task));
}

fn poll(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/stats"), |r, cx| {
        let Ok(d) = r else { return };
        let Some(sys) = d.get("system").filter(|x| x.is_object()).cloned() else { return };
        let cpu = sys.get("cpu").cloned().unwrap_or_default();
        let overall = cpu.get("overall").and_then(|x| x.as_f64());
        let cores: Vec<f64> = cpu.get("cores").and_then(|x| x.as_array()).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
        let hist: Vec<f64> = sys.pointer("/history/cpu").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default();
        let s2 = Sys { cpu: rules::fmt_pct(overall), overall: overall.unwrap_or(0.0), cores, hist: rules::spark(&hist, 100.0), rows: rules::sys_rows(&sys) };
        // The same figures draw nothing.
        if ent(cx).read(cx).sys.as_ref() == Some(&s2) { return; }
        with(cx, |s, _| s.sys = Some(s2));
    });
}

fn load_maint(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/maintenance"), |r, cx| with(cx, |s, _| match r {
        Ok(d) => { s.maint = d.get("tasks").and_then(|x| x.as_array()).cloned().unwrap_or_default(); s.maint_state = if s.maint.is_empty() { "empty".into() } else { "data".into() }; }
        Err(_) => s.maint_state = "error".into(),
    }));
}

fn load_collab(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/collaborators"), |r, cx| with(cx, |s, _| {
        let d = match r { Ok(d) if rules::ok(&d) => d, _ => { s.collab_state = "error".into(); return; } };
        let list = d.get("collaborators").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let sb = d.get("sandbox").cloned().unwrap_or(json!({ "ok": false, "problems": ["Could not check the sandbox"] }));
        s.people = rules::people_of(&list, rules::now_ms());
        s.sb_ok = rules::ok(&sb);
        s.sb_problems = sb.get("problems").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        s.sb_fix = rules::flag(&sb, "canAutoFix");
        s.collab_state = "data".into();
    }));
}

// ─── The screen's state ───

impl InfoView {
    fn ensure_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inputs.is_some() { return; }
        let mk = |ph: &'static str, v: &str, window: &mut Window, cx: &mut Context<Self>| {
            let v = v.to_string();
            cx.new(|cx| { let mut s = w::InputState::new(window, cx).placeholder(ph); s.set_value(v, window, cx); s })
        };
        let name = mk("e.g. Sam", &self.inv_name, window, cx);
        let tg = mk("without the @", &self.inv_tg, window, cx);
        let amount = mk("", &self.inv_amount, window, cx);
        let limit = mk("e.g. 2M or 500k — blank for no limit", &self.inv_limit, window, cx);
        let sub = |e: &Entity<w::InputState>, k: u8, window: &mut Window, cx: &mut Context<Self>| {
            cx.subscribe_in(e, window, move |this: &mut Self, inp: &Entity<w::InputState>, ev: &w::InputEvent, _, cx| {
                if !matches!(ev, w::InputEvent::Change) { return; }
                let v = inp.read(cx).value().to_string();
                match k { 0 => this.inv_name = v, 1 => this.inv_tg = v, 2 => this.inv_amount = v, _ => this.inv_limit = v }
                cx.notify();
            })
        };
        let subs = vec![sub(&name, 0, window, cx), sub(&tg, 1, window, cx), sub(&amount, 2, window, cx), sub(&limit, 3, window, cx)];
        self.inputs = Some(Inputs { name, tg, amount, limit, _subs: subs });
    }

    fn append_log(&mut self, m: &Value) {
        self.logs.push(rules::log_line(m));
        let n = self.logs.len();
        self.log_list.splice(n - 1..n - 1, 1);
        if n > LOG_KEEP { self.logs.remove(0); self.log_list.splice(0..1, 0); }
        self.logs_loading = false;
    }

    fn expiry(&self) -> String { rules::expiry_text(&self.inv_amount, self.inv_unit == 1, rules::now_ms(), &rules::ends) }

    fn copy_text(&self, what: &str) -> String {
        match (&self.cred, what) {
            (Some(c), "url") => c.url.clone(), (Some(c), "name") => c.name.clone(), (Some(c), "pass") => c.pass.clone(),
            (Some(c), _) => c.block.clone(), (None, _) => String::new(),
        }
    }
}

/// The open project's facts (alias, name, path, terminal, services), None without one.
fn project_facts(services: &[Value], cx: &App) -> Option<(String, String, String, String, String)> {
    let ss = session::get(cx);
    let alias = ss.project.clone();
    if alias.is_empty() { return None; }
    let p = ss.projects.iter().find(|x| rules::s(x, "alias") == alias)?;
    let (name, path) = (rules::s(p, "name"), rules::s(p, "path"));
    let active = ss.terminals.iter().any(|t| rules::s(t, "project") == alias && rules::s(t, "kind") != "parked");
    let mine: Vec<&Value> = services.iter().filter(|x| rules::s(x, "alias") == alias).collect();
    let running = mine.iter().filter(|x| rules::s(x, "status") == "running").count();
    Some((alias.clone(), if name.is_empty() { alias } else { name }, if path.is_empty() { "—".into() } else { path },
        if active { "Active".into() } else { "Not started".into() }, format!("{} registered, {running} running", mine.len())))
}

/// The projects to tick in the invite form: (alias, label).
fn invite_projects(cx: &App) -> Vec<(String, String)> {
    session::get(cx).projects.iter().map(|p| { let a = rules::s(p, "alias"); let n = rules::s(p, "name"); (a.clone(), if n.is_empty() { a } else { n }) }).collect()
}

// ─── Actions ───

fn act(action: &str, arg: &str, window: &mut Window, cx: &mut App) {
    let arg = arg.to_string();
    match action {
        "open-url" => { if arg.starts_with("http://") || arg.starts_with("https://") { if std::env::var_os("CRUNDI_TEST_PORT").is_none() { cx.open_url(&arg); } } }
        "copy-logs" => {
            let text = ent(cx).read(cx).logs.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>().join("\n");
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            w::toast(window, cx, "Logs copied", w::Toast::Info);
        }
        "maint-run" => maint_click(arg, cx),
        "inv-project" => with(cx, |s, _| { if !s.picked.remove(&arg) { s.picked.insert(arg); } }),
        "inv-edit" => with(cx, |_, _| {}),
        "collab-create" => collab_create(Some(window), cx),
        "collab-sandbox-setup" => w::ask_confirm(window, cx, w::Ask {
            title: "Set up the collaborator sandbox?".into(),
            message: "This uses sudo on the server to install bubblewrap and socat, and adds an AppArmor profile that lets bubblewrap create sandboxes.".into(),
            ok: "Set up".into(), ..Default::default()
        }, |yes, _, cx| {
            if !yes { return; }
            post(cx, "/api/collaborators", json!({ "action": "sandboxSetup" }), |r, cx| {
                if rules::ok(&r) { toast(cx, "Sandbox ready", w::Toast::Success); } else { toast(cx, rules::err_of(&r, "Setup failed"), w::Toast::Error); }
                load_collab(cx);
            });
        }),
        a if a.starts_with("collab-") => collab_action(a, arg, window, cx),
        "cred-copy" => {
            let text = ent(cx).read(cx).copy_text(&arg);
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            with(cx, |s, _| s.copied = arg.clone());
            cx.spawn(async move |cx: &mut AsyncApp| {
                cx.background_executor().timer(Duration::from_millis(1400)).await;
                cx.update(|cx| with(cx, |s, _| if s.copied == arg { s.copied.clear(); }));
            }).detach();
        }
        "cred-close" => with(cx, |s, _| { s.cred = None; s.copied.clear(); }),
        "git-cancel" => with(cx, |s, _| s.git = None),
        "git-go" => {
            let project = with(cx, |s, _| { if let Some(g) = &mut s.git { g.busy = true; g.project.clone() } else { String::new() } });
            if project.is_empty() { return; }
            post(cx, "/api/collaborators", json!({ "action": "initGit", "project": project }), move |r, cx| {
                with(cx, |s, _| if let Some(g) = &mut s.git { g.busy = false; });
                if !rules::ok(&r) { toast(cx, rules::err_of(&r, "git init failed"), w::Toast::Error); return; }
                with(cx, |s, _| s.git = None);
                toast(cx, format!("Set up git in {project}"), w::Toast::Success);
                collab_create(None, cx);
            });
        }
        _ => {}
    }
}

/// A reclaim button: the first click arms it ("Clear it?"), the second runs it.
fn maint_click(id: String, cx: &mut App) {
    let armed = ent(cx).read(cx).armed == id;
    if !armed {
        let g = with(cx, |s, _| { s.armed = id.clone(); s.arm_gen += 1; s.arm_gen });
        cx.spawn(async move |cx: &mut AsyncApp| {
            cx.background_executor().timer(ARM).await;
            cx.update(|cx| with(cx, |s, _| if s.arm_gen == g { s.armed.clear(); }));
        }).detach();
        return;
    }
    with(cx, |s, _| { s.armed.clear(); s.maint_busy = id.clone(); });
    crundi_shell::call(cx, move |api| post_raw(api, "/api/maintenance", &json!({ "id": id })), |r, cx| {
        with(cx, |s, _| s.maint_busy.clear());
        match r {
            Ok(d) if rules::ok(&d) => toast(cx, match d.get("freed").and_then(|x| x.as_f64()) { None => "Done.".to_string(), Some(f) => format!("Freed {}", rules::fmt_bytes(f)) }, w::Toast::Success),
            Ok(d) => toast(cx, rules::err_of(&d, "The command failed"), w::Toast::Error),
            Err(e) => toast(cx, e, w::Toast::Error),
        }
        load_maint(cx);
        poll(cx);
    });
}

fn collab_send(body: Value, cx: &mut App) {
    let b2 = body.clone();
    post(cx, "/api/collaborators", body, move |r, cx| {
        if !rules::ok(&r) { toast(cx, rules::err_of(&r, "That did not work"), w::Toast::Error); return; }
        let pass = rules::s(&r, "passcode");
        if !pass.is_empty() {
            let name = rules::s(&r, "name");
            show_invite("New passcode", if name.is_empty() { "them" } else { &name }, &pass, &[], "", cx);
        }
        if rules::s(&b2, "action") == "merge" { toast(cx, format!("Merged {} into {}", rules::s(&r, "merged"), rules::s(&r, "into")), w::Toast::Success); }
        else { toast(cx, "Done", w::Toast::Success); }
        load_collab(cx);
    });
}

fn collab_action(action: &str, id: String, window: &mut Window, cx: &mut App) {
    match action {
        "collab-extend" => w::ask(window, cx, w::Ask {
            title: "Extend access".into(), message: "Extend access by how long? e.g. 6h, 3d, or 7 for seven days".into(),
            ok: "Extend".into(), prompt: Some("7d".into()), ..Default::default()
        }, move |raw, window, cx| {
            let Some(raw) = raw else { return };
            match rules::parse_extend(&raw) { Some(h) => collab_send(json!({ "id": id, "action": "update", "hours": h }), cx), None => w::toast(window, cx, "Try something like 6h or 3d", w::Toast::Error) }
        }),
        "collab-revoke" => collab_send(json!({ "id": id, "action": "revoke" }), cx),
        "collab-unrevoke" => collab_send(json!({ "id": id, "action": "update", "revoked": false }), cx),
        "collab-limit" => {
            let cur = ent(cx).read(cx).people.iter().find(|p| p.id == id).map(|p| p.limit).unwrap_or(0.0);
            w::ask(window, cx, w::Ask {
                title: "Token limit".into(), message: "Token limit for this person, across all their projects. e.g. 2M or 500k. Leave empty for no limit.".into(),
                ok: "Save".into(), prompt: Some(if cur > 0.0 { rules::fmt_tok(cur) } else { String::new() }), ..Default::default()
            }, move |raw, window, cx| {
                let Some(raw) = raw else { return };
                let n = if raw.trim().is_empty() { Some(0.0) } else { rules::parse_tok(&raw).filter(|x| *x > 0.0) };
                match n { Some(n) => collab_send(json!({ "id": id, "action": "setLimit", "tokenLimit": n }), cx), None => w::toast(window, cx, "Try something like 2M or 500k", w::Toast::Error) }
            });
        }
        "collab-domain-remove" => {
            let (pid, host) = id.split_once('\n').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or_default();
            collab_send(json!({ "id": pid, "action": "removeDomain", "host": host }), cx);
        }
        "collab-reset-usage" => w::ask_confirm(window, cx, w::Ask { title: "Reset usage".into(), message: "Reset their token count to zero?".into(), ok: "Reset".into(), ..Default::default() },
            move |yes, _, cx| if yes { collab_send(json!({ "id": id, "action": "resetUsage" }), cx); }),
        "collab-reissue" => collab_send(json!({ "id": id, "action": "reissue" }), cx),
        "collab-merge-owner" => collab_send(json!({ "id": id, "action": "merge" }), cx),
        "collab-remove" => w::ask_confirm(window, cx, w::Ask { title: "Remove this access?".into(), message: "Their working copy is deleted. Their branch and commits are kept.".into(), ok: "Remove".into(), danger: true, ..Default::default() },
            move |yes, _, cx| if yes { collab_send(json!({ "id": id, "action": "remove" }), cx); }),
        _ => {}
    }
}

fn collab_create(window: Option<&mut Window>, cx: &mut App) {
    let fail = |window: Option<&mut Window>, cx: &mut App, t: &str| match window { Some(w0) => w::toast(w0, cx, t.to_string(), w::Toast::Error), None => toast(cx, t.to_string(), w::Toast::Error) };
    let (name, tg, amount, unit, lim_raw, picked) = { let s = ent(cx).read(cx); (s.inv_name.clone(), s.inv_tg.clone(), s.inv_amount.clone(), s.inv_unit, s.inv_limit.clone(), s.picked.clone()) };
    let amount = amount.trim().parse::<f64>().ok().map(|x| x.trunc() as i64).filter(|x| *x != 0).unwrap_or(7);
    let hours = if unit == 1 { amount } else { amount * 24 };
    let chosen: Vec<String> = invite_projects(cx).into_iter().map(|(a, _)| a).filter(|a| picked.contains(a)).collect();
    if name.trim().is_empty() { return fail(window, cx, "Give them a name"); }
    if chosen.is_empty() { return fail(window, cx, "Pick at least one project"); }
    let limit = if lim_raw.trim().is_empty() { 0.0 } else { match rules::parse_tok(&lim_raw).filter(|x| *x > 0.0) { Some(n) => n, None => return fail(window, cx, "Token limit: try something like 2M or 500k") } };
    let body = json!({ "action": "create", "name": name, "telegram": tg, "hours": hours, "projects": chosen, "tokenLimit": limit });
    post(cx, "/api/collaborators", body, move |r, cx| {
        if !rules::ok(&r) {
            let needs: Vec<String> = r.get("needsGit").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            if needs.len() == 1 { offer_git_init(needs[0].clone(), cx); return; }
            toast(cx, rules::err_of(&r, "Could not create that"), w::Toast::Error);
            return;
        }
        let made = r.get("collaborators").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let pass = rules::s(&r, "passcode");
        if !pass.is_empty() {
            let projects: Vec<String> = made.iter().map(|x| rules::s(x, "project")).collect();
            let ends = made.first().and_then(|m| rules::ms_of(m, "expiresAt")).map(rules::ends_long).unwrap_or_default();
            show_invite(&format!("Access created for {name}"), &name, &pass, &projects, &ends, cx);
        } else if rules::flag(&r, "sharesExisting") {
            toast(cx, "Added to their existing access — their passcode is unchanged", w::Toast::Success);
        }
        let failed: Vec<String> = r.get("failed").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        if !failed.is_empty() { toast(cx, failed.join("; "), w::Toast::Error); }
        load_collab(cx);
    });
}

/// showInviteModal: the passcode, shown once.
fn show_invite(title: &str, name: &str, pass: &str, projects: &[String], ends: &str, cx: &mut App) {
    let url = crundi_shell::api(cx).map(|a| a.base().to_string()).unwrap_or_default();
    let block = rules::invite_block(&url, name, pass, projects, ends);
    let c = Cred { title: title.into(), url, name: name.into(), pass: pass.into(), projects: projects.join(", "), ends: if projects.is_empty() { String::new() } else { ends.into() }, block };
    with(cx, |s, _| { s.cred = Some(c); s.copied.clear(); });
}

/// offerGitInit: what making the project a repository would commit.
fn offer_git_init(project: String, cx: &mut App) {
    let p2 = project.clone();
    post(cx, "/api/collaborators", json!({ "action": "gitPreview", "project": project }), move |p, cx| {
        if !rules::ok(&p) { toast(cx, rules::err_of(&p, "Could not inspect that project"), w::Toast::Error); return; }
        if rules::flag(&p, "alreadyGit") { collab_create(None, cx); return; }
        let (commit, kept, risky, tail) = rules::git_words(&p);
        with(cx, |s, _| s.git = Some(Git { title: format!("{p2} is not a git repository yet"), project: p2, commit, kept, risky, tail, busy: false }));
    });
}

// ─── Drawing ───

/// A button's click: the action with its argument.
fn on(a: &'static str, arg: impl ToString) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let arg = arg.to_string();
    move |_, window, cx| act(a, &arg, window, cx)
}

fn level_color(pct: f64) -> Hsla { match rules::level(pct) { 2 => T::red(), 1 => T::yellow(), _ => T::green() } }
fn level_dim(pct: f64) -> Hsla { match rules::level(pct) { 2 => T::red_dim(), 1 => T::yellow_dim(), _ => T::green_dim() } }

/// A section card (.info-section): its title (muted, upper case), then the children.
fn section(title: &str) -> Div {
    div().flex().flex_col().flex_none().gap(T::SP_10).p(T::SP_16).bg(T::bg_secondary()).rounded(T::R_CONTROL).border_1().border_color(T::border())
        .when(!title.is_empty(), |d| d.child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child(title.to_string())))
}

/// A two-sided row (.info-row): the label left, the value right (mono), a hairline under it.
fn kv(label: impl Into<SharedString>, value: impl Into<SharedString>, rule: bool) -> Div {
    div().flex().items_center().gap(T::SP_8).py(T::SP_6).when(rule, |d| d.border_b_1().border_color(T::border_subtle()))
        .child(div().flex_none().text_size(T::TEXT_13).text_color(T::text_secondary()).child(label.into()))
        .child(div().flex_1().min_w_0())
        .child(div().max_w(relative(T::INFO_VALUE_SHARE)).text_right().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text()).child(value.into()))
}

fn quiet(t: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_13).text_color(T::text_muted()).child(t.into()) }
fn hint(t: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_11).text_color(T::text_muted()).child(t.into()) }
fn label(t: impl Into<SharedString>) -> Div { div().pt(T::SP_4).text_size(T::TEXT_12).text_color(T::text_secondary()).child(t.into()) }
fn note(warn: bool) -> Div {
    div().flex().flex_col().gap(T::SP_4).p(T::SP_8).rounded(T::R_CONTROL).border_1()
        .border_color(if warn { T::info_yellow_edge() } else { T::border() }).when(warn, |d| d.bg(T::yellow_dim()))
}

impl InfoView {
    fn sys_panel(&self) -> Div {
        let Some(sys) = &self.sys else { return div().child(quiet("Reading the machine…")) };
        let mut d = div().flex().flex_col().gap(T::SP_6).child(kv("CPU", sys.cpu.clone(), false));
        if !sys.cores.is_empty() {
            d = d.child(div().flex().h(T::SP_24 + T::SP_4).gap(T::SP_2).children(sys.cores.iter().map(|c| {
                let c = c.clamp(0.0, 100.0);
                div().flex_1().min_w_0().h_full().flex().flex_col().justify_end().rounded(T::SP_2).bg(T::bg_tertiary()).overflow_hidden()
                    .child(div().w_full().h(relative((c / 100.0) as f32)).bg(level_color(c)))
            })));
        }
        // The CPU history (sparkSvg): the area faint, the line over it.
        let pts = sys.hist.clone();
        let (fill, line) = (level_dim(sys.overall), level_color(sys.overall));
        d = d.child(div().w_full().h(T::SP_40).child(canvas(|_, _, _| (), move |b, _, window, _| {
            if pts.len() < 2 { return; }
            let at = |(x, y): (f32, f32)| point(b.origin.x + b.size.width * (x / 100.), b.origin.y + b.size.height * (y / 20.));
            let mut area = PathBuilder::fill();
            area.move_to(at(pts[0]));
            for p in &pts[1..] { area.line_to(at(*p)); }
            area.line_to(at((100., 20.)));
            area.line_to(at((0., 20.)));
            area.close();
            if let Ok(p) = area.build() { window.paint_path(p, fill); }
            let mut stroke = PathBuilder::stroke(px(1.));
            stroke.move_to(at(pts[0]));
            for p in &pts[1..] { stroke.line_to(at(*p)); }
            if let Ok(p) = stroke.build() { window.paint_path(p, line); }
        }).size_full()));
        for r in &sys.rows {
            d = d.child(match r {
                SysRow::Gap => div().h(T::SP_8),
                SysRow::Line(k, v) => kv(k.clone(), v.clone(), false),
                SysRow::Bar(p) => w::meter((*p / 100.0) as f32, level_color(*p), T::SP_6),
            });
        }
        d
    }

    fn maint_panel(&self, cx: &mut Context<Self>) -> Div {
        match self.maint_state.as_str() {
            "loading" => return div().child(quiet("Measuring…")),
            "error" => return div().child(quiet("Could not read reclaimable space.")),
            "empty" => return div().child(quiet("Nothing on this machine to reclaim.")),
            _ => {}
        }
        let (head, rows) = rules::maint_rows(&self.maint, &self.maint_busy);
        let mut d = div().flex().flex_col().child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(head));
        for (i, t) in rows.into_iter().enumerate() {
            let id = t.id.clone();
            let armed = self.armed == t.id;
            let btn = if !t.busy.is_empty() { w::disabled(w::button(("maint", i), t.busy.clone()), true) }
                else if armed { w::danger(("maint", i), t.arm.clone()) }
                else if t.danger { w::disabled(w::danger(("maint", i), "Remove").icon(IconName::Trash), !t.enabled) }
                else { w::disabled(w::button(("maint", i), "Clear").icon(IconName::Trash), !t.enabled) };
            d = d.child(div().flex().items_center().gap(T::SP_12).py(T::SP_10).when(i > 0, |d| d.border_t_1().border_color(T::border_subtle()))
                .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
                    .child(div().flex().items_center().gap(T::SP_6)
                        .child(div().text_size(T::TEXT_13).text_color(T::text()).child(t.label.clone()))
                        .when(t.root, |d| d.child(w::tag("root", "warn")))
                        .when(t.danger, |d| d.child(w::tag("destructive", "error"))))
                    .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child(t.desc.clone()))
                    .when(!t.detail.is_empty(), |d| d.child(div().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::yellow()).child(t.detail.clone()))))
                .child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_13).text_color(if t.none { T::text_muted() } else { T::text() }).child(t.size.clone()))
                .child(btn.on_click(cx.listener(move |_, _, window, cx| act("maint-run", &id, window, cx)))));
        }
        d
    }

    fn collab_panel(&self, cx: &mut Context<Self>) -> Div {
        match self.collab_state.as_str() {
            "loading" => return div().child(quiet("Loading…")),
            "error" => return div().child(quiet("Could not load collaborators.")),
            _ => {}
        }
        let mut d = div().flex().flex_col().gap(T::SP_8);
        // The sandbox (.collab-sb).
        d = d.child(if self.sb_ok {
            note(false).child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Sandbox ready. Collaborators\u{2019} shells are confined to their own worktree."))
        } else {
            note(true)
                .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Sandbox not set up. Invites are disabled and collaborator chats will not start until it is."))
                .children(self.sb_problems.iter().map(|p| div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(format!("•  {p}"))))
                .child(if self.sb_fix { div().flex().child(w::button("sb-setup", "Set up sandbox").on_click(on("collab-sandbox-setup", String::new()))) }
                    else { div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("On the server run: sudo apt-get install -y bubblewrap socat") })
        });
        if self.people.is_empty() { d = d.child(quiet("Nobody outside has access right now.")); }
        for (pi, p) in self.people.iter().enumerate() {
            let mut card = note(false)
                .child(div().flex().items_center().gap(T::SP_8)
                    .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(p.name.clone()))
                    .child(div().flex_1().min_w_0().text_size(T::TEXT_11).text_color(T::text_muted()).child(p.tg.clone()))
                    .child(w::button(("reissue", pi), "New passcode").on_click(on("collab-reissue", p.id.clone()))))
                .child(div().flex().items_center().gap(T::SP_6)
                    .child(div().text_size(T::TEXT_11).text_color(if p.over { T::red() } else { T::text_muted() }).child(p.usage.clone()))
                    .children(p.meter.map(|m| div().w(T::INFO_METER_W).child(w::meter(m, if p.over { T::red() } else { T::accent() }, T::SP_4))))
                    .child(w::button(("limit", pi), p.limit_btn.clone()).on_click(on("collab-limit", p.id.clone())))
                    .when(p.can_reset, |d| d.child(w::button(("reset", pi), "Reset").on_click(on("collab-reset-usage", p.id.clone())))));
            if !p.sites.is_empty() {
                card = card.child(div().flex().flex_wrap().items_center().gap(T::SP_4)
                    .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child("Allowed sites:"))
                    .children(p.sites.iter().enumerate().map(|(si, s)| div().flex().items_center().gap(T::SP_2).pl(T::SP_6).rounded(T::R_CHIP).bg(T::bg_tertiary())
                        .child(div().text_size(T::TEXT_11).text_color(T::text_secondary()).child(s.clone()))
                        .child(w::icon_button(("site-x", pi * 1000 + si), IconName::X, "Remove").on_click(on("collab-domain-remove", format!("{}\n{s}", p.id)))))));
            }
            for (ri, r) in p.rows.iter().enumerate() {
                let k = pi * 1000 + ri;
                card = card.child(div().flex().items_center().gap(T::SP_8).when(r.gone, |d| d.opacity(T::INFO_O_SOFT))
                    .child(div().min_w(T::SP_48 + T::SP_40).text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(r.project.clone()))
                    .child(div().min_w_0().truncate().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::sky()).child(r.branch.clone()))
                    .child(div().flex_1().min_w_0().text_right().text_size(T::TEXT_12).text_color(T::text_muted()).child(r.exp.clone()))
                    .child(w::button(("extend", k), "Extend").on_click(on("collab-extend", r.id.clone())))
                    .child(w::button(("merge", k), "Merge").on_click(on("collab-merge-owner", r.id.clone())))
                    .child(if r.revoked { w::button(("restore", k), "Restore").on_click(on("collab-unrevoke", r.id.clone())) }
                        else { w::button(("revoke", k), "Revoke").on_click(on("collab-revoke", r.id.clone())) })
                    .child(w::danger(("remove", k), "Remove").on_click(on("collab-remove", r.id.clone()))));
            }
            d = d.child(card);
        }
        // Invite someone (.collab-invite).
        let me = cx.entity().downgrade();
        let mut inv = div().flex().flex_col().gap(T::SP_4).pt(T::SP_8).border_t_1().border_color(T::border())
            .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child("Invite someone"));
        if let Some(i) = &self.inputs {
            let mut ticks = div().flex().flex_col().gap(T::SP_4).p(T::SP_6).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg_tertiary());
            for (k, (alias, lbl)) in invite_projects(cx).into_iter().enumerate() {
                let on = self.picked.contains(&alias);
                ticks = ticks.child(w::check(("inv-proj", k), lbl, on, move |_, window, cx| act("inv-project", &alias, window, cx)));
            }
            inv = inv
                .child(label("Their name")).child(w::input(&i.name)).child(hint("They type this to sign in."))
                .child(label("Telegram username (optional)")).child(w::input(&i.tg)).child(hint("Lets them use the Telegram button instead of a passcode."))
                .child(label("Projects (pick one or more)")).child(ticks)
                .child(hint("Tick several for several. Each gets its own git worktree and branch."))
                .child(label("Access lasts"))
                .child(div().flex().items_center().gap(T::SP_6)
                    .child(div().w(T::SP_48 + T::SP_32).child(w::input(&i.amount)))
                    .child(w::dropdown("inv-unit", &["days", "hours"], self.inv_unit, false, move |u, _, cx| { let _ = me.update(cx, |s, cx| { s.inv_unit = u; cx.notify(); }); })))
                .child(hint(self.expiry()))
                .child(label("Token limit (optional)")).child(w::input(&i.limit))
                .child(hint("Counted across all their projects. Their chat stops the moment it is reached, and you can change it later."))
                .child(div().flex().pt(T::SP_4).child(w::disabled(w::button("collab-create", "Create access"), !self.sb_ok).on_click(on("collab-create", String::new()))))
                .child(quiet("They see only the projects you pick, and nothing else on this machine."));
        }
        d.child(inv)
    }

    fn log_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().id("server-logs-area").h(T::INFO_LOGS_H).flex_none().flex().flex_col().p(T::SP_2).bg(T::bg()).rounded(T::R_CONTROL).border_1().border_color(T::border())
            .when(self.logs_loading, |d| d.child(div().p(T::SP_6).font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child("(loading...)")))
            .child(list(self.log_list.clone(), cx.processor(|v: &mut InfoView, i: usize, _, _| {
                let Some((t, l)) = v.logs.get(i) else { return div().into_any_element() };
                div().px(T::SP_6).font_family(T::MONO).text_size(T::TEXT_11)
                    .text_color(match l.as_str() { "error" => T::red(), "warn" => T::yellow(), _ => T::text_muted() }).child(t.clone()).into_any_element()
            })).flex_1().min_h_0())
    }

    fn dialogs(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (close, body): (&'static str, Vec<AnyElement>) = if let Some(c) = &self.cred {
            let field = |lbl: &str, v: &str, id: &'static str, big: bool, _cx: &mut Context<Self>| div().flex().flex_col().gap(T::SP_4).child(label(lbl.to_string()))
                .child(div().flex().items_center().gap(T::SP_6)
                    .child(div().flex_1().min_w_0().px(T::SP_8).py(T::SP_6).rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg())
                        .text_size(T::TEXT_13).text_color(T::text()).when(big, |d| d.font_family(T::MONO)).child(v.to_string()))
                    .child(if self.copied == id { w::primary(ElementId::Name(format!("cred-copy-{id}").into()), "Copied") } else { w::button(ElementId::Name(format!("cred-copy-{id}").into()), "Copy") }.on_click(on("cred-copy", id))));
            ("cred-close", vec![
                w::modal_head("cred-x", c.title.clone(), false, cx.listener(|_, _, window, cx| act("cred-close", "", window, cx))).into_any_element(),
                div().flex().flex_col().gap(T::SP_10).p(T::SP_8)
                    .child(hint("Send these three things to them however you like."))
                    .child(field("Sign in at", &c.url, "url", false, cx))
                    .child(field("Name", &c.name, "name", false, cx))
                    .child(field("Passcode", &c.pass, "pass", true, cx))
                    .child(note(true).child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("The passcode is shown only here. It is stored hashed, so it cannot be looked up later — only replaced with a new one.")))
                    .when(!c.projects.is_empty(), |d| d.child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(format!("Projects: {}", c.projects))))
                    .when(!c.ends.is_empty(), |d| d.child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(format!("Access ends: {}", c.ends))))
                    .child(div().flex().justify_end().gap(T::SP_8)
                        .child(if self.copied == "all" { w::primary("cred-all", "Copied") } else { w::button("cred-all", "Copy everything") }.on_click(on("cred-copy", "all")))
                        .child(w::primary("cred-done", "Done").on_click(on("cred-close", ""))))
                    .into_any_element(),
            ])
        } else if let Some(g) = &self.git {
            ("git-cancel", vec![
                w::modal_head("git-x", g.title.clone(), false, cx.listener(|_, _, window, cx| act("git-cancel", "", window, cx))).into_any_element(),
                div().flex().flex_col().gap(T::SP_10).p(T::SP_8)
                    .child(hint("Sharing gives each person their own branch, so it needs one. Setting it up makes a first commit of what is in the folder now."))
                    .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(g.commit.clone()))
                    .when(!g.kept.is_empty(), |d| d.child(div().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_secondary()).child(g.kept.clone())))
                    .when(!g.risky.is_empty(), |d| d.child(note(true).child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(g.risky.clone()))))
                    .child(hint(g.tail.clone()))
                    .child(div().flex().justify_end().gap(T::SP_8)
                        .child(w::button("git-cancel", "Cancel").on_click(on("git-cancel", "")))
                        .child(w::disabled(w::primary("git-go", if g.busy { "Setting up…" } else { "Set up git and share" }), g.busy).on_click(on("git-go", ""))))
                    .into_any_element(),
            ])
        } else { return None };
        let m = w::Modal {
            id: "info-dialog", width: T::DIALOG_W_LG, focus: self.focus.clone(), scroll: self.dlg_scroll.clone(),
            on_close: Rc::new(move |window, cx| act(close, "", window, cx)),
            on_key: Rc::new(move |e, window, cx| if e.keystroke.key == "escape" { act(close, "", window, cx) }),
        };
        Some(w::overlay_modal(m, window, body).into_any_element())
    }
}

impl InfoView {
    /// The page's rows, as the page shows them now.
    fn kinds(&self) -> Vec<&'static str> {
        let mut k = vec![];
        if !self.online { k.push("offline"); }
        if self.facts.is_some() { k.push("project"); k.push("status"); }
        k.extend(["machine", "reclaim", "collab"]);
        if self.tunnel.is_some() { k.push("tunnel"); }
        k.push("logs");
        k
    }

    /// After a change: the rows again (a row more or less: the list anew),
    /// else every row measured again where it is (the scroll place kept).
    fn relist(&mut self) {
        let k = self.kinds();
        if k != self.page_kinds { self.page_list.reset(k.len()); self.page_kinds = k; } else { self.page_list.remeasure(); }
    }

    /// One row of the page (and the page's gap under it, but the last).
    fn item(&mut self, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(kind) = self.page_kinds.get(i).copied() else { return div().into_any_element() };
        let el: AnyElement = match kind {
            // A down server: say so over the last figures, with Retry (F66).
            "offline" => w::error_state("info-retry", "Not connected to the server: these figures may be out of date.", |_, _, cx| { render_all(cx); with(cx, |_, _| {}); }).into_any_element(),
            "project" => { let (alias, name, path, _, _) = self.facts.clone().unwrap_or_default(); section("PROJECT").gap(px(0.)).child(kv("Alias", alias, true)).child(kv("Name", name, true)).child(kv("Path", path, false)).into_any_element() }
            "status" => { let (_, _, _, term, svc) = self.facts.clone().unwrap_or_default(); section("STATUS").gap(px(0.)).child(kv("Claude Terminal", term, true)).child(kv("Services", svc, false)).into_any_element() }
            "machine" => section("THIS MACHINE").child(self.sys_panel()).into_any_element(),
            "reclaim" => section("RECLAIM DISK SPACE").child(self.maint_panel(cx)).into_any_element(),
            "collab" => section("OUTSIDE COLLABORATORS").child(self.collab_panel(cx)).into_any_element(),
            "tunnel" => {
                let Some(t) = &self.tunnel else { return div().into_any_element() };
                let url = t.url.clone();
                let link = url != "—";
                section("TUNNEL").gap(px(0.))
                    .child(div().flex().items_center().gap(T::SP_8).py(T::SP_6).border_b_1().border_color(T::border_subtle())
                        .child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child("URL"))
                        .child(div().flex_1().min_w_0())
                        .child(div().id("tunnel-url").max_w(relative(T::INFO_VALUE_SHARE)).font_family(T::MONO).text_size(T::TEXT_12)
                            .text_color(if link { T::accent() } else { T::text() })
                            .when(link, |d| d.cursor_pointer().hover(|d| d.text_color(T::accent_hover())))
                            .child(url.clone())
                            .on_click(move |_, window, cx| if link { act("open-url", &url, window, cx) })))
                    .child(kv("Status", t.status.clone(), !t.cf.is_empty()))
                    .when(!t.cf.is_empty(), |d| d.child(kv("Cloudflare service", t.cf.clone(), false))).into_any_element()
            }
            _ => section("")
                .child(div().flex().items_center().gap(T::SP_8)
                    .child(div().flex_1().min_w_0().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child("SERVER LOGS"))
                    .child(w::button("copy-logs", "Copy").icon(IconName::Copy).on_click(cx.listener(|_, _, window, cx| act("copy-logs", "", window, cx)))))
                .child(self.log_box(cx)).into_any_element(),
        };
        div().w_full().when(i + 1 < self.page_kinds.len(), |d| d.pb(T::SP_12)).child(el).into_any_element()
    }
}

impl Render for InfoView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("info");
        if self.page_kinds.is_empty() { self.relist(); }
        // The page as a virtual list of its sections: a wheel frame builds
        // only the sections on screen.
        let page = list(self.page_list.clone(), cx.processor(|v: &mut InfoView, i: usize, _, cx| v.item(i, cx)))
            .size_full().p(T::SP_16).bg(T::bg());
        let dlg = self.dialogs(window, cx);
        div().size_full().flex().flex_col().min_h_0().child(page).children(dlg)
    }
}

// ─── The driver ───

/// A driver action (Some(answer) for a name it knows).
pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "info-act" => { act(&a(0), &a(1), window, cx); Some(json!({ "ok": true })) }
        "info-log" => {
            let m = json!({ "time": rules::now_ms(), "level": if a(1).is_empty() { "info".into() } else { a(1) }, "text": a(0) });
            with(cx, |s, _| s.append_log(&m));
            Some(json!({ "ok": true }))
        }
        "info-field" => {
            let e = ent(cx);
            e.update(cx, |v, cx| v.ensure_inputs(window, cx));
            let (field, val) = (a(0), a(1));
            let ok = e.update(cx, |s, cx| {
                let inp = s.inputs.as_ref().map(|i| match field.as_str() { "name" => Some(i.name.clone()), "tg" => Some(i.tg.clone()), "amount" => Some(i.amount.clone()), "limit" => Some(i.limit.clone()), _ => None });
                match field.as_str() {
                    "name" => s.inv_name = val.clone(), "tg" => s.inv_tg = val.clone(), "amount" => s.inv_amount = val.clone(), "limit" => s.inv_limit = val.clone(),
                    "unit" => s.inv_unit = val.parse().unwrap_or(0),
                    _ => return false,
                }
                if let Some(Some(i)) = inp { i.update(cx, |st, cx| st.set_value(val.clone(), window, cx)); }
                cx.notify();
                true
            });
            Some(if ok { json!({ "ok": true }) } else { json!({ "ok": false, "error": "unknown field" }) })
        }
        _ => None,
    }
}

/// The driver's view of this screen (the Slint client's names).
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    if !cx.has_global::<Handle>() { return json!({ "shown": false, "polling": false, "project": null, "tunnel": null, "logs": 0, "logsLoading": true }); }
    let e = ent(cx);
    let s = e.read(cx);
    let project = project_facts(&s.services, cx).map(|(alias, name, path, terminal, services)| json!({ "alias": alias, "name": name, "path": path, "terminal": terminal, "services": services })).unwrap_or(Value::Null);
    let rows: Vec<Value> = s.sys.as_ref().map(|x| x.rows.iter().filter_map(|r| if let SysRow::Line(k, v) = r { Some(json!([k, v])) } else { None }).collect()).unwrap_or_default();
    let (total, tasks) = rules::maint_rows(&s.maint, &s.maint_busy);
    let projects: Vec<Value> = invite_projects(cx).into_iter().map(|(k, _)| json!({ "key": k, "on": s.picked.contains(&k) })).collect();
    json!({
        "shown": s.shown, "polling": s.poll.is_some(),
        "project": project,
        "sys": { "ready": s.sys.is_some(), "cpu": s.sys.as_ref().map(|x| x.cpu.clone()).unwrap_or_default(), "cores": s.sys.as_ref().map(|x| x.cores.len()).unwrap_or(0),
                 "spark": s.sys.as_ref().is_some_and(|x| x.hist.len() >= 2), "rows": rows },
        "maint": { "state": s.maint_state, "total": if s.maint_state == "data" { total } else { String::new() },
                   "tasks": tasks.iter().map(|t| json!({ "id": t.id, "label": t.label, "size": t.size, "enabled": t.enabled, "armed": s.armed == t.id })).collect::<Vec<_>>() },
        "collab": { "state": s.collab_state, "sandboxOk": s.sb_ok, "people": s.people.len(), "expiry": s.expiry(), "projects": projects },
        "tunnel": s.tunnel.as_ref().map(|t| json!({ "url": t.url, "status": t.status, "cf": t.cf })).unwrap_or(Value::Null),
        "logs": s.logs.len(), "lastLog": s.logs.last().map(|x| x.0.clone()).unwrap_or_default(), "logsLoading": s.logs_loading,
        "cred": s.cred.is_some(), "git": s.git.is_some(), "copied": s.copied, "modal": s.cred.is_some() || s.git.is_some(),
    })
}

#[cfg(test)]
mod gpui_tests {
    use super::{ent, LOG_KEEP};
    use serde_json::json;

    #[gpui::test]
    fn the_server_log_is_a_bounded_virtual_list(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let e = ent(cx);
            e.update(cx, |v, _| {
                for i in 0..LOG_KEEP + 25 { v.append_log(&json!({ "text": format!("line {i}"), "level": "info" })); }
                assert_eq!(v.logs.len(), LOG_KEEP);
                assert_eq!(v.log_list.item_count(), LOG_KEEP, "the list follows the lines, spliced");
                assert!(v.logs.last().is_some_and(|l| l.0.ends_with(&format!("line {}", LOG_KEEP + 24))));
            });
        });
    }
}
