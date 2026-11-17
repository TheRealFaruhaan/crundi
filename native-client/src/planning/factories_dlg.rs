//! Factories: the dialogs (crundi-factories.js): Start a factory (the change
//! request, its draft kept per factory, attachments uploaded at Start, the
//! project's precheck, missing skills and Start anyway), the PIN prompt for
//! the factory's secrets, a trigger (kanban, schedule, webhook with its
//! security, sample preview, URL, key or secret, history), and a person's
//! transcript.

use super::model::{self, s, n, b, arr};
use super::{g, get, post, ss, err_text};
use crate::{kit, PlanningFactories, PlanningFxSec, PlanningFxFile, PlanningFxLine};
use crate::planning::prefs;
use serde_json::{json, Value};
use slint::{SharedString, VecModel};
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

pub struct Models {
    st_projects: Rc<VecModel<SharedString>>,
    st_factories: Rc<VecModel<SharedString>>,
    st_sections: Rc<VecModel<PlanningFxSec>>,
    st_files: Rc<VecModel<PlanningFxFile>>,
    st_info: Rc<VecModel<PlanningFxLine>>,
    tr_factories: Rc<VecModel<SharedString>>,
    tr_days: Rc<VecModel<bool>>,
    tr_sections: Rc<VecModel<PlanningFxSec>>,
    tr_preview: Rc<VecModel<PlanningFxLine>>,
    tr_history: Rc<VecModel<SharedString>>,
}
impl Models {
    pub fn new(f: &PlanningFactories) -> Models {
        let m = Models { st_projects: kit::model(), st_factories: kit::model(), st_sections: kit::model(), st_files: kit::model(), st_info: kit::model(), tr_factories: kit::model(), tr_days: kit::model(), tr_sections: kit::model(), tr_preview: kit::model(), tr_history: kit::model() };
        f.set_st_projects(kit::rc(&m.st_projects)); f.set_st_factories(kit::rc(&m.st_factories)); f.set_st_sections(kit::rc(&m.st_sections)); f.set_st_files(kit::rc(&m.st_files)); f.set_st_info(kit::rc(&m.st_info));
        f.set_tr_factories(kit::rc(&m.tr_factories)); f.set_tr_days(kit::rc(&m.tr_days)); f.set_tr_sections(kit::rc(&m.tr_sections)); f.set_tr_preview(kit::rc(&m.tr_preview)); f.set_tr_history(kit::rc(&m.tr_history));
        m
    }
}

