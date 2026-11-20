//! Factories: the Factories tab (the head with Library / Runs / Cleanup;
//! the library's cards and templates; the runs; the leftovers), the factory
//! editor, the live run graph, and the project's Factory panel (pane kind
//! "panel:factory": the live graph while a run is going, else the start,
//! the earlier runs and the triggers). Spec: app/vendor/crundi-factories.js
//! and factory-flow.js; ported from native-gpui's planning/factories*.rs,
//! checked by test/planning/factories.mjs.
//!
//! On v2's rules: all of their state lives in one store entity (`Fx`: the
//! tab and panel, the run graph, the editor, the dialogs, the text fields
//! and canvases). A Global holds only its handle, read by handlers through
//! `with(cx, …)`; the tab's leaf, the panel's leaf and the panel's head keep
//! the handle, observe it and draw from it. A change notifies the store
//! (`bump`); nothing refreshes the window. Both leaves are cached and sit
//! side by side with the rest of the window. The graph
//! builds and paints only the steps, connections and labels on screen; its
//! paths are parsed once per layout and kept in the store (parts.rs `parsed`). The run's timer
//! fetches only while the graph is on show, and draws only when the answer
//! changed something.
//!
//! This file holds the shared state, the tab, the panel, the badge and the
//! live events; the live graph is view.rs, the editor edit.rs, the dialogs
//! dlg.rs, the shared parts and the canvas parts.rs, the pure rules and
//! words model.rs.

pub mod model;
mod data;
mod parts;
mod view;
mod edit;
mod dlg;

use crundi_core::live::Event;
use crundi_shell::Stores;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use model::{arr, b, n, s};
use parts::{btn, get, note, post, Card, Clean, Kv, RunRow, Tpl};
use serde_json::{json, Value};
use std::{collections::HashMap};

// ─── State ───

#[derive(Default)]
pub(crate) struct Tab {
    view: String,
    sub: String, // "" | editor | viewer | loading | error
    sub_id: String,
    sub_error: String,
    lib: Option<(Vec<Value>, Vec<Value>)>,
    runs: Option<Vec<Value>>,
    clean: Option<(Vec<Value>, Value)>,
    picks: HashMap<String, bool>,
    busy_tpl: String,
    gn: u64,
    // What the tab shows (the Slint global's properties).
    lib_loading: bool,
    /// The library could not be loaded (the server unreachable, say): shown
    /// with Retry instead of an empty library (F66; the web shows it empty).
    lib_err: String,
    factories: Vec<Card>,
    templates: Vec<Tpl>,
    runs_loading: bool,
    run_rows: Vec<RunRow>,
    clean_loading: bool,
    clean_rows: Vec<Clean>,
    clean_days: String,
}

#[derive(Default)]
pub(crate) struct Panel {
    project: String,
    runs: Vec<Value>,
    triggers: Vec<Value>,
    active: Option<String>,
    gn: u64,
    state: String, // none | loading | idle | live
    earlier: Vec<RunRow>,
    earlier_open: bool,
    trigger_rows: Vec<RunRow>,
    triggers_open: bool,
    triggers_loaded: bool,
    asked: bool,
}

#[derive(Default)]
pub(crate) struct F {
    pub tab: Tab,
    pub panel: Panel,
    pub state_runs: Vec<Value>,
    pub caps: Option<Vec<String>>,
    pub badge: String,
    pub cur_tab: String,
    // A person's transcript (the web's .fx-chatwrap; inside the panel, the graph's).
    pub chat: Option<(String, String)>,
    pub chat_open: bool,
    pub chat_name: String,
    pub chat_state: String,
    pub chat_lines: Vec<Kv>,
}

fn fresh() -> F {
    F { tab: Tab { view: "library".into(), lib_loading: true, runs_loading: true, clean_loading: true, clean_days: "0".into(), ..Default::default() },
        panel: Panel { state: "loading".into(), ..Default::default() }, cur_tab: "workbench".into(), ..Default::default() }
}

// ─── The store (ARCHITECTURE.md rules 1-2) ───

/// Everything the factories screens hold: the tab and the panel (`f`), the
/// run graph (`v`), the editor (`e`), the dialogs (`d`), the text fields,
/// sizes and pointer of the canvases (`p`). One entity: a change notifies it
/// (`bump`) and only the views that show factories (the tab's leaf, the
/// panel's leaf, the panel's head) draw again. Views keep its handle and
/// draw from it; handlers reach it through `with` (the Global holds only the
/// handle). No window refresh.
pub struct Fx {
    pub(crate) f: F,
    pub(crate) v: view::V,
    pub(crate) e: edit::E,
    pub(crate) d: dlg::D,
    pub(crate) p: parts::P,
}

struct Handle(Entity<Fx>);
impl Global for Handle {}

/// The store's handle (handlers and init only, never in render).
pub(crate) fn store(cx: &mut App) -> Entity<Fx> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let e = cx.new(|_| Fx { f: fresh(), v: view::blank(), e: edit::blank(), d: dlg::D::default(), p: parts::P::default() });
    cx.set_global(Handle(e.clone()));
    e
}

/// The whole store, from a handler.
pub(crate) fn with_fx<R>(cx: &mut App, f: impl FnOnce(&mut Fx) -> R) -> R { store(cx).update(cx, |x, _| f(x)) }

/// The tab's and the panel's state, from a handler.
pub(crate) fn with<R>(cx: &mut App, f: impl FnOnce(&mut F) -> R) -> R { with_fx(cx, |x| f(&mut x.f)) }

/// While drawing: the tab's and the panel's state, from the store being drawn.
fn rf<R>(fx: &mut Fx, f: impl FnOnce(&mut F) -> R) -> R { f(&mut fx.f) }

/// Something the factories screens show changed: notify their views.
pub(crate) fn bump(cx: &mut App) { store(cx).update(cx, |_, cx| cx.notify()); }

/// The Factories tab's leaf: the page (library / runs / cleanup, the editor
/// or a run's graph) and its dialogs; cached, redrawn when the store changes.
pub struct TabLeaf { store: Entity<Fx>, _sub: Subscription }
impl Render for TabLeaf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("factories");
        // (An update, not a read: the text fields' states are made on first draw.)
        self.store.update(cx, |fx, cx| view("factories", "", fx, window, cx))
    }
}

