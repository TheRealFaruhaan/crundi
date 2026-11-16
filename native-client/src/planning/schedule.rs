//! Schedules and webhooks (4.5, 4.6): the schedule tab, the web's
//! loadSchedules, renderSchedules, openSchedModal, renderSchedModal,
//! captureSchedForm, saveSchedule, provisionChatConversation and
//! setupScheduleHandlers (src/webapp-html.js). The list is loaded whole on
//! show, after every change and on the server's `schedule` event.
//!
//! The dialog's draft lives here (a JSON object, as the web's
//! scheduleDraft); the dialog's text fields live in PlanningSchedule and are
//! read back (`capture`) before anything re-renders it, as the web does.

#[path = "schedule_rules.rs"]
mod rules;
#[path = "schedule_webhook.rs"]
mod webhook;

use super::board;
use crate::{areas::AreaEvent, kit, MainWindow, PlanningSched, PlanningSchedCond, PlanningSchedule};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct St {
    schedules: Vec<Value>,
    services: Vec<Value>,
    /// The toolbar's filter (the web's scheduleFilter, default on).
    all: bool,
    load_gen: u64,
    /// The schedule on the dialog (None: closed).
    draft: Option<Value>,
    /// The draft's project's board (for kanban conditions).
    board: Option<Value>,
    items: Option<Rc<VecModel<PlanningSched>>>,
    conds: Option<Rc<VecModel<PlanningSchedCond>>>,
}

thread_local! { static S: RefCell<St> = RefCell::new(St::default()); }

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { S.with(|s| f(&mut s.borrow_mut())) }

fn sg(f: impl FnOnce(&PlanningSchedule)) {
    if let Some(w) = kit::ui() { f(&w.global::<PlanningSchedule>()); }
}

fn ss(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn strings(v: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(v.into_iter().map(SharedString::from).collect::<Vec<_>>()))
}

pub fn init(w: &MainWindow) {
    let g = w.global::<PlanningSchedule>();
    let items = kit::model::<PlanningSched>();
    let conds = kit::model::<PlanningSchedCond>();
    g.set_items(kit::rc(&items));
    g.set_conds(kit::rc(&conds));
    g.set_model_opts(strings(std::iter::once("Default".to_string()).chain(rules::MODELS.iter().map(|m| m.to_string())).collect()));
    g.set_effort_opts(strings(std::iter::once("Default".to_string()).chain(rules::EFFORTS.iter().map(|m| m.to_string())).collect()));
    g.set_days(ModelRc::new(VecModel::from(vec![false; 7])));
    with(|s| { s.items = Some(items); s.conds = Some(conds); });
    g.on_act(|a, id| act(a.as_str(), id.as_str()));
    g.on_seg(|grp, v| seg(grp.as_str(), v.as_str()));
    g.on_day(day);
    g.on_pick(|f, i| pick(f.as_str(), i));
    g.on_cond_pick(|ci, f, i| cond_pick(ci, f.as_str(), i));
    g.on_cond_value(|ci, v| cond_value(ci, v.as_str()));
    g.on_add_cond(|t| add_cond(t.as_str()));
    g.on_remove_cond(remove_cond);
    g.on_save(save);
    g.on_close(close);
    g.on_provision(provision);
    g.on_rotate(webhook::rotate);
}

pub fn signed_in() {
    sg(|g| g.set_has_project(!kit::project().is_empty()));
    if visible() { load(); }
}

pub fn signed_out() {
    if with(|s| s.draft.is_some()) { close(); }
    with(|s| { s.schedules.clear(); s.services.clear(); s.board = None; });
    sg(|g| g.set_state("loading".into()));
    render_list();
}

pub fn project(alias: &str) {
    sg(|g| g.set_has_project(!alias.is_empty()));
    if visible() { load(); }
}

pub fn tab_shown(tab: &str) {
    if tab == "schedule" { load(); }
}

pub fn event(e: &AreaEvent) {
    match e {
        // Any change to a schedule (here, another client, a tool) reloads the list.
        AreaEvent::Server("schedule", _) => { if visible() { load(); } }
        AreaEvent::Connected(true) => { if visible() { load(); } }
        _ => {}
    }
}

