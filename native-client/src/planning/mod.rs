//! The planning area (AREAS.md): phase 4: Kanban, Mind map, Notes, Schedules, webhooks, Factories.
//!
//! The registry (src/areas.rs) calls the functions below; keep their
//! signatures. Server calls go through crate::kit (kit::call, never on the
//! UI thread), the shared dialogs through kit::confirm / kit::prompt /
//! kit::menu. Pure rules go in their own files here, unit-tested.

pub mod board;
pub mod kanban;
pub mod media_bridge;

use crate::{areas::AreaEvent, kit, MainWindow, PlanningAsk};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;

pub fn init(w: &MainWindow) {
    kanban::init(w);
    w.global::<PlanningAsk>().on_answer(ask_answer);
}

pub fn signed_in() { kanban::project(&kit::project()); }

pub fn signed_out() {
    kanban::signed_out();
    if let Some(w) = kit::ui() { if w.global::<PlanningAsk>().get_open() { ask_answer(false); } }
}

pub fn project(alias: &str) { kanban::project(alias); }

pub fn tab_shown(tab: &str) {
    if tab == "kanban" { kanban::load(); }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Server("kanban", v) => {
            let p = kit::project().to_lowercase();
            if !p.is_empty() && kit::s(v, "project") == p && kanban::visible() { kanban::load(); }
        }
        AreaEvent::Connected(true) => { if kanban::visible() { kanban::load(); } }
        _ => {}
    }
}

/// The Media modal changed something (workspace calls this).
#[allow(dead_code)]
pub fn media_changed() { kanban::media_changed(); }

// ─── Closures that wait on a server call (kit::call's `done` must be Send;
// these hold UI-thread state, so they wait here, on the UI thread) ───

type Parked = Box<dyn FnOnce(Value)>;
thread_local! { static PARKED: RefCell<(u64, std::collections::HashMap<u64, Parked>)> = RefCell::new((0, Default::default())); }

pub fn park(f: impl FnOnce(Value) + 'static) -> u64 {
    PARKED.with(|p| { let mut p = p.borrow_mut(); p.0 += 1; let id = p.0; p.1.insert(id, Box::new(f)); id })
}

pub fn unpark(id: u64, v: Value) {
    let f = PARKED.with(|p| p.borrow_mut().1.remove(&id));
    if let Some(f) = f { f(v); }
}

// ─── The ask dialog (the web's askText, with its choice row) ───

pub struct Choice { pub label: String, pub options: Vec<(String, String)>, pub value: String }

type AskFn = Box<dyn FnOnce(Option<(String, String)>)>;
thread_local! { static ASK: RefCell<Option<AskFn>> = const { RefCell::new(None) }; }

/// Ask for text (and a choice): `then(Some((text, choice)))` on OK or
/// Enter, `then(None)` on Cancel, ×, Esc or the scrim.
pub fn ask(title: &str, label: &str, value: &str, multiline: bool, ok: &str, choice: Option<Choice>, then: impl FnOnce(Option<(String, String)>) + 'static) {
    let old = ASK.with(|a| a.borrow_mut().replace(Box::new(then)));
    if let Some(old) = old { old(None); }
    let Some(w) = kit::ui() else { return };
    let a = w.global::<PlanningAsk>();
    a.set_title(title.into());
    a.set_label(label.into());
    a.set_value(value.into());
    a.set_multiline(multiline);
    a.set_ok_label(if ok.is_empty() { "OK".into() } else { ok.into() });
    let (cl, keys, texts, cv) = match choice {
        Some(c) => (c.label, c.options.iter().map(|o| o.0.clone().into()).collect::<Vec<slint::SharedString>>(), c.options.iter().map(|o| o.1.clone().into()).collect::<Vec<slint::SharedString>>(), c.value),
        None => (String::new(), vec![], vec![], String::new()),
    };
    a.set_choice_label(cl.into());
    a.set_choice_keys(ModelRc::new(VecModel::from(keys)));
    a.set_choice_texts(ModelRc::new(VecModel::from(texts)));
    a.set_choice(cv.into());
    a.set_gen(a.get_gen() + 1);
    // A second open replaces the first (above): close and reopen so the field takes the focus.
    a.set_open(false);
    a.set_open(true);
    w.global::<crate::Shell>().set_modal_planning(true);
}

fn ask_answer(ok: bool) {
    let Some(w) = kit::ui() else { return };
    let a = w.global::<PlanningAsk>();
    let r = ok.then(|| (a.get_value().to_string(), a.get_choice().to_string()));
    a.set_open(false);
    w.global::<crate::Shell>().set_modal_planning(false);
    let f = ASK.with(|x| x.borrow_mut().take());
    if let Some(f) = f { f(r); }
}

fn ask_of() -> Value {
    let Some(w) = kit::ui() else { return Value::Null };
    let a = w.global::<PlanningAsk>();
    if !a.get_open() { return Value::Null; }
    use slint::Model;
    json!({ "title": a.get_title().as_str(), "label": a.get_label().as_str(), "value": a.get_value().as_str(), "multiline": a.get_multiline(), "ok": a.get_ok_label().as_str(),
        "choiceLabel": a.get_choice_label().as_str(), "choices": a.get_choice_keys().iter().map(|k| k.to_string()).collect::<Vec<_>>(), "choice": a.get_choice().as_str() })
}

// ─── Small saved settings of the area (the web's localStorage) ───

pub mod prefs {
    use serde_json::Value;
    fn path() -> std::path::PathBuf { crate::store::config_dir().join("planning.json") }
    fn all() -> Value { std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Object(Default::default())) }
    pub fn get(k: &str) -> String { all().get(k).and_then(|v| v.as_str()).unwrap_or("").to_string() }
    pub fn set(k: &str, v: &str) {
        let mut a = all();
        a[k] = Value::String(v.to_string());
        let _ = std::fs::write(path(), a.to_string());
    }
}

/// The test driver's "planning:<name>" actions; Some(answer) for a known one.
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "ping" => Some(json!({ "ok": true, "area": "planning" })),
        // Answer the ask dialog: ok, then the text and the choice (empty: as they are).
        "ask-answer" => {
            if let Some(w) = kit::ui() {
                let x = w.global::<PlanningAsk>();
                if !a(1).is_empty() { x.set_value(a(1).into()); }
                if !a(2).is_empty() { x.set_choice(a(2).into()); }
            }
            ask_answer(a(0) == "true");
            Some(json!({ "ok": true }))
        }
        _ => kanban::test_ui(name, args),
    }
}

/// What the test driver sees of the area ({"cmd":"area","area":"planning"}).
pub fn props() -> Value {
    json!({ "kanban": kanban::props(), "ask": ask_of() })
}
