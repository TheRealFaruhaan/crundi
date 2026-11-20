//! Docker (the Docker tab; app/vendor/crundi-docker.js, loadDockerTab and
//! checkDockerPresence in src/webapp-html.js; Slint: native-client/src/ops/docker.rs,
//! ui/ops/docker.slint, specs/ops/docker.md): the status gate, containers
//! (summary, bay plan, rows, actions), a container's detail (overview with
//! its ports' forwards, live logs over /ws, stats charts, a shell over /ws,
//! inspect; rename and remove), Compose stacks, images (pull, clean),
//! volumes, networks, Crundi's forwards matched to containers, the engine
//! and its disk use, and the job drawer for compose and pull output.
//! Pure rules: docker/fmt.rs.

mod fmt;
mod shell;

use crate::services::kit::{self, arr, s as ss, strs, t};
use crate::compat::AreaEvent;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use fmt::{bytes, num, pct};
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{collections::HashMap, rc::Rc, time::Duration};

const VIEWS: [(&str, &str, IconName); 7] = [
    ("containers", "Containers", IconName::Container), ("stacks", "Stacks", IconName::Layers), ("images", "Images", IconName::Archive),
    ("volumes", "Volumes", IconName::HardDrive), ("networks", "Networks", IconName::Network), ("forwards", "Forwards", IconName::Globe),
    ("system", "System", IconName::Server),
];
const TABS: [(&str, &str); 5] = [("overview", "Overview"), ("logs", "Logs"), ("stats", "Stats"), ("shell", "Shell"), ("inspect", "Inspect")];
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
    samples: Vec<Value>,
    stats_msg: Option<String>,
    prompt: String,
}

/// What the virtual page lists.
enum PageRows { Ctr(Vec<Ctr>), Things(String, Vec<Row>) }
impl PageRows { fn len(&self) -> usize { match self { PageRows::Ctr(r) => r.len(), PageRows::Things(_, r) => r.len() } } }

#[derive(Default)]
struct Job { id: String, title: String, text: String, next: i64, done: bool, code: i64 }

/// The inputs (made once, with the window).
struct Inputs {
    filter: Entity<w::InputState>,
    pull: Entity<w::InputState>,
    form_name: Entity<w::InputState>,
    form_target: Entity<w::InputState>,
    newname: Entity<w::InputState>,
    log_filter: Entity<w::InputState>,
}

pub struct Docker {
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
    /// The forward form: under which port ("any": the Forwards view's), its modes.
    form: Option<String>,
    form_values: Vec<String>,
    form_labels: Vec<String>,
    form_mode: usize,
    form_public: bool,
    form_watch: bool,
    form_help: String,
    detail: Option<Detail>,
    job: Option<Job>,
    loading: bool,
    refs: u64,
    last_url: String,
    copied: String,
    labels_open: bool,
    rmvol: bool,
    rmforce: bool,
    log_follow: bool,
    log_ts: bool,
    log_wrap: bool,
    timer: Option<Task<()>>,
    stats_timer: Option<Task<()>>,
    inputs: Option<Inputs>,
    log_list: ListState,
    /// What the log list shows: its rows, for the filter and Times it was made with.
    log_rows: Rc<Vec<usize>>,
    log_key: (String, usize),
    /// The shell's emulator and its two cached leaves (made with the window).
    shell: Option<Entity<shell::ShellTerm>>,
    shell_views: Option<shell::ShellViews>,
    /// The page's virtual list (containers, images, volumes, networks), its
    /// rows as last made and what they were made for (view, data version).
    page_list: ListState,
    page_rows: Rc<Option<PageRows>>,
    page_key: (String, usize),
    job_scroll: ScrollHandle,
    subs: Vec<Subscription>,
}

impl Default for Docker {
    fn default() -> Self {
        Docker {
            shown: false, connected: true, status: None, view: "containers".into(), filter: HashMap::new(), data: HashMap::new(), error: None,
            disk: None, disk_loading: false, fwd_list: None, fwd_at: 0.0, fwd_opts: None, form: None, form_values: vec![], form_labels: vec![],
            form_mode: 0, form_public: false, form_watch: false, form_help: String::new(), detail: None, job: None, loading: false, refs: 0,
            last_url: String::new(), copied: String::new(), labels_open: false, rmvol: false, rmforce: true, log_follow: true, log_ts: false, log_wrap: true,
            timer: None, stats_timer: None, inputs: None, log_list: ListState::new(0, ListAlignment::Top, T::SP_48 * 4.),
            log_rows: Rc::new(vec![]), log_key: (String::new(), usize::MAX), shell: None, shell_views: None, page_list: ListState::new(0, ListAlignment::Top, T::SP_48 * 4.), page_rows: Rc::new(None), page_key: (String::new(), usize::MAX), job_scroll: ScrollHandle::new(), subs: vec![],
        }
    }
}

struct Handle(Entity<Docker>);
impl Global for Handle {}

fn ent(cx: &mut App) -> Entity<Docker> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let e = cx.new(|_| Docker::default());
    cx.set_global(Handle(e.clone()));
    e
}

fn upd<R>(cx: &mut App, f: impl FnOnce(&mut Docker) -> R) -> R {
    let e = ent(cx);
    e.update(cx, |s, cx| { let r = f(s); cx.notify(); r })
}

/// Change what the screen does not draw (bookkeeping: no redraw).
fn hush<R>(cx: &mut App, f: impl FnOnce(&mut Docker) -> R) -> R {
    let e = ent(cx);
    e.update(cx, |s, _| f(s))
}

fn rd<R>(cx: &mut App, f: impl FnOnce(&Docker) -> R) -> R {
    let e = ent(cx);
    f(e.read(cx))
}

fn running(c: &Value) -> bool { t(c, "running") }

/// A server answer: `{ok:false, error}` as the server's words.
fn answer(r: Result<Value, String>) -> Result<Value, String> {
    match r {
        Ok(v) if v.get("ok") == Some(&Value::Bool(false)) => Err(ss(&v, "error")),
        other => other,
    }
}

fn get(cx: &mut App, path: String, done: impl FnOnce(Result<Value, String>, &mut App) + 'static) {
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| done(answer(r), cx));
}

fn post(cx: &mut App, path: &'static str, body: Value, done: impl FnOnce(Result<Value, String>, &mut App) + 'static) {
    crundi_shell::call(cx, move |api| api.post(path, &body), move |r, cx| done(answer(r), cx));
}

/// The server's words, or `d` when it gave none.
fn eo(e: &str, d: &str) -> String { if e.is_empty() { d.to_string() } else { e.to_string() } }

fn after(cx: &mut App, ms: u64, f: impl FnOnce(&mut App) + 'static) -> Task<()> {
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_millis(ms)).await;
        let _ = cx.update(f);
    })
}

fn copy(cx: &mut App, text: &str) {
    if text.is_empty() { return; }
    upd(cx, |s| s.copied = text.to_string());
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    kit::info(cx, "Copied");
}

fn open_url(cx: &mut App, url: &str) {
    let full = kit::open_url(cx, url);
    if !full.is_empty() { upd(cx, |s| s.last_url = full); }
}

// ─── The registry ───

pub fn view(tab: &str, window: &mut Window, cx: &mut App) -> Option<AnyView> {
    if tab != "docker" { return None; }
    let e = ent(cx);
    e.update(cx, |s, cx| s.ensure_inputs(window, cx));
    Some(e.into())
}

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    match e {
        AreaEvent::Connected(on) => {
            upd(cx, |s| s.connected = *on);
            if *on { on_reconnect(cx); }
        }
        AreaEvent::Ws(ty, m) if ty.starts_with("docker-") => on_message(ty, m, cx),
        _ => {}
    }
}

pub fn on_project(_alias: &str, _cx: &mut App) {}
pub fn on_signed_in(_cx: &mut App) {}

pub fn on_signed_out(cx: &mut App) {
    hide(cx);
    upd(cx, |s| {
        let (inputs, subs) = (s.inputs.take(), std::mem::take(&mut s.subs));
        *s = Docker::default();
        s.inputs = inputs;
        s.subs = subs;
    });
}

pub fn on_tab(tab: &str, cx: &mut App) {
    let was = rd(cx, |s| s.shown);
    if tab == "docker" && !was { show(cx); } else if tab != "docker" && was { hide(cx); }
}

// ─── Showing, the gate, loading ───

fn show(cx: &mut App) {
    upd(cx, |s| s.shown = true);
    get(cx, "/api/docker/status".into(), |r, cx| {
        let st = r.unwrap_or_else(|e| json!({ "installed": true, "reachable": false, "error": e }));
        upd(cx, |s| s.status = Some(st));
        if !gate_ok(cx) { return; }
        let detail = rd(cx, |s| s.detail.as_ref().map(|d| d.tab.clone()));
        if let Some(tab) = detail { select_tab(&tab, cx); refresh_head(true, cx); } else { load(true, cx); }
        schedule(cx);
    });
}

fn hide(cx: &mut App) {
    upd(cx, |s| { s.shown = false; s.timer = None; s.stats_timer = None; });
    // Logs stop following while nobody looks; a shell is left alone, since
    // whatever runs in it would die with it.
    stop_logs(cx);
}

fn gate_ok(cx: &mut App) -> bool {
    rd(cx, |s| s.status.as_ref().is_some_and(|x| t(x, "installed") && t(x, "reachable")))
}

fn recheck(cx: &mut App) { get(cx, "/api/docker/status?force=1".into(), |_, cx| show(cx)); }

fn refresh(cx: &mut App) {
    if rd(cx, |s| s.detail.is_some()) { refresh_head(true, cx); } else { load(false, cx); }
}