/// The tab on show (Schedules has no workbench panel).
fn visible() -> bool {
    kit::ui().map(|w| w.global::<crate::Shell>().get_tab() == "schedule").unwrap_or(false)
}

// ─── The list ───

/// GET /api/schedules (this project's, or all), then draw it.
pub fn load() {
    let p = kit::project();
    let all = with(|s| s.all);
    let proj = if !all && !p.is_empty() { p.clone() } else { String::new() };
    let gen = with(|s| { s.load_gen += 1; s.load_gen });
    let path = if proj.is_empty() { "/api/schedules".to_string() } else { format!("/api/schedules?project={}", kit::enc(&proj)) };
    kit::call(move |api| api.get(&path), move |r| {
        if with(|s| s.load_gen != gen) { return; }
        match r {
            Err(e) => sg(|g| { g.set_error(format!("Error: {e}").into()); g.set_state("error".into()); }),
            Ok(v) if !kit::truthy(&v["ok"]) => {
                let e = kit::s(&v, "error");
                sg(|g| { g.set_error(if e.is_empty() { "Failed".into() } else { e.into() }); g.set_state("error".into()); });
            }
            Ok(v) => {
                let list = v["schedules"].as_array().cloned().unwrap_or_default();
                let svcs = v["services"].as_array().cloned().unwrap_or_default();
                with(|s| { s.schedules = list; s.services = svcs; });
                sg(|g| g.set_state("ok".into()));
                render_list();
            }
        }
    });
}

fn last_run(iso: &str) -> String {
    if iso.is_empty() { String::new() } else { format!("last run {}", board::local_time(iso)) }
}

fn render_list() {
    let (rows, filtered) = with(|s| {
        let filtered = !s.all && !kit::project().is_empty();
        let rows: Vec<PlanningSched> = s.schedules.iter().map(|x| {
            let conds: Vec<String> = x["conditions"].as_array().map(|a| a.iter().filter(|c| ss(c, "type") != "time").map(rules::cond_summary).collect()).unwrap_or_default();
            PlanningSched {
                id: ss(x, "id").into(),
                name: ss(x, "name").into(),
                project: ss(x, "project").into(),
                enabled: kit::truthy(&x["enabled"]),
                summary: rules::action_summary(&x["action"], &s.services).into(),
                when: rules::when_summary(rules::effective_when(x).as_ref()).into(),
                conds: strings(conds),
                last: last_run(&ss(x, "lastRun")).into(),
            }
        }).collect();
        (rows, filtered)
    });
    sg(|g| {
        g.set_filter(!with(|s| s.all));
        g.set_has_project(!kit::project().is_empty());
        g.set_empty_text(if filtered { "No scheduled tasks for this project".into() } else { "No scheduled tasks".into() });
    });
    if let Some(m) = with(|s| s.items.clone()) { kit::sync_rows(&m, rows); }
}

fn act(a: &str, id: &str) {
    match a {
        "add" => open(None),
        "edit" => open(Some(id.to_string())),
        "filter" => { with(|s| s.all = !s.all); load(); }
        "delete" => {
            let Some(name) = with(|s| s.schedules.iter().find(|x| ss(x, "id") == id).map(|x| ss(x, "name"))) else { return };
            let id = id.to_string();
            let msg = format!("Delete schedule \"{name}\"?");
            kit::confirm(kit::Confirm { title: &msg, message: "", ok: "Delete", danger: true }, move |ok| {
                if ok { post(json!({ "action": "delete", "id": id }), |_| {}); }
            });
        }
        "enable" => {
            let Some(on) = with(|s| s.schedules.iter().find(|x| ss(x, "id") == id).map(|x| kit::truthy(&x["enabled"]))) else { return };
            post(json!({ "action": "enable", "id": id, "enabled": !on }), |_| {});
        }
        _ => {}
    }
}

/// POST /api/schedules (schedPost): a refusal in a toast, then the list again.
fn post(body: Value, then: impl FnOnce(Value) + 'static) {
    let id = super::park(then);
    kit::call(move |api| api.post("/api/schedules", &body), move |r| {
        let v = match r {
            Ok(v) => { if !kit::truthy(&v["ok"]) { let e = kit::s(&v, "error"); kit::toast_error(if e.is_empty() { "Failed" } else { &e }); } v }
            Err(e) => { kit::toast_error(&format!("Error: {e}")); json!({ "ok": false }) }
        };
        load();
        super::unpark(id, v);
    });
}