/// The tab's root (layout: the leaf, cached).
pub struct TabRoot(Entity<TabLeaf>);
impl Render for TabRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// The Factories tab's view (lib.rs's registry).
pub fn tab(_: &Stores, _window: &mut Window, cx: &mut App) -> AnyView {
    let st = store(cx);
    let leaf = cx.new(|cx| TabLeaf { store: st.clone(), _sub: cx.observe(&st, |_, _, cx| cx.notify()) });
    with(cx, |s| s.cur_tab = "factories".into());
    cx.new(|_| TabRoot(leaf)).into()
}

/// The Factory panel's body (a pane's cached leaf).
pub struct PanelLeaf { store: Entity<Fx>, _sub: Subscription }
impl Render for PanelLeaf {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("factory-panel");
        self.store.update(cx, |fx, cx| panel_view("panel:factory", fx, window, cx))
    }
}

/// At start: the panel's pane kind, the live graph's timer.
pub fn init(cx: &mut App) {
    store(cx);
    crundi_workbench::register_kind(cx, crundi_workbench::PaneKind {
        prefix: "panel:factory",
        chat: false,
        make: std::rc::Rc::new(|_, _stores, _window, cx| {
            let s = store(cx);
            let leaf = cx.new(|cx| PanelLeaf { store: s.clone(), _sub: cx.observe(&s, |_, _, cx| cx.notify()) });
            let s2 = s.clone();
            crundi_workbench::PaneParts {
                body: std::rc::Rc::new(move |_| leaf.clone().cached(StyleRefinement::default().size_full()).into_any_element()),
                info: std::rc::Rc::new(move |cx| crundi_workbench::HeadInfo { title: "Factory".into(), long: String::new(),
                    state: if s2.read(cx).f.panel.state == "live" { "working".to_string() } else { "idle".to_string() } }),
                watch: Box::new(move |cx| vec![cx.observe(&s, |h, _, cx| h.refresh(cx))]),
            }
        }),
        close: None,
        zoom: None,
    });
    view::start_timer(cx);
}

pub fn on_signed_in(_: &Stores, cx: &mut App) {
    let t = crate::util::current_tab(cx);
    with(cx, |s| { s.caps = None; s.cur_tab = t; });
    if tab_visible(cx) { show_tab_view(cx); }
    if panel_visible(cx) { panel_load(cx); }
}

pub fn on_signed_out(_: &Stores, cx: &mut App) {
    with(cx, |s| { let view = s.tab.view.clone(); let t = s.cur_tab.clone(); *s = fresh(); s.tab.view = if view.is_empty() { "library".into() } else { view }; s.cur_tab = t; });
    view::reset(cx);
    edit::reset_quiet(cx);
    dlg::close_all(cx);
    bump(cx);
}

pub fn on_project(_: &Stores, alias: &str, cx: &mut App) {
    let changed = with(cx, |s| s.panel.project != alias.to_lowercase());
    if changed && panel_visible(cx) { panel_load(cx); }
}

pub fn on_tab(tab: &str, cx: &mut App) {
    with(cx, |s| s.cur_tab = tab.to_string());
    if tab == "factories" {
        let sub = with(cx, |s| s.tab.sub.clone());
        if sub == "viewer" { let id = with(cx, |s| s.tab.sub_id.clone()); view::open(&id, false, cx); } else if sub.is_empty() { show_tab_view(cx); }
    } else if tab == "workbench" && panel_visible(cx) {
        panel_load(cx);
    }
    bump(cx);
}

/// The frame's live events: the runs' badge (the state), a run's change, the
/// library's, a reconnection.
pub fn on_batch(_: &Stores, batch: &[Event], cx: &mut App) {
    for e in batch {
        match e {
            Event::State(v) => {
                if let Some(list) = v.get("factoryRuns").and_then(|x| x.as_array()) {
                    let badge = model::badge(list);
                    let changed = with(cx, |s| { let c = s.badge != badge; s.state_runs = list.clone(); s.badge = badge; c });
                    if changed { bump(cx); }
                }
            }
            Event::Server(n, d) if n == "factoryRun" => run_event(d, cx),
            Event::Server(n, _) if n == "factories" => run_event(&json!({ "kind": "factories" }), cx),
            Event::Connected(true) => {
                if tab_visible(cx) { let sub = with(cx, |s| s.tab.sub.clone()); if sub.is_empty() { show_tab_view(cx); } else if sub == "viewer" { view::fetch(cx); } }
                if panel_visible(cx) { panel_load(cx); }
            }
            _ => {}
        }
    }
}

/// The web's runEvent(): every view that listens hears a run's change.
fn run_event(d: &Value, cx: &mut App) {
    if tab_visible(cx) {
        let (sub, view) = with(cx, |s| (s.tab.sub.clone(), s.tab.view.clone()));
        if sub.is_empty() && (view == "runs" || (view == "library" && s(d, "kind") == "factories")) { show_tab_view(cx); }
    }
    let id = s(d, "id");
    if view::showing(cx) && (id.is_empty() || id == view::run_id(cx)) { view::fetch(cx); }
    if panel_visible(cx) {
        let p = s(d, "project").to_lowercase();
        let mine = with(cx, |s| s.panel.project.clone());
        if p.is_empty() || p == mine { panel_load(cx); }
    }
    bump(cx);
}

pub(crate) fn tab_visible(cx: &mut App) -> bool { with(cx, |s| s.cur_tab == "factories") }

/// The panel is in the workbench on show. Never call it while the workbench draws.
pub(crate) fn panel_visible(cx: &mut App) -> bool {
    if with(cx, |s| s.cur_tab != "workbench") { return false; }
    crundi_workbench::workbench(cx).map(|wb| wb.update(cx, |w, _| w.mosaic().keys().iter().any(|k| k == "panel:factory"))).unwrap_or(false)
}

/// What the server can do (its handshake), asked once.
pub(crate) fn caps(cx: &mut App, then: impl FnOnce(Vec<String>, &mut App) + 'static) {
    if let Some(c) = with(cx, |s| s.caps.clone()) { then(c, cx); return; }
    get(cx, "/api/handshake".into(), move |hs, cx| {
        let list: Vec<String> = hs.get("server").and_then(|x| x.get("capabilities")).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        if hs.get("server").is_some() { with(cx, |s| s.caps = Some(list.clone())); }
        then(list, cx);
    });
}