fn set_view(v: &str, window: &mut Window, cx: &mut App) {
    close_detail(cx);
    let (f, input) = upd(cx, |s| {
        s.view = v.to_string(); s.error = None; s.form = None; s.data.remove(v);
        (s.filter.get(v).cloned().unwrap_or_default(), s.inputs.as_ref().map(|i| i.filter.clone()))
    });
    kit::set_value(&input, &f, window, cx);
    load(true, cx);
    schedule(cx);
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
fn load(first: bool, cx: &mut App) {
    let (go, detail, v) = hush(cx, |s| {
        if !s.shown || (s.loading && !first) { return (false, false, String::new()); }
        s.loading = true;
        (true, s.detail.is_some(), s.view.clone())
    });
    if !go { return; }
    if detail { hush(cx, |s| s.loading = false); refresh_head(false, cx); return; }
    let vv = v.clone();
    get(cx, path_of(&v).into(), move |r, cx| {
        hush(cx, |s| s.loading = false);
        if rd(cx, |s| s.view != vv || s.detail.is_some()) { return; }
        match r {
            Err(e) => upd(cx, |s| s.error = Some(("Docker did not answer".into(), e))),
            Ok(d) => {
                let key = if vv == "system" { "engine" } else { vv.as_str() };
                let val = d.get(key).cloned().unwrap_or(d.clone());
                // A refresh that brings nothing new draws nothing.
                let same = rd(cx, |s| s.error.is_none() && s.data.get(&vv) == Some(&val));
                if !same { upd(cx, |s| { s.error = None; s.data.insert(vv.clone(), val); }); }
                if vv == "system" { load_disk(cx); }
                if vv == "forwards" { take_forwards(arr(&d, "forwards").into_iter().cloned().collect(), cx); }
                if vv == "containers" {
                    load_forwards(false, cx);
                    // The Stacks count follows from the container labels.
                    let mut seen: Vec<String> = arr(&d, "containers").iter().map(|c| ss(c, "project")).filter(|p| !p.is_empty()).collect();
                    seen.sort(); seen.dedup();
                    let n = rd(cx, |s| s.data.get("stacks").and_then(|x| x.as_array()).map(|a| a.len()));
                    if n != Some(seen.len()) { upd(cx, |s| { s.data.insert("stacks".into(), Value::Array(seen.iter().map(|k| json!({ "name": k })).collect())); }); }
                }
            }
        }
    });
}

fn schedule(cx: &mut App) {
    let (shown, detail, v) = rd(cx, |s| (s.shown, s.detail.is_some(), s.view.clone()));
    hush(cx, |s| s.timer = None);
    if !shown { return; }
    let every = if detail { 4000 } else { match v.as_str() { "containers" => 3000, "stacks" => 5000, "forwards" => 10000, _ => 0 } };
    if every == 0 { return; }
    let task = after(cx, every, |cx| { load(false, cx); schedule(cx); });
    hush(cx, |s| s.timer = Some(task));
}

fn load_disk(cx: &mut App) {
    if upd(cx, |s| std::mem::replace(&mut s.disk_loading, true)) { return; }
    get(cx, "/api/docker/system/df".into(), |r, cx| {
        upd(cx, |s| {
            s.disk_loading = false;
            s.disk = Some(match r { Ok(d) => d.get("disk").cloned().unwrap_or(Value::Null), Err(e) => json!({ "error": eo(&e, "Could not measure") }) });
        });
    });
}

// ─── Forwards ───

fn take_forwards(list: Vec<Value>, cx: &mut App) {
    let same = rd(cx, |s| s.fwd_list.as_ref() == Some(&list));
    if same { hush(cx, |s| s.fwd_at = fmt::now_ms()); } else { upd(cx, |s| { s.fwd_list = Some(list); s.fwd_at = fmt::now_ms(); }); }
}

fn load_forwards(force: bool, cx: &mut App) {
    let skip = hush(cx, |s| {
        if !force && s.fwd_list.is_some() && fmt::now_ms() - s.fwd_at < 15000.0 { return true; }
        s.fwd_at = fmt::now_ms();
        false
    });
    if skip { return; }
    get(cx, "/api/docker/forwards".into(), |r, cx| {
        if let Ok(d) = r { take_forwards(arr(&d, "forwards").into_iter().cloned().collect(), cx); }
    });
}

fn open_form(port: &str, window: &mut Window, cx: &mut App) {
    let blind = port == "any";
    if !blind && rd(cx, |s| s.detail.is_none()) { return; }
    if rd(cx, |s| s.fwd_opts.is_none()) {
        let port = port.to_string();
        get(cx, "/api/forwards/options".into(), move |r, cx| {
            upd(cx, |s| s.fwd_opts = Some(r.unwrap_or_else(|_| json!({ "choices": [] }))));
            kit::with_window(cx, move |window, cx| open_form(&port, window, cx));
        });
        return;
    }
    let (opts, name) = rd(cx, |s| {
        let name = if blind { String::new() } else {
            let c = s.detail.as_ref().map(|d| d.data.get("container").cloned().unwrap_or_default()).unwrap_or_default();
            let n = ss(&c, "service");
            fmt::slug(&if n.is_empty() { ss(&c, "name") } else { n })
        };
        (s.fwd_opts.clone().unwrap_or_default(), name)
    });
    let sub = arr(&opts, "choices").into_iter().find(|c| ss(c, "id") == "subdomain" && t(c, "available")).cloned();
    let (mut labels, mut values) = (Vec::new(), Vec::new());
    if let Some(sub) = &sub {
        let dom = { let d = ss(sub, "suggestedDomain"); if d.is_empty() { ss(&opts, "domain") } else { d } };
        labels.push(format!("name.{dom}")); values.push("subdomain".to_string());
    }
    labels.push("/tunnel/name/ on this server".into()); values.push("path".into());
    let help = if blind { "Forwards 127.0.0.1 on the port you give: a container\u{2019}s published port, a dev server, anything listening on this machine.".to_string() } else { format!("Forwards 127.0.0.1:{port}.") }
        + " Private unless Public is ticked: only people signed in to Crundi can open it.";
    let inputs = upd(cx, |s| {
        s.form = Some(port.to_string()); s.form_values = values; s.form_labels = labels;
        s.form_mode = 0; s.form_public = false; s.form_watch = false; s.form_help = help;
        s.inputs.as_ref().map(|i| (i.form_name.clone(), i.form_target.clone()))
    });
    if let Some((n, tg)) = inputs {
        kit::set_value(&Some(n.clone()), &name, window, cx);
        kit::set_value(&Some(tg.clone()), "", window, cx);
        let f = if blind { tg } else { n };
        f.update(cx, |s, cx| s.focus(window, cx));
    }
}

fn create_forward(cx: &mut App) {
    let Some(port) = rd(cx, |s| s.form.clone()) else { return };
    let blind = port == "any";
    let (name, target_txt) = { let e = ent(cx); let s = e.read(cx); s.inputs.as_ref().map(|i| (i.form_name.read(cx).value().to_string(), i.form_target.read(cx).value().to_string())).unwrap_or_default() };
    let target: i64 = if blind { target_txt.trim().parse().unwrap_or(0) } else { port.parse().unwrap_or(0) };
    if !(1..=65535).contains(&target) { kit::err(cx, "Give the port to forward, 1 to 65535"); return; }
    let (mode, public, watch, project, desc) = rd(cx, |s| {
        let (p, d) = match (&s.detail, blind) {
            (Some(d), false) => (ss(&d.data["overview"], "project"), format!("Container {}", ss(&d.data["container"], "name"))),
            _ => (String::new(), String::new()),
        };
        (s.form_values.get(s.form_mode).cloned().unwrap_or_else(|| "path".into()), s.form_public, s.form_watch, p, d)
    });
    let body = json!({ "name": name.trim(), "port": target, "mode": mode, "public": public, "project": project, "description": desc });
    post(cx, "/api/forwards", body, move |r, cx| match r {
        Err(e) => kit::err(cx, &eo(&e, "Could not create the forward")),
        Ok(d) => {
            let f = d.get("forward").cloned().unwrap_or_default();
            let host = ss(&f, "host");
            let done = move |cx: &mut App| {
                kit::ok(cx, &format!("Forward ready: {}", ss(&f, "url")));
                upd(cx, |s| s.form = None);
                load_forwards(true, cx);
                if blind && rd(cx, |s| s.view == "forwards" && s.detail.is_none()) { load(true, cx); }
            };
            if watch && !host.is_empty() { set_forward_watch(&host, true, true, cx, move |_, cx| done(cx)); } else { done(cx); }
        }
    });
}

fn remove_forward(host: &str, quiet: bool, cx: &mut App, then: impl FnOnce(bool, &mut App) + 'static) {
    let path = format!("/api/forwards/{}", kit::enc(host));
    let host = host.to_string();
    crundi_shell::call(cx, move |api| api.delete(&path), move |r, cx| {
        let r = answer(r);
        if let Err(e) = &r { if !quiet { kit::err(cx, &eo(e, &format!("Could not remove {host}"))); } }
        then(r.is_ok(), cx);
    });
}

fn set_forward_watch(host: &str, on: bool, quiet: bool, cx: &mut App, then: impl FnOnce(bool, &mut App) + 'static) {
    let path = format!("/api/forwards/{}", kit::enc(host));
    let host = host.to_string();
    crundi_shell::call(cx, move |api| api.patch(&path, &json!({ "watch": on })), move |r, cx| {
        let r = answer(r);
        match &r {
            Err(e) => kit::err(cx, &eo(e, "Could not change that")),
            Ok(_) if !quiet => kit::ok(cx, &if on { format!("You will be warned when {host} goes down") } else { format!("No longer watching {host}") }),
            _ => {}
        }
        then(r.is_ok(), cx);
    });
}

fn fwd_watch(host: &str, on: bool, cx: &mut App) {
    set_forward_watch(host, on, false, cx, |_, cx| { upd(cx, |s| s.fwd_at = 0.0); load(true, cx); });
}

fn rm_fwd(host: &str, cx: &mut App) {
    let h = host.to_string();
    remove_forward(host, false, cx, move |ok, cx| { if ok { kit::ok(cx, &format!("Removed {h}")); } upd(cx, |s| s.fwd_at = 0.0); load(true, cx); });
}

fn remove_orphans(cx: &mut App) {
    let orphans: Vec<String> = rd(cx, |s| s.fwd_list.clone().unwrap_or_default()).iter().filter(|f| ss(f, "state") == "orphaned").map(|f| ss(f, "host")).collect();
    fn next(mut left: Vec<String>, n: usize, cx: &mut App) {
        let Some(h) = left.pop() else {
            kit::ok(cx, &format!("Removed {n} orphaned forward{}", if n == 1 { "" } else { "s" }));
            upd(cx, |s| s.fwd_at = 0.0);
            load(true, cx);
            return;
        };
        remove_forward(&h, true, cx, move |ok, cx| next(left, n + ok as usize, cx));
    }
    next(orphans, 0, cx);
}

// ─── Actions ───

fn container_verb(id: &str, verb: &str, cx: &mut App) {
    let id = if id.is_empty() { rd(cx, |s| s.detail.as_ref().map(|d| d.id.clone()).unwrap_or_default()) } else { id.to_string() };
    let verb = verb.to_string();
    post(cx, "/api/docker/containers/action", json!({ "id": id, "action": verb }), move |r, cx| {
        match &r {
            Err(e) => kit::err(cx, &eo(e, &format!("Could not {verb} it"))),
            Ok(d) => {
                let w = match verb.as_str() { "start" => "Started ", "stop" => "Stopped ", "restart" => "Restarted ", "pause" => "Paused ", "unpause" => "Resumed ", _ => "Killed " };
                kit::ok(cx, &format!("{w}{}", ss(d, "name")));
            }
        }
        if rd(cx, |s| s.detail.as_ref().is_some_and(|d| d.id == id)) {
            let v = verb.clone();
            refresh_head_then(true, cx, move |cx| {
                if v == "start" || v == "restart" || v == "unpause" {
                    follow_logs(cx);
                    if rd(cx, |s| s.detail.as_ref().is_some_and(|d| d.tab == "shell")) { open_shell(cx); }
                }
            });
        } else {
            load(false, cx);
        }
    });
}

fn stack_action(name: &str, action: &str, cx: &mut App) {
    let title = format!("{name}: compose {action}");
    post(cx, "/api/docker/stacks/action", json!({ "name": name, "action": action }), move |r, cx| match r {
        Err(e) => kit::err(cx, &eo(&e, "Could not run it")),
        Ok(d) => watch_job(&ss(&d, "job"), &title, cx),
    });
}

fn pull(window: &mut Window, cx: &mut App) {
    let input = rd(cx, |s| s.inputs.as_ref().map(|i| i.pull.clone()));
    let r = kit::value(&input, cx).trim().to_string();
    if r.is_empty() { return; }
    kit::set_value(&input, "", window, cx);
    post(cx, "/api/docker/images/pull", json!({ "reference": r }), move |res, cx| match res {
        Err(e) => kit::err(cx, &eo(&e, "Could not pull it")),
        Ok(d) => watch_job(&ss(&d, "job"), &format!("Pull {r}"), cx),
    });
}

fn prune(kind: &str, cx: &mut App) {
    post(cx, "/api/docker/prune", json!({ "kind": kind }), |r, cx| match r {
        Err(e) => kit::err(cx, &eo(&e, "Clean-up failed")),
        Ok(d) => {
            let freed = num(&d, "reclaimed").filter(|x| *x > 0.0).map(|x| format!(", freed {}", bytes(Some(x)))).unwrap_or_default();
            kit::ok(cx, &format!("Removed {}{freed}", ss(&d, "removed")));
            upd(cx, |s| s.disk = None);
            load(true, cx);
        }
    });
}

fn remove_thing(kind: &str, key: &str, cx: &mut App) {
    let (path, body, what) = match kind {
        "images" => ("/api/docker/images/remove", json!({ "id": key }), "image"),
        "volumes" => ("/api/docker/volumes/remove", json!({ "name": key }), "volume"),
        _ => ("/api/docker/networks/remove", json!({ "id": key }), "network"),
    };
    post(cx, path, body, move |r, cx| {
        match r {
            Err(e) => kit::err(cx, &eo(&e, &format!("Could not remove the {what}"))),
            Ok(d) => kit::ok(cx, &format!("Removed {}", ss(&d, "name"))),
        }
        load(false, cx);
    });
}

fn chip_act(act: &str, target: &str, cx: &mut App) {
    match act {
        "open" => open_detail(target, cx),
        "url" => open_url(cx, target),
        _ => {}
    }
}

// ─── Jobs ───

fn watch_job(id: &str, title: &str, cx: &mut App) {
    upd(cx, |s| s.job = Some(Job { id: id.into(), title: title.into(), ..Default::default() }));
    poll_job(id.to_string(), cx);
}

fn poll_job(id: String, cx: &mut App) {
    let next = rd(cx, |s| s.job.as_ref().filter(|j| j.id == id && !j.done).map(|j| j.next));
    let Some(from) = next else { return };
    let jid = id.clone();
    get(cx, format!("/api/docker/job?id={}&from={from}", kit::enc(&id)), move |r, cx| {
        if !rd(cx, |s| s.job.as_ref().is_some_and(|j| j.id == jid)) { return; }
        let finished = upd(cx, |s| {
            let j = s.job.as_mut().unwrap();
            match &r {
                Err(_) => { j.done = true; j.code = -1; j.text.push_str("\n(lost track of this job)"); }
                Ok(d) => {
                    j.text.push_str(&ss(d, "text"));
                    j.next = kit::n(d, "next") as i64;
                    j.done = t(d, "done");
                    j.code = kit::n(d, "code") as i64;
                    if j.text.len() > 400_000 { let mut c = j.text.len() - 300_000; while !j.text.is_char_boundary(c) { c += 1; } j.text.drain(..c); }
                }
            }
            s.job_scroll.scroll_to_bottom();
            let j = s.job.as_ref().unwrap();
            j.done.then(|| (j.title.clone(), j.code))
        });
        match finished {
            Some((title, code)) => {
                if code == 0 { kit::ok(cx, &format!("{title} finished")); } else { kit::err(cx, &format!("{title} failed (exit {code})")); }
                load(false, cx);
            }
            None => after(cx, 700, move |cx| poll_job(jid, cx)).detach(),
        }
    });
}

// ─── The detail ───

fn open_detail(id: &str, cx: &mut App) {
    if id.is_empty() { return; }
    close_detail(cx);
    upd(cx, |s| { s.form = None; s.labels_open = false; s.detail = Some(Detail { id: id.into(), tab: "overview".into(), ..Default::default() }); });
    let want = id.to_string();
    get(cx, format!("/api/docker/containers/detail?id={}", kit::enc(id)), move |r, cx| {
        if !rd(cx, |s| s.detail.as_ref().is_some_and(|d| d.id == want && !d.loaded)) { return; }
        upd(cx, |s| {
            let dt = s.detail.as_mut().unwrap();
            match r {
                Err(e) => dt.error = eo(&e, "Could not open it"),
                Ok(d) => { dt.id = ss(&d["container"], "id"); dt.data = d; dt.loaded = true; }
            }
        });
        if rd(cx, |s| s.detail.as_ref().is_some_and(|d| d.loaded)) {
            let tab = rd(cx, |s| s.detail.as_ref().map(|d| d.tab.clone()).unwrap_or_default());
            select_tab(&tab, cx);
            load_forwards(false, cx);
        }
        schedule(cx);
    });
}

fn close_detail(cx: &mut App) {
    if rd(cx, |s| s.detail.is_none()) { return; }
    stop_logs(cx);
    close_shell(cx);
    upd(cx, |s| { s.stats_timer = None; s.detail = None; });
}

fn back(window: &mut Window, cx: &mut App) {
    let v = rd(cx, |s| s.view.clone());
    set_view(&v, window, cx);
}

fn select_tab(tab: &str, cx: &mut App) {
    let Some((log_ref, shell_ref)) = upd(cx, |s| s.detail.as_mut().map(|d| { d.tab = tab.to_string(); (d.log_ref.is_some(), d.shell_ref.is_some()) })) else { return };
    if tab == "logs" && !log_ref { follow_logs(cx); }
    if tab == "stats" { start_stats(cx); } else { upd(cx, |s| s.stats_timer = None); }
    if tab == "shell" && !shell_ref { open_shell(cx); }
}

fn refresh_head(force: bool, cx: &mut App) { refresh_head_then(force, cx, |_| {}); }

fn refresh_head_then(force: bool, cx: &mut App, then: impl FnOnce(&mut App) + 'static) {
    if !rd(cx, |s| s.detail.as_ref().is_some_and(|d| d.loaded && (force || d.prompt.is_empty()))) { return; }
    let id = rd(cx, |s| s.detail.as_ref().unwrap().id.clone());
    get(cx, "/api/docker/containers".into(), move |r, cx| {
        let Ok(d) = r else { return };
        if rd(cx, |s| !s.detail.as_ref().is_some_and(|x| x.id == id)) { return; }
        let c = arr(&d, "containers").into_iter().find(|x| ss(x, "id") == id || ss(x, "fullId") == id).cloned();
        let Some(c) = c else {
            let name = rd(cx, |s| ss(&s.detail.as_ref().unwrap().data["container"], "name"));
            kit::info(cx, &format!("{} is gone", if name.is_empty() { "The container".into() } else { name }));
            kit::with_window(cx, |window, cx| set_view("containers", window, cx));
            return;
        };
        let now = running(&c);
        let (was, tab, shell) = upd(cx, |s| {
            let dt = s.detail.as_mut().unwrap();
            let was = running(&dt.data["container"]);
            dt.data["container"] = c;
            (was, dt.tab.clone(), dt.shell_ref.is_some())
        });
        if !was && now && tab == "shell" && !shell { open_shell(cx); }
        then(cx);
    });
}

fn open_prompt(kind: &str, window: &mut Window, cx: &mut App) {
    let (name, input) = upd(cx, |s| {
        let name = match s.detail.as_mut() { Some(d) => { d.prompt = kind.to_string(); ss(&d.data["container"], "name") } None => String::new() };
        if kind == "remove" { s.rmvol = false; s.rmforce = true; }
        (name, s.inputs.as_ref().map(|i| i.newname.clone()))
    });
    if kind == "rename" {
        kit::set_value(&input, &name, window, cx);
        if let Some(i) = input { i.update(cx, |s, cx| { s.focus(window, cx); s.select_all(window, cx); }); }
    }
}

fn rename_do(cx: &mut App) {
    let input = rd(cx, |s| s.inputs.as_ref().map(|i| i.newname.clone()));
    let name = kit::value(&input, cx).trim().to_string();
    let Some(id) = rd(cx, |s| s.detail.as_ref().map(|d| d.id.clone())) else { return };
    post(cx, "/api/docker/containers/rename", json!({ "id": id, "name": name }), |r, cx| match r {
        Err(e) => kit::err(cx, &eo(&e, "Could not rename it")),
        Ok(d) => {
            kit::ok(cx, &format!("Renamed to {}", ss(&d, "name")));
            upd(cx, |s| if let Some(d) = s.detail.as_mut() { d.prompt.clear(); });
            refresh_head(true, cx);
        }
    });
}

fn remove_do(cx: &mut App) {
    let Some((id, live, vol, force)) = rd(cx, |s| s.detail.as_ref().map(|d| { let c = &d.data["container"]; (d.id.clone(), running(c) || ss(c, "state") == "paused", s.rmvol, s.rmforce) })) else { return };
    post(cx, "/api/docker/containers/remove", json!({ "id": id, "volumes": vol, "force": live && force }), |r, cx| match r {
        Err(e) => kit::err(cx, &eo(&e, "Could not remove it")),
        Ok(d) => { kit::ok(cx, &format!("Removed {}", ss(&d, "name"))); kit::with_window(cx, |window, cx| set_view("containers", window, cx)); }
    });
}

// ─── Logs ───

fn next_ref(p: &str, cx: &mut App) -> String { upd(cx, |s| { s.refs += 1; format!("{p}{}", s.refs) }) }

fn follow_logs(cx: &mut App) {
    let Some(id) = rd(cx, |s| s.detail.as_ref().map(|d| d.id.clone())) else { return };
    stop_logs(cx);
    upd(cx, |s| { let d = s.detail.as_mut().unwrap(); d.lines.clear(); d.partial = Default::default(); });
    if rd(cx, |s| s.connected) {
        let r = next_ref("l", cx);
        upd(cx, |s| s.detail.as_mut().unwrap().log_ref = Some(r.clone()));
        crundi_shell::ws_send(cx, &json!({ "type": "docker-logs-follow", "ref": r, "id": id, "tail": 500 }));
    } else {
        // No live socket: a one-off read so the tab is not blank.
        let want = id.clone();
        get(cx, format!("/api/docker/containers/logs?id={}&tail=500", kit::enc(&id)), move |r, cx| {
            if rd(cx, |s| !s.detail.as_ref().is_some_and(|d| d.id == want)) { return; }
            match r {
                Err(e) => sys_line(&eo(&e, "Could not read the log"), cx),
                Ok(d) => { add_frames(&[json!({ "stream": "stdout", "text": ss(&d, "text") })], cx); sys_line("Not live: the connection to the server is down.", cx); }
            }
        });
    }
}

fn stop_logs(cx: &mut App) {
    let r = upd(cx, |s| s.detail.as_mut().and_then(|d| d.log_ref.take()));
    if let Some(r) = r { crundi_shell::ws_send(cx, &json!({ "type": "docker-logs-stop", "ref": r })); }
}

fn add_frames(frames: &[Value], cx: &mut App) {
    upd(cx, |s| {
        let Some(d) = s.detail.as_mut() else { return };
        for f in frames {
            let k = if ss(f, "stream") == "stderr" { 1 } else { 0 };
            let text = format!("{}{}", d.partial[k], ss(f, "text"));
            let mut parts: Vec<&str> = text.split('\n').collect();
            d.partial[k] = parts.pop().unwrap_or("").to_string();
            for line in parts {
                let line = line.strip_suffix('\r').unwrap_or(line);
                let (ts, text) = fmt::split_ts(line);
                d.lines.push(LogRec { ts, text, err: k == 1, sys: false });
            }
        }
        if d.lines.len() > MAX_LINES { let cut = d.lines.len() - MAX_LINES; d.lines.drain(..cut); s.log_key.1 = usize::MAX; }
    });
}

fn sys_line(text: &str, cx: &mut App) {
    let rec = LogRec { text: format!("\u{2014} {text}"), sys: true, ..Default::default() };
    upd(cx, |s| if let Some(d) = s.detail.as_mut() { d.lines.push(rec); });
}

fn flush_partial(cx: &mut App) {
    let rest = upd(cx, |s| {
        let mut out = Vec::new();
        if let Some(d) = s.detail.as_mut() {
            for (k, stream) in [(0, "stdout"), (1, "stderr")] {
                if !d.partial[k].is_empty() { out.push(json!({ "stream": stream, "text": format!("{}\n", d.partial[k]) })); d.partial[k].clear(); }
            }
        }
        out
    });
    if !rest.is_empty() { add_frames(&rest, cx); }
}

fn log_filter_text(s: &Docker, cx: &App) -> String {
    s.inputs.as_ref().map(|i| i.log_filter.read(cx).value().trim().to_string()).unwrap_or_default()
}

/// The lines the log shows (the filter's; system lines always).
fn shown_lines(d: &Detail, filter: &str) -> Vec<usize> {
    let f = filter.to_lowercase();
    d.lines.iter().enumerate().filter(|(_, r)| r.sys || f.is_empty() || fmt::strip_ansi(&r.text).to_lowercase().contains(&f)).map(|(i, _)| i).collect()
}

fn log_copy(cx: &mut App) {
    let text = rd(cx, |s| s.detail.as_ref().map(|d| d.lines.iter().map(|l| format!("{}{}", if s.log_ts && !l.ts.is_empty() { format!("{} ", l.ts) } else { String::new() }, fmt::strip_ansi(&l.text))).collect::<Vec<_>>().join("\n")).unwrap_or_default());
    copy(cx, &text);
}

fn log_option(k: &str, on: bool, cx: &mut App) {
    upd(cx, |s| match k {
        // Follow ticked: to the end at once (box.scrollTop = scrollHeight).
        "follow" => { s.log_follow = on; let n = s.log_rows.len(); if on && n > 0 { s.log_list.scroll_to_reveal_item(n - 1); } }
        "ts" => s.log_ts = on,
        _ => s.log_wrap = on,
    });
}

// ─── Stats ───

fn start_stats(cx: &mut App) {
    upd(cx, |s| if let Some(d) = s.detail.as_mut() { d.samples.clear(); d.stats_msg = None; });
    stats_tick(cx);
    let task = cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(Duration::from_millis(2000)).await;
        let _ = cx.update(|cx| stats_tick(cx));
    });
    upd(cx, |s| s.stats_timer = Some(task));
}

