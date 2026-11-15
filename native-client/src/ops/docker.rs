//! Ops area: Docker (app/vendor/crundi-docker.js; loadDockerTab and
//! checkDockerPresence in src/webapp-html.js): the status gate, containers
//! (summary, bay plan, rows, actions), a container's detail (overview with
//! its ports' forwards, live logs over /ws, stats charts, a shell over /ws,
//! inspect; rename and remove), Compose stacks, images (pull, clean),
//! volumes, networks, Crundi's forwards matched to containers, the engine
//! and its disk use, and the job drawer for compose and pull output.
//! UI: ui/ops/docker.slint (global OpsDocker). Pure rules: docker/fmt.rs.

mod fmt;

use crate::{
    areas::AreaEvent, kit, MainWindow, OpsDkAct, OpsDkBay, OpsDkChart, OpsDkChip, OpsDkCtr, OpsDkDisk, OpsDkFwd, OpsDkGroup,
    OpsDkKv, OpsDkLog, OpsDkPort, OpsDkRow, OpsDkSeg, OpsDkStack, OpsDkStat, OpsDkSvc, OpsDocker,
};
use fmt::{bytes, num, pct};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, StyledText, Timer, TimerMode, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

const VIEWS: [(&str, &str); 7] = [
    ("containers", "Containers"), ("stacks", "Stacks"), ("images", "Images"), ("volumes", "Volumes"),
    ("networks", "Networks"), ("forwards", "Forwards"), ("system", "System"),
];
const MAX_LINES: usize = 5000;

#[derive(Clone, Default)]
struct LogRec { ts: String, text: String, err: bool, sys: bool }

#[derive(Default)]
struct Detail {
    id: String,
    tab: String,
    loaded: bool,
    error: String,
    /// The detail answer: container, overview, inspect.
    data: Value,
    lines: Vec<LogRec>,
    partial: [String; 2],
    log_ref: Option<String>,
    shell_ref: Option<String>,
    shell_msg: Option<String>,
    screen: fmt::Screen,
    samples: Vec<Value>,
    stats_msg: Option<String>,
    prompt: String,
}

#[derive(Default)]
struct Job { id: String, title: String, text: String, next: i64, done: bool, code: i64 }

#[derive(Default)]
struct St {
    shown: bool,
    connected: bool,
    status: Option<Value>,
    view: String,
    filter: HashMap<String, String>,
    data: HashMap<String, Value>,
    /// The last load's error for the view (title, words).
    error: Option<(String, String)>,
    disk: Option<Value>,
    disk_loading: bool,
    fwd_list: Option<Vec<Value>>,
    fwd_at: f64,
    fwd_opts: Option<Value>,
    /// The forward form: under which port ("any": the Forwards view's), the modes' values.
    form: Option<String>,
    form_values: Vec<String>,
    detail: Option<Detail>,
    job: Option<Job>,
    loading: bool,
    refs: u64,
    last_url: String,
    copied: String,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St { view: "containers".into(), connected: true, ..Default::default() });
    static TIMER: Timer = Timer::default();
    static STATS_TIMER: Timer = Timer::default();
    static SEGS: Rc<VecModel<OpsDkSeg>> = kit::model();
    static STATS: Rc<VecModel<OpsDkStat>> = kit::model();
    static BAY: Rc<VecModel<OpsDkBay>> = kit::model();
    static CTRS: Rc<VecModel<OpsDkCtr>> = kit::model();
    static STACKS: Rc<VecModel<OpsDkStack>> = kit::model();
    static ROWS: Rc<VecModel<OpsDkRow>> = kit::model();
    static GROUPS: Rc<VecModel<OpsDkGroup>> = kit::model();
    static ENGINE: Rc<VecModel<OpsDkKv>> = kit::model();
    static CONTENTS: Rc<VecModel<OpsDkKv>> = kit::model();
    static WARNINGS: Rc<VecModel<SharedString>> = kit::model();
    static DISK: Rc<VecModel<OpsDkDisk>> = kit::model();
    static ACTS: Rc<VecModel<OpsDkAct>> = kit::model();
    static OV: [Rc<VecModel<OpsDkKv>>; 6] = std::array::from_fn(|_| kit::model());
    static PORTS: Rc<VecModel<OpsDkPort>> = kit::model();
    static LOGS: Rc<VecModel<OpsDkLog>> = kit::model();
    static CHARTS: Rc<VecModel<OpsDkChart>> = kit::model();
    static MODES: Rc<VecModel<SharedString>> = kit::model();
}

fn st<R>(f: impl FnOnce(&mut St) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

fn g(w: &MainWindow) -> OpsDocker<'_> {
    w.global::<OpsDocker>()
}

fn with_g(f: impl FnOnce(&OpsDocker)) {
    if let Some(w) = kit::ui() { f(&g(&w)); }
}

fn ss(v: &Value, k: &str) -> String { kit::s(v, k) }
fn arr<'a>(v: &'a Value, k: &str) -> Vec<&'a Value> { v.get(k).and_then(|x| x.as_array()).map(|a| a.iter().collect()).unwrap_or_default() }
fn strs(v: &Value, k: &str) -> Vec<String> { arr(v, k).into_iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect() }
fn vm<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> { ModelRc::new(VecModel::from(v)) }
fn chip(text: &str, tone: &str, act: &str, target: &str) -> OpsDkChip {
    OpsDkChip { text: text.into(), tone: tone.into(), act: act.into(), target: target.into(), tip: SharedString::default() }
}
fn kv(k: &str, v: &str) -> OpsDkKv { OpsDkKv { k: k.into(), v: v.into(), hid: false } }

/// A server answer: `{ok:false, error}` and HTTP errors as the server's words.
fn answer(r: Result<Value, String>) -> Result<Value, String> {
    match r {
        Ok(v) if v.get("ok") == Some(&Value::Bool(false)) => Err(v.get("error").and_then(|e| e.as_str()).unwrap_or("").to_string()),
        other => other,
    }
}

fn get(path: String, done: impl FnOnce(Result<Value, String>) + Send + 'static) {
    kit::call(move |api| api.get(&path), move |r| done(answer(r)));
}

fn post(path: &'static str, body: Value, done: impl FnOnce(Result<Value, String>) + Send + 'static) {
    kit::call(move |api| api.post(path, &body), move |r| done(answer(r)));
}

/// The server's words, or `d` when it gave none.
fn eo(e: &str, d: &str) -> String { if e.is_empty() { d.to_string() } else { e.to_string() } }

fn copy(text: &str) {
    if text.is_empty() { return; }
    st(|s| s.copied = text.to_string());
    let done = arboard::Clipboard::new().ok().and_then(|mut cb| cb.set_text(text.to_string()).ok().map(|_| cb));
    match done {
        Some(cb) => { crate::app::keep_clipboard(cb); kit::toast("Copied"); }
        None => kit::toast_error("Could not copy"),
    }

}

fn open_url(url: &str) {
    if url.is_empty() { return; }
    let full = if url.starts_with('/') { format!("{}{}", kit::base(), url) } else { url.to_string() };
    st(|s| s.last_url = full.clone());
    if std::env::var_os("CRUNDI_TEST_PORT").is_none() {
        std::thread::spawn(move || { let _ = open::that(&full); });
    }
}

// ─── The registry ───

pub fn init(w: &MainWindow) {
    let o = g(w);
    o.set_segs(SEGS.with(kit::rc));
    o.set_stats(STATS.with(kit::rc));
    o.set_bay(BAY.with(kit::rc));
    o.set_ctrs(CTRS.with(kit::rc));
    o.set_stacks(STACKS.with(kit::rc));
    o.set_rows(ROWS.with(kit::rc));
    o.set_groups(GROUPS.with(kit::rc));
    o.set_engine(ENGINE.with(kit::rc));
    o.set_contents(CONTENTS.with(kit::rc));
    o.set_warnings(WARNINGS.with(kit::rc));
    o.set_disk(DISK.with(kit::rc));
    o.set_d_acts(ACTS.with(kit::rc));
    OV.with(|ov| {
        o.set_ov_state(kit::rc(&ov[0]));
        o.set_ov_runs(kit::rc(&ov[1]));
        o.set_ov_nets(kit::rc(&ov[2]));
        o.set_ov_mounts(kit::rc(&ov[3]));
        o.set_ov_env(kit::rc(&ov[4]));
        o.set_ov_labels(kit::rc(&ov[5]));
    });
    o.set_ov_ports(PORTS.with(kit::rc));
    o.set_logs(LOGS.with(kit::rc));
    o.set_charts(CHARTS.with(kit::rc));
    o.set_form_modes(MODES.with(kit::rc));
    o.set_chart_w(fmt::CHART_W as f32);
    o.set_chart_h(fmt::CHART_H as f32);

    o.on_set_view(|v| set_view(v.as_str()));
    o.on_filter_edited(|t| { st(|s| { let v = s.view.clone(); s.filter.insert(v, t.to_string()); }); render(); });
    o.on_refresh(refresh);
    o.on_recheck(recheck);
    o.on_open(|id| open_detail(id.as_str()));
    o.on_back(|| { let v = st(|s| s.view.clone()); set_view(&v); });
    o.on_dtab(|t| select_tab(t.as_str()));
    o.on_ctr(|id, verb| container_verb(id.as_str(), verb.as_str()));
    o.on_prompt(|k| open_prompt(k.as_str()));
    o.on_rename_do(rename_do);
    o.on_remove_do(remove_do);
    o.on_log_clear(|| { st(|s| if let Some(d) = s.detail.as_mut() { d.lines.clear(); }); rebuild_logs(); });
    o.on_log_copy(log_copy);
    o.on_log_filter_edited(|_| log_filter_soon());
    o.on_log_option(|k, on| log_option(k.as_str(), on));
    o.on_inspect_copy(|| { let t = inspect_text(); copy(&t); });
    o.on_shell_again(open_shell);
    o.on_shell_send(|t| shell_input(&format!("{t}\r")));
    o.on_shell_key(|k| shell_key(k.as_str()));
    o.on_stack(|n, a| stack_action(n.as_str(), a.as_str()));
    o.on_pull(pull);
    o.on_prune(|k| prune(k.as_str()));
    o.on_rm(|kind, key| remove_thing(kind.as_str(), key.as_str()));
    o.on_chip(|c| chip_act(&c));
    o.on_fwd_add(|p| open_form(p.as_str()));
    o.on_fwd_create(create_forward);
    o.on_fwd_cancel(|| { st(|s| s.form = None); render(); });
    o.on_fwd_watch(|h, on| { let h = h.to_string(); set_forward_watch(&h, on, false, move |_| { st(|s| s.fwd_at = 0.0); load(true); }); });
    o.on_rm_fwd(|h| { let h = h.to_string(); remove_forward(&h.clone(), false, move |ok| { if ok { kit::toast_ok(&format!("Removed {h}")); } st(|s| s.fwd_at = 0.0); load(true); }); });
    o.on_rm_orphans(remove_orphans);
    o.on_job_close(|| { st(|s| s.job = None); render_job(); });
    render();
}