// ─── The dialog ───

fn merge(base: Value, over: Option<&Value>) -> Value {
    let mut b = base;
    if let (Some(bo), Some(Value::Object(o))) = (b.as_object_mut(), over) {
        for (k, v) in o { bo.insert(k.clone(), v.clone()); }
    }
    b
}

/// openSchedModal: a copy of the schedule (or a new one), with every field there.
fn open(id: Option<String>) {
    let cur = kit::project();
    let draft = match &id {
        Some(id) => {
            let Some(src) = with(|s| s.schedules.iter().find(|x| ss(x, "id") == *id).cloned()) else { return };
            let def = rules::default_schedule(&cur);
            let mut d = src.clone();
            d["action"] = merge(def["action"].clone(), src.get("action"));
            d["when"] = merge(def["when"].clone(), rules::effective_when(&src).as_ref());
            d["conditions"] = json!(src["conditions"].as_array().map(|a| a.iter().filter(|c| ss(c, "type") != "time").cloned().collect::<Vec<_>>()).unwrap_or_default());
            d["webhook"] = merge(def["webhook"].clone(), src.get("webhook"));
            d
        }
        None => {
            let p = if !cur.is_empty() { cur.clone() } else { kit::projects().first().map(|p| kit::s(p, "alias")).unwrap_or_default() };
            rules::default_schedule(&p)
        }
    };
    let mut draft = draft;
    // A service key by default (the project's first service).
    if ss(&draft["action"], "serviceKey").is_empty() {
        if let Some(k) = project_services(&ss(&draft, "project")).first().map(|s| ss(s, "key")) { draft["action"]["serviceKey"] = json!(k); }
    }
    let project = ss(&draft, "project");
    with(|s| { s.draft = Some(draft); s.board = None; });
    // Its board first (kanban conditions), then the dialog, as the web does.
    load_board(&project, || {
        if with(|s| s.draft.is_none()) { return; }
        sg(|g| { g.set_gen(g.get_gen() + 1); g.set_open(true); });
        if let Some(w) = kit::ui() { w.global::<crate::Shell>().set_modal_planning(true); }
        render_dialog();
    });
}

fn load_board(project: &str, then: impl FnOnce() + 'static) {
    let path = format!("/api/kanban?project={}", kit::enc(project));
    let id = super::park(move |_| then());
    kit::call(move |api| api.get(&path), move |r| {
        if let Ok(v) = r { if kit::truthy(&v["ok"]) { let b = v["board"].clone(); with(|s| s.board = Some(b)); } }
        super::unpark(id, Value::Null);
    });
}

fn close() {
    with(|s| { s.draft = None; s.board = None; });
    sg(|g| { g.set_open(false); g.set_provisioning(false); });
    if let Some(w) = kit::ui() { w.global::<crate::Shell>().set_modal_planning(false); }
}

fn project_services(project: &str) -> Vec<Value> {
    let p = project.to_lowercase();
    with(|s| s.services.iter().filter(|x| ss(x, "alias").to_lowercase() == p).cloned().collect())
}

fn tasks() -> Vec<Value> {
    with(|s| s.board.as_ref().and_then(|b| b["tasks"].as_array().cloned()).unwrap_or_default())
}

fn statuses() -> Vec<String> {
    with(|s| s.board.as_ref().map(board::statuses)).unwrap_or_else(|| board::statuses(&Value::Null))
}

/// captureSchedForm: what is typed on the dialog, into the draft.
fn capture() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningSchedule>();
    with(|s| {
        let Some(d) = s.draft.as_mut() else { return };
        d["name"] = json!(g.get_name().as_str());
        d["when"]["time"] = json!(g.get_time().as_str());
        d["when"]["date"] = json!(g.get_date().as_str());
        let a = &mut d["action"];
        a["sessionId"] = json!(g.get_session_id().as_str());
        a["prompt"] = json!(g.get_prompt().as_str());
        a["command"] = json!(g.get_command().as_str());
        a["cwd"] = json!(g.get_cwd().as_str());
        webhook::capture(&g, d);
    });
}

