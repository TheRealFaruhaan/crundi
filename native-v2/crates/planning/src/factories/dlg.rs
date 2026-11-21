//! Factories: the dialogs (crundi-factories.js): Start a factory (the change
//! request, its draft kept per factory, attachments uploaded at Start, the
//! project's precheck, missing skills and Start anyway), the PIN prompt for
//! the factory's secrets, a trigger (kanban, schedule, webhook with its
//! security, sample preview, URL, key or secret, history), and a person's
//! transcript (a sheet on the right). Drawn with the one modal look
//! (w::overlay_modal).
//!
//! Known gaps (as the Slint client): files cannot be dropped or pasted on
//! the change request (Attach picks them). The transcript is the person's
//! live chat (view.rs transcript_body).

use super::model::{self, arr, b, n, s};
use super::parts::{self, btn, field, note, prefs, FileChip, Line, Sec};
use super::{act as fact, edit_field, with as fwith, Fx};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, rc::Rc};

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
    /// The drafts: shown, not pickable (the web's disabled options).
    factory_off: Vec<bool>,
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

/// After a start: secrets unlocked with an old lock, offered a passphrase
/// (crundi-secret-lock.js upgrade()), one at a time.
#[derive(Default)]
struct Up { queue: Vec<(String, String, String, String)>, open: bool, a: String, b: String, err: String, busy: bool }

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
pub(crate) struct D { st: St, pin: Pin, up: Up, tr: Tr, pin_then: Option<PinThen>, handles: HashMap<&'static str, Handles> }

struct Handles { focus: FocusHandle, scroll: ScrollHandle }

fn with<R>(cx: &mut App, f: impl FnOnce(&mut D) -> R) -> R { super::with_fx(cx, |x| f(&mut x.d)) }
/// While drawing: the dialogs' state and the tab's, from the store being drawn.
fn rd<R>(fx: &mut Fx, f: impl FnOnce(&mut D) -> R) -> R { f(&mut fx.d) }
fn rf<R>(fx: &mut Fx, f: impl FnOnce(&mut super::F) -> R) -> R { f(&mut fx.f) }

const MAX_ATTACH: usize = 10;
const MAX_BYTES: u64 = 25 * 1024 * 1024;
const MERGE: [&str; 3] = ["", "ask", "auto"];
const BUSY: [&str; 3] = ["manager", "queue", "skip"];
const KINDS: [&str; 3] = ["kanban", "schedule", "webhook"];

/// One of these is open (the pointer goes to it only).
pub fn modal_open(cx: &mut App) -> bool { with(cx, |d| d.st.open || d.tr.open || d.pin.open || d.up.open) || fwith(cx, |f| f.chat_open) }

pub fn close_all(cx: &mut App) {
    with(cx, |d| *d = D::default());
    with(cx, |d| d.pin_then.take());
    fwith(cx, |f| { f.chat_open = false; f.chat = None; });
}

// ─── Start a factory ───

fn draft_key(fid: &str) -> String { format!("crundi.factoryCr.{fid}") }
fn read_draft(fid: &str) -> Option<Value> { serde_json::from_str(&prefs::get(&draft_key(fid))).ok().filter(|v: &Value| v.is_object()) }
fn save_draft(fid: &str, v: &Value) { if !fid.is_empty() { prefs::set(&draft_key(fid), &v.to_string()); } }
fn drop_draft(fid: &str) { prefs::set(&draft_key(fid), ""); }

pub fn open_start(o: StartOpts, cx: &mut App) {
    let projects: Vec<(String, String)> = crundi_shell::stores(cx).projects.read(cx).list.iter().map(|p| (p.alias.clone(), if p.name.is_empty() { p.alias.clone() } else { p.name.clone() })).collect();
    let project = if !o.project.is_empty() { o.project.clone() } else { let c = crundi_shell::project(cx); if !c.is_empty() { c } else { projects.first().map(|p| p.0.clone()).unwrap_or_default() } };
    let pi = projects.iter().position(|p| p.0.eq_ignore_ascii_case(&project)).unwrap_or(0);
    parts::pwith(cx, |p| parts::forget(p, "start|"));
    let gn = with(cx, |d| { let g2 = d.st.gn + 1; d.st = St { open: true, loading: true, then: o.then, projects: projects.clone(), project_i: pi, go_label: "Start".into(), gn: g2, focus: true, ..Default::default() }; g2 });
    let want = o.factory.clone();
    parts::get(cx, "/api/factories".into(), move |lib, cx| {
        if with(cx, |d| d.st.gn != gn || !d.st.open) { return; }
        let list = arr(&lib, "factories").to_vec();
        if list.is_empty() { with(cx, |d| { d.st.loading = false; d.st.empty = true; }); return; }
        let fi = list.iter().position(|f| s(f, "id") == want && !want.is_empty()).or_else(|| list.iter().position(|f| b(f, "runnable"))).unwrap_or(0);
        let labels: Vec<String> = list.iter().map(|f| format!("{}{}", s(f, "name"), if b(f, "runnable") { String::new() } else { format!(" (draft: {} to fix)", n(f, "errorCount") as i64) })).collect();
        with(cx, |d| { d.st.factories = list.clone(); d.st.factory_i = fi; d.st.factory_labels = labels; d.st.factory_off = list.iter().map(|f| !b(f, "runnable")).collect(); });
        render_sections(cx, None);
        with(cx, |d| { d.st.loading = false; d.st.empty = false; });
        precheck(cx);
    });
}

fn cur_factory(cx: &mut App) -> Option<Value> { with(cx, |d| d.st.factories.get(d.st.factory_i).cloned()) }
fn fid(cx: &mut App) -> String { cur_factory(cx).map(|f| s(&f, "id")).unwrap_or_default() }
fn values(cx: &mut App) -> Value { with(cx, |d| json!({ "title": d.st.title, "sections": d.st.sections })) }

/// renderSections(): the factory's CR template; values from `keep`, else its draft.
fn render_sections(cx: &mut App, keep: Option<Value>) {
    let f = cur_factory(cx).unwrap_or(Value::Null);
    let template = arr(&f, "crTemplate").to_vec();
    let draft = keep.clone().or_else(|| read_draft(&s(&f, "id"))).unwrap_or(json!({ "title": "", "sections": {} }));
    with(cx, |d| {
        if keep.is_none() { let t = s(&draft, "title"); if !t.is_empty() { d.st.title = t; } } else { d.st.title = s(&draft, "title").or_keep(&d.st.title); }
        let old = std::mem::take(&mut d.st.sections);
        for sec in &template {
            let id = s(sec, "id");
            let v = draft.get("sections").and_then(|x| x.get(&id)).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(String::from).or_else(|| old.get(&id).cloned()).unwrap_or_default();
            d.st.sections.insert(id, v);
        }
        d.st.template = template.clone();
    });
    refresh_rows(cx);
}

trait OrKeep { fn or_keep(self, old: &str) -> String; }
impl OrKeep for String { fn or_keep(self, old: &str) -> String { if self.is_empty() && !old.is_empty() { old.to_string() } else { self } } }

fn refresh_rows(cx: &mut App) {
    with(cx, |d| {
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
    let project = with(cx, |d| { d.st.info.clear(); d.st.go_enabled = false; d.st.projects.get(d.st.project_i).map(|p| p.0.clone()).unwrap_or_default() });
    let gn = with(cx, |d| d.st.gn);
    parts::get(cx, format!("/api/factory-runs/precheck?project={}", parts::enc(&project)), move |pre, cx| {
        if with(cx, |d| d.st.gn != gn) { return; }
        let pre = if pre.get("ok").is_none() && pre.get("active").is_none() { json!({ "ok": false, "error": "Could not check the project." }) } else { pre };
        let lines = model::precheck_lines(&pre);
        let ok = b(&pre, "ok") && !pre.get("active").map(|a| a.is_object()).unwrap_or(false);
        with(cx, |d| { d.st.info = lines.into_iter().map(|(k, t)| if k == "open" { ("open".to_string(), String::new(), t) } else { (k, t, String::new()) }).collect(); d.st.pre = Some(pre); d.st.go_enabled = ok && !d.st.factories.is_empty(); });
    });
}

fn schedule_draft(cx: &mut App) {
    let gn = with(cx, |d| { d.st.save_gen += 1; d.st.save_gen });
    parts::after(cx, 250, move |cx| { if with(cx, |d| d.st.save_gen == gn && d.st.open) { save_draft(&fid(cx), &values(cx)); } });
}

fn field_error(id: &str, msg: &str, cx: &mut App) {
    parts::toast_error(cx, msg);
    with(cx, |d| { if id == "title" { d.st.title_bad = true; d.st.focus = true; } else { d.st.bad = id.to_string(); } });
    refresh_rows(cx);
    let gn = with(cx, |d| d.st.gn);
    parts::after(cx, 2500, move |cx| { if with(cx, |d| d.st.gn == gn) { with(cx, |d| { d.st.title_bad = false; d.st.bad.clear(); }); refresh_rows(cx); } });
}

/// start(): check, ask the secrets' PINs, upload, start.
fn start(anyway: bool, cx: &mut App) {
    let (title, template, sections) = with(cx, |d| (d.st.title.clone(), d.st.template.clone(), d.st.sections.clone()));
    if title.trim().is_empty() { field_error("title", "Give the change request a title.", cx); return; }
    for sec in &template {
        if b(sec, "required") && sections.get(&s(sec, "id")).map(|v| v.trim().is_empty()).unwrap_or(true) { field_error(&s(sec, "id"), &format!("Fill in \"{}\".", s(sec, "label")), cx); return; }
    }
    let held = with(cx, |d| if anyway { d.st.held.clone() } else { None });
    if let Some(pins) = held { start2(anyway, Some(pins), cx); return; }
    let f = cur_factory(cx).unwrap_or(Value::Null);
    let ids: Vec<String> = arr(&f, "secrets").iter().filter_map(|x| x.as_str().map(String::from)).collect();
    if ids.is_empty() { start2(anyway, None, cx); return; }
    super::caps(cx, move |caps, cx| {
        if !caps.iter().any(|c| c == "factory-secrets") { start2(anyway, None, cx); return; }
        parts::get(cx, "/api/secrets".into(), move |d2, cx| {
            let known = arr(&d2, "secrets").to_vec();
            let list: Vec<Value> = ids.iter().filter_map(|id| known.iter().find(|x| s(x, "id") == *id)).map(|x| json!({ "id": s(x, "id"), "name": s(x, "name"), "lock": if s(x, "lock") == "passphrase" { "passphrase" } else { "pin" } })).collect();
            if list.is_empty() { start2(anyway, None, cx); return; }
            with(cx, |d| d.st.to_unlock = list.clone());
            ask_pins(cx, list, move |got, cx| {
                let Some(pins) = got else { return };
                with(cx, |d| d.st.held = Some(pins.clone()));
                start2(anyway, Some(pins), cx);
            });
        });
    });
}

fn start2(anyway: bool, pins: Option<Value>, cx: &mut App) {
    let (project, fid2, files, mm, title, sections) = with(cx, |d| (d.st.projects.get(d.st.project_i).map(|p| p.0.clone()).unwrap_or_default(), d.st.factories.get(d.st.factory_i).map(|f| s(f, "id")).unwrap_or_default(), d.st.files.clone(), MERGE[d.st.merge_i.min(2)].to_string(), d.st.title.clone(), d.st.sections.clone()));
    with(cx, |d| { d.st.go_enabled = false; d.st.go_label = if files.is_empty() { "Starting…".into() } else { format!("Uploading 1/{}…", files.len()) }; });
    let gn = with(cx, |d| d.st.gn);
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
        super::bump(cx);
        if with(cx, |d| d.st.gn != gn) { return; }
        with(cx, |d| { d.st.go_label = "Start".into(); d.st.go_enabled = true; });
        if !s(&r, "upload").is_empty() { parts::toast_error(cx, &format!("Could not upload {}: {}", s(&r, "upload"), s(&r, "error"))); return; }
        if b(&r, "ok") || s(&r, "field") == "secrets" { with(cx, |d| d.st.held = None); }
        if b(&r, "ok") {
            let (fid3, then) = with(cx, |d| (d.st.factories.get(d.st.factory_i).map(|f| s(f, "id")).unwrap_or_default(), d.st.then));
            drop_draft(&fid3);
            close_start(cx);
            parts::toast_ok(cx, "Factory started.");
            let run_id = s(&r["run"], "id");
            match then { Then::Viewer => super::open_sub("viewer", &run_id, cx), Then::Panel => super::panel_load(cx), Then::None => {} }
            // Secrets unlocked with an old lock: offer to set a passphrase now,
            // with what was just typed (the run keeps working: the server moves
            // its kept lock to the new one).
            if let (Some(p), Some(ids)) = (pins.as_ref(), r.get("upgradable").and_then(|x| x.as_array())) {
                let known = with(cx, |d| d.st.to_unlock.clone());
                let q: Vec<(String, String, String, String)> = ids.iter().filter_map(|x| x.as_str()).filter_map(|id| {
                    let pin = s(p, id);
                    if pin.is_empty() { return None; }
                    let x = known.iter().find(|k| s(k, "id") == id);
                    Some((id.to_string(), x.map(|k| s(k, "name")).unwrap_or_default(), x.map(|k| s(k, "lock")).unwrap_or_default(), pin))
                }).collect();
                if !q.is_empty() { upgrade(q, cx); }
            }
            return;
        }
        if s(&r, "field") == "secrets" { with(cx, |d| d.st.info = vec![("err".into(), format!("{} Press Start to type it again.", parts::err_text(&r, "A secret could not be unlocked.")), String::new())]); return; }
        let field = s(&r, "field");
        if !field.is_empty() && field != "attachments" { field_error(&field, &s(&r, "error"), cx); return; }
        if let Some(ms) = r.get("missingSkills").and_then(|x| x.as_array()) {
            let names: Vec<String> = ms.iter().filter_map(|x| x.as_str().map(String::from)).collect();
            with(cx, |d| d.st.info = vec![("warn".into(), model::missing_skills_text(&names), String::new()), ("anyway".into(), String::new(), String::new())]);
            return;
        }
        with(cx, |d| {
            d.st.info = vec![("err".into(), parts::err_text(&r, "It could not start."), String::new())];
            for e in arr(&r, "errors").iter().take(5) { d.st.info.push(("note".into(), format!("· {}", s(e, "message")), String::new())); }
        });
    });
}

fn close_start(cx: &mut App) { with(cx, |d| { d.st.open = false; d.st.gn += 1; }); }

fn attach(cx: &mut App) {
    crundi_shell::run(cx, || crundi_core::driver::take_pick().map(|p| vec![p]).or_else(|| rfd::FileDialog::new().pick_files()).unwrap_or_default(), add_files);
}

/// Files for the change request (Attach, a drop, a paste): the web's addFiles.
fn add_files(paths: Vec<PathBuf>, cx: &mut App) {
    for p in paths {
        if with(cx, |d| d.st.files.len() >= MAX_ATTACH) { parts::toast_error(cx, &format!("Up to {MAX_ATTACH} attachments.")); break; }
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        if size > MAX_BYTES { parts::toast_error(cx, &format!("{} is over 25 MB.", p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default())); continue; }
        with(cx, |d| d.st.files.push(p));
    }
    super::bump(cx);
}

/// A paste in the change request: copied files, or a picture (saved as
/// pasted-<time>-<n>.png, as the web names it), become attachments; text is
/// the box's own paste.
fn paste_files(item: &ClipboardItem, cx: &mut App) -> bool {
    let mut paths: Vec<PathBuf> = vec![];
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    for (i, e) in item.entries().iter().enumerate() {
        match e {
            ClipboardEntry::ExternalPaths(p) => paths.extend(p.paths().iter().cloned()),
            ClipboardEntry::Image(img) => {
                let dir = std::env::temp_dir().join("crundi-factory-paste");
                let ext = match img.format { ImageFormat::Png => "png".to_string(), f => format!("{f:?}").to_lowercase() };
                let f = dir.join(format!("pasted-{stamp}-{i}.{ext}"));
                if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&f, &img.bytes)).is_ok() { paths.push(f); }
            }
            _ => {}
        }
    }
    if paths.is_empty() { return false; }
    add_files(paths, cx);
    true
}

