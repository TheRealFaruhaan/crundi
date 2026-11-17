//! Factories (4.7, 4.8): the factories tab, the factory panel, the run graph. Called by src/planning/mod.rs with the registry's functions.
//!
//! The web's app/vendor/crundi-factories.js (mountTab, mountPanel,
//! startDialog, triggerDialog, openChat) and the factory-flow.js island
//! (tools/factory-flow/src: Editor, Viewer). This file holds the shared
//! state, the tab (Library, Runs, Cleanup), the panel, the badge and the
//! live events; the live graph is factories_view.rs, the editor
//! factories_edit.rs, the dialogs factories_dlg.rs; the pure rules and
//! words factories_model.rs (unit-tested).

#[path = "factories_model.rs"]
pub mod model;
#[path = "factories_data.rs"]
mod data;
#[path = "factories_view.rs"]
mod view;
#[path = "factories_edit.rs"]
mod edit;
#[path = "factories_dlg.rs"]
mod dlg;

use crate::{areas::AreaEvent, kit, MainWindow, PlanningFactories};
use crate::{PlanningFxCard, PlanningFxCardRow, PlanningFxTpl, PlanningFxTplRow, PlanningFxRun, PlanningFxClean};
use model::{s, n, b, arr};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

// ─── State ───

#[derive(Default)]
pub(crate) struct Tab {
    view: String,
    sub: String,                  // "" | editor | viewer | loading | error
    sub_id: String,
    lib: Option<(Vec<Value>, Vec<Value>)>,
    lib_w: f32,
    runs: Option<Vec<Value>>,
    clean: Option<(Vec<Value>, Value)>,
    picks: HashMap<String, bool>,
    busy_tpl: String,
    gen: u64,
}

#[derive(Default)]
pub(crate) struct Panel {
    project: String,
    runs: Vec<Value>,
    triggers: Vec<Value>,
    active: Option<String>,
    gen: u64,
}

#[derive(Default)]
pub(crate) struct F {
    pub tab: Tab,
    pub panel: Panel,
    pub state_runs: Vec<Value>,
    pub caps: Option<Vec<String>>,
    pub chat: Option<(String, String)>,
}

thread_local! {
    static S: RefCell<F> = RefCell::new(F { tab: Tab { view: "library".into(), lib_w: 900.0, ..Default::default() }, ..Default::default() });
    static M: RefCell<Option<Models>> = const { RefCell::new(None) };
}

pub(crate) fn with<R>(f: impl FnOnce(&mut F) -> R) -> R { S.with(|s| f(&mut s.borrow_mut())) }

/// Run `f` with the Factories global (on the UI thread).
pub(crate) fn g(f: impl FnOnce(&PlanningFactories)) {
    if let Some(w) = kit::ui() { f(&w.global::<PlanningFactories>()); }
}

/// The lists, set once on their properties (kit::sync_rows rewrites them).
pub(crate) struct Models {
    pub factories: Rc<VecModel<PlanningFxCardRow>>,
    pub templates: Rc<VecModel<PlanningFxTplRow>>,
    pub runs: Rc<VecModel<PlanningFxRun>>,
    pub clean: Rc<VecModel<PlanningFxClean>>,
    pub earlier: Rc<VecModel<PlanningFxRun>>,
    pub triggers: Rc<VecModel<PlanningFxRun>>,
    pub v: view::Models,
    pub e: edit::Models,
    pub d: dlg::Models,
}

pub(crate) fn models<R>(f: impl FnOnce(&Models) -> R) -> Option<R> { M.with(|m| m.borrow().as_ref().map(f)) }

pub fn ss(x: impl AsRef<str>) -> SharedString { SharedString::from(x.as_ref()) }
pub fn rc_of<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> { ModelRc::new(VecModel::from(v)) }
pub fn strs(v: &[String]) -> ModelRc<SharedString> { rc_of(v.iter().map(ss).collect()) }