/// renderSchedModal: the draft onto the dialog. Drop-downs that show their
/// first option when the draft's value is not among them (a <select>'s
/// way) take that value, as the web's next capture would.
fn render_dialog() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningSchedule>();
    let projects = kit::projects();
    let tasks = tasks();
    let statuses = statuses();
    let Some(mut d) = with(|s| s.draft.clone()) else { return };

    // Project.
    let pi = projects.iter().position(|p| kit::s(p, "alias").to_lowercase() == ss(&d, "project").to_lowercase());
    if pi.is_none() { if let Some(p) = projects.first() { d["project"] = json!(kit::s(p, "alias")); } }
    let project = ss(&d, "project");
    let popts: Vec<String> = if projects.is_empty() { vec!["No projects".into()] } else {
        projects.iter().map(|p| { let n = kit::s(p, "name"); if n.is_empty() { kit::s(p, "alias") } else { n } }).collect()
    };
    let cwd_hint = projects.iter().find(|p| kit::s(p, "alias") == project).map(|p| kit::s(p, "path")).filter(|x| !x.is_empty()).unwrap_or_else(|| "/absolute/path".into());

    // Service (only read back when on show, as the web's select).
    let svcs = project_services(&project);
    let a = d["action"].clone();
    let mut si = svcs.iter().position(|x| ss(x, "key") == ss(&a, "serviceKey")).map(|i| i as i32).unwrap_or(-1);
    if ss(&a, "kind") == "service" && si < 0 {
        d["action"]["serviceKey"] = json!(svcs.first().map(|x| ss(x, "key")).unwrap_or_default());
        si = if svcs.is_empty() { -1 } else { 0 };
    }

    // Conditions (kanban: the subtask must be the task's; the status one of the options).
    let mut rows = vec![];
    if let Some(cs) = d["conditions"].as_array_mut() {
        for c in cs.iter_mut() {
            let t = ss(c, "type");
            let op = rules::OPS.iter().position(|o| *o == { let o = ss(c, "op"); if o.is_empty() { "below".to_string() } else { o } }).unwrap_or(0);
            if t != "kanban" { c["op"] = json!(rules::OPS[op]); }
            match t.as_str() {
                "usage" | "terminals" => {
                    let def = if t == "usage" { "50" } else { "1" };
                    let v = if c["value"].is_null() { def.to_string() } else { ss(c, "value") };
                    rows.push(PlanningSchedCond {
                        kind: t.clone().into(),
                        title: if t == "terminals" { "Open terminals".into() } else if ss(c, "metric") == "week" { "Weekly usage".into() } else { "5h usage".into() },
                        op: op as i32, value: v.into(), task: 0, todo: 0, todo_opts: strings(vec![]), status: 0, status_opts: strings(vec![]),
                    });
                }
                "kanban" => {
                    let ti = tasks.iter().position(|x| ss(x, "id") == ss(c, "taskId"));
                    if ti.is_none() { c["taskId"] = json!(""); }
                    let todos: Vec<Value> = ti.and_then(|i| tasks[i]["todos"].as_array().cloned()).unwrap_or_default();
                    let di = todos.iter().position(|x| ss(x, "id") == ss(c, "todoId"));
                    if di.is_none() { c["todoId"] = json!(""); }
                    let (keys, labels): (Vec<String>, Vec<String>) = if di.is_some() {
                        (rules::TODO_STATUSES.iter().map(|x| x.to_string()).collect(), rules::TODO_STATUSES.iter().map(|x| x.to_string()).collect())
                    } else {
                        (statuses.clone(), statuses.iter().map(|x| board::label(x)).collect())
                    };
                    let mut sti = keys.iter().position(|k| *k == ss(c, "status"));
                    if sti.is_none() {
                        // The web's subtask select falls back to "done" (c.status || 'done'), else the first option.
                        sti = Some(0);
                        c["status"] = json!(keys.first().cloned().unwrap_or_default());
                    }
                    rows.push(PlanningSchedCond {
                        kind: "kanban".into(), title: "Kanban".into(), op: 0, value: "".into(),
                        task: ti.map(|i| i as i32 + 1).unwrap_or(0),
                        todo: di.map(|i| i as i32 + 1).unwrap_or(0),
                        todo_opts: strings(std::iter::once("whole task".to_string()).chain(todos.iter().map(|x| ss(x, "text"))).collect()),
                        status: sti.unwrap_or(0) as i32,
                        status_opts: strings(labels),
                    });
                }
                _ => {}
            }
        }
    }
    let id = ss(&d, "id");
    let wm = d["when"].clone();
    let days: Vec<bool> = (0..7).map(|i| wm["days"].as_array().map(|a| a.iter().any(|x| x.as_i64() == Some(i))).unwrap_or(false)).collect();
    let idx = |list: &[&str], v: &str| list.iter().position(|x| *x == v).map(|i| i as i32 + 1).unwrap_or(0);

    g.set_editing(!id.is_empty());
    g.set_name(ss(&d, "name").into());
    g.set_project_opts(strings(popts));
    g.set_project(pi.map(|i| i as i32).unwrap_or(0));
    g.set_when_mode(ss(&wm, "mode").into());
    g.set_date(ss(&wm, "date").into());
    g.set_time({ let t = ss(&wm, "time"); if t.is_empty() { "09:00".into() } else { t.into() } });
    g.set_days(ModelRc::new(VecModel::from(days)));
    webhook::render(&g, &d);
    let a = d["action"].clone();
    g.set_kind(ss(&a, "kind").into());
    g.set_agent({ let x = ss(&a, "agent"); if x.is_empty() { "claude".into() } else { x.into() } });
    g.set_model(idx(&rules::MODELS, &ss(&a, "model")));
    g.set_effort(idx(&rules::EFFORTS, &ss(&a, "effort")));
    g.set_mode(ss(&a, "mode").into());
    g.set_session(ss(&a, "session").into());
    g.set_session_id(ss(&a, "sessionId").into());
    g.set_session_short(ss(&a, "sessionId").chars().take(8).collect::<String>().into());
    g.set_prompt(ss(&a, "prompt").into());
    g.set_cwd(ss(&a, "cwd").into());
    g.set_cwd_hint(cwd_hint.into());
    g.set_command(ss(&a, "command").into());
    g.set_has_services(!svcs.is_empty());
    g.set_service_opts(strings(if svcs.is_empty() { vec!["No services for this project".into()] } else { svcs.iter().map(|x| ss(x, "name")).collect() }));
    g.set_service(if svcs.is_empty() { 0 } else { si.max(0) });
    g.set_op({ let o = ss(&a, "op"); if o.is_empty() { "start".into() } else { o.into() } });
    let mut topts = vec!["\u{2014} pick task \u{2014}".to_string()];
    topts.extend(tasks.iter().map(|t| ss(t, "title")));
    g.set_task_opts(strings(topts));
    if let Some(m) = with(|s| s.conds.clone()) { kit::sync_rows(&m, rows); }
    with(|s| s.draft = Some(d));
}