fn stats_tick(cx: &mut App) {
    let Some(id) = rd(cx, |s| if s.shown { s.detail.as_ref().map(|d| d.id.clone()) } else { None }) else { return };
    let want = id.clone();
    get(cx, format!("/api/docker/containers/stats?id={}", kit::enc(&id)), move |r, cx| {
        upd(cx, |s| {
            let Some(d) = s.detail.as_mut().filter(|d| d.id == want) else { return };
            match r {
                Err(e) => d.stats_msg = Some(eo(&e, "No numbers")),
                Ok(v) if !t(&v, "running") => {
                    d.stats_msg = Some("The container is not running, so there is nothing to measure.".into());
                    d.samples.clear();
                }
                Ok(v) => {
                    d.stats_msg = None;
                    if let Some(x) = v.get("stats").filter(|x| !x.is_null()) { d.samples.push(x.clone()); if d.samples.len() > 120 { d.samples.remove(0); } }
                }
            }
        });
    });
}

/// One chart (Stats): its title, value and line under it, two series on a
/// shared scale, their legends.
#[derive(Clone, Default)]
struct Chart { title: String, value: String, sub: String, a: Vec<f32>, b: Vec<f32>, max: f32, leg1: String, leg2: String }

fn charts(samples: &[Value]) -> (Vec<Chart>, String) {
    let Some(last) = samples.last() else { return (vec![], String::new()) };
    let col = |k: &str| samples.iter().map(|x| num(x, k)).collect::<Vec<_>>();
    let (cpu, mem) = (col("cpuPct"), col("memory"));
    let (rx, tx, br, bw) = (fmt::rates(samples, "netRx"), fmt::rates(samples, "netTx"), fmt::rates(samples, "blkRead"), fmt::rates(samples, "blkWrite"));
    let last_of = |a: &[Option<f64>]| a.last().copied().flatten();
    let mem_limit = num(last, "memoryLimit").filter(|x| *x > 0.0);
    let mk = |title: &str, value: String, sub: String, a: &[Option<f64>], b: Option<&[Option<f64>]>, hint: f64, leg: (String, String)| Chart {
        title: title.into(), value, sub, a: fmt::series(a), b: b.map(fmt::series).unwrap_or_default(),
        max: fmt::chart_max(&[a, b.unwrap_or(&[])], hint) as f32, leg1: leg.0, leg2: leg.1,
    };
    let cpus = ss(last, "cpus");
    let list = vec![
        mk("CPU", pct(num(last, "cpuPct")), if cpus.is_empty() || cpus == "0" { String::new() } else { format!("of one core \u{b7} {cpus} available") }, &cpu, None, 100.0, Default::default()),
        mk("MEMORY", bytes(num(last, "memory")), mem_limit.map(|l| format!("of {} ({})", bytes(Some(l)), pct(num(last, "memory").map(|m| m / l * 100.0)))).unwrap_or_default(), &mem, None, 0.0, Default::default()),
        mk("NETWORK", format!("{}/s", bytes(last_of(&rx))), "in".into(), &rx, Some(&tx), 0.0,
            (format!("in {}/s \u{b7} {} total", bytes(last_of(&rx)), bytes(num(last, "netRx"))), format!("out {}/s \u{b7} {} total", bytes(last_of(&tx)), bytes(num(last, "netTx"))))),
        mk("DISK", format!("{}/s", bytes(last_of(&br))), "read".into(), &br, Some(&bw), 0.0,
            (format!("read {} total", bytes(num(last, "blkRead"))), format!("write {} total", bytes(num(last, "blkWrite"))))),
    ];
    let mins = (samples.len() as f64 * 2.0 / 60.0 * 10.0).round() / 10.0;
    (list, format!("{} processes \u{b7} a sample every 2 seconds, the last {} minutes", ss(last, "pids"), mins))
}

// ─── Shell ───

fn open_shell(cx: &mut App) {
    let Some((id, live)) = rd(cx, |s| s.detail.as_ref().map(|d| (d.id.clone(), running(&d.data["container"])))) else { return };
    close_shell(cx);
    upd(cx, |s| s.detail.as_mut().unwrap().shell_msg = None);
    if let Some(t) = rd(cx, |s| s.shell.clone()) { t.update(cx, |t, cx| t.clear(cx)); }
    if !live { return; }
    if !rd(cx, |s| s.connected) {
        upd(cx, |s| s.detail.as_mut().unwrap().shell_msg = Some("No connection to the server right now.".into()));
        return;
    }
    let r = next_ref("s", cx);
    upd(cx, |s| s.detail.as_mut().unwrap().shell_ref = Some(r.clone()));
    let t = shell_term(cx);
    t.update(cx, |t, cx| t.open(r, id, cx));
}

fn close_shell(cx: &mut App) {
    let r = upd(cx, |s| s.detail.as_mut().and_then(|d| d.shell_ref.take()));
    if let Some(t) = rd(cx, |s| s.shell.clone()) { t.update(cx, |t, cx| { t.close(); cx.notify(); }); }
    if let Some(r) = r { crundi_shell::ws_send(cx, &json!({ "type": "docker-shell-close", "ref": r })); }
}

/// The shell's emulator (one for the tab, made on first use).
fn shell_term(cx: &mut App) -> Entity<shell::ShellTerm> {
    if let Some(t) = rd(cx, |s| s.shell.clone()) { return t; }
    let t = cx.new(|_| shell::ShellTerm::new());
    hush(cx, |s| s.shell = Some(t.clone()));
    t
}

/// The shell's end: the screen keeps what it showed; nothing more is sent.
fn shell_ended(msg: String, cx: &mut App) {
    upd(cx, |s| { let d = s.detail.as_mut().unwrap(); d.shell_ref = None; d.shell_msg = Some(msg); });
    if let Some(t) = rd(cx, |s| s.shell.clone()) { t.update(cx, |t, cx| { t.close(); cx.notify(); }); }
}

fn shell_input(data: &str, cx: &mut App) {
    if let Some(t) = rd(cx, |s| s.shell.clone()) { t.read(cx).input(data, cx); }
}

fn shell_key(k: &str, cx: &mut App) {
    let data = match k {
        "ctrl-c" => "\u{3}".to_string(),
        "ctrl-d" => "\u{4}".to_string(),
        "ctrl-l" => "\u{c}".to_string(),
        "ctrl-z" => "\u{1a}".to_string(),
        "up" => "\u{1b}[A".to_string(),
        "down" => "\u{1b}[B".to_string(),
        _ => match k.strip_prefix("tab:") { Some(t) => format!("{t}\t"), None => return },
    };
    shell_input(&data, cx);
}

/// The shell's bar and button: (words, "New shell" / "Open again" / "").
fn shell_bar(d: &Detail) -> (String, &'static str) {
    let live = running(&d.data["container"]);
    match (&d.shell_msg, live) {
        (Some(m), r) => (m.clone(), if r { "Open again" } else { "" }),
        (None, false) => ("Start the container to open a shell in it.".to_string(), ""),
        (None, true) => (format!("A shell inside {} (bash when the image has it, else sh)", ss(&d.data["container"], "name")), "New shell"),
    }
}

// ─── Socket messages ───

fn on_message(ty: &str, m: &Value, cx: &mut App) {
    let r = ss(m, "ref");
    let (log_ref, shell_ref) = rd(cx, |s| s.detail.as_ref().map(|d| (d.log_ref.clone(), d.shell_ref.clone())).unwrap_or_default());
    match ty {
        "docker-logs" if log_ref.as_deref() == Some(&r) => add_frames(&arr(m, "frames").into_iter().cloned().collect::<Vec<_>>(), cx),
        "docker-logs-end" if log_ref.as_deref() == Some(&r) => {
            let live = upd(cx, |s| { let d = s.detail.as_mut().unwrap(); d.log_ref = None; running(&d.data["container"]) });
            flush_partial(cx);
            sys_line(if live { "The log stream ended." } else { "The container stopped. Start it to follow the log again." }, cx);
        }
        "docker-logs-error" if log_ref.as_deref() == Some(&r) => {
            upd(cx, |s| s.detail.as_mut().unwrap().log_ref = None);
            sys_line(&eo(&ss(m, "error"), "Could not follow the log"), cx);
        }
        "docker-shell-data" if shell_ref.as_deref() == Some(&r) => {
            // Into the emulator alone: the screen redraws, the tab does not.
            let t = shell_term(cx);
            let data = ss(m, "data");
            t.update(cx, |t, cx| t.output(&data, cx));
        }
        "docker-shell-exit" if shell_ref.as_deref() == Some(&r) => {
            let code = ss(m, "code");
            let msg = if code == "126" || code == "127" { format!("This image may not include a shell (exit {code}).") } else { format!("The shell ended (exit {code}).") };
            shell_ended(msg, cx);
        }
        "docker-shell-error" if shell_ref.as_deref() == Some(&r) => {
            shell_ended(eo(&ss(m, "error"), "Could not open a shell"), cx);
        }
        _ => {}
    }
}

fn on_reconnect(cx: &mut App) {
    let Some((log, tab, shell)) = rd(cx, |s| if s.shown { s.detail.as_ref().map(|d| (d.log_ref.is_some(), d.tab.clone(), d.shell_ref.is_some())) } else { None }) else { return };
    // The server dropped our streams with the old socket.
    if log || tab == "logs" { follow_logs(cx); }
    if shell {
        shell_ended("The connection dropped, which ended the shell.".into(), cx);
    }
}

// ─── What each view shows (for drawing and for the driver) ───

/// A chip (.dk-chip): its text, tone ("", ok, warn, bad, fwd), what a click
/// does ("" nothing, "open" a container by name, "url" a link, "watch"
/// nothing: the watched mark), its tooltip.
#[derive(Clone, Default)]
struct Chip { text: String, tone: &'static str, act: &'static str, target: String, tip: String }

fn chip(text: &str, tone: &'static str, act: &'static str, target: &str) -> Chip {
    Chip { text: text.into(), tone, act, target: target.into(), tip: String::new() }
}

fn fwd_chip(f: &Value) -> Chip {
    let mut c = chip(&ss(f, "host"), "fwd", "url", &ss(f, "url"));
    c.tip = format!("Forward: {}{}", ss(f, "url"), if t(f, "public") { " (public)" } else { "" });
    c
}

struct Ctr { id: String, name: String, sub: String, state: String, chips: Vec<Chip>, cpu: String, mem: String, act: &'static str }
struct Bay { id: String, state: String, tip: String }
struct Svc { id: String, state: String, name: String, sub: String, img: String, status: String }
struct Stack { name: String, chip: String, tone: &'static str, place: String, files: bool, live: bool, svcs: Vec<Svc> }
struct Row { key: String, name: String, tip: String, tags: Vec<Chip>, sub: String, chips: Vec<Chip>, can_rm: bool, arm: &'static str }
struct Fwd { host: String, sub: String, tags: Vec<Chip>, chips: Vec<Chip>, watch: bool }
struct Group { title: String, help: &'static str, rows: Vec<Fwd> }
struct Disk { label: &'static str, note: String, count: String, size: String, reclaim: String, kind: &'static str, arm: &'static str, enabled: bool }
struct Act { verb: &'static str, label: &'static str, arm: String, kind: &'static str }
struct Port { port: String, k: String, v: String, fwds: Vec<Chip>, btn: &'static str }

fn kv(k: &str, v: &str) -> (String, String) { (k.to_string(), v.to_string()) }

impl Docker {
    fn cur_filter(&self) -> String { self.filter.get(&self.view).cloned().unwrap_or_default() }

    fn gate(&self) -> &'static str {
        match &self.status {
            None => "loading",
            Some(x) if !t(x, "installed") => "absent",
            Some(x) if !t(x, "reachable") => "unreachable",
            _ => "ok",
        }
    }