// ─── The PIN prompt ───

fn ask_pins(cx: &mut App, list: Vec<Value>, then: impl FnOnce(Option<Value>, &mut App) + 'static) {
    with(cx, |d| d.pin_then = Some(Box::new(then)));
    with(cx, |d| d.pin = Pin { list, ..Default::default() });
    show_pin_quiet(cx);
}

/// The next secret's prompt; Some(pins) when every one was asked.
fn show_pin_quiet(cx: &mut App) -> Option<Value> {
    let (done, x, i, total) = with(cx, |d| (d.pin.i >= d.pin.list.len(), d.pin.list.get(d.pin.i).cloned().unwrap_or(Value::Null), d.pin.i, d.pin.list.len()));
    if done {
        let pins = with(cx, |d| { let p = std::mem::take(&mut d.pin.pins); d.pin = Pin::default(); p });
        return Some(Value::Object(pins));
    }
    let pass = s(&x, "lock") == "passphrase";
    parts::pwith(cx, |p| parts::forget(p, "pin|"));
    with(cx, |d| { let p = &mut d.pin; p.value.clear(); p.err.clear(); p.open = true; p.name = s(&x, "name"); p.step = if total > 1 { format!("Secret {} of {total}", i + 1) } else { String::new() }; p.sub = model::pin_sub(pass); p.pass = pass; p.busy = false; });
    None
}
fn show_pin(cx: &mut App) { if let Some(p) = show_pin_quiet(cx) { pin_done(Some(p), cx); } }

