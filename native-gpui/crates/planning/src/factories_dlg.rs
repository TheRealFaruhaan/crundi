//! Factories: the dialogs (crundi-factories.js): Start a factory (the change
//! request, its draft kept per factory, attachments uploaded at Start, the
//! project's precheck, missing skills and Start anyway), the PIN prompt for
//! the factory's secrets, a trigger (kanban, schedule, webhook with its
//! security, sample preview, URL, key or secret, history), and a person's
//! transcript (a sheet on the right). Drawn with the one modal look
//! (w::overlay_modal).
//!
//! Known gaps (as the Slint client): files cannot be dropped or pasted on
//! the change request (Attach picks them); the transcript is read-only text,
//! not the chat page.

use super::model::{self, arr, b, n, s};
use super::parts::{self, btn, field, note, prefs, FileChip, Line, Sec};
use super::{act as fact, edit_field, with as fwith};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

#[derive(Default, Clone, Copy, PartialEq)]
pub enum Then { #[default] None, Viewer, Panel }
pub struct StartOpts { pub project: String, pub factory: String, pub then: Then }

#[derive(Default)]
struct St {
    open: bool,
    loading: bool,
    empty: bool,
    then: Then,
    projects: Vec<(String, String)>,
    factories: Vec<Value>,
    factory_labels: Vec<String>,
    project_i: usize,
    factory_i: usize,
    merge_i: usize,
    title: String,
    sections: HashMap<String, String>,
    template: Vec<Value>,
    rows: Vec<Sec>,
    files: Vec<PathBuf>,
    pre: Option<Value>,
    info: Vec<(String, String, String)>,
    go_label: String,
    go_enabled: bool,
    title_bad: bool,
    bad: String,
    held: Option<Value>,
    to_unlock: Vec<Value>,
    gn: u64,
    save_gen: u64,
    focus: bool,
}

#[derive(Default)]
struct Pin { list: Vec<Value>, i: usize, pins: serde_json::Map<String, Value>, value: String, err: String, open: bool, name: String, step: String, sub: String, pass: bool, busy: bool }

#[derive(Default)]
struct Tr {
    open: bool,
    loading: bool,
    empty: bool,
    title: String,
    project: String,
    saved: Option<Value>,
    factories: Vec<Value>,
    cur: Value,
    sections: HashMap<String, String>,
    rows: Vec<Sec>,
    preview: Vec<Line>,
    history: Vec<String>,
    sample_open: bool,
    history_open: bool,
    hook: Option<Hook>,
    gn: u64,
}

#[derive(Default, Clone)]
struct Hook { url: String, signed: bool, secret: String, hint: String, curl: String, seen: String }

type PinThen = Box<dyn FnOnce(Option<Value>, &mut App)>;

#[derive(Default)]
struct D { st: St, pin: Pin, tr: Tr }

struct Handles { focus: FocusHandle, scroll: ScrollHandle }

thread_local! {
    static DS: RefCell<D> = RefCell::new(D::default());
    static PIN_THEN: RefCell<Option<PinThen>> = RefCell::new(None);
    static HANDLES: RefCell<HashMap<&'static str, Handles>> = RefCell::new(HashMap::new());
}
fn with<R>(f: impl FnOnce(&mut D) -> R) -> R { DS.with(|s| f(&mut s.borrow_mut())) }

const MAX_ATTACH: usize = 10;
const MAX_BYTES: u64 = 25 * 1024 * 1024;
const MERGE: [&str; 3] = ["", "ask", "auto"];
const BUSY: [&str; 3] = ["manager", "queue", "skip"];
const KINDS: [&str; 3] = ["kanban", "schedule", "webhook"];

/// One of these is open (the pointer goes to it only).
pub fn modal_open() -> bool { with(|d| d.st.open || d.tr.open || d.pin.open) || fwith(|f| f.chat_open) }

pub fn close_all() {
    with(|d| *d = D::default());
    PIN_THEN.with(|p| p.borrow_mut().take());
    fwith(|f| { f.chat_open = false; f.chat = None; });
}

// ─── Start a factory ───

fn draft_key(fid: &str) -> String { format!("crundi.factoryCr.{fid}") }
fn read_draft(fid: &str) -> Option<Value> { serde_json::from_str(&prefs::get(&draft_key(fid))).ok().filter(|v: &Value| v.is_object()) }
fn save_draft(fid: &str, v: &Value) { if !fid.is_empty() { prefs::set(&draft_key(fid), &v.to_string()); } }
fn drop_draft(fid: &str) { prefs::set(&draft_key(fid), ""); }

pub fn open_start(o: StartOpts, cx: &mut App) {
    let projects: Vec<(String, String)> = crundi_shell::session::get(cx).projects.iter().map(|p| (s(p, "alias"), { let nm = s(p, "name"); if nm.is_empty() { s(p, "alias") } else { nm } })).collect();
    let project = if !o.project.is_empty() { o.project.clone() } else { let c = crundi_shell::project(cx); if !c.is_empty() { c } else { projects.first().map(|p| p.0.clone()).unwrap_or_default() } };
    let pi = projects.iter().position(|p| p.0.eq_ignore_ascii_case(&project)).unwrap_or(0);
    parts::forget("start|");
    let gn = with(|d| { let g2 = d.st.gn + 1; d.st = St { open: true, loading: true, then: o.then, projects: projects.clone(), project_i: pi, go_label: "Start".into(), gn: g2, focus: true, ..Default::default() }; g2 });
    let want = o.factory.clone();
    parts::get(cx, "/api/factories".into(), move |lib, cx| {
        if with(|d| d.st.gn != gn || !d.st.open) { return; }
        let list = arr(&lib, "factories").to_vec();
        if list.is_empty() { with(|d| { d.st.loading = false; d.st.empty = true; }); return; }
        let fi = list.iter().position(|f| s(f, "id") == want && !want.is_empty()).or_else(|| list.iter().position(|f| b(f, "runnable"))).unwrap_or(0);
        let labels: Vec<String> = list.iter().map(|f| format!("{}{}", s(f, "name"), if b(f, "runnable") { String::new() } else { format!(" (draft: {} to fix)", n(f, "errorCount") as i64) })).collect();
        with(|d| { d.st.factories = list.clone(); d.st.factory_i = fi; d.st.factory_labels = labels; });
        render_sections(None);
        with(|d| { d.st.loading = false; d.st.empty = false; });
        precheck(cx);
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
        if keep.is_none() { let t = s(&draft, "title"); if !t.is_empty() { d.st.title = t; } } else { d.st.title = s(&draft, "title").or_keep(&d.st.title); }
        let old = std::mem::take(&mut d.st.sections);
        for sec in &template {
            let id = s(sec, "id");
            let v = draft.get("sections").and_then(|x| x.get(&id)).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(String::from).or_else(|| old.get(&id).cloned()).unwrap_or_default();
            d.st.sections.insert(id, v);
        }
        d.st.template = template.clone();
    });
    refresh_rows();
}

trait OrKeep { fn or_keep(self, old: &str) -> String; }
impl OrKeep for String { fn or_keep(self, old: &str) -> String { if self.is_empty() && !old.is_empty() { old.to_string() } else { self } } }

fn refresh_rows() {
    with(|d| {
        d.st.rows = d.st.template.iter().map(|sec| {
            let list = b(sec, "list");
            let hint = s(sec, "hint");
            let per_line = hint.to_lowercase().contains("per line");
            Sec { id: s(sec, "id"), label: s(sec, "label"), hint: format!("{hint}{}", if list && !per_line { "\nOne per line." } else { "" }), required: b(sec, "required"),
                rows: if list { 3 } else { 2 }, value: d.st.sections.get(&s(sec, "id")).cloned().unwrap_or_default(), bad: d.st.bad == s(sec, "id") }
        }).collect();
    });
}

fn precheck(cx: &mut App) {
    let project = with(|d| { d.st.info.clear(); d.st.go_enabled = false; d.st.projects.get(d.st.project_i).map(|p| p.0.clone()).unwrap_or_default() });
    let gn = with(|d| d.st.gn);
    parts::get(cx, format!("/api/factory-runs/precheck?project={}", parts::enc(&project)), move |pre, _| {
        if with(|d| d.st.gn != gn) { return; }
        let pre = if pre.get("ok").is_none() && pre.get("active").is_none() { json!({ "ok": false, "error": "Could not check the project." }) } else { pre };
        let lines = model::precheck_lines(&pre);
        let ok = b(&pre, "ok") && !pre.get("active").map(|a| a.is_object()).unwrap_or(false);
        with(|d| { d.st.info = lines.into_iter().map(|(k, t)| if k == "open" { ("open".to_string(), String::new(), t) } else { (k, t, String::new()) }).collect(); d.st.pre = Some(pre); d.st.go_enabled = ok && !d.st.factories.is_empty(); });
    });
}

fn schedule_draft(cx: &mut App) {
    let gn = with(|d| { d.st.save_gen += 1; d.st.save_gen });
    parts::after(cx, 250, move |_| { if with(|d| d.st.save_gen == gn && d.st.open) { save_draft(&fid(), &values()); } });
}

fn field_error(id: &str, msg: &str, cx: &mut App) {
    parts::toast_error(cx, msg);
    with(|d| { if id == "title" { d.st.title_bad = true; d.st.focus = true; } else { d.st.bad = id.to_string(); } });
    refresh_rows();
    let gn = with(|d| d.st.gn);
    parts::after(cx, 2500, move |_| { if with(|d| d.st.gn == gn) { with(|d| { d.st.title_bad = false; d.st.bad.clear(); }); refresh_rows(); } });
}

/// start(): check, ask the secrets' PINs, upload, start.
fn start(anyway: bool, cx: &mut App) {
    let (title, template, sections) = with(|d| (d.st.title.clone(), d.st.template.clone(), d.st.sections.clone()));
    if title.trim().is_empty() { field_error("title", "Give the change request a title.", cx); return; }
    for sec in &template {
        if b(sec, "required") && sections.get(&s(sec, "id")).map(|v| v.trim().is_empty()).unwrap_or(true) { field_error(&s(sec, "id"), &format!("Fill in \"{}\".", s(sec, "label")), cx); return; }
    }
    let held = with(|d| if anyway { d.st.held.clone() } else { None });
    if let Some(pins) = held { start2(anyway, Some(pins), cx); return; }
    let f = cur_factory().unwrap_or(Value::Null);
    let ids: Vec<String> = arr(&f, "secrets").iter().filter_map(|x| x.as_str().map(String::from)).collect();
    if ids.is_empty() { start2(anyway, None, cx); return; }
    super::caps(cx, move |caps, cx| {
        if !caps.iter().any(|c| c == "factory-secrets") { start2(anyway, None, cx); return; }
        parts::get(cx, "/api/secrets".into(), move |d2, cx| {
            let known = arr(&d2, "secrets").to_vec();
            let list: Vec<Value> = ids.iter().filter_map(|id| known.iter().find(|x| s(x, "id") == *id)).map(|x| json!({ "id": s(x, "id"), "name": s(x, "name"), "lock": if s(x, "lock") == "passphrase" { "passphrase" } else { "pin" } })).collect();
            if list.is_empty() { start2(anyway, None, cx); return; }
            with(|d| d.st.to_unlock = list.clone());
            ask_pins(list, move |got, cx| {
                let Some(pins) = got else { return };
                with(|d| d.st.held = Some(pins.clone()));
                start2(anyway, Some(pins), cx);
            });
        });
    });
}

fn start2(anyway: bool, pins: Option<Value>, cx: &mut App) {
    let (project, fid2, files, mm, title, sections) = with(|d| (d.st.projects.get(d.st.project_i).map(|p| p.0.clone()).unwrap_or_default(), d.st.factories.get(d.st.factory_i).map(|f| s(f, "id")).unwrap_or_default(), d.st.files.clone(), MERGE[d.st.merge_i.min(2)].to_string(), d.st.title.clone(), d.st.sections.clone()));
    with(|d| { d.st.go_enabled = false; d.st.go_label = if files.is_empty() { "Starting…".into() } else { format!("Uploading 1/{}…", files.len()) }; });
    let gn = with(|d| d.st.gn);
    let pins2 = pins.clone();
    let project2 = project.clone();
    crundi_shell::call(cx, move |api| {
        use base64::Engine;
        let mut atts = vec![];
        for p in files.iter() {
            let name = p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            let up = std::fs::read(p).map_err(|e| format!("Could not read {name}: {e}")).and_then(|bytes| api.post("/api/attachments/upload", &json!({ "project": project2, "name": name, "data": base64::engine::general_purpose::STANDARD.encode(bytes) })).map_err(|e| e.to_string()));
            match up {
                Ok(r) if b(&r, "ok") => atts.push(json!({ "path": s(&r, "path"), "name": name })),
                Ok(r) => return Ok(json!({ "upload": name, "error": parts::err_text(&r, "failed") })),
                Err(e) => return Ok(json!({ "upload": name, "error": e })),
            }
        }
        let mut body = json!({ "action": "start", "project": project2, "factoryId": fid2, "cr": { "title": title, "sections": sections, "attachments": atts }, "ignoreMissingSkills": anyway });
        if !mm.is_empty() { body["mergeMode"] = json!(mm); }
        if let Some(p) = pins2 { body["secretPins"] = p; }
        match api.post("/api/factory-runs", &body) { Ok(r) => Ok(r), Err(e) => Ok(json!({ "ok": false, "error": e.to_string() })) }
    }, move |r, cx| {
        let r = r.unwrap_or_else(|e| json!({ "ok": false, "error": e }));
        cx.refresh_windows();
        if with(|d| d.st.gn != gn) { return; }
        with(|d| { d.st.go_label = "Start".into(); d.st.go_enabled = true; });
        if !s(&r, "upload").is_empty() { parts::toast_error(cx, &format!("Could not upload {}: {}", s(&r, "upload"), s(&r, "error"))); return; }
        if b(&r, "ok") || s(&r, "field") == "secrets" { with(|d| d.st.held = None); }
        if b(&r, "ok") {
            let (fid3, then) = with(|d| (d.st.factories.get(d.st.factory_i).map(|f| s(f, "id")).unwrap_or_default(), d.st.then));
            drop_draft(&fid3);
            close_start();
            parts::toast_ok(cx, "Factory started.");
            let run_id = s(&r["run"], "id");
            match then { Then::Viewer => super::open_sub("viewer", &run_id, cx), Then::Panel => super::panel_load(cx), Then::None => {} }
            // Known gap: secrets unlocked with an old lock are not offered a passphrase here (the vault is Operations').
            return;
        }
        if s(&r, "field") == "secrets" { with(|d| d.st.info = vec![("err".into(), format!("{} Press Start to type it again.", parts::err_text(&r, "A secret could not be unlocked.")), String::new())]); return; }
        let field = s(&r, "field");
        if !field.is_empty() && field != "attachments" { field_error(&field, &s(&r, "error"), cx); return; }
        if let Some(ms) = r.get("missingSkills").and_then(|x| x.as_array()) {
            let names: Vec<String> = ms.iter().filter_map(|x| x.as_str().map(String::from)).collect();
            with(|d| d.st.info = vec![("warn".into(), model::missing_skills_text(&names), String::new()), ("anyway".into(), String::new(), String::new())]);
            return;
        }
        with(|d| {
            d.st.info = vec![("err".into(), parts::err_text(&r, "It could not start."), String::new())];
            for e in arr(&r, "errors").iter().take(5) { d.st.info.push(("note".into(), format!("· {}", s(e, "message")), String::new())); }
        });
    });
}

fn close_start() { with(|d| { d.st.open = false; d.st.gn += 1; }); }

fn attach(cx: &mut App) {
    crundi_shell::run(cx, || crundi_core::driver::take_pick().map(|p| vec![p]).or_else(|| rfd::FileDialog::new().pick_files()).unwrap_or_default(), |paths, cx| {
        for p in paths {
            if with(|d| d.st.files.len() >= MAX_ATTACH) { parts::toast_error(cx, &format!("Up to {MAX_ATTACH} attachments.")); break; }
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            if size > MAX_BYTES { parts::toast_error(cx, &format!("{} is over 25 MB.", p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default())); continue; }
            with(|d| d.st.files.push(p));
        }
        cx.refresh_windows();
    });
}

// ─── The PIN prompt ───

fn ask_pins(list: Vec<Value>, then: impl FnOnce(Option<Value>, &mut App) + 'static) {
    PIN_THEN.with(|p| *p.borrow_mut() = Some(Box::new(then)));
    with(|d| d.pin = Pin { list, ..Default::default() });
    show_pin_quiet();
}

/// The next secret's prompt; Some(pins) when every one was asked.
fn show_pin_quiet() -> Option<Value> {
    let (done, x, i, total) = with(|d| (d.pin.i >= d.pin.list.len(), d.pin.list.get(d.pin.i).cloned().unwrap_or(Value::Null), d.pin.i, d.pin.list.len()));
    if done {
        let pins = with(|d| { let p = std::mem::take(&mut d.pin.pins); d.pin = Pin::default(); p });
        return Some(Value::Object(pins));
    }
    let pass = s(&x, "lock") == "passphrase";
    parts::forget("pin|");
    with(|d| { let p = &mut d.pin; p.value.clear(); p.err.clear(); p.open = true; p.name = s(&x, "name"); p.step = if total > 1 { format!("Secret {} of {total}", i + 1) } else { String::new() }; p.sub = model::pin_sub(pass); p.pass = pass; p.busy = false; });
    None
}
fn show_pin(cx: &mut App) { if let Some(p) = show_pin_quiet() { pin_done(Some(p), cx); } }

fn pin_done(v: Option<Value>, cx: &mut App) {
    with(|d| d.pin = Pin::default());
    let then = PIN_THEN.with(|p| p.borrow_mut().take());
    if let Some(t) = then { t(v, cx); }
}

fn pin_unlock(cx: &mut App) {
    let (x, value) = with(|d| (d.pin.list.get(d.pin.i).cloned().unwrap_or(Value::Null), d.pin.value.clone()));
    let pass = s(&x, "lock") == "passphrase";
    let pin = if pass { value } else { value.trim().to_string() };
    let ok = if pass { !pin.is_empty() && pin.chars().count() <= 1024 } else { pin.len() == 6 && pin.chars().all(|c| c.is_ascii_digit()) };
    if !ok { with(|d| d.pin.err = if pass { "Enter the passphrase" } else { "Enter a 6-digit PIN" }.into()); return; }
    with(|d| d.pin.busy = true);
    let id = s(&x, "id");
    parts::post(cx, "/api/secrets", json!({ "action": "check", "id": id, "pin": pin }), move |r, cx| {
        if !b(&r, "ok") { let e = parts::err_text(&r, "Failed"); with(|d| { d.pin.busy = false; d.pin.err = e; }); return; }
        with(|d| { d.pin.pins.insert(id, json!(pin)); d.pin.i += 1; });
        show_pin(cx);
    });
}

// ─── A trigger ───

pub fn open_trigger(project: &str, id: &str, cx: &mut App) {
    let saved = fwith(|s2| s2.panel.triggers.iter().find(|t| s(t, "id") == id && !id.is_empty()).cloned());
    parts::forget("trig|");
    let gn = with(|d| { let g2 = d.tr.gn + 1; d.tr = Tr { open: true, loading: true, title: if saved.is_some() { "Trigger".into() } else { "New trigger".into() }, project: project.to_string(), saved: saved.clone(), gn: g2, ..Default::default() }; g2 });
    parts::get(cx, "/api/factories".into(), move |lib, cx| {
        if with(|d| d.tr.gn != gn) { return; }
        let saved = with(|d| d.tr.saved.clone());
        let list: Vec<Value> = arr(&lib, "factories").iter().filter(|f| b(f, "runnable") || saved.as_ref().map(|t| s(t, "factoryId") == s(f, "id")).unwrap_or(false)).cloned().collect();
        if list.is_empty() { with(|d| { d.tr.loading = false; d.tr.empty = true; }); return; }
        let cur = saved.clone().unwrap_or_else(|| json!({ "name": "", "enabled": true, "kind": "kanban", "factoryId": s(&list[0], "id"), "schedule": { "at": "09:00", "days": [] }, "kanban": { "status": "todo" }, "cr": { "title": "", "sections": {} }, "mergeMode": "", "ifBusy": "queue" }));
        with(|d| { d.tr.factories = list.clone(); d.tr.cur = cur; });
        render_kind(true);
        render_hook(saved.as_ref(), cx);
        render_history(saved.as_ref());
        with(|d| d.tr.loading = false);
    });
}

fn tr_get(k: &str) -> String { with(|d| s(&d.tr.cur, k)) }

/// renderKind(): a new webhook tells the Manager; then the CR.
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
    let rows: Vec<Sec> = tpl.iter().map(|sec| {
        let id = s(sec, "id");
        let v = if !first && keep.contains_key(&id) { keep[&id].clone() } else {
            let saved_v = s(&cur["cr"]["sections"], &id);
            if !saved_v.is_empty() { saved_v } else if kind == "kanban" && saved_none { match id.as_str() { "goal" => "{{card.description}}".into(), "done" => "{{card.todos}}".into(), _ => String::new() } } else { String::new() }
        };
        sections.insert(id.clone(), v.clone());
        Sec { id, label: s(sec, "label"), hint: s(sec, "hint"), required: false, rows: 2, value: v, bad: false }
    }).collect();
    with(|d| { d.tr.sections = sections; d.tr.cur["cr"]["title"] = json!(title); d.tr.rows = rows; });
}