// ─── The tab ───

fn show_tab_view(cx: &mut App) {
    let view = with(cx, |s| s.tab.view.clone());
    match view.as_str() { "runs" => load_runs(cx), "cleanup" => load_cleanup(cx), _ => load_library(cx) }
}

fn set_view(v: &str, cx: &mut App) {
    with(cx, |s| { s.tab.view = v.to_string(); match v { "runs" => s.tab.runs_loading = true, "cleanup" => s.tab.clean_loading = true, _ => s.tab.lib_loading = true } });
    show_tab_view(cx);
}

/// Open the editor or a run's graph in the tab (openSub()).
pub(crate) fn open_sub(kind: &str, id: &str, cx: &mut App) {
    with(cx, |s| { s.tab.sub = kind.to_string(); s.tab.sub_id = id.to_string(); });
    if kind == "viewer" { view::open(id, false, cx); } else { edit::open(id, cx); }
}

/// closeSub() and show(current).
pub(crate) fn close_sub(cx: &mut App) {
    let was = with(cx, |s| { let w = s.tab.sub.clone(); s.tab.sub.clear(); s.tab.sub_id.clear(); w });
    if was == "viewer" { view::close(cx); }
    if was == "editor" { edit::reset(cx); }
    show_tab_view(cx);
}

/// ctx.openTab(runId): the Factories tab, on that run's graph.
pub(crate) fn open_run_in_tab(id: &str, cx: &mut App) {
    with(cx, |s| s.tab.view = "runs".into());
    crate::util::show_tab(cx, "factories");
    open_sub("viewer", id, cx);
}

fn load_library(cx: &mut App) {
    let gn = with(cx, |s| { s.tab.gn += 1; s.tab.gn });
    crundi_shell::call(cx, |api| {
        let f = match api.get("/api/factories") { Ok(v) => v, Err(e) => return Ok(json!({ "err": e.to_string() })) };
        let t = api.get("/api/factory-templates").unwrap_or_else(|_| json!({ "templates": [] }));
        Ok(json!({ "f": f, "t": t }))
    }, move |r, cx| {
        let v = match r { Ok(v) => v, Err(e) => json!({ "err": e }) };
        if with(cx, |s| s.tab.gn != gn) { return; }
        let err = s(&v, "err");
        with(cx, |st| st.tab.lib_err = if err.is_empty() { String::new() } else { format!("Could not load the factories: {err}") });
        if !err.is_empty() { with(cx, |st| st.tab.lib_loading = false); bump(cx); return; }
        let list = arr(&v["f"], "factories").to_vec();
        let tpls = arr(&v["t"], "templates").to_vec();
        with(cx, |s| s.tab.lib = Some((list, tpls)));
        render_library(cx);
        bump(cx);
    });
}

fn render_library(cx: &mut App) {
    let (lib, busy) = with(cx, |s| (s.tab.lib.clone(), s.tab.busy_tpl.clone()));
    let Some((list, tpls)) = lib else { return };
    let now = model::now_ms();
    let cards: Vec<Card> = list.iter().map(|f| Card {
        id: s(f, "id"), name: s(f, "name"), runnable: b(f, "runnable"),
        chip: if b(f, "runnable") { "Ready".to_string() } else { format!("Draft · {} to fix", n(f, "errorCount") as i64) },
        desc: { let d = s(f, "description"); if d.is_empty() { "No description.".into() } else { d } },
        meta: model::factory_meta(f, now),
    }).collect();
    let tcards: Vec<Tpl> = tpls.iter().map(|t| Tpl {
        id: s(t, "id"), name: s(t, "name"), good_for: s(t, "goodFor"), description: s(t, "description"),
        team: arr(t, "team").iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "), busy: busy == s(t, "id"),
    }).collect();
    with(cx, |s| { s.tab.factories = cards; s.tab.templates = tcards; s.tab.lib_loading = false; });
}

pub(crate) fn run_row(r: &Value, now: f64, earlier: bool) -> RunRow {
    let (chip, cls) = model::status_chip(r);
    RunRow {
        id: s(r, "id"), name: s(r, "factoryName"), project: s(r, "project"), chip, tone: model::chip_tone(&cls).to_string(),
        task: s(r, "task"), meta: if earlier { model::earlier_meta(r, now) } else { model::run_meta(r, now) }, ..Default::default()
    }
}

fn load_runs(cx: &mut App) {
    let gn = with(cx, |s| { s.tab.gn += 1; s.tab.gn });
    get(cx, "/api/factory-runs".into(), move |j, cx| {
        if with(cx, |s| s.tab.gn != gn) { return; }
        let runs = arr(&j, "runs").to_vec();
        let now = model::now_ms();
        let rows: Vec<RunRow> = runs.iter().map(|r| run_row(r, now, false)).collect();
        with(cx, |s| { s.tab.runs = Some(runs); s.tab.run_rows = rows; s.tab.runs_loading = false; });
    });
}

fn load_cleanup(cx: &mut App) {
    let gn = with(cx, |s| { s.tab.gn += 1; s.tab.gn });
    get(cx, "/api/factory-cleanup".into(), move |j, cx| {
        if with(cx, |s| s.tab.gn != gn) { return; }
        let items = arr(&j, "items").to_vec();
        let settings = j.get("settings").cloned().unwrap_or(json!({}));
        let days = (n(&settings, "autoDeleteDays") as i64).to_string();
        with(cx, |s| { s.tab.clean = Some((items, settings)); s.tab.picks.clear(); s.tab.clean_days = days; s.tab.clean_loading = false; });
        render_cleanup(cx);
    });
}

fn render_cleanup(cx: &mut App) {
    let (clean, picks) = with(cx, |s| (s.tab.clean.clone(), s.tab.picks.clone()));
    let Some((items, _)) = clean else { return };
    let rows: Vec<Clean> = items.iter().map(|it| Clean {
        id: s(it, "id"), title: model::cleanup_title(it), meta: model::cleanup_meta(it), why: s(it, "why"), warn: model::cleanup_warn(it),
        size: model::cleanup_size(it), age: model::cleanup_age(it), picked: picks.get(&s(it, "id")).copied().unwrap_or(false),
    }).collect();
    with(cx, |s| s.tab.clean_rows = rows);
}