pub fn signed_in() {}

pub fn signed_out() {
    hide();
    st(|s| { *s = St { view: "containers".into(), connected: true, ..Default::default() }; });
    render();
    render_job();
}

pub fn project(_alias: &str) {}

pub fn tab_shown(tab: &str) {
    let was = st(|s| s.shown);
    if tab == "docker" && !was { show(); } else if tab != "docker" && was { hide(); }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Connected(on) => {
            st(|s| s.connected = *on);
            if *on { on_reconnect(); }
        }
        AreaEvent::Ws(t, m) if t.starts_with("docker-") => on_message(t, m),
        _ => {}
    }
}

// ─── Showing, the gate, loading ───

fn show() {
    st(|s| s.shown = true);
    get("/api/docker/status".into(), |r| {
        let s = r.unwrap_or_else(|e| json!({ "installed": true, "reachable": false, "error": e }));
        st(|x| x.status = Some(s.clone()));
        render();
        if !gate_ok() { return; }
        let detail = st(|s| s.detail.as_ref().map(|d| d.tab.clone()));
        if let Some(tab) = detail {
            select_tab(&tab);
            refresh_head(true);
        } else {
            load(true);
        }
        schedule();
    });
}

fn hide() {
    st(|s| s.shown = false);
    TIMER.with(|t| t.stop());
    STATS_TIMER.with(|t| t.stop());
    // Logs stop following while nobody looks; a shell is left alone, since
    // whatever runs in it would die with it.
    stop_logs();
}

fn gate_ok() -> bool {
    st(|s| s.status.as_ref().is_some_and(|x| kit::truthy(x.get("installed").unwrap_or(&Value::Null)) && kit::truthy(x.get("reachable").unwrap_or(&Value::Null))))
}

fn recheck() {
    get("/api/docker/status?force=1".into(), |_| show());
}

fn refresh() {
    if st(|s| s.detail.is_some()) { refresh_head(true); } else { load(false); }
}

fn set_view(v: &str) {
    close_detail();
    st(|s| { s.view = v.to_string(); s.error = None; s.form = None; s.data.remove(v); });
    render();
    load(true);
}

fn path_of(v: &str) -> &'static str {
    match v {
        "containers" => "/api/docker/containers?stats=1",
        "stacks" => "/api/docker/stacks",
        "images" => "/api/docker/images",
        "volumes" => "/api/docker/volumes",
        "networks" => "/api/docker/networks",
        "system" => "/api/docker/system",
        _ => "/api/docker/forwards",
    }
}

/// A poll skips its turn while a request is out; a first load never waits.
fn load(first: bool) {
    let (go, detail, v) = st(|s| {
        if !s.shown || (s.loading && !first) { return (false, false, String::new()); }
        s.loading = true;
        (true, s.detail.is_some(), s.view.clone())
    });
    if !go { return; }
    if detail { st(|s| s.loading = false); refresh_head(false); return; }
    let vv = v.clone();
    get(path_of(&v).into(), move |r| {
        st(|s| s.loading = false);
        if st(|s| s.view != vv || s.detail.is_some()) { return; }
        match r {
            Err(e) => st(|s| s.error = Some(("Docker did not answer".into(), e))),
            Ok(d) => {
                let key = if vv == "system" { "engine" } else { vv.as_str() };
                st(|s| {
                    s.error = None;
                    s.data.insert(vv.clone(), d.get(key).cloned().unwrap_or(d.clone()));
                });
                if vv == "system" { load_disk(); }
                if vv == "forwards" { take_forwards(arr(&d, "forwards").into_iter().cloned().collect()); }
                if vv == "containers" {
                    load_forwards(false);
                    // The Stacks count follows from the container labels.
                    let mut seen: Vec<String> = arr(&d, "containers").iter().map(|c| ss(c, "project")).filter(|p| !p.is_empty()).collect();
                    seen.sort(); seen.dedup();
                    st(|s| {
                        let n = s.data.get("stacks").and_then(|x| x.as_array()).map(|a| a.len());
                        if n != Some(seen.len()) { s.data.insert("stacks".into(), Value::Array(seen.iter().map(|k| json!({ "name": k })).collect())); }
                    });
                }
            }
        }
        render();
    });
}

fn schedule() {
    let (shown, detail, v) = st(|s| (s.shown, s.detail.is_some(), s.view.clone()));
    TIMER.with(|t| t.stop());
    if !shown { return; }
    let every = if detail { 4000 } else { match v.as_str() { "containers" => 3000, "stacks" => 5000, "forwards" => 10000, _ => 0 } };
    if every == 0 { return; }
    TIMER.with(|t| t.start(TimerMode::SingleShot, Duration::from_millis(every), || { load(false); schedule(); }));
}

fn load_disk() {
    if st(|s| std::mem::replace(&mut s.disk_loading, true)) { return; }
    get("/api/docker/system/df".into(), |r| {
        st(|s| {
            s.disk_loading = false;
            s.disk = Some(match r { Ok(d) => d.get("disk").cloned().unwrap_or(Value::Null), Err(e) => json!({ "error": if e.is_empty() { "Could not measure".into() } else { e } }) });
        });
        render();
    });
}

// ─── Forwards ───

fn take_forwards(list: Vec<Value>) {
    st(|s| { s.fwd_list = Some(list); s.fwd_at = fmt::now_ms(); });
}

fn by_port(port: &str) -> Vec<Value> {
    st(|s| s.fwd_list.as_ref().map(|l| l.iter().filter(|f| ss(f, "port") == port).cloned().collect()).unwrap_or_default())
}

fn load_forwards(force: bool) {
    let skip = st(|s| {
        if !force && s.fwd_list.is_some() && fmt::now_ms() - s.fwd_at < 15000.0 { return true; }
        s.fwd_at = fmt::now_ms();
        false
    });
    if skip { return; }
    get("/api/docker/forwards".into(), |r| {
        if let Ok(d) = r { take_forwards(arr(&d, "forwards").into_iter().cloned().collect()); render(); }
    });
}

fn fwd_chip(f: &Value) -> OpsDkChip {
    let mut c = chip(&ss(f, "host"), "fwd", "url", &ss(f, "url"));
    c.tip = format!("Forward: {}{}", ss(f, "url"), if kit::truthy(f.get("public").unwrap_or(&Value::Null)) { " (public)" } else { "" }).into();
    c
}

fn open_form(port: &str) {
    let blind = port == "any";
    if !blind && st(|s| s.detail.is_none()) { return; }
    let port = port.to_string();
    let have = st(|s| s.fwd_opts.is_some());
    if !have {
        get("/api/forwards/options".into(), move |r| {
            st(|s| s.fwd_opts = Some(r.unwrap_or_else(|_| json!({ "choices": [] }))));
            open_form(&port);
        });
        return;
    }
    let (opts, name) = st(|s| {
        let name = if blind { String::new() } else {
            let c = s.detail.as_ref().map(|d| d.data.get("container").cloned().unwrap_or_default()).unwrap_or_default();
            let n = ss(&c, "service");
            fmt::slug(&if n.is_empty() { ss(&c, "name") } else { n })
        };
        (s.fwd_opts.clone().unwrap_or_default(), name)
    });
    let sub = arr(&opts, "choices").into_iter().find(|c| ss(c, "id") == "subdomain" && kit::truthy(c.get("available").unwrap_or(&Value::Null))).cloned();
    let mut labels = Vec::new();
    let mut values = Vec::new();
    if let Some(sub) = &sub {
        let dom = { let d = ss(sub, "suggestedDomain"); if d.is_empty() { ss(&opts, "domain") } else { d } };
        labels.push(format!("name.{dom}")); values.push("subdomain".to_string());
    }
    labels.push("/tunnel/name/ on this server".into()); values.push("path".into());
    let help = if blind { "Forwards 127.0.0.1 on the port you give: a container\u{2019}s published port, a dev server, anything listening on this machine.".to_string() } else { format!("Forwards 127.0.0.1:{port}.") }
        + " Private unless Public is ticked: only people signed in to Crundi can open it.";
    st(|s| { s.form = Some(port.clone()); s.form_values = values; });
    MODES.with(|m| kit::sync_rows(m, labels.into_iter().map(SharedString::from).collect()));
    with_g(|o| {
        o.set_form_name(name.into());
        o.set_form_target("".into());
        o.set_form_mode(0);
        o.set_form_public(false);
        o.set_form_watch(false);
        o.set_form_help(help.into());
    });
    render();
}

fn create_forward() {
    let Some(port) = st(|s| s.form.clone()) else { return };
    let blind = port == "any";
    let Some(w) = kit::ui() else { return };
    let o = g(&w);
    let target: i64 = if blind { o.get_form_target().trim().parse().unwrap_or(0) } else { port.parse().unwrap_or(0) };
    if !(1..=65535).contains(&target) { kit::toast_error("Give the port to forward, 1 to 65535"); return; }
    let mode = st(|s| s.form_values.get(o.get_form_mode() as usize).cloned().unwrap_or_else(|| "path".into()));
    let (project, desc) = st(|s| match (&s.detail, blind) {
        (Some(d), false) => (ss(&d.data["overview"], "project"), format!("Container {}", ss(&d.data["container"], "name"))),
        _ => (String::new(), String::new()),
    });
    let watch = o.get_form_watch();
    let body = json!({ "name": o.get_form_name().trim(), "port": target, "mode": mode, "public": o.get_form_public(), "project": project, "description": desc });
    post("/api/forwards", body, move |r| match r {
        Err(e) => kit::toast_error(&eo(&e, "Could not create the forward")),
        Ok(d) => {
            let f = d.get("forward").cloned().unwrap_or_default();
            let done = move || {
                kit::toast_ok(&format!("Forward ready: {}", ss(&f, "url")));
                st(|s| s.form = None);
                load_forwards(true);
                if blind && st(|s| s.view == "forwards" && s.detail.is_none()) { load(true); }
                render();
            };
            let host = ss(&d.get("forward").cloned().unwrap_or_default(), "host");
            if watch && !host.is_empty() { set_forward_watch(&host, true, true, move |_| done()); } else { done(); }
        }
    });
}

fn remove_forward(host: &str, quiet: bool, then: impl FnOnce(bool) + Send + 'static) {
    let path = format!("/api/forwards/{}", kit::enc(host));
    let host = host.to_string();
    kit::call(move |api| api.delete(&path), move |r| {
        let r = answer(r);
        if let Err(e) = &r { if !quiet { kit::toast_error(&eo(e, &format!("Could not remove {host}"))); } }
        then(r.is_ok());
    });
}