fn render_hook(tr: Option<&Value>, cx: &App) {
    let Some(tr) = tr.filter(|t| s(t, "kind") == "webhook") else { with(|d| d.tr.hook = None); return };
    let wh = &tr["webhook"];
    let signed = s(wh, "auth") == "signature";
    let u = format!("{}/api/factory-hook/trigger/{}{}", parts::base(cx), s(tr, "id"), if signed { String::new() } else { format!("?key={}", s(wh, "key")) });
    let ts = s(wh, "timestampHeader");
    let sh = s(wh, "signatureHeader");
    let secret = s(wh, "secret");
    let hint = if signed { format!("Send the signature in {sh} as hex (or sha256=<hex>){}. Grafana: in the webhook contact point set HMAC Signature with this secret{}.", if ts.is_empty() { String::new() } else { format!(", and the time in {ts}; sign \"<time>:<body>\"") }, if ts.is_empty() { "" } else { " and this timestamp header" }) } else { String::new() };
    let curl = if signed {
        if ts.is_empty() { format!("body='{{\"messageId\": \"a1\", \"message\": \"test\"}}'; sig=$(printf '%s' \"$body\" | openssl dgst -sha256 -hmac '{secret}' -hex | sed 's/.* //'); curl -X POST -H 'Content-Type: application/json' -H \"{sh}: $sig\" -d \"$body\" '{u}'") } else { String::new() }
    } else { format!("curl -X POST -H 'Content-Type: application/json' -d '{{\"messageId\": \"a1\", \"message\": \"test\"}}' '{u}'") };
    let seen = n(tr, "seenCount") as i64;
    let h = Hook { url: u, signed, secret, hint, curl, seen: if seen > 0 { format!("{seen} idempotency key{} remembered (30 days).", if seen == 1 { "" } else { "s" }) } else { String::new() } };
    with(|d| d.tr.hook = Some(h));
}