    fn state(&self) -> &'static str {
        if self.error.is_some() { "error" } else if self.data.contains_key(&self.view) || (self.view == "forwards" && self.fwd_list.is_some()) { "data" } else { "loading" }
    }

    fn count(&self, k: &str) -> String {
        if k == "forwards" { return self.fwd_list.as_ref().map(|l| l.len().to_string()).unwrap_or_default(); }
        self.data.get(k).and_then(|x| x.as_array()).map(|a| a.len().to_string()).unwrap_or_default()
    }

    fn list(&self, k: &str) -> Vec<Value> { self.data.get(k).and_then(|x| x.as_array()).cloned().unwrap_or_default() }

    /// Containers: the summary, the bay, the rows, nothing matching.
    fn containers(&self) -> (Vec<(String, String)>, Vec<Bay>, Vec<Ctr>, bool) {
        let list = self.list("containers");
        let f = self.cur_filter();
        let (mut run, mut paused, mut stopped, mut cpu, mut mem) = (0, 0, 0, 0.0, 0.0);
        for c in &list {
            match ss(c, "state").as_str() { "running" => run += 1, "paused" => paused += 1, _ => stopped += 1 }
            if let Some(x) = c.get("stats").filter(|x| x.is_object()) { cpu += num(x, "cpuPct").unwrap_or(0.0); mem += num(x, "memory").unwrap_or(0.0); }
        }
        let stats = vec![kv(&run.to_string(), "RUNNING"), kv(&paused.to_string(), "PAUSED"), kv(&stopped.to_string(), "STOPPED"), kv(&pct(Some(cpu)), "CPU, OF ONE CORE"), kv(&bytes(Some(mem)), "MEMORY")];
        let bay = list.iter().map(|c| Bay { id: ss(c, "id"), state: fmt::state_key(c), tip: format!("{} \u{2014} {}", ss(c, "name"), ss(c, "status")) }).collect();
        let shown: Vec<&Value> = list.iter().filter(|c| fmt::matches(&f, &[ss(c, "name"), ss(c, "image"), ss(c, "project"), ss(c, "service"), ss(c, "status"), ss(c, "state"), ss(c, "id")])).collect();
        let nomatch = !list.is_empty() && shown.is_empty();
        let rows = shown.iter().map(|c| {
            let mut chips = Vec::new();
            let p = ss(c, "project");
            if !p.is_empty() { chips.push(chip(&p, "", "", "")); }
            for port in arr(c, "ports").into_iter().filter(|p| kit::n(p, "public") > 0.0).take(3) {
                chips.push(chip(&fmt::port_text(port), "", "", ""));
                if let Some(l) = &self.fwd_list { for fw in l.iter().filter(|fw| ss(fw, "port") == ss(port, "public")) { chips.push(fwd_chip(fw)); } }
            }
            let h = ss(c, "health");
            if !h.is_empty() { chips.push(chip(&h, if h == "healthy" { "ok" } else if h == "unhealthy" { "bad" } else { "warn" }, "", "")); }
            let stt = c.get("stats").filter(|x| x.is_object());
            Ctr {
                id: ss(c, "id"), name: ss(c, "name"), sub: format!("{} \u{b7} {}", ss(c, "image"), ss(c, "status")), state: fmt::state_key(c), chips,
                cpu: stt.map(|x| pct(num(x, "cpuPct"))).unwrap_or_default(), mem: stt.map(|x| bytes(num(x, "memory"))).unwrap_or_default(),
                act: if running(c) { "stop" } else if ss(c, "state") == "paused" { "unpause" } else { "start" },
            }
        }).collect();
        (stats, bay, rows, nomatch)
    }

    /// Stacks: (empty, the stacks, nothing matching).
    fn stacks(&self) -> (bool, Vec<Stack>, bool) {
        let list = self.list("stacks");
        let full = list.first().is_some_and(|x| x.get("containers").is_some());
        if list.is_empty() || !full { return (true, vec![], false); }
        let f = self.cur_filter();
        let shown: Vec<&Value> = list.iter().filter(|st| {
            let mut parts = vec![ss(st, "name"), ss(st, "directory")];
            parts.extend(arr(st, "containers").iter().map(|c| format!("{} {}", ss(c, "service"), ss(c, "name"))));
            fmt::matches(&f, &parts)
        }).collect();
        let rows: Vec<Stack> = shown.iter().map(|st| {
            let (live, total) = (kit::n(st, "live") as i64, kit::n(st, "total") as i64);
            let files = strs(st, "files");
            let present = t(st, "filesPresent");
            let mut place = if files.is_empty() { "compose file unknown".to_string() } else { files.join(", ") };
            if !files.is_empty() && !present { place.push_str("  (not found on this machine)"); }
            let svcs = arr(st, "containers").iter().map(|c| {
                let pubs: Vec<String> = arr(c, "ports").into_iter().filter(|p| kit::n(p, "public") > 0.0).take(2).map(fmt::port_text).collect();
                let svc = ss(c, "service");
                Svc { id: ss(c, "id"), state: fmt::state_key(c), name: if svc.is_empty() { ss(c, "name") } else { svc }, sub: ss(c, "name"),
                    img: format!("{}{}", ss(c, "image"), if pubs.is_empty() { String::new() } else { format!(" \u{b7} {}", pubs.join("  ")) }), status: ss(c, "status") }
            }).collect();
            Stack { name: ss(st, "name"), chip: format!("{live}/{total} running"), tone: if live == total { "ok" } else if live == 0 { "" } else { "warn" }, place, files: present, live: live > 0, svcs }
        }).collect();
        let nomatch = rows.is_empty();
        (false, rows, nomatch)
    }

    /// Images, volumes, networks: (the count line, empty, the rows, nothing matching).
    fn things(&self) -> (String, bool, Vec<Row>, bool) {
        let list = self.list(&self.view);
        let f = self.cur_filter();
        let used = |x: &Value| strs(x, "usedBy");
        let open_chips = |names: &[String], most: usize| {
            let mut c: Vec<Chip> = names.iter().take(most).map(|n| chip(n, "ok", "open", n)).collect();
            if names.len() > most { c.push(chip(&format!("+{}", names.len() - most), "", "", "")); }
            c
        };
        let (head, rows): (String, Vec<Row>) = match self.view.as_str() {
            "images" => {
                let in_use = list.iter().filter(|i| !used(i).is_empty()).count();
                let total: f64 = list.iter().map(|i| kit::n(i, "size")).sum();
                let rows = list.iter().filter(|i| fmt::matches(&f, &[ss(i, "name"), strs(i, "tags").join(" "), ss(i, "id"), used(i).join(" ")])).map(|i| {
                    let u = used(i);
                    let extra: Vec<String> = strs(i, "tags").into_iter().skip(1).collect();
                    let chips = if !u.is_empty() { open_chips(&u, 4) } else if t(i, "dangling") { vec![chip("untagged", "warn", "", "")] } else { vec![chip("unused", "", "", "")] };
                    Row { key: ss(i, "fullId"), name: ss(i, "name"), tip: String::new(), tags: vec![],
                        sub: format!("{} \u{b7} {} \u{b7} {}{}", ss(i, "id"), bytes(num(i, "size")), fmt::ago(i.get("created").unwrap_or(&Value::Null)), if extra.is_empty() { String::new() } else { format!(" \u{b7} also {}", extra.join(", ")) }),
                        chips, can_rm: u.is_empty(), arm: if u.is_empty() { "Remove?" } else { "" } }
                }).collect();
                (format!("{} images, {in_use} in use \u{b7} {}", list.len(), bytes(Some(total))), rows)
            }
            "volumes" => {
                let unused = list.iter().filter(|v| used(v).is_empty()).count();
                let rows = list.iter().filter(|v| fmt::matches(&f, &[ss(v, "name"), ss(v, "project"), ss(v, "driver"), used(v).join(" ")])).map(|v| {
                    let u = used(v);
                    let name = ss(v, "name");
                    let shown = if t(v, "anonymous") { format!("anonymous {}", name.chars().take(12).collect::<String>()) } else { name.clone() };
                    let p = ss(v, "project");
                    Row { key: name.clone(), name: shown, tip: name, tags: vec![],
                        sub: format!("{}{} \u{b7} {}", ss(v, "driver"), if p.is_empty() { String::new() } else { format!(" \u{b7} {p}") }, fmt::ago(v.get("created").unwrap_or(&Value::Null))),
                        chips: if u.is_empty() { vec![chip("unused", "", "", "")] } else { open_chips(&u, usize::MAX) },
                        can_rm: u.is_empty(), arm: if u.is_empty() { "Delete its data?" } else { "" } }
                }).collect();
                (format!("{} volumes, {unused} not used by any container. Sizes are under System.", list.len()), rows)
            }
            _ => {
                let rows = list.iter().filter(|n| fmt::matches(&f, &[ss(n, "name"), ss(n, "driver"), ss(n, "subnet"), ss(n, "project"), used(n).join(" ")])).map(|n| {
                    let u = used(n);
                    let builtin = t(n, "builtin");
                    let mut tags = Vec::new();
                    if builtin { tags.push(chip("built in", "", "", "")); }
                    if t(n, "internal") { tags.push(chip("internal", "", "", "")); }
                    let (sub, gw) = (ss(n, "subnet"), ss(n, "gateway"));
                    let can = !builtin && u.is_empty();
                    Row { key: ss(n, "fullId"), name: ss(n, "name"), tip: String::new(), tags,
                        sub: format!("{} \u{b7} {}{}{}", ss(n, "driver"), ss(n, "scope"), if sub.is_empty() { String::new() } else { format!(" \u{b7} {sub}") }, if gw.is_empty() { String::new() } else { format!(" via {gw}") }),
                        chips: open_chips(&u, 6), can_rm: can, arm: if can { "Remove?" } else { "" } }
                }).collect();
                (format!("{} networks", list.len()), rows)
            }
        };
        let empty = list.is_empty() && self.view != "networks";
        let nomatch = rows.is_empty() && (!list.is_empty() || self.view == "networks");
        (head, empty, rows, nomatch)
    }

    /// Forwards: (the count line, empty, orphans, the groups, nothing matching).
    fn forwards(&self) -> (String, bool, usize, Vec<Group>, bool) {
        const GROUPS: [(&str, &str, &str); 4] = [
            ("orphaned", "Nothing behind it", "No container publishes this port and nothing answers on it. Left over from something that is gone, most likely."),
            ("stopped", "Container stopped", "The container that publishes this port is not running."),
            ("container", "Running container", ""),
            ("other", "Another program", "Something on this machine answers on the port, but not a container."),
        ];
        let list = self.fwd_list.clone().unwrap_or_default();
        let f = self.cur_filter();
        let orphans = list.iter().filter(|x| ss(x, "state") == "orphaned").count();
        let head = format!("{} forwards{}", list.len(), if orphans > 0 { format!(", {orphans} with nothing behind them") } else { String::new() });
        let shown: Vec<&Value> = list.iter().filter(|fw| {
            let c = fw.get("container").map(|c| ss(c, "name")).unwrap_or_default();
            fmt::matches(&f, &[ss(fw, "host"), ss(fw, "url"), ss(fw, "port"), ss(fw, "project"), c, ss(fw, "state")])
        }).collect();
        let nomatch = !list.is_empty() && shown.is_empty();
        let groups = GROUPS.iter().filter_map(|(state, title, help)| {
            let rows: Vec<Fwd> = shown.iter().filter(|fw| ss(fw, "state") == *state).map(|fw| {
                let mut tags = Vec::new();
                if t(fw, "public") { tags.push(chip("public", "warn", "", "")); }
                let watch = t(fw, "watch");
                if watch { tags.push(Chip { tip: "You are warned when this goes down".into(), ..chip("watched", "ok", "watch", "") }); }
                let mut chips = Vec::new();
                if let Some(c) = fw.get("container").filter(|c| c.is_object()) {
                    let live = running(c);
                    let h = ss(c, "health");
                    let tone = if h == "unhealthy" { "bad" } else if live { "ok" } else { "" };
                    let mut ch = chip(&format!("{}{}", ss(c, "name"), if h == "unhealthy" { " \u{b7} unhealthy" } else { "" }), tone, "open", &ss(c, "name"));
                    ch.tip = if live { format!("Running{}", if h.is_empty() { String::new() } else { format!(", {h}") }) } else { "Stopped".into() };
                    chips.push(ch);
                }
                chips.push(chip("Open", "fwd", "url", &ss(fw, "url")));
                let (p, d) = (ss(fw, "project"), ss(fw, "description"));
                Fwd {
                    host: ss(fw, "host"),
                    sub: format!("port {} \u{b7} {}{}{}", ss(fw, "port"), if ss(fw, "mode") == "path" { "path" } else { "subdomain" }, if p.is_empty() { String::new() } else { format!(" \u{b7} {p}") }, if d.is_empty() { String::new() } else { format!(" \u{b7} {d}") }),
                    tags, chips, watch,
                }
            }).collect();
            if rows.is_empty() { None } else { Some(Group { title: format!("{} \u{b7} {}", title.to_uppercase(), rows.len()), help, rows }) }
        }).collect();
        (head, list.is_empty(), orphans, groups, nomatch)
    }