fn seg(group: &str, v: &str) {
    capture();
    with(|s| {
        let Some(d) = s.draft.as_mut() else { return };
        match group {
            "kind" => d["action"]["kind"] = json!(v),
            "whenmode" => d["when"]["mode"] = json!(v),
            "hookauth" => d["webhook"]["auth"] = json!(v),
            g => d["action"][g] = json!(v), // mode | session | op | agent
        }
    });
    render_dialog();
}

fn day(i: i32) {
    capture();
    with(|s| {
        let Some(d) = s.draft.as_mut() else { return };
        let mut days: Vec<i64> = d["when"]["days"].as_array().map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default();
        if let Some(p) = days.iter().position(|x| *x == i as i64) { days.remove(p); } else { days.push(i as i64); days.sort(); }
        d["when"]["days"] = json!(days);
    });
    render_dialog();
}

fn pick(field: &str, i: i32) {
    capture();
    let i = i.max(0) as usize;
    let mut reload_board = None;
    match field {
        "project" => {
            let Some(p) = kit::projects().get(i).map(|p| kit::s(p, "alias")) else { return };
            with(|s| if let Some(d) = s.draft.as_mut() { d["project"] = json!(p); });
            reload_board = Some(p);
        }
        "model" | "effort" => {
            let list: &[&str] = if field == "model" { &rules::MODELS } else { &rules::EFFORTS };
            let v = if i == 0 { String::new() } else { list.get(i - 1).map(|x| x.to_string()).unwrap_or_default() };
            with(|s| if let Some(d) = s.draft.as_mut() { d["action"][field] = json!(v); });
        }
        "service" => {
            let p = with(|s| s.draft.as_ref().map(|d| ss(d, "project"))).unwrap_or_default();
            let Some(k) = project_services(&p).get(i).map(|x| ss(x, "key")) else { return };
            with(|s| if let Some(d) = s.draft.as_mut() { d["action"]["serviceKey"] = json!(k); });
        }
        _ => return,
    }
    match reload_board {
        // A new project: its board (for kanban conditions) and services.
        Some(p) => { with(|s| s.board = None); load_board(&p, render_dialog); }
        None => render_dialog(),
    }
}