fn render_history(tr: Option<&Value>) {
    let now = model::now_ms();
    let lines: Vec<String> = tr.map(|t| arr(t, "history").iter().map(|h| model::trigger_history_line(h, now)).collect()).unwrap_or_default();
    with(|d| d.tr.history = lines);
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

fn preview(cx: &mut App) {
    let sample = tr_get("_sample");
    let set_preview = |lines: Vec<(String, String)>| with(|d| d.tr.preview = lines.into_iter().map(|(k, t)| Line { kind: k, text: t, id: String::new() }).collect());
    let body: Value = if sample.trim().is_empty() { json!({}) } else {
        match serde_json::from_str(&sample) { Ok(v) => v, Err(e) => { set_preview(vec![("note".into(), format!("That is not valid JSON: {e}"))]); return; } }
    };
    let idem_typed = !with(|d| s(&d.tr.cur["webhook"], "idempotency")).trim().is_empty();
    parts::post(cx, "/api/factory-triggers", json!({ "action": "preview", "trigger": collect(), "sample": { "body": body } }), move |r, _| {
        if !b(&r, "ok") || !r["cr"].is_object() { set_preview(vec![("note".into(), parts::err_text(&r, "No preview."))]); return; }
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

fn close_trigger() { with(|d| { d.tr.open = false; d.tr.gn += 1; }); }

fn save_trigger(cx: &mut App) {
    parts::post(cx, "/api/factory-triggers", json!({ "action": "save", "trigger": collect() }), |r, cx| {
        if !b(&r, "ok") { parts::toast_error(cx, &parts::err_text(&r, "Could not save.")); return; }
        let t = r["trigger"].clone();
        with(|d| d.tr.saved = Some(t.clone()));
        parts::toast_ok(cx, "Saved.");
        super::panel_load(cx);
        if s(&t, "kind") == "webhook" { render_hook(Some(&t), cx); with(|d| d.tr.title = "Trigger".into()); } else { close_trigger(); }
    });
}

// ─── The actions and fields ───

pub fn act(a: &str, x: &str, _y: &str, cx: &mut App) {
    match a {
        "st-close" => close_start(),
        "st-go" => start(x == "anyway", cx),
        "st-attach" => attach(cx),
        "st-unattach" => { let i: usize = x.parse().unwrap_or(usize::MAX); with(|d| if i < d.st.files.len() { d.st.files.remove(i); }); }
        "st-clear" => {
            let typed = with(|d| !d.st.title.is_empty() || d.st.sections.values().any(|v| !v.is_empty()));
            let go = || {
                with(|d| { d.st.title.clear(); for v in d.st.sections.values_mut() { v.clear(); } d.st.files.clear(); });
                drop_draft(&fid());
                render_sections(Some(json!({ "title": "", "sections": {} })));
                with(|d| d.st.title.clear());
            };
            if typed { parts::confirm(cx, "Clear form", "Clear the change request?", "Clear", false, move |ok, _| if ok { go() }); } else { go(); }
        }
        "st-open-active" => { close_start(); super::open_run_in_tab(x, cx); }
        "pin-cancel" => pin_done(None, cx),
        "pin-skip" => { with(|d| d.pin.i += 1); show_pin(cx); }
        "pin-unlock" => pin_unlock(cx),
        "tr-close" => close_trigger(),
        "tr-save" => save_trigger(cx),
        "tr-preview" => preview(cx),
        "tr-sample" => with(|d| d.tr.sample_open = !d.tr.sample_open),
        "tr-history" => with(|d| d.tr.history_open = !d.tr.history_open),
        "tr-delete" => {
            let Some(id) = with(|d| d.tr.saved.as_ref().map(|t| s(t, "id"))) else { return };
            parts::confirm(cx, "Delete", "Delete this trigger?", "Delete", true, move |ok, cx| {
                if !ok { return; }
                parts::post(cx, "/api/factory-triggers", json!({ "action": "delete", "id": id }), |r, cx| { if b(&r, "ok") { close_trigger(); super::panel_load(cx); } else { parts::toast_error(cx, &parts::err_text(&r, "That did not work.")); } });
            });
        }
        "tr-run" => {
            let Some(id) = with(|d| d.tr.saved.as_ref().map(|t| s(t, "id"))) else { return };
            parts::post(cx, "/api/factory-triggers", json!({ "action": "run", "id": id }), |r, cx| {
                if b(&r, "ok") { parts::toast_ok(cx, if b(&r, "queued") { "Queued: a run is going in this project." } else { "Started." }); } else { parts::toast_error(cx, &parts::err_text(&r, "It could not start.")); }
                super::panel_load(cx);
            });
        }
        "tr-rotate" => {
            let Some(t) = with(|d| d.tr.saved.clone()) else { return };
            let signed = s(&t["webhook"], "auth") == "signature";
            let msg = if signed { "Calls signed with the old secret stop working. Make a new one?" } else { "The old URL stops working. Make a new key?" };
            parts::confirm(cx, if signed { "New secret" } else { "New key" }, msg, "OK", false, move |ok, cx| {
                if !ok { return; }
                parts::post(cx, "/api/factory-triggers", json!({ "action": "rotate", "id": s(&t, "id"), "which": if signed { "secret" } else { "key" } }), |r, cx| {
                    if b(&r, "ok") { let t2 = r["trigger"].clone(); with(|d| d.tr.saved = Some(t2.clone())); render_hook(Some(&t2), cx); }
                });
            });
        }
        "chat-close" => fwith(|s2| { s2.chat = None; s2.chat_open = false; }),
        "chat-tab" => { if let Some((id, _)) = fwith(|s2| s2.chat.clone()) { let _ = open::that(format!("{}/chat/{}", parts::base(cx), parts::enc(&id))); } }
        _ => {}
    }
}

pub fn edit(form: &str, k: &str, v: &str, cx: &mut App) {
    match form {
        "start" => match k {
            "project" => { let i: usize = v.parse().unwrap_or(0); with(|d| d.st.project_i = i); precheck(cx); }
            "factory" => {
                let i: usize = v.parse().unwrap_or(0);
                // A draft factory cannot be picked (a disabled option).
                if !with(|d| d.st.factories.get(i).map(|f| b(f, "runnable")).unwrap_or(false)) { return; }
                let last = fid();
                if !last.is_empty() { save_draft(&last, &values()); }
                with(|d| d.st.factory_i = i);
                let mine = read_draft(&fid());
                render_sections(if mine.is_some() { None } else { Some(values()) });
                let ok = with(|d| d.st.pre.as_ref().map(|p| b(p, "ok") && !p.get("active").map(|a| a.is_object()).unwrap_or(false)).unwrap_or(false));
                with(|d| d.st.go_enabled = ok);
            }
            "merge" => with(|d| d.st.merge_i = v.parse().unwrap_or(0)),
            "title" => { with(|d| d.st.title = v.chars().take(120).collect()); schedule_draft(cx); }
            x if x.starts_with("sec:") => { with(|d| { d.st.sections.insert(x[4..].to_string(), v.to_string()); }); refresh_rows(); schedule_draft(cx); }
            _ => {}
        },
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
                    x if x.starts_with("sec:") => { d.tr.sections.insert(x[4..].to_string(), v.to_string()); if let Some(r) = d.tr.rows.iter_mut().find(|r| r.id == x[4..]) { r.value = v.to_string(); } }
                    _ => {}
                }
            });
            if rerender { if k == "kind" { render_kind(false); } else { render_cr(); } }
        }
        _ => {}
    }
}