pub fn init(w: &MainWindow) {
    let f = w.global::<PlanningFactories>();
    let m = Models {
        factories: kit::model(), templates: kit::model(), runs: kit::model(), clean: kit::model(), earlier: kit::model(), triggers: kit::model(),
        v: view::Models::new(&f), e: edit::Models::new(&f), d: dlg::Models::new(&f),
    };
    f.set_factories(kit::rc(&m.factories));
    f.set_templates(kit::rc(&m.templates));
    f.set_runs(kit::rc(&m.runs));
    f.set_clean(kit::rc(&m.clean));
    f.set_earlier(kit::rc(&m.earlier));
    f.set_triggers(kit::rc(&m.triggers));
    M.with(|x| *x.borrow_mut() = Some(m));
    f.on_act(|a, x, y| act(a.as_str(), x.as_str(), y.as_str()));
    f.on_edit(|form, k, v| edit_field(form.as_str(), k.as_str(), v.as_str()));
    f.on_cv_size(|which, w, h| {
        match which.as_str() {
            "lib" => { let changed = with(|s| { let c = (s.tab.lib_w - w).abs() > 0.5; s.tab.lib_w = w; c }); if changed { render_library(); } }
            "v" => view::size(w, h),
            "e" => edit::size(w, h),
            _ => {}
        }
        let _ = h;
    });
    f.on_cv_pan(|which, dx, dy| if which == "v" { view::pan(dx, dy) } else { edit::pan(dx, dy) });
    f.on_cv_zoom(|which, d, x, y| if which == "v" { view::zoom_at(d, x, y) } else { edit::zoom_at(d, x, y) });
    f.on_cv_node_down(|which, id| if which == "e" { edit::node_down(id.as_str()) });
    f.on_cv_node_drag(|which, id, dx, dy| if which == "e" { edit::node_drag(id.as_str(), dx, dy) });
    f.on_cv_node_up(|which, id, moved| if which == "v" { view::node_click(id.as_str()) } else { edit::node_up(id.as_str(), moved) });
    f.on_cv_handle_drag(|which, node, h, x, y| if which == "e" { edit::handle_drag(node.as_str(), h.as_str(), x, y) });
    f.on_cv_handle_up(|which, node, h, x, y| if which == "e" { edit::handle_up(node.as_str(), h.as_str(), x, y) });
    f.on_cv_edge(|which, id| if which == "e" { edit::edge_click(id.as_str()) });
    f.on_cv_pane(|which| if which == "v" { view::pane_click() } else { edit::pane_click() });
    f.on_cv_key(|which, k| if which == "e" { edit::key(k.as_str()) });
    f.on_cv_mini(|which, x, y| if which == "v" { view::mini(x, y) } else { edit::mini(x, y) });
    view::start_timer();
}

pub fn signed_in() {
    with(|s| { s.caps = None; });
    if tab_visible() { show_tab_view(); }
    if panel_visible() { panel_load(); }
}

pub fn signed_out() {
    with(|s| { let view = s.tab.view.clone(); *s = F::default(); s.tab.view = if view.is_empty() { "library".into() } else { view }; s.tab.lib_w = 900.0; });
    view::reset();
    edit::reset();
    dlg::close_all();
    g(|f| { f.set_sub("".into()); f.set_lib_loading(true); f.set_runs_loading(true); f.set_clean_loading(true); f.set_panel("loading".into()); f.set_chat_open(false); });
    set_badge("");
}

pub fn project(alias: &str) {
    let changed = with(|s| s.panel.project != alias.to_lowercase());
    if changed && panel_visible() { panel_load(); }
}

pub fn tab_shown(tab: &str) {
    if tab == "factories" {
        let sub = with(|s| s.tab.sub.clone());
        if sub == "viewer" { let id = with(|s| s.tab.sub_id.clone()); view::open(&id, false); } else if sub.is_empty() { show_tab_view(); }
    } else if tab == "workbench" && panel_visible() {
        panel_load();
    }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::State(v) => {
            if let Some(list) = v.get("factoryRuns").and_then(|x| x.as_array()) {
                let badge = model::badge(list);
                with(|s| s.state_runs = list.clone());
                set_badge(&badge);
            }
        }
        AreaEvent::Server("factoryRun", d) => run_event(d),
        AreaEvent::Server("factories", _) => run_event(&json!({ "kind": "factories" })),
        AreaEvent::Connected(true) => {
            if tab_visible() { let sub = with(|s| s.tab.sub.clone()); if sub.is_empty() { show_tab_view(); } else if sub == "viewer" { view::fetch(); } }
            if panel_visible() { panel_load(); }
        }
        _ => {}
    }
}