fn cond_pick(ci: i32, field: &str, i: i32) {
    capture();
    let tasks = tasks();
    let statuses = statuses();
    with(|s| {
        let Some(c) = s.draft.as_mut().and_then(|d| d["conditions"].get_mut(ci as usize)) else { return };
        let i = i.max(0) as usize;
        match field {
            "op" => c["op"] = json!(rules::OPS.get(i).copied().unwrap_or("below")),
            "task" => c["taskId"] = json!(if i == 0 { String::new() } else { tasks.get(i - 1).map(|t| ss(t, "id")).unwrap_or_default() }),
            "todo" => {
                let todos = tasks.iter().find(|t| ss(t, "id") == ss(c, "taskId")).and_then(|t| t["todos"].as_array().cloned()).unwrap_or_default();
                c["todoId"] = json!(if i == 0 { String::new() } else { todos.get(i - 1).map(|t| ss(t, "id")).unwrap_or_default() });
            }
            "status" => {
                let v = if ss(c, "todoId").is_empty() { statuses.get(i).cloned().unwrap_or_default() } else { rules::TODO_STATUSES.get(i).map(|x| x.to_string()).unwrap_or_default() };
                c["status"] = json!(v);
            }
            _ => {}
        }
    });
    render_dialog();
}

/// A condition's number, as typed (read back without drawing again, so the caret stays).
fn cond_value(ci: i32, v: &str) {
    with(|s| {
        let Some(c) = s.draft.as_mut().and_then(|d| d["conditions"].get_mut(ci as usize)) else { return };
        c["value"] = v.trim().parse::<f64>().ok().and_then(|n| if n.fract() == 0.0 { Some(json!(n as i64)) } else { serde_json::Number::from_f64(n).map(Value::Number) }).unwrap_or_else(|| json!(v));
    });
}

fn add_cond(t: &str) {
    capture();
    let first = statuses().first().cloned().unwrap_or_default();
    let has_board = with(|s| s.board.is_some());
    let c = rules::new_cond(t, if has_board { &first } else { "" });
    with(|s| if let Some(d) = s.draft.as_mut() { if let Some(a) = d["conditions"].as_array_mut() { a.push(c); } });
    render_dialog();
}

fn remove_cond(ci: i32) {
    capture();
    with(|s| if let Some(a) = s.draft.as_mut().and_then(|d| d["conditions"].as_array_mut()) { if (ci as usize) < a.len() { a.remove(ci as usize); } });
    render_dialog();
}