pub fn props() -> Value {
    with(|d| {
        let st = &d.st;
        let tr = &d.tr;
        let c = &tr.cur;
        json!({
            "start": { "open": st.open, "loading": st.loading, "empty": st.empty, "project": st.project_i, "factory": st.factory_i, "merge": st.merge_i, "title": st.title, "titleBad": st.title_bad,
                "go": st.go_label, "goEnabled": st.go_enabled, "projects": st.projects.iter().map(|p| p.1.clone()).collect::<Vec<_>>(), "factories": st.factory_labels,
                "sections": st.rows.iter().map(|x| json!({ "id": x.id, "label": x.label, "required": x.required, "value": x.value, "bad": x.bad })).collect::<Vec<_>>(),
                "files": st.files.iter().map(|p| p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()).collect::<Vec<_>>(),
                "info": st.info.iter().map(|(k, t, id)| json!({ "kind": k, "text": t, "id": id })).collect::<Vec<_>>() },
            "pin": { "open": d.pin.open, "name": d.pin.name, "step": d.pin.step, "err": d.pin.err, "pass": d.pin.pass },
            "trigger": { "open": tr.open, "loading": tr.loading, "empty": tr.empty, "title": tr.title, "editing": tr.saved.is_some(), "name": s(c, "name"),
                "kind": KINDS.iter().position(|k| *k == s(c, "kind")).unwrap_or(0), "crTitle": s(&c["cr"], "title"), "crHead": model::trigger_cr_head(&s(c, "kind")),
                "busy": BUSY.iter().position(|m| *m == s(c, "ifBusy")).unwrap_or(1), "enabled": c.get("enabled").map(|x| x != &json!(false)).unwrap_or(true),
                "hook": tr.hook.is_some(), "hookUrl": tr.hook.as_ref().map(|h| h.url.clone()).unwrap_or_default(),
                "sections": tr.rows.iter().map(|x| json!({ "id": x.id, "value": tr.sections.get(&x.id).cloned().unwrap_or_default() })).collect::<Vec<_>>(),
                "preview": tr.preview.iter().map(|x| x.text.clone()).collect::<Vec<_>>(), "history": tr.history.len() },
        })
    })
}

