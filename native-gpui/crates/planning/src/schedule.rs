//! Planning 4.5, 4.6: Schedules (the list, the add / edit dialog) and webhooks (phase 4 on GPUI).
//! Port of the Slint client's native-client/src/planning/schedule*.rs and ui/planning/schedule*.slint (its
//! logic as it was; only the screens are redrawn), checked by native-client/test/planning/schedule.mjs.
//! Spec: the web UI (src/webapp-html.js: loadSchedules, renderSchedules, openSchedModal,
//! renderSchedModal, captureSchedForm, saveSchedule, provisionChatConversation).
//!
//! One entity (`Sched`) holds the list, the dialog's draft (a JSON object, as the web's
//! scheduleDraft) and the dialog's text boxes; the boxes are read back into the draft
//! (`capture`) before any change to it, as the web does. The list loads whole on show,
//! after every change and on the server's `schedule` event.

#[path = "schedule_rules.rs"]
mod rules;
#[path = "schedule_webhook.rs"]
mod webhook;

use crundi_shell::AreaEvent;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::rc::Rc;

// ─── Small helpers ───

fn ss(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn merge(base: Value, over: Option<&Value>) -> Value {
    let mut b = base;
    if let (Some(bo), Some(Value::Object(o))) = (b.as_object_mut(), over) {
        for (k, v) in o { bo.insert(k.clone(), v.clone()); }
    }
    b
}

fn projects(cx: &App) -> Vec<Value> { cx.try_global::<crundi_shell::Session>().map(|s| s.projects.clone()).unwrap_or_default() }
fn base(cx: &App) -> String { crundi_shell::api(cx).map(|a| a.base().to_string()).unwrap_or_default() }

fn toast_err(window: &mut Window, cx: &mut App, t: &str) { w::toast(window, cx, t.to_string(), w::Toast::Error); }
fn toast_ok(window: &mut Window, cx: &mut App, t: &str) { w::toast(window, cx, t.to_string(), w::Toast::Success); }

/// The web's "last run …" (toLocaleString).
fn last_run(iso: &str) -> String {
    if iso.is_empty() { return String::new(); }
    let t = crundi_core::sched::when_label(&json!({ "trigger": { "type": "time", "at": iso } }));
    format!("last run {}", if t.is_empty() { iso.to_string() } else { t })
}

/// The window, later (a server's answer), with the screen.
fn in_view(cx: &mut App, f: impl FnOnce(&mut Sched, &mut Window, &mut Context<Sched>) + 'static) {
    let Some(e) = get(cx) else { return };
    if let Some(h) = cx.windows().first().copied() {
        let _ = cx.update_window(h, move |_, window, cx| e.update(cx, |s, cx| f(s, window, cx)));
    }
}

// ─── The state ───

struct Fields {
    name: Entity<w::InputState>,
    date: Entity<w::InputState>,
    time: Entity<w::InputState>,
    session_id: Entity<w::InputState>,
    cwd: Entity<w::InputState>,
    command: Entity<w::InputState>,
    sig: Entity<w::InputState>,
    ts: Entity<w::InputState>,
    idem: Entity<w::InputState>,
    url: Entity<w::InputState>,
    secret: Entity<w::InputState>,
    prompt: Entity<w::TextareaState>,
}

pub struct Sched {
    schedules: Vec<Value>,
    services: Vec<Value>,
    /// The toolbar's filter off (the web's scheduleFilter, on by default).
    all: bool,
    load_gen: u64,
    /// "loading", "error", "ok".
    state: &'static str,
    error: String,
    /// The schedule on the dialog (None: closed) and its project's board (kanban conditions).
    draft: Option<Value>,
    board: Option<Value>,
    /// The dialog on show (once the draft's board has loaded).
    open_flag: bool,
    /// The folder's and the prompt's placeholders on show.
    ph: (String, &'static str),
    provisioning: bool,
    f: Fields,
    /// One number box per extra condition (used by usage and terminals rows).
    cond_vals: Vec<Entity<w::InputState>>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    shown: bool,
}

struct Handle(Entity<Sched>);
impl Global for Handle {}

fn get(cx: &App) -> Option<Entity<Sched>> { cx.try_global::<Handle>().map(|h| h.0.clone()) }

fn ensure(window: &mut Window, cx: &mut App) -> Entity<Sched> {
    if let Some(e) = get(cx) { return e; }
    let shown = cx.try_global::<Shown>().is_some_and(|s| s.0);
    let e = cx.new(|cx| Sched::new(shown, window, cx));
    cx.set_global(Handle(e.clone()));
    e.update(cx, |s, cx| s.load(cx));
    e
}

/// The tab on show, before the screen exists.
#[derive(Default)]
struct Shown(bool);
impl Global for Shown {}

/// The dialog's derived parts (renderSchedModal's), from the draft.
#[derive(Default)]
struct Dm {
    popts: Vec<String>,
    pi: usize,
    cwd_hint: String,
    svcs: Vec<String>,
    has_services: bool,
    si: usize,
    rows: Vec<Row>,
    task_opts: Vec<String>,
    days: [bool; 7],
    model: usize,
    effort: usize,
    url: String,
    secret: String,
}

#[derive(Default, Clone)]
struct Row {
    kind: String,
    title: String,
    op: usize,
    task: usize,
    todo: usize,
    todo_opts: Vec<String>,
    status: usize,
    status_opts: Vec<String>,
}

impl Sched {
    fn new(shown: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let inp = |ph: &str, window: &mut Window, cx: &mut Context<Self>| { let ph = ph.to_string(); cx.new(|cx| w::InputState::new(window, cx).placeholder(ph)) };
        let f = Fields {
            name: inp("My scheduled task", window, cx),
            date: inp("YYYY-MM-DD", window, cx),
            time: inp("HH:MM", window, cx),
            session_id: inp("session id e.g. 8441d374-fd1a-...", window, cx),
            cwd: inp("/absolute/path", window, cx),
            command: inp("e.g. npm run build", window, cx),
            sig: inp("", window, cx),
            ts: inp("X-Grafana-Alerting-Signature-Timestamp", window, cx),
            idem: inp("{{payload.messageId}}", window, cx),
            url: inp("", window, cx),
            secret: inp("", window, cx),
            prompt: cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(3, 10).placeholder("What should the agent do?")),
        };
        Sched {
            schedules: vec![], services: vec![], all: false, load_gen: 0, state: "loading", error: String::new(),
            draft: None, board: None, open_flag: false, ph: (String::new(), ""), provisioning: false, f, cond_vals: vec![],
            focus: cx.focus_handle(), scroll: ScrollHandle::new(), shown,
        }
    }

    // ─── The list ───

    /// GET /api/schedules (this project's, or all).
    fn load(&mut self, cx: &mut Context<Self>) {
        let p = crundi_shell::project(cx);
        let proj = if !self.all && !p.is_empty() { p } else { String::new() };
        self.load_gen += 1;
        let generation = self.load_gen;
        let path = if proj.is_empty() { "/api/schedules".to_string() } else { format!("/api/schedules?project={}", enc(&proj)) };
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
            let _ = me.update(cx, |s, cx| {
                if s.load_gen != generation { return; }
                match r {
                    Err(e) => { s.error = format!("Error: {e}"); s.state = "error"; }
                    Ok(v) if !truthy(&v["ok"]) => { let e = ss(&v, "error"); s.error = if e.is_empty() { "Failed".into() } else { e }; s.state = "error"; }
                    Ok(v) => {
                        s.schedules = v["schedules"].as_array().cloned().unwrap_or_default();
                        s.services = v["services"].as_array().cloned().unwrap_or_default();
                        s.state = "ok";
                    }
                }
                cx.notify();
            });
        });
    }

    /// POST /api/schedules (schedPost): a refusal in a toast, then the list again.
    fn post(&mut self, body: Value, cx: &mut Context<Self>, then: impl FnOnce(Value, &mut Sched, &mut Window, &mut Context<Sched>) + 'static) {
        crundi_shell::call(cx, move |api| api.post("/api/schedules", &body), move |r, cx| {
            in_view(cx, move |s, window, cx| {
                let v = match r {
                    Ok(v) => { if !truthy(&v["ok"]) { let e = ss(&v, "error"); toast_err(window, cx, if e.is_empty() { "Failed" } else { &e }); } v }
                    Err(e) => { toast_err(window, cx, &format!("Error: {e}")); json!({ "ok": false }) }
                };
                s.load(cx);
                then(v, s, window, cx);
            });
        });
    }

    fn act(&mut self, a: &str, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match a {
            "add" => self.open(None, cx),
            "edit" => self.open(Some(id.to_string()), cx),
            "filter" => { self.all = !self.all; self.load(cx); cx.notify(); }
            "delete" => {
                let Some(name) = self.schedules.iter().find(|x| ss(x, "id") == id).map(|x| ss(x, "name")) else { return };
                let id = id.to_string();
                let me = cx.entity().downgrade();
                w::ask_confirm(window, cx, w::Ask { title: format!("Delete schedule \"{name}\"?").into(), ok: "Delete".into(), danger: true, ..Default::default() }, move |ok, _, cx| {
                    if !ok { return; }
                    let id = id.clone();
                    let _ = me.update(cx, |s, cx| s.post(json!({ "action": "delete", "id": id }), cx, |_, _, _, _| {}));
                });
            }
            "enable" => {
                let Some(on) = self.schedules.iter().find(|x| ss(x, "id") == id).map(|x| truthy(&x["enabled"])) else { return };
                self.post(json!({ "action": "enable", "id": id, "enabled": !on }), cx, |_, _, _, _| {});
            }
            _ => {}
        }
    }

    // ─── The dialog ───

    fn project_services(&self, project: &str) -> Vec<Value> {
        let p = project.to_lowercase();
        self.services.iter().filter(|x| ss(x, "alias").to_lowercase() == p).cloned().collect()
    }

    fn tasks(&self) -> Vec<Value> { self.board.as_ref().and_then(|b| b["tasks"].as_array().cloned()).unwrap_or_default() }
    fn statuses(&self) -> Vec<String> { rules::statuses(self.board.as_ref().unwrap_or(&Value::Null)) }

    /// openSchedModal: a copy of the schedule (or a new one) with every field
    /// there; its project's board first (kanban conditions), then the dialog.
    fn open(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let cur = crundi_shell::project(cx);
        let mut draft = match &id {
            Some(id) => {
                let Some(src) = self.schedules.iter().find(|x| ss(x, "id") == *id).cloned() else { return };
                let def = rules::default_schedule(&cur);
                let mut d = src.clone();
                d["action"] = merge(def["action"].clone(), src.get("action"));
                d["when"] = merge(def["when"].clone(), rules::effective_when(&src).as_ref());
                d["conditions"] = json!(src["conditions"].as_array().map(|a| a.iter().filter(|c| ss(c, "type") != "time").cloned().collect::<Vec<_>>()).unwrap_or_default());
                d["webhook"] = merge(def["webhook"].clone(), src.get("webhook"));
                d
            }
            None => {
                let p = if !cur.is_empty() { cur.clone() } else { projects(cx).first().map(|p| ss(p, "alias")).unwrap_or_default() };
                rules::default_schedule(&p)
            }
        };
        // A service key by default (the project's first service).
        if ss(&draft["action"], "serviceKey").is_empty() {
            if let Some(k) = self.project_services(&ss(&draft, "project")).first().map(|s| ss(s, "key")) { draft["action"]["serviceKey"] = json!(k); }
        }
        let project = ss(&draft, "project");
        self.draft = Some(draft);
        self.board = None;
        self.provisioning = false;
        self.load_board(&project, cx, |s, window, cx| {
            if s.draft.is_none() { return; }
            s.fill(window, cx);
            s.f.name.update(cx, |st, cx| st.focus(window, cx));
            s.open_flag = true;
            cx.notify();
        });
    }

    fn load_board(&mut self, project: &str, cx: &mut Context<Self>, then: impl FnOnce(&mut Sched, &mut Window, &mut Context<Sched>) + 'static) {
        let path = format!("/api/kanban?project={}", enc(project));
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
            in_view(cx, move |s, window, cx| {
                if let Ok(v) = r { if truthy(&v["ok"]) { s.board = Some(v["board"].clone()); } }
                then(s, window, cx);
            });
        });
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.draft = None;
        self.board = None;
        self.open_flag = false;
        self.provisioning = false;
        cx.notify();
    }

    fn val(e: &Entity<w::InputState>, cx: &App) -> String { e.read(cx).value().to_string() }

    fn set(e: &Entity<w::InputState>, v: &str, window: &mut Window, cx: &mut App) {
        let v = v.to_string();
        e.update(cx, |st, cx| if st.value().as_ref() != v { st.set_value(v, window, cx) });
    }

    /// captureSchedForm: what is typed on the dialog, into the draft.
    fn capture(&mut self, cx: &App) {
        let f = &self.f;
        let vals: Vec<String> = self.cond_vals.iter().map(|e| Self::val(e, cx)).collect();
        let Some(d) = self.draft.as_mut() else { return };
        d["name"] = json!(Self::val(&f.name, cx));
        d["when"]["time"] = json!(Self::val(&f.time, cx));
        d["when"]["date"] = json!(Self::val(&f.date, cx));
        let a = &mut d["action"];
        a["sessionId"] = json!(Self::val(&f.session_id, cx));
        a["prompt"] = json!(f.prompt.read(cx).value().to_string());
        a["command"] = json!(Self::val(&f.command, cx));
        a["cwd"] = json!(Self::val(&f.cwd, cx));
        webhook::capture(f, d, cx);
        if let Some(cs) = d["conditions"].as_array_mut() {
            for (c, v) in cs.iter_mut().zip(vals) {
                let t = ss(c, "type");
                if t == "usage" || t == "terminals" {
                    c["value"] = v.trim().parse::<f64>().ok()
                        .and_then(|n| if n.fract() == 0.0 { Some(json!(n as i64)) } else { serde_json::Number::from_f64(n).map(Value::Number) })
                        .unwrap_or_else(|| json!(v));
                }
            }
        }
    }

    /// The draft onto the dialog's text boxes (renderSchedModal's values).
    fn fill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.draft.clone() else { return };
        let f = &self.f;
        let wm = &d["when"];
        let a = &d["action"];
        Self::set(&f.name, &ss(&d, "name"), window, cx);
        Self::set(&f.date, &ss(wm, "date"), window, cx);
        Self::set(&f.time, &{ let t = ss(wm, "time"); if t.is_empty() { "09:00".to_string() } else { t } }, window, cx);
        Self::set(&f.session_id, &ss(a, "sessionId"), window, cx);
        Self::set(&f.cwd, &ss(a, "cwd"), window, cx);
        Self::set(&f.command, &ss(a, "command"), window, cx);
        let p = ss(a, "prompt");
        f.prompt.update(cx, |st, cx| if st.value().as_ref() != p { st.set_value(p, window, cx) });
        webhook::fill(f, &d, window, cx);
        self.sync_conds(window, cx);
    }

    /// One number box per condition, holding the draft's value.
    fn sync_conds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cs = self.draft.as_ref().and_then(|d| d["conditions"].as_array().cloned()).unwrap_or_default();
        self.cond_vals.truncate(cs.len());
        while self.cond_vals.len() < cs.len() { self.cond_vals.push(cx.new(|cx| w::InputState::new(window, cx))); }
        for (c, e) in cs.iter().zip(self.cond_vals.clone()) {
            let t = ss(c, "type");
            let v = if c["value"].is_null() { if t == "usage" { "50".into() } else { "1".into() } } else { ss(c, "value") };
            Self::set(&e, &v, window, cx);
        }
    }

    /// renderSchedModal: the derived parts; drop-downs that show their first
    /// option when the draft's value is not among them (a <select>'s way)
    /// take that value, as the web's next capture would.
    fn model(&mut self, cx: &App) -> Dm {
        let projects = projects(cx);
        let tasks = self.tasks();
        let statuses = self.statuses();
        let base = base(cx);
        let Some(mut d) = self.draft.clone() else { return Dm::default() };
        let mut m = Dm::default();

        let pi = projects.iter().position(|p| ss(p, "alias").to_lowercase() == ss(&d, "project").to_lowercase());
        if pi.is_none() { if let Some(p) = projects.first() { d["project"] = json!(ss(p, "alias")); } }
        m.pi = pi.unwrap_or(0);
        let project = ss(&d, "project");
        m.popts = if projects.is_empty() { vec!["No projects".into()] } else {
            projects.iter().map(|p| { let n = ss(p, "name"); if n.is_empty() { ss(p, "alias") } else { n } }).collect()
        };
        m.cwd_hint = projects.iter().find(|p| ss(p, "alias") == project).map(|p| ss(p, "path")).filter(|x| !x.is_empty()).unwrap_or_else(|| "/absolute/path".into());

        let svcs = self.project_services(&project);
        let mut si = svcs.iter().position(|x| ss(x, "key") == ss(&d["action"], "serviceKey"));
        if ss(&d["action"], "kind") == "service" && si.is_none() {
            d["action"]["serviceKey"] = json!(svcs.first().map(|x| ss(x, "key")).unwrap_or_default());
            si = if svcs.is_empty() { None } else { Some(0) };
        }
        m.has_services = !svcs.is_empty();
        m.svcs = if svcs.is_empty() { vec!["No services for this project".into()] } else { svcs.iter().map(|x| ss(x, "name")).collect() };
        m.si = si.unwrap_or(0);

        if let Some(cs) = d["conditions"].as_array_mut() {
            for c in cs.iter_mut() {
                let t = ss(c, "type");
                let op = rules::OPS.iter().position(|o| *o == { let o = ss(c, "op"); if o.is_empty() { "below".to_string() } else { o } }).unwrap_or(0);
                if t != "kanban" { c["op"] = json!(rules::OPS[op]); }
                match t.as_str() {
                    "usage" | "terminals" => m.rows.push(Row {
                        kind: t.clone(),
                        title: if t == "terminals" { "Open terminals".into() } else if ss(c, "metric") == "week" { "Weekly usage".into() } else { "5h usage".into() },
                        op, ..Default::default()
                    }),
                    "kanban" => {
                        let ti = tasks.iter().position(|x| ss(x, "id") == ss(c, "taskId"));
                        if ti.is_none() { c["taskId"] = json!(""); }
                        let todos: Vec<Value> = ti.and_then(|i| tasks[i]["todos"].as_array().cloned()).unwrap_or_default();
                        let di = todos.iter().position(|x| ss(x, "id") == ss(c, "todoId"));
                        if di.is_none() { c["todoId"] = json!(""); }
                        let (keys, labels): (Vec<String>, Vec<String>) = if di.is_some() {
                            (rules::TODO_STATUSES.iter().map(|x| x.to_string()).collect(), rules::TODO_STATUSES.iter().map(|x| x.to_string()).collect())
                        } else {
                            (statuses.clone(), statuses.iter().map(|x| rules::label(x)).collect())
                        };
                        let sti = match keys.iter().position(|k| *k == ss(c, "status")) {
                            Some(i) => i,
                            None => { c["status"] = json!(keys.first().cloned().unwrap_or_default()); 0 }
                        };
                        m.rows.push(Row {
                            kind: "kanban".into(), title: "Kanban".into(), op: 0,
                            task: ti.map(|i| i + 1).unwrap_or(0),
                            todo: di.map(|i| i + 1).unwrap_or(0),
                            todo_opts: std::iter::once("whole task".to_string()).chain(todos.iter().map(|x| ss(x, "text"))).collect(),
                            status: sti, status_opts: labels,
                        });
                    }
                    _ => m.rows.push(Row { kind: t.clone(), title: t.clone(), ..Default::default() }),
                }
            }
        }
        let wm = &d["when"];
        for i in 0..7 { m.days[i] = wm["days"].as_array().map(|a| a.iter().any(|x| x.as_i64() == Some(i as i64))).unwrap_or(false); }
        let idx = |list: &[&str], v: &str| list.iter().position(|x| *x == v).map(|i| i + 1).unwrap_or(0);
        m.model = idx(&rules::MODELS, &ss(&d["action"], "model"));
        m.effort = idx(&rules::EFFORTS, &ss(&d["action"], "effort"));
        m.task_opts = std::iter::once("\u{2014} pick task \u{2014}".to_string()).chain(tasks.iter().map(|t| ss(t, "title"))).collect();
        (m.url, m.secret) = webhook::url_secret(&d, &base);
        self.draft = Some(d);
        m
    }

    fn seg(&mut self, group: &str, v: &str, cx: &mut Context<Self>) {
        self.capture(cx);
        let Some(d) = self.draft.as_mut() else { return };
        match group {
            "kind" => d["action"]["kind"] = json!(v),
            "whenmode" => d["when"]["mode"] = json!(v),
            "hookauth" => d["webhook"]["auth"] = json!(v),
            g => d["action"][g] = json!(v), // mode | session | op | agent
        }
        cx.notify();
    }

    fn day(&mut self, i: i64, cx: &mut Context<Self>) {
        self.capture(cx);
        let Some(d) = self.draft.as_mut() else { return };
        let mut days: Vec<i64> = d["when"]["days"].as_array().map(|a| a.iter().filter_map(|x| x.as_i64()).collect()).unwrap_or_default();
        if let Some(p) = days.iter().position(|x| *x == i) { days.remove(p); } else { days.push(i); days.sort(); }
        d["when"]["days"] = json!(days);
        cx.notify();
    }

    fn pick(&mut self, field: &str, i: usize, cx: &mut Context<Self>) {
        self.capture(cx);
        match field {
            "project" => {
                let Some(p) = projects(cx).get(i).map(|p| ss(p, "alias")) else { return };
                if let Some(d) = self.draft.as_mut() { d["project"] = json!(p); }
                // A new project: its board (for kanban conditions) and services.
                self.board = None;
                self.load_board(&p, cx, |_, _, cx| cx.notify());
            }
            "model" | "effort" => {
                let list: &[&str] = if field == "model" { &rules::MODELS } else { &rules::EFFORTS };
                let v = if i == 0 { String::new() } else { list.get(i - 1).map(|x| x.to_string()).unwrap_or_default() };
                if let Some(d) = self.draft.as_mut() { d["action"][field] = json!(v); }
            }
            "service" => {
                let p = self.draft.as_ref().map(|d| ss(d, "project")).unwrap_or_default();
                let Some(k) = self.project_services(&p).get(i).map(|x| ss(x, "key")) else { return };
                if let Some(d) = self.draft.as_mut() { d["action"]["serviceKey"] = json!(k); }
            }
            _ => return,
        }
        cx.notify();
    }

    fn cond_pick(&mut self, ci: usize, field: &str, i: usize, cx: &mut Context<Self>) {
        self.capture(cx);
        let tasks = self.tasks();
        let statuses = self.statuses();
        let Some(c) = self.draft.as_mut().and_then(|d| d["conditions"].get_mut(ci)) else { return };
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
        cx.notify();
    }

    fn add_cond(&mut self, t: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.capture(cx);
        let first = self.statuses().first().cloned().unwrap_or_default();
        let c = rules::new_cond(t, if self.board.is_some() { &first } else { "" });
        if let Some(a) = self.draft.as_mut().and_then(|d| d["conditions"].as_array_mut()) { a.push(c); }
        self.sync_conds(window, cx);
        cx.notify();
    }

    fn remove_cond(&mut self, ci: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.capture(cx);
        if let Some(a) = self.draft.as_mut().and_then(|d| d["conditions"].as_array_mut()) { if ci < a.len() { a.remove(ci); } }
        self.sync_conds(window, cx);
        cx.notify();
    }

    /// saveSchedule: check, then add or update; a new webhook schedule stays
    /// open to show its URL.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture(cx);
        let _ = self.model(cx);
        let Some(mut d) = self.draft.clone() else { return };
        // The web's time and date inputs only ever hold a valid value or nothing.
        d["when"]["time"] = json!(rules::norm_time(&ss(&d["when"], "time")));
        d["when"]["date"] = json!(rules::norm_date(&ss(&d["when"], "date")));
        // A time or date that reads right is shown tidied ("7:5" → "07:05"); one that doesn't stays as typed.
        let (t, dt) = (ss(&d["when"], "time"), ss(&d["when"], "date"));
        if !t.is_empty() { Self::set(&self.f.time, &t, window, cx); }
        if !dt.is_empty() { Self::set(&self.f.date, &dt, window, cx); }
        if let Err(e) = rules::validate(&d) { toast_err(window, cx, e); return; }
        self.draft = Some(d.clone());
        let hook = ss(&d["when"], "mode") == "webhook";
        let id = ss(&d, "id");
        let payload = json!({ "name": d["name"], "project": d["project"], "enabled": d["enabled"], "tz": rules::local_tz(), "action": d["action"], "when": d["when"], "conditions": d["conditions"], "webhook": d["webhook"] });
        let body = if id.is_empty() { json!({ "action": "add", "schedule": payload }) } else { json!({ "action": "update", "id": id, "schedule": payload }) };
        crundi_shell::call(cx, move |api| api.post("/api/schedules", &body), move |r, cx| {
            in_view(cx, move |s, window, cx| {
                let res = match r {
                    Ok(v) => { if !truthy(&v["ok"]) { let e = ss(&v, "error"); toast_err(window, cx, &format!("Save failed: {}", if e.is_empty() { "unknown" } else { &e })); } v }
                    Err(e) => { toast_err(window, cx, &format!("Error: {e}")); json!({ "ok": false }) }
                };
                if !truthy(&res["ok"]) { return; }
                s.load(cx);
                if hook && id.is_empty() && res["schedule"].is_object() && s.draft.is_some() {
                    s.capture(cx);
                    if let Some(d) = s.draft.as_mut() { webhook::adopt_saved(d, &res["schedule"]); }
                    s.fill(window, cx);
                    toast_ok(window, cx, "Saved. Its URL is below.");
                    cx.notify();
                    return;
                }
                s.close(cx);
            });
        });
    }

    /// provisionChatConversation: save what is on show, then have the server
    /// open a chat, say hello and keep its session id.
    fn provision(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d0) = self.draft.clone() else { return };
        let id = ss(&d0, "id");
        if id.is_empty() { toast_err(window, cx, "Save the schedule first"); return; }
        self.capture(cx);
        let Some(d) = self.draft.clone() else { return };
        self.provisioning = true;
        cx.notify();
        let upd = json!({ "action": "update", "id": id, "schedule": { "name": d["name"], "project": d["project"], "enabled": d["enabled"], "action": d["action"], "when": d["when"], "conditions": d["conditions"] } });
        let pid = id.clone();
        crundi_shell::call(cx, move |api| {
            let sd = api.post("/api/schedules", &upd)?;
            if !truthy(&sd["ok"]) { let e = ss(&sd, "error"); return Ok(json!({ "ok": false, "error": if e.is_empty() { "Could not save first".to_string() } else { e } })); }
            api.post("/api/schedules", &json!({ "action": "provisionChat", "id": pid }))
        }, move |r, cx| {
            in_view(cx, move |s, window, cx| {
                s.provisioning = false;
                cx.notify();
                let res = match r { Ok(v) => v, Err(e) => json!({ "ok": false, "error": e }) };
                if !truthy(&res["ok"]) { let e = ss(&res, "error"); toast_err(window, cx, if e.is_empty() { "Failed" } else { &e }); return; }
                if s.draft.as_ref().map(|d| ss(d, "id")) != Some(id.clone()) { return; }
                s.capture(cx);
                let sid = ss(&res, "sessionId");
                if let Some(d) = s.draft.as_mut() { d["action"]["session"] = json!("resume"); d["action"]["sessionId"] = json!(sid); }
                s.fill(window, cx);
                toast_ok(window, cx, "Conversation created");
            });
        });
    }
}