/// saveSchedule: check, then add or update; a new webhook schedule stays
/// open to show its URL.
fn save() {
    capture();
    let Some(mut d) = with(|s| s.draft.clone()) else { return };
    // The web's time and date inputs only ever hold a valid value or nothing.
    d["when"]["time"] = json!(rules::norm_time(&ss(&d["when"], "time")));
    d["when"]["date"] = json!(rules::norm_date(&ss(&d["when"], "date")));
    // A time or date that reads right is shown tidied ("7:5" → "07:05"); one that doesn't stays as typed.
    let (t, dt) = (ss(&d["when"], "time"), ss(&d["when"], "date"));
    sg(|g| { if !t.is_empty() { g.set_time(t.into()); } if !dt.is_empty() { g.set_date(dt.into()); } });
    if let Err(e) = rules::validate(&d) { kit::toast_error(e); return; }
    with(|s| s.draft = Some(d.clone()));
    let hook = ss(&d["when"], "mode") == "webhook";
    let id = ss(&d, "id");
    let payload = json!({ "name": d["name"], "project": d["project"], "enabled": d["enabled"], "tz": rules::local_tz(), "action": d["action"], "when": d["when"], "conditions": d["conditions"], "webhook": d["webhook"] });
    let body = if id.is_empty() { json!({ "action": "add", "schedule": payload }) } else { json!({ "action": "update", "id": id, "schedule": payload }) };
    let pid = super::park(move |res: Value| {
        if !kit::truthy(&res["ok"]) { return; }
        load();
        if hook && id.is_empty() && res["schedule"].is_object() {
            capture();
            with(|s| if let Some(d) = s.draft.as_mut() { webhook::adopt_saved(d, &res["schedule"]); });
            kit::toast_ok("Saved. Its URL is below.");
            render_dialog();
            return;
        }
        close();
    });
    kit::call(move |api| api.post("/api/schedules", &body), move |r| {
        let v = match r {
            Ok(v) => { if !kit::truthy(&v["ok"]) { let e = kit::s(&v, "error"); kit::toast_error(&format!("Save failed: {}", if e.is_empty() { "unknown" } else { &e })); } v }
            Err(e) => { kit::toast_error(&format!("Error: {e}")); json!({ "ok": false }) }
        };
        super::unpark(pid, v);
    });
}

/// provisionChatConversation: save what is on show, then have the server
/// open a chat, say hello and keep its session id.
fn provision() {
    let Some(d0) = with(|s| s.draft.clone()) else { return };
    let id = ss(&d0, "id");
    if id.is_empty() { kit::toast_error("Save the schedule first"); return; }
    capture();
    let Some(d) = with(|s| s.draft.clone()) else { return };
    sg(|g| g.set_provisioning(true));
    let upd = json!({ "action": "update", "id": id, "schedule": { "name": d["name"], "project": d["project"], "enabled": d["enabled"], "action": d["action"], "when": d["when"], "conditions": d["conditions"] } });
    let pid = super::park(move |res: Value| {
        sg(|g| g.set_provisioning(false));
        if !kit::truthy(&res["ok"]) { let e = kit::s(&res, "error"); kit::toast_error(if e.is_empty() { "Failed" } else { &e }); return; }
        if with(|s| s.draft.as_ref().map(|d| ss(d, "id"))) != Some(ss(&d, "id")) { return; }
        capture();
        let sid = kit::s(&res, "sessionId");
        with(|s| if let Some(d) = s.draft.as_mut() { d["action"]["session"] = json!("resume"); d["action"]["sessionId"] = json!(sid); });
        kit::toast_ok("Conversation created");
        render_dialog();
    });
    kit::call(move |api| {
        let sd = api.post("/api/schedules", &upd)?;
        if !kit::truthy(&sd["ok"]) { let e = kit::s(&sd, "error"); return Ok(json!({ "ok": false, "error": if e.is_empty() { "Could not save first".to_string() } else { e } })); }
        api.post("/api/schedules", &json!({ "action": "provisionChat", "id": id }))
    }, move |r| {
        let v = match r { Ok(v) => v, Err(e) => json!({ "ok": false, "error": e }) };
        super::unpark(pid, v);
    });
}

// ─── For the test driver ───