// ─── Drawing ───

fn handles(id: &'static str, cx: &mut App) -> (FocusHandle, ScrollHandle) {
    HANDLES.with(|h| { let mut h = h.borrow_mut(); let x = h.entry(id).or_insert_with(|| Handles { focus: cx.focus_handle(), scroll: ScrollHandle::new() }); (x.focus.clone(), x.scroll.clone()) })
}

fn modal(id: &'static str, width: Pixels, close: &'static str, title: &str, body: Vec<AnyElement>, window: &mut Window, cx: &mut App) -> AnyElement {
    let (focus, scroll) = handles(id, cx);
    let m = w::Modal {
        id, width, focus, scroll,
        on_close: Rc::new(move |_, cx| fact(close, "", "", cx)),
        on_key: Rc::new(move |e: &KeyDownEvent, _, cx| if e.keystroke.key == "escape" { fact(close, "", "", cx) }),
    };
    let mut kids = vec![w::modal_head(match close { "st-close" => "st-x", "tr-close" => "tr-x", "chat-close" => "chat-x", _ => "pin-x" }, title.to_string(), false, move |_, _, cx| fact(close, "", "", cx)).into_any_element()];
    kids.push(div().flex().flex_col().gap(T::SP_12).px(T::SP_10).pb(T::SP_10).children(body).into_any_element());
    w::overlay_modal(m, window, kids).into_any_element()
}