// Whether the dialog is on show lives beside the draft (the draft is set
// before its board loads; the dialog shows once it has).
impl Sched {
    fn is_open(&self) -> bool { self.open_flag && self.draft.is_some() }
}

// ─── What it draws ───

/// A field's label (.sm-label) with its hint (.sm-hint, not upper case).
fn label(text: &'static str, hint: &str, first: bool) -> Div {
    div().flex().items_center().gap(T::SP_4).when(!first, |d| d.pt(T::SP_12)).pb(T::SP_4)
        .child(w::field_label(text))
        .when(!hint.is_empty(), |d| d.child(div().flex_1().min_w_0().text_size(T::TEXT_11).text_color(T::text_muted()).child(hint.to_string())))
}

/// A note under a field (.sm-note).
fn note(text: impl Into<SharedString>) -> Div {
    div().pt(T::SP_6).text_size(T::TEXT_12).line_height(relative(T::LH_COMPOSER)).text_color(T::text_muted()).child(text.into())
}

fn opts(v: &[String]) -> Vec<SharedString> { v.iter().map(|s| SharedString::from(s.clone())).collect() }

impl Sched {
    fn weak_pick(cx: &mut Context<Self>, f: impl Fn(&mut Sched, usize, &mut Window, &mut Context<Sched>) + 'static) -> impl Fn(usize, &mut Window, &mut App) + 'static {
        let me = cx.entity().downgrade();
        let f = Rc::new(f);
        move |i, window, cx| { let f = f.clone(); let _ = me.update(cx, |s, cx| f(s, i, window, cx)); }
    }

    fn seg_ctl(&self, id: &'static str, group: &'static str, labels: &[&'static str], keys: &'static [&'static str], on: usize, cx: &mut Context<Self>) -> Div {
        let me = cx.entity().downgrade();
        // As wide as its choices (the web's .sm-seg), not the form's width.
        div().flex().child(w::segmented(id, labels, on, move |i, _, cx| {
            let k = keys.get(*i).copied().unwrap_or("");
            let _ = me.update(cx, |s, cx| s.seg(group, k, cx));
        }))
    }

    fn render_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let has_project = !crundi_shell::project(cx).is_empty();
        let filter = !self.all;
        let tb = w::toolbar().px(T::SP_12).py(T::SP_10).gap(T::SP_8)
            .child(w::primary("sched-add", "+ Add schedule").on_click(cx.listener(|s, _, window, cx| s.act("add", "", window, cx))))
            .child(div().flex_1())
            .when(has_project, |d| d.child(w::toggle("sched-filter", if filter { "This project" } else { "All projects" }, filter)
                .on_click(cx.listener(|s, _, window, cx| s.act("filter", "", window, cx)))));
        let show_proj = !filter || !has_project;
        let body = if self.schedules.is_empty() {
            w::empty(if filter && has_project { "No scheduled tasks for this project" } else { "No scheduled tasks" }).into_any_element()
        } else {
            let mut list = div().id("sched-list").flex().flex_col().flex_1().min_h_0().overflow_y_scroll().px(T::SP_12).py(T::SP_10).gap(T::SP_8);
            for (i, x) in self.schedules.clone().iter().enumerate() {
                list = list.child(self.item(i, x, show_proj, cx));
            }
            list.into_any_element()
        };
        div().flex().flex_col().size_full().child(tb).child(body).into_any_element()
    }

    /// One schedule (.sched-item): the switch, the body (a click edits), Edit and Delete.
    fn item(&self, i: usize, x: &Value, show_proj: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = ss(x, "id");
        let on = truthy(&x["enabled"]);
        let summary = rules::action_summary(&x["action"], &self.services);
        let when = rules::when_summary(rules::effective_when(x).as_ref());
        let conds: Vec<String> = x["conditions"].as_array().map(|a| a.iter().filter(|c| ss(c, "type") != "time").map(rules::cond_summary).collect()).unwrap_or_default();
        let last = last_run(&ss(x, "lastRun"));
        let me = cx.entity().downgrade();
        let (id1, id2, id3, id4) = (id.clone(), id.clone(), id.clone(), id.clone());
        let power = w::tip(div().id(("sched-pow-tip", i)).pt(T::SP_2).flex_none()
            .child(w::switch(("sched-pow", i), on, move |_, window, cx| { let id = id1.clone(); let _ = me.update(cx, |s, cx| s.act("enable", &id, window, cx)); })),
            if on { "Enabled — click to disable" } else { "Disabled — click to enable" });
        let mut chips = div().flex().flex_wrap().min_w_0().gap(T::SP_4).pt(T::SP_6).child(w::tag(when, "").max_w_full().truncate());
        for c in conds { chips = chips.child(w::tag(c, "").max_w_full().truncate()); }
        let body = div().id(("sched-body", i)).flex().flex_col().flex_1().min_w_0().gap(T::SP_2).cursor_pointer()
            .on_click(cx.listener(move |s, _, window, cx| s.act("edit", &id2, window, cx)))
            .child(div().flex().items_center().gap(T::SP_6).min_w_0()
                .child(div().min_w_0().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(ss(x, "name")))
                .when(show_proj, |d| d.child(w::badge(ss(x, "project")))))
            .when(!summary.is_empty(), |d| d.child(div().min_w_0().text_size(T::TEXT_13).text_color(T::text_secondary()).child(summary)))
            .child(chips)
            .when(!last.is_empty(), |d| d.child(div().pt(T::SP_4).text_size(T::TEXT_10).text_color(T::text_muted()).child(last)));
        div().flex().items_start().gap(T::SP_10).px(T::SP_12).py(T::SP_10).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg_card())
            .when(!on, |d| d.opacity(T::SCHED_O_OFF))
            .child(power)
            .child(body)
            .child(div().flex().flex_none().gap(T::SP_2)
                .child(w::icon_button(("sched-edit", i), IconName::Pencil, "Edit").on_click(cx.listener(move |s, _, window, cx| s.act("edit", &id3, window, cx))))
                .child(w::icon_button(("sched-del", i), IconName::Trash, "Delete").on_click(cx.listener(move |s, _, window, cx| s.act("delete", &id4, window, cx)))))
            .into_any_element()
    }

    fn model_effort(&self, m: &Dm, cx: &mut Context<Self>) -> Div {
        let mo: Vec<SharedString> = std::iter::once("Default").chain(rules::MODELS).map(SharedString::from).collect();
        let ef: Vec<SharedString> = std::iter::once("Default").chain(rules::EFFORTS).map(SharedString::from).collect();
        div().flex().gap(T::SP_10)
            .child(div().flex().flex_col().flex_1().child(label("MODEL", "", false))
                .child(w::dropdown_of("sched-model", mo, m.model, false, true, Self::weak_pick(cx, |s, i, _, cx| s.pick("model", i, cx)))))
            .child(div().flex().flex_col().flex_1().child(label("EFFORT", "", false))
                .child(w::dropdown_of("sched-effort", ef, m.effort, false, true, Self::weak_pick(cx, |s, i, _, cx| s.pick("effort", i, cx)))))
    }

    fn mode_seg(&self, mode: &str, cx: &mut Context<Self>) -> Div {
        div().flex().flex_col().child(label("MODE", "", false))
            .child(self.seg_ctl("sched-mode", "mode", &["Normal", "Skip permissions"], &["normal", "skip"], if mode == "skip" { 1 } else { 0 }, cx))
    }

    /// An extra condition (.sm-cond): its label, its fields, × to remove.
    fn cond_row(&self, i: usize, r: &Row, m: &Dm, cx: &mut Context<Self>) -> AnyElement {
        let mut d = div().flex().flex_wrap().items_center().gap(T::SP_6).px(T::SP_8).py(T::SP_6).rounded(T::R_CONTROL)
            .border_1().border_color(T::border_subtle()).bg(T::bg_tertiary())
            .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(r.title.clone()));
        if r.kind != "kanban" {
            let ops: Vec<SharedString> = rules::OPS.iter().map(|o| SharedString::from(*o)).collect();
            d = d.child(div().w(T::SP_48 * 2.).child(w::dropdown_of(("sched-cop", i), ops, r.op, false, true, Self::weak_pick(cx, move |s, k, _, cx| s.cond_pick(i, "op", k, cx)))));
            if let Some(e) = self.cond_vals.get(i) { d = d.child(div().w(T::SP_32 * 2.).child(w::input(e))); }
            if r.kind == "usage" { d = d.child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child("%")); }
            d = d.child(div().flex_1());
        } else {
            d = d.child(div().flex_1().min_w_0().child(w::dropdown_of(("sched-ctask", i), opts(&m.task_opts), r.task, false, true, Self::weak_pick(cx, move |s, k, _, cx| s.cond_pick(i, "task", k, cx)))))
                .child(div().flex_1().min_w_0().child(w::dropdown_of(("sched-ctodo", i), opts(&r.todo_opts), r.todo, false, true, Self::weak_pick(cx, move |s, k, _, cx| s.cond_pick(i, "todo", k, cx)))))
                .child(div().flex_1().min_w_0().child(w::dropdown_of(("sched-cst", i), opts(&r.status_opts), r.status, false, true, Self::weak_pick(cx, move |s, k, _, cx| s.cond_pick(i, "status", k, cx)))));
        }
        d.child(w::icon_button(("sched-cx", i), IconName::X, "Remove").on_click(cx.listener(move |s, _, window, cx| s.remove_cond(i, window, cx))))
            .into_any_element()
    }

    /// The add/edit dialog (#sched-modal): its head, the body, Cancel and Save.
    fn render_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let m = self.model(cx);
        let Some(d) = self.draft.clone() else { return div().into_any_element() };
        let f = &self.f;
        // The folder's placeholder: the project's path.
        let hint = m.cwd_hint.clone();
        if self.ph.0 != hint { self.ph.0 = hint.clone(); f.cwd.update(cx, |st, cx| st.set_placeholder(hint, window, cx)); }
        let wm = d["when"].clone();
        let a = d["action"].clone();
        let mode = ss(&wm, "mode");
        let hook = mode == "webhook";
        let kind = ss(&a, "kind");
        let session = ss(&a, "session");
        let editing = !ss(&d, "id").is_empty();
        let f = &self.f;
        let mut body = div().flex().flex_col().px(T::SP_16).py(T::SP_12)
            .child(label("NAME", "", true)).child(w::input(&f.name))
            .child(label("PROJECT", "", false))
            .child(w::dropdown_of("sched-project", opts(&m.popts), m.pi, false, true, Self::weak_pick(cx, |s, i, _, cx| s.pick("project", i, cx))))
            .child(label("WHEN", "", false))
            .child(self.seg_ctl("sched-when", "whenmode", &["One-time", "Recurring", "When called"], &["once", "recurring", "webhook"],
                if mode == "once" { 0 } else if hook { 2 } else { 1 }, cx));
        if hook {
            body = body.child(webhook::section(self, &d, &m, window, cx));
        } else {
            let mut row = div().flex().flex_wrap().items_center().gap(T::SP_10).pt(T::SP_6);
            if mode == "once" {
                row = row.child(div().w(T::SP_48 * 2. + T::SP_32).child(w::input(&self.f.date)));
            } else {
                let mut days = div().flex().gap(T::SP_4);
                for (i, n) in rules::DAYS.iter().enumerate() {
                    days = days.child(w::toggle(("sched-day", i), *n, m.days[i]).on_click(cx.listener(move |s, _, _, cx| s.day(i as i64, cx))));
                }
                row = row.child(days);
            }
            row = row.child(div().w(T::SP_48 + T::SP_40).child(w::input(&self.f.time)));
            body = body.child(row);
        }
        body = body.child(label("ACTION", "", false))
            .child(self.seg_ctl("sched-kind", "kind", &["Agent", "Background chat", "CLI command", "Service"], &["agent", "chat", "command", "service"],
                match kind.as_str() { "chat" => 1, "command" => 2, "service" => 3, _ => 0 }, cx));
        let f = &self.f;
        match kind.as_str() {
            "agent" => {
                body = body.child(label("AGENT", "", false))
                    .child(self.seg_ctl("sched-agent", "agent", &["Claude"], &["claude"], 0, cx))
                    .child(self.model_effort(&m, cx))
                    .child(self.mode_seg(&ss(&a, "mode"), cx))
                    .child(label("SESSION", "", false))
                    .child(self.seg_ctl("sched-session", "session", &["New", "Continue last", "Resume id"], &["new", "continue", "resume"],
                        match session.as_str() { "continue" => 1, "resume" => 2, _ => 0 }, cx))
                    .when(session == "resume", |b| b.child(div().pt(T::SP_6).child(w::input(&f.session_id))))
                    .child(label("PROMPT", "", false))
                    .child(w::textarea(&f.prompt))
                    .child(note("Tip: you can include slash commands like /frontend-design or /loop right in the prompt — they run just like typing them in the terminal."));
            }
            "chat" => {
                let sid = ss(&a, "sessionId");
                body = body.child(note("Opens a chat, runs the prompt, and closes itself when the work finishes. It stays open if it needs you or hits an error, and it will not close while it is waiting on a background command."))
                    .child(self.model_effort(&m, cx))
                    .child(self.mode_seg(&ss(&a, "mode"), cx))
                    .child(label("FOLDER", "(blank = the project root)", false)).child(w::input(&self.f.cwd))
                    .child(label("CONVERSATION", "", false))
                    .child(self.seg_ctl("sched-conv", "session", &["Fresh each run", "One ongoing chat"], &["new", "resume"], if session == "resume" { 1 } else { 0 }, cx));
                let busy = self.provisioning;
                if session == "resume" && !sid.is_empty() {
                    body = body.child(div().flex().items_center().gap(T::SP_6).pt(T::SP_6)
                        .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Resuming"))
                        .child(w::tag(sid.chars().take(8).collect::<String>(), "accent"))
                        .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("…"))
                        .child(w::disabled(w::button("sched-prov", if busy { "Creating…" } else { "Start a new one" }), busy)
                            .on_click(cx.listener(|s, _, window, cx| s.provision(window, cx)))));
                } else if session == "resume" && editing {
                    body = body.child(div().flex().items_center().gap(T::SP_6).pt(T::SP_6)
                        .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("No conversation yet."))
                        .child(w::disabled(w::primary("sched-prov", if busy { "Creating…" } else { "Create it now" }), busy)
                            .on_click(cx.listener(|s, _, window, cx| s.provision(window, cx))))
                        .child(div().flex_1().min_w_0().text_size(T::TEXT_12).text_color(T::text_muted()).child("— this opens a chat, says hello, and keeps its id.")));
                } else if session == "resume" {
                    body = body.child(note("Save the schedule first, then create the conversation."));
                }
                let f = &self.f;
                body = body.child(label("PROMPT", "", false)).child(w::textarea(&f.prompt))
                    .when(hook, |b| b.child(note("Read the call with {{payload.some.field}} (arrays by index: {{payload.alerts.0.labels.alertname}}), {{headers.name}}, {{date}}. Anything a call does not have comes out empty and never stops it, and the chat always gets the full request as well.")))
                    .child(note("The job can rewrite this prompt for its next run with the schedule_update tool, so it can leave notes for itself."));
            }
            "command" => { body = body.child(label("COMMAND", "", false)).child(w::input(&f.command)); }
            "service" => {
                let op = { let o = ss(&a, "op"); if o.is_empty() { "start".to_string() } else { o } };
                body = body.child(label("SERVICE", "", false))
                    .child(w::dropdown_of("sched-service", opts(&m.svcs), m.si, !m.has_services, true, Self::weak_pick(cx, |s, i, _, cx| s.pick("service", i, cx))))
                    .child(label("OPERATION", "", false))
                    .child(self.seg_ctl("sched-op", "op", &["Start if stopped", "Stop if running", "Toggle"], &["start", "stop", "toggle"],
                        match op.as_str() { "stop" => 1, "toggle" => 2, _ => 0 }, cx));
            }
            _ => {}
        }
        // The prompt's placeholder follows the action (the web's).
        let ph = if kind == "chat" { if hook { "Look into this alert: {{payload.message}}" } else { "What should it do each time?" } } else { "What should the agent do?" };
        if self.ph.1 != ph { self.ph.1 = ph; self.f.prompt.update(cx, |st, cx| st.set_placeholder(ph, window, cx)); }

        body = body.child(label("EXTRA CONDITIONS", "(optional — all must be true)", false));
        let mut conds = div().flex().flex_col().gap(T::SP_6);
        for (i, r) in m.rows.iter().enumerate() { conds = conds.child(self.cond_row(i, r, &m, cx)); }
        body = body.child(conds).child(div().flex().flex_wrap().gap(T::SP_6).pt(if m.rows.is_empty() { T::SP_4 } else { T::SP_6 })
            .child(w::button("sched-c-usage", "+ 5h usage").on_click(cx.listener(|s, _, window, cx| s.add_cond("usage", window, cx))))
            .child(w::button("sched-c-week", "+ Weekly usage").on_click(cx.listener(|s, _, window, cx| s.add_cond("usage-week", window, cx))))
            .child(w::button("sched-c-term", "+ Open terminals").on_click(cx.listener(|s, _, window, cx| s.add_cond("terminals", window, cx))))
            .child(w::button("sched-c-kanban", "+ Kanban status").on_click(cx.listener(|s, _, window, cx| s.add_cond("kanban", window, cx)))));

        let foot = div().flex().justify_end().gap(T::SP_8).px(T::SP_16).py(T::SP_12).border_t_1().border_color(T::border())
            .child(w::button("sched-cancel", "Cancel").on_click(cx.listener(|s, _, _, cx| s.close(cx))))
            .child(w::primary("sched-save", "Save").on_click(cx.listener(|s, _, window, cx| s.save(window, cx))));
        let me = cx.entity().downgrade();
        let me2 = me.clone();
        let me3 = me.clone();
        let head = w::modal_head("sched-x", if editing { "Edit schedule" } else { "New schedule" }, false, move |_, _, cx| { let _ = me3.update(cx, |s, cx| s.close(cx)); });
        let modal = w::Modal {
            id: "sched-modal", width: T::DIALOG_W_XL, focus: self.focus.clone(), scroll: self.scroll.clone(),
            on_close: Rc::new(move |_, cx| { let _ = me.update(cx, |s, cx| s.close(cx)); }),
            // Esc anywhere in the dialog closes it.
            on_key: Rc::new(move |e: &KeyDownEvent, _, cx| if e.keystroke.key == "escape" { let _ = me2.update(cx, |s, cx| s.close(cx)); }),
        };
        w::overlay_modal(modal, window, vec![head.into_any_element(), w::rule().into_any_element(), body.into_any_element(), foot.into_any_element()]).into_any_element()
    }
}