fn cleanup_delete(cx: &mut App) {
    let (items, picks) = with(cx, |s| (s.tab.clean.clone().map(|c| c.0).unwrap_or_default(), s.tab.picks.clone()));
    let ids: Vec<String> = picks.iter().filter(|(_, v)| **v).map(|(k, _)| k.clone()).collect();
    if ids.is_empty() { parts::toast(cx, "Choose what to delete first."); return; }
    let picked: Vec<&Value> = items.iter().filter(|i| ids.contains(&s(i, "id"))).collect();
    let msg = model::cleanup_confirm(&picked);
    parts::confirm(cx, "Delete", &msg, "Delete", true, move |ok, cx| {
        if !ok { return; }
        post(cx, "/api/factory-cleanup", json!({ "action": "remove", "ids": ids }), |r, cx| {
            if b(&r, "ok") { parts::toast_ok(cx, "Deleted."); } else { parts::toast_error(cx, &parts::err_text(&r, "Some could not be deleted.")); }
            load_cleanup(cx);
        });
    });
}

// ─── The library's actions ───

fn new_factory(cx: &mut App) {
    parts::prompt(cx, "New factory", "Name the new factory", "New factory", "Create", |name, cx| {
        let Some(name) = name else { return };
        let name = if name.trim().is_empty() { "New factory".to_string() } else { name };
        post(cx, "/api/factories", json!({ "action": "create", "name": name }), |r, cx| {
            if !b(&r, "ok") { parts::toast_error(cx, &parts::err_text(&r, "Could not create it.")); return; }
            open_sub("editor", &s(&r["factory"], "id"), cx);
        });
    });
}

fn import_factory(cx: &mut App) {
    crundi_shell::run(cx, || crundi_core::driver::take_pick().or_else(|| rfd::FileDialog::new().add_filter("Factory", &["json"]).pick_file()).and_then(|p| std::fs::read_to_string(p).ok()), |text, cx| {
        let Some(text) = text else { return };
        post(cx, "/api/factories", json!({ "action": "import", "data": text }), |r, cx| {
            if !b(&r, "ok") { parts::toast_error(cx, &parts::err_text(&r, "Could not import it.")); return; }
            parts::toast_ok(cx, &model::import_toast(&r));
            load_library(cx);
        });
    });
}

fn name_of(cx: &mut App, id: &str) -> String {
    with(cx, |st| st.tab.lib.as_ref().and_then(|l| l.0.iter().find(|f| s(f, "id") == id).map(|f| s(f, "name")))).unwrap_or_default()
}

fn export_factory(id: &str, cx: &mut App) {
    let name = name_of(cx, id);
    get(cx, format!("/api/factories/export?id={}", parts::enc(id)), move |r, cx| {
        if !b(&r, "ok") { parts::toast_error(cx, &parts::err_text(&r, "Could not export it.")); return; }
        let text = serde_json::to_string_pretty(&r["data"]).unwrap_or_default();
        let file = model::export_name(&name);
        crundi_shell::run(cx, move || {
            let path = crundi_core::driver::take_pick().or_else(|| rfd::FileDialog::new().set_title(format!("Save {file}")).set_file_name(&file).save_file());
            path.map(|p| std::fs::write(&p, text).map(|_| p.display().to_string()).map_err(|e| e.to_string()))
        }, |res, cx| match res { Some(Ok(p)) => parts::toast_ok(cx, &format!("Saved {p}")), Some(Err(e)) => parts::toast_error(cx, &e), None => {} });
    });
}

fn delete_factory(id: &str, cx: &mut App) {
    let name = name_of(cx, id);
    let id = id.to_string();
    let msg = format!("Delete \"{name}\"? Runs that already started keep their own copy.");
    parts::confirm(cx, "Delete factory", &msg, "Delete", true, move |ok, cx| {
        if !ok { return; }
        post(cx, "/api/factories", json!({ "action": "delete", "id": id }), |r, cx| { if !b(&r, "ok") { parts::toast_error(cx, &parts::err_text(&r, "That did not work.")); } load_library(cx); });
    });
}

fn use_template(id: &str, cx: &mut App) {
    let name = with(cx, |st| { st.tab.busy_tpl = id.to_string(); st.tab.lib.as_ref().and_then(|l| l.1.iter().find(|t| s(t, "id") == id).map(|t| s(t, "name"))).unwrap_or_default() });
    render_library(cx);
    post(cx, "/api/factories", json!({ "action": "fromTemplate", "templateId": id }), move |r, cx| {
        with(cx, |s| s.tab.busy_tpl.clear());
        render_library(cx);
        if !b(&r, "ok") { parts::toast_error(cx, &parts::err_text(&r, "Could not copy it.")); return; }
        parts::toast_ok(cx, &format!("\"{name}\" is in your factories now. Change it however you like."));
        open_sub("editor", &s(&r["factory"], "id"), cx);
    });
}

// ─── The project's Factory panel (mountPanel) ───