    /// System: the engine, the contents, the warnings, the disk's state and rows.
    fn system(&self) -> (Vec<(String, String)>, Vec<(String, String)>, Vec<String>, (&'static str, String), Vec<Disk>) {
        let Some(e) = self.data.get("system").filter(|e| e.is_object()) else { return (vec![], vec![], vec![], ("measuring", String::new()), vec![]) };
        let compose = ss(e, "compose");
        let engine = [
            ("Docker", format!("{} (API {})", ss(e, "version"), ss(e, "apiVersion"))),
            ("Compose", if compose.is_empty() { "not installed".into() } else { compose }),
            ("System", ss(e, "os")), ("Kernel", ss(e, "kernel")),
            ("Machine", format!("{} CPUs \u{b7} {} \u{b7} {}", ss(e, "cpus"), bytes(num(e, "memory")), ss(e, "arch"))),
            ("Storage", ss(e, "storageDriver")), ("Cgroups", ss(e, "cgroup")), ("Data", ss(e, "rootDir")),
        ].into_iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| kv(k, &v)).collect();
        let contents = vec![
            kv("Containers", &format!("{} ({} running, {} paused, {} stopped)", ss(e, "containers"), ss(e, "running"), ss(e, "paused"), ss(e, "stopped"))),
            kv("Images", &ss(e, "images")),
        ];
        let (state, rows) = match &self.disk {
            None => (("measuring", String::new()), vec![]),
            Some(k) if k.get("error").is_some() => (("error", ss(k, "error")), vec![]),
            Some(k) => {
                let row = |label: &'static str, key: &str, kind: &'static str, arm: &'static str, note: &str| -> Option<Disk> {
                    let x = k.get(key).filter(|x| x.is_object())?;
                    if x.get("error").is_some() {
                        return Some(Disk { label, note: ss(x, "error"), count: "\u{2014}".into(), size: "\u{2014}".into(), reclaim: "\u{2014}".into(), kind, arm, enabled: true });
                    }
                    Some(Disk { label, note: note.into(), count: ss(x, "count"), size: bytes(num(x, "size")), reclaim: bytes(num(x, "reclaimable")), kind, arm, enabled: kit::n(x, "reclaimable") > 0.0 || kind == "containers" })
                };
                (("data", String::new()), [
                    row("Images", "images", "images-all", "Remove every image no container uses?", "Cleaning removes every image no container uses"),
                    row("Stopped containers", "containers", "containers", "Remove every stopped container?", "Their writable layers; running ones are kept"),
                    row("Volumes", "volumes", "volumes", "Delete every unused volume and its data?", "Data in volumes no container uses. Deleted for good."),
                    row("Build cache", "build", "build", "Clear the whole build cache?", "Rebuilt on the next build"),
                ].into_iter().flatten().collect())
            }
        };
        (engine, contents, strs(e, "warnings"), state, rows)
    }
}

fn detail_acts(c: &Value) -> Vec<Act> {
    let a = |verb, label, arm: String, kind| Act { verb, label, arm, kind };
    let name = ss(c, "name");
    match ss(c, "state").as_str() {
        "running" => vec![a("stop", "Stop", format!("Stop {name}?"), "danger"), a("restart", "Restart", String::new(), ""), a("pause", "Pause", String::new(), ""), a("kill", "Kill", "Kill it now?".into(), "danger")],
        "paused" => vec![a("unpause", "Resume", String::new(), ""), a("stop", "Stop", format!("Stop {name}?"), "danger")],
        _ => vec![a("start", "Start", String::new(), "primary")],
    }
}

/// The overview's sections: state, runs, networks, mounts, environment (with hidden), labels.
fn overview_rows(o: &Value) -> [Vec<(String, String, bool)>; 6] {
    let rows = |r: Vec<(&str, String)>| r.into_iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| (k.to_string(), v, false)).collect::<Vec<_>>();
    let state = ss(o, "state");
    let live = state == "running" || state == "paused";
    let health = ss(o, "health");
    let (project, service) = (ss(o, "project"), ss(o, "service"));
    let rp = ss(o, "restartPolicy");
    let st = rows(vec![
        ("Status", format!("{state}{}", if health.is_empty() { String::new() } else { format!(" ({health})") })),
        ("Started", if live { fmt::when(o.get("startedAt").unwrap_or(&Value::Null)) } else { String::new() }),
        ("Finished", if !live { fmt::when(o.get("finishedAt").unwrap_or(&Value::Null)) } else { String::new() }),
        ("Exit code", if state == "exited" { format!("{}{}", ss(o, "exitCode"), if t(o, "oomKilled") { " (out of memory)" } else { "" }) } else { String::new() }),
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
    let nets = arr(o, "networks").iter().map(|n| { let ip = ss(n, "ip"); let gw = ss(n, "gateway"); (ss(n, "name"), format!("{}{}", if ip.is_empty() { "no address".into() } else { ip }, if gw.is_empty() { String::new() } else { format!("  via {gw}") }), false) }).collect();
    let mounts = arr(o, "mounts").iter().map(|m| {
        let name = ss(m, "name");
        let what = if ss(m, "type") == "volume" { format!("volume {}", if name.chars().count() > 40 { format!("{}\u{2026}", name.chars().take(12).collect::<String>()) } else { name }) } else { ss(m, "source") };
        (ss(m, "destination"), format!("{what}{}", if t(m, "rw") { "" } else { "  (read-only)" }), false)
    }).collect();
    let env = arr(o, "env").iter().map(|e| { let hid = t(e, "hidden"); (ss(e, "key"), if hid { "hidden".into() } else { ss(e, "value") }, hid) }).collect();
    let mut labels: Vec<(String, String, bool)> = o.get("labels").and_then(|l| l.as_object()).map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string(), false)).collect()).unwrap_or_default();
    labels.sort();
    [st, runs, nets, mounts, env, labels]
}

fn ports(ov: &Value, fwd_list: Option<&Vec<Value>>) -> Vec<Port> {
    arr(ov, "ports").iter().map(|pt| {
        let public = ss(pt, "public");
        let has = !public.is_empty() && public != "0";
        let ip = ss(pt, "ip");
        let fwds: Vec<Chip> = if has { fwd_list.map(|l| l.iter().filter(|f| ss(f, "port") == public).map(fwd_chip).collect()).unwrap_or_default() } else { vec![] };
        Port {
            port: if has { public.clone() } else { "x".into() },
            k: format!("{}/{}", ss(pt, "private"), ss(pt, "type")),
            v: if has { format!("{}:{public}", if ip.is_empty() { "0.0.0.0".into() } else { ip }) } else { "not published".into() },
            btn: if has { if fwds.is_empty() { "Add forward" } else { "Another" } } else { "" },
            fwds,
        }
    }).collect()
}

// ─── Drawing ───

fn state_ink(s: &str) -> Hsla {
    match s { "running" => T::green(), "paused" | "unhealthy" => T::yellow(), "restarting" => T::accent(), "dead" => T::red(), _ => T::text_muted() }
}

/// A state dot with its ring (.dk-dot).
fn dk_dot(state: &str) -> Div {
    let ring = match state { "running" => T::green_dim(), "paused" | "unhealthy" => T::yellow_dim(), _ => gpui::transparent_black() };
    div().flex_none().size(T::SP_8 + T::SP_6).rounded_full().bg(ring).flex().items_center().justify_center().child(w::dot(state_ink(state)))
}

fn quiet(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_12).text_color(T::text_muted()).child(text.into()) }
fn group_title(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_10).text_color(T::text_muted()).child(text.into()) }
fn mono_sub(text: impl Into<SharedString>) -> Div { div().truncate().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(text.into()) }

/// A centred empty state: an icon, a title, a hint (.dk-empty).
fn dk_empty(icon: IconName, title: impl Into<SharedString>, hint: impl Into<SharedString>) -> Div {
    let hint: SharedString = hint.into();
    div().flex().flex_col().items_center().gap(T::SP_8).py(T::SP_40).px(T::SP_24)
        .child(div().text_color(T::text_muted()).opacity(0.4).child(w::icon(icon, T::SP_40)))
        .child(div().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(title.into()))
        .when(!hint.is_empty(), |d| d.child(div().text_size(T::TEXT_13).text_color(T::text_muted()).child(hint)))
}

/// A bordered list of rows (.dk-list).
fn dk_list() -> Div { div().flex().flex_col().rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_card()).overflow_hidden() }

/// A titled section box (.dk-sec).
fn dk_sec(title: &str) -> Div {
    div().flex().flex_col().min_w_0().overflow_hidden().gap(T::SP_6).p(T::SP_12).rounded(T::R_FIELD).bg(T::bg_secondary()).border_1().border_color(T::border())
        .when(!title.is_empty(), |d| d.child(div().text_size(T::TEXT_10).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child(title.to_string())))
}

fn dk_kv(k: &str, v: &str, hid: bool) -> Div {
    div().flex().gap(T::SP_8).text_size(T::TEXT_12)
        .child(div().flex_none().w(T::DOCKER_KV_W).text_color(T::text_muted()).child(k.to_string()))
        .child(div().flex_1().min_w_0().overflow_hidden().font_family(T::MONO).text_color(if hid { T::text_muted() } else { T::text() }).child(v.to_string()))
}

fn row_frame(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).flex().items_center().gap(T::SP_12).min_h(T::CONTROL + T::SP_8).px(T::SP_12).py(T::SP_8)
}

fn chip_el(id: impl Into<ElementId>, c: &Chip) -> AnyElement {
    let tone = match c.tone { "ok" => "ok", "warn" => "warn", "bad" => "error", "fwd" => "accent", _ => "" };
    let mut label = c.text.clone();
    if c.tone == "fwd" { label = format!("\u{2197} {label}"); }
    // The watched mark carries its bell (icon('bell') + 'watched').
    let el = div().id(id).flex_none().child(if c.act == "watch" { w::icon_tag(IconName::Bell, label, tone) } else { w::tag(label, tone) });
    let el = w::tip(el, c.tip.clone());
    if c.act == "open" || c.act == "url" {
        let (a, tg) = (c.act, c.target.clone());
        el.cursor_pointer().on_click(move |_, _, cx| { cx.stop_propagation(); chip_act(a, &tg, cx); }).into_any_element()
    } else { el.into_any_element() }
}

fn chips_el(prefix: &str, i: usize, chips: &[Chip]) -> Div {
    let p = SharedString::from(prefix.to_string());
    div().flex().flex_wrap().items_center().gap(T::SP_4).children(chips.iter().enumerate().map(|(j, c)| chip_el((p.clone(), i * 64 + j), c)))
}

fn num_col(width: Pixels, v: String, l: &'static str) -> Div {
    div().flex_none().w(width).flex().flex_col().items_end()
        .child(div().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text_secondary()).child(v.clone()))
        .when(!v.is_empty(), |d| d.child(div().text_size(T::TEXT_10).text_color(T::text_muted()).child(l)))
}

fn ansi_ink(c: u8) -> Hsla {
    match c { 30 => T::text_muted(), 31 => T::docker_red(), 32 => T::green(), 33 => T::amber(), 34 => T::accent_hover(), 35 => T::docker_magenta(), 36 => T::docker_cyan(), _ => T::text() }
}

/// A log line: its time (Times), its pieces in their colours, the matches marked.
fn log_line(rec: &LogRec, filter: &str, ts: bool, wrap: bool) -> AnyElement {
    let base = if rec.err { T::error_text() } else { T::text_secondary() };
    let mut text = String::new();
    let mut hl: Vec<(std::ops::Range<usize>, HighlightStyle)> = vec![];
    if rec.sys {
        text = rec.text.clone();
        hl.push((0..text.len(), HighlightStyle { color: Some(T::text_muted()), font_style: Some(FontStyle::Italic), ..Default::default() }));
    } else {
        if ts && !rec.ts.is_empty() {
            let t = format!("{}  ", fmt::ts_show(&rec.ts));
            hl.push((0..t.len(), HighlightStyle { color: Some(T::text_muted()), ..Default::default() }));
            text.push_str(&t);
        }
        for p in fmt::pieces(&rec.text, filter) {
            let at = text.len();
            text.push_str(&p.text);
            let mut st = HighlightStyle::default();
            if p.color >= 30 { st.color = Some(ansi_ink(p.color)); }
            if p.bold { st.font_weight = Some(T::W_BOLD); }
            if p.marked { st.color = Some(T::text()); st.background_color = Some(T::accent_edge()); }
            if st != HighlightStyle::default() { hl.push((at..text.len(), st)); }
        }
    }
    if text.is_empty() { text.push(' '); }
    div().px(T::SP_12).pb(T::SP_2).font_family(T::MONO).text_size(T::TEXT_12).text_color(base)
        .when(!wrap, |d| d.whitespace_nowrap().overflow_hidden())
        .child(StyledText::new(text).with_highlights(hl))
        .into_any_element()
}