/// The driver's "planning:schedule-*" actions.
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let n = |i: usize| a(i).parse::<i32>().unwrap_or(0);
    Some(match name {
        "schedule-act" => { act(&a(0), &a(1)); json!({ "ok": true }) }
        "schedule-seg" => { seg(&a(0), &a(1)); json!({ "ok": true }) }
        "schedule-day" => { day(n(0)); json!({ "ok": true }) }
        "schedule-pick" => { pick(&a(0), n(1)); json!({ "ok": true }) }
        "schedule-cond-pick" => { cond_pick(n(0), &a(1), n(2)); json!({ "ok": true }) }
        "schedule-cond-value" => { cond_value(n(0), &a(1)); json!({ "ok": true }) }
        "schedule-add-cond" => { add_cond(&a(0)); json!({ "ok": true }) }
        "schedule-remove-cond" => { remove_cond(n(0)); json!({ "ok": true }) }
        // Type into a field of the dialog: name, date, time, prompt, command, cwd, session-id, sig-header, ts-header, idem.
        "schedule-type" => {
            let v: SharedString = a(1).into();
            sg(|g| match a(0).as_str() {
                "name" => g.set_name(v), "date" => g.set_date(v), "time" => g.set_time(v), "prompt" => g.set_prompt(v),
                "command" => g.set_command(v), "cwd" => g.set_cwd(v), "session-id" => g.set_session_id(v),
                "sig-header" => g.set_sig_header(v), "ts-header" => g.set_ts_header(v), "idem" => g.set_idem(v), _ => {}
            });
            json!({ "ok": true })
        }
        "schedule-save" => { save(); json!({ "ok": true }) }
        "schedule-close" => { close(); json!({ "ok": true }) }
        "schedule-provision" => { provision(); json!({ "ok": true }) }
        "schedule-rotate" => { webhook::rotate(); json!({ "ok": true }) }
        "schedule-reload" => { load(); json!({ "ok": true }) }
        _ => return None,
    })
}

/// props.areas.planning.schedule
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return Value::Null };
    let g = w.global::<PlanningSchedule>();
    let list = |m: ModelRc<SharedString>| m.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let dialog = if g.get_open() {
        json!({
            "title": if g.get_editing() { "Edit schedule" } else { "New schedule" },
            "name": g.get_name().as_str(), "projects": list(g.get_project_opts()), "project": g.get_project(),
            "when": g.get_when_mode().as_str(), "date": g.get_date().as_str(), "time": g.get_time().as_str(),
            "days": g.get_days().iter().collect::<Vec<_>>(),
            "hookAuth": g.get_hook_auth().as_str(), "sigHeader": g.get_sig_header().as_str(), "tsHeader": g.get_ts_header().as_str(), "idem": g.get_idem().as_str(),
            "hookUrl": g.get_hook_url().as_str(), "hookSecret": g.get_hook_secret().as_str(),
            "kind": g.get_kind().as_str(), "agent": g.get_agent().as_str(), "model": g.get_model(), "effort": g.get_effort(), "mode": g.get_mode().as_str(),
            "session": g.get_session().as_str(), "sessionId": g.get_session_id().as_str(), "provisioning": g.get_provisioning(),
            "prompt": g.get_prompt().as_str(), "cwd": g.get_cwd().as_str(), "cwdHint": g.get_cwd_hint().as_str(), "command": g.get_command().as_str(),
            "services": list(g.get_service_opts()), "hasServices": g.get_has_services(), "service": g.get_service(), "op": g.get_op().as_str(),
            "tasks": list(g.get_task_opts()),
            "conds": g.get_conds().iter().map(|c| json!({ "kind": c.kind.as_str(), "title": c.title.as_str(), "op": c.op, "value": c.value.as_str(),
                "task": c.task, "todo": c.todo, "todos": list(c.todo_opts.clone()), "status": c.status, "statuses": list(c.status_opts.clone()) })).collect::<Vec<_>>(),
            "draft": with(|s| s.draft.clone()).unwrap_or(Value::Null),
        })
    } else { Value::Null };
    json!({
        "state": g.get_state().as_str(), "error": g.get_error().as_str(), "hasProject": g.get_has_project(), "filter": g.get_filter(),
        "empty": g.get_empty_text().as_str(),
        "items": g.get_items().iter().map(|x| json!({ "id": x.id.as_str(), "name": x.name.as_str(), "project": x.project.as_str(), "enabled": x.enabled,
            "summary": x.summary.as_str(), "when": x.when.as_str(), "conds": list(x.conds.clone()), "last": x.last.as_str() })).collect::<Vec<_>>(),
        "dialog": dialog,
    })
}