pub(crate) fn panel_load(cx: &mut App) {
    let p = crundi_shell::project(cx).to_lowercase();
    let gn = with(cx, |s| { let changed = s.panel.project != p; s.panel.project = p.clone(); if changed { s.panel.active = None; } s.panel.gn += 1; s.panel.asked = true; s.panel.gn });
    if p.is_empty() { with(cx, |s| s.panel.state = "none".into()); return; }
    let path = format!("/api/factory-runs?project={}", parts::enc(&p));
    let tpath = format!("/api/factory-triggers?project={}", parts::enc(&p));
    crundi_shell::call(cx, move |api| {
        let r = api.get(&path).unwrap_or_else(|_| json!({ "runs": [] }));
        let t = api.get(&tpath).unwrap_or_else(|_| json!({ "triggers": [] }));
        Ok(json!({ "r": r, "t": t }))
    }, move |r, cx| {
        let v = r.unwrap_or(Value::Null);
        if with(cx, |s| s.panel.gn != gn) { return; }
        let runs = arr(&v["r"], "runs").to_vec();
        let triggers = arr(&v["t"], "triggers").to_vec();
        let active = runs.iter().find(|r| model::is_active(r)).map(|r| s(r, "id"));
        let was = with(cx, |s| { let w = s.panel.active.clone(); s.panel.runs = runs.clone(); s.panel.triggers = triggers.clone(); s.panel.active = active.clone(); w });
        if let Some(id) = active {
            // A run is going: the panel is its live graph; it keeps itself up to date.
            if was.as_deref() == Some(id.as_str()) && view::is_panel(cx) && view::run_id(cx) == id { bump(cx); return; }
            with(cx, |s| s.panel.state = "live".into());
            view::open(&id, true, cx);
            bump(cx);
            return;
        }
        if was.is_some() && view::is_panel(cx) { view::close(cx); }
        let now = model::now_ms();
        let earlier: Vec<RunRow> = runs.iter().take(8).map(|r| run_row(r, now, true)).collect();
        let trows: Vec<RunRow> = triggers.iter().map(|t| {
            let q = arr(t, "queue").len();
            RunRow {
                id: s(t, "id"), name: s(t, "name"), task: model::trigger_when(t), meta: model::trigger_last(t, now), off: !b(t, "enabled"),
                tag1: if b(t, "enabled") { String::new() } else { "off".into() }, tag2: if q > 0 { format!("{q} waiting") } else { String::new() }, ..Default::default()
            }
        }).collect();
        with(cx, |s| {
            // The web draws the panel anew on each load: Triggers open when there are any.
            s.panel.triggers_open = !trows.is_empty();
            s.panel.earlier = earlier; s.panel.trigger_rows = trows;
            s.panel.triggers_loaded = true;
            s.panel.state = "idle".into();
        });
        bump(cx);
    });
}

// ─── Everything a button does ───

pub(crate) fn act(a: &str, x: &str, y: &str, cx: &mut App) {
    if a.starts_with("v-") { view::act(a, x, y, cx); bump(cx); return; }
    if a.starts_with("e-") { edit::act(a, x, y, cx); bump(cx); return; }
    if a.starts_with("st-") || a.starts_with("pin-") || a.starts_with("tr-") || a.starts_with("chat-") { dlg::act(a, x, y, cx); bump(cx); return; }
    match a {
        "view" => set_view(x, cx),
        "new" => new_factory(cx),
        "import" => import_factory(cx),
        // Start one (the head) or a card's Start: the run's graph after.
        "start-one" => dlg::open_start(dlg::StartOpts { project: String::new(), factory: x.to_string(), then: dlg::Then::Viewer }, cx),
        "edit" => open_sub("editor", x, cx),
        "copy" => post(cx, "/api/factories", json!({ "action": "duplicate", "id": x }), |r, cx| { if !b(&r, "ok") { parts::toast_error(cx, &parts::err_text(&r, "That did not work.")); } load_library(cx); }),
        "export" => export_factory(x, cx),
        "delete" => delete_factory(x, cx),
        "use-template" => use_template(x, cx),
        "open-run" => open_run_in_tab(x, cx),
        "cleanup-delete" => cleanup_delete(cx),
        "sub-back" => close_sub(cx),
        "panel-start" => { let p = with(cx, |s| s.panel.project.clone()); dlg::open_start(dlg::StartOpts { project: p, factory: String::new(), then: dlg::Then::Panel }, cx); }
        "trigger" => { let p = with(cx, |s| s.panel.project.clone()); dlg::open_trigger(&p, x, cx); }
        "earlier-open" => with(cx, |s| s.panel.earlier_open = !s.panel.earlier_open),
        "triggers-open" => with(cx, |s| s.panel.triggers_open = !s.panel.triggers_open),
        "cv-zoom-in" | "cv-zoom-out" | "cv-fit" => { let k = a.trim_start_matches("cv-"); if x == "v" { view::control(cx, k) } else { edit::control(cx, k) } }
        _ => {}
    }
    bump(cx);
}

pub(crate) fn edit_field(form: &str, k: &str, v: &str, cx: &mut App) {
    match form {
        "cleanup" => {
            if let Some(id) = k.strip_prefix("pick:") { with(cx, |s| { s.tab.picks.insert(id.to_string(), v == "1"); }); render_cleanup(cx); return; }
            if k == "days" {
                let days: i64 = v.trim().parse::<i64>().unwrap_or(0).clamp(0, 365);
                let cur = with(cx, |s| s.tab.clean.as_ref().map(|c| n(&c.1, "autoDeleteDays") as i64).unwrap_or(0));
                if days == cur { return; }
                post(cx, "/api/factory-cleanup", json!({ "action": "settings", "autoDeleteDays": days }), move |r, cx| {
                    if b(&r, "ok") { with(cx, |s| if let Some(c) = s.tab.clean.as_mut() { c.1["autoDeleteDays"] = json!(days); }); parts::toast_ok(cx, "Saved."); } else { parts::toast_error(cx, &parts::err_text(&r, "Not saved")); }
                });
            }
        }
        "limits" | "viewer" => view::edit(cx, form, k, v),
        "ed" => edit::edit(k, v, cx),
        _ => dlg::edit(form, k, v, cx),
    }
    bump(cx);
}

// ─── Drawing: the tab ───

/// The tab's page (or a pane's body: `key` is the pane's key, "" in a tab).
pub fn view(_tab: &str, _key: &str, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let sub = rf(fx, |s| s.tab.sub.clone());
    let body = match sub.as_str() {
        "viewer" => view::render(fx, window, cx),
        "editor" => edit::render(fx, window, cx),
        "error" => div().flex().flex_col().p(T::SP_16).gap(T::SP_10)
            .child(note("err", rf(fx, |s| s.tab.sub_error.clone())))
            .child(div().child(btn("fx-sub-back", "Back", None, "", true, false).on_click(|_, _, cx| act("sub-back", "", "", cx)))).into_any_element(),
        _ => tab_page(fx, window, cx),
    };
    div().flex().flex_col().size_full().min_h_0().bg(T::bg()).child(body)
        .children(dlg::render(fx, window, cx))
        .into_any_element()
}

fn lib_cols(p: &parts::P) -> usize {
    let w = parts::bounds_of(p, "fx-lib").map(|b| f32::from(b.size.width)).unwrap_or(900.0);
    let (gap, min) = (f32::from(T::SP_12), f32::from(T::FACTORIES_CARD_MIN));
    (((w + gap) / (min + gap)).floor() as usize).max(1)
}