fn set(form: &'static str, k: &str, v: &str, cx: &mut App) { edit_field(form, k, v, cx); }
fn sel(id: impl Into<ElementId>, form: &'static str, k: &'static str, items: Vec<String>, on: usize) -> impl IntoElement {
    w::dropdown_of(id, items.into_iter().map(SharedString::from).collect(), on, false, true, move |i, _, cx| set(form, k, &i.to_string(), cx))
}
fn pair(stack: bool, a: impl IntoElement, b: impl IntoElement) -> Div {
    if stack { div().flex().flex_col().gap(T::SP_10).child(a).child(b) } else { div().flex().gap(T::SP_10).child(div().flex_1().min_w_0().child(a)).child(div().flex_1().min_w_0().child(b)) }
}

/// Every dialog of the screen that is open, drawn over the window.
pub fn render(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let mut out = vec![];
    let (st, tr, pin) = with(|d| (d.st.open, d.tr.open, d.pin.open));
    let in_graph = fwith(|f| f.chat_open);
    if in_graph { out.push(chat_sheet(window, cx)); }
    if tr { out.push(trigger_dialog(window, cx)); }
    if st { out.push(start_dialog(window, cx)); }
    if pin { out.push(pin_dialog(window, cx)); }
    out
}

fn start_dialog(window: &mut Window, cx: &mut App) -> AnyElement {
    let d = with(|d| (d.st.loading, d.st.empty, d.st.projects.iter().map(|p| p.1.clone()).collect::<Vec<_>>(), d.st.factory_labels.clone(), d.st.project_i, d.st.factory_i, d.st.merge_i,
        d.st.title.clone(), d.st.title_bad, d.st.rows.clone(), d.st.files.clone(), d.st.info.clone(), d.st.go_label.clone(), d.st.go_enabled, std::mem::take(&mut d.st.focus)));
    let (loading, empty, projects, factories, pi, fi, mi, title, title_bad, rows, files, info, go_label, go_enabled, focus) = d;
    let stack = window.viewport_size().width < T::FACTORIES_PAIR_STACK;
    let mut body: Vec<AnyElement> = vec![];
    if loading { body.push(note("note", "Loading…").into_any_element()); }
    else if empty {
        body.push(note("note", "There are no factories yet. Create one in the Factories tab first.").into_any_element());
        body.push(div().flex().justify_end().child(btn("st-close-e", "Close", None, "ghost", true, false).on_click(|_, _, cx| fact("st-close", "", "", cx))).into_any_element());
    } else {
        body.push(pair(stack, field("Project", "", "", sel("st-project", "start", "project", projects, pi)), field("Factory", "", "", sel("st-factory", "start", "factory", factories, fi))).into_any_element());
        // .fx-cr: the change request.
        let title_box = parts::line("start|title", &title, "One line: what should change", false, false, parts::Commit::Change, window, cx, |v, cx| set("start", "title", &v, cx));
        if focus { parts::focus("start|title", window, cx); }
        let mut cr = div().flex().flex_col().gap(T::SP_10).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg())
            .child(div().flex().flex_col().gap(T::SP_2)
                .child(div().text_size(T::TEXT_13).font_weight(T::W_BOLD).child("Change request"))
                .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("The Manager gets all of it; every step gets the title, goal and the checks for done, and can open the rest.")))
            .child(field("Title", "required", "", div().rounded(T::R_CONTROL).when(title_bad, |d| d.border_2().border_color(T::red())).child(title_box)));
        for sec in &rows {
            let k = format!("sec:{}", sec.id);
            let ta = parts::area(&format!("start|{k}"), &sec.value, &sec.hint, sec.rows as usize, false, window, cx, move |v, cx| set("start", &k, &v, cx));
            cr = cr.child(field(&sec.label, if sec.required { "required" } else { "" }, "", div().rounded(T::R_CONTROL).when(sec.bad, |d| d.border_2().border_color(T::red())).child(ta)));
        }
        let mut att = div().flex().flex_wrap().items_center().gap(T::SP_6).child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Attachments"));
        for (i, f) in files.iter().enumerate() {
            let nm = f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            let chip = FileChip { image: [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg"].iter().any(|e| nm.to_lowercase().ends_with(e)), name: nm };
            let is = i.to_string();
            att = att.child(div().flex().items_center().gap(T::SP_4).h(T::CONTROL_SM - T::SP_2).max_w(T::FACTORIES_CHIP_MAX).pl(T::SP_8).pr(T::SP_4).rounded(T::R_PILL).border_1().border_color(T::border()).bg(T::bg_tertiary())
                .child(w::icon(if chip.image { IconName::Image } else { IconName::File }, T::ICON_MD))
                .child(div().min_w_0().truncate().text_size(T::TEXT_12).child(chip.name.clone()))
                .child(w::icon_button(("st-unattach", i), IconName::X, "Remove").on_click(move |_, _, cx| fact("st-unattach", &is, "", cx))));
        }
        att = att.child(btn("st-attach", "Attach", Some(IconName::Paperclip), "", true, false).on_click(|_, _, cx| fact("st-attach", "", "", cx)));
        cr = cr.child(div().flex().flex_col().gap(T::SP_6).child(att).child(div().text_size(T::TEXT_11).text_color(T::text_secondary()).child("Screenshots, mockups, specs, logs. Attach picks them from this PC.")));
        body.push(cr.into_any_element());
        body.push(field("When the work is done", "", "", sel("st-merge", "start", "merge", vec!["As the factory says".into(), "Show me first, I decide".into(), "Merge automatically (with Undo)".into()], mi)).into_any_element());
        for (i, (k, t, id)) in info.iter().enumerate() {
            let el = match k.as_str() {
                "open" => { let id = id.clone(); div().child(btn(("st-open", i), "Open it", Some(IconName::Route), "", true, false).on_click(move |_, _, cx| fact("st-open-active", &id, "", cx))).into_any_element() }
                "anyway" => div().child(btn(("st-anyway", i), "Start anyway", Some(IconName::Play), "", true, false).on_click(|_, _, cx| fact("st-go", "anyway", "", cx))).into_any_element(),
                _ => note(k, t.clone()).into_any_element(),
            };
            body.push(el);
        }
        body.push(div().flex().items_center().gap(T::SP_8)
            .child(btn("st-clear", "Clear form", None, "ghost", true, false).on_click(|_, _, cx| fact("st-clear", "", "", cx)))
            .child(div().flex_1())
            .child(btn("st-cancel", "Cancel", None, "ghost", true, false).on_click(|_, _, cx| fact("st-close", "", "", cx)))
            .child(btn("st-go", &go_label, Some(IconName::Play), "primary", go_enabled, false).on_click(|_, _, cx| fact("st-go", "", "", cx))).into_any_element());
    }
    modal("fx-start", T::FACTORIES_MODAL_WIDE, "st-close", "Start a factory", body, window, cx)
}

fn pin_dialog(window: &mut Window, cx: &mut App) -> AnyElement {
    let (name, step, subt, pass, value, err, busy) = with(|d| (d.pin.name.clone(), d.pin.step.clone(), d.pin.sub.clone(), d.pin.pass, d.pin.value.clone(), d.pin.err.clone(), d.pin.busy));
    let input = parts::line("pin|value", &value, if pass { "Passphrase" } else { "••••••" }, false, true, parts::Commit::Change, window, cx, |v, cx| set("pin", "value", &v, cx));
    parts::on_enter("pin|value", |_, cx| fact("pin-unlock", "", "", cx));
    parts::focus("pin|value", window, cx);
    let body = vec![
        div().flex().items_center().justify_center().gap(T::SP_6).child(w::icon(IconName::Lock, T::ICON_LG)).child(div().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).child(name)).into_any_element(),
        div().when(!step.is_empty(), |d| d.flex().justify_center().text_size(T::TEXT_11).text_color(T::text_muted()).child(step)).into_any_element(),
        div().flex().justify_center().text_size(T::TEXT_12).text_color(T::text_secondary()).child(subt).into_any_element(),
        input,
        div().flex().justify_center().min_h(T::SP_16).text_size(T::TEXT_12).text_color(T::red()).child(err).into_any_element(),
        div().flex().gap(T::SP_8)
            .child(div().flex_1().child(btn("pin-cancel", "Cancel", None, "ghost", true, false).on_click(|_, _, cx| fact("pin-cancel", "", "", cx))))
            .child(div().flex_1().child(btn("pin-skip", "Skip", None, "", true, false).on_click(|_, _, cx| fact("pin-skip", "", "", cx))))
            .child(div().flex_1().child(btn("pin-unlock", "Unlock", Some(IconName::Lock), "primary", !busy, false).on_click(|_, _, cx| fact("pin-unlock", "", "", cx)))).into_any_element(),
    ];
    modal("fx-pin", T::FACTORIES_PIN_W, "pin-cancel", "Unlock", body, window, cx)
}