impl Render for Sched {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main = match self.state {
            "loading" => w::loading("Loading…").into_any_element(),
            "error" => w::error_state("sched-retry", self.error.clone(), cx.listener(|s, _, _, cx| s.load(cx))).into_any_element(),
            _ => self.render_list(cx),
        };
        let dialog = if self.is_open() { Some(self.render_dialog(window, cx)) } else { None };
        div().flex().flex_col().size_full().bg(T::bg())
            .child(main)
            .children(dialog)
            // The window's confirm layer (until the shell draws it once at its root).
            .child(w::ask_layer(cx))
    }
}

// ─── The part's hooks (lib.rs) ───

/// At start: no workbench pane kind (Schedules has no panel).
pub fn init(_cx: &mut App) {}

/// The tab's page.
pub fn view(_tab: &str, _key: &str, window: &mut Window, cx: &mut App) -> AnyElement {
    AnyView::from(ensure(window, cx)).into_any_element()
}

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    let Some(h) = get(cx) else { return };
    match e {
        // Any change to a schedule (here, another client, a tool) reloads the list.
        AreaEvent::Server("schedule", _) | AreaEvent::Connected(true) => h.update(cx, |s, cx| if s.shown { s.load(cx) }),
        _ => {}
    }
}

pub fn on_project(_alias: &str, cx: &mut App) {
    if let Some(h) = get(cx) { h.update(cx, |s, cx| { if s.shown { s.load(cx); } cx.notify(); }); }
}