/// The web's runEvent(): every view that listens hears a run's change.
fn run_event(d: &Value) {
    // The tab's list views (not while an editor or a graph is open).
    if tab_visible() {
        let (sub, view) = with(|s| (s.tab.sub.clone(), s.tab.view.clone()));
        if sub.is_empty() && (view == "runs" || (view == "library" && s(d, "kind") == "factories")) { show_tab_view(); }
    }
    // The live graph: its own run, or any when the event names none.
    let id = s(d, "id");
    if view::showing() && (id.is_empty() || id == view::run_id()) { view::fetch(); }
    // The panel: its project's runs.
    if panel_visible() {
        let p = s(d, "project").to_lowercase();
        let mine = with(|s| s.panel.project.clone());
        if p.is_empty() || p == mine { panel_load(); }
    }
}

pub(crate) fn set_badge(text: &str) {
    if let Some(w) = kit::ui() { w.global::<crate::Shell>().set_badge_factories(text.into()); }
}

pub(crate) fn tab_visible() -> bool {
    kit::ui().map(|w| w.global::<crate::Shell>().get_tab() == "factories").unwrap_or(false)
}

pub(crate) fn panel_visible() -> bool {
    let Some(w) = kit::ui() else { return false };
    w.global::<crate::Shell>().get_tab() == "workbench" && w.get_panes().iter().any(|p| p.key == "panel:factory")
}

// ─── Server calls ───

/// A GET; `done` gets the JSON, or {ok:false, error} when it failed.
pub(crate) fn get(path: String, done: impl FnOnce(Value) + Send + 'static) {
    kit::call(move |api| api.get(&path), move |r| done(r.unwrap_or_else(|e| json!({ "ok": false, "error": e }))));
}
/// A POST; the same.
pub(crate) fn post(path: &str, body: Value, done: impl FnOnce(Value) + Send + 'static) {
    let path = path.to_string();
    kit::call(move |api| api.post(&path, &body), move |r| done(r.unwrap_or_else(|e| json!({ "ok": false, "error": e }))));
}

/// What the server can do (its handshake), asked once.
pub(crate) fn caps(then: impl FnOnce(Vec<String>) + 'static) {
    if let Some(c) = with(|s| s.caps.clone()) { then(c); return; }
    let id = super::park(move |v| then(v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()));
    get("/api/handshake".into(), move |hs| {
        let list = hs.get("server").and_then(|x| x.get("capabilities")).cloned().unwrap_or(json!([]));
        if hs.get("server").is_some() { with(|s| s.caps = Some(list.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default())); }
        super::unpark(id, list);
    });
}

pub(crate) fn err_text(r: &Value, fallback: &str) -> String { let e = s(r, "error"); if e.is_empty() { fallback.into() } else { e } }

// ─── The tab ───

fn show_tab_view() {
    let view = with(|s| s.tab.view.clone());
    g(|f| f.set_view(view.clone().into()));
    match view.as_str() { "runs" => load_runs(), "cleanup" => load_cleanup(), _ => load_library() }
}

fn set_view(v: &str) {
    with(|s| { s.tab.view = v.to_string(); });
    g(|f| { f.set_view(v.into()); match v { "runs" => f.set_runs_loading(true), "cleanup" => f.set_clean_loading(true), _ => f.set_lib_loading(true) } });
    show_tab_view();
}

/// Open the editor or a run's graph in the tab (openSub()).
pub(crate) fn open_sub(kind: &str, id: &str) {
    with(|s| { s.tab.sub = kind.to_string(); s.tab.sub_id = id.to_string(); });
    g(|f| f.set_sub(kind.into()));
    if kind == "viewer" { view::open(id, false); } else { edit::open(id); }
}

/// closeSub() and show(current).
pub(crate) fn close_sub() {
    let was = with(|s| { let w = s.tab.sub.clone(); s.tab.sub.clear(); s.tab.sub_id.clear(); w });
    if was == "viewer" { view::close(); }
    if was == "editor" { edit::reset(); }
    g(|f| f.set_sub("".into()));
    show_tab_view();
}