fn set_forward_watch(host: &str, on: bool, quiet: bool, then: impl FnOnce(bool) + Send + 'static) {
    let path = format!("/api/forwards/{}", kit::enc(host));
    let host = host.to_string();
    kit::call(move |api| api.patch(&path, &json!({ "watch": on })), move |r| {
        let r = answer(r);
        match &r {
            Err(e) => kit::toast_error(&eo(e, "Could not change that")),
            Ok(_) if !quiet => kit::toast_ok(&if on { format!("You will be warned when {host} goes down") } else { format!("No longer watching {host}") }),
            _ => {}
        }
        then(r.is_ok());
    });
}

fn remove_orphans() {
    let orphans: Vec<String> = st(|s| s.fwd_list.clone().unwrap_or_default()).iter().filter(|f| ss(f, "state") == "orphaned").map(|f| ss(f, "host")).collect();
    fn next(left: Vec<String>, n: usize) {
        let mut left = left;
        let Some(h) = left.pop() else {
            kit::toast_ok(&format!("Removed {n} orphaned forward{}", if n == 1 { "" } else { "s" }));
            st(|s| s.fwd_at = 0.0);
            load(true);
            return;
        };
        remove_forward(&h, true, move |ok| next(left, n + ok as usize));
    }
    next(orphans, 0);
}

// ─── Actions ───

fn container_verb(id: &str, verb: &str) {
    let id = if id.is_empty() { st(|s| s.detail.as_ref().map(|d| d.id.clone()).unwrap_or_default()) } else { id.to_string() };
    let verb = verb.to_string();
    post("/api/docker/containers/action", json!({ "id": id, "action": verb }), move |r| {
        match &r {
            Err(e) => kit::toast_error(&eo(&e, &format!("Could not {verb} it"))),
            Ok(d) => {
                let w = match verb.as_str() { "start" => "Started ", "stop" => "Stopped ", "restart" => "Restarted ", "pause" => "Paused ", "unpause" => "Resumed ", _ => "Killed " };
                kit::toast_ok(&format!("{w}{}", ss(d, "name")));
            }
        }
        if st(|s| s.detail.as_ref().is_some_and(|d| d.id == id)) {
            let v = verb.clone();
            refresh_head_then(true, move || {
                if v == "start" || v == "restart" || v == "unpause" {
                    follow_logs();
                    if st(|s| s.detail.as_ref().is_some_and(|d| d.tab == "shell")) { open_shell(); }
                }
            });
        } else {
            load(false);
        }
    });
}

fn stack_action(name: &str, action: &str) {
    let title = format!("{name}: compose {action}");
    post("/api/docker/stacks/action", json!({ "name": name, "action": action }), move |r| match r {
        Err(e) => kit::toast_error(&eo(&e, "Could not run it")),
        Ok(d) => watch_job(&ss(&d, "job"), &title),
    });
}

fn pull() {
    let Some(w) = kit::ui() else { return };
    let r = g(&w).get_pull_ref().trim().to_string();
    if r.is_empty() { return; }
    post("/api/docker/images/pull", json!({ "reference": r }), move |res| match res {
        Err(e) => kit::toast_error(&eo(&e, "Could not pull it")),
        Ok(d) => { with_g(|o| o.set_pull_ref("".into())); watch_job(&ss(&d, "job"), &format!("Pull {r}")); }
    });
}

fn prune(kind: &str) {
    post("/api/docker/prune", json!({ "kind": kind }), |r| match r {
        Err(e) => kit::toast_error(&eo(&e, "Clean-up failed")),
        Ok(d) => {
            let freed = num(&d, "reclaimed").filter(|x| *x > 0.0).map(|x| format!(", freed {}", bytes(Some(x)))).unwrap_or_default();
            kit::toast_ok(&format!("Removed {}{freed}", ss(&d, "removed")));
            st(|s| s.disk = None);
            load(true);
        }
    });
}

fn remove_thing(kind: &str, key: &str) {
    let (path, body, what) = match kind {
        "images" => ("/api/docker/images/remove", json!({ "id": key }), "image"),
        "volumes" => ("/api/docker/volumes/remove", json!({ "name": key }), "volume"),
        _ => ("/api/docker/networks/remove", json!({ "id": key }), "network"),
    };
    post(path, body, move |r| {
        match r {
            Err(e) => kit::toast_error(&eo(&e, &format!("Could not remove the {what}"))),
            Ok(d) => kit::toast_ok(&format!("Removed {}", ss(&d, "name"))),
        }
        load(false);
    });
}

fn chip_act(c: &OpsDkChip) {
    match c.act.as_str() {
        "open" => open_detail(c.target.as_str()),
        "url" => open_url(c.target.as_str()),
        _ => {}
    }
}

// ─── Jobs ───

fn watch_job(id: &str, title: &str) {
    st(|s| s.job = Some(Job { id: id.into(), title: title.into(), ..Default::default() }));
    render_job();
    poll_job(id.to_string());
}

fn poll_job(id: String) {
    let next = st(|s| s.job.as_ref().filter(|j| j.id == id && !j.done).map(|j| j.next));
    let Some(from) = next else { return };
    let jid = id.clone();
    get(format!("/api/docker/job?id={}&from={from}", kit::enc(&id)), move |r| {
        let still = st(|s| s.job.as_ref().is_some_and(|j| j.id == jid));
        if !still { return; }
        let finished = st(|s| {
            let j = s.job.as_mut().unwrap();
            match &r {
                Err(_) => { j.done = true; j.code = -1; j.text.push_str("\n(lost track of this job)"); }
                Ok(d) => {
                    j.text.push_str(&ss(d, "text"));
                    j.next = kit::n(d, "next") as i64;
                    j.done = kit::truthy(d.get("done").unwrap_or(&Value::Null));
                    j.code = kit::n(d, "code") as i64;
                    if j.text.len() > 400_000 { let cut = j.text.len() - 300_000; let mut c = cut; while !j.text.is_char_boundary(c) { c += 1; } j.text.drain(..c); }
                }
            }
            j.done.then(|| (j.title.clone(), j.code))
        });
        render_job();
        match finished {
            Some((title, code)) => {
                if code == 0 { kit::toast_ok(&format!("{title} finished")); } else { kit::toast_error(&format!("{title} failed (exit {code})")); }
                load(false);
            }
            None => { let j = jid.clone(); Timer::single_shot(Duration::from_millis(700), move || poll_job(j)); }
        }
    });
}

fn render_job() {
    let job = st(|s| s.job.as_ref().map(|j| (j.title.clone(), j.text.clone(), j.done, j.code)));
    with_g(|o| match job {
        None => o.set_job_open(false),
        Some((title, text, done, code)) => {
            o.set_job_open(true);
            o.set_job_title(title.into());
            o.set_job_text(text.into());
            o.set_job_done(done);
            o.set_job_ok(code == 0);
            o.set_job_code(code.to_string().into());
            o.set_job_gen(o.get_job_gen() + 1);
        }
    });
}

// ─── The detail ───

fn open_detail(id: &str) {
    if id.is_empty() { return; }
    close_detail();
    st(|s| { s.form = None; s.detail = Some(Detail { id: id.into(), tab: "overview".into(), ..Default::default() }); });
    render();
    let want = id.to_string();
    get(format!("/api/docker/containers/detail?id={}", kit::enc(id)), move |r| {
        let mine = st(|s| s.detail.as_ref().is_some_and(|d| d.id == want && !d.loaded));
        if !mine { return; }
        match r {
            Err(e) => st(|s| { let d = s.detail.as_mut().unwrap(); d.error = if e.is_empty() { "Could not open it".into() } else { e }; }),
            Ok(d) => st(|s| {
                let dt = s.detail.as_mut().unwrap();
                dt.id = ss(&d["container"], "id");
                dt.data = d;
                dt.loaded = true;
            }),
        }
        render();
        let tab = st(|s| s.detail.as_ref().map(|d| d.tab.clone()).unwrap_or_default());
        if st(|s| s.detail.as_ref().is_some_and(|d| d.loaded)) {
            select_tab(&tab);
            load_forwards(false);
        }
        schedule();
    });
}

fn close_detail() {
    if st(|s| s.detail.is_none()) { return; }
    stop_logs();
    close_shell();
    STATS_TIMER.with(|t| t.stop());
    st(|s| s.detail = None);
}

fn select_tab(t: &str) {
    let ok = st(|s| match s.detail.as_mut() { Some(d) => { d.tab = t.to_string(); true } None => false });
    if !ok { return; }
    render();
    let (log_ref, shell_ref) = st(|s| { let d = s.detail.as_ref().unwrap(); (d.log_ref.is_some(), d.shell_ref.is_some()) });
    if t == "logs" && !log_ref { follow_logs(); }
    if t == "stats" { start_stats(); } else { STATS_TIMER.with(|x| x.stop()); }
    if t == "shell" && !shell_ref { open_shell(); }
}

fn refresh_head(force: bool) { refresh_head_then(force, || {}); }

fn refresh_head_then(force: bool, then: impl FnOnce() + Send + 'static) {
    let go = st(|s| s.detail.as_ref().is_some_and(|d| d.loaded && (force || d.prompt.is_empty())));
    if !go { return; }
    let id = st(|s| s.detail.as_ref().unwrap().id.clone());
    get("/api/docker/containers".into(), move |r| {
        let Ok(d) = r else { return };
        if st(|s| !s.detail.as_ref().is_some_and(|x| x.id == id)) { return; }
        let c = arr(&d, "containers").into_iter().find(|x| ss(x, "id") == id || ss(x, "fullId") == id).cloned();
        let Some(c) = c else {
            let name = st(|s| ss(&s.detail.as_ref().unwrap().data["container"], "name"));
            kit::toast(&format!("{} is gone", if name.is_empty() { "The container".into() } else { name }));
            set_view("containers");
            return;
        };
        let (was, tab, shell) = st(|s| {
            let dt = s.detail.as_mut().unwrap();
            let was = kit::truthy(dt.data["container"].get("running").unwrap_or(&Value::Null));
            dt.data["container"] = c.clone();
            (was, dt.tab.clone(), dt.shell_ref.is_some())
        });
        let now = kit::truthy(c.get("running").unwrap_or(&Value::Null));
        render();
        if !was && now && tab == "shell" && !shell { open_shell(); }
        then();
    });
}

fn open_prompt(kind: &str) {
    let name = st(|s| match s.detail.as_mut() { Some(d) => { d.prompt = kind.to_string(); ss(&d.data["container"], "name") } None => String::new() });
    with_g(|o| {
        if kind == "rename" { o.set_d_newname(name.into()); }
        if kind == "remove" { o.set_d_rmvol(false); o.set_d_rmforce(true); }
    });
    render();
}