pub fn on_signed_in(cx: &mut App) {
    if let Some(h) = get(cx) { h.update(cx, |s, cx| if s.shown { s.load(cx) }); }
}

pub fn on_signed_out(cx: &mut App) {
    if let Some(h) = get(cx) {
        h.update(cx, |s, cx| { s.close(cx); s.schedules.clear(); s.services.clear(); s.state = "loading"; cx.notify(); });
    }
}

pub fn on_tab(tab: &str, cx: &mut App) {
    let on = tab == "schedule";
    cx.set_global(Shown(on));
    if let Some(h) = get(cx) { h.update(cx, |s, cx| { s.shown = on; if on { s.load(cx); } }); }
}

/// The driver's "planning:schedule-*" actions (the Slint client's names).
pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    if !name.starts_with("schedule-") { return None; }
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let n = |i: usize| a(i).parse::<usize>().unwrap_or(0);
    // The confirm's answer (until the driver's dialog-answer answers the ask layer).
    if name == "schedule-ask-answer" { return Some(json!({ "ok": w::ask_answer(a(0) == "true", None, window, cx) })); }
    let h = ensure(window, cx);
    let known = h.update(cx, |s, cx| {
        match name {
            "schedule-act" => s.act(&a(0), &a(1), window, cx),
            "schedule-seg" => s.seg(&a(0), &a(1), cx),
            "schedule-day" => s.day(n(0) as i64, cx),
            "schedule-pick" => s.pick(&a(0), n(1), cx),
            "schedule-cond-pick" => s.cond_pick(n(0), &a(1), n(2), cx),
            "schedule-cond-value" => { if let Some(e) = s.cond_vals.get(n(0)).cloned() { Sched::set(&e, &a(1), window, cx); } }
            "schedule-add-cond" => s.add_cond(&a(0), window, cx),
            "schedule-remove-cond" => s.remove_cond(n(0), window, cx),
            // Type into a field of the dialog: name, date, time, prompt, command, cwd, session-id, sig-header, ts-header, idem.
            "schedule-type" => {
                let v = a(1);
                let f = &s.f;
                let e = match a(0).as_str() {
                    "name" => Some(&f.name), "date" => Some(&f.date), "time" => Some(&f.time), "command" => Some(&f.command),
                    "cwd" => Some(&f.cwd), "session-id" => Some(&f.session_id), "sig-header" => Some(&f.sig), "ts-header" => Some(&f.ts),
                    "idem" => Some(&f.idem), _ => None,
                }.cloned();
                if let Some(e) = e { Sched::set(&e, &v, window, cx); }
                if a(0) == "prompt" { s.f.prompt.clone().update(cx, |st, cx| st.set_value(v, window, cx)); }
            }
            "schedule-save" => s.save(window, cx),
            "schedule-close" => s.close(cx),
            "schedule-provision" => s.provision(window, cx),
            "schedule-rotate" => webhook::rotate(s, window, cx),
            "schedule-reload" => s.load(cx),
            _ => return false,
        }
        cx.notify();
        true
    });
    known.then(|| json!({ "ok": true }))
}