fn pin_done(v: Option<Value>, cx: &mut App) {
    with(cx, |d| d.pin = Pin::default());
    let then = with(cx, |d| d.pin_then.take());
    if let Some(t) = then { t(v, cx); }
}

fn pin_unlock(cx: &mut App) {
    let (x, value) = with(cx, |d| (d.pin.list.get(d.pin.i).cloned().unwrap_or(Value::Null), d.pin.value.clone()));
    let pass = s(&x, "lock") == "passphrase";
    let pin = if pass { value } else { value.trim().to_string() };
    let ok = if pass { !pin.is_empty() && pin.chars().count() <= 1024 } else { pin.len() == 6 && pin.chars().all(|c| c.is_ascii_digit()) };
    if !ok { with(cx, |d| d.pin.err = if pass { "Enter the passphrase" } else { "Enter a 6-digit PIN" }.into()); return; }
    with(cx, |d| d.pin.busy = true);
    let id = s(&x, "id");
    parts::post(cx, "/api/secrets", json!({ "action": "check", "id": id, "pin": pin }), move |r, cx| {
        if !b(&r, "ok") { let e = parts::err_text(&r, "Failed"); with(cx, |d| { d.pin.busy = false; d.pin.err = e; }); return; }
        with(cx, |d| { d.pin.pins.insert(id, json!(pin)); d.pin.i += 1; });
        show_pin(cx);
    });
}