impl Docker {
    fn ensure_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inputs.is_some() { return; }
        let i = Inputs {
            filter: kit::input(window, cx, "Filter"),
            pull: kit::input(window, cx, "Pull an image, e.g. nginx:alpine"),
            form_name: kit::input(window, cx, "name"),
            form_target: kit::input(window, cx, "Port"),
            newname: kit::input(window, cx, ""),
            log_filter: kit::input(window, cx, "Filter lines"),
        };
        self.subs.push(cx.subscribe_in(&i.filter, window, |this, inp, ev: &w::InputEvent, _, cx| {
            if let w::InputEvent::Change = ev { let v = inp.read(cx).value().to_string(); let k = this.view.clone(); this.filter.insert(k, v); cx.notify(); }
        }));
        self.subs.push(cx.subscribe_in(&i.log_filter, window, |_, _, ev: &w::InputEvent, _, cx| { if let w::InputEvent::Change = ev { cx.notify(); } }));
        let enter = |f: fn(&mut Window, &mut App)| move |_: &mut Docker, _: &Entity<w::InputState>, ev: &w::InputEvent, window: &mut Window, cx: &mut Context<Docker>| {
            if let w::InputEvent::PressEnter { .. } = ev { window.defer(cx, f); }
        };
        self.subs.push(cx.subscribe_in(&i.pull, window, enter(|w, cx| pull(w, cx))));
        self.subs.push(cx.subscribe_in(&i.form_name, window, enter(|_, cx| create_forward(cx))));
        self.subs.push(cx.subscribe_in(&i.form_target, window, enter(|_, cx| create_forward(cx))));
        self.subs.push(cx.subscribe_in(&i.newname, window, enter(|_, cx| rename_do(cx))));
        if self.shell_views.is_none() {
            let t = self.shell.get_or_insert_with(|| cx.new(|_| shell::ShellTerm::new())).clone();
            self.shell_views = Some(shell::views(&t, window, cx));
        }
        self.inputs = Some(i);
    }

    fn render_nav(&self, cx: &mut Context<Self>) -> Div {
        let on = if self.detail.is_some() { usize::MAX } else { VIEWS.iter().position(|v| v.0 == self.view).unwrap_or(0) };
        let tabs = VIEWS.iter().map(|(k, l, ic)| w::SegTab { icon: Some(*ic), label: (*l).into(), count: self.count(k).into(), alert: false }).collect();
        let _ = cx;
        div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_16).py(T::SP_10).border_b_1().border_color(T::border())
            .child(w::seg_tabs("dk-nav", tabs, on, |i, window, cx| set_view(VIEWS[i].0, window, cx)))
            .child(div().flex_1().min_w_0())
            .when(self.detail.is_none() && self.view != "system", |d| d.children(self.inputs.as_ref().map(|i| div().w(T::DOCKER_FILTER_W).child(w::input(&i.filter)))))
            .child(w::icon_button("dk-refresh", IconName::RefreshCw, "Refresh").on_click(|_, _, cx| refresh(cx)))
    }

    /// The containers view's top: the counts and the bay plan.
    fn ctr_head(stats: Vec<(String, String)>, bay: Vec<Bay>) -> Div {
        div().flex().flex_col().gap(T::SP_12)
            .child(div().flex().gap(T::SP_20).children(stats.into_iter().map(|(v, l)| div().flex().flex_col()
                .child(div().font_family(T::MONO).text_size(T::TEXT_16).text_color(T::text()).child(v))
                .child(div().text_size(T::TEXT_10).text_color(T::text_muted()).child(l)))))
            // The bay plan: every container at a glance, by state.
            .child(div().flex().flex_wrap().gap(T::SP_4).p(T::SP_10).rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border_subtle())
                .children(bay.into_iter().enumerate().map(|(i, b)| {
                    let id = b.id.clone();
                    w::tip(div().id(("dk-bay", i)).size(T::DOCKER_BAY).rounded(T::SP_2).bg(state_ink(&b.state)).cursor_pointer().hover(|d| d.opacity(0.6))
                        .on_click(move |_, _, cx| open_detail(&id, cx)), b.tip)
                })))
    }

    /// A container's row.
    fn ctr_row(i: usize, c: &Ctr) -> Stateful<Div> {
        let (id, id2, act) = (c.id.clone(), c.id.clone(), c.act);
        let btn = match act {
            "stop" => w::arm_button(("dk-ctr-act", i), "", Some(IconName::Square), "Stop?", move |_, cx| container_verb(&id2, "stop", cx)).into_any_element(),
            _ => w::arm_button(("dk-ctr-act", i), "", Some(IconName::Play), "", move |_, cx| container_verb(&id2, act, cx)).into_any_element(),
        };
        row_frame(("dk-ctr", i)).cursor_pointer().hover(|d| d.bg(T::bg_hover())).on_click(move |_, _, cx| open_detail(&id, cx))
            .child(dk_dot(&c.state))
            .child(div().flex().flex_col().flex_grow(16.).flex_basis(relative(0.)).min_w_0().gap(T::SP_2)
                .child(div().truncate().text_size(T::TEXT_13).text_color(T::text()).child(c.name.clone()))
                .child(mono_sub(c.sub.clone())))
            .child(chips_el("dk-ctr-chip", i, &c.chips).flex_grow(10.).flex_basis(relative(0.)).min_w_0())
            .child(num_col(T::DOCKER_NUM_W, c.cpu.clone(), "CPU"))
            .child(num_col(T::DOCKER_MEM_W, c.mem.clone(), "MEM"))
            .child(div().flex_none().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(btn))
    }

    /// The containers view when it has no rows to list (empty, or nothing matches).
    fn render_containers(&self) -> Div {
        let (stats, bay, _, nomatch) = self.containers();
        let p = div().flex().flex_col().gap(T::SP_12);
        if self.list("containers").is_empty() { return p.child(dk_empty(IconName::Container, "No containers", "Nothing has been created on this machine yet.")); }
        let p = p.child(Self::ctr_head(stats, bay));
        if nomatch { return p.child(quiet("Nothing matches the filter.")); }
        p
    }

    fn render_stacks(&self) -> Div {
        let (empty, stacks, nomatch) = self.stacks();
        let p = div().flex().flex_col().gap(T::SP_12);
        if empty { return p.child(dk_empty(IconName::Layers, "No Compose projects", "Projects started with docker compose show here, with their services.")); }
        if nomatch { return p.child(quiet("Nothing matches the filter.")); }
        p.children(stacks.into_iter().enumerate().map(|(i, st)| {
            let n = st.name.clone();
            let a = move |act: &'static str| { let n = n.clone(); move |_: &mut Window, cx: &mut App| stack_action(&n, act, cx) };
            let mut head = div().flex().items_center().gap(T::SP_12).px(T::SP_16).py(T::SP_12)
                .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_4)
                    .child(div().flex().items_center().gap(T::SP_8)
                        .child(div().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(st.name.clone()))
                        .child(w::tag(st.chip.clone(), match st.tone { "ok" => "ok", "warn" => "warn", _ => "" })))
                    .child(mono_sub(st.place.clone())))
                .child(w::arm_button(("dk-st-up", i), "Up", Some(IconName::ArrowUp), "", a("up")).look("primary").disabled(!st.files))
                .child(w::arm_button(("dk-st-pull", i), "Pull", Some(IconName::Download), "", a("pull")).disabled(!st.files));
            if st.live {
                head = head.child(w::arm_button(("dk-st-restart", i), "Restart", Some(IconName::RotateCcw), "", a("restart")))
                    .child(w::arm_button(("dk-st-stop", i), "Stop", Some(IconName::Square), format!("Stop {}?", st.name), a("stop")).look("danger"));
            } else {
                head = head.child(w::arm_button(("dk-st-start", i), "Start", Some(IconName::Play), "", a("start")));
            }
            head = head.child(w::arm_button(("dk-st-down", i), "Down", Some(IconName::Trash), "Down: remove its containers?", a("down")).look("danger"));
            let mut l = dk_list().child(head).child(w::rule());
            for (j, c) in st.svcs.iter().enumerate() {
                if j > 0 { l = l.child(w::rule()); }
                let id = c.id.clone();
                l = l.child(row_frame(("dk-st-svc", i * 1000 + j)).cursor_pointer().hover(|d| d.bg(T::bg_hover())).on_click(move |_, _, cx| open_detail(&id, cx))
                    .child(dk_dot(&c.state))
                    .child(div().flex().flex_col().flex_1().min_w_0().child(div().truncate().text_size(T::TEXT_13).text_color(T::text()).child(c.name.clone())).child(mono_sub(c.sub.clone())))
                    .child(mono_sub(c.img.clone()).flex_1().min_w_0())
                    .child(mono_sub(c.status.clone()).flex_none()));
            }
            l
        }))
    }

    /// Images, volumes and networks: the bar (Pull, the counts, the cleans).
    fn things_head(&self, head: String) -> Div {
        let view = self.view.clone();
        let mut bar = div().flex().items_center().gap(T::SP_8);
        if view == "images" {
            if let Some(i) = &self.inputs { bar = bar.child(div().w(T::DOCKER_PULL_W).child(w::input(&i.pull))); }
            bar = bar.child(w::primary("dk-pull", "Pull").icon(IconName::Download).on_click(|_, window, cx| pull(window, cx)));
        } else {
            bar = bar.child(quiet(head.clone()));
        }
        bar = bar.child(div().flex_1().min_w_0());
        match view.as_str() {
            "images" => {
                bar = bar.child(w::arm_button("dk-prune-untagged", "Clean untagged", Some(IconName::Trash), "Remove untagged images?", |_, cx| prune("images", cx)).look("danger"))
                    .child(w::arm_button("dk-prune-images", "Clean unused", Some(IconName::Trash), "Remove every unused image?", |_, cx| prune("images-all", cx)).look("danger"));
            }
            "volumes" => bar = bar.child(w::arm_button("dk-prune-vol", "Clean unused", Some(IconName::Trash), "Delete every unused volume and its data?", |_, cx| prune("volumes", cx)).look("danger")),
            _ => bar = bar.child(w::arm_button("dk-prune-net", "Clean unused", Some(IconName::Trash), "Remove unused networks?", |_, cx| prune("networks", cx)).look("danger")),
        }
        let p = div().flex().flex_col().gap(T::SP_12).child(bar);
        if view == "images" { p.child(quiet(head)) } else { p }
    }

    /// An image's, volume's or network's row.
    fn thing_row(view: &str, i: usize, r: &Row) -> Stateful<Div> {
        let (kind, key) = (view.to_string(), r.key.clone());
        row_frame(("dk-row", i))
            .child(div().flex().flex_col().flex_grow(16.).flex_basis(relative(0.)).min_w_0().gap(T::SP_2)
                .child(div().flex().items_center().gap(T::SP_6)
                    .child(w::tip(div().id(("dk-row-name", i)).truncate().text_size(T::TEXT_13).text_color(T::text()).child(r.name.clone()), r.tip.clone()))
                    .children(r.tags.iter().enumerate().map(|(j, c)| chip_el(("dk-row-tag", i * 16 + j), c))))
                .child(mono_sub(r.sub.clone())))
            .child(chips_el("dk-row-chip", i, &r.chips).flex_grow(10.).flex_basis(relative(0.)).min_w_0())
            .child(w::arm_button(("dk-row-rm", i), "", Some(IconName::Trash), r.arm, move |_, cx| remove_thing(&kind, &key, cx)).look(if r.can_rm { "danger" } else { "" }).disabled(!r.can_rm))
    }

    /// Images, volumes or networks with no rows to list (empty, or nothing matches).
    fn render_things(&self) -> Div {
        let (head, empty, _, _) = self.things();
        let view = self.view.clone();
        let p = self.things_head(head);
        if empty {
            return p.child(if view == "images" { dk_empty(IconName::Archive, "No images", "") } else { dk_empty(IconName::HardDrive, "No volumes", "") });
        }
        p.child(quiet("Nothing matches the filter."))
    }

    /// The rows the page lists (rule 4: a virtual list draws only those on
    /// screen), made once per render; None when the view has nothing to list.
    fn page_rows(&self) -> Option<PageRows> {
        match self.view.as_str() {
            "containers" => {
                if self.list("containers").is_empty() { return None; }
                let (_, _, rows, nomatch) = self.containers();
                (!nomatch && !rows.is_empty()).then(|| PageRows::Ctr(rows))
            }
            "images" | "volumes" | "networks" => {
                let (_, empty, rows, nomatch) = self.things();
                (!empty && !nomatch && !rows.is_empty()).then(|| PageRows::Things(self.view.clone(), rows))
            }
            _ => None,
        }
    }

    /// One item of the virtual page: 0 the view's top, then a row each, in
    /// the list's frame (.dk-list: one border, rounded at its ends, a rule
    /// between rows).
    fn page_item(&self, ix: usize, rows: &PageRows) -> AnyElement {
        let n = rows.len();
        if ix == 0 {
            let top = match rows {
                PageRows::Ctr(_) => { let (stats, bay, _, _) = self.containers(); Self::ctr_head(stats, bay) }
                PageRows::Things(_, _) => { let (head, _, _, _) = self.things(); self.things_head(head) }
            };
            return div().px(T::SP_16).pt(T::SP_16).pb(T::SP_12).child(top).into_any_element();
        }
        let i = ix - 1;
        let row = match rows {
            PageRows::Ctr(r) => Self::ctr_row(i, &r[i]),
            PageRows::Things(v, r) => Self::thing_row(v, i, &r[i]),
        };
        let (first, last) = (i == 0, i + 1 == n);
        let frame = div().flex().flex_col().overflow_hidden().bg(T::bg_card()).border_l_1().border_r_1().border_color(T::border())
            .when(first, |d| d.border_t_1().rounded_t(T::R_FIELD))
            .when(last, |d| d.border_b_1().rounded_b(T::R_FIELD))
            .when(!first, |d| d.child(w::rule()))
            .child(row);
        div().px(T::SP_16).when(last, |d| d.pb(T::SP_16)).child(frame).into_any_element()
    }

    fn render_form(&self, blind: bool) -> Div {
        let Some(i) = &self.inputs else { return div() };
        let labels: Vec<SharedString> = self.form_labels.iter().map(|l| SharedString::from(l.clone())).collect();
        div().flex().flex_col().gap(T::SP_6)
            .child(div().flex().flex_wrap().items_center().gap(T::SP_8).p(T::SP_10).rounded(T::R_CONTROL).bg(T::bg_tertiary()).border_1().border_color(T::border())
                .when(blind, |d| d.child(div().w(T::SP_48 + T::SP_40).child(w::input(&i.form_target))))
                .child(div().flex_1().min_w_0().min_w(T::SP_48 + T::SP_48).child(w::input(&i.form_name)))
                .child(w::select("dk-form-mode", labels, Some(self.form_mode), "", false, |m, _, cx| upd(cx, |s| s.form_mode = m)))
                .child(w::check("dk-form-public", "Public", self.form_public, |on, _, cx| { let on = *on; upd(cx, |s| s.form_public = on); }))
                .child(w::check("dk-form-watch", "Warn if it goes down", self.form_watch, |on, _, cx| { let on = *on; upd(cx, |s| s.form_watch = on); }))
                .child(w::primary("dk-form-create", "Create").icon(IconName::Check).on_click(|_, _, cx| create_forward(cx)))
                .child(w::button("dk-form-cancel", "Cancel").on_click(|_, _, cx| upd(cx, |s| s.form = None))))
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child(self.form_help.clone()))
    }

    fn render_forwards(&self) -> Div {
        let (head, empty, orphans, groups, nomatch) = self.forwards();
        let blind = self.form.as_deref() == Some("any");
        let mut p = div().flex().flex_col().gap(T::SP_12);
        if empty {
            let mut e = dk_empty(IconName::Globe, "No forwards", "A forward reaches a port on this machine through Crundi, by name: for a container, or for anything else listening here.");
            e = if blind { e.child(self.render_form(true)) } else { e.child(w::primary("dk-fwd-add-empty", "Add forward").icon(IconName::Plus).on_click(|_, window, cx| open_form("any", window, cx))) };
            return p.child(e);
        }
        p = p.child(div().flex().items_center().gap(T::SP_8).child(quiet(head)).child(div().flex_1().min_w_0())
            .child(w::primary("dk-fwd-add", "Add forward").icon(IconName::Plus).on_click(|_, window, cx| open_form("any", window, cx)))
            .when(orphans > 0, |d| d.child(w::arm_button("dk-rm-orphans", "Remove orphaned", Some(IconName::Trash), format!("Remove {orphans} orphaned?"), |_, cx| remove_orphans(cx)).look("danger"))));
        if blind { p = p.child(self.render_form(true)); }
        for (gi, g) in groups.iter().enumerate() {
            let mut l = dk_list();
            for (i, f) in g.rows.iter().enumerate() {
                if i > 0 { l = l.child(w::rule()); }
                let k = gi * 1000 + i;
                let (h1, h2, watch) = (f.host.clone(), f.host.clone(), f.watch);
                l = l.child(row_frame(("dk-fwd", k))
                    .child(div().flex().flex_col().flex_grow(16.).flex_basis(relative(0.)).min_w_0().gap(T::SP_2)
                        .child(div().flex().items_center().gap(T::SP_6).child(div().truncate().text_size(T::TEXT_13).text_color(T::text()).child(f.host.clone()))
                            .children(f.tags.iter().enumerate().map(|(j, c)| chip_el(("dk-fwd-tag", k * 8 + j), c))))
                        .child(mono_sub(f.sub.clone())))
                    .child(chips_el("dk-fwd-chip", k, &f.chips).flex_grow(10.).flex_basis(relative(0.)).min_w_0())
                    .child(w::icon_toggle(("dk-fwd-watch", k), IconName::Bell, "Warn if it goes down", watch).on_click(move |_, _, cx| fwd_watch(&h1, !watch, cx)))
                    .child(w::arm_button(("dk-fwd-rm", k), "", Some(IconName::Trash), "Remove?", move |_, cx| rm_fwd(&h2, cx)).look("danger")));
            }
            p = p.child(div().flex().flex_col().gap(T::SP_6).child(group_title(g.title.clone()))
                .when(!g.help.is_empty(), |d| d.child(quiet(g.help)))
                .child(l));
        }
        if nomatch { p = p.child(quiet("Nothing matches the filter.")); }
        p
    }

    fn render_system(&self) -> Div {
        let (engine, contents, warnings, (dstate, derr), disk) = self.system();
        let p = div().flex().flex_col().gap(T::SP_12);
        if engine.is_empty() { return p.child(quiet("Loading\u{2026}")); }
        let mut d = dk_sec("DISK USE");
        match dstate {
            "measuring" => d = d.child(quiet("Measuring\u{2026} Docker walks every layer for this, which can take a while.")),
            "error" => d = d.child(quiet(derr)),
            _ => {
                let cols = |a: Div, b: Div, c: Div, e: Div, f: AnyElement| div().flex().items_center().gap(T::SP_8).px(T::SP_8)
                    .child(a.flex_1().min_w_0()).child(b.w(T::DOCKER_NUM_W).flex().justify_end()).child(c.w(T::DOCKER_MEM_W + T::SP_12).flex().justify_end())
                    .child(e.w(T::DOCKER_MEM_W + T::SP_24).flex().justify_end()).child(div().w(T::DOCKER_MEM_W).flex().justify_end().child(f));
                d = d.child(cols(group_title("WHAT"), group_title("COUNT"), group_title("SIZE"), group_title("RECLAIMABLE"), div().into_any_element()).pb(T::SP_6))
                    .child(div().h(px(1.)).w_full().bg(T::border()));
                let num = |v: String| div().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text_secondary()).child(v);
                for (i, r) in disk.into_iter().enumerate() {
                    if i > 0 { d = d.child(w::rule()); }
                    let kind = r.kind;
                    d = d.child(cols(div().flex().flex_col().child(div().text_size(T::TEXT_13).text_color(T::text()).child(r.label))
                            .when(!r.note.is_empty(), |x| x.child(div().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(r.note.clone()))),
                        num(r.count), num(r.size), num(r.reclaim),
                        w::arm_button(("dk-disk-clean", i), "Clean", Some(IconName::Trash), r.arm, move |_, cx| prune(kind, cx)).look("danger").disabled(!r.enabled).into_any_element()).py(T::SP_8));
                }
                d = d.child(quiet("Docker is shared by every project on this machine. A stopped container may be one someone means to start again."));
            }
        }
        p.child(div().flex().gap(T::SP_12)
                .child(dk_sec("ENGINE").flex_1().min_w_0().children(engine.iter().map(|(k, v)| dk_kv(k, v, false))))
                .child(dk_sec("CONTENTS").flex_1().min_w_0().children(contents.iter().map(|(k, v)| dk_kv(k, v, false)))
                    .children(warnings.into_iter().map(|w| div().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::yellow()).child(w)))))
            .child(d)
    }

    fn render_overview(&self, d: &Detail) -> Div {
        let ov = &d.data["overview"];
        let [st, runs, nets, mounts, env, labels] = overview_rows(ov);
        let sec = |title: &str, rows: &[(String, String, bool)], none: &'static str| {
            let s = dk_sec(title).flex_1().min_w_0();
            if rows.is_empty() && !none.is_empty() { s.child(quiet(none)) } else { s.children(rows.iter().map(|(k, v, h)| dk_kv(k, v, *h))) }
        };
        let mut ports_sec = dk_sec("PORTS").flex_1().min_w_0();
        let ps = ports(ov, self.fwd_list.as_ref());
        if ps.is_empty() { ports_sec = ports_sec.child(quiet("No ports.")); }
        for (i, p) in ps.iter().enumerate() {
            if i > 0 { ports_sec = ports_sec.child(w::rule()); }
            let port = p.port.clone();
            ports_sec = ports_sec.child(div().flex().items_center().gap(T::SP_10)
                .child(div().flex_none().w(T::SP_48 + T::SP_40).text_size(T::TEXT_12).text_color(T::text_muted()).child(p.k.clone()))
                .child(div().flex_1().min_w_0().truncate().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text()).child(p.v.clone()))
                .children(p.fwds.iter().enumerate().map(|(j, c)| chip_el(("dk-port-fwd", i * 16 + j), c)))
                .when(!p.btn.is_empty(), |x| x.child(w::button(("dk-port-add", i), p.btn).icon(IconName::Plus).on_click(move |_, window, cx| open_form(&port, window, cx)))));
            if self.form.as_deref() == Some(p.port.as_str()) { ports_sec = ports_sec.child(self.render_form(false)); }
        }
        let has_hidden = env.iter().any(|x| x.2);
        let mut page = div().flex().flex_col().gap(T::SP_12)
            .child(div().flex().gap(T::SP_12).child(sec("STATE", &st, "")).child(sec("RUNS", &runs, "")))
            .child(div().flex().gap(T::SP_12).child(ports_sec).child(sec("NETWORKS", &nets, "Not on any network.")))
            .child(sec("MOUNTS", &mounts, "No mounts."))
            .child(sec("ENVIRONMENT", &env, "None.").when(has_hidden, |x| x.child(quiet("Values that look like secrets are hidden here. Inspect shows everything."))));
        if !labels.is_empty() {
            let open = self.labels_open;
            page = page.child(dk_sec("")
                .child(div().id("dk-labels").flex().items_center().gap(T::SP_6).cursor_pointer().text_size(T::TEXT_12).text_color(T::text_secondary())
                    .child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM))
                    .child(format!("Labels ({})", labels.len()))
                    .on_click(|_, _, cx| upd(cx, |s| s.labels_open = !s.labels_open)))
                .when(open, |x| x.children(labels.iter().map(|(k, v, _)| dk_kv(k, v, false)))));
        }
        page
    }

    fn render_logs(&mut self, cx: &mut Context<Self>) -> Div {
        let filter = log_filter_text(self, cx);
        let (rows, n) = match &self.detail { Some(d) => (shown_lines(d, &filter), d.lines.len()), None => (vec![], 0) };
        // The list follows the rows: appended ones are spliced in, anything else resets it.
        let old = self.log_rows.len();
        let same = self.log_key.0 == filter && self.log_key.1 <= n && rows.len() >= old && rows[..old] == self.log_rows[..];
        if !same { self.log_list.reset(rows.len()); } else if rows.len() > old { self.log_list.splice(old..old, rows.len() - old); }
        if self.log_follow && rows.len() != old && !rows.is_empty() { self.log_list.scroll_to_reveal_item(rows.len() - 1); }
        self.log_key = (filter.clone(), n);
        self.log_rows = Rc::new(rows);
        let (me, idx, ts, wrap) = (cx.entity().downgrade(), self.log_rows.clone(), self.log_ts, self.log_wrap);
        let list = list(self.log_list.clone(), move |ix, _, cx| {
            let Some(me) = me.upgrade() else { return div().into_any_element() };
            let s = me.read(cx);
            match (s.detail.as_ref(), idx.get(ix)) {
                (Some(d), Some(i)) if *i < d.lines.len() => log_line(&d.lines[*i], &filter, ts, wrap),
                _ => div().into_any_element(),
            }
        }).size_full();
        div().flex().flex_col().flex_1().min_w_0().min_h_0()
            .child(div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_12).py(T::SP_4).border_b_1().border_color(T::border_subtle())
                .children(self.inputs.as_ref().map(|i| div().w(T::DOCKER_LOG_FILTER_W).child(w::input(&i.log_filter))))
                .child(w::check("dk-log-follow", "Follow", self.log_follow, |on, _, cx| log_option("follow", *on, cx)))
                .child(w::check("dk-log-ts", "Times", self.log_ts, |on, _, cx| log_option("ts", *on, cx)))
                .child(w::check("dk-log-wrap", "Wrap", self.log_wrap, |on, _, cx| log_option("wrap", *on, cx)))
                .child(div().flex_1().min_w_0())
                .child(w::icon_button("dk-log-copy", IconName::Copy, "Copy").on_click(|_, _, cx| log_copy(cx)))
                .child(w::icon_button("dk-log-clear", IconName::X, "Clear").on_click(|_, _, cx| upd(cx, |s| if let Some(d) = s.detail.as_mut() { d.lines.clear(); }))))
            .child(div().flex_1().min_w_0().min_h_0().bg(T::bg()).pt(T::SP_6).child(list))
    }

    fn render_stats(&self, d: &Detail) -> Div {
        let p = div().flex().flex_col().gap(T::SP_12);
        if let Some(m) = &d.stats_msg { return p.child(quiet(m.clone())); }
        if d.samples.is_empty() { return p.child(quiet("Measuring\u{2026}")); }
        let (cs, note) = charts(&d.samples);
        let inks = [(T::accent(), T::accent()), (T::green(), T::green()), (T::docker_cyan(), T::docker_magenta()), (T::amber(), T::docker_red())];
        let chart = |c: &Chart, (c1, c2): (Hsla, Hsla)| {
            let fill = |c: Hsla| Hsla { a: T::DOCKER_FILL, ..c };
            let mut series = vec![w::Series { values: c.a.clone(), max: c.max, stroke: c1, fill: fill(c1) }];
            if !c.b.is_empty() { series.push(w::Series { values: c.b.clone(), max: c.max, stroke: c2, fill: fill(c2) }); }
            let leg = |col: Hsla, text: String| div().flex().items_center().gap(T::SP_4).child(div().size(T::SP_8).rounded(T::SP_2).bg(col))
                .child(div().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(text));
            div().flex().flex_col().flex_1().min_w_0().gap(T::SP_8).px(T::SP_16).py(T::SP_12).rounded(T::R_FIELD).bg(T::bg_secondary()).border_1().border_color(T::border())
                .child(div().flex().items_end().gap(T::SP_10)
                    .child(div().text_size(T::TEXT_10).text_color(T::text_muted()).child(c.title.clone()))
                    .child(div().flex_1().min_w_0())
                    .child(div().font_family(T::MONO).text_size(T::TEXT_15).text_color(T::text()).child(c.value.clone()))
                    .when(!c.sub.is_empty(), |x| x.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(c.sub.clone()))))
                .child(div().h(T::DOCKER_CHART_H).w_full().child(w::spark(series, fmt::CHART_N, Some(T::border_subtle())).size_full()))
                .when(!c.leg1.is_empty(), |x| x.child(div().flex().gap(T::SP_12).child(leg(c1, c.leg1.clone())).child(leg(c2, c.leg2.clone()))))
        };
        p.child(div().flex().gap(T::SP_12).child(chart(&cs[0], inks[0])).child(chart(&cs[1], inks[1])))
            .child(div().flex().gap(T::SP_12).child(chart(&cs[2], inks[2])).child(chart(&cs[3], inks[3])))
            .child(quiet(note))
    }

    fn render_shell(&self, d: &Detail) -> Div {
        let (bar, btn) = shell_bar(d);
        div().flex().flex_col().flex_1().min_w_0().min_h_0()
            .child(div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_12).h(T::CONTROL + T::SP_8).border_b_1().border_color(T::border_subtle())
                .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).text_color(T::text_secondary()).child(bar))
                .when(!btn.is_empty(), |x| x.child(w::button("dk-shell-again", btn).icon(if btn == "New shell" { IconName::RotateCcw } else { IconName::Terminal }).on_click(|_, _, cx| open_shell(cx)))))
            // The terminal (xterm.js in the web): the screen and its cursor,
            // cached leaves: output redraws them, never this tab.
            .children(self.shell_views.as_ref().map(|v| v.element()))
    }

    fn render_detail(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(d) = &self.detail else { return div() };
        let c = d.data["container"].clone();
        let (state, name, sub, dot) = if !d.loaded {
            (if d.error.is_empty() { "loading" } else { "error" }, d.id.clone(), String::new(), String::new())
        } else {
            let p = ss(&c, "project");
            ("data", ss(&c, "name"), format!("{} \u{b7} {}{}", ss(&c, "image"), ss(&c, "status"), if p.is_empty() { String::new() } else { format!(" \u{b7} {p}") }), fmt::state_key(&c))
        };
        let mut head = div().flex().flex_none().items_center().gap(T::SP_10).px(T::SP_16).py(T::SP_8).min_h(T::CONTROL + T::SP_16).border_b_1().border_color(T::border())
            .child(w::icon_button("dk-back", IconName::ChevronLeft, "Back").on_click(|_, window, cx| back(window, cx)))
            .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
                .child(div().flex().items_center().gap(T::SP_8).when(!dot.is_empty(), |x| x.child(dk_dot(&dot)))
                    .child(div().truncate().text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(name.clone())))
                .when(!sub.is_empty(), |x| x.child(mono_sub(sub))));
        if state == "data" {
            for (i, a) in detail_acts(&c).into_iter().enumerate() {
                let verb = a.verb;
                let icon = match verb { "stop" => IconName::Square, "restart" => IconName::RotateCcw, "pause" => IconName::Pause, "kill" => IconName::Zap, _ => IconName::Play };
                head = head.child(w::arm_button(("dk-act", i), a.label, Some(icon), a.arm, move |_, cx| container_verb("", verb, cx)).look(a.kind));
            }
            head = head.child(w::button("dk-rename", "Rename").icon(IconName::Pencil).on_click(|_, window, cx| open_prompt("rename", window, cx)))
                .child(w::danger("dk-remove", "Remove").icon(IconName::Trash).on_click(|_, window, cx| open_prompt("remove", window, cx)));
        }
        let mut out = div().flex().flex_col().flex_1().min_w_0().min_h_0().child(head);
        if state == "loading" { return out.child(div().p(T::SP_16).child(quiet("Loading\u{2026}"))); }
        if state == "error" { return out.child(div().p(T::SP_16).child(quiet(d.error.clone()))); }
        // Rename / remove: inline under the head (.dk-prompt).
        if !d.prompt.is_empty() {
            let mut pr = div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_16).py(T::SP_8).bg(T::bg_secondary()).border_b_1().border_color(T::border())
                .on_key_down(|e: &KeyDownEvent, window, cx| if e.keystroke.key == "escape" { open_prompt("", window, cx); });
            if d.prompt == "rename" {
                pr = pr.child(div().text_size(T::TEXT_13).text_color(T::text()).child("New name"))
                    .children(self.inputs.as_ref().map(|i| div().flex_1().min_w_0().child(w::input(&i.newname))))
                    .child(w::primary("dk-rename-do", "Rename").on_click(|_, _, cx| rename_do(cx)));
            } else {
                let force_shown = running(&c) || ss(&c, "state") == "paused";
                pr = pr.child(div().text_size(T::TEXT_13).text_color(T::text()).child(format!("Remove {name}? This cannot be undone.")))
                    .child(w::check("dk-rmvol", "Also its anonymous volumes", self.rmvol, |on, _, cx| { let on = *on; upd(cx, |s| s.rmvol = on); }))
                    .when(force_shown, |x| x.child(w::check("dk-rmforce", "Stop it first", self.rmforce, |on, _, cx| { let on = *on; upd(cx, |s| s.rmforce = on); })))
                    .child(div().flex_1().min_w_0())
                    .child(w::danger("dk-remove-do", "Remove").icon(IconName::Trash).on_click(|_, _, cx| remove_do(cx)));
            }
            out = out.child(pr.child(w::button("dk-prompt-cancel", "Cancel").on_click(|_, window, cx| open_prompt("", window, cx))));
        }
        let tab = d.tab.clone();
        out = out.child(div().flex().flex_none().items_end().gap(T::SP_2).pl(T::SP_10).h(T::CONTROL + T::SP_4).border_b_1().border_color(T::border())
            .children(TABS.iter().map(|(k, l)| {
                let on = tab == *k;
                let k = *k;
                div().id(SharedString::from(format!("dk-tab-{k}"))).flex().items_center().h_full().px(T::SP_12).cursor_pointer().text_size(T::TEXT_13)
                    .text_color(if on { T::text() } else { T::text_secondary() }).hover(|d| d.text_color(T::text()))
                    .border_b_2().border_color(if on { T::accent() } else { gpui::transparent_black() })
                    .child(*l).on_click(move |_, _, cx| select_tab(k, cx))
            })));
        let body = match tab.as_str() {
            "logs" => self.render_logs(cx),
            _ => {
                let d = self.detail.as_ref().unwrap();
                match tab.as_str() {
                    "stats" => div().id("dk-stats").flex_1().min_w_0().min_h_0().overflow_y_scroll().p(T::SP_16).child(self.render_stats(d)).into_any_element().into_div(),
                    "shell" => self.render_shell(d),
                    "inspect" => {
                        let text = serde_json::to_string_pretty(&d.data["inspect"]).unwrap_or_default();
                        div().flex().flex_col().flex_1().min_w_0().min_h_0()
                            .child(div().flex().flex_none().justify_end().items_center().px(T::SP_12).h(T::CONTROL + T::SP_8).border_b_1().border_color(T::border_subtle())
                                .child(w::button("dk-inspect-copy", "Copy JSON").icon(IconName::Copy).on_click(|_, _, cx| { let t = inspect_text(cx); copy(cx, &t); })))
                            .child(div().id("dk-inspect").flex_1().min_w_0().min_h_0().overflow_scroll().bg(T::bg()).px(T::SP_16).py(T::SP_12)
                                .font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text_secondary()).whitespace_nowrap().child(text))
                    }
                    _ => div().id("dk-overview").flex_1().min_w_0().min_h_0().overflow_y_scroll().p(T::SP_16).child(self.render_overview(d)).into_any_element().into_div(),
                }
            }
        };
        out.child(body)
    }
}