/// props.planningArea.schedule (the Slint client's names).
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    let Some(h) = get(cx) else { return json!({ "schedule": null }) };
    let ask = w::ask_props(cx);
    let has_project = !crundi_shell::project(cx).is_empty();
    h.update(cx, |s, cx| {
        let dialog = if s.is_open() {
            let m = s.model(cx);
            let d = s.draft.clone().unwrap_or(Value::Null);
            let f = &s.f;
            let v = |e: &Entity<w::InputState>| Sched::val(e, cx);
            let a = &d["action"];
            let wh = &d["webhook"];
            let mut o = serde_json::Map::new();
            let mut put = |k: &str, x: Value| { o.insert(k.to_string(), x); };
            put("title", json!(if ss(&d, "id").is_empty() { "New schedule" } else { "Edit schedule" }));
            put("name", json!(v(&f.name))); put("projects", json!(m.popts)); put("project", json!(m.pi));
            put("when", json!(ss(&d["when"], "mode"))); put("date", json!(v(&f.date))); put("time", json!(v(&f.time))); put("days", json!(m.days));
            put("hookAuth", json!(if ss(wh, "auth") == "signature" { "signature" } else { "key" }));
            put("sigHeader", json!(v(&f.sig))); put("tsHeader", json!(v(&f.ts))); put("idem", json!(v(&f.idem)));
            put("hookUrl", json!(m.url)); put("hookSecret", json!(m.secret));
            put("kind", json!(ss(a, "kind"))); put("agent", json!((|| { let x = ss(a, "agent"); if x.is_empty() { "claude".to_string() } else { x } })()));
            put("model", json!(m.model)); put("effort", json!(m.effort)); put("mode", json!(ss(a, "mode")));
            put("session", json!(ss(a, "session"))); put("sessionId", json!(v(&f.session_id))); put("provisioning", json!(s.provisioning));
            put("prompt", json!(f.prompt.read(cx).value().to_string())); put("cwd", json!(v(&f.cwd))); put("cwdHint", json!(m.cwd_hint)); put("command", json!(v(&f.command)));
            put("services", json!(m.svcs)); put("hasServices", json!(m.has_services)); put("service", json!(m.si));
            put("op", json!((|| { let o = ss(a, "op"); if o.is_empty() { "start".to_string() } else { o } })()));
            put("tasks", json!(m.task_opts));
            let conds: Vec<Value> = m.rows.iter().enumerate().map(|(i, r)| {
                let mut c = serde_json::Map::new();
                c.insert("kind".into(), json!(r.kind)); c.insert("title".into(), json!(r.title)); c.insert("op".into(), json!(r.op));
                c.insert("value".into(), json!(s.cond_vals.get(i).map(|e| Sched::val(e, cx)).unwrap_or_default()));
                c.insert("task".into(), json!(r.task)); c.insert("todo".into(), json!(r.todo)); c.insert("todos".into(), json!(r.todo_opts));
                c.insert("status".into(), json!(r.status)); c.insert("statuses".into(), json!(r.status_opts));
                Value::Object(c)
            }).collect();
            put("conds", Value::Array(conds));
            put("draft", d.clone());
            Value::Object(o)
        } else { Value::Null };
        let items: Vec<Value> = s.schedules.iter().map(|x| json!({
            "id": ss(x, "id"), "name": ss(x, "name"), "project": ss(x, "project"), "enabled": truthy(&x["enabled"]),
            "summary": rules::action_summary(&x["action"], &s.services),
            "when": rules::when_summary(rules::effective_when(x).as_ref()),
            "conds": x["conditions"].as_array().map(|a| a.iter().filter(|c| ss(c, "type") != "time").map(rules::cond_summary).collect::<Vec<_>>()).unwrap_or_default(),
            "last": last_run(&ss(x, "lastRun")),
        })).collect();
        let filtered = !s.all && has_project;
        json!({ "schedule": {
            "state": s.state, "error": s.error, "hasProject": has_project, "filter": !s.all,
            "empty": if filtered { "No scheduled tasks for this project" } else { "No scheduled tasks" },
            "items": items, "dialog": dialog,
            // The dialog's scrim takes the pointer (the Slint client's top-level pointerModal).
            "pointerModal": s.is_open(),
            // The open confirm (the Slint client's top-level "dialog"), until the driver reports the ask layer.
            "ask": ask,
        }})
    })
}