// ─── A secret's new lock (crundi-secret-lock.js upgrade) ───

const RULE: &str = "At least 8 characters, and not only digits";
fn passphrase_ok(t: &str) -> bool { t.chars().count() >= 8 && t.chars().any(|c| !c.is_ascii_digit()) }

/// Asked only when the server lists 'secret-upgrade'.
fn upgrade(q: Vec<(String, String, String, String)>, cx: &mut App) {
    super::caps(cx, move |caps, cx| {
        if !caps.iter().any(|c| c == "secret-upgrade") { return; }
        let show = with(cx, |d| { let idle = !d.up.open && d.up.queue.is_empty(); d.up.queue.extend(q); idle });
        if show { next_upgrade(cx); }
    });
}

fn next_upgrade(cx: &mut App) {
    parts::pwith(cx, |p| parts::forget(p, "up|"));
    with(cx, |d| { let open = !d.up.queue.is_empty(); d.up = Up { queue: std::mem::take(&mut d.up.queue), open, ..Default::default() }; });
}

/// What is wrong with the new passphrase so far ("" nothing yet).
fn up_why(a: &str, b: &str) -> String {
    if a.is_empty() { return String::new(); }
    if !passphrase_ok(a) { return format!("{RULE}."); }
    if !b.is_empty() && b != a { return "The two do not match.".into(); }
    String::new()
}

fn up_go(cx: &mut App) {
    let job = with(cx, |d| {
        let u = &d.up;
        if u.busy || !passphrase_ok(&u.a) || u.a != u.b { return None; }
        u.queue.first().map(|x| (x.0.clone(), x.3.clone(), u.a.clone()))
    });
    let Some((id, old, np)) = job else { return };
    with(cx, |d| { d.up.busy = true; d.up.err.clear(); });
    parts::post(cx, "/api/secrets", json!({ "action": "edit", "id": id, "pin": old, "newPin": np }), |r, cx| {
        if !with(cx, |d| d.up.open) { return; }
        if b(&r, "ok") {
            parts::toast_ok(cx, "Lock updated");
            with(cx, |d| { if !d.up.queue.is_empty() { d.up.queue.remove(0); } });
            next_upgrade(cx);
            return;
        }
        let e = { let x = s(&r, "error"); if x.is_empty() { "Could not update it.".to_string() } else if x.contains("Unknown secrets action") { "This server does not support this yet. Update the server to use it.".into() } else { x } };
        with(cx, |d| { d.up.busy = false; d.up.err = e; });
    });
}

fn up_later(cx: &mut App) {
    with(cx, |d| { if !d.up.queue.is_empty() { d.up.queue.remove(0); } });
    next_upgrade(cx);
}

// ─── A trigger ───

pub fn open_trigger(project: &str, id: &str, cx: &mut App) {
    let saved = fwith(cx, |s2| s2.panel.triggers.iter().find(|t| s(t, "id") == id && !id.is_empty()).cloned());
    parts::pwith(cx, |p| parts::forget(p, "trig|"));
    let gn = with(cx, |d| { let g2 = d.tr.gn + 1; d.tr = Tr { open: true, loading: true, title: if saved.is_some() { "Trigger".into() } else { "New trigger".into() }, project: project.to_string(), saved: saved.clone(), gn: g2, ..Default::default() }; g2 });
    parts::get(cx, "/api/factories".into(), move |lib, cx| {
        if with(cx, |d| d.tr.gn != gn) { return; }
        let saved = with(cx, |d| d.tr.saved.clone());
        let list: Vec<Value> = arr(&lib, "factories").iter().filter(|f| b(f, "runnable") || saved.as_ref().map(|t| s(t, "factoryId") == s(f, "id")).unwrap_or(false)).cloned().collect();
        if list.is_empty() { with(cx, |d| { d.tr.loading = false; d.tr.empty = true; }); return; }
        let cur = saved.clone().unwrap_or_else(|| json!({ "name": "", "enabled": true, "kind": "kanban", "factoryId": s(&list[0], "id"), "schedule": { "at": "09:00", "days": [] }, "kanban": { "status": "todo" }, "cr": { "title": "", "sections": {} }, "mergeMode": "", "ifBusy": "queue" }));
        with(cx, |d| { d.tr.factories = list.clone(); d.tr.cur = cur; });
        render_kind(cx, true);
        render_hook(saved.as_ref(), cx);
        render_history(cx, saved.as_ref());
        with(cx, |d| d.tr.loading = false);
    });
}

fn tr_get(cx: &mut App, k: &str) -> String { with(cx, |d| s(&d.tr.cur, k)) }

/// renderKind(): a new webhook tells the Manager; then the CR.
fn render_kind(cx: &mut App, first: bool) {
    let new = with(cx, |d| d.tr.saved.is_none());
    if !first && tr_get(cx, "kind") == "webhook" && new && tr_get(cx, "ifBusy") == "queue" { with(cx, |d| d.tr.cur["ifBusy"] = json!("manager")); }
    render_cr(cx);
}

/// renderCr(): the change request box, keeping what was typed by section.
fn render_cr(cx: &mut App) {
    let (cur, factories, saved_none) = with(cx, |d| (d.tr.cur.clone(), d.tr.factories.clone(), d.tr.saved.is_none()));
    let kind = s(&cur, "kind");
    let f = factories.iter().find(|x| s(x, "id") == s(&cur, "factoryId")).cloned().unwrap_or(Value::Null);
    let tpl = arr(&f, "crTemplate").to_vec();
    let mut keep = with(cx, |d| d.tr.sections.clone());
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
    with(cx, |d| { d.tr.sections = sections; d.tr.cur["cr"]["title"] = json!(title); d.tr.rows = rows; });
}

fn render_hook(tr: Option<&Value>, cx: &mut App) {
    let Some(tr) = tr.filter(|t| s(t, "kind") == "webhook") else { with(cx, |d| d.tr.hook = None); return };
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
    with(cx, |d| d.tr.hook = Some(h));
}