/// ctx.openTab(runId): the Factories tab, on that run's graph.
pub(crate) fn open_run_in_tab(id: &str) {
    with(|s| s.tab.view = "runs".into());
    g(|f| f.set_view("runs".into()));
    kit::show_tab("factories");
    open_sub("viewer", id);
}

fn load_library() {
    let gen = with(|s| { s.tab.gen += 1; s.tab.gen });
    kit::call(|api| {
        let f = api.get("/api/factories").unwrap_or_else(|_| json!({ "factories": [] }));
        let t = api.get("/api/factory-templates").unwrap_or_else(|_| json!({ "templates": [] }));
        Ok(json!({ "f": f, "t": t }))
    }, move |r| {
        let v = r.unwrap_or(Value::Null);
        if with(|s| s.tab.gen != gen) { return; }
        let list = arr(&v["f"], "factories").to_vec();
        let tpls = arr(&v["t"], "templates").to_vec();
        with(|s| s.tab.lib = Some((list, tpls)));
        render_library();
    });
}

fn render_library() {
    let (lib, w, busy) = with(|s| (s.tab.lib.clone(), s.tab.lib_w, s.tab.busy_tpl.clone()));
    let Some((list, tpls)) = lib else { return };
    let now = model::now_ms();
    let cols = (((w + 12.0) / (280.0 + 12.0)).floor() as usize).max(1);
    let cards: Vec<PlanningFxCard> = list.iter().map(|f| PlanningFxCard {
        id: ss(s(f, "id")), name: ss(s(f, "name")), runnable: b(f, "runnable"),
        chip: ss(if b(f, "runnable") { "Ready".to_string() } else { format!("Draft · {} to fix", n(f, "errorCount") as i64) }),
        desc: ss({ let d = s(f, "description"); if d.is_empty() { "No description.".into() } else { d } }),
        meta: ss(model::factory_meta(f, now)),
    }).collect();
    let tcards: Vec<PlanningFxTpl> = tpls.iter().map(|t| PlanningFxTpl {
        id: ss(s(t, "id")), name: ss(s(t, "name")), good_for: ss(s(t, "goodFor")), description: ss(s(t, "description")),
        team: ss(arr(t, "team").iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")), busy: busy == s(t, "id"),
    }).collect();
    fn rows<T: Clone + Default>(v: Vec<T>, cols: usize) -> Vec<Vec<T>> {
        v.chunks(cols).map(|c| { let mut r = c.to_vec(); while r.len() < cols { r.push(T::default()); } r }).collect()
    }
    let frows: Vec<PlanningFxCardRow> = rows(cards, cols).into_iter().map(|r| PlanningFxCardRow { cards: rc_of(r) }).collect();
    let trows: Vec<PlanningFxTplRow> = rows(tcards, cols).into_iter().map(|r| PlanningFxTplRow { cards: rc_of(r) }).collect();
    models(|m| {
        // Rows hold models: set them anew only when the grid changed.
        let same = |old: Option<PlanningFxCardRow>, new: &PlanningFxCardRow| old.map(|o| o.cards.iter().collect::<Vec<_>>() == new.cards.iter().collect::<Vec<_>>()).unwrap_or(false);
        let n = frows.len();
        for (i, r) in frows.into_iter().enumerate() { if i < m.factories.row_count() { if !same(m.factories.row_data(i), &r) { m.factories.set_row_data(i, r); } } else { m.factories.push(r); } }
        while m.factories.row_count() > n { m.factories.remove(m.factories.row_count() - 1); }
        let tsame = |old: Option<PlanningFxTplRow>, new: &PlanningFxTplRow| old.map(|o| o.cards.iter().collect::<Vec<_>>() == new.cards.iter().collect::<Vec<_>>()).unwrap_or(false);
        let n = trows.len();
        for (i, r) in trows.into_iter().enumerate() { if i < m.templates.row_count() { if !tsame(m.templates.row_data(i), &r) { m.templates.set_row_data(i, r); } } else { m.templates.push(r); } }
        while m.templates.row_count() > n { m.templates.remove(m.templates.row_count() - 1); }
    });
    g(|f| { f.set_lib_cols(cols as i32); f.set_lib_empty(list.is_empty()); f.set_lib_loading(false); });
}

pub(crate) fn run_row(r: &Value, now: f64, earlier: bool) -> PlanningFxRun {
    let (chip, cls) = model::status_chip(r);
    PlanningFxRun {
        id: ss(s(r, "id")), name: ss(s(r, "factoryName")), project: ss(s(r, "project")), chip: ss(chip), tone: ss(model::chip_tone(&cls)),
        task: ss(s(r, "task")), meta: ss(if earlier { model::earlier_meta(r, now) } else { model::run_meta(r, now) }), off: false, tag1: "".into(), tag2: "".into(),
    }
}

fn load_runs() {
    let gen = with(|s| { s.tab.gen += 1; s.tab.gen });
    get("/api/factory-runs".into(), move |j| {
        if with(|s| s.tab.gen != gen) { return; }
        let runs = arr(&j, "runs").to_vec();
        let now = model::now_ms();
        let rows: Vec<PlanningFxRun> = runs.iter().map(|r| run_row(r, now, false)).collect();
        with(|s| s.tab.runs = Some(runs));
        models(|m| kit::sync_rows(&m.runs, rows));
        g(|f| f.set_runs_loading(false));
    });
}

fn load_cleanup() {
    let gen = with(|s| { s.tab.gen += 1; s.tab.gen });
    get("/api/factory-cleanup".into(), move |j| {
        if with(|s| s.tab.gen != gen) { return; }
        let items = arr(&j, "items").to_vec();
        let settings = j.get("settings").cloned().unwrap_or(json!({}));
        with(|s| { s.tab.clean = Some((items, settings.clone())); s.tab.picks.clear(); });
        render_cleanup();
        g(|f| { f.set_clean_days(ss((n(&settings, "autoDeleteDays") as i64).to_string())); f.set_clean_gen(f.get_clean_gen() + 1); f.set_clean_loading(false); });
    });
}

fn render_cleanup() {
    let (clean, picks) = with(|s| (s.tab.clean.clone(), s.tab.picks.clone()));
    let Some((items, _)) = clean else { return };
    let rows: Vec<PlanningFxClean> = items.iter().map(|it| PlanningFxClean {
        id: ss(s(it, "id")), title: ss(model::cleanup_title(it)), meta: ss(model::cleanup_meta(it)), why: ss(s(it, "why")), warn: ss(model::cleanup_warn(it)),
        size: ss(model::cleanup_size(it)), age: ss(model::cleanup_age(it)), picked: picks.get(&s(it, "id")).copied().unwrap_or(false),
    }).collect();
    models(|m| kit::sync_rows(&m.clean, rows));
}

fn cleanup_delete() {
    let (items, picks) = with(|s| (s.tab.clean.clone().map(|c| c.0).unwrap_or_default(), s.tab.picks.clone()));
    let ids: Vec<String> = picks.iter().filter(|(_, v)| **v).map(|(k, _)| k.clone()).collect();
    if ids.is_empty() { kit::toast("Choose what to delete first."); return; }
    let picked: Vec<&Value> = items.iter().filter(|i| ids.contains(&s(i, "id"))).collect();
    let msg = model::cleanup_confirm(&picked);
    kit::confirm(kit::Confirm { title: "Delete", message: &msg, ok: "Delete", danger: true }, move |ok| {
        if !ok { return; }
        post("/api/factory-cleanup", json!({ "action": "remove", "ids": ids }), |r| {
            if b(&r, "ok") { kit::toast_ok("Deleted."); } else { kit::toast_error(&err_text(&r, "Some could not be deleted.")); }
            load_cleanup();
        });
    });
}

// ─── The library's actions ───

fn new_factory() {
    kit::prompt("New factory", "Name the new factory", "New factory", "", "Create", false, |name| {
        let Some(name) = name else { return };
        let name = if name.trim().is_empty() { "New factory".to_string() } else { name };
        post("/api/factories", json!({ "action": "create", "name": name }), |r| {
            if !b(&r, "ok") { kit::toast_error(&err_text(&r, "Could not create it.")); return; }
            open_sub("editor", &s(&r["factory"], "id"));
        });
    });
}

fn import_factory() {
    kit::bg(|| crate::testctl::take_pick().or_else(|| rfd::FileDialog::new().add_filter("Factory", &["json"]).pick_file()).and_then(|p| std::fs::read_to_string(p).ok()), |text| {
        let Some(text) = text else { return };
        post("/api/factories", json!({ "action": "import", "data": text }), |r| {
            if !b(&r, "ok") { kit::toast_error(&err_text(&r, "Could not import it.")); return; }
            kit::toast_ok(&model::import_toast(&r));
            load_library();
        });
    });
}

fn export_factory(id: &str) {
    let name = with(|s| s.tab.lib.as_ref().and_then(|l| l.0.iter().find(|f| s_id(f) == id).map(|f| model::s(f, "name")))).unwrap_or_default();
    get(format!("/api/factories/export?id={}", kit::enc(id)), move |r| {
        if !b(&r, "ok") { kit::toast_error(&err_text(&r, "Could not export it.")); return; }
        let text = serde_json::to_string_pretty(&r["data"]).unwrap_or_default();
        let file = model::export_name(&name);
        kit::bg(move || {
            let path = crate::testctl::take_pick().or_else(|| rfd::FileDialog::new().set_title(format!("Save {file}")).set_file_name(&file).save_file());
            path.map(|p| std::fs::write(&p, text).map(|_| p.display().to_string()).map_err(|e| e.to_string()))
        }, |res| match res { Some(Ok(p)) => kit::toast_ok(&format!("Saved {p}")), Some(Err(e)) => kit::toast_error(&e), None => {} });
    });
}
fn s_id(v: &Value) -> String { s(v, "id") }

fn delete_factory(id: &str) {
    let name = with(|s| s.tab.lib.as_ref().and_then(|l| l.0.iter().find(|f| s_id(f) == id).map(|f| model::s(f, "name")))).unwrap_or_default();
    let id = id.to_string();
    let msg = format!("Delete \"{name}\"? Runs that already started keep their own copy.");
    kit::confirm(kit::Confirm { title: "Delete factory", message: &msg, ok: "Delete", danger: true }, move |ok| {
        if !ok { return; }
        post("/api/factories", json!({ "action": "delete", "id": id }), |r| { if !b(&r, "ok") { kit::toast_error(&err_text(&r, "That did not work.")); } load_library(); });
    });
}

fn use_template(id: &str) {
    let name = with(|s| { s.tab.busy_tpl = id.to_string(); s.tab.lib.as_ref().and_then(|l| l.1.iter().find(|t| s_id(t) == id).map(|t| model::s(t, "name"))).unwrap_or_default() });
    render_library();
    post("/api/factories", json!({ "action": "fromTemplate", "templateId": id }), move |r| {
        with(|s| s.tab.busy_tpl.clear());
        render_library();
        if !b(&r, "ok") { kit::toast_error(&err_text(&r, "Could not copy it.")); return; }
        kit::toast_ok(&format!("\"{name}\" is in your factories now. Change it however you like."));
        open_sub("editor", &s(&r["factory"], "id"));
    });
}

// ─── The project's Factory panel (mountPanel) ───

pub(crate) fn panel_load() {
    let p = kit::project().to_lowercase();
    let gen = with(|s| { let changed = s.panel.project != p; s.panel.project = p.clone(); if changed { s.panel.active = None; } s.panel.gen += 1; s.panel.gen });
    if p.is_empty() { g(|f| f.set_panel("none".into())); return; }
    let path = format!("/api/factory-runs?project={}", kit::enc(&p));
    let tpath = format!("/api/factory-triggers?project={}", kit::enc(&p));
    kit::call(move |api| {
        let r = api.get(&path).unwrap_or_else(|_| json!({ "runs": [] }));
        let t = api.get(&tpath).unwrap_or_else(|_| json!({ "triggers": [] }));
        Ok(json!({ "r": r, "t": t }))
    }, move |r| {
        let v = r.unwrap_or(Value::Null);
        if with(|s| s.panel.gen != gen) { return; }
        let runs = arr(&v["r"], "runs").to_vec();
        let triggers = arr(&v["t"], "triggers").to_vec();
        let active = runs.iter().find(|r| model::is_active(r)).map(|r| s(r, "id"));
        let was = with(|s| { let w = s.panel.active.clone(); s.panel.runs = runs.clone(); s.panel.triggers = triggers.clone(); s.panel.active = active.clone(); w });
        if let Some(id) = active {
            // A run is going: the panel is its live graph; it keeps itself up to date.
            if was.as_deref() == Some(id.as_str()) && view::showing() && view::run_id() == id && view::is_panel() { return; }
            g(|f| f.set_panel("live".into()));
            view::open(&id, true);
            return;
        }
        if was.is_some() && view::is_panel() { view::close(); }
        let now = model::now_ms();
        let earlier: Vec<PlanningFxRun> = runs.iter().take(8).map(|r| run_row(r, now, true)).collect();
        let trows: Vec<PlanningFxRun> = triggers.iter().map(|t| {
            let q = arr(t, "queue").len();
            PlanningFxRun {
                id: ss(s(t, "id")), name: ss(s(t, "name")), project: "".into(), chip: "".into(), tone: "".into(),
                task: ss(model::trigger_when(t)), meta: ss(model::trigger_last(t, now)), off: !b(t, "enabled"),
                tag1: if b(t, "enabled") { "".into() } else { "off".into() }, tag2: if q > 0 { ss(format!("{q} waiting")) } else { "".into() },
            }
        }).collect();
        let has_triggers = !trows.is_empty();
        models(|m| { kit::sync_rows(&m.earlier, earlier); kit::sync_rows(&m.triggers, trows); });
        g(|f| {
            // The web draws the panel anew on each load: Triggers open when there are any.
            f.set_triggers_open(has_triggers);
            f.set_triggers_loaded(true);
            f.set_panel("idle".into());
        });
    });
}

// ─── Everything a button does ───

fn act(a: &str, x: &str, y: &str) {
    if a.starts_with("v-") { view::act(a, x, y); return; }
    if a.starts_with("e-") { edit::act(a, x, y); return; }
    if a.starts_with("st-") || a.starts_with("pin-") || a.starts_with("tr-") || a.starts_with("chat-") { dlg::act(a, x, y); return; }
    match a {
        "view" => set_view(x),
        "new" => new_factory(),
        "import" => import_factory(),
        "start-one" => {
            // Start one (the head) or a card's Start: the run's graph after.
            dlg::open_start(dlg::StartOpts { project: String::new(), factory: x.to_string(), then: dlg::Then::Viewer });
        }
        "edit" => open_sub("editor", x),
        "copy" => post("/api/factories", json!({ "action": "duplicate", "id": x }), |r| { if !b(&r, "ok") { kit::toast_error(&err_text(&r, "That did not work.")); } load_library(); }),
        "export" => export_factory(x),
        "delete" => delete_factory(x),
        "use-template" => use_template(x),
        "open-run" => open_run_in_tab(x),
        "cleanup-delete" => cleanup_delete(),
        "sub-back" => close_sub(),
        "panel-start" => {
            let p = with(|s| s.panel.project.clone());
            dlg::open_start(dlg::StartOpts { project: p, factory: String::new(), then: dlg::Then::Panel });
        }
        "trigger" => { let p = with(|s| s.panel.project.clone()); dlg::open_trigger(&p, x); }
        "cv-zoom-in" | "cv-zoom-out" | "cv-fit" => {
            let k = a.trim_start_matches("cv-");
            if x == "v" { view::control(k) } else { edit::control(k) }
        }
        _ => {}
    }
    let _ = y;
}

fn edit_field(form: &str, k: &str, v: &str) {
    match form {
        "cleanup" => {
            if let Some(id) = k.strip_prefix("pick:") { with(|s| { s.tab.picks.insert(id.to_string(), v == "1"); }); render_cleanup(); return; }
            if k == "days" {
                let days: i64 = v.trim().parse::<i64>().unwrap_or(0).clamp(0, 365);
                let cur = with(|s| s.tab.clean.as_ref().map(|c| n(&c.1, "autoDeleteDays") as i64).unwrap_or(0));
                if days == cur { return; }
                post("/api/factory-cleanup", json!({ "action": "settings", "autoDeleteDays": days }), move |r| {
                    if b(&r, "ok") { with(|s| if let Some(c) = s.tab.clean.as_mut() { c.1["autoDeleteDays"] = json!(days); }); kit::toast_ok("Saved."); } else { kit::toast_error(&err_text(&r, "Not saved")); }
                });
            }
        }
        "limits" | "viewer" => view::edit(form, k, v),
        "ed" => edit::edit(k, v),
        _ => dlg::edit(form, k, v),
    }
}

// ─── The driver ───

/// The driver's "planning:<name>" actions this screen answers (prefix them "factories-").
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let name = name.strip_prefix("factories-")?;
    match name {
        // Any button: factories-act <what> <id> <more>.
        "act" => { act(&a(0), &a(1), &a(2)); Some(json!({ "ok": true })) }
        "edit" => { edit_field(&a(0), &a(1), &a(2)); Some(json!({ "ok": true })) }
        "node-click" => { if a(0) == "v" { view::node_click(&a(1)); } else { edit::node_down(&a(1)); edit::node_up(&a(1), false); } Some(json!({ "ok": true })) }
        "node-move" => { edit::node_down(&a(0)); let z = edit::zoom(); edit::node_drag(&a(0), a(1).parse::<f32>().unwrap_or(0.0) * z, a(2).parse::<f32>().unwrap_or(0.0) * z); edit::node_up(&a(0), true); Some(json!({ "ok": true })) }
        "connect" => { Some(json!({ "ok": edit::connect_ids(&a(0), &a(1), &a(2)) })) }
        "edge-click" => { edit::edge_click(&a(0)); Some(json!({ "ok": true })) }
        "key" => { edit::key(&a(0)); Some(json!({ "ok": true })) }
        "pane" => { if a(0) == "v" { view::pane_click() } else { edit::pane_click() } Some(json!({ "ok": true })) }
        "badge" => Some(json!({ "ok": true, "badge": kit::ui().map(|w| w.global::<crate::Shell>().get_badge_factories().to_string()).unwrap_or_default() })),
        _ => None,
    }
}