/// An element in a flex column filling the rest (a scrolling page in the detail).
trait IntoDiv { fn into_div(self) -> Div; }
impl IntoDiv for AnyElement { fn into_div(self) -> Div { div().flex().flex_col().flex_1().min_w_0().min_h_0().child(self) } }

fn inspect_text(cx: &mut App) -> String {
    rd(cx, |s| s.detail.as_ref().map(|d| serde_json::to_string_pretty(&d.data["inspect"]).unwrap_or_default()).unwrap_or_default())
}

impl Render for Docker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("docker");
        self.ensure_inputs(window, cx);
        let mut root = div().relative().flex().flex_col().size_full().bg(T::bg());
        match self.gate() {
            "loading" => return root.child(dk_empty(IconName::Container, "Loading\u{2026}", "")),
            "absent" => return root.child(dk_empty(IconName::Container, "Docker is not installed", "Install Docker Engine on this machine and this tab will manage it.")),
            "unreachable" => {
                let e = self.status.as_ref().map(|x| ss(x, "error")).unwrap_or_default();
                return root.child(dk_empty(IconName::Container, "Docker is installed, but Crundi cannot reach it", e)
                    .child(w::button("dk-recheck", "Try again").icon(IconName::RefreshCw).on_click(|_, _, cx| recheck(cx))));
            }
            _ => {}
        }
        root = root.child(self.render_nav(cx));
        if self.detail.is_some() {
            root = root.child(self.render_detail(cx));
        } else if let Some(rows) = (self.state() == "data").then(|| self.page_rows()).flatten() {
            // A list to show: a virtual list, drawn only where on screen (rule
            // 4); its rows made once per change (this render), measured again.
            let n = rows.len() + 1;
            let (old_view, old) = self.page_key.clone();
            if old_view != self.view { self.page_list.reset(n); }
            else {
                if n > old { self.page_list.splice(old..old, n - old); } else if n < old { self.page_list.splice(n..old, 0); }
                self.page_list.remeasure();
            }
            self.page_key = (self.view.clone(), n);
            self.page_rows = Rc::new(Some(rows));
            let (me, rows) = (cx.entity().downgrade(), self.page_rows.clone());
            let l = list(self.page_list.clone(), move |ix, _, cx| {
                let (Some(me), Some(r)) = (me.upgrade(), rows.as_ref().as_ref()) else { return div().into_any_element() };
                me.read(cx).page_item(ix, r)
            }).size_full();
            root = root.child(div().id("dk-page").flex_1().min_w_0().min_h_0().child(l));
        } else {
            self.page_key = (String::new(), usize::MAX);
            self.page_rows = Rc::new(None);
            let body = match self.state() {
                "loading" => quiet("Loading\u{2026}"),
                "error" => { let (t, e) = self.error.clone().unwrap_or_default(); dk_empty(IconName::Container, t, e) }
                _ => match self.view.as_str() {
                    "containers" => self.render_containers(),
                    "stacks" => self.render_stacks(),
                    "images" | "volumes" | "networks" => self.render_things(),
                    "forwards" => self.render_forwards(),
                    _ => self.render_system(),
                },
            };
            root = root.child(div().id("dk-page").flex_1().min_w_0().min_h_0().overflow_y_scroll().p(T::SP_16).child(body));
        }
        // The job drawer (.dk-job): compose and pull output, over the bottom.
        if let Some(j) = &self.job {
            root = root.child(div().absolute().left(T::SP_12).right(T::SP_12).bottom(T::SP_12).h(relative(T::DOCKER_JOB_SHARE))
                .flex().flex_col().rounded(T::R_CARD).bg(T::bg_secondary()).border_1().border_color(T::border()).occlude()
                .child(div().flex().flex_none().items_center().gap(T::SP_10).pl(T::SP_12).pr(T::SP_8).h(T::CONTROL + T::SP_4).border_b_1().border_color(T::border_subtle())
                    .child(w::icon(IconName::Terminal, T::ICON_MD))
                    .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(j.title.clone()))
                    .child(if j.done { w::tag(if j.code == 0 { "done".to_string() } else { format!("exit {}", j.code) }, if j.code == 0 { "ok" } else { "error" }).into_any_element() } else { w::spinner(T::ICON_MD).into_any_element() })
                    .child(w::icon_button("dk-job-close", IconName::X, "Hide").on_click(|_, _, cx| upd(cx, |s| s.job = None))))
                .child(div().id("dk-job-out").flex_1().min_w_0().min_h_0().overflow_y_scroll().track_scroll(&self.job_scroll).px(T::SP_12).py(T::SP_8)
                    .font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text_secondary()).child(j.text.clone())));
        }
        root
    }
}