fn render_history(cx: &mut App, tr: Option<&Value>) {
    let now = model::now_ms();
    let lines: Vec<String> = tr.map(|t| arr(t, "history").iter().map(|h| model::trigger_history_line(h, now)).collect()).unwrap_or_default();
    with(cx, |d| d.tr.history = lines);
}

fn collect(cx: &mut App) -> Value {
    with(cx, |d| {
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
    let sample = tr_get(cx, "_sample");
    let set_preview = |cx: &mut App, lines: Vec<(String, String)>| with(cx, |d| d.tr.preview = lines.into_iter().map(|(k, t)| Line { kind: k, text: t, id: String::new() }).collect());
    let body: Value = if sample.trim().is_empty() { json!({}) } else {
        match serde_json::from_str(&sample) { Ok(v) => v, Err(e) => { set_preview(cx, vec![("note".into(), format!("That is not valid JSON: {e}"))]); return; } }
    };
    let idem_typed = !with(cx, |d| s(&d.tr.cur["webhook"], "idempotency")).trim().is_empty();
    let trigger = collect(cx);
    parts::post(cx, "/api/factory-triggers", json!({ "action": "preview", "trigger": trigger, "sample": { "body": body } }), move |r, cx| {
        if !b(&r, "ok") || !r["cr"].is_object() { set_preview(cx, vec![("note".into(), parts::err_text(&r, "No preview."))]); return; }
        let c = &r["cr"];
        let mut lines = vec![("b".to_string(), s(c, "title"))];
        if let Some(o) = c["sections"].as_object() { for (k, v) in o { lines.push(("meta".into(), format!("{k}: {}", v.as_str().map(String::from).unwrap_or_else(|| v.to_string())))); } }
        let idem = s(c, "idem");
        lines.push(("meta".into(), format!("Idempotency key: {}", if idem.is_empty() { format!("none in this call{}", if idem_typed { ", so repeats cannot be told apart" } else { "" }) } else { idem })));
        let miss: Vec<String> = arr(c, "missing").iter().chain(arr(c, "idemMissing").iter()).filter_map(|x| x.as_str()).map(|m| format!("{{{{{m}}}}}")).collect();
        if !miss.is_empty() { lines.push(("meta".into(), format!("Not in this payload (left empty, never an error): {}", miss.join(" ")))); }
        lines.push(("meta".into(), "The Manager also gets the whole request (headers, query, body) as a file.".into()));
        set_preview(cx, lines);
    });
}

fn close_trigger(cx: &mut App) { with(cx, |d| { d.tr.open = false; d.tr.gn += 1; }); }

fn save_trigger(cx: &mut App) {
    let trigger = collect(cx);
    parts::post(cx, "/api/factory-triggers", json!({ "action": "save", "trigger": trigger }), |r, cx| {
        if !b(&r, "ok") { parts::toast_error(cx, &parts::err_text(&r, "Could not save.")); return; }
        let t = r["trigger"].clone();
        with(cx, |d| d.tr.saved = Some(t.clone()));
        parts::toast_ok(cx, "Saved.");
        super::panel_load(cx);
        if s(&t, "kind") == "webhook" { render_hook(Some(&t), cx); with(cx, |d| d.tr.title = "Trigger".into()); } else { close_trigger(cx); }
    });
}

// ─── The actions and fields ───

pub fn act(a: &str, x: &str, _y: &str, cx: &mut App) {
    match a {
        "st-close" => close_start(cx),
        "st-go" => start(x == "anyway", cx),
        "st-attach" => attach(cx),
        "st-unattach" => { let i: usize = x.parse().unwrap_or(usize::MAX); with(cx, |d| if i < d.st.files.len() { d.st.files.remove(i); }); }
        "st-clear" => {
            let typed = with(cx, |d| !d.st.title.is_empty() || d.st.sections.values().any(|v| !v.is_empty()));
            let go = |cx: &mut App| {
                with(cx, |d| { d.st.title.clear(); for v in d.st.sections.values_mut() { v.clear(); } d.st.files.clear(); });
                drop_draft(&fid(cx));
                render_sections(cx, Some(json!({ "title": "", "sections": {} })));
                with(cx, |d| d.st.title.clear());
            };
            if typed { parts::confirm(cx, "Clear form", "Clear the change request?", "Clear", false, move |ok, cx| if ok { go(cx) }); } else { go(cx); }
        }
        "st-open-active" => { close_start(cx); super::open_run_in_tab(x, cx); }
        "pin-cancel" => pin_done(None, cx),
        "pin-up-later" => up_later(cx),
        "pin-up-go" => up_go(cx),
        "pin-skip" => { with(cx, |d| d.pin.i += 1); show_pin(cx); }
        "pin-unlock" => pin_unlock(cx),
        "tr-close" => close_trigger(cx),
        "tr-save" => save_trigger(cx),
        "tr-preview" => preview(cx),
        "tr-sample" => with(cx, |d| d.tr.sample_open = !d.tr.sample_open),
        "tr-history" => with(cx, |d| d.tr.history_open = !d.tr.history_open),
        "tr-delete" => {
            let Some(id) = with(cx, |d| d.tr.saved.as_ref().map(|t| s(t, "id"))) else { return };
            parts::confirm(cx, "Delete", "Delete this trigger?", "Delete", true, move |ok, cx| {
                if !ok { return; }
                parts::post(cx, "/api/factory-triggers", json!({ "action": "delete", "id": id }), |r, cx| { if b(&r, "ok") { close_trigger(cx); super::panel_load(cx); } else { parts::toast_error(cx, &parts::err_text(&r, "That did not work.")); } });
            });
        }
        "tr-run" => {
            let Some(id) = with(cx, |d| d.tr.saved.as_ref().map(|t| s(t, "id"))) else { return };
            parts::post(cx, "/api/factory-triggers", json!({ "action": "run", "id": id }), |r, cx| {
                if b(&r, "ok") { parts::toast_ok(cx, if b(&r, "queued") { "Queued: a run is going in this project." } else { "Started." }); } else { parts::toast_error(cx, &parts::err_text(&r, "It could not start.")); }
                super::panel_load(cx);
            });
        }
        "tr-rotate" => {
            let Some(t) = with(cx, |d| d.tr.saved.clone()) else { return };
            let signed = s(&t["webhook"], "auth") == "signature";
            let msg = if signed { "Calls signed with the old secret stop working. Make a new one?" } else { "The old URL stops working. Make a new key?" };
            parts::confirm(cx, if signed { "New secret" } else { "New key" }, msg, "OK", false, move |ok, cx| {
                if !ok { return; }
                parts::post(cx, "/api/factory-triggers", json!({ "action": "rotate", "id": s(&t, "id"), "which": if signed { "secret" } else { "key" } }), |r, cx| {
                    if b(&r, "ok") { let t2 = r["trigger"].clone(); with(cx, |d| d.tr.saved = Some(t2.clone())); render_hook(Some(&t2), cx); }
                });
            });
        }
        "chat-close" => fwith(cx, |s2| { s2.chat = None; s2.chat_open = false; }),
        "chat-tab" => { if let Some((id, _)) = fwith(cx, |s2| s2.chat.clone()) { let _ = crundi_shell::session::open_url(&format!("{}/chat/{}", parts::base(cx), parts::enc(&id))); } }
        _ => {}
    }
}

pub fn edit(form: &str, k: &str, v: &str, cx: &mut App) {
    match form {
        "start" => match k {
            "project" => { let i: usize = v.parse().unwrap_or(0); with(cx, |d| d.st.project_i = i); precheck(cx); }
            "factory" => {
                let i: usize = v.parse().unwrap_or(0);
                // A draft factory cannot be picked (a disabled option).
                if !with(cx, |d| d.st.factories.get(i).map(|f| b(f, "runnable")).unwrap_or(false)) { return; }
                let last = fid(cx);
                if !last.is_empty() { save_draft(&last, &values(cx)); }
                with(cx, |d| d.st.factory_i = i);
                let mine = read_draft(&fid(cx));
                let keep = if mine.is_some() { None } else { Some(values(cx)) };
                render_sections(cx, keep);
                let ok = with(cx, |d| d.st.pre.as_ref().map(|p| b(p, "ok") && !p.get("active").map(|a| a.is_object()).unwrap_or(false)).unwrap_or(false));
                with(cx, |d| d.st.go_enabled = ok);
            }
            "merge" => with(cx, |d| d.st.merge_i = v.parse().unwrap_or(0)),
            "title" => { with(cx, |d| d.st.title = v.chars().take(120).collect()); schedule_draft(cx); }
            x if x.starts_with("sec:") => { with(cx, |d| { d.st.sections.insert(x[4..].to_string(), v.to_string()); }); refresh_rows(cx); schedule_draft(cx); }
            _ => {}
        },
        "pin" if k == "value" => with(cx, |d| d.pin.value = v.to_string()),
        "pin" if k == "up-a" => with(cx, |d| d.up.a = v.to_string()),
        "pin" if k == "up-b" => with(cx, |d| d.up.b = v.to_string()),
        "trig" => {
            let i: usize = v.parse().unwrap_or(0);
            let mut rerender = false;
            with(cx, |d| {
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
            if rerender { if k == "kind" { render_kind(cx, false); } else { render_cr(cx); } }
        }
        _ => {}
    }
}

pub fn props(cx: &mut App) -> Value {
    with(cx, |d| {
        let up_hint = { let w2 = up_why(&d.up.a, &d.up.b); if w2.is_empty() { format!("{RULE}.") } else { w2 } };
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
            "upgrade": { "open": d.up.open, "name": d.up.queue.first().map(|x| x.1.clone()).unwrap_or_default(), "err": d.up.err, "hint": up_hint, "ready": passphrase_ok(&d.up.a) && d.up.a == d.up.b },
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

fn handles(fx: &mut Fx, id: &'static str, cx: &mut App) -> (FocusHandle, ScrollHandle) {
    let x = fx.d.handles.entry(id).or_insert_with(|| Handles { focus: cx.focus_handle(), scroll: ScrollHandle::new() }); (x.focus.clone(), x.scroll.clone())
}

fn modal(id: &'static str, width: Pixels, close: &'static str, title: &str, body: Vec<AnyElement>, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let (focus, scroll) = handles(fx, id, cx);
    let m = w::Modal {
        id, width, focus, scroll,
        on_close: Rc::new(move |_, cx| fact(close, "", "", cx)),
        on_key: Rc::new(move |e: &KeyDownEvent, _, cx| if e.keystroke.key == "escape" { fact(close, "", "", cx) }),
    };
    // A dialog with no title (the PIN box, the lock's update) draws its own head.
    let mut kids = if title.is_empty() { vec![] } else { vec![w::modal_head(match close { "st-close" => "st-x", "tr-close" => "tr-x", "chat-close" => "chat-x", _ => "pin-x" }, title.to_string(), false, move |_, _, cx| fact(close, "", "", cx)).into_any_element()] };
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
pub fn render(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Vec<AnyElement> {
    let mut out = vec![];
    let (st, tr, pin, up) = rd(fx, |d| (d.st.open, d.tr.open, d.pin.open, d.up.open));
    let in_graph = rf(fx, |f| f.chat_open);
    if in_graph { out.push(chat_sheet(fx, window, cx)); }
    if tr { out.push(trigger_dialog(fx, window, cx)); }
    if st { out.push(start_dialog(fx, window, cx)); }
    if pin { out.push(pin_dialog(fx, window, cx)); }
    if up { out.push(upgrade_dialog(fx, window, cx)); }
    out
}

fn start_dialog(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let off = rd(fx, |d| d.st.factory_off.clone());
    let d = rd(fx, |d| (d.st.loading, d.st.empty, d.st.projects.iter().map(|p| p.1.clone()).collect::<Vec<_>>(), d.st.factory_labels.clone(), d.st.project_i, d.st.factory_i, d.st.merge_i,
        d.st.title.clone(), d.st.title_bad, d.st.rows.clone(), d.st.files.clone(), d.st.info.clone(), d.st.go_label.clone(), d.st.go_enabled, std::mem::take(&mut d.st.focus)));
    let (loading, empty, projects, factories, pi, fi, mi, title, title_bad, rows, files, info, go_label, go_enabled, focus) = d;
    let stack = window.viewport_size().width < T::FACTORIES_PAIR_STACK;
    let mut body: Vec<AnyElement> = vec![];
    if loading { body.push(note("note", "Loading…").into_any_element()); }
    else if empty {
        body.push(note("note", "There are no factories yet. Create one in the Factories tab first.").into_any_element());
        body.push(div().flex().justify_end().child(btn("st-close-e", "Close", None, "ghost", true, false).on_click(|_, _, cx| fact("st-close", "", "", cx))).into_any_element());
    } else {
        body.push(pair(stack, field("Project", "", "", sel("st-project", "start", "project", projects, pi)), field("Factory", "", "", w::dropdown_off("st-factory", factories.into_iter().map(SharedString::from).collect(), &off, fi, false, true, |i, _, cx| set("start", "factory", &i.to_string(), cx)))).into_any_element());
        // .fx-cr: the change request.
        // A paste in any of its fields: files and pictures attach.
        parts::on_paste(&mut fx.p, "start|", |item, _, cx| paste_files(item, cx));
        let title_box = parts::line(&mut fx.p, "start|title", &title, "One line: what should change", false, false, parts::Commit::Change, window, cx, |v, cx| set("start", "title", &v, cx));
        if focus { parts::focus(&mut fx.p, "start|title", window, cx); }
        let mut cr = div().flex().flex_col().gap(T::SP_10).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg())
            .child(div().flex().flex_col().gap(T::SP_2)
                .child(div().text_size(T::TEXT_13).font_weight(T::W_BOLD).child("Change request"))
                .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("The Manager gets all of it; every step gets the title, goal and the checks for done, and can open the rest.")))
            .child(field("Title", "required", "", div().rounded(T::R_CONTROL).when(title_bad, |d| d.border_2().border_color(T::red())).child(title_box)));
        for sec in &rows {
            let k = format!("sec:{}", sec.id);
            let ta = parts::area(&mut fx.p, &format!("start|{k}"), &sec.value, &sec.hint, sec.rows as usize, false, window, cx, move |v, cx| set("start", &k, &v, cx));
            cr = cr.child(field(&sec.label, if sec.required { "required" } else { "" }, "", div().rounded(T::R_CONTROL).when(sec.bad, |d| d.border_2().border_color(T::red())).child(ta)));
        }
        let mut att = div().flex().flex_wrap().items_center().gap(T::SP_6).child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Attachments"));
        for (i, f) in files.iter().enumerate() {
            let nm = f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            let chip = FileChip { image: [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg"].iter().any(|e| nm.to_lowercase().ends_with(e)), name: nm };
            let is = i.to_string();
            // .fx-chip: the file's icon, its name, its ×.
            att = att.child(w::chip(if chip.image { IconName::Image } else { IconName::File }, chip.name.clone(), false)
                .child(w::icon_button(("st-unattach", i), IconName::X, "Remove").on_click(move |_, _, cx| fact("st-unattach", &is, "", cx))));
        }
        att = att.child(btn("st-attach", "Attach", Some(IconName::Paperclip), "", true, false).on_click(|_, _, cx| fact("st-attach", "", "", cx)));
        cr = cr.child(div().flex().flex_col().gap(T::SP_6).child(att).child(div().text_size(T::TEXT_11).text_color(T::text_secondary()).child("Screenshots, mockups, specs, logs. Drop or paste them here too.")))
            // Files dropped on the change request attach (.fx-cr.drop: the
            // accent border while they are over it).
            .drag_over::<ExternalPaths>(|st, _, _, _| st.border_color(T::accent()).bg(T::bg_hover()))
            .on_drop(|p: &ExternalPaths, _, cx| { add_files(p.paths().to_vec(), cx); cx.stop_propagation(); });
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
    modal("fx-start", T::FACTORIES_MODAL_WIDE, "st-close", "Start a factory", body, fx, window, cx)
}

fn pin_dialog(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let (name, step, subt, pass, value, err, busy) = rd(fx, |d| (d.pin.name.clone(), d.pin.step.clone(), d.pin.sub.clone(), d.pin.pass, d.pin.value.clone(), d.pin.err.clone(), d.pin.busy));
    let input = parts::line(&mut fx.p, "pin|value", &value, if pass { "Passphrase" } else { "••••••" }, false, true, parts::Commit::Change, window, cx, |v, cx| set("pin", "value", &v, cx));
    parts::on_enter(&mut fx.p, "pin|value", |_, cx| fact("pin-unlock", "", "", cx));
    parts::focus(&mut fx.p, "pin|value", window, cx);
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
    // The web's .fx-pin box: no title; its head is the lock and the secret's name.
    modal("fx-pin", T::FACTORIES_PIN_W, "pin-cancel", "", body, fx, window, cx)
}

/// "Update this secret’s lock" (crundi-secret-lock.js drawUpgrade).
fn upgrade_dialog(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let (name, lock, a, b2, err, busy) = rd(fx, |d| { let x = d.up.queue.first().cloned().unwrap_or_default(); (x.1, x.2, d.up.a.clone(), d.up.b.clone(), d.up.err.clone(), d.up.busy) });
    let why = up_why(&a, &b2);
    let ready = passphrase_ok(&a) && a == b2;
    let first = parts::line(&mut fx.p, "up|a", &a, "New passphrase", false, true, parts::Commit::Change, window, cx, |v, cx| set("pin", "up-a", &v, cx));
    let second = parts::line(&mut fx.p, "up|b", &b2, "Confirm the passphrase", false, true, parts::Commit::Change, window, cx, |v, cx| set("pin", "up-b", &v, cx));
    let body = vec![
        div().flex().items_center().gap(T::SP_8).child(w::icon(IconName::Lock, T::ICON_LG)).child(div().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).child("Update this secret\u{2019}s lock")).into_any_element(),
        div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).child(if name.is_empty() { "This secret".to_string() } else { name }).into_any_element(),
        div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(if lock != "passphrase" { "This secret uses an old 6-digit PIN. Set a new passphrase (8+ characters)." } else { "This secret uses an older, weaker lock. Set a passphrase (8+ characters) to seal it again; it may be the same one." }).into_any_element(),
        first, second,
        div().text_size(T::TEXT_11).text_color(if why.is_empty() { T::text_muted() } else { T::yellow() }).child(if why.is_empty() { format!("{RULE}.") } else { why }).into_any_element(),
        div().min_h(T::SP_16).text_size(T::TEXT_12).text_color(T::red()).child(err).into_any_element(),
        div().flex().justify_end().gap(T::SP_8)
            .child(btn("pin-up-later", "Not now", None, "", !busy, false).on_click(|_, _, cx| fact("pin-up-later", "", "", cx)))
            .child(btn("pin-up-go", if busy { "Updating…" } else { "Update" }, None, "primary", ready && !busy, false).on_click(|_, _, cx| fact("pin-up-go", "", "", cx))).into_any_element(),
    ];
    modal("fx-upgrade", T::FACTORIES_PIN_W, "pin-up-later", "", body, fx, window, cx)
}

fn trigger_dialog(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let (loading, empty, title, cur, factories, rows, preview, history, sample_open, history_open, hook, editing) = rd(fx, |d| { let t = &d.tr;
        (t.loading, t.empty, t.title.clone(), t.cur.clone(), t.factories.clone(), t.rows.clone(), t.preview.clone(), t.history.clone(), t.sample_open, t.history_open, t.hook.clone(), t.saved.is_some()) });
    let stack = window.viewport_size().width < T::FACTORIES_PAIR_STACK;
    let mut body: Vec<AnyElement> = vec![];
    if loading { body.push(note("note", "Loading…").into_any_element()); return modal("fx-trig", T::FACTORIES_MODAL_WIDE, "tr-close", &title, body, fx, window, cx); }
    if empty {
        body.push(note("note", "There are no runnable factories yet. Create one in the Factories tab first.").into_any_element());
        body.push(div().child(btn("tr-close-e", "Close", None, "ghost", true, false).on_click(|_, _, cx| fact("tr-close", "", "", cx))).into_any_element());
        return modal("fx-trig", T::FACTORIES_MODAL_WIDE, "tr-close", &title, body, fx, window, cx);
    }
    let kind = KINDS.iter().position(|k| *k == s(&cur, "kind")).unwrap_or(0);
    let tline = |k: &'static str, v: &str, ph: &str, mono: bool, fx: &mut Fx, window: &mut Window, cx: &mut App| parts::line(&mut fx.p, &format!("trig|{k}"), v, ph, mono, false, parts::Commit::Change, window, cx, move |t, cx| set("trig", k, &t, cx));
    let fi = factories.iter().position(|x| s(x, "id") == s(&cur, "factoryId")).unwrap_or(0);
    body.push(pair(stack, field("Name", "", "", tline("name", &s(&cur, "name"), "Cards to the team", false, fx, window, cx)),
        field("Factory", "", "", sel("tr-factory", "trig", "factory", factories.iter().map(|f| s(f, "name")).collect(), fi))).into_any_element());
    body.push(field("Start when", "", "", sel("tr-kind", "trig", "kind", vec!["A kanban card enters a column".into(), "A time of day".into(), "A webhook is called".into()], kind)).into_any_element());
    if kind == 1 {
        body.push(field("At (this server's time)", "", "", tline("at", &{ let a = s(&cur["schedule"], "at"); if a.is_empty() { "09:00".into() } else { a } }, "09:00", false, fx, window, cx)).into_any_element());
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
        .child(field("Title", "required", "", tline("title", &s(&cur["cr"], "title"), "", false, fx, window, cx)));
    for sec in &rows {
        let k = format!("sec:{}", sec.id);
        cr = cr.child(field(&sec.label, "", "", parts::area(&mut fx.p, &format!("trig|{k}"), &sec.value, &sec.hint, sec.rows as usize, false, window, cx, move |v, cx| set("trig", &k, &v, cx))));
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
            c = c.child(pair(stack, field("Signature header", "", "", tline("sig-h", &{ let x = s(wh, "signatureHeader"); if x.is_empty() { "x-crundi-signature".into() } else { x } }, "x-crundi-signature", true, fx, window, cx)),
                field("Timestamp header (optional)", "", "", tline("ts-h", &s(wh, "timestampHeader"), "None (Grafana: X-Grafana-Alerting-Signature-Timestamp)", true, fx, window, cx))));
        }
        c = c.child(field("Idempotency key", "a call with a key already seen is not processed again", "", tline("idem", &s(wh, "idempotency"), "{{payload.messageId}}", true, fx, window, cx)))
            .child(w::disclosure("tr-sample", "Try it with a sample payload", sample_open).on_click(|_, _, cx| fact("tr-sample", "", "", cx)));
        if sample_open {
            let mut sp = div().flex().flex_col().gap(T::SP_8)
                .child(parts::area(&mut fx.p, "trig|sample", &s(&cur, "_sample"), "{\"messageId\": \"a1\", \"message\": \"CPU above 90% on web-1\"}", 4, true, window, cx, |v, cx| set("trig", "sample", &v, cx)))
                .child(div().child(btn("tr-preview", "Preview", None, "ghost", true, false).on_click(|_, _, cx| fact("tr-preview", "", "", cx))));
            for l in &preview { sp = sp.child(note(&l.kind, l.text.clone())); }
            c = c.child(sp);
        }
        body.push(c.into_any_element());
    }
    if let Some(h) = hook {
        let mut c = div().flex().flex_col().gap(T::SP_8)
            .child(field("Webhook URL (POST, JSON body)", "", "", parts::line(&mut fx.p, "trig|hook-url", &h.url, "", true, false, parts::Commit::Leave, window, cx, |_, _| {})));
        if h.signed { c = c.child(field("Secret (sign the raw body with HMAC-SHA256)", "", "", parts::line(&mut fx.p, "trig|hook-secret", &h.secret, "", true, false, parts::Commit::Leave, window, cx, |_, _| {}))); }
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
    modal("fx-trig", T::FACTORIES_MODAL_WIDE, "tr-close", &title, body, fx, window, cx)
}

/// A person's transcript (.fx-chatwrap): a sheet on the right, over the scrim.
fn chat_sheet(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let (name, chat) = rf(fx, |f| (f.chat_name.clone(), f.chat.as_ref().map(|c| c.0.clone()).unwrap_or_default()));
    let (focus, _) = handles(fx, "fx-chat", cx);
    let vp = window.viewport_size();
    // The chat page in the frame (.fx-chatbox iframe): its header, the conversation, the box.
    let page = super::view::transcript_body(fx, &chat, "chat-close", window, cx);
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
                .child(div().flex().flex_col().flex_1().min_h_0().child(page))),
    )).with_priority(1).into_any_element()
}