fn trigger_dialog(window: &mut Window, cx: &mut App) -> AnyElement {
    let (loading, empty, title, cur, factories, rows, preview, history, sample_open, history_open, hook, editing) = with(|d| { let t = &d.tr;
        (t.loading, t.empty, t.title.clone(), t.cur.clone(), t.factories.clone(), t.rows.clone(), t.preview.clone(), t.history.clone(), t.sample_open, t.history_open, t.hook.clone(), t.saved.is_some()) });
    let stack = window.viewport_size().width < T::FACTORIES_PAIR_STACK;
    let mut body: Vec<AnyElement> = vec![];
    if loading { body.push(note("note", "Loading…").into_any_element()); return modal("fx-trig", T::FACTORIES_MODAL_WIDE, "tr-close", &title, body, window, cx); }
    if empty {
        body.push(note("note", "There are no runnable factories yet. Create one in the Factories tab first.").into_any_element());
        body.push(div().child(btn("tr-close-e", "Close", None, "ghost", true, false).on_click(|_, _, cx| fact("tr-close", "", "", cx))).into_any_element());
        return modal("fx-trig", T::FACTORIES_MODAL_WIDE, "tr-close", &title, body, window, cx);
    }
    let kind = KINDS.iter().position(|k| *k == s(&cur, "kind")).unwrap_or(0);
    let tline = |k: &'static str, v: &str, ph: &str, mono: bool, window: &mut Window, cx: &mut App| parts::line(&format!("trig|{k}"), v, ph, mono, false, parts::Commit::Change, window, cx, move |t, cx| set("trig", k, &t, cx));
    let fi = factories.iter().position(|x| s(x, "id") == s(&cur, "factoryId")).unwrap_or(0);
    body.push(pair(stack, field("Name", "", "", tline("name", &s(&cur, "name"), "Cards to the team", false, window, cx)),
        field("Factory", "", "", sel("tr-factory", "trig", "factory", factories.iter().map(|f| s(f, "name")).collect(), fi))).into_any_element());
    body.push(field("Start when", "", "", sel("tr-kind", "trig", "kind", vec!["A kanban card enters a column".into(), "A time of day".into(), "A webhook is called".into()], kind)).into_any_element());
    if kind == 1 {
        body.push(field("At (this server's time)", "", "", tline("at", &{ let a = s(&cur["schedule"], "at"); if a.is_empty() { "09:00".into() } else { a } }, "09:00", false, window, cx)).into_any_element());
        let days: Vec<i64> = arr(&cur["schedule"], "days").iter().filter_map(|x| x.as_i64()).collect();
        let mut row = div().flex().gap(T::SP_4);
        for (i, dn) in model::DAYS.iter().enumerate() {
            let on = days.contains(&(i as i64));
            let is = i.to_string();
            row = row.child(w::toggle(("tr-day", i), *dn, on).on_click(move |_, _, cx| set("trig", "day", &is, cx)));
        }
        body.push(field("On (none = every day)", "", "", row).into_any_element());
    }
    if kind == 0 {
        let ci = model::COLUMNS.iter().position(|c| c.0 == s(&cur["kanban"], "status")).unwrap_or(1);
        body.push(field("Column", "", "", sel("tr-column", "trig", "column", model::COLUMNS.iter().map(|c| c.1.to_string()).collect(), ci)).into_any_element());
    }
    let mut cr = div().flex().flex_col().gap(T::SP_10).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg())
        .child(div().flex().flex_col().gap(T::SP_2)
            .child(div().text_size(T::TEXT_13).font_weight(T::W_BOLD).child("Change request"))
            .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(model::trigger_cr_head(&s(&cur, "kind")))))
        .child(field("Title", "required", "", tline("title", &s(&cur["cr"], "title"), "", false, window, cx)));
    for sec in &rows {
        let k = format!("sec:{}", sec.id);
        cr = cr.child(field(&sec.label, "", "", parts::area(&format!("trig|{k}"), &sec.value, &sec.hint, sec.rows as usize, false, window, cx, move |v, cx| set("trig", &k, &v, cx))));
    }
    body.push(cr.into_any_element());
    body.push(pair(stack,
        field("When the work is done", "", "", sel("tr-merge", "trig", "merge", vec!["As the factory says".into(), "Show me first, I decide".into(), "Merge automatically (with Undo)".into()], MERGE.iter().position(|m| *m == s(&cur, "mergeMode")).unwrap_or(0))),
        field("If a run is already going", "", "", sel("tr-busy", "trig", "busy", vec!["Tell its Manager (a run of the same factory)".into(), "Wait its turn, then start".into(), "Skip it".into()], BUSY.iter().position(|m| *m == s(&cur, "ifBusy")).unwrap_or(1)))).into_any_element());
    body.push(field("Start a run", "", "", sel("tr-needs", "trig", "needs", vec!["Always".into(), "Only if Backlog has a card".into(), "Only if To do has a card".into(), "Only if In progress has a card".into(), "Only if Done has a card".into()],
        model::COLUMNS.iter().position(|c| c.0 == s(&cur, "needsCards")).map(|i| i + 1).unwrap_or(0))).into_any_element());
    let enabled = cur.get("enabled").map(|x| x != &json!(false)).unwrap_or(true);
    body.push(w::check("tr-enabled", "On", enabled, |v, _, cx| set("trig", "enabled", if *v { "1" } else { "0" }, cx)).into_any_element());
    if kind == 2 {
        let wh = &cur["webhook"];
        let auth_i = (s(wh, "auth") == "signature") as usize;
        let mut c = div().flex().flex_col().gap(T::SP_10)
            .child(div().flex().flex_col().gap(T::SP_2)
                .child(div().text_size(T::TEXT_13).font_weight(T::W_BOLD).child("Webhook"))
                .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Public URL, so every call must prove it is from you: the key, or a signature made with the secret. Calls are answered at once; the change request above is filled from the call's JSON.")))
            .child(field("Security", "", "", sel("tr-auth", "trig", "auth", vec!["A key in the URL (or X-Crundi-Key / Bearer header)".into(), "Signed with a secret (HMAC-SHA256)".into()], auth_i)));
        if auth_i == 1 {
            c = c.child(pair(stack, field("Signature header", "", "", tline("sig-h", &{ let x = s(wh, "signatureHeader"); if x.is_empty() { "x-crundi-signature".into() } else { x } }, "x-crundi-signature", true, window, cx)),
                field("Timestamp header (optional)", "", "", tline("ts-h", &s(wh, "timestampHeader"), "None (Grafana: X-Grafana-Alerting-Signature-Timestamp)", true, window, cx))));
        }
        c = c.child(field("Idempotency key", "a call with a key already seen is not processed again", "", tline("idem", &s(wh, "idempotency"), "{{payload.messageId}}", true, window, cx)))
            .child(w::disclosure("tr-sample", "Try it with a sample payload", sample_open).on_click(|_, _, cx| fact("tr-sample", "", "", cx)));
        if sample_open {
            let mut sp = div().flex().flex_col().gap(T::SP_8)
                .child(parts::area("trig|sample", &s(&cur, "_sample"), "{\"messageId\": \"a1\", \"message\": \"CPU above 90% on web-1\"}", 4, true, window, cx, |v, cx| set("trig", "sample", &v, cx)))
                .child(div().child(btn("tr-preview", "Preview", None, "ghost", true, false).on_click(|_, _, cx| fact("tr-preview", "", "", cx))));
            for l in &preview { sp = sp.child(note(&l.kind, l.text.clone())); }
            c = c.child(sp);
        }
        body.push(c.into_any_element());
    }
    if let Some(h) = hook {
        let mut c = div().flex().flex_col().gap(T::SP_8)
            .child(field("Webhook URL (POST, JSON body)", "", "", parts::line("trig|hook-url", &h.url, "", true, false, parts::Commit::Leave, window, cx, |_, _| {})));
        if h.signed { c = c.child(field("Secret (sign the raw body with HMAC-SHA256)", "", "", parts::line("trig|hook-secret", &h.secret, "", true, false, parts::Commit::Leave, window, cx, |_, _| {}))); }
        if !h.hint.is_empty() { c = c.child(note("meta", h.hint.clone())); }
        if !h.curl.is_empty() { c = c.child(div().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(h.curl.clone())); }
        if !h.seen.is_empty() { c = c.child(note("meta", h.seen.clone())); }
        c = c.child(div().child(btn("tr-rotate", if h.signed { "New secret" } else { "New key" }, None, "ghost", true, false).on_click(|_, _, cx| fact("tr-rotate", "", "", cx))));
        body.push(c.into_any_element());
    }
    if !history.is_empty() {
        body.push(w::disclosure("tr-history", format!("What it did ({})", history.len()), history_open).on_click(|_, _, cx| fact("tr-history", "", "", cx)).into_any_element());
        if history_open { let mut l = div().flex().flex_col().gap(T::SP_4); for h in &history { l = l.child(note("meta", h.clone())); } body.push(l.into_any_element()); }
    }
    body.push(div().flex().items_center().gap(T::SP_8)
        .when(editing, |d| d.child(btn("tr-delete", "Delete", Some(IconName::Trash), "ghost", true, false).on_click(|_, _, cx| fact("tr-delete", "", "", cx)))
            .child(btn("tr-run", "Run now", Some(IconName::Play), "", true, false).on_click(|_, _, cx| fact("tr-run", "", "", cx))))
        .child(div().flex_1())
        .child(btn("tr-close", "Close", None, "ghost", true, false).on_click(|_, _, cx| fact("tr-close", "", "", cx)))
        .child(btn("tr-save", "Save", Some(IconName::Check), "primary", true, false).on_click(|_, _, cx| fact("tr-save", "", "", cx))).into_any_element());
    modal("fx-trig", T::FACTORIES_MODAL_WIDE, "tr-close", &title, body, window, cx)
}