fn rename_do() {
    let Some(w) = kit::ui() else { return };
    let name = g(&w).get_d_newname().trim().to_string();
    let Some(id) = st(|s| s.detail.as_ref().map(|d| d.id.clone())) else { return };
    post("/api/docker/containers/rename", json!({ "id": id, "name": name }), |r| match r {
        Err(e) => kit::toast_error(&eo(&e, "Could not rename it")),
        Ok(d) => {
            kit::toast_ok(&format!("Renamed to {}", ss(&d, "name")));
            st(|s| if let Some(d) = s.detail.as_mut() { d.prompt.clear(); });
            render();
            refresh_head(true);
        }
    });
}

fn remove_do() {
    let Some(w) = kit::ui() else { return };
    let o = g(&w);
    let Some((id, live)) = st(|s| s.detail.as_ref().map(|d| (d.id.clone(), { let c = &d.data["container"]; kit::truthy(c.get("running").unwrap_or(&Value::Null)) || ss(c, "state") == "paused" }))) else { return };
    let body = json!({ "id": id, "volumes": o.get_d_rmvol(), "force": live && o.get_d_rmforce() });
    post("/api/docker/containers/remove", body, |r| match r {
        Err(e) => kit::toast_error(&eo(&e, "Could not remove it")),
        Ok(d) => { kit::toast_ok(&format!("Removed {}", ss(&d, "name"))); set_view("containers"); }
    });
}

// ─── Logs ───

fn next_ref(p: &str) -> String { st(|s| { s.refs += 1; format!("{p}{}", s.refs) }) }

fn follow_logs() {
    let Some(id) = st(|s| s.detail.as_ref().map(|d| d.id.clone())) else { return };
    stop_logs();
    st(|s| { let d = s.detail.as_mut().unwrap(); d.lines.clear(); d.partial = Default::default(); });
    rebuild_logs();
    if st(|s| s.connected) {
        let r = next_ref("l");
        st(|s| s.detail.as_mut().unwrap().log_ref = Some(r.clone()));
        kit::ws_send(&json!({ "type": "docker-logs-follow", "ref": r, "id": id, "tail": 500 }));
    } else {
        // No live socket: a one-off read so the tab is not blank.
        let want = id.clone();
        get(format!("/api/docker/containers/logs?id={}&tail=500", kit::enc(&id)), move |r| {
            if st(|s| !s.detail.as_ref().is_some_and(|d| d.id == want)) { return; }
            match r {
                Err(e) => sys_line(if e.is_empty() { "Could not read the log" } else { &e }),
                Ok(d) => { add_frames(&[json!({ "stream": "stdout", "text": ss(&d, "text") })]); sys_line("Not live: the connection to the server is down."); }
            }
        });
    }
}

fn stop_logs() {
    let r = st(|s| s.detail.as_mut().and_then(|d| d.log_ref.take()));
    if let Some(r) = r { kit::ws_send(&json!({ "type": "docker-logs-stop", "ref": r })); }
}

fn inks() -> ([String; 8], String) {
    let hex = |c: slint::Color| format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue());
    let Some(w) = kit::ui() else { return (Default::default(), String::new()) };
    let o = g(&w);
    let a = o.get_ansi();
    (std::array::from_fn(|i| a.row_data(i).map(hex).unwrap_or_default()), hex(o.get_mark()))
}

fn log_row(rec: &LogRec, filter: &str, ts: bool, ink: &([String; 8], String)) -> Option<OpsDkLog> {
    if !rec.sys && !filter.is_empty() && !fmt::strip_ansi(&rec.text).to_lowercase().contains(&filter.to_lowercase()) { return None; }
    let markup = if rec.sys {
        format!("<font color=\"{}\">*{}*</font>", ink.0[0], fmt::md_escape(&rec.text, true))
    } else {
        let body = fmt::line_markup(&rec.text, filter, &ink.0, &ink.1);
        if ts && !rec.ts.is_empty() { format!("<font color=\"{}\">{}</font>\u{a0}\u{a0}{body}", ink.0[0], fmt::md_escape(&fmt::ts_show(&rec.ts), false)) } else { body }
    };
    let md = StyledText::from_markdown(&markup)
        .or_else(|_| StyledText::from_markdown(&fmt::md_escape(&fmt::strip_ansi(&rec.text), true)))
        .unwrap_or_default();
    Some(OpsDkLog { md, err: rec.err })
}

fn log_opts() -> (String, bool) {
    kit::ui().map(|w| { let o = g(&w); (o.get_log_filter().trim().to_string(), o.get_log_ts()) }).unwrap_or_default()
}

fn rebuild_logs() {
    let (filter, ts) = log_opts();
    let ink = inks();
    let lines = st(|s| s.detail.as_ref().map(|d| d.lines.clone()).unwrap_or_default());
    let rows: Vec<OpsDkLog> = lines.iter().filter_map(|r| log_row(r, &filter, ts, &ink)).collect();
    LOGS.with(|m| m.set_vec(rows));
    with_g(|o| o.set_log_gen(o.get_log_gen() + 1));
}

fn append_logs(fresh: &[LogRec], dropped: usize) {
    if dropped > 0 { rebuild_logs(); return; }
    let (filter, ts) = log_opts();
    let ink = inks();
    LOGS.with(|m| for r in fresh { if let Some(row) = log_row(r, &filter, ts, &ink) { m.push(row); } });
    with_g(|o| o.set_log_gen(o.get_log_gen() + 1));
}

fn add_frames(frames: &[Value]) {
    let r = st(|s| {
        let d = s.detail.as_mut()?;
        let mut fresh = Vec::new();
        for f in frames {
            let k = if ss(f, "stream") == "stderr" { 1 } else { 0 };
            let text = format!("{}{}", d.partial[k], ss(f, "text"));
            let mut parts: Vec<&str> = text.split('\n').collect();
            d.partial[k] = parts.pop().unwrap_or("").to_string();
            for line in parts {
                let line = line.strip_suffix('\r').unwrap_or(line);
                let (ts, text) = fmt::split_ts(line);
                let rec = LogRec { ts, text, err: k == 1, sys: false };
                d.lines.push(rec.clone());
                fresh.push(rec);
            }
        }
        let mut dropped = 0;
        if d.lines.len() > MAX_LINES { dropped = d.lines.len() - MAX_LINES; d.lines.drain(..dropped); }
        Some((fresh, dropped))
    });
    if let Some((fresh, dropped)) = r { append_logs(&fresh, dropped); }
}

fn sys_line(text: &str) {
    let rec = LogRec { text: format!("— {text}"), sys: true, ..Default::default() };
    let ok = st(|s| match s.detail.as_mut() { Some(d) => { d.lines.push(rec.clone()); true } None => false });
    if ok { append_logs(&[rec], 0); }
}

fn flush_partial() {
    let rest = st(|s| {
        let d = s.detail.as_mut()?;
        let mut out = Vec::new();
        for (k, stream) in [(0, "stdout"), (1, "stderr")] {
            if !d.partial[k].is_empty() { out.push(json!({ "stream": stream, "text": format!("{}\n", d.partial[k]) })); d.partial[k].clear(); }
        }
        Some(out)
    });
    if let Some(r) = rest { if !r.is_empty() { add_frames(&r); } }
}

fn log_copy() {
    let ts = log_opts().1;
    let text = st(|s| s.detail.as_ref().map(|d| d.lines.iter().map(|l| format!("{}{}", if ts && !l.ts.is_empty() { format!("{} ", l.ts) } else { String::new() }, fmt::strip_ansi(&l.text))).collect::<Vec<_>>().join("\n")).unwrap_or_default());
    copy(&text);
}

fn log_filter_soon() {
    thread_local! { static T: Timer = Timer::default(); }
    T.with(|t| t.start(TimerMode::SingleShot, Duration::from_millis(150), rebuild_logs));
}

fn log_option(k: &str, on: bool) {
    with_g(|o| match k { "follow" => o.set_log_follow(on), "ts" => o.set_log_ts(on), _ => o.set_log_wrap(on) });
    if k == "ts" { rebuild_logs(); }
    if k == "follow" && on { with_g(|o| o.set_log_gen(o.get_log_gen() + 1)); }
}

// ─── Stats ───

fn start_stats() {
    st(|s| if let Some(d) = s.detail.as_mut() { d.samples.clear(); d.stats_msg = None; });
    stats_tick();
    STATS_TIMER.with(|t| t.start(TimerMode::Repeated, Duration::from_millis(2000), stats_tick));
}

fn stats_tick() {
    let Some(id) = st(|s| if s.shown { s.detail.as_ref().map(|d| d.id.clone()) } else { None }) else { return };
    let want = id.clone();
    get(format!("/api/docker/containers/stats?id={}", kit::enc(&id)), move |r| {
        st(|s| {
            let Some(d) = s.detail.as_mut().filter(|d| d.id == want) else { return };
            match r {
                Err(e) => d.stats_msg = Some(if e.is_empty() { "No numbers".into() } else { e }),
                Ok(v) if !kit::truthy(v.get("running").unwrap_or(&Value::Null)) => {
                    d.stats_msg = Some("The container is not running, so there is nothing to measure.".into());
                    d.samples.clear();
                }
                Ok(v) => {
                    d.stats_msg = None;
                    if let Some(x) = v.get("stats").filter(|x| !x.is_null()) { d.samples.push(x.clone()); if d.samples.len() > 120 { d.samples.remove(0); } }
                }
            }
        });
        render_stats();
    });
}