#[derive(Default, Clone, Copy, PartialEq)]
pub enum Then { #[default] None, Viewer, Panel }
pub struct StartOpts { pub project: String, pub factory: String, pub then: Then }

#[derive(Default)]
struct St {
    open: bool,
    then: Then,
    projects: Vec<(String, String)>,
    factories: Vec<Value>,
    project_i: usize,
    factory_i: usize,
    merge_i: usize,
    title: String,
    sections: HashMap<String, String>,
    template: Vec<Value>,
    files: Vec<PathBuf>,
    pre: Option<Value>,
    info: Vec<(String, String, String)>,
    go_label: String,
    go_enabled: bool,
    title_bad: bool,
    bad: String,
    held: Option<Value>,
    to_unlock: Vec<Value>,
    gen: u64,
    save_gen: u64,
}

#[derive(Default)]
struct Pin { list: Vec<Value>, i: usize, pins: serde_json::Map<String, Value>, value: String, err: String }

#[derive(Default)]
struct Tr {
    open: bool,
    project: String,
    saved: Option<Value>,
    factories: Vec<Value>,
    cur: Value,
    sections: HashMap<String, String>,
    gen: u64,
}

#[derive(Default)]
struct D { st: St, pin: Pin, tr: Tr, modal: bool }

thread_local! {
    static DS: RefCell<D> = RefCell::new(D::default());
    static PIN_THEN: RefCell<Option<Box<dyn FnOnce(Option<Value>)>>> = const { RefCell::new(None) };
}
fn with<R>(f: impl FnOnce(&mut D) -> R) -> R { DS.with(|s| f(&mut s.borrow_mut())) }

const MAX_ATTACH: usize = 10;
const MAX_BYTES: u64 = 25 * 1024 * 1024;
const MERGE: [&str; 3] = ["", "ask", "auto"];
const BUSY: [&str; 3] = ["manager", "queue", "skip"];
const KINDS: [&str; 3] = ["kanban", "schedule", "webhook"];

/// Shell.modal-planning while one of these is open (and only then, from here).
pub fn sync_modal() {
    let open = with(|d| d.st.open || d.tr.open || !d.pin.list.is_empty()) || kit::ui().map(|w| { use slint::ComponentHandle; w.global::<PlanningFactories>().get_chat_open() }).unwrap_or(false);
    let was = with(|d| { let w = d.modal; d.modal = open; w });
    if open != was { if let Some(w) = kit::ui() { use slint::ComponentHandle; w.global::<crate::Shell>().set_modal_planning(open); } }
}

pub fn close_all() {
    with(|d| { d.st = St::default(); d.tr = Tr::default(); d.pin = Pin::default(); });
    PIN_THEN.with(|p| p.borrow_mut().take());
    g(|f| { let mut a = f.get_start(); a.open = false; f.set_start(a); let mut p = f.get_pin(); p.open = false; f.set_pin(p); let mut t = f.get_trig(); t.open = false; f.set_trig(t); f.set_chat_open(false); });
    sync_modal();
}

// ─── Start a factory ───

fn draft_key(fid: &str) -> String { format!("crundi.factoryCr.{fid}") }
fn read_draft(fid: &str) -> Option<Value> { serde_json::from_str(&prefs::get(&draft_key(fid))).ok().filter(|v: &Value| v.is_object()) }
fn save_draft(fid: &str, v: &Value) { if !fid.is_empty() { prefs::set(&draft_key(fid), &v.to_string()); } }
fn drop_draft(fid: &str) { prefs::set(&draft_key(fid), ""); }

pub fn open_start(o: StartOpts) {
    let projects: Vec<(String, String)> = kit::projects().iter().map(|p| (s(p, "alias"), { let nm = s(p, "name"); if nm.is_empty() { s(p, "alias") } else { nm } })).collect();
    let project = if !o.project.is_empty() { o.project.clone() } else { let c = kit::project(); if !c.is_empty() { c } else { projects.first().map(|p| p.0.clone()).unwrap_or_default() } };
    let pi = projects.iter().position(|p| p.0.eq_ignore_ascii_case(&project)).unwrap_or(0);
    let gen = with(|d| { let g2 = d.st.gen + 1; d.st = St { open: true, then: o.then, projects: projects.clone(), project_i: pi, go_label: "Start".into(), gen: g2, ..Default::default() }; g2 });
    g(|f| { let mut a = f.get_start(); a.open = true; a.loading = true; a.empty = false; a.title = "".into(); a.title_bad = false; a.gen += 1; f.set_start(a); });
    super::models(|m| { m.d.st_projects.set_vec(projects.iter().map(|p| ss(&p.1)).collect::<Vec<_>>()); m.d.st_info.set_vec(vec![]); m.d.st_files.set_vec(vec![]); });
    sync_modal();
    let want = o.factory.clone();
    get("/api/factories".into(), move |lib| {
        if with(|d| d.st.gen != gen || !d.st.open) { return; }
        let list = arr(&lib, "factories").to_vec();
        if list.is_empty() { g(|f| { let mut a = f.get_start(); a.loading = false; a.empty = true; f.set_start(a); }); return; }
        let fi = list.iter().position(|f| s(f, "id") == want && !want.is_empty()).or_else(|| list.iter().position(|f| b(f, "runnable"))).unwrap_or(0);
        with(|d| { d.st.factories = list.clone(); d.st.factory_i = fi; });
        let labels: Vec<SharedString> = list.iter().map(|f| ss(format!("{}{}", s(f, "name"), if b(f, "runnable") { String::new() } else { format!(" (draft: {} to fix)", n(f, "errorCount") as i64) }))).collect();
        super::models(|m| m.d.st_factories.set_vec(labels));
        render_sections(None);
        g(|f| { let mut a = f.get_start(); a.loading = false; a.empty = false; f.set_start(a); });
        push_start();
        precheck();
    });
}

fn cur_factory() -> Option<Value> { with(|d| d.st.factories.get(d.st.factory_i).cloned()) }
fn fid() -> String { cur_factory().map(|f| s(&f, "id")).unwrap_or_default() }
fn values() -> Value { with(|d| json!({ "title": d.st.title, "sections": d.st.sections })) }

/// renderSections(): the factory's CR template; values from `keep`, else its draft.
fn render_sections(keep: Option<Value>) {
    let f = cur_factory().unwrap_or(Value::Null);
    let template = arr(&f, "crTemplate").to_vec();
    let draft = keep.clone().or_else(|| read_draft(&s(&f, "id"))).unwrap_or(json!({ "title": "", "sections": {} }));
    with(|d| {
        if keep.is_none() { let t = s(&draft, "title"); if !t.is_empty() { d.st.title = t; } }
        let old = std::mem::take(&mut d.st.sections);
        for sec in &template {
            let id = s(sec, "id");
            let v = draft.get("sections").and_then(|x| x.get(&id)).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(String::from).or_else(|| old.get(&id).cloned()).unwrap_or_default();
            d.st.sections.insert(id, v);
        }
        d.st.template = template.clone();
    });
    let rows: Vec<PlanningFxSec> = with(|d| d.st.template.iter().map(|sec| {
        let list = b(sec, "list");
        let hint = s(sec, "hint");
        let per_line = hint.to_lowercase().contains("per line");
        PlanningFxSec { id: ss(s(sec, "id")), label: ss(s(sec, "label")), hint: ss(format!("{hint}{}", if list && !per_line { "\nOne per line." } else { "" })), required: b(sec, "required"),
            rows: if list { 3 } else { 2 }, value: ss(d.st.sections.get(&s(sec, "id")).cloned().unwrap_or_default()), bad: d.st.bad == s(sec, "id") }
    }).collect());
    super::models(|m| m.d.st_sections.set_vec(rows));
    let title = with(|d| d.st.title.clone());
    g(|f| { let mut a = f.get_start(); a.title = ss(&title); a.gen += 1; f.set_start(a); });
}

fn push_start() {
    let (pi, fi, mi, title_bad, label, enabled, info, files) = with(|d| (d.st.project_i, d.st.factory_i, d.st.merge_i, d.st.title_bad, d.st.go_label.clone(), d.st.go_enabled, d.st.info.clone(), d.st.files.clone()));
    g(|f| { let mut a = f.get_start(); a.project_i = pi as i32; a.factory_i = fi as i32; a.merge_i = mi as i32; a.title_bad = title_bad; a.go_label = ss(label); a.go_enabled = enabled; f.set_start(a); });
    super::models(|m| {
        kit::sync_rows(&m.d.st_info, info.into_iter().map(|(k, t, id)| PlanningFxLine { kind: ss(k), text: ss(t), id: ss(id) }).collect());
        kit::sync_rows(&m.d.st_files, files.iter().map(|p| { let nm = p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(); let low = nm.to_lowercase(); PlanningFxFile { image: [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg"].iter().any(|e| low.ends_with(e)), name: ss(nm) } }).collect());
    });
}

fn precheck() {
    let project = with(|d| { d.st.info.clear(); d.st.go_enabled = false; d.st.projects.get(d.st.project_i).map(|p| p.0.clone()).unwrap_or_default() });
    push_start();
    let gen = with(|d| d.st.gen);
    get(format!("/api/factory-runs/precheck?project={}", kit::enc(&project)), move |pre| {
        if with(|d| d.st.gen != gen) { return; }
        let pre = if pre.get("ok").is_none() && pre.get("active").is_none() { json!({ "ok": false, "error": "Could not check the project." }) } else { pre };
        let lines = model::precheck_lines(&pre);
        let ok = b(&pre, "ok") && !pre.get("active").map(|a| a.is_object()).unwrap_or(false);
        with(|d| { d.st.info = lines.into_iter().map(|(k, t)| if k == "open" { ("open".to_string(), String::new(), t) } else { (k, t, String::new()) }).collect(); d.st.pre = Some(pre); d.st.go_enabled = ok && !d.st.factories.is_empty(); });
        push_start();
    });
}

fn schedule_draft() {
    let gen = with(|d| { d.st.save_gen += 1; d.st.save_gen });
    slint::Timer::single_shot(std::time::Duration::from_millis(250), move || { if with(|d| d.st.save_gen == gen && d.st.open) { save_draft(&fid(), &values()); } });
}

fn field_error(id: &str, msg: &str) {
    kit::toast_error(msg);
    with(|d| { if id == "title" { d.st.title_bad = true; } else { d.st.bad = id.to_string(); } });
    if id == "title" { push_start(); g(|f| { let mut a = f.get_start(); a.gen += 1; f.set_start(a); }); } else { refresh_bad(); }
    let gen = with(|d| d.st.gen);
    slint::Timer::single_shot(std::time::Duration::from_millis(2500), move || { if with(|d| d.st.gen == gen) { with(|d| { d.st.title_bad = false; d.st.bad.clear(); }); push_start(); refresh_bad(); } });
}
fn refresh_bad() {
    let bad = with(|d| d.st.bad.clone());
    super::models(|m| { use slint::Model; for i in 0..m.d.st_sections.row_count() { if let Some(mut r) = m.d.st_sections.row_data(i) { let want = r.id == bad.as_str(); if r.bad != want { r.bad = want; m.d.st_sections.set_row_data(i, r); } } } });
}

/// start(): check, ask the secrets' PINs, upload, start.
fn start(anyway: bool) {
    let (title, template, sections) = with(|d| (d.st.title.clone(), d.st.template.clone(), d.st.sections.clone()));
    if title.trim().is_empty() { field_error("title", "Give the change request a title."); return; }
    for sec in &template {
        if b(sec, "required") && sections.get(&s(sec, "id")).map(|v| v.trim().is_empty()).unwrap_or(true) { field_error(&s(sec, "id"), &format!("Fill in \"{}\".", s(sec, "label"))); return; }
    }
    let held = with(|d| if anyway { d.st.held.clone() } else { None });
    if let Some(pins) = held { start2(anyway, Some(pins)); return; }
    let f = cur_factory().unwrap_or(Value::Null);
    let ids: Vec<String> = arr(&f, "secrets").iter().filter_map(|x| x.as_str().map(String::from)).collect();
    if ids.is_empty() { start2(anyway, None); return; }
    super::caps(move |caps| {
        if !caps.iter().any(|c| c == "factory-secrets") { start2(anyway, None); return; }
        get("/api/secrets".into(), move |d2| {
            let known = arr(&d2, "secrets").to_vec();
            let list: Vec<Value> = ids.iter().filter_map(|id| known.iter().find(|x| s(x, "id") == *id)).map(|x| json!({ "id": s(x, "id"), "name": s(x, "name"), "lock": if s(x, "lock") == "passphrase" { "passphrase" } else { "pin" } })).collect();
            if list.is_empty() { start2(anyway, None); return; }
            with(|d| d.st.to_unlock = list.clone());
            ask_pins(list, move |got| {
                let Some(pins) = got else { return };
                with(|d| d.st.held = Some(pins.clone()));
                start2(anyway, Some(pins));
            });
        });
    });
}

fn start2(anyway: bool, pins: Option<Value>) {
    let (project, fid2, files, mm, title, sections) = with(|d| (d.st.projects.get(d.st.project_i).map(|p| p.0.clone()).unwrap_or_default(), d.st.factories.get(d.st.factory_i).map(|f| s(f, "id")).unwrap_or_default(), d.st.files.clone(), MERGE[d.st.merge_i.min(2)].to_string(), d.st.title.clone(), d.st.sections.clone()));
    with(|d| { d.st.go_enabled = false; d.st.go_label = if files.is_empty() { "Starting…".into() } else { format!("Uploading 1/{}…", files.len()) }; });
    push_start();
    let gen = with(|d| d.st.gen);
    let pins2 = pins.clone();
    let project2 = project.clone();
    kit::call(move |api| {
        use base64::Engine;
        let mut atts = vec![];
        let total = files.len();
        for (i, p) in files.iter().enumerate() {
            let name = p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            let label = format!("Uploading {}/{total}…", i + 1);
            let _ = slint::invoke_from_event_loop(move || { with(|d| d.st.go_label = label); push_start(); });
            let up = std::fs::read(p).map_err(|e| format!("Could not read {name}: {e}")).and_then(|bytes| api.post("/api/attachments/upload", &json!({ "project": project2, "name": name, "data": base64::engine::general_purpose::STANDARD.encode(bytes) })).map_err(|e| e.to_string()));
            match up {
                Ok(r) if b(&r, "ok") => atts.push(json!({ "path": s(&r, "path"), "name": name })),
                Ok(r) => return Ok(json!({ "upload": name, "error": err_text(&r, "failed") })),
                Err(e) => return Ok(json!({ "upload": name, "error": e })),
            }
        }
        let _ = slint::invoke_from_event_loop(|| { with(|d| d.st.go_label = "Starting…".into()); push_start(); });
        let mut body = json!({ "action": "start", "project": project2, "factoryId": fid2, "cr": { "title": title, "sections": sections, "attachments": atts }, "ignoreMissingSkills": anyway });
        if !mm.is_empty() { body["mergeMode"] = json!(mm); }
        if let Some(p) = pins2 { body["secretPins"] = p; }
        match api.post("/api/factory-runs", &body) { Ok(r) => Ok(r), Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })) }
    }, move |r| {
        let r = r.unwrap_or_else(|e| json!({ "ok": false, "error": e }));
        if with(|d| d.st.gen != gen) { return; }
        with(|d| { d.st.go_label = "Start".into(); d.st.go_enabled = true; });
        if !s(&r, "upload").is_empty() { push_start(); kit::toast_error(&format!("Could not upload {}: {}", s(&r, "upload"), s(&r, "error"))); return; }
        if b(&r, "ok") || s(&r, "field") == "secrets" { with(|d| d.st.held = None); }
        if b(&r, "ok") {
            let (fid3, then, to_unlock) = with(|d| (d.st.factories.get(d.st.factory_i).map(|f| s(f, "id")).unwrap_or_default(), d.st.then, d.st.to_unlock.clone()));
            drop_draft(&fid3);
            close_start();
            kit::toast_ok("Factory started.");
            let run_id = s(&r["run"], "id");
            match then { Then::Viewer => super::open_sub("viewer", &run_id), Then::Panel => super::panel_load(), Then::None => {} }
            // Secrets unlocked with an old lock: offer the passphrase now.
            if let (Some(up), Some(p)) = (r.get("upgradable").and_then(|x| x.as_array()), pins.as_ref()) {
                for id in up.iter().filter_map(|x| x.as_str()) {
                    let x = to_unlock.iter().find(|t| s(t, "id") == id).cloned().unwrap_or(json!({ "id": id, "name": "" }));
                    let pin = s(p, id);
                    if !pin.is_empty() { crate::ops::vault::upgrade(id, &s(&x, "name"), &s(&x, "lock"), &pin); }
                }
            }
            return;
        }
        if s(&r, "field") == "secrets" { with(|d| d.st.info = vec![("err".into(), format!("{} Press Start to type it again.", err_text(&r, "A secret could not be unlocked.")), String::new())]); push_start(); return; }
        let field = s(&r, "field");
        if !field.is_empty() && field != "attachments" { push_start(); field_error(&field, &s(&r, "error")); return; }
        if let Some(ms) = r.get("missingSkills").and_then(|x| x.as_array()) {
            let names: Vec<String> = ms.iter().filter_map(|x| x.as_str().map(String::from)).collect();
            with(|d| d.st.info = vec![("warn".into(), model::missing_skills_text(&names), String::new()), ("anyway".into(), String::new(), String::new())]);
            push_start();
            return;
        }
        with(|d| {
            d.st.info = vec![("err".into(), err_text(&r, "It could not start."), String::new())];
            for e in arr(&r, "errors").iter().take(5) { d.st.info.push(("note".into(), format!("· {}", s(e, "message")), String::new())); }
        });
        push_start();
    });
}

fn close_start() {
    with(|d| { d.st.open = false; d.st.gen += 1; });
    g(|f| { let mut a = f.get_start(); a.open = false; f.set_start(a); });
    sync_modal();
}

fn attach() {
    kit::bg(|| crate::testctl::take_pick().map(|p| vec![p]).or_else(|| rfd::FileDialog::new().pick_files()).unwrap_or_default(), |paths| {
        for p in paths {
            if with(|d| d.st.files.len() >= MAX_ATTACH) { kit::toast_error(&format!("Up to {MAX_ATTACH} attachments.")); break; }
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            if size > MAX_BYTES { kit::toast_error(&format!("{} is over 25 MB.", p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default())); continue; }
            with(|d| d.st.files.push(p));
        }
        push_start();
    });
}

// ─── The PIN prompt ───

fn ask_pins(list: Vec<Value>, then: impl FnOnce(Option<Value>) + 'static) {
    PIN_THEN.with(|p| *p.borrow_mut() = Some(Box::new(then)));
    with(|d| d.pin = Pin { list, ..Default::default() });
    show_pin();
    sync_modal();
}

fn show_pin() {
    let (done, x, i, total) = with(|d| (d.pin.i >= d.pin.list.len(), d.pin.list.get(d.pin.i).cloned().unwrap_or(Value::Null), d.pin.i, d.pin.list.len()));
    if done {
        let pins = with(|d| { let p = std::mem::take(&mut d.pin.pins); d.pin = Pin::default(); p });
        pin_done(Some(Value::Object(pins)));
        return;
    }
    let pass = s(&x, "lock") == "passphrase";
    with(|d| { d.pin.value.clear(); d.pin.err.clear(); });
    g(|f| { let mut p = f.get_pin(); p.open = true; p.name = ss(s(&x, "name")); p.step = ss(if total > 1 { format!("Secret {} of {total}", i + 1) } else { String::new() }); p.sub = ss(model::pin_sub(pass)); p.pass = pass; p.value = "".into(); p.err = "".into(); p.busy = false; p.gen += 1; f.set_pin(p); });
}

fn pin_done(v: Option<Value>) {
    with(|d| d.pin = Pin::default());
    g(|f| { let mut p = f.get_pin(); p.open = false; f.set_pin(p); });
    sync_modal();
    let then = PIN_THEN.with(|p| p.borrow_mut().take());
    if let Some(t) = then { t(v); }
}

fn pin_unlock() {
    let (x, value) = with(|d| (d.pin.list.get(d.pin.i).cloned().unwrap_or(Value::Null), d.pin.value.clone()));
    let pass = s(&x, "lock") == "passphrase";
    let pin = if pass { value } else { value.trim().to_string() };
    let ok = if pass { !pin.is_empty() && pin.chars().count() <= 1024 } else { pin.len() == 6 && pin.chars().all(|c| c.is_ascii_digit()) };
    if !ok { let e = if pass { "Enter the passphrase" } else { "Enter a 6-digit PIN" }; g(|f| { let mut p = f.get_pin(); p.err = e.into(); f.set_pin(p); }); return; }
    g(|f| { let mut p = f.get_pin(); p.busy = true; f.set_pin(p); });
    let id = s(&x, "id");
    post("/api/secrets", json!({ "action": "check", "id": id, "pin": pin }), move |r| {
        if !b(&r, "ok") {
            let e = err_text(&r, "Failed");
            g(|f| { let mut p = f.get_pin(); p.busy = false; p.err = ss(e); p.gen += 1; f.set_pin(p); });
            return;
        }
        with(|d| { d.pin.pins.insert(id, json!(pin)); d.pin.i += 1; });
        show_pin();
    });
}

// ─── A trigger ───

pub fn open_trigger(project: &str, id: &str) {
    let saved = super::with(|s2| s2.panel.triggers.iter().find(|t| s(t, "id") == id && !id.is_empty()).cloned());
    let gen = with(|d| { let g2 = d.tr.gen + 1; d.tr = Tr { open: true, project: project.to_string(), saved: saved.clone(), gen: g2, ..Default::default() }; g2 });
    g(|f| { let mut t = f.get_trig(); t.open = true; t.loading = true; t.empty = false; t.title = ss(if saved.is_some() { "Trigger" } else { "New trigger" }); t.editing = saved.is_some(); t.hook = false; f.set_trig(t); });
    super::models(|m| { m.d.tr_preview.set_vec(vec![]); m.d.tr_history.set_vec(vec![]); });
    sync_modal();
    get("/api/factories".into(), move |lib| {
        if with(|d| d.tr.gen != gen) { return; }
        let saved = with(|d| d.tr.saved.clone());
        let list: Vec<Value> = arr(&lib, "factories").iter().filter(|f| b(f, "runnable") || saved.as_ref().map(|t| s(t, "factoryId") == s(f, "id")).unwrap_or(false)).cloned().collect();
        if list.is_empty() { g(|f| { let mut t = f.get_trig(); t.loading = false; t.empty = true; f.set_trig(t); }); return; }
        let cur = saved.clone().unwrap_or_else(|| json!({ "name": "", "enabled": true, "kind": "kanban", "factoryId": s(&list[0], "id"), "schedule": { "at": "09:00", "days": [] }, "kanban": { "status": "todo" }, "cr": { "title": "", "sections": {} }, "mergeMode": "", "ifBusy": "queue" }));
        with(|d| { d.tr.factories = list.clone(); d.tr.cur = cur; });
        super::models(|m| m.d.tr_factories.set_vec(list.iter().map(|f| ss(s(f, "name"))).collect::<Vec<_>>()));
        render_kind(true);
        render_hook(saved.as_ref());
        render_history(saved.as_ref());
        g(|f| { let mut t = f.get_trig(); t.loading = false; t.gen += 1; f.set_trig(t); });
    });
}

fn tr_get(k: &str) -> String { with(|d| s(&d.tr.cur, k)) }

/// renderKind(): the kind's fields, a new webhook tells the Manager, then the CR.
fn render_kind(first: bool) {
    let new = with(|d| d.tr.saved.is_none());
    if !first && tr_get("kind") == "webhook" && new && tr_get("ifBusy") == "queue" { with(|d| d.tr.cur["ifBusy"] = json!("manager")); }
    render_cr();
}

/// renderCr(): the change request box, keeping what was typed by section.
fn render_cr() {
    let (cur, factories, saved_none) = with(|d| (d.tr.cur.clone(), d.tr.factories.clone(), d.tr.saved.is_none()));
    let kind = s(&cur, "kind");
    let f = factories.iter().find(|x| s(x, "id") == s(&cur, "factoryId")).cloned().unwrap_or(Value::Null);
    let tpl = arr(&f, "crTemplate").to_vec();
    let mut keep = with(|d| d.tr.sections.clone());
    let first = keep.is_empty();
    let mut title = s(&cur["cr"], "title");
    if kind != "kanban" {
        for v in keep.values_mut() { if v.contains("{{card.") || v.contains("{{ card.") { v.clear(); } }
        if title.contains("{{card.") || title.contains("{{ card.") { title.clear(); }
    }
    if title.is_empty() && kind == "kanban" { title = "{{card.title}}".into(); }
    let mut sections: HashMap<String, String> = HashMap::new();
    let rows: Vec<PlanningFxSec> = tpl.iter().map(|sec| {
        let id = s(sec, "id");
        let v = if !first && keep.contains_key(&id) { keep[&id].clone() } else {
            let saved_v = s(&cur["cr"]["sections"], &id);
            if !saved_v.is_empty() { saved_v } else if kind == "kanban" && saved_none { match id.as_str() { "goal" => "{{card.description}}".into(), "done" => "{{card.todos}}".into(), _ => String::new() } } else { String::new() }
        };
        sections.insert(id.clone(), v.clone());
        PlanningFxSec { id: ss(&id), label: ss(s(sec, "label")), hint: ss(s(sec, "hint")), required: false, rows: 2, value: ss(v), bad: false }
    }).collect();
    with(|d| { d.tr.sections = sections; d.tr.cur["cr"]["title"] = json!(title); });
    super::models(|m| m.d.tr_sections.set_vec(rows));
    push_trig();
}

fn push_trig() {
    let (cur, factories) = with(|d| (d.tr.cur.clone(), d.tr.factories.clone()));
    let kind = s(&cur, "kind");
    let days: Vec<i64> = arr(&cur["schedule"], "days").iter().filter_map(|x| x.as_i64()).collect();
    let wh = &cur["webhook"];
    g(|f| {
        let mut t = f.get_trig();
        t.name = ss(s(&cur, "name"));
        t.factory_i = factories.iter().position(|x| s(x, "id") == s(&cur, "factoryId")).unwrap_or(0) as i32;
        t.kind_i = KINDS.iter().position(|k| *k == kind).unwrap_or(0) as i32;
        t.at = ss({ let a = s(&cur["schedule"], "at"); if a.is_empty() { "09:00".into() } else { a } });
        t.column_i = model::COLUMNS.iter().position(|c| c.0 == s(&cur["kanban"], "status")).unwrap_or(1) as i32;
        t.cr_head = ss(model::trigger_cr_head(&kind));
        t.cr_title = ss(s(&cur["cr"], "title"));
        t.merge_i = MERGE.iter().position(|m| *m == s(&cur, "mergeMode")).unwrap_or(0) as i32;
        t.busy_i = BUSY.iter().position(|m| *m == s(&cur, "ifBusy")).unwrap_or(1) as i32;
        t.needs_i = model::COLUMNS.iter().position(|c| c.0 == s(&cur, "needsCards")).map(|i| i + 1).unwrap_or(0) as i32;
        t.enabled = cur.get("enabled").map(|x| x != &json!(false)).unwrap_or(true);
        t.auth_i = (s(wh, "auth") == "signature") as i32;
        t.sig_h = ss({ let x = s(wh, "signatureHeader"); if x.is_empty() { "x-crundi-signature".into() } else { x } });
        t.ts_h = ss(s(wh, "timestampHeader"));
        t.idem = ss(s(wh, "idempotency"));
        t.sample = ss(s(&cur, "_sample"));
        f.set_trig(t);
    });
    super::models(|m| kit::sync_rows(&m.d.tr_days, (0..7).map(|i| days.contains(&i)).collect()));
}

fn render_hook(tr: Option<&Value>) {
    let Some(tr) = tr.filter(|t| s(t, "kind") == "webhook") else { g(|f| { let mut t = f.get_trig(); t.hook = false; f.set_trig(t); }); return };
    let wh = &tr["webhook"];
    let signed = s(wh, "auth") == "signature";
    let u = format!("{}/api/factory-hook/trigger/{}{}", kit::base(), s(tr, "id"), if signed { String::new() } else { format!("?key={}", s(wh, "key")) });
    let ts = s(wh, "timestampHeader");
    let sh = s(wh, "signatureHeader");
    let secret = s(wh, "secret");
    let hint = if signed { format!("Send the signature in {sh} as hex (or sha256=<hex>){}. Grafana: in the webhook contact point set HMAC Signature with this secret{}.", if ts.is_empty() { String::new() } else { format!(", and the time in {ts}; sign \"<time>:<body>\"") }, if ts.is_empty() { "" } else { " and this timestamp header" }) } else { String::new() };
    let curl = if signed {
        if ts.is_empty() { format!("body='{{\"messageId\": \"a1\", \"message\": \"test\"}}'; sig=$(printf '%s' \"$body\" | openssl dgst -sha256 -hmac '{secret}' -hex | sed 's/.* //'); curl -X POST -H 'Content-Type: application/json' -H \"{sh}: $sig\" -d \"$body\" '{u}'") } else { String::new() }
    } else { format!("curl -X POST -H 'Content-Type: application/json' -d '{{\"messageId\": \"a1\", \"message\": \"test\"}}' '{u}'") };
    let seen = n(tr, "seenCount") as i64;
    g(|f| { let mut t = f.get_trig(); t.hook = true; t.hook_url = ss(&u); t.hook_signed = signed; t.hook_secret = ss(&secret); t.hook_hint = ss(&hint); t.hook_curl = ss(&curl);
        t.hook_seen = ss(if seen > 0 { format!("{seen} idempotency key{} remembered (30 days).", if seen == 1 { "" } else { "s" }) } else { String::new() }); f.set_trig(t); });
}

fn render_history(tr: Option<&Value>) {
    let now = model::now_ms();
    let lines: Vec<SharedString> = tr.map(|t| arr(t, "history").iter().map(|h| ss(model::trigger_history_line(h, now))).collect()).unwrap_or_default();
    super::models(|m| m.d.tr_history.set_vec(lines));
}

fn collect() -> Value {
    with(|d| {
        let c = &d.tr.cur;
        let sections: serde_json::Map<String, Value> = d.tr.sections.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(k, v)| (k.clone(), json!(v))).collect();
        let wh = &c["webhook"];
        let at = { let a = s(&c["schedule"], "at"); if a.is_empty() { "09:00".to_string() } else { a } };
        let status = { let st = s(&c["kanban"], "status"); if st.is_empty() { "todo".to_string() } else { st } };
        let sig: String = { let x = s(wh, "signatureHeader"); if x.is_empty() { "x-crundi-signature".into() } else { x } };
        let auth = if s(wh, "auth") == "signature" { "signature" } else { "key" };
        let enabled = c.get("enabled").map(|x| x != &json!(false)).unwrap_or(true);
        let mut t = json!({ "name": s(c, "name"), "enabled": enabled, "project": d.tr.project, "factoryId": s(c, "factoryId"), "kind": s(c, "kind"),
            "schedule": { "at": at, "days": c["schedule"]["days"].clone() },
            "kanban": { "status": status }, "cr": { "title": s(&c["cr"], "title"), "sections": sections },
            "mergeMode": s(c, "mergeMode"), "ifBusy": s(c, "ifBusy"), "needsCards": s(c, "needsCards"),
            "webhook": { "auth": auth, "signatureHeader": sig, "timestampHeader": s(wh, "timestampHeader"), "idempotency": s(wh, "idempotency") } });
        if let Some(sv) = &d.tr.saved { t["id"] = json!(s(sv, "id")); }
        if !t["schedule"]["days"].is_array() { t["schedule"]["days"] = json!([]); }
        t
    })
}

fn preview() {
    let sample = tr_get("_sample");
    let body: Value = if sample.trim().is_empty() { json!({}) } else {
        match serde_json::from_str(&sample) { Ok(v) => v, Err(e) => { set_preview(vec![("note".into(), format!("That is not valid JSON: {e}"))]); return; } }
    };
    let idem_typed = !with(|d| s(&d.tr.cur["webhook"], "idempotency")).trim().is_empty();
    post("/api/factory-triggers", json!({ "action": "preview", "trigger": collect(), "sample": { "body": body } }), move |r| {
        if !b(&r, "ok") || !r["cr"].is_object() { set_preview(vec![("note".into(), err_text(&r, "No preview."))]); return; }
        let c = &r["cr"];
        let mut lines = vec![("b".to_string(), s(c, "title"))];
        if let Some(o) = c["sections"].as_object() { for (k, v) in o { lines.push(("meta".into(), format!("{k}: {}", v.as_str().map(String::from).unwrap_or_else(|| v.to_string())))); } }
        let idem = s(c, "idem");
        lines.push(("meta".into(), format!("Idempotency key: {}", if idem.is_empty() { format!("none in this call{}", if idem_typed { ", so repeats cannot be told apart" } else { "" }) } else { idem })));
        let miss: Vec<String> = arr(c, "missing").iter().chain(arr(c, "idemMissing").iter()).filter_map(|x| x.as_str()).map(|m| format!("{{{{{m}}}}}")).collect();
        if !miss.is_empty() { lines.push(("meta".into(), format!("Not in this payload (left empty, never an error): {}", miss.join(" ")))); }
        lines.push(("meta".into(), "The Manager also gets the whole request (headers, query, body) as a file.".into()));
        set_preview(lines);
    });
}
fn set_preview(lines: Vec<(String, String)>) { super::models(|m| m.d.tr_preview.set_vec(lines.into_iter().map(|(k, t)| PlanningFxLine { kind: ss(k), text: ss(t), id: "".into() }).collect::<Vec<_>>())); }

fn close_trigger() {
    with(|d| { d.tr.open = false; d.tr.gen += 1; });
    g(|f| { let mut t = f.get_trig(); t.open = false; f.set_trig(t); });
    sync_modal();
}

fn save_trigger() {
    post("/api/factory-triggers", json!({ "action": "save", "trigger": collect() }), |r| {
        if !b(&r, "ok") { kit::toast_error(&err_text(&r, "Could not save.")); return; }
        let t = r["trigger"].clone();
        with(|d| d.tr.saved = Some(t.clone()));
        kit::toast_ok("Saved.");
        super::panel_load();
        if s(&t, "kind") == "webhook" { render_hook(Some(&t)); g(|f| { let mut x = f.get_trig(); x.title = "Trigger".into(); x.editing = true; f.set_trig(x); }); } else { close_trigger(); }
    });
}

// ─── The actions and fields ───

pub fn act(a: &str, x: &str, _y: &str) {
    match a {
        "st-close" => close_start(),
        "st-go" => start(x == "anyway"),
        "st-attach" => attach(),
        "st-unattach" => { let i: usize = x.parse().unwrap_or(usize::MAX); with(|d| if i < d.st.files.len() { d.st.files.remove(i); }); push_start(); }
        "st-clear" => {
            let typed = with(|d| !d.st.title.is_empty() || d.st.sections.values().any(|v| !v.is_empty()));
            let go = || {
                with(|d| { d.st.title.clear(); for v in d.st.sections.values_mut() { v.clear(); } d.st.files.clear(); });
                drop_draft(&fid());
                render_sections(Some(json!({ "title": "", "sections": {} })));
                push_start();
            };
            if typed { kit::confirm(kit::Confirm { title: "Clear form", message: "Clear the change request?", ok: "Clear", danger: false }, move |ok| if ok { go() }); } else { go(); }
        }
        "st-open-active" => { close_start(); super::open_run_in_tab(x); }
        "pin-cancel" => pin_done(None),
        "pin-skip" => { with(|d| d.pin.i += 1); show_pin(); }
        "pin-unlock" => pin_unlock(),
        "tr-close" => close_trigger(),
        "tr-save" => save_trigger(),
        "tr-preview" => preview(),
        "tr-delete" => {
            let Some(id) = with(|d| d.tr.saved.as_ref().map(|t| s(t, "id"))) else { return };
            kit::confirm(kit::Confirm { title: "Delete", message: "Delete this trigger?", ok: "Delete", danger: true }, move |ok| {
                if !ok { return; }
                post("/api/factory-triggers", json!({ "action": "delete", "id": id }), |r| { if b(&r, "ok") { close_trigger(); super::panel_load(); } else { kit::toast_error(&err_text(&r, "That did not work.")); } });
            });
        }
        "tr-run" => {
            let Some(id) = with(|d| d.tr.saved.as_ref().map(|t| s(t, "id"))) else { return };
            post("/api/factory-triggers", json!({ "action": "run", "id": id }), |r| {
                if b(&r, "ok") { kit::toast_ok(if b(&r, "queued") { "Queued: a run is going in this project." } else { "Started." }); } else { kit::toast_error(&err_text(&r, "It could not start.")); }
                super::panel_load();
            });
        }
        "tr-rotate" => {
            let Some(t) = with(|d| d.tr.saved.clone()) else { return };
            let signed = s(&t["webhook"], "auth") == "signature";
            let msg = if signed { "Calls signed with the old secret stop working. Make a new one?" } else { "The old URL stops working. Make a new key?" };
            kit::confirm(kit::Confirm { title: if signed { "New secret" } else { "New key" }, message: msg, ok: "OK", danger: false }, move |ok| {
                if !ok { return; }
                post("/api/factory-triggers", json!({ "action": "rotate", "id": s(&t, "id"), "which": if signed { "secret" } else { "key" } }), |r| {
                    if b(&r, "ok") { let t2 = r["trigger"].clone(); with(|d| d.tr.saved = Some(t2.clone())); render_hook(Some(&t2)); }
                });
            });
        }
        "chat-close" => { super::with(|s2| s2.chat = None); g(|f| f.set_chat_open(false)); sync_modal(); }
        "chat-tab" => { if let Some((id, _)) = super::with(|s2| s2.chat.clone()) { let _ = open::that(format!("{}/chat/{}", kit::base(), kit::enc(&id))); } }
        _ => {}
    }
}

pub fn edit(form: &str, k: &str, v: &str) {
    match form {
        "start" => {
            match k {
                "project" => { let i: usize = v.parse().unwrap_or(0); with(|d| d.st.project_i = i); push_start(); precheck(); }
                "factory" => {
                    let i: usize = v.parse().unwrap_or(0);
                    // A draft factory cannot be picked (a disabled option).
                    if !with(|d| d.st.factories.get(i).map(|f| b(f, "runnable")).unwrap_or(false)) { push_start(); return; }
                    let last = fid();
                    if !last.is_empty() { save_draft(&last, &values()); }
                    with(|d| d.st.factory_i = i);
                    let mine = read_draft(&fid());
                    render_sections(if mine.is_some() { None } else { Some(values()) });
                    let ok = with(|d| d.st.pre.as_ref().map(|p| b(p, "ok") && !p.get("active").map(|a| a.is_object()).unwrap_or(false)).unwrap_or(false));
                    with(|d| d.st.go_enabled = ok);
                    push_start();
                }
                "merge" => { with(|d| d.st.merge_i = v.parse().unwrap_or(0)); push_start(); }
                "title" => { with(|d| d.st.title = v.chars().take(120).collect()); schedule_draft(); }
                x if x.starts_with("sec:") => { with(|d| { d.st.sections.insert(x[4..].to_string(), v.to_string()); }); schedule_draft(); }
                _ => {}
            }
        }
        "pin" if k == "value" => with(|d| d.pin.value = v.to_string()),
        "trig" => {
            let i: usize = v.parse().unwrap_or(0);
            let mut rerender = false;
            with(|d| {
                let c = &mut d.tr.cur;
                match k {
                    "name" => c["name"] = json!(v),
                    "factory" => { c["factoryId"] = json!(d.tr.factories.get(i).map(|f| s(f, "id")).unwrap_or_default()); rerender = true; }
                    "kind" => { c["kind"] = json!(KINDS[i.min(2)]); rerender = true; }
                    "at" => c["schedule"]["at"] = json!(v),
                    "day" => {
                        let mut days: Vec<i64> = arr(&c["schedule"], "days").iter().filter_map(|x| x.as_i64()).collect();
                        let dd = i as i64;
                        if let Some(p) = days.iter().position(|x| *x == dd) { days.remove(p); } else { days.push(dd); }
                        c["schedule"]["days"] = json!(days);
                    }
                    "column" => c["kanban"]["status"] = json!(model::COLUMNS[i.min(3)].0),
                    "title" => c["cr"]["title"] = json!(v),
                    "merge" => c["mergeMode"] = json!(MERGE[i.min(2)]),
                    "busy" => c["ifBusy"] = json!(BUSY[i.min(2)]),
                    "needs" => c["needsCards"] = json!(if i == 0 { "" } else { model::COLUMNS[(i - 1).min(3)].0 }),
                    "enabled" => c["enabled"] = json!(v == "1"),
                    "auth" => c["webhook"]["auth"] = json!(if i == 1 { "signature" } else { "key" }),
                    "sig-h" => c["webhook"]["signatureHeader"] = json!(v),
                    "ts-h" => c["webhook"]["timestampHeader"] = json!(v),
                    "idem" => c["webhook"]["idempotency"] = json!(v),
                    "sample" => c["_sample"] = json!(v),
                    x if x.starts_with("sec:") => { d.tr.sections.insert(x[4..].to_string(), v.to_string()); }
                    _ => {}
                }
            });
            if rerender { if k == "kind" { render_kind(false); } else { render_cr(); } g(|f| { let mut t = f.get_trig(); t.gen += 1; f.set_trig(t); }); }
            else if !["name", "title", "sig-h", "ts-h", "idem", "sample", "at"].contains(&k) && !k.starts_with("sec:") { push_trig(); }
        }
        _ => {}
    }
}

pub fn props(f: &PlanningFactories) -> Value {
    use slint::Model;
    let st = f.get_start();
    let pin = f.get_pin();
    let tr = f.get_trig();
    json!({
        "start": { "open": st.open, "loading": st.loading, "empty": st.empty, "project": st.project_i, "factory": st.factory_i, "merge": st.merge_i, "title": st.title.as_str(), "titleBad": st.title_bad,
            "go": st.go_label.as_str(), "goEnabled": st.go_enabled, "projects": f.get_st_projects().iter().map(|x| x.to_string()).collect::<Vec<_>>(), "factories": f.get_st_factories().iter().map(|x| x.to_string()).collect::<Vec<_>>(),
            "sections": f.get_st_sections().iter().map(|x| json!({ "id": x.id.as_str(), "label": x.label.as_str(), "required": x.required, "value": x.value.as_str(), "bad": x.bad })).collect::<Vec<_>>(),
            "files": f.get_st_files().iter().map(|x| x.name.to_string()).collect::<Vec<_>>(), "info": f.get_st_info().iter().map(|x| json!({ "kind": x.kind.as_str(), "text": x.text.as_str(), "id": x.id.as_str() })).collect::<Vec<_>>() },
        "pin": { "open": pin.open, "name": pin.name.as_str(), "step": pin.step.as_str(), "err": pin.err.as_str(), "pass": pin.pass },
        "trigger": { "open": tr.open, "loading": tr.loading, "empty": tr.empty, "title": tr.title.as_str(), "editing": tr.editing, "name": tr.name.as_str(), "kind": tr.kind_i, "crTitle": tr.cr_title.as_str(), "crHead": tr.cr_head.as_str(),
            "busy": tr.busy_i, "enabled": tr.enabled, "hook": tr.hook, "hookUrl": tr.hook_url.as_str(), "sections": f.get_tr_sections().iter().map(|x| json!({ "id": x.id.as_str(), "value": x.value.as_str() })).collect::<Vec<_>>(),
            "preview": f.get_tr_preview().iter().map(|x| x.text.to_string()).collect::<Vec<_>>(), "history": f.get_tr_history().row_count() },
    })
}