fn grid(cols: usize, cells: Vec<AnyElement>) -> Div {
    let mut out = div().flex().flex_col().gap(T::SP_12).w_full();
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let mut row = div().flex().gap(T::SP_12).w_full();
        for i in 0..cols {
            let c = cells.next();
            row = row.child(div().flex().flex_col().flex_1().min_w_0().when_some(c, |d, c| d.child(c)));
            let _ = i;
        }
        out = out.child(row);
    }
    out
}

fn factory_card(i: usize, f: &Card) -> AnyElement {
    let id = f.id.clone();
    let a = |what: &'static str, id: String| move |_: &ClickEvent, _: &mut Window, cx: &mut App| act(what, &id, "", cx);
    w::card().flex_1().min_w_0().overflow_hidden().px(T::SP_16).py(T::SP_12).gap(T::SP_8)
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).child(f.name.clone()))
            .child(w::tag(f.chip.clone(), if f.runnable { "ok" } else { "warn" })))
        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).line_clamp(2).text_ellipsis().child(f.desc.clone()))
        .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).truncate().child(f.meta.clone()))
        .child(div().flex_1())
        .child(div().flex().flex_wrap().gap(T::SP_6)
            .child(btn(("fx-edit", i), "Edit", Some(IconName::Pencil), "primary", true, false).on_click(a("edit", id.clone())))
            .child(btn(("fx-start", i), "Start", Some(IconName::Play), "", f.runnable, false).on_click(a("start-one", id.clone())))
            .child(btn(("fx-copy", i), "Copy", Some(IconName::Copy), "ghost", true, false).on_click(a("copy", id.clone())))
            .child(btn(("fx-export", i), "Export", Some(IconName::Download), "ghost", true, false).on_click(a("export", id.clone())))
            .child(btn(("fx-delete", i), "Delete", Some(IconName::Trash), "ghost-danger", true, false).on_click(a("delete", id))))
        .into_any_element()
}

fn template_card(i: usize, t: &Tpl) -> AnyElement {
    let id = t.id.clone();
    w::card().flex_1().min_w_0().overflow_hidden().px(T::SP_16).py(T::SP_12).gap(T::SP_8)
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).child(t.name.clone()))
            .child(w::tag("Template", "")))
        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).line_clamp(3).text_ellipsis().child(t.good_for.clone()))
        .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(t.description.clone()))
        .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(format!("Team: Manager, {}", t.team)))
        .child(div().flex_1())
        .child(div().child(btn(("fx-use", i), "Use this", Some(IconName::Copy), "primary", !t.busy, false).on_click(move |_, _, cx| act("use-template", &id, "", cx))))
        .into_any_element()
}

/// A run row (.fx-run): name (and project), status chip, task, meta; a trigger row too.
pub(crate) fn run_row_view(id: impl Into<ElementId>, r: &RunRow, with_project: bool, on: impl Fn(&mut App) + 'static) -> AnyElement {
    // flex_none: in the scrolling column a row keeps its height (shrunk to fit, its text was clipped).
    w::card().id(id).flex_none().min_w_0().overflow_hidden().px(T::SP_12).py(T::SP_10).gap(T::SP_6).cursor_pointer().hover(|d| d.border_color(T::accent()))
        .when(r.off, |d| d.opacity(T::FACTORIES_O_SOFT))
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().flex().flex_1().min_w_0().items_center().gap(T::SP_4)
                .child(div().truncate().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).child(r.name.clone()))
                .when(with_project, |d| d.child(div().truncate().text_size(T::TEXT_11).text_color(T::text_muted()).child(format!("in {}", r.project))))
                .when(!r.tag1.is_empty(), |d| d.child(w::tag(r.tag1.clone(), "")))
                .when(!r.tag2.is_empty(), |d| d.child(w::tag(r.tag2.clone(), ""))))
            .when(!r.chip.is_empty(), |d| d.child(w::tag(r.chip.clone(), parts::tone(&r.tone)))))
        .when(!r.task.is_empty(), |d| d.child(div().truncate().text_size(T::TEXT_12).text_color(T::text_secondary()).child(r.task.clone())))
        .when(!r.meta.is_empty(), |d| d.child(div().truncate().text_size(T::TEXT_11).text_color(T::text_muted()).child(r.meta.clone())))
        .on_click(move |_, _, cx| on(cx))
        .into_any_element()
}