fn render_stats() {
    let (msg, s) = st(|s| s.detail.as_ref().map(|d| (d.stats_msg.clone(), d.samples.clone())).unwrap_or_default());
    with_g(|o| {
        if let Some(m) = msg { o.set_stats_state("msg".into()); o.set_stats_msg(m.into()); return; }
        if s.is_empty() { o.set_stats_state("measuring".into()); return; }
        let last = s.last().unwrap();
        let col = |k: &str| s.iter().map(|x| num(x, k)).collect::<Vec<_>>();
        let (cpu, mem) = (col("cpuPct"), col("memory"));
        let (rx, tx, br, bw) = (fmt::rates(&s, "netRx"), fmt::rates(&s, "netTx"), fmt::rates(&s, "blkRead"), fmt::rates(&s, "blkWrite"));
        let last_of = |a: &[Option<f64>]| a.last().copied().flatten();
        let mem_limit = num(last, "memoryLimit").filter(|x| *x > 0.0);
        let mk = |title: &str, value: String, sub: String, a: &[Option<f64>], b: Option<&[Option<f64>]>, hint: f64, leg: (String, String)| {
            let max = fmt::chart_max(&[a, b.unwrap_or(&[])], hint);
            let (a1, l1) = fmt::chart_paths(a, max);
            let (a2, l2) = b.map(|b| fmt::chart_paths(b, max)).unwrap_or_default();
            OpsDkChart { title: title.into(), value: value.into(), sub: sub.into(), a1: a1.into(), l1: l1.into(), a2: a2.into(), l2: l2.into(), leg1: leg.0.into(), leg2: leg.1.into() }
        };
        let cpus = ss(last, "cpus");
        let charts = vec![
            mk("CPU", pct(num(last, "cpuPct")), if cpus.is_empty() || cpus == "0" { String::new() } else { format!("of one core · {cpus} available") }, &cpu, None, 100.0, Default::default()),
            mk("MEMORY", bytes(num(last, "memory")), mem_limit.map(|l| format!("of {} ({})", bytes(Some(l)), pct(num(last, "memory").map(|m| m / l * 100.0)))).unwrap_or_default(), &mem, None, 0.0, Default::default()),
            mk("NETWORK", format!("{}/s", bytes(last_of(&rx))), "in".into(), &rx, Some(&tx), 0.0,
                (format!("in {}/s · {} total", bytes(last_of(&rx)), bytes(num(last, "netRx"))), format!("out {}/s · {} total", bytes(last_of(&tx)), bytes(num(last, "netTx"))))),
            mk("DISK", format!("{}/s", bytes(last_of(&br))), "read".into(), &br, Some(&bw), 0.0,
                (format!("read {} total", bytes(num(last, "blkRead"))), format!("write {} total", bytes(num(last, "blkWrite"))))),
        ];
        CHARTS.with(|m| kit::sync_rows(m, charts));
        let mins = (s.len() as f64 * 2.0 / 60.0 * 10.0).round() / 10.0;
        o.set_stats_note(format!("{} processes · a sample every 2 seconds, the last {} minutes", ss(last, "pids"), mins).into());
        o.set_stats_state("data".into());
    });
}

// ─── Shell ───

fn open_shell() {
    let Some((id, running)) = st(|s| s.detail.as_ref().map(|d| (d.id.clone(), kit::truthy(d.data["container"].get("running").unwrap_or(&Value::Null))))) else { return };
    close_shell();
    st(|s| { let d = s.detail.as_mut().unwrap(); d.shell_msg = None; d.screen = fmt::Screen::new(); });
    if running {
        if !st(|s| s.connected) {
            st(|s| s.detail.as_mut().unwrap().shell_msg = Some("No connection to the server right now.".into()));
        } else {
            let r = next_ref("s");
            st(|s| s.detail.as_mut().unwrap().shell_ref = Some(r.clone()));
            kit::ws_send(&json!({ "type": "docker-shell-open", "ref": r, "id": id, "cols": 120, "rows": 32 }));
        }
    }
    render_shell();
}

fn close_shell() {
    let r = st(|s| s.detail.as_mut().and_then(|d| d.shell_ref.take()));
    if let Some(r) = r { kit::ws_send(&json!({ "type": "docker-shell-close", "ref": r })); }
}

fn shell_input(data: &str) {
    let r = st(|s| s.detail.as_ref().and_then(|d| d.shell_ref.clone()));
    if let Some(r) = r { kit::ws_send(&json!({ "type": "docker-shell-input", "ref": r, "data": data })); }
}

fn shell_key(k: &str) {
    let data = match k {
        "ctrl-c" => "\u{3}".to_string(),
        "ctrl-d" => "\u{4}".to_string(),
        "ctrl-l" => "\u{c}".to_string(),
        "ctrl-z" => "\u{1a}".to_string(),
        "up" => "\u{1b}[A".to_string(),
        "down" => "\u{1b}[B".to_string(),
        _ => match k.strip_prefix("tab:") { Some(t) => format!("{t}\t"), None => return },
    };
    shell_input(&data);
}

fn render_shell() {
    let info = st(|s| s.detail.as_ref().map(|d| (d.shell_msg.clone(), d.shell_ref.is_some(), ss(&d.data["container"], "name"), kit::truthy(d.data["container"].get("running").unwrap_or(&Value::Null)), d.screen.text())));
    let Some((msg, live, name, running, text)) = info else { return };
    with_g(|o| {
        let (bar, btn) = match (&msg, running) {
            (Some(m), r) => (m.clone(), if r { "Open again" } else { "" }),
            (None, false) => ("Start the container to open a shell in it.".to_string(), ""),
            (None, true) => (format!("A shell inside {name} (bash when the image has it, else sh)"), "New shell"),
        };
        o.set_shell_bar(bar.into());
        o.set_shell_btn(btn.into());
        o.set_shell_live(live);
        o.set_shell_text(text.into());
        o.set_shell_gen(o.get_shell_gen() + 1);
    });
}

fn inspect_text() -> String {
    st(|s| s.detail.as_ref().map(|d| serde_json::to_string_pretty(&d.data["inspect"]).unwrap_or_default()).unwrap_or_default())
}

// ─── Socket messages ───

fn on_message(t: &str, m: &Value) {
    let r = ss(m, "ref");
    let (log_ref, shell_ref) = st(|s| s.detail.as_ref().map(|d| (d.log_ref.clone(), d.shell_ref.clone())).unwrap_or_default());
    match t {
        "docker-logs" if log_ref.as_deref() == Some(&r) => add_frames(&arr(m, "frames").into_iter().cloned().collect::<Vec<_>>()),
        "docker-logs-end" if log_ref.as_deref() == Some(&r) => {
            let running = st(|s| { let d = s.detail.as_mut().unwrap(); d.log_ref = None; kit::truthy(d.data["container"].get("running").unwrap_or(&Value::Null)) });
            flush_partial();
            sys_line(if running { "The log stream ended." } else { "The container stopped. Start it to follow the log again." });
        }
        "docker-logs-error" if log_ref.as_deref() == Some(&r) => {
            st(|s| s.detail.as_mut().unwrap().log_ref = None);
            let e = ss(m, "error");
            sys_line(if e.is_empty() { "Could not follow the log" } else { &e });
        }
        "docker-shell-data" if shell_ref.as_deref() == Some(&r) => {
            st(|s| s.detail.as_mut().unwrap().screen.write(&ss(m, "data")));
            render_shell();
        }
        "docker-shell-exit" if shell_ref.as_deref() == Some(&r) => {
            let code = ss(m, "code");
            let msg = if code == "126" || code == "127" { format!("This image may not include a shell (exit {code}).") } else { format!("The shell ended (exit {code}).") };
            st(|s| { let d = s.detail.as_mut().unwrap(); d.shell_ref = None; d.shell_msg = Some(msg); });
            render_shell();
        }
        "docker-shell-error" if shell_ref.as_deref() == Some(&r) => {
            let e = ss(m, "error");
            st(|s| { let d = s.detail.as_mut().unwrap(); d.shell_ref = None; d.shell_msg = Some(if e.is_empty() { "Could not open a shell".into() } else { e }); });
            render_shell();
        }
        _ => {}
    }
}

fn on_reconnect() {
    let Some((log, tab, shell)) = st(|s| if s.shown { s.detail.as_ref().map(|d| (d.log_ref.is_some(), d.tab.clone(), d.shell_ref.is_some())) } else { None }) else { return };
    // The server dropped our streams with the old socket.
    if log || tab == "logs" { follow_logs(); }
    if shell {
        st(|s| { let d = s.detail.as_mut().unwrap(); d.shell_ref = None; d.shell_msg = Some("The connection dropped, which ended the shell.".into()); });
        render_shell();
    }
}

// ─── Drawing ───

fn render() {
    let Some(w) = kit::ui() else { return };
    let o = g(&w);
    ST.with(|cell| {
        let s = cell.borrow();
        // The gate.
        let gate = match &s.status {
            None => "loading",
            Some(x) if !kit::truthy(x.get("installed").unwrap_or(&Value::Null)) => "absent",
            Some(x) if !kit::truthy(x.get("reachable").unwrap_or(&Value::Null)) => "unreachable",
            _ => "ok",
        };
        o.set_gate(gate.into());
        o.set_gate_error(s.status.as_ref().map(|x| ss(x, "error")).unwrap_or_default().into());
        // The nav.
        let count = |k: &str| -> String {
            if k == "forwards" { return s.fwd_list.as_ref().map(|l| l.len().to_string()).unwrap_or_default(); }
            s.data.get(k).and_then(|x| x.as_array()).map(|a| a.len().to_string()).unwrap_or_default()
        };
        SEGS.with(|m| kit::sync_rows(m, VIEWS.iter().map(|(k, l)| OpsDkSeg { key: (*k).into(), label: (*l).into(), count: count(k).into() }).collect()));
        o.set_view(s.view.clone().into());
        let filter = s.filter.get(&s.view).cloned().unwrap_or_default();
        if o.get_filter().as_str() != filter { o.set_filter(filter.clone().into()); }
        o.set_state(if s.error.is_some() { "error" } else if s.data.contains_key(&s.view) || (s.view == "forwards" && s.fwd_list.is_some()) { "data" } else { "loading" }.into());
        if let Some((t, e)) = &s.error { o.set_err_title(t.into()); o.set_err_text(e.into()); }
        o.set_form_port(s.form.clone().unwrap_or_default().into());
        let mut nomatch = false;
        match s.view.as_str() {
            "containers" => draw_containers(&o, &s, &filter, &mut nomatch),
            "stacks" => draw_stacks(&o, &s, &filter, &mut nomatch),
            "images" | "volumes" | "networks" => draw_things(&o, &s, &filter, &mut nomatch),
            "forwards" => draw_forwards(&o, &s, &filter, &mut nomatch),
            _ => draw_system(&o, &s),
        }
        o.set_nomatch(nomatch);
        draw_detail(&o, &s);
    });
    render_shell();
}