/// A person's transcript (.fx-chatwrap): a sheet on the right, over the scrim.
fn chat_sheet(window: &mut Window, cx: &mut App) -> AnyElement {
    let name = fwith(|f| f.chat_name.clone());
    let (focus, _) = handles("fx-chat", cx);
    let vp = window.viewport_size();
    deferred(anchored().position(point(px(0.), px(0.))).child(
        div().id("fx-chat-scrim").occlude().w(vp.width).h(vp.height).bg(T::scrim()).flex().justify_end()
            .on_mouse_down(MouseButton::Left, |_, _, cx| fact("chat-close", "", "", cx))
            .child(div().id("fx-chat-sheet").track_focus(&focus).flex().flex_col().h_full().w(T::FACTORIES_SHEET_W.min(vp.width)).bg(T::bg()).border_l_1().border_color(T::border())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_key_down(|e, _, cx| if e.keystroke.key == "escape" { fact("chat-close", "", "", cx) })
                .child(div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_8).border_b_1().border_color(T::border())
                    .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_14).font_weight(T::W_BOLD).child(name))
                    .child(btn("chat-tab", "Open in a new tab", None, "ghost", true, false).on_click(|_, _, cx| fact("chat-tab", "", "", cx)))
                    .child(btn("chat-close", "Close", Some(IconName::X), "ghost", true, false).on_click(|_, _, cx| fact("chat-close", "", "", cx))))
                .child(super::view::transcript_body())),
    )).with_priority(1).into_any_element()
}