/// This screen's part of the area's props (props.areas.planning.factories).
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return json!({}) };
    let f = w.global::<PlanningFactories>();
    let runrow = |r: PlanningFxRun| json!({ "id": r.id.as_str(), "name": r.name.as_str(), "project": r.project.as_str(), "chip": r.chip.as_str(), "tone": r.tone.as_str(), "task": r.task.as_str(), "meta": r.meta.as_str(), "off": r.off, "tags": [r.tag1.as_str(), r.tag2.as_str()] });
    json!({
        "view": f.get_view().as_str(),
        "sub": f.get_sub().as_str(),
        "subError": f.get_sub_error().as_str(),
        "badge": w.global::<crate::Shell>().get_badge_factories().as_str(),
        "library": {
            "loading": f.get_lib_loading(), "empty": f.get_lib_empty(), "cols": f.get_lib_cols(),
            "factories": f.get_factories().iter().flat_map(|r| r.cards.iter().collect::<Vec<_>>()).filter(|c| !c.id.is_empty()).map(|c| json!({ "id": c.id.as_str(), "name": c.name.as_str(), "runnable": c.runnable, "chip": c.chip.as_str(), "desc": c.desc.as_str(), "meta": c.meta.as_str() })).collect::<Vec<_>>(),
            "templates": f.get_templates().iter().flat_map(|r| r.cards.iter().collect::<Vec<_>>()).filter(|c| !c.id.is_empty()).map(|c| json!({ "id": c.id.as_str(), "name": c.name.as_str(), "team": c.team.as_str() })).collect::<Vec<_>>(),
        },
        "runs": { "loading": f.get_runs_loading(), "items": f.get_runs().iter().map(runrow).collect::<Vec<_>>() },
        "cleanup": { "loading": f.get_clean_loading(), "days": f.get_clean_days().as_str(), "items": f.get_clean().iter().map(|c| json!({ "id": c.id.as_str(), "title": c.title.as_str(), "meta": c.meta.as_str(), "size": c.size.as_str(), "age": c.age.as_str(), "picked": c.picked, "warn": c.warn.as_str() })).collect::<Vec<_>>() },
        "panel": { "state": f.get_panel().as_str(), "earlier": f.get_earlier().iter().map(runrow).collect::<Vec<_>>(), "earlierOpen": f.get_earlier_open(), "triggers": f.get_triggers().iter().map(runrow).collect::<Vec<_>>(), "triggersOpen": f.get_triggers_open() },
        "viewer": view::props(&f),
        "editor": edit::props(&f),
        "dialogs": dlg::props(&f),
    })
}