fn draw_containers(o: &OpsDocker, s: &St, f: &str, nomatch: &mut bool) {
    let list: Vec<Value> = s.data.get("containers").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    o.set_ctr_empty(list.is_empty());
    let (mut run, mut paused, mut stopped, mut cpu, mut mem) = (0, 0, 0, 0.0, 0.0);
    for c in &list {
        match ss(c, "state").as_str() { "running" => run += 1, "paused" => paused += 1, _ => stopped += 1 }
        if let Some(st) = c.get("stats").filter(|x| x.is_object()) { cpu += num(st, "cpuPct").unwrap_or(0.0); mem += num(st, "memory").unwrap_or(0.0); }
    }
    let stat = |v: String, l: &str| OpsDkStat { v: v.into(), l: l.to_uppercase().into() };
    STATS.with(|m| kit::sync_rows(m, vec![stat(run.to_string(), "running"), stat(paused.to_string(), "paused"), stat(stopped.to_string(), "stopped"), stat(pct(Some(cpu)), "CPU, of one core"), stat(bytes(Some(mem)), "memory")]));
    BAY.with(|m| kit::sync_rows(m, list.iter().map(|c| OpsDkBay { id: ss(c, "id").into(), state: fmt::state_key(c).into(), tip: format!("{} — {}", ss(c, "name"), ss(c, "status")).into() }).collect()));
    let shown: Vec<&Value> = list.iter().filter(|c| fmt::matches(f, &[ss(c, "name"), ss(c, "image"), ss(c, "project"), ss(c, "service"), ss(c, "status"), ss(c, "state"), ss(c, "id")])).collect();
    *nomatch = !list.is_empty() && shown.is_empty();
    let rows = shown.iter().map(|c| {
        let mut chips = Vec::new();
        let p = ss(c, "project");
        if !p.is_empty() { chips.push(chip(&p, "", "", "")); }
        for port in arr(c, "ports").into_iter().filter(|p| kit::n(p, "public") > 0.0).take(3) {
            chips.push(chip(&fmt::port_text(port), "", "", ""));
            if let Some(l) = &s.fwd_list { for fw in l.iter().filter(|fw| ss(fw, "port") == ss(port, "public")) { chips.push(fwd_chip(fw)); } }
        }
        let h = ss(c, "health");
        if !h.is_empty() { chips.push(chip(&h, if h == "healthy" { "ok" } else if h == "unhealthy" { "bad" } else { "warn" }, "", "")); }
        let stt = c.get("stats").filter(|x| x.is_object());
        let act = if kit::truthy(c.get("running").unwrap_or(&Value::Null)) { "stop" } else if ss(c, "state") == "paused" { "unpause" } else { "start" };
        OpsDkCtr {
            id: ss(c, "id").into(), name: ss(c, "name").into(), sub: format!("{} · {}", ss(c, "image"), ss(c, "status")).into(),
            state: fmt::state_key(c).into(), chips: vm(chips),
            cpu: stt.map(|x| pct(num(x, "cpuPct"))).unwrap_or_default().into(),
            mem: stt.map(|x| bytes(num(x, "memory"))).unwrap_or_default().into(),
            act: act.into(),
        }
    }).collect();
    CTRS.with(|m| kit::sync_rows(m, rows));
}

fn draw_stacks(o: &OpsDocker, s: &St, f: &str, nomatch: &mut bool) {
    let list: Vec<Value> = s.data.get("stacks").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let full = list.first().is_some_and(|x| x.get("containers").is_some());
    o.set_stacks_empty(list.is_empty() || !full);
    if !full { STACKS.with(|m| kit::sync_rows(m, vec![])); return; }
    let shown: Vec<&Value> = list.iter().filter(|st| {
        let mut parts = vec![ss(st, "name"), ss(st, "directory")];
        parts.extend(arr(st, "containers").iter().map(|c| format!("{} {}", ss(c, "service"), ss(c, "name"))));
        fmt::matches(f, &parts)
    }).collect();
    *nomatch = shown.is_empty();
    let rows = shown.iter().map(|st| {
        let (live, total) = (kit::n(st, "live") as i64, kit::n(st, "total") as i64);
        let files = strs(st, "files");
        let present = kit::truthy(st.get("filesPresent").unwrap_or(&Value::Null));
        let mut wh = if files.is_empty() { "compose file unknown".to_string() } else { files.join(", ") };
        if !files.is_empty() && !present { wh.push_str("  (not found on this machine)"); }
        let svcs = arr(st, "containers").iter().map(|c| {
            let pubs: Vec<String> = arr(c, "ports").into_iter().filter(|p| kit::n(p, "public") > 0.0).take(2).map(fmt::port_text).collect();
            let svc = ss(c, "service");
            OpsDkSvc {
                id: ss(c, "id").into(), state: fmt::state_key(c).into(),
                name: if svc.is_empty() { ss(c, "name") } else { svc }.into(), sub: ss(c, "name").into(),
                img: format!("{}{}", ss(c, "image"), if pubs.is_empty() { String::new() } else { format!(" · {}", pubs.join("  ")) }).into(),
                status: ss(c, "status").into(),
            }
        }).collect::<Vec<_>>();
        OpsDkStack {
            name: ss(st, "name").into(), chip: format!("{live}/{total} running").into(),
            tone: if live == total { "ok" } else if live == 0 { "" } else { "warn" }.into(),
            r#where: wh.into(), files: present, live: live > 0, svcs: vm(svcs),
        }
    }).collect();
    STACKS.with(|m| kit::sync_rows(m, rows));
}

fn draw_things(o: &OpsDocker, s: &St, f: &str, nomatch: &mut bool) {
    let list: Vec<Value> = s.data.get(&s.view).and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let used = |x: &Value| strs(x, "usedBy");
    let open_chips = |names: &[String], most: usize| {
        let mut c: Vec<OpsDkChip> = names.iter().take(most).map(|n| chip(n, "ok", "open", n)).collect();
        if names.len() > most { c.push(chip(&format!("+{}", names.len() - most), "", "", "")); }
        c
    };
    let (head, rows): (String, Vec<OpsDkRow>) = match s.view.as_str() {
        "images" => {
            let in_use = list.iter().filter(|i| !used(i).is_empty()).count();
            let total: f64 = list.iter().map(|i| kit::n(i, "size")).sum();
            let rows = list.iter().filter(|i| fmt::matches(f, &[ss(i, "name"), strs(i, "tags").join(" "), ss(i, "id"), used(i).join(" ")])).map(|i| {
                let u = used(i);
                let extra: Vec<String> = strs(i, "tags").into_iter().skip(1).collect();
                let chips = if !u.is_empty() { open_chips(&u, 4) } else if kit::truthy(i.get("dangling").unwrap_or(&Value::Null)) { vec![chip("untagged", "warn", "", "")] } else { vec![chip("unused", "", "", "")] };
                OpsDkRow {
                    key: ss(i, "fullId").into(), name: ss(i, "name").into(), tip: SharedString::default(), tags: vm(vec![]),
                    sub: format!("{} · {} · {}{}", ss(i, "id"), bytes(num(i, "size")), fmt::ago(i.get("created").unwrap_or(&Value::Null)), if extra.is_empty() { String::new() } else { format!(" · also {}", extra.join(", ")) }).into(),
                    chips: vm(chips), can_rm: u.is_empty(), arm: if u.is_empty() { "Remove?" } else { "" }.into(),
                }
            }).collect();
            (format!("{} images, {in_use} in use · {}", list.len(), bytes(Some(total))), rows)
        }
        "volumes" => {
            let unused = list.iter().filter(|v| used(v).is_empty()).count();
            let rows = list.iter().filter(|v| fmt::matches(f, &[ss(v, "name"), ss(v, "project"), ss(v, "driver"), used(v).join(" ")])).map(|v| {
                let u = used(v);
                let name = ss(v, "name");
                let shown = if kit::truthy(v.get("anonymous").unwrap_or(&Value::Null)) { format!("anonymous {}", name.chars().take(12).collect::<String>()) } else { name.clone() };
                let p = ss(v, "project");
                OpsDkRow {
                    key: name.clone().into(), name: shown.into(), tip: name.into(), tags: vm(vec![]),
                    sub: format!("{}{} · {}", ss(v, "driver"), if p.is_empty() { String::new() } else { format!(" · {p}") }, fmt::ago(v.get("created").unwrap_or(&Value::Null))).into(),
                    chips: vm(if u.is_empty() { vec![chip("unused", "", "", "")] } else { open_chips(&u, usize::MAX) }),
                    can_rm: u.is_empty(), arm: if u.is_empty() { "Delete its data?" } else { "" }.into(),
                }
            }).collect();
            (format!("{} volumes, {unused} not used by any container. Sizes are under System.", list.len()), rows)
        }
        _ => {
            let rows = list.iter().filter(|n| fmt::matches(f, &[ss(n, "name"), ss(n, "driver"), ss(n, "subnet"), ss(n, "project"), used(n).join(" ")])).map(|n| {
                let u = used(n);
                let builtin = kit::truthy(n.get("builtin").unwrap_or(&Value::Null));
                let mut tags = Vec::new();
                if builtin { tags.push(chip("built in", "", "", "")); }
                if kit::truthy(n.get("internal").unwrap_or(&Value::Null)) { tags.push(chip("internal", "", "", "")); }
                let (sub, gw) = (ss(n, "subnet"), ss(n, "gateway"));
                let can = !builtin && u.is_empty();
                OpsDkRow {
                    key: ss(n, "fullId").into(), name: ss(n, "name").into(), tip: SharedString::default(), tags: vm(tags),
                    sub: format!("{} · {}{}{}", ss(n, "driver"), ss(n, "scope"), if sub.is_empty() { String::new() } else { format!(" · {sub}") }, if gw.is_empty() { String::new() } else { format!(" via {gw}") }).into(),
                    chips: vm(open_chips(&u, 6)), can_rm: can, arm: if can { "Remove?" } else { "" }.into(),
                }
            }).collect();
            (format!("{} networks", list.len()), rows)
        }
    };
    o.set_list_head(head.into());
    o.set_list_empty(list.is_empty() && s.view != "networks");
    *nomatch = rows.is_empty() && (!list.is_empty() || s.view == "networks");
    ROWS.with(|m| kit::sync_rows(m, rows));
}

const FWD_GROUPS: [(&str, &str, &str); 4] = [
    ("orphaned", "Nothing behind it", "No container publishes this port and nothing answers on it. Left over from something that is gone, most likely."),
    ("stopped", "Container stopped", "The container that publishes this port is not running."),
    ("container", "Running container", ""),
    ("other", "Another program", "Something on this machine answers on the port, but not a container."),
];