fn tab_page(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let view = rf(fx, |s| s.tab.view.clone());
    let narrow = parts::bounds_of(&fx.p, "fx-tab").map(|b| b.size.width < T::FACTORIES_NARROW).unwrap_or(false);
    let pad = if narrow { T::SP_10 } else { T::SP_16 };
    let seg_on = match view.as_str() { "runs" => 1, "cleanup" => 2, _ => 0 };
    let head = div().flex().flex_none().items_center().gap(T::SP_8).px(pad).py(T::SP_10).border_b_1().border_color(T::border())
        .child(div().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).child("Factories"))
        .child(w::segmented("fx-seg", &["Library", "Runs", "Cleanup"], seg_on, |i, _, cx| act("view", match i { 1 => "runs", 2 => "cleanup", _ => "library" }, "", cx)))
        .child(div().flex_1())
        .when(view == "library", |d| d
            .child(btn("fx-start-one", "Start one", Some(IconName::Play), "", true, narrow).on_click(|_, _, cx| act("start-one", "", "", cx)))
            .child(btn("fx-import", "Import", Some(IconName::Upload), "ghost", true, narrow).on_click(|_, _, cx| act("import", "", "", cx)))
            .child(btn("fx-new", "New factory", Some(IconName::Plus), "primary", true, narrow).on_click(|_, _, cx| act("new", "", "", cx))));
    let mut body = div().id("fx-body").relative().flex().flex_col().flex_1().min_h_0().overflow_y_scroll().p(pad).gap(T::SP_12);
    match view.as_str() {
        "runs" => {
            let (loading, rows) = rf(fx, |s| (s.tab.runs_loading, s.tab.run_rows.clone()));
            body = body.gap(T::SP_8);
            if !loading && rows.is_empty() { body = body.child(note("note", "No runs yet. Start a factory from the Library, or from a project's Factory panel.")); }
            for (i, r) in rows.iter().enumerate() { let id = r.id.clone(); body = body.child(run_row_view(("fx-run", i), r, true, move |cx| act("open-run", &id, "", cx))); }
        }
        "cleanup" => {
            if rf(fx, |s| s.tab.clean_loading) { body = body.child(note("note", "Looking for leftovers…")); } else { body = body.gap(T::SP_10).child(cleanup_body(fx, window, cx)); }
        }
        _ => {
            let (loading, cards, tpls, err) = rf(fx, |s| (s.tab.lib_loading, s.tab.factories.clone(), s.tab.templates.clone(), s.tab.lib_err.clone()));
            if !err.is_empty() {
                let body = body.child(w::error_state("fx-retry", err, |_, _, cx| { with(cx, |s| { s.tab.lib_err.clear(); s.tab.lib_loading = true; }); load_library(cx); bump(cx); }));
                return div().relative().flex().flex_col().size_full().min_h_0()
                    .child(parts::probe(cx.entity(), "fx-tab", |_, _| {}))
                    .child(head).child(body).into_any_element();
            }
            let cols = lib_cols(&fx.p);
            body = body.child(div().relative().w_full().h(px(1.)).child(parts::probe(cx.entity(), "fx-lib", |_, _| {})));
            if loading { body = body.child(note("note", "Loading…")); }
            if !loading && cards.is_empty() {
                body = body.child(note("note", "No factories of your own yet. A factory is a team: a Manager who owns the change request and employees with personas, wired together as a flow of steps and decisions. Start from one of the templates below, build your own with New factory, or ask any Claude chat to build one for you."));
            }
            body = body.child(grid(cols, cards.iter().enumerate().map(|(i, c)| factory_card(i, c)).collect()));
            if !loading && !tpls.is_empty() {
                body = body.child(div().flex().flex_col().gap(T::SP_2).pt(T::SP_8)
                    .child(div().text_size(T::TEXT_14).font_weight(T::W_BOLD).child("Templates"))
                    .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Modelled on how real teams work. Use one as it is, copy it and change it to fit how your team works, or build your own from scratch.")));
                body = body.child(grid(cols, tpls.iter().enumerate().map(|(i, t)| template_card(i, t)).collect()));
            }
        }
    }
    div().relative().flex().flex_col().size_full().min_h_0()
        .child(parts::probe(cx.entity(), "fx-tab", |_, _| {}))
        .child(head).child(body).into_any_element()
}

fn cleanup_body(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let (rows, days) = rf(fx, |s| (s.tab.clean_rows.clone(), s.tab.clean_days.clone()));
    let mut d = div().flex().flex_col().gap(T::SP_10)
        .child(note("note", "Merging and discarding clean up after themselves. What is listed here was kept: stopped runs, kept branches, and folders no run knows about. Nothing is deleted unless you choose to."))
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Delete leftovers older than"))
            .child(div().w(T::FACTORIES_DAYS_W).child(parts::line(&mut fx.p, "cleanup|days", &days, "", false, false, parts::Commit::Leave, window, cx, |v, cx| edit_field("cleanup", "days", &v, cx))))
            .child(div().flex_1().text_size(T::TEXT_12).text_color(T::text_muted()).child("days automatically (0 = never; never one with uncommitted work, never a kept branch).")));
    if rows.is_empty() { return d.child(note("note", "Nothing to clean up.")).into_any_element(); }
    let colh = |t: &'static str| div().text_size(T::TEXT_11).text_color(T::text_muted()).font_weight(T::W_MEDIUM).child(t);
    let mut tbl = div().flex().flex_col()
        .child(div().flex().gap(T::SP_12).py(T::SP_8).child(div().w(T::SP_16)).child(colh("What").flex_1()).child(colh("Size").w(T::SP_48 + T::SP_24)).child(colh("Age").w(T::SP_40)))
        .child(w::rule());
    for (i, it) in rows.iter().enumerate() {
        let id = it.id.clone();
        tbl = tbl.child(div().flex().gap(T::SP_12).py(T::SP_8)
            .child(div().pt(T::SP_2).w(T::SP_16).child(w::check(("fx-pick", i), "", it.picked, move |v, _, cx| edit_field("cleanup", &format!("pick:{id}"), if *v { "1" } else { "0" }, cx))))
            .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
                .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).child(it.title.clone()))
                .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(it.meta.clone()))
                .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(it.why.clone()))
                .when(!it.warn.is_empty(), |d| d.child(div().pt(T::SP_4).child(note("warn", it.warn.clone())))))
            .child(div().w(T::SP_48 + T::SP_24).text_size(T::TEXT_12).child(it.size.clone()))
            .child(div().w(T::SP_40).text_size(T::TEXT_12).child(it.age.clone())))
            .child(w::rule());
    }
    d = d.child(tbl).child(div().child(btn("fx-clean-del", "Delete selected", Some(IconName::Trash), "danger", true, false).on_click(|_, _, cx| act("cleanup-delete", "", "", cx))));
    d.into_any_element()
}

// ─── Drawing: the panel ───

fn panel_view(_key: &str, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let (state, asked) = rf(fx, |s| (s.panel.state.clone(), s.panel.asked));
    // Drawn before anyone loaded it (added while on show): load now, after this frame.
    if !asked { rf(fx, |s| s.panel.asked = true); cx.defer(|cx| panel_load(cx)); }
    let body = match state.as_str() {
        "live" => view::render(fx, window, cx),
        "idle" => panel_idle(fx),
        "none" => w::empty("Open a project first.").into_any_element(),
        _ => w::loading("Loading…").into_any_element(),
    };
    div().flex().flex_col().size_full().min_h_0().bg(T::bg()).child(body)
        .children(dlg::render(fx, window, cx))
        .into_any_element()
}