// ─── The driver ───

fn arg(a: &[String], i: usize) -> &str { a.get(i).map(|s| s.as_str()).unwrap_or("") }

pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    if !name.starts_with("docker-") { return None; }
    ent(cx).update(cx, |s, cx| s.ensure_inputs(window, cx));
    let input = |cx: &mut App, f: fn(&Inputs) -> Entity<w::InputState>| rd(cx, |s| s.inputs.as_ref().map(f));
    match name {
        "docker-view" => set_view(arg(args, 0), window, cx),
        "docker-filter" => {
            let v = arg(args, 0).to_string();
            let i = input(cx, |i| i.filter.clone());
            kit::set_value(&i, &v, window, cx);
            upd(cx, |s| { let k = s.view.clone(); s.filter.insert(k, v); });
        }
        "docker-refresh" => refresh(cx),
        "docker-recheck" => recheck(cx),
        "docker-open" => open_detail(arg(args, 0), cx),
        "docker-back" => back(window, cx),
        "docker-tab" => select_tab(arg(args, 0), cx),
        "docker-ctr" => container_verb(arg(args, 0), arg(args, 1), cx),
        "docker-prompt" => open_prompt(arg(args, 0), window, cx),
        "docker-rename" => { let i = input(cx, |i| i.newname.clone()); kit::set_value(&i, arg(args, 0), window, cx); rename_do(cx); }
        "docker-remove" => { let (v, f) = (arg(args, 0) == "true", arg(args, 1) != "false"); upd(cx, |s| { s.rmvol = v; s.rmforce = f; }); remove_do(cx); }
        "docker-log-filter" => { let i = input(cx, |i| i.log_filter.clone()); kit::set_value(&i, arg(args, 0), window, cx); }
        "docker-log-option" => log_option(arg(args, 0), arg(args, 1) == "true", cx),
        "docker-log-copy" => { log_copy(cx); return Some(json!({ "ok": true, "text": rd(cx, |s| s.copied.clone()) })); }
        "docker-log-clear" => upd(cx, |s| if let Some(d) = s.detail.as_mut() { d.lines.clear(); }),
        "docker-inspect-copy" => { let t = inspect_text(cx); copy(cx, &t); return Some(json!({ "ok": true, "text": rd(cx, |s| s.copied.clone()) })); }
        "docker-shell-send" => shell_input(&format!("{}\r", arg(args, 0)), cx),
        "docker-shell-key" => shell_key(arg(args, 0), cx),
        "docker-shell-again" => open_shell(cx),
        "docker-stack" => stack_action(arg(args, 0), arg(args, 1), cx),
        "docker-pull" => { let i = input(cx, |i| i.pull.clone()); kit::set_value(&i, arg(args, 0), window, cx); pull(window, cx); }
        "docker-prune" => prune(arg(args, 0), cx),
        "docker-rm" => remove_thing(arg(args, 0), arg(args, 1), cx),
        "docker-chip" => { let a = match arg(args, 0) { "open" => "open", "url" => "url", _ => "" }; chip_act(a, arg(args, 1), cx); }
        "docker-fwd-add" => open_form(arg(args, 0), window, cx),
        // name, mode index, public, watch, port (the Forwards view's).
        "docker-fwd-fill" => {
            let (n, tg) = (input(cx, |i| i.form_name.clone()), input(cx, |i| i.form_target.clone()));
            kit::set_value(&n, arg(args, 0), window, cx);
            kit::set_value(&tg, arg(args, 4), window, cx);
            let (m, p, w) = (arg(args, 1).parse().unwrap_or(0), arg(args, 2) == "true", arg(args, 3) == "true");
            upd(cx, |s| { s.form_mode = m; s.form_public = p; s.form_watch = w; });
        }
        "docker-fwd-create" => create_forward(cx),
        "docker-fwd-cancel" => upd(cx, |s| s.form = None),
        "docker-fwd-watch" => fwd_watch(arg(args, 0), arg(args, 1) == "true", cx),
        "docker-rm-fwd" => rm_fwd(arg(args, 0), cx),
        "docker-rm-orphans" => remove_orphans(cx),
        "docker-job-close" => upd(cx, |s| s.job = None),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

fn chip_texts(c: &[Chip]) -> Vec<String> { c.iter().map(|c| c.text.clone()).collect() }
fn pairs(v: &[(String, String)]) -> Vec<Value> { v.iter().map(|(k, x)| json!([k, x])).collect() }
fn pairs3(v: &[(String, String, bool)]) -> Vec<Value> { v.iter().map(|(k, x, _)| json!([k, x])).collect() }
fn tail(s: &str, n: usize) -> String { let c: Vec<char> = s.chars().collect(); c[c.len().saturating_sub(n)..].iter().collect() }

/// The driver's view of this screen (the Slint client's names).
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    let e = ent(cx);
    let s = e.read(cx);
    let filter = s.inputs.as_ref().map(|i| i.filter.read(cx).value().to_string()).unwrap_or_default();
    let log_filter = log_filter_text(s, cx);
    let newname = s.inputs.as_ref().map(|i| i.newname.read(cx).value().to_string()).unwrap_or_default();
    let form_name = s.inputs.as_ref().map(|i| i.form_name.read(cx).value().to_string()).unwrap_or_default();
    let (stats, bay, ctrs, c_nomatch) = s.containers();
    let (stacks_empty, stacks, s_nomatch) = s.stacks();
    let (thead, list_empty, rows, t_nomatch) = s.things();
    let (fhead, fwd_empty, orphans, groups, f_nomatch) = s.forwards();
    let (engine, contents, _, (disk_state, _), disk) = s.system();
    let nomatch = match s.view.as_str() { "containers" => c_nomatch, "stacks" => s_nomatch, "images" | "volumes" | "networks" => t_nomatch, "forwards" => f_nomatch, _ => false };
    let list_head = match s.view.as_str() { "forwards" => fhead, "images" | "volumes" | "networks" => thead, _ => String::new() };
    let detail = s.detail.as_ref().map(|d| {
        let c = &d.data["container"];
        let ov = &d.data["overview"];
        let [st, runs, nets, mounts, env, labels] = overview_rows(ov);
        let lines: Vec<String> = d.lines.iter().map(|l| fmt::strip_ansi(&l.text)).collect();
        let (bar, btn) = shell_bar(d);
        let (cs, note) = charts(&d.samples);
        let state = if d.loaded { "data" } else if d.error.is_empty() { "loading" } else { "error" };
        json!({
            "state": state, "error": d.error, "name": if d.loaded { ss(c, "name") } else { d.id.clone() }, "tab": d.tab,
            "sub": if d.loaded { format!("{} \u{b7} {}", ss(c, "image"), ss(c, "status")) } else { String::new() },
            "dot": if d.loaded { fmt::state_key(c) } else { String::new() }, "prompt": d.prompt, "newname": newname,
            "rmforceShown": running(c) || ss(c, "state") == "paused",
            "acts": if d.loaded { detail_acts(c).iter().map(|a| a.label).collect::<Vec<_>>() } else { vec![] },
            "ovState": pairs3(&st), "ovRuns": pairs3(&runs), "ovNets": pairs3(&nets), "ovMounts": pairs3(&mounts), "ovEnv": env.len(), "ovLabels": labels.len(),
            "ports": ports(ov, s.fwd_list.as_ref()).iter().map(|p| json!({ "port": p.port, "k": p.k, "v": p.v, "btn": p.btn, "fwds": chip_texts(&p.fwds) })).collect::<Vec<_>>(),
            "logRows": shown_lines(d, &log_filter).len(), "lines": lines.len(), "tail": lines.iter().rev().take(20).rev().cloned().collect::<Vec<_>>(),
            "logFollow": s.log_follow, "logTs": s.log_ts, "logWrap": s.log_wrap,
            "statsState": if d.stats_msg.is_some() { "msg" } else if d.samples.is_empty() { "measuring" } else { "data" },
            "statsMsg": d.stats_msg.clone().unwrap_or_default(),
            "charts": cs.iter().map(|c| json!({ "title": c.title, "value": c.value, "line": c.a.len() >= 2 })).collect::<Vec<_>>(), "statsNote": note,
            "shellBar": bar, "shellBtn": btn, "shellLive": d.shell_ref.is_some(), "shellText": tail(&s.shell.as_ref().map(|t| t.read(cx).text()).unwrap_or_default(), 2000),
            "inspect": serde_json::to_string_pretty(&d.data["inspect"]).map(|x| x.len()).unwrap_or(0),
        })
    });
    json!({
        "shown": s.shown,
        "gate": s.gate(), "gateError": s.status.as_ref().map(|x| ss(x, "error")).unwrap_or_default(),
        "view": s.view, "state": s.state(),
        "errTitle": s.error.as_ref().map(|e| e.0.clone()).unwrap_or_default(), "errText": s.error.as_ref().map(|e| e.1.clone()).unwrap_or_default(),
        "segs": VIEWS.iter().map(|(k, l, _)| json!({ "key": k, "label": l, "count": s.count(k) })).collect::<Vec<_>>(),
        "filter": filter, "nomatch": nomatch,
        "ctrEmpty": s.list("containers").is_empty(),
        "stats": pairs(&stats), "bay": bay.len(),
        "ctrs": ctrs.iter().map(|c| json!({ "id": c.id, "name": c.name, "state": c.state, "act": c.act, "chips": chip_texts(&c.chips), "cpu": c.cpu })).collect::<Vec<_>>(),
        "stacksEmpty": stacks_empty,
        "stacks": stacks.iter().map(|x| json!({ "name": x.name, "chip": x.chip, "svcs": x.svcs.len() })).collect::<Vec<_>>(),
        "listHead": list_head, "listEmpty": list_empty,
        "rows": rows.iter().map(|r| json!({ "key": r.key, "name": r.name, "sub": r.sub, "chips": chip_texts(&r.chips), "tags": chip_texts(&r.tags), "canRm": r.can_rm })).collect::<Vec<_>>(),
        "fwdEmpty": fwd_empty, "orphans": orphans,
        "groups": groups.iter().map(|g| json!({ "title": g.title, "rows": g.rows.iter().map(|f| json!({ "host": f.host, "sub": f.sub, "watch": f.watch, "tags": chip_texts(&f.tags), "chips": chip_texts(&f.chips) })).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "form": s.form.clone().unwrap_or_default(), "formName": form_name, "formModes": s.form_labels, "formHelp": s.form_help,
        "engine": pairs(&engine), "contents": pairs(&contents),
        "diskState": disk_state, "disk": disk.iter().map(|d| json!({ "label": d.label, "size": d.size, "enabled": d.enabled })).collect::<Vec<_>>(),
        "detail": detail.unwrap_or(Value::Null),
        "job": s.job.as_ref().map(|j| json!({ "title": j.title, "done": j.done, "ok": j.code == 0, "code": j.code.to_string(), "text": tail(&j.text, 1000) })).unwrap_or(Value::Null),
        "lastUrl": s.last_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn acts_by_state() {
        let l = |c: Value| detail_acts(&c).iter().map(|a| a.label).collect::<Vec<_>>().join(",");
        assert_eq!(l(json!({ "state": "running", "name": "x" })), "Stop,Restart,Pause,Kill");
        assert_eq!(l(json!({ "state": "paused" })), "Resume,Stop");
        assert_eq!(l(json!({ "state": "exited" })), "Start");
    }

    #[::core::prelude::v1::test]
    fn port_rows() {
        let ov = json!({ "ports": [{ "private": 80, "type": "tcp", "public": 8080, "ip": "127.0.0.1" }, { "private": 53, "type": "udp" }] });
        let fw = vec![json!({ "host": "web", "port": 8080, "url": "/tunnel/web/" })];
        let p = ports(&ov, Some(&fw));
        assert_eq!((p[0].port.as_str(), p[0].v.as_str(), p[0].btn, p[0].fwds.len()), ("8080", "127.0.0.1:8080", "Another", 1));
        assert_eq!((p[1].port.as_str(), p[1].v.as_str(), p[1].btn), ("x", "not published", ""));
        assert_eq!(ports(&ov, None)[0].btn, "Add forward");
    }

    #[::core::prelude::v1::test]
    fn overview() {
        let o = json!({ "state": "exited", "exitCode": 1, "oomKilled": true, "env": [{ "key": "A", "value": "1" }, { "key": "T", "value": "x", "hidden": true }], "labels": { "b": "2", "a": "1" } });
        let [st, _, _, _, env, labels] = overview_rows(&o);
        assert!(st.iter().any(|r| r.0 == "Exit code" && r.1 == "1 (out of memory)"));
        assert_eq!(env[1].1, "hidden");
        assert_eq!(labels[0].0, "a");
    }

    #[::core::prelude::v1::test]
    fn log_lines() {
        let d = Detail { lines: vec![LogRec { text: "tick 1".into(), ..Default::default() }, LogRec { text: "tock".into(), ..Default::default() }, LogRec { text: "\u{2014} ended".into(), sys: true, ..Default::default() }], ..Default::default() };
        assert_eq!(shown_lines(&d, ""), vec![0, 1, 2]);
        assert_eq!(shown_lines(&d, "TICK"), vec![0, 2]);
    }

    #[::core::prelude::v1::test]
    fn shell_words() {
        let mut d = Detail { data: json!({ "container": { "running": true, "name": "c" } }), ..Default::default() };
        assert_eq!(shell_bar(&d).1, "New shell");
        d.shell_msg = Some("The shell ended (exit 0).".into());
        assert_eq!(shell_bar(&d), ("The shell ended (exit 0).".into(), "Open again"));
        d.data["container"]["running"] = json!(false);
        d.shell_msg = None;
        assert_eq!(shell_bar(&d).0, "Start the container to open a shell in it.");
    }

    #[::core::prelude::v1::test]
    fn stats_charts() {
        let s = vec![json!({ "at": 0, "cpuPct": 1.0, "memory": 100, "netRx": 0, "pids": 3 }), json!({ "at": 2000, "cpuPct": 2.0, "memory": 200, "netRx": 2000, "pids": 3 })];
        let (c, note) = charts(&s);
        assert_eq!(c.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), vec!["CPU", "MEMORY", "NETWORK", "DISK"]);
        assert_eq!(c[0].a.len(), 2);
        assert!(note.starts_with("3 processes"));
    }
}