fn draw_forwards(o: &OpsDocker, s: &St, f: &str, nomatch: &mut bool) {
    let list = s.fwd_list.clone().unwrap_or_default();
    o.set_fwd_empty(list.is_empty());
    let orphans = list.iter().filter(|x| ss(x, "state") == "orphaned").count();
    o.set_orphans(orphans as i32);
    o.set_list_head(format!("{} forwards{}", list.len(), if orphans > 0 { format!(", {orphans} with nothing behind them") } else { String::new() }).into());
    let shown: Vec<&Value> = list.iter().filter(|fw| {
        let c = fw.get("container").map(|c| ss(c, "name")).unwrap_or_default();
        fmt::matches(f, &[ss(fw, "host"), ss(fw, "url"), ss(fw, "port"), ss(fw, "project"), c, ss(fw, "state")])
    }).collect();
    *nomatch = !list.is_empty() && shown.is_empty();
    let groups = FWD_GROUPS.iter().filter_map(|(state, title, help)| {
        let rows: Vec<OpsDkFwd> = shown.iter().filter(|fw| ss(fw, "state") == *state).map(|fw| {
            let mut tags = Vec::new();
            if kit::truthy(fw.get("public").unwrap_or(&Value::Null)) { tags.push(chip("public", "warn", "", "")); }
            let watch = kit::truthy(fw.get("watch").unwrap_or(&Value::Null));
            if watch { tags.push(chip("watched", "ok", "watch", "")); }
            let mut chips = Vec::new();
            if let Some(c) = fw.get("container").filter(|c| c.is_object()) {
                let running = kit::truthy(c.get("running").unwrap_or(&Value::Null));
                let h = ss(c, "health");
                let tone = if h == "unhealthy" { "bad" } else if running { "ok" } else { "" };
                let mut ch = chip(&format!("{}{}", ss(c, "name"), if h == "unhealthy" { " · unhealthy" } else { "" }), tone, "open", &ss(c, "name"));
                ch.tip = if running { format!("Running{}", if h.is_empty() { String::new() } else { format!(", {h}") }) } else { "Stopped".into() }.into();
                chips.push(ch);
            }
            chips.push(chip("Open", "fwd", "url", &ss(fw, "url")));
            let (p, d) = (ss(fw, "project"), ss(fw, "description"));
            OpsDkFwd {
                host: ss(fw, "host").into(),
                sub: format!("port {} · {}{}{}", ss(fw, "port"), if ss(fw, "mode") == "path" { "path" } else { "subdomain" }, if p.is_empty() { String::new() } else { format!(" · {p}") }, if d.is_empty() { String::new() } else { format!(" · {d}") }).into(),
                tags: vm(tags), chips: vm(chips), watch,
            }
        }).collect();
        if rows.is_empty() { None } else { Some(OpsDkGroup { title: format!("{} · {}", title.to_uppercase(), rows.len()).into(), help: (*help).into(), rows: vm(rows) }) }
    }).collect();
    GROUPS.with(|m| kit::sync_rows(m, groups));
}

fn draw_system(o: &OpsDocker, s: &St) {
    let Some(e) = s.data.get("system").filter(|e| e.is_object()) else {
        ENGINE.with(|m| kit::sync_rows(m, vec![]));
        return;
    };
    let compose = ss(e, "compose");
    let rows: Vec<OpsDkKv> = [
        ("Docker", format!("{} (API {})", ss(e, "version"), ss(e, "apiVersion"))),
        ("Compose", if compose.is_empty() { "not installed".into() } else { compose }),
        ("System", ss(e, "os")), ("Kernel", ss(e, "kernel")),
        ("Machine", format!("{} CPUs · {} · {}", ss(e, "cpus"), bytes(num(e, "memory")), ss(e, "arch"))),
        ("Storage", ss(e, "storageDriver")), ("Cgroups", ss(e, "cgroup")), ("Data", ss(e, "rootDir")),
    ].into_iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| kv(k, &v)).collect();
    ENGINE.with(|m| kit::sync_rows(m, rows));
    CONTENTS.with(|m| kit::sync_rows(m, vec![
        kv("Containers", &format!("{} ({} running, {} paused, {} stopped)", ss(e, "containers"), ss(e, "running"), ss(e, "paused"), ss(e, "stopped"))),
        kv("Images", &ss(e, "images")),
    ]));
    WARNINGS.with(|m| kit::sync_rows(m, strs(e, "warnings").into_iter().map(SharedString::from).collect()));
    match &s.disk {
        None => o.set_disk_state("measuring".into()),
        Some(k) if k.get("error").is_some() => { o.set_disk_state("error".into()); o.set_disk_error(ss(k, "error").into()); }
        Some(k) => {
            o.set_disk_state("data".into());
            let row = |label: &str, key: &str, kind: &str, arm: &str, note: &str| -> Option<OpsDkDisk> {
                let x = k.get(key).filter(|x| x.is_object())?;
                if x.get("error").is_some() {
                    return Some(OpsDkDisk { label: label.into(), note: ss(x, "error").into(), error: true, count: "—".into(), size: "—".into(), reclaim: "—".into(), kind: kind.into(), arm: arm.into(), enabled: true });
                }
                let rec = kit::n(x, "reclaimable");
                Some(OpsDkDisk {
                    label: label.into(), note: note.into(), error: false, count: ss(x, "count").into(), size: bytes(num(x, "size")).into(), reclaim: bytes(num(x, "reclaimable")).into(),
                    kind: kind.into(), arm: arm.into(), enabled: rec > 0.0 || kind == "containers",
                })
            };
            let rows = [
                row("Images", "images", "images-all", "Remove every image no container uses?", "Cleaning removes every image no container uses"),
                row("Stopped containers", "containers", "containers", "Remove every stopped container?", "Their writable layers; running ones are kept"),
                row("Volumes", "volumes", "volumes", "Delete every unused volume and its data?", "Data in volumes no container uses. Deleted for good."),
                row("Build cache", "build", "build", "Clear the whole build cache?", "Rebuilt on the next build"),
            ].into_iter().flatten().collect();
            DISK.with(|m| kit::sync_rows(m, rows));
        }
    }
}

fn detail_acts(c: &Value) -> Vec<OpsDkAct> {
    let a = |verb: &str, label: &str, arm: String, kind: &str| OpsDkAct { verb: verb.into(), label: label.into(), arm: arm.into(), kind: kind.into() };
    let name = ss(c, "name");
    match ss(c, "state").as_str() {
        "running" => vec![a("stop", "Stop", format!("Stop {name}?"), "danger"), a("restart", "Restart", String::new(), ""), a("pause", "Pause", String::new(), ""), a("kill", "Kill", "Kill it now?".into(), "danger")],
        "paused" => vec![a("unpause", "Resume", String::new(), ""), a("stop", "Stop", format!("Stop {name}?"), "danger")],
        _ => vec![a("start", "Start", String::new(), "primary")],
    }
}

fn overview_rows(o: &Value) -> [Vec<OpsDkKv>; 6] {
    let rows = |r: Vec<(&str, String)>| r.into_iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| kv(k, &v)).collect::<Vec<_>>();
    let state = ss(o, "state");
    let live = state == "running" || state == "paused";
    let health = ss(o, "health");
    let (project, service) = (ss(o, "project"), ss(o, "service"));
    let rp = ss(o, "restartPolicy");
    let st = rows(vec![
        ("Status", format!("{state}{}", if health.is_empty() { String::new() } else { format!(" ({health})") })),
        ("Started", if live { fmt::when(o.get("startedAt").unwrap_or(&Value::Null)) } else { String::new() }),
        ("Finished", if !live { fmt::when(o.get("finishedAt").unwrap_or(&Value::Null)) } else { String::new() }),
        ("Exit code", if state == "exited" { format!("{}{}", ss(o, "exitCode"), if kit::truthy(o.get("oomKilled").unwrap_or(&Value::Null)) { " (out of memory)" } else { "" }) } else { String::new() }),
        ("Error", ss(o, "error")),
        ("Restarts", ss(o, "restartCount")),
        ("Restart policy", if rp.is_empty() { "no".into() } else { rp }),
        ("Created", fmt::when(o.get("created").unwrap_or(&Value::Null))),
    ]);
    let runs = rows(vec![
        ("Image", ss(o, "image")),
        ("Image id", ss(o, "imageId").replace("sha256:", "").chars().take(12).collect()),
        ("Entrypoint", ss(o, "entrypoint")), ("Command", ss(o, "cmd")), ("Working dir", ss(o, "workingDir")),
        ("User", ss(o, "user")), ("Hostname", ss(o, "hostname")),
        ("Compose", if project.is_empty() { String::new() } else { format!("{project}{}", if service.is_empty() { String::new() } else { format!(" / {service}") }) }),
        ("Id", ss(o, "id")),
    ]);
    let nets = arr(o, "networks").iter().map(|n| { let ip = ss(n, "ip"); let gw = ss(n, "gateway"); kv(&ss(n, "name"), &format!("{}{}", if ip.is_empty() { "no address".into() } else { ip }, if gw.is_empty() { String::new() } else { format!("  via {gw}") })) }).collect();
    let mounts = arr(o, "mounts").iter().map(|m| {
        let name = ss(m, "name");
        let what = if ss(m, "type") == "volume" { format!("volume {}", if name.chars().count() > 40 { format!("{}…", name.chars().take(12).collect::<String>()) } else { name }) } else { ss(m, "source") };
        kv(&ss(m, "destination"), &format!("{what}{}", if kit::truthy(m.get("rw").unwrap_or(&Value::Null)) { "" } else { "  (read-only)" }))
    }).collect();
    let env = arr(o, "env").iter().map(|e| { let hid = kit::truthy(e.get("hidden").unwrap_or(&Value::Null)); OpsDkKv { k: ss(e, "key").into(), v: if hid { "hidden".into() } else { ss(e, "value") }.into(), hid } }).collect();
    let mut labels: Vec<(String, String)> = o.get("labels").and_then(|l| l.as_object()).map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect()).unwrap_or_default();
    labels.sort();
    [st, runs, nets, mounts, env, labels.iter().map(|(k, v)| kv(k, v)).collect()]
}

fn draw_detail(o: &OpsDocker, s: &St) {
    let Some(d) = &s.detail else { o.set_d_open(false); return };
    o.set_d_open(true);
    o.set_d_tab(d.tab.clone().into());
    o.set_d_prompt(d.prompt.clone().into());
    if !d.loaded {
        o.set_d_state(if d.error.is_empty() { "loading" } else { "error" }.into());
        o.set_d_error(d.error.clone().into());
        o.set_d_name(d.id.clone().into());
        o.set_d_sub("".into());
        o.set_d_dot("".into());
        return;
    }
    o.set_d_state("data".into());
    let c = &d.data["container"];
    let ov = &d.data["overview"];
    let p = ss(c, "project");
    o.set_d_name(ss(c, "name").into());
    o.set_d_sub(format!("{} · {}{}", ss(c, "image"), ss(c, "status"), if p.is_empty() { String::new() } else { format!(" · {p}") }).into());
    o.set_d_dot(fmt::state_key(c).into());
    o.set_d_rmforce_shown(kit::truthy(c.get("running").unwrap_or(&Value::Null)) || ss(c, "state") == "paused");
    ACTS.with(|m| kit::sync_rows(m, detail_acts(c)));
    let [a, b, n, mt, e, l] = overview_rows(ov);
    o.set_ov_env_note(e.iter().any(|x| x.hid));
    OV.with(|m| {
        kit::sync_rows(&m[0], a); kit::sync_rows(&m[1], b); kit::sync_rows(&m[2], n);
        kit::sync_rows(&m[3], mt); kit::sync_rows(&m[4], e); kit::sync_rows(&m[5], l);
    });
    let ports = arr(ov, "ports").iter().map(|pt| {
        let public = ss(pt, "public");
        let has = !public.is_empty() && public != "0";
        let ip = ss(pt, "ip");
        let fws: Vec<OpsDkChip> = if has { s.fwd_list.as_ref().map(|l| l.iter().filter(|f| ss(f, "port") == public).map(fwd_chip).collect()).unwrap_or_default() } else { vec![] };
        OpsDkPort {
            port: if has { public.clone() } else { "x".into() }.into(),
            k: format!("{}/{}", ss(pt, "private"), ss(pt, "type")).into(),
            v: if has { format!("{}:{public}", if ip.is_empty() { "0.0.0.0".into() } else { ip }) } else { "not published".into() }.into(),
            btn: if has { if fws.is_empty() { "Add forward" } else { "Another" } } else { "" }.into(),
            fwds: vm(fws),
        }
    }).collect();
    PORTS.with(|m| kit::sync_rows(m, ports));
    let insp = serde_json::to_string_pretty(&d.data["inspect"]).unwrap_or_default();
    if o.get_inspect().as_str() != insp { o.set_inspect(insp.into()); }
}