fn panel_idle(fx: &mut Fx) -> AnyElement {
    let p = rf(fx, |s| (s.panel.earlier.clone(), s.panel.earlier_open, s.panel.trigger_rows.clone(), s.panel.triggers_open, s.panel.triggers_loaded));
    let (earlier, eopen, triggers, topen, tloaded) = p;
    let mut d = div().id("fx-panel").flex().flex_col().size_full().min_h_0().overflow_y_scroll().p(T::SP_12).gap(T::SP_10)
        .child(note("note", "A factory is a team of Claude employees, run by a Manager, that takes a large piece of work from start to finish in its own worktree. One runs at a time per project."))
        .child(div().child(btn("fx-panel-start", "Start a factory", Some(IconName::Play), "primary", true, false).on_click(|_, _, cx| act("panel-start", "", "", cx))));
    if !earlier.is_empty() {
        d = d.child(w::disclosure("fx-earlier", format!("Earlier runs ({})", earlier.len()), eopen).on_click(|_, _, cx| act("earlier-open", "", "", cx)));
        if eopen {
            let mut l = div().flex().flex_col().gap(T::SP_8);
            for (i, r) in earlier.iter().enumerate() { let id = r.id.clone(); l = l.child(run_row_view(("fx-earlier-run", i), r, false, move |cx| act("open-run", &id, "", cx))); }
            d = d.child(l);
        }
    }
    if tloaded {
        let t = format!("Triggers{}: start a run by itself", if triggers.is_empty() { String::new() } else { format!(" ({})", triggers.len()) });
        d = d.child(div().pt(T::SP_2).child(w::disclosure("fx-triggers", t, topen).on_click(|_, _, cx| act("triggers-open", "", "", cx))));
        if topen {
            let mut l = div().flex().flex_col().gap(T::SP_8);
            if triggers.is_empty() { l = l.child(note("note", "Start a factory at a time of day, when a kanban card enters a column, or when a webhook is called (CI, an issue tracker). The change request is filled in from the card or the call.")); }
            for (i, r) in triggers.iter().enumerate() { let id = r.id.clone(); l = l.child(run_row_view(("fx-trigger", i), r, false, move |cx| act("trigger", &id, "", cx))); }
            l = l.child(div().child(btn("fx-add-trigger", "Add a trigger", Some(IconName::Plus), "", true, false).on_click(|_, _, cx| act("trigger", "", "", cx))));
            d = d.child(l);
        }
    }
    d.into_any_element()
}

// ─── The driver ───

/// The driver's "planning:<name>" actions this screen answers (the Slint client's, prefixed "factories-").
pub fn test_ui(_: &Stores, name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let name = name.strip_prefix("factories-")?;
    let r = match name {
        // Any button: factories-act <what> <id> <more>.
        "act" => { act(&a(0), &a(1), &a(2), cx); json!({ "ok": true }) }
        "edit" => { edit_field(&a(0), &a(1), &a(2), cx); json!({ "ok": true }) }
        "node-click" => { if a(0) == "v" { view::node_click(&a(1), cx); } else { edit::node_down(cx, &a(1)); edit::node_up(&a(1), false, cx); } json!({ "ok": true }) }
        "node-move" => { edit::node_down(cx, &a(0)); let z = edit::zoom(cx); edit::node_drag(cx, &a(0), a(1).parse::<f32>().unwrap_or(0.0) * z, a(2).parse::<f32>().unwrap_or(0.0) * z); edit::node_up(&a(0), true, cx); json!({ "ok": true }) }
        "connect" => json!({ "ok": edit::connect_ids(&a(0), &a(1), &a(2), cx) }),
        "edge-click" => { edit::edge_click(cx, &a(0)); json!({ "ok": true }) }
        "key" => { edit::key(&a(0), cx); json!({ "ok": true }) }
        "pane" => { if a(0) == "v" { view::pane_click(cx) } else { edit::pane_click(cx) } json!({ "ok": true }) }
        "badge" => json!({ "ok": true, "badge": with(cx, |s| s.badge.clone()) }),
        // GPUI: the confirm and the prompt (the widgets' ask) answered here
        // until the driver's dialog-answer reaches them (shared/requests.md).
        "dialog" => { let ok = a(0) == "true"; let v = a(1); json!({ "ok": w::ask_answer(ok, (!v.is_empty()).then_some(v), window, cx) }) }
        _ => return None,
    };
    bump(cx);
    Some(r)
}

/// This screen's props (props.planningArea.factories, the Slint client's areas.planning.factories).
pub fn state(_: &Stores, _window: &mut Window, cx: &mut App) -> Value {
    let runrow = |r: &RunRow| json!({ "id": r.id, "name": r.name, "project": r.project, "chip": r.chip, "tone": r.tone, "task": r.task, "meta": r.meta, "off": r.off, "tags": [r.tag1, r.tag2] });
    let modal = dlg::modal_open(cx) || w::ask_props(cx) != Value::Null;
    let cols = parts::pwith(cx, |p| lib_cols(p));
    let f = with(cx, |s| json!({
        "view": s.tab.view, "sub": s.tab.sub, "subError": s.tab.sub_error, "badge": s.badge,
        "library": {
            "loading": s.tab.lib_loading, "error": s.tab.lib_err, "empty": s.tab.factories.is_empty(), "cols": cols,
            "factories": s.tab.factories.iter().map(|c| json!({ "id": c.id, "name": c.name, "runnable": c.runnable, "chip": c.chip, "desc": c.desc, "meta": c.meta })).collect::<Vec<_>>(),
            "templates": s.tab.templates.iter().map(|c| json!({ "id": c.id, "name": c.name, "team": c.team })).collect::<Vec<_>>(),
        },
        "runs": { "loading": s.tab.runs_loading, "items": s.tab.run_rows.iter().map(runrow).collect::<Vec<_>>() },
        "cleanup": { "loading": s.tab.clean_loading, "days": s.tab.clean_days, "items": s.tab.clean_rows.iter().map(|c| json!({ "id": c.id, "title": c.title, "meta": c.meta, "size": c.size, "age": c.age, "picked": c.picked, "warn": c.warn })).collect::<Vec<_>>() },
        "panel": { "state": s.panel.state, "earlier": s.panel.earlier.iter().map(runrow).collect::<Vec<_>>(), "earlierOpen": s.panel.earlier_open, "triggers": s.panel.trigger_rows.iter().map(runrow).collect::<Vec<_>>(), "triggersOpen": s.panel.triggers_open },
    }));
    let mut f = f;
    f["viewer"] = view::props(cx);
    f["editor"] = edit::props(cx);
    f["dialogs"] = dlg::props(cx);
    f["pointerModal"] = json!(modal);
    // The open confirm or prompt (the Slint client's props.dialog), until the driver carries it.
    f["dialog"] = w::ask_props(cx);
    json!({ "factories": f })
}