// ─── The test driver ───

fn arg(a: &[String], i: usize) -> &str { a.get(i).map(|s| s.as_str()).unwrap_or("") }

pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    if !name.starts_with("docker-") { return None; }
    let ok = Some(json!({ "ok": true }));
    let w = kit::ui()?;
    let o = g(&w);
    match name {
        "docker-view" => { o.invoke_set_view(arg(args, 0).into()); ok }
        "docker-filter" => { o.set_filter(arg(args, 0).into()); o.invoke_filter_edited(arg(args, 0).into()); ok }
        "docker-refresh" => { o.invoke_refresh(); ok }
        "docker-recheck" => { o.invoke_recheck(); ok }
        "docker-open" => { o.invoke_open(arg(args, 0).into()); ok }
        "docker-back" => { o.invoke_back(); ok }
        "docker-tab" => { o.invoke_dtab(arg(args, 0).into()); ok }
        "docker-ctr" => { o.invoke_ctr(arg(args, 0).into(), arg(args, 1).into()); ok }
        "docker-prompt" => { o.invoke_prompt(arg(args, 0).into()); ok }
        "docker-rename" => { o.set_d_newname(arg(args, 0).into()); o.invoke_rename_do(); ok }
        "docker-remove" => { o.set_d_rmvol(arg(args, 0) == "true"); o.set_d_rmforce(arg(args, 1) != "false"); o.invoke_remove_do(); ok }
        "docker-log-filter" => { o.set_log_filter(arg(args, 0).into()); rebuild_logs(); ok }
        "docker-log-option" => { o.invoke_log_option(arg(args, 0).into(), arg(args, 1) == "true"); ok }
        "docker-log-copy" => { o.invoke_log_copy(); Some(json!({ "ok": true, "text": st(|s| s.copied.clone()) })) }
        "docker-log-clear" => { o.invoke_log_clear(); ok }
        "docker-inspect-copy" => { o.invoke_inspect_copy(); Some(json!({ "ok": true, "text": st(|s| s.copied.clone()) })) }
        "docker-shell-send" => { o.invoke_shell_send(arg(args, 0).into()); ok }
        "docker-shell-key" => { o.invoke_shell_key(arg(args, 0).into()); ok }
        "docker-shell-again" => { o.invoke_shell_again(); ok }
        "docker-stack" => { o.invoke_stack(arg(args, 0).into(), arg(args, 1).into()); ok }
        "docker-pull" => { o.set_pull_ref(arg(args, 0).into()); o.invoke_pull(); ok }
        "docker-prune" => { o.invoke_prune(arg(args, 0).into()); ok }
        "docker-rm" => { o.invoke_rm(arg(args, 0).into(), arg(args, 1).into()); ok }
        "docker-chip" => { o.invoke_chip(chip(arg(args, 1), "", arg(args, 0), arg(args, 1))); ok }
        "docker-fwd-add" => { o.invoke_fwd_add(arg(args, 0).into()); ok }
        // name, mode index, public, watch, port (the Forwards view's).
        "docker-fwd-fill" => {
            o.set_form_name(arg(args, 0).into());
            o.set_form_mode(arg(args, 1).parse().unwrap_or(0));
            o.set_form_public(arg(args, 2) == "true");
            o.set_form_watch(arg(args, 3) == "true");
            o.set_form_target(arg(args, 4).into());
            ok
        }
        "docker-fwd-create" => { o.invoke_fwd_create(); ok }
        "docker-fwd-cancel" => { o.invoke_fwd_cancel(); ok }
        "docker-fwd-watch" => { o.invoke_fwd_watch(arg(args, 0).into(), arg(args, 1) == "true"); ok }
        "docker-rm-fwd" => { o.invoke_rm_fwd(arg(args, 0).into()); ok }
        "docker-rm-orphans" => { o.invoke_rm_orphans(); ok }
        "docker-job-close" => { o.invoke_job_close(); ok }
        _ => None,
    }
}

/// This screen's part of the area's props (under "docker").
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return json!({}) };
    let o = g(&w);
    let strs_of = |m: ModelRc<OpsDkKv>| m.iter().map(|k| json!([k.k.as_str(), k.v.as_str()])).collect::<Vec<_>>();
    let chips = |m: &ModelRc<OpsDkChip>| m.iter().map(|c| c.text.to_string()).collect::<Vec<_>>();
    let (shown, last_url, lines) = st(|s| (s.shown, s.last_url.clone(), s.detail.as_ref().map(|d| d.lines.iter().map(|l| fmt::strip_ansi(&l.text)).collect::<Vec<_>>()).unwrap_or_default()));
    let tail: Vec<String> = lines.iter().rev().take(20).rev().cloned().collect();
    json!({
        "shown": shown,
        "gate": o.get_gate().as_str(), "gateError": o.get_gate_error().as_str(),
        "view": o.get_view().as_str(), "state": o.get_state().as_str(), "errTitle": o.get_err_title().as_str(), "errText": o.get_err_text().as_str(),
        "segs": o.get_segs().iter().map(|s| json!({ "key": s.key.as_str(), "label": s.label.as_str(), "count": s.count.as_str() })).collect::<Vec<_>>(),
        "filter": o.get_filter().as_str(), "nomatch": o.get_nomatch(),
        "ctrEmpty": o.get_ctr_empty(),
        "stats": o.get_stats().iter().map(|s| json!([s.v.as_str(), s.l.as_str()])).collect::<Vec<_>>(),
        "bay": o.get_bay().row_count(),
        "ctrs": o.get_ctrs().iter().map(|c| json!({ "id": c.id.as_str(), "name": c.name.as_str(), "state": c.state.as_str(), "act": c.act.as_str(), "chips": chips(&c.chips), "cpu": c.cpu.as_str() })).collect::<Vec<_>>(),
        "stacksEmpty": o.get_stacks_empty(),
        "stacks": o.get_stacks().iter().map(|s| json!({ "name": s.name.as_str(), "chip": s.chip.as_str(), "svcs": s.svcs.row_count() })).collect::<Vec<_>>(),
        "listHead": o.get_list_head().as_str(), "listEmpty": o.get_list_empty(),
        "rows": o.get_rows().iter().map(|r| json!({ "key": r.key.as_str(), "name": r.name.as_str(), "sub": r.sub.as_str(), "chips": chips(&r.chips), "tags": chips(&r.tags), "canRm": r.can_rm })).collect::<Vec<_>>(),
        "fwdEmpty": o.get_fwd_empty(), "orphans": o.get_orphans(),
        "groups": o.get_groups().iter().map(|g| json!({ "title": g.title.as_str(), "rows": g.rows.iter().map(|f| json!({ "host": f.host.as_str(), "sub": f.sub.as_str(), "watch": f.watch, "tags": chips(&f.tags), "chips": chips(&f.chips) })).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "form": o.get_form_port().as_str(), "formName": o.get_form_name().as_str(), "formModes": o.get_form_modes().iter().map(|m| m.to_string()).collect::<Vec<_>>(), "formHelp": o.get_form_help().as_str(),
        "engine": strs_of(o.get_engine()), "contents": strs_of(o.get_contents()),
        "diskState": o.get_disk_state().as_str(), "disk": o.get_disk().iter().map(|d| json!({ "label": d.label.as_str(), "size": d.size.as_str(), "enabled": d.enabled })).collect::<Vec<_>>(),
        "detail": if o.get_d_open() { json!({
            "state": o.get_d_state().as_str(), "error": o.get_d_error().as_str(), "name": o.get_d_name().as_str(), "sub": o.get_d_sub().as_str(), "tab": o.get_d_tab().as_str(),
            "dot": o.get_d_dot().as_str(), "prompt": o.get_d_prompt().as_str(), "newname": o.get_d_newname().as_str(), "rmforceShown": o.get_d_rmforce_shown(),
            "acts": o.get_d_acts().iter().map(|a| a.label.to_string()).collect::<Vec<_>>(),
            "ovState": strs_of(o.get_ov_state()), "ovRuns": strs_of(o.get_ov_runs()), "ovNets": strs_of(o.get_ov_nets()),
            "ovMounts": strs_of(o.get_ov_mounts()), "ovEnv": o.get_ov_env().row_count(), "ovLabels": o.get_ov_labels().row_count(),
            "ports": o.get_ov_ports().iter().map(|p| json!({ "port": p.port.as_str(), "k": p.k.as_str(), "v": p.v.as_str(), "btn": p.btn.as_str(), "fwds": chips(&p.fwds) })).collect::<Vec<_>>(),
            "logRows": o.get_logs().row_count(), "lines": lines.len(), "tail": tail,
            "logFollow": o.get_log_follow(), "logTs": o.get_log_ts(), "logWrap": o.get_log_wrap(),
            "statsState": o.get_stats_state().as_str(), "statsMsg": o.get_stats_msg().as_str(), "charts": o.get_charts().iter().map(|c| json!({ "title": c.title.as_str(), "value": c.value.as_str(), "line": !c.l1.is_empty() })).collect::<Vec<_>>(), "statsNote": o.get_stats_note().as_str(),
            "shellBar": o.get_shell_bar().as_str(), "shellBtn": o.get_shell_btn().as_str(), "shellLive": o.get_shell_live(), "shellText": o.get_shell_text().chars().rev().take(2000).collect::<String>().chars().rev().collect::<String>(),
            "inspect": o.get_inspect().len(),
        }) } else { Value::Null },
        "job": if o.get_job_open() { json!({ "title": o.get_job_title().as_str(), "done": o.get_job_done(), "ok": o.get_job_ok(), "code": o.get_job_code().as_str(), "text": o.get_job_text().chars().rev().take(1000).collect::<String>().chars().rev().collect::<String>() }) } else { Value::Null },
        "lastUrl": last_url,
    })
}

