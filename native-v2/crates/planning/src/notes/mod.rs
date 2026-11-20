//! Notes: the tab and the Notes panes (app/vendor/crundi-notes.js:
//! mountManager, mountPicker, mountEditor; their host in src/webapp-html.js:
//! loadNotesTab, mountNotesCell, notesBusyIds, the text size, the SSE `notes`
//! event), ported from native-gpui's planning/notes.rs with its rules as they
//! were. doc.rs is the page model and the pure rules (with their unit tests);
//! view.rs draws it.
//!
//! One place shows a page: the tab (key "") or a workbench pane (its key,
//! panel:notes:<n>). A page is open in one place at a time ("one page, one
//! pane"). Each place has its own state here, keyed by its key.
//!
//! On v2's rules (ARCHITECTURE.md): the state lives in one entity (`S`, the
//! store; a Global holds only its handle, read in handlers). A change
//! notifies only the view of the place it touched (`render(key)`); the views
//! read the store through the handle they hold. The tab's view is a layout
//! (its bar, the page list and the page are cached leaves side by side); a
//! pane's body places its page's view cached. A page's blocks, the page list
//! and a pane's picker are virtualised lists (only what is on screen is
//! drawn), spliced for the blocks an edit touched.

#[allow(dead_code)]
pub mod doc;
mod view;

use crate::util;
use crundi_core::live::Event;
use crundi_shell::Stores;
use crundi_widgets as w;
use doc::{Block, Mark, Run};
use gpui::*;
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, rc::Rc, time::{Duration, Instant}};

const FONT_BASE: i32 = 15;

#[derive(Default)]
struct Ed {
    page_id: String,
    version: i64,
    title: String,
    blocks: Vec<Block>,
    loaded: bool,
    dirty: bool,
    saving: bool,
    save_gen: u64,
    load_gen: u64,
    /// The block selection: its anchor and focus blocks.
    sel: Option<(String, String)>,
    /// The block (and table cell) with the caret.
    focus: Option<String>,
    cell: (usize, usize),
    save_state: String,
    save_text: String,
    save_tip: String,
}

/// One place: the tab ("") or a pane.
struct Place {
    key: String,
    /// A pane's page (None: the picker). The tab's is `tab.current`.
    page: Option<String>,
    title: String,
    mode: String,
    msg: String,
    ed: Option<Ed>,
    fresh: bool,
    // The picker's list.
    pages: Vec<Value>,
    search: String,
    list_msg: String,
    new_busy: bool,
    // What the view is told: a focus request (block, cell row and column,
    // the selection), bumped each time; the title to show again.
    focus: (String, i32, i32, i32, i32),
    focus_gen: i32,
    title_gen: i32,
    drop_y: Option<f32>,
    cur_id: String,
    focus_n: u64,
    /// Each block's top and height (window px), from the last frame.
    boxes: Vec<(String, f32, f32)>,
    /// The page's box (x, y, w, h), from the last frame.
    page_box: (f32, f32, f32, f32),
    /// Each block's "+" (its centre, window px), from the last frame (the driver).
    plus: Vec<(String, f32, f32)>,
}

/// The tab's list (mountManager).
#[derive(Default)]
struct Tab {
    open: bool,
    project: String,
    view: String,
    pages: Vec<Value>,
    search: String,
    current: Option<String>,
    list_msg: String,
    new_busy: bool,
    list_gen: u64,
    /// With no page open: the first page the server said was created, which
    /// opens when the list arrives (the web's first reload, made
    /// deterministic: a second page made a moment later doesn't take its place).
    arrived: Option<String>,
}

/// What floats over the page (one at a time, as the web's `floating`).
#[derive(Clone)]
enum Float {
    Slash { key: String, id: String, start: Option<usize>, query: String },
    Block { key: String, id: String },
    Lang { key: String, id: String, query: String },
    Item { id: String },
    Fmt { key: String, id: String, r: i32, c: i32, a: usize, b: usize, link: bool },
}

impl Float {
    /// The place it floats over (a page's menu: the tab's).
    fn key(&self) -> &str {
        match self { Float::Slash { key, .. } | Float::Block { key, .. } | Float::Lang { key, .. } | Float::Fmt { key, .. } => key, Float::Item { .. } => "" }
    }
}

#[derive(Clone)]
struct Drag { key: String, ids: Vec<String>, spot: Option<(usize, bool)> }

/// A menu's row (.nt-menu button): its key box, name, hint, a tick.
#[derive(Clone, Default)]
pub(crate) struct Mi { pub id: String, pub k: String, pub name: String, pub d: String, pub check: bool, pub danger: bool, pub sep: bool }

/// The menu on show (one at a time).
#[derive(Clone, Default)]
pub(crate) struct MenuUi { pub open: bool, pub x: f32, pub y: f32, pub label: String, pub items: Vec<Mi>, pub act: i32, pub query_on: bool, pub query: String, pub empty: String, pub gen_n: u64 }

/// The format bar on show.
#[derive(Clone, Default)]
pub(crate) struct FmtUi { pub open: bool, pub x: f32, pub y: f32, pub link: bool, pub link_value: String, pub has_link: bool, pub gen_n: u64 }

/// A page in a list (.nt-item): `meta` the "Edited 5m ago" line, `busy`
/// open in another pane, `cur` the page open in the tab.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Item { pub id: String, pub title: String, pub preview: String, pub meta: String, pub busy: bool, pub cur: bool }

/// A block as drawn: the block, its number, in the selection (and the ends
/// of that run), its grip's menu open.
#[derive(Clone, Hash)]
pub(crate) struct BlockRow { pub b: Block, pub num: String, pub sel: bool, pub sel_top: bool, pub sel_bottom: bool, pub menu_open: bool }

/// One place as the view draws it (the Slint PlanningNotesPane).
#[derive(Clone, Default)]
pub(crate) struct PlaceRow {
    pub key: String, pub mode: String, pub msg: String, pub page_id: String, pub title: String, pub title_gen: i32,
    pub save_state: String, pub save_text: String, pub save_tip: String,
    pub blocks: Vec<BlockRow>, pub sel_count: usize,
    pub focus: (String, i32, i32, i32, i32), pub focus_gen: i32, pub cur_id: String, pub drop_y: Option<f32>,
    pub items: Vec<Item>, pub list_msg: String, pub new_busy: bool,
}

#[derive(Default)]
pub(crate) struct S {
    client_id: String,
    project: String,
    font: i32,
    places: Vec<Place>,
    tab: Tab,
    tab_project: bool,
    float: Option<Float>,
    menu_ids: Vec<String>,
    menu: MenuUi,
    fmt: FmtUi,
    drag: Option<Drag>,
    drag_label: String,
    press: Option<(String, String, bool)>,
    caret_xy: (f32, f32),
    /// Where the caret was when nothing was selected (the selection's
    /// other end: the format bar centres between the two).
    sel_anchor: (f32, f32),
    hint_at: Option<Instant>,
    last_tab: String,
    seen: HashSet<String>,
    /// The last text this screen put on the clipboard (the driver's state).
    clipboard: String,
    /// The pane header carries the Pages and text size buttons (no stand-in row).
    head_hosted: bool,
    ready: bool,
    /// The views that draw a place (by its key), the tab's list and bar.
    views: HashMap<String, Entity<view::PlaceView>>,
    side: Option<Entity<view::SideView>>,
    bar: Option<Entity<view::TabBar>>,
    /// The workbench is watched (panes closed there let their page go).
    wb_watch: Option<Subscription>,
}

struct Handle(Entity<S>);
impl Global for Handle {}

/// The store (handlers only; a view holds its handle).
pub(crate) fn store(cx: &App) -> Entity<S> { cx.global::<Handle>().0.clone() }
fn with<R>(cx: &mut App, f: impl FnOnce(&mut S) -> R) -> R { store(cx).update(cx, |s, _| f(s)) }
fn rd<R>(cx: &App, f: impl FnOnce(&S) -> R) -> R { f(store(cx).read(cx)) }

fn place_new(key: &str) -> Place {
    Place {
        key: key.into(), page: None, title: String::new(), mode: if key.is_empty() { "empty".into() } else { "picker".into() }, msg: String::new(), ed: None, fresh: false,
        pages: vec![], search: String::new(), list_msg: String::new(), new_busy: false,
        focus: (String::new(), -1, -1, 0, 0), focus_gen: 0, title_gen: 0, drop_y: None, cur_id: String::new(), focus_n: 0,
        boxes: vec![], page_box: (0.0, 0.0, 0.0, 0.0), plus: vec![],
    }
}

// ─── Small helpers ───

fn jstr(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn truthy(v: &Value) -> bool { v.as_bool().unwrap_or(false) }
fn project(cx: &App) -> String { crundi_shell::project(cx) }
fn enc(t: &str) -> String { util::enc(t) }
fn now() -> f64 { crundi_core::time::now_secs() }

/// Redraw the place that changed (the tab's: its list too): only its view
/// is notified (rule 2).
fn render(key: &str, cx: &mut App) {
    let ids: Vec<EntityId> = rd(cx, |s| {
        let mut v: Vec<EntityId> = s.views.get(key).map(|e| e.entity_id()).into_iter().collect();
        if key.is_empty() { v.extend(s.side.as_ref().map(|e| e.entity_id())); }
        v
    });
    for id in ids { cx.notify(id); }
}

/// Redraw every place (the text size, a page's busy state in the pickers).
fn render_all(cx: &mut App) {
    let ids: Vec<EntityId> = rd(cx, |s| s.views.values().map(|e| e.entity_id()).chain(s.side.as_ref().map(|e| e.entity_id())).chain(s.bar.as_ref().map(|e| e.entity_id())).collect());
    for id in ids { cx.notify(id); }
}

/// Redraw the place a menu or the format bar floats over.
fn render_float(cx: &mut App) {
    if let Some(k) = rd(cx, |s| s.float.as_ref().map(|f| f.key().to_string())) { render(&k, cx); }
}

/// Run `f` after `ms` on the UI thread.
fn after(cx: &mut App, ms: u64, f: impl FnOnce(&mut App) + 'static) {
    let t = cx.background_executor().timer(Duration::from_millis(ms));
    cx.spawn(async move |cx: &mut AsyncApp| { t.await; let _ = cx.update(f); }).detach();
}

fn toast(cx: &mut App, text: &str) { util::toast(cx, text, w::Toast::Info); }
fn toast_error(cx: &mut App, text: &str) { util::toast(cx, text, w::Toast::Error); }

fn confirm(cx: &mut App, title: &str, message: &str, ok: &str, danger: bool, then: impl Fn(&mut App) + 'static) {
    let c = w::Confirm { title: title.to_string().into(), message: message.to_string().into(), ok: ok.to_string().into(), danger };
    let then = Rc::new(then);
    cx.defer(move |cx| util::in_window(cx, move |window, cx| w::confirm(window, cx, c, move |_, cx| then(cx))));
}

use util::prefs;

// ─── The registry's functions ───

/// At start: the store, the workbench pane kind (panel:notes:<n>), the keys.
pub fn init(cx: &mut App) {
    let font = prefs::get("crundi_notes_font").parse::<i32>().ok().filter(|f| (11..=26).contains(f)).unwrap_or(FONT_BASE);
    let st = cx.new(|_| {
        let mut s = S { client_id: doc::gen_id(), font, ready: true, ..Default::default() };
        s.tab.view = "pages".into();
        s.places.push(place_new(""));
        s
    });
    cx.set_global(Handle(st));
    crundi_workbench::register_kind(cx, crundi_workbench::PaneKind {
        prefix: "panel:notes",
        chat: false,
        make: Rc::new(|key, _, window, cx| {
            watch_workbench(cx);
            let v = view::place_view(key, window, cx);
            let (v2, v3) = (v.clone(), v.clone());
            let k = key.to_string();
            let st = store(cx);
            crundi_workbench::PaneParts {
                body: Rc::new(move |_| div().size_full().child(v2.clone().cached(StyleRefinement::default().size_full())).into_any_element()),
                info: Rc::new(move |cx| crundi_workbench::HeadInfo { title: pane_title_in(st.read(cx), &k), long: String::new(), state: "idle".into() }),
                // The head's title follows its page's (its view is notified when the place changes).
                watch: Box::new(move |cx| vec![cx.observe(&v3, |h, _, cx| h.refresh(cx))]),
            }
        }),
        close: None,
        zoom: None,
    });
    // The editor's keys come before the text box's own.
    cx.intercept_keystrokes(|e, window, cx| {
        let name = view::key_name(&e.keystroke);
        if name.is_empty() { return; }
        if view::key(&name, window, cx) { cx.stop_propagation(); }
    }).detach();
}

/// Panes closed in the workbench: their page is saved and let go (on the
/// workbench's change, no timer).
fn watch_workbench(cx: &mut App) {
    if rd(cx, |s| s.wb_watch.is_some()) { return; }
    let Some(wb) = crundi_workbench::workbench(cx) else { return };
    let sub = cx.observe(&wb, |_, cx| cx.defer(prune));
    with(cx, |s| s.wb_watch = Some(sub));
}

/// The Notes tab's view (a layout: the bar, the list and the page, cached).
pub fn tab(_: &Stores, window: &mut Window, cx: &mut App) -> AnyView {
    view::tab_view(window, cx).into()
}

/// A Notes pane for the workbench ("+" and an empty pane's chip): a new
/// key each time (panel:notes:<n>, one more than the highest).
pub fn add_pane(cx: &mut App) {
    let Some(wb) = crundi_workbench::workbench(cx) else { return };
    let keys = wb.update(cx, |w, _| w.keys());
    let n = keys.iter().filter_map(|k| k.strip_prefix("panel:notes:").and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0) + 1;
    crate::place(cx, &format!("panel:notes:{n}"));
}

pub fn on_batch(_: &Stores, batch: &[Event], cx: &mut App) {
    for e in batch {
        match e {
            Event::Server(n, v) if n == "notes" => on_notes(v, cx),
            Event::Connected(true) => {
                let keys: Vec<String> = rd(cx, |s| s.places.iter().filter(|p| !p.key.is_empty() && p.ed.is_none()).map(|p| p.key.clone()).collect());
                for k in keys { reload_picker(&k, cx); }
                if rd(cx, |s| s.tab.open) { tab_reload(cx); }
            }
            _ => {}
        }
    }
}

fn on_notes(v: &Value, cx: &mut App) {
    let p = project(cx).to_lowercase();
    if p.is_empty() || jstr(v, "project").to_lowercase() != p { return; }
    let keys: Vec<(String, bool)> = rd(cx, |s| s.places.iter().map(|x| (x.key.clone(), x.ed.is_some())).collect());
    for (k, has_ed) in keys {
        if k.is_empty() { continue; }
        if has_ed { remote_change(&k, v, cx); } else { reload_picker(&k, cx); }
    }
    if rd(cx, |s| s.tab.open) {
        if jstr(v, "action") == "create" && !jstr(v, "id").is_empty() {
            let id = jstr(v, "id");
            with(cx, |s| if s.tab.current.is_none() && s.tab.arrived.is_none() { s.tab.arrived = Some(id); });
        }
        remote_change("", v, cx);
        // Our own typing saves often: refresh the list once it settles.
        let mine = jstr(v, "by") == rd(cx, |s| s.client_id.clone()) && jstr(v, "action") == "save";
        if mine {
            let g = with(cx, |s| { s.tab.list_gen += 1; s.tab.list_gen });
            after(cx, 1500, move |cx| { if rd(cx, |s| s.tab.list_gen == g) { tab_reload(cx); } });
        } else { tab_reload(cx); }
    }
}

pub fn on_project(_: &Stores, alias: &str, cx: &mut App) {
    let changed = with(cx, |s| { let c = s.project != alias; s.project = alias.to_string(); c });
    if !changed { return; }
    // Save what waits, then every place starts again for the new project.
    let keys: Vec<String> = rd(cx, |s| s.places.iter().map(|p| p.key.clone()).collect());
    for k in &keys { flush(k, cx); }
    close_float(cx);
    close_tab(cx);
    with(cx, |s| for p in s.places.iter_mut() { let key = p.key.clone(); *p = place_new(&key); });
    for k in keys.iter().filter(|k| !k.is_empty()) {
        if let Some((id, title)) = saved_pane(k, cx) {
            with(cx, |s| if let Some(i) = idx(s, k) { s.places[i].page = Some(id.clone()); s.places[i].title = title; });
            open_editor(k, &id, false, cx);
        } else {
            reload_picker(k, cx);
        }
    }
    if util::current_tab(cx) == "notes" { load_tab(cx); }
    with(cx, |s| s.tab_project = !alias.is_empty());
    render_all(cx);
}

pub fn on_signed_in(stores: &Stores, cx: &mut App) { let p = project(cx); on_project(stores, &p, cx); }

pub fn on_signed_out(_: &Stores, cx: &mut App) {
    close_float(cx);
    with(cx, |s| {
        for p in s.places.iter_mut() { let k = p.key.clone(); *p = place_new(&k); }
        s.tab = Tab { view: "pages".into(), ..Default::default() };
        s.project.clear();
        s.drag = None;
    });
    render_all(cx);
}

/// The project tab on show changed (lib.rs's tab watch).
pub fn on_tab(tab: &str, cx: &mut App) {
    let was = with(cx, |s| std::mem::replace(&mut s.last_tab, tab.to_string()));
    if tab == "notes" { load_tab(cx); }
    else if was == "notes" { close_tab(cx); }
    if tab == "workbench" { prune(cx); }
}

// ─── Places ───

fn idx(s: &S, key: &str) -> Option<usize> { s.places.iter().position(|p| p.key == key) }

fn panes_key(cx: &App) -> String { format!("notes_panes_{}", project(cx).to_lowercase()) }

fn saved_pane(key: &str, cx: &App) -> Option<(String, String)> {
    let v: Value = serde_json::from_str(&prefs::get(&panes_key(cx))).unwrap_or(Value::Null);
    let p = v.get(key)?;
    let id = jstr(p, "id");
    (!id.is_empty()).then(|| (id, jstr(p, "title")))
}

fn save_pane(key: &str, page: Option<(&str, &str)>, cx: &App) {
    if key.is_empty() { return; }
    let k = panes_key(cx);
    let mut v: Value = serde_json::from_str(&prefs::get(&k)).unwrap_or(json!({}));
    if !v.is_object() { v = json!({}); }
    match page { Some((id, t)) => v[key] = json!({ "id": id, "title": t }), None => { if let Some(o) = v.as_object_mut() { o.remove(key); } } }
    prefs::set(&k, &v.to_string());
}

/// A view came on show (the tab, or a pane by its key).
fn attach(key: &str, cx: &mut App) {
    if key.is_empty() { load_tab(cx); render("", cx); return; }
    let fresh = with(cx, |s| {
        s.seen.insert(key.to_string());
        if idx(s, key).is_some() { return false; }
        s.places.push(place_new(key));
        true
    });
    if fresh {
        if let Some((id, title)) = saved_pane(key, cx) {
            with(cx, |s| { let i = idx(s, key).unwrap(); s.places[i].page = Some(id.clone()); s.places[i].title = title; });
            open_editor(key, &id, false, cx);
        } else {
            reload_picker(key, cx);
        }
    }
    render(key, cx);
}

/// Panes that left the workbench save and close.
fn prune(cx: &mut App) {
    if util::current_tab(cx) != "workbench" { return; }
    let Some(wb) = crundi_workbench::workbench(cx) else { return };
    let keys: HashSet<String> = wb.update(cx, |w, _| w.keys()).into_iter().collect();
    let gone: Vec<String> = rd(cx, |s| s.places.iter().filter(|p| !p.key.is_empty() && s.seen.contains(&p.key) && !keys.contains(&p.key)).map(|p| p.key.clone()).collect());
    for k in gone {
        flush(&k, cx);
        save_pane(&k, None, cx);
        with(cx, |s| { s.places.retain(|p| p.key != k); s.seen.remove(&k); s.views.remove(&k); });
        refresh_pickers(cx);
    }
}

/// Pages open somewhere else (notesBusyIds): the panes' pages but `except`'s,
/// and the tab's page when `with_tab`.
fn busy_ids(except: &str, with_tab: bool, cx: &mut App) -> HashSet<String> {
    with(cx, |s| {
        let mut out: HashSet<String> = s.places.iter().filter(|p| !p.key.is_empty() && p.key != except).filter_map(|p| p.page.clone()).collect();
        if with_tab && s.tab.open { if let Some(c) = &s.tab.current { out.insert(c.clone()); } }
        out
    })
}

fn busy_ids_in(s: &S, except: &str, with_tab: bool) -> HashSet<String> {
    let mut out: HashSet<String> = s.places.iter().filter(|p| !p.key.is_empty() && p.key != except).filter_map(|p| p.page.clone()).collect();
    if with_tab && s.tab.open && !except.is_empty() { if let Some(c) = &s.tab.current { out.insert(c.clone()); } }
    out
}

/// The pane header's title for a notes pane: the page's ("Untitled" when it
/// has none), or "Notes" with no page.
pub(crate) fn pane_title_in(s: &S, key: &str) -> String {
    (s.places.iter().find(|p| p.key == key).and_then(|p| p.page.as_ref().map(|_| if p.title.trim().is_empty() { "Untitled".to_string() } else { p.title.clone() }))).unwrap_or_else(|| "Notes".into())
}


fn item_of(v: &Value, busy: bool, cur: bool, now: f64) -> Item {
    let t = jstr(v, "title");
    let at = crundi_core::time::parse_iso(&jstr(v, "updatedAt")).unwrap_or(0.0);
    Item {
        id: jstr(v, "id"), title: if t.is_empty() { "Untitled".into() } else { t }, preview: jstr(v, "preview").replace('\n', " "),
        meta: if busy { "Open in another pane".into() } else { format!("Edited {}", doc::ago(now, at)) }, busy, cur,
    }
}

/// One place as drawn (pane_row).
pub(crate) fn place_row(s: &S, key: &str) -> Option<PlaceRow> {
    {
        let i = idx(s, key)?;
        let float_menu = match &s.float { Some(Float::Block { key: k, id }) if k == key => Some(id.clone()), _ => None };
        let p = &s.places[i];
        let (blocks, sel_count, save) = match p.ed.as_ref() {
            Some(ed) => {
                let nums = doc::numbers(&ed.blocks);
                let (lo, hi) = sel_range(ed).unwrap_or((usize::MAX, 0));
                let rows: Vec<BlockRow> = ed.blocks.iter().enumerate().map(|(k, b)| {
                    let inn = k >= lo && k <= hi;
                    BlockRow { b: b.clone(), num: nums[k].clone(), sel: inn, sel_top: inn && k == lo, sel_bottom: inn && k == hi, menu_open: float_menu.as_deref() == Some(b.id.as_str()) }
                }).collect();
                let n = if lo == usize::MAX { 0 } else { hi - lo + 1 };
                (rows, n, (ed.save_state.clone(), ed.save_text.clone(), ed.save_tip.clone()))
            }
            None => (vec![], 0, Default::default()),
        };
        let busy = busy_ids_in(s, &p.key, true);
        let now = now();
        let items: Vec<Item> = doc::filter(&p.pages, None, &p.search).iter().map(|v| item_of(v, busy.contains(&s_id(v)), false, now)).collect();
        let list_msg = if !p.list_msg.is_empty() { p.list_msg.clone() } else if items.is_empty() && !p.key.is_empty() && p.mode == "picker" { if p.pages.is_empty() { "No pages yet in this project.".into() } else { "No page matches.".into() } } else { String::new() };
        Some(PlaceRow {
            key: p.key.clone(), mode: p.mode.clone(), msg: p.msg.clone(), page_id: p.ed.as_ref().map(|e| e.page_id.clone()).unwrap_or_default(),
            title: p.ed.as_ref().map(|e| e.title.clone()).unwrap_or_default(), title_gen: p.title_gen,
            save_state: save.0, save_text: save.1, save_tip: save.2,
            blocks, sel_count, focus: p.focus.clone(), focus_gen: p.focus_gen, cur_id: p.cur_id.clone(), drop_y: p.drop_y,
            items, list_msg, new_busy: p.new_busy,
        })
    }
}

fn s_id(v: &Value) -> String { jstr(v, "id") }

fn refresh_pickers(cx: &mut App) { render_all(cx); }

// ─── The text size (shared by every page) ───

fn set_font(dir: i32, cx: &mut App) {
    let f = with(cx, |s| { s.font = if dir == 0 { FONT_BASE } else { (s.font + dir.signum()).clamp(11, 26) }; s.font });
    prefs::set("crundi_notes_font", &if f == FONT_BASE { String::new() } else { f.to_string() });
    render_all(cx);
}

pub(crate) fn scale_of(s: &S) -> f32 { s.font as f32 / FONT_BASE as f32 }

// ─── Lists ───

fn fetch_pages(include_deleted: bool, cx: &mut App, done: impl FnOnce(Result<Vec<Value>, String>, &mut App) + 'static) {
    let p = project(cx);
    let path = format!("/api/notes?project={}{}", enc(&p), if include_deleted { "&includeDeleted=1" } else { "" });
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
        let r = match r {
            Err(e) => Err(e),
            Ok(v) if truthy(&v["ok"]) => Ok(v.get("pages").and_then(|x| x.as_array()).cloned().unwrap_or_default()),
            Ok(v) => Err({ let e = jstr(&v, "error"); if e.is_empty() { "Could not list pages".into() } else { e } }),
        };
        done(r, cx);
    });
}

fn reload_picker(key: &str, cx: &mut App) {
    if project(cx).is_empty() { return; }
    let k = key.to_string();
    fetch_pages(false, cx, move |r, cx| {
        with(cx, |s| if let Some(i) = idx(s, &k) { match r { Ok(ps) => { s.places[i].pages = ps; s.places[i].list_msg.clear(); } Err(e) => s.places[i].list_msg = e } });
        render(&k, cx);
    });
}

fn search(key: &str, q: &str, cx: &mut App) {
    if key.is_empty() { with(cx, |s| s.tab.search = q.to_string()); render("", cx); return; }
    with(cx, |s| if let Some(i) = idx(s, key) { s.places[i].search = q.to_string(); });
    render(key, cx);
}

/// POST /api/notes with the project and our client id.
fn post(mut body: Value, cx: &mut App, then: impl FnOnce(Value, &mut App) + 'static) {
    body["project"] = json!(project(cx));
    body["clientId"] = json!(with(cx, |s| s.client_id.clone()));
    crundi_shell::call(cx, move |api| api.post("/api/notes", &body), move |r, cx| then(r.unwrap_or_else(|e| json!({ "ok": false, "__net": true, "error": e })), cx));
}

fn new_page(key: &str, cx: &mut App) {
    let k = key.to_string();
    let busy = with(cx, |s| if k.is_empty() { let b = s.tab.new_busy; s.tab.new_busy = true; b } else { idx(s, &k).map(|i| { let b = s.places[i].new_busy; s.places[i].new_busy = true; b }).unwrap_or(true) });
    if busy { return; }
    render(&k, cx);
    post(json!({ "action": "create", "title": "" }), cx, move |d, cx| {
        let ok = truthy(&d["ok"]);
        with(cx, |s| if k.is_empty() { s.tab.new_busy = false; } else if let Some(i) = idx(s, &k) { s.places[i].new_busy = false; });
        if !ok {
            let e = jstr(&d, "error");
            toast_error(cx, if e.is_empty() { "Could not create a page" } else { &e });
            render(&k, cx);
            return;
        }
        let page = d["page"].clone();
        let id = jstr(&page, "id");
        if k.is_empty() {
            with(cx, |s| { s.tab.view = "pages".into(); s.tab.pages.insert(0, json!({ "id": id, "title": "", "preview": "", "updatedAt": s_at(&page) })); });
            tab_open(&id, true, cx);
        } else {
            on_pick(&k, &id, "", true, cx);
        }
    });
}

fn s_at(page: &Value) -> String { jstr(page, "updatedAt") }

/// The picker's choice (or the tab's list).
fn pick(key: &str, id: &str, cx: &mut App) {
    if key.is_empty() { if with(cx, |s| s.tab.current.as_deref() != Some(id) && s.tab.view == "pages") { tab_open(id, false, cx); } return; }
    if busy_ids(key, true, cx).contains(id) { toast_error(cx, "That page is open in another pane"); return; }
    let title = with(cx, |s| idx(s, key).and_then(|i| s.places[i].pages.iter().find(|p| s_id(p) == id).map(|p| jstr(p, "title")))).unwrap_or_default();
    on_pick(key, id, &title, false, cx);
}

fn on_pick(key: &str, id: &str, title: &str, fresh: bool, cx: &mut App) {
    if busy_ids(key, true, cx).contains(id) { toast_error(cx, "That page is open in another pane"); return; }
    with(cx, |s| if let Some(i) = idx(s, key) { s.places[i].page = Some(id.to_string()); s.places[i].title = title.to_string(); });
    save_pane(key, Some((id, title)), cx);
    open_editor(key, id, fresh, cx);
    refresh_pickers(cx);
}

/// "Pages" in a pane's header: back to the picker (the page stays as it is).
pub fn switch_to_picker(key: &str, cx: &mut App) {
    if key.is_empty() { return; }
    flush(key, cx);
    close_float_of(key, cx);
    with(cx, |s| if let Some(i) = idx(s, key) { let p = &mut s.places[i]; p.page = None; p.title.clear(); p.ed = None; p.mode = "picker".into(); p.search.clear(); });
    save_pane(key, None, cx);
    reload_picker(key, cx);
    refresh_pickers(cx);
}

// ─── The tab (mountManager) ───

fn load_tab(cx: &mut App) {
    let p = project(cx);
    if p.is_empty() {
        close_tab(cx);
        with(cx, |s| s.tab_project = false);
        return;
    }
    with(cx, |s| s.tab_project = true);
    if with(cx, |s| s.tab.open && s.tab.project == p) { return; }
    close_tab(cx);
    let last = prefs::get(&format!("crundi_notes_last_{p}"));
    let initial = (!last.is_empty() && !busy_ids("", false, cx).contains(&last)).then_some(last);
    with(cx, |s| { s.tab.open = true; s.tab.project = p.clone(); s.tab.current = None; s.tab.pages.clear(); s.tab.list_msg.clear(); });
    match initial {
        Some(id) => tab_open(&id, false, cx),
        None => { with(cx, |s| if let Some(i) = idx(s, "") { s.places[i].mode = "empty".into(); s.places[i].msg = "Loading…".into(); }); render("", cx); }
    }
    tab_reload(cx);
    refresh_pickers(cx);
}

/// Leaving the tab closes its page (saving it first), so the tab never holds
/// a page out of a workbench pane while it is not even on screen.
fn close_tab(cx: &mut App) {
    if !with(cx, |s| s.tab.open) { return; }
    flush("", cx);
    close_float_of("", cx);
    with(cx, |s| {
        s.tab.open = false;
        s.tab.current = None;
        if let Some(i) = idx(s, "") { let p = &mut s.places[i]; p.ed = None; p.mode = "empty".into(); p.msg.clear(); }
    });
    refresh_pickers(cx);
}

fn tab_open(id: &str, focus: bool, cx: &mut App) {
    if busy_ids("", false, cx).contains(id) { toast_error(cx, "That page is open in a workbench pane. One page, one pane."); return; }
    flush("", cx);
    close_float_of("", cx);
    with(cx, |s| { s.tab.current = Some(id.to_string()); s.tab.arrived = None; });
    prefs::set(&format!("crundi_notes_last_{}", project(cx)), id);
    open_editor("", id, focus, cx);
    refresh_pickers(cx);
}

fn tab_reload(cx: &mut App) {
    let gen_n = with(cx, |s| { s.tab.list_gen += 1; s.tab.list_gen });
    fetch_pages(true, cx, move |r, cx| {
        if !with(cx, |s| s.tab.open) { return; }
        // Every answer counts, as the web's reload: an older one (a page that
        // arrived first) still opens a page when none is open; only the
        // newest replaces the list.
        let newest = with(cx, |s| s.tab.list_gen == gen_n);
        match r {
            Ok(ps) => {
                if newest { with(cx, |s| { s.tab.pages = ps.clone(); s.tab.list_msg.clear(); }); render("", cx); }
                let (cur, view) = with(cx, |s| (s.tab.current.clone(), s.tab.view.clone()));
                if cur.is_none() && view == "pages" {
                    let busy = busy_ids("", false, cx);
                    let (first, any) = {
                        let live: Vec<&Value> = ps.iter().filter(|p| !p.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false)).collect();
                        let arrived = with(cx, |s| s.tab.arrived.take()).filter(|a| live.iter().any(|p| s_id(p) == *a) && !busy.contains(a));
                        (arrived.or_else(|| live.iter().map(|p| s_id(p)).find(|id| !busy.contains(id))), !live.is_empty())
                    };
                    match first {
                        Some(id) => tab_open(&id, false, cx),
                        None => tab_empty(if any { "Every page is open in a workbench pane." } else { "No pages yet. Start one with New page." }, cx),
                    }
                }
            }
            Err(e) => if newest { with(cx, |s| s.tab.list_msg = e); render("", cx); },
        }
    });
}

fn tab_empty(msg: &str, cx: &mut App) {
    flush("", cx);
    with(cx, |s| if let Some(i) = idx(s, "") { let p = &mut s.places[i]; p.ed = None; p.mode = "empty".into(); p.msg = msg.into(); });
    render("", cx);
}

/// The tab's list as drawn (draw_tab_list): its items and its line.
pub(crate) fn tab_list(s: &S) -> (Vec<Item>, String) {
    {
        let busy = busy_ids_in(s, "", false);
        let trash = s.tab.view == "trash";
        let shown = doc::filter(&s.tab.pages, Some(trash), &s.tab.search);
        let now = now();
        let items: Vec<Item> = shown.iter().map(|v| {
            let id = s_id(v);
            let cur = !trash && s.tab.current.as_deref() == Some(id.as_str());
            item_of(v, busy.contains(&id) && !cur, cur, now)
        }).collect();
        let msg = if !s.tab.list_msg.is_empty() { s.tab.list_msg.clone() } else if items.is_empty() {
            if trash { "The trash is empty.".into() } else if s.tab.pages.iter().any(|p| !p.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false)) { "No page matches.".into() } else { "No pages yet. Start one with New page.".into() }
        } else { String::new() };
        (items, msg)
    }
}

/// What the tab bar and list are told (project, view, search, busy).
pub(crate) fn tab_state(s: &S) -> (bool, String, String, bool) { (s.tab_project, s.tab.view.clone(), s.tab.search.clone(), s.tab.new_busy) }

fn set_view(v: &str, cx: &mut App) { with(cx, |s| s.tab.view = v.to_string()); render("", cx); }

fn item_menu(id: &str, x: f32, y: f32, cx: &mut App) {
    if with(cx, |s| s.tab.view == "trash") { return; }
    close_float(cx);
    with(cx, |s| { s.float = Some(Float::Item { id: id.to_string() }); s.menu_ids = vec!["trash".into()]; });
    show_menu(x, y, "", vec![mi("trash", "\u{2715}", "Move to trash", "", false, true, false)], -1, false, "", "", cx);
}

fn page_act(action: &str, id: &str, cx: &mut App, then: impl FnOnce(Value, &mut App) + 'static) {
    post(json!({ "action": action, "id": id }), cx, move |d, cx| {
        if !truthy(&d["ok"]) { let e = jstr(&d, "error"); toast_error(cx, if e.is_empty() { "Failed" } else { &e }); }
        then(d, cx);
    });
}

fn trash_act(a: &str, id: &str, cx: &mut App) {
    let id = id.to_string();
    match a {
        "restore" => page_act("restore", &id, cx, |_, cx| tab_reload(cx)),
        "purge" => {
            let title = with(cx, |s| s.tab.pages.iter().find(|p| s_id(p) == id).map(|p| jstr(p, "title"))).unwrap_or_default();
            let msg = format!("Delete \u{201c}{}\u{201d} for good? This cannot be undone.", if title.is_empty() { "Untitled" } else { &title });
            confirm(cx, "Delete for good", &msg, "Delete for good", true, move |cx| page_act("purge", &id, cx, |_, cx| tab_reload(cx)));
        }
        _ => {}
    }
}

// ─── The editor (mountEditor) ───

fn ed_mut<R>(s: &mut S, key: &str, f: impl FnOnce(&mut Ed) -> R) -> Option<R> {
    let i = idx(s, key)?;
    s.places[i].ed.as_mut().map(f)
}

fn set_save(ed: &mut Ed, state: &str, text: &str) { ed.save_state = state.into(); ed.save_text = text.into(); }

fn open_editor(key: &str, id: &str, focus: bool, cx: &mut App) {
    with(cx, |s| if let Some(i) = idx(s, key) {
        let p = &mut s.places[i];
        let mut ed = Ed { page_id: id.to_string(), ..Default::default() };
        set_save(&mut ed, "busy", "Loading…");
        p.ed = Some(ed);
        p.mode = "editor".into();
        p.msg.clear();
        p.fresh = focus;
        p.title_gen += 1;
        p.cur_id.clear();
    });
    load_page(key, cx);
}

fn load_page(key: &str, cx: &mut App) {
    let Some((id, gen_n)) = with(cx, |s| ed_mut(s, key, |e| { e.load_gen += 1; set_save(e, "busy", "Loading…"); (e.page_id.clone(), e.load_gen) })) else { return };
    render(key, cx);
    let path = format!("/api/notes/page?project={}&id={}", enc(&project(cx)), enc(&id));
    let k = key.to_string();
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| on_loaded(&k, &id, gen_n, r.unwrap_or_else(|e| json!({ "__err": e })), cx));
}

fn on_loaded(key: &str, id: &str, gen_n: u64, d: Value, cx: &mut App) {
    let live = with(cx, |s| ed_mut(s, key, |e| e.page_id == id && e.load_gen == gen_n).unwrap_or(false));
    if !live { return; }
    if let Some(e) = d.get("__err").and_then(|e| e.as_str()) {
        with(cx, |s| ed_mut(s, key, |ed| set_save(ed, "err", "Could not load")));
        toast_error(cx, e);
        render(key, cx);
        return;
    }
    if !truthy(&d["ok"]) {
        let e = jstr(&d, "error");
        with(cx, |s| { ed_mut(s, key, |ed| set_save(ed, "err", if e.is_empty() { "Not found" } else { &e })); if let Some(i) = idx(s, key) { s.places[i].mode = "msg".into(); s.places[i].msg = if e.is_empty() { "This page could not be opened.".into() } else { e.clone() }; } });
        render(key, cx);
        // A pane whose page is gone goes back to its picker (onMissing).
        if !key.is_empty() { switch_to_picker(key, cx); }
        return;
    }
    let page = &d["page"];
    if page.get("deleted").and_then(|x| x.as_bool()).unwrap_or(false) {
        with(cx, |s| { ed_mut(s, key, |ed| set_save(ed, "err", "In the trash")); if let Some(i) = idx(s, key) { s.places[i].mode = "msg".into(); s.places[i].msg = "This page is in the trash. Restore it from the Notes tab to edit it.".into(); } });
        render(key, cx);
        return;
    }
    show_page(key, page, cx);
    let fresh = with(cx, |s| idx(s, key).map(|i| std::mem::take(&mut s.places[i].fresh)).unwrap_or(false));
    if fresh {
        let empty_title = with(cx, |s| ed_mut(s, key, |e| e.title.is_empty()).unwrap_or(true));
        if empty_title { focus_title(key, cx); } else { focus_first(key, cx); }
    }
}

fn show_page(key: &str, page: &Value, cx: &mut App) {
    let title = jstr(page, "title");
    let blocks = doc::blocks_from(&page["blocks"]);
    let updated = jstr(page, "updatedAt");
    with(cx, |s| {
        if let Some(i) = idx(s, key) {
            let p = &mut s.places[i];
            p.mode = "editor".into();
            p.title_gen += 1;
            if let Some(ed) = p.ed.as_mut() {
                ed.version = page.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
                ed.title = title.clone();
                ed.blocks = blocks;
                ed.loaded = true;
                ed.dirty = false;
                ed.sel = None;
                set_save(ed, "ok", "Saved");
                ed.save_tip = crundi_core::time::parse_iso(&updated).map(|t| format!("Last edited {}", doc::ago(now(), t))).unwrap_or_default();
            }
        }
    });
    on_title(key, &title, cx);
    render(key, cx);
}

/// The page's title changed (the pane header, the tab's list).
fn on_title(key: &str, title: &str, cx: &mut App) {
    let id = with(cx, |s| idx(s, key).and_then(|i| { s.places[i].title = title.to_string(); s.places[i].ed.as_ref().map(|e| e.page_id.clone()) }));
    if key.is_empty() {
        if let Some(id) = id { with(cx, |s| if let Some(p) = s.tab.pages.iter_mut().find(|p| s_id(p) == id) { p["title"] = json!(title); }); render("", cx); }
    } else if let Some(id) = id {
        save_pane(key, Some((&id, title)), cx);
    }
}

/// Something on the page changed: save it soon (changed()).
fn changed(key: &str, cx: &mut App) {
    let gen_n = with(cx, |s| ed_mut(s, key, |e| {
        if !e.loaded { return None; }
        e.dirty = true;
        set_save(e, "busy", "Editing…");
        e.save_gen += 1;
        Some(e.save_gen)
    }).flatten());
    let Some(gen_n) = gen_n else { return };
    schedule_save(key, gen_n, 700, cx);
}

fn schedule_save(key: &str, gen_n: u64, ms: u64, cx: &mut App) {
    let k = key.to_string();
    after(cx, ms, move |cx| {
        if with(cx, |s| ed_mut(s, &k, |e| e.save_gen == gen_n).unwrap_or(false)) { save(&k, cx); }
    });
}

fn save(key: &str, cx: &mut App) {
    let body = with(cx, |s| ed_mut(s, key, |e| {
        if !e.dirty || !e.loaded { return None; }
        if e.saving { e.save_gen += 1; return Some(Err(e.save_gen)); }
        e.saving = true;
        e.dirty = false;
        set_save(e, "busy", "Saving…");
        Some(Ok(json!({ "action": "save", "id": e.page_id, "title": e.title.trim(), "blocks": e.blocks.iter().map(|b| b.to_json()).collect::<Vec<_>>(), "baseVersion": e.version })))
    }).flatten());
    let Some(body) = body else { return };
    let body = match body { Ok(b) => b, Err(gen_n) => { schedule_save(key, gen_n, 400, cx); return; } };
    render(key, cx);
    let k = key.to_string();
    let page_id = jstr(&body, "id");
    let mine = body.clone();
    post(body, cx, move |d, cx| {
        let live = with(cx, |s| ed_mut(s, &k, |e| e.page_id == page_id).unwrap_or(false));
        if !live { return; }
        if truthy(&d["ok"]) {
            let again = with(cx, |s| ed_mut(s, &k, |e| {
                e.saving = false;
                e.version = d.get("version").and_then(|v| v.as_i64()).unwrap_or(e.version + 1);
                set_save(e, "ok", "Saved");
                e.save_tip = "Saved just now".into();
                if e.dirty { e.save_gen += 1; Some(e.save_gen) } else { None }
            }).flatten());
            if let Some(g) = again { schedule_save(&k, g, 400, cx); }
            render(&k, cx);
            return;
        }
        if truthy(&d["conflict"]) { with(cx, |s| ed_mut(s, &k, |e| e.saving = false)); on_conflict(&k, &mine, &d["page"], cx); return; }
        if truthy(&d["__net"]) {
            let g = with(cx, |s| ed_mut(s, &k, |e| { e.saving = false; e.dirty = true; set_save(e, "err", "Offline \u{2014} will retry"); e.save_gen += 1; e.save_gen }));
            if let Some(g) = g { schedule_save(&k, g, 4000, cx); }
            render(&k, cx);
            return;
        }
        let e = jstr(&d, "error");
        with(cx, |s| ed_mut(s, &k, |ed| { ed.saving = false; ed.dirty = true; set_save(ed, "err", if e.is_empty() { "Not saved" } else { &e }); }));
        toast_error(cx, if e.is_empty() { "Could not save the page" } else { &e });
        render(&k, cx);
    });
}

/// Someone else saved this page since we loaded it. Nothing is thrown away:
/// what was typed here becomes a copy, and this place shows theirs.
fn on_conflict(key: &str, mine: &Value, theirs: &Value, cx: &mut App) {
    let t = mine["title"].as_str().unwrap_or("");
    post(json!({ "action": "create", "title": format!("{} (your copy)", if t.is_empty() { "Untitled" } else { t }), "blocks": mine["blocks"] }), cx, |d, cx| {
        let name = d["page"]["title"].as_str().unwrap_or("a copy").to_string();
        toast_error(cx, &format!("This page was changed elsewhere. Your version was saved as \u{201c}{name}\u{201d}."));
    });
    show_page(key, theirs, cx);
}

/// Save now what waits (closing a pane, switching pages).
fn flush(key: &str, cx: &mut App) {
    let go = with(cx, |s| ed_mut(s, key, |e| { if e.dirty { e.save_gen += 1; true } else { false } }).unwrap_or(false));
    if go { save(key, cx); }
}

/// A save from elsewhere: reload when nothing here is unsaved.
fn remote_change(key: &str, info: &Value, cx: &mut App) {
    let me = with(cx, |s| s.client_id.clone());
    let reload = with(cx, |s| ed_mut(s, key, |e| {
        if s_id(info) != e.page_id || jstr(info, "by") == me { return false; }
        let a = jstr(info, "action");
        if a == "delete" || a == "purge" { return true; }
        if let Some(v) = info.get("version").and_then(|v| v.as_i64()) { if v <= e.version { return false; } }
        !e.dirty && !e.saving
    }).unwrap_or(false));
    if reload { load_page(key, cx); }
}

// ─── Focus ───

fn request_focus(key: &str, id: &str, r: i32, c: i32, a: usize, b: usize, cx: &mut App) {
    with(cx, |s| if let Some(i) = idx(s, key) { let p = &mut s.places[i]; p.focus = (id.to_string(), r, c, a as i32, b as i32); p.focus_gen += 1; });
    render(key, cx);
}

fn block_of<'a>(ed: &'a Ed, id: &str) -> Option<(usize, &'a Block)> { ed.blocks.iter().enumerate().find(|(_, b)| b.id == id) }

/// Put the caret in a block: at its start, its end, or a plain offset.
#[derive(Clone, Copy)]
enum At { Start, End, Plain(usize) }

fn focus_block(key: &str, id: &str, at: At, cx: &mut App) {
    let t = with(cx, |s| ed_mut(s, key, |e| block_of(e, id).map(|(_, b)| b.clone())).flatten());
    let Some(b) = t else { return };
    match b.kind.as_str() {
        "divider" => request_focus(key, id, -1, -1, 0, 0, cx),
        "table" => {
            let (r, c) = match at { At::End => (b.rows.len().saturating_sub(1), b.rows.last().map(|r| r.len().saturating_sub(1)).unwrap_or(0)), _ => (0, 0) };
            let src = b.rows.get(r).and_then(|x| x.get(c)).cloned().unwrap_or_default();
            let o = if matches!(at, At::End) { src.len() } else { 0 };
            request_focus(key, id, r as i32, c as i32, o, o, cx);
        }
        "code" => { let o = if matches!(at, At::Start) { 0 } else { b.text.len() }; request_focus(key, id, -1, -1, o, o, cx); }
        _ => {
            let o = match at {
                At::Start => 0,
                At::End => b.text.len(),
                At::Plain(k) => { let (_, map) = doc::runs_to_src_map(&doc::src_to_runs(&b.text)); doc::plain_to_src_at(&map, k) }
            };
            request_focus(key, id, -1, -1, o, o, cx);
        }
    }
}

fn focus_title(key: &str, cx: &mut App) {
    let n = with(cx, |s| ed_mut(s, key, |e| e.title.len()).unwrap_or(0));
    request_focus(key, "title", -1, -1, n, n, cx);
}

fn focus_first(key: &str, cx: &mut App) {
    if let Some(id) = with(cx, |s| ed_mut(s, key, |e| e.blocks.first().map(|b| b.id.clone())).flatten()) { focus_block(key, &id, At::End, cx); }
}

fn hint_select_all(cx: &mut App) {
    let go = with(cx, |s| { let ok = s.hint_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(8)); if ok { s.hint_at = Some(Instant::now()); } ok });
    if go { toast(cx, &format!("{m}+A selects this block\u{2019}s text. {m}+Shift+A selects the whole page.", m = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" })); }
}

// ─── Block edits ───

/// Change a block in place (the checkbox, a table cell).
fn edit_block(key: &str, id: &str, cx: &mut App, f: impl FnOnce(&mut Block)) {
    let ok = with(cx, |s| ed_mut(s, key, |e| e.blocks.iter_mut().find(|b| b.id == id).map(f).is_some()).unwrap_or(false));
    if ok { changed(key, cx); render(key, cx); }
}

fn insert_after(key: &str, after: Option<&str>, b: Block, cx: &mut App) -> String {
    let id = b.id.clone();
    with(cx, |s| ed_mut(s, key, |e| {
        let at = after.and_then(|a| e.blocks.iter().position(|x| x.id == a)).map(|i| i + 1).unwrap_or(e.blocks.len());
        e.blocks.insert(at, b);
    }));
    changed(key, cx);
    render(key, cx);
    id
}

fn remove_block(e: &mut Ed, id: &str) {
    e.blocks.retain(|b| b.id != id);
    if e.blocks.is_empty() { e.blocks.push(Block::new("p", "")); }
}

/// Turn a block into another type (convert()), with extra settings.
fn convert(key: &str, id: &str, kind: &str, checked: bool, lang: Option<String>, cx: &mut App) {
    with(cx, |s| ed_mut(s, key, |e| if let Some(b) = e.blocks.iter_mut().find(|b| b.id == id) {
        let mut nb = b.converted(kind);
        if checked { nb.checked = true; }
        if let Some(l) = lang { nb.lang = l; nb.text.clear(); }
        *b = nb;
    }));
    changed(key, cx);
    render(key, cx);
}

fn neighbours(key: &str, id: &str, cx: &mut App) -> (Option<Block>, Option<Block>) {
    with(cx, |s| ed_mut(s, key, |e| {
        let i = e.blocks.iter().position(|b| b.id == id);
        match i { Some(i) => (i.checked_sub(1).map(|j| e.blocks[j].clone()), e.blocks.get(i + 1).cloned()), None => (None, None) }
    }).unwrap_or((None, None)))
}

fn cur_block(key: &str, id: &str, cx: &mut App) -> Option<Block> { with(cx, |s| ed_mut(s, key, |e| block_of(e, id).map(|(_, b)| b.clone())).flatten()) }

fn set_text(key: &str, id: &str, text: String, cx: &mut App) {
    with(cx, |s| ed_mut(s, key, |e| if let Some(b) = e.blocks.iter_mut().find(|b| b.id == id) { b.text = text; }));
}

fn title_edited(key: &str, t: &str, cx: &mut App) {
    // The title is one line: a pasted line break becomes a space.
    let clean = if t.contains(['\n', '\r']) { t.split_whitespace().collect::<Vec<_>>().join(" ") } else { t.to_string() };
    let reset = clean != t;
    with(cx, |s| { ed_mut(s, key, |e| e.title = clean.clone()); if reset { if let Some(i) = idx(s, key) { s.places[i].title_gen += 1; } } });
    on_title(key, clean.trim(), cx);
    changed(key, cx);
    render(key, cx);
}

fn title_key(key: &str, name: &str, cx: &mut App) -> bool {
    match name {
        // Into the first block, at its start (the web's title keys).
        "enter" | "down" => {
            if let Some(id) = with(cx, |s| ed_mut(s, key, |e| e.blocks.first().map(|b| b.id.clone())).flatten()) { focus_block(key, &id, At::Start, cx); }
            true
        }
        "ctrl-a" => { hint_select_all(cx); false }
        "ctrl-shift-a" => { select_all(key, cx); true }
        _ => false,
    }
}

fn edited(key: &str, id: &str, text: &str, caret: i32, cx: &mut App) {
    let Some(b) = cur_block(key, id, cx) else { return };
    if matches!(&with(cx, |s| s.float.clone()), Some(Float::Fmt { link: false, .. })) { close_float(cx); }
    set_text(key, id, text.to_string(), cx);
    if b.kind == "code" { changed(key, cx); render(key, cx); return; }
    // Markdown-style shortcuts at the start of a block.
    if let Some(sc) = doc::shortcut(&b.kind, text) {
        close_float(cx);
        match sc {
            doc::Shortcut::To(k, ch) => { set_text(key, id, String::new(), cx); convert(key, id, k, ch, None, cx); focus_block(key, id, At::Start, cx); }
            doc::Shortcut::Divider => {
                set_text(key, id, String::new(), cx);
                convert(key, id, "divider", false, None, cx);
                let nid = insert_after(key, Some(id), Block::new("p", ""), cx);
                focus_block(key, &nid, At::Start, cx);
            }
            doc::Shortcut::Code(lang) => { convert(key, id, "code", false, Some(lang), cx); focus_block(key, id, At::Start, cx); }
        }
        return;
    }
    changed(key, cx);
    render(key, cx);
    // The slash menu: follow it, or open it on a "/" typed at the start or after a space.
    let plain = doc::src_plain(text);
    let cp = doc::src_to_plain_at(text, caret.max(0) as usize);
    let fl = with(cx, |s| s.float.clone());
    match fl {
        Some(Float::Slash { key: k, id: bid, start, .. }) if k == key && bid == id => {
            match start.and_then(|st| doc::slash_query(&plain, st, cp).map(|q| (st, q))) {
                Some((st, q)) => { with(cx, |s| s.float = Some(Float::Slash { key: k, id: bid, start: Some(st), query: q })); draw_slash(cx); }
                None => if start.is_some() { close_float(cx); },
            }
        }
        _ => if doc::slash_opens(&plain, cp) { open_slash(key, id, Some(cp - 1), cx); },
    }
}

#[allow(clippy::too_many_arguments)]
fn text_key(key: &str, id: &str, name: &str, anchor: i32, caret: i32, first: bool, last: bool, cx: &mut App) -> bool {
    // The slash menu takes its keys first.
    if let Some(Float::Slash { key: k, id: bid, .. }) = with(cx, |s| s.float.clone()) {
        if k == key && bid == id && matches!(name, "up" | "down" | "enter" | "tab" | "escape") { return menu_key(name, cx); }
    }
    let Some(b) = cur_block(key, id, cx) else { return false };
    let (a, c) = (anchor.max(0) as usize, caret.max(0) as usize);
    let collapsed = a == c;
    match name {
        "ctrl-shift-a" => { select_all(key, cx); return true; }
        "ctrl-a" => { if b.kind != "code" { hint_select_all(cx); } return false; }
        _ => {}
    }
    if let Some(h) = block_key(key, id, -1, -1, name, a, c, cx) { return h; }
    let (prev, next) = neighbours(key, id, cx);
    if b.kind == "code" {
        return match name {
            "tab" => { let mut t = b.text.clone(); let (lo, hi) = (a.min(c).min(t.len()), a.max(c).min(t.len())); t.replace_range(lo..hi, "  "); set_text(key, id, t, cx); changed(key, cx); render(key, cx); request_focus(key, id, -1, -1, lo + 2, lo + 2, cx); true }
            "shift-enter" => { let n = insert_after(key, Some(id), Block::new("p", ""), cx); focus_block(key, &n, At::Start, cx); true }
            "backspace" if b.text.is_empty() => { convert(key, id, "p", false, None, cx); focus_block(key, id, At::Start, cx); true }
            "up" if first && collapsed => { if let Some(p) = prev { focus_block(key, &p.id, At::End, cx); true } else { false } }
            "down" if last && collapsed => {
                match next { Some(n) => focus_block(key, &n.id, At::Start, cx), None => { let n = insert_after(key, Some(id), Block::new("p", ""), cx); focus_block(key, &n, At::Start, cx); } }
                true
            }
            "shift-up" if first && collapsed && prev.is_some() => { set_sel(key, id, &prev.unwrap().id, cx); true }
            "shift-down" if last && collapsed && next.is_some() => { set_sel(key, id, &next.unwrap().id, cx); true }
            _ => false,
        };
    }
    match name {
        "shift-up" if first && collapsed && prev.is_some() => { set_sel(key, id, &prev.unwrap().id, cx); true }
        "shift-down" if last && collapsed && next.is_some() => { set_sel(key, id, &next.unwrap().id, cx); true }
        "ctrl-b" => { mark(key, id, -1, -1, a, c, Mark::B, cx); true }
        "ctrl-i" => { mark(key, id, -1, -1, a, c, Mark::I, cx); true }
        "ctrl-u" => { mark(key, id, -1, -1, a, c, Mark::U, cx); true }
        "ctrl-e" => { mark(key, id, -1, -1, a, c, Mark::Code, cx); true }
        "ctrl-shift-s" => { mark(key, id, -1, -1, a, c, Mark::S, cx); true }
        "ctrl-k" => { open_fmt(key, id, -1, -1, a, c, true, cx); true }
        "ctrl-v" => paste(key, id, -1, -1, a, c, cx),
        "tab" | "shift-tab" if doc::listy(&b.kind) => { close_float(cx); nest(key, id, name == "shift-tab", cx); true }
        // Elsewhere Tab types no tab into the words (the web's leaves the line).
        "tab" | "shift-tab" => true,
        "enter" => {
            close_float(cx);
            if doc::listy(&b.kind) && b.level > 0 && doc::src_plain(&b.text).trim().is_empty() {
                // An empty nested item steps out one level.
                edit_block(key, id, cx, |b| b.level -= 1);
                return true;
            }
            if doc::listy(&b.kind) && doc::src_plain(&b.text).trim().is_empty() {
                // An empty list item ends the list.
                convert(key, id, "p", false, None, cx);
                focus_block(key, id, At::Start, cx);
                return true;
            }
            let runs = doc::src_to_runs(&b.text);
            let (lo, hi) = (doc::src_to_plain_at(&b.text, a.min(c)), doc::src_to_plain_at(&b.text, a.max(c)));
            let (left, rest) = doc::split_runs(&runs, lo);
            let (_, right) = doc::split_runs(&rest, hi - lo);
            set_text(key, id, doc::runs_to_src(&left), cx);
            let kind = if doc::listy(&b.kind) { b.kind.as_str() } else { "p" };
            // A list goes on at the same level.
            let level = if doc::listy(&b.kind) { b.level } else { 0 };
            let nb = Block { id: doc::gen_id(), kind: kind.into(), text: doc::runs_to_src(&right), level, ..Default::default() };
            let n = insert_after(key, Some(id), nb, cx);
            focus_block(key, &n, At::Start, cx);
            true
        }
        "backspace" if collapsed && c == 0 => {
            if b.kind != "p" { convert(key, id, "p", false, None, cx); focus_block(key, id, At::Start, cx); return true; }
            let Some(p) = prev else { return false };
            if p.kind == "divider" { with(cx, |s| ed_mut(s, key, |e| remove_block(e, &p.id))); changed(key, cx); render(key, cx); return true; }
            if doc::texty(&p.kind) {
                let len = doc::src_plain(&p.text).chars().count();
                let joined = doc::norm(doc::src_to_runs(&p.text).into_iter().chain(doc::src_to_runs(&b.text)).collect());
                set_text(key, &p.id, doc::runs_to_src(&joined), cx);
                with(cx, |s| ed_mut(s, key, |e| remove_block(e, id)));
                changed(key, cx);
                render(key, cx);
                focus_block(key, &p.id, At::Plain(len), cx);
            } else if doc::src_plain(&b.text).is_empty() {
                with(cx, |s| ed_mut(s, key, |e| remove_block(e, id)));
                changed(key, cx);
                render(key, cx);
                focus_block(key, &p.id, At::End, cx);
            } else {
                focus_block(key, &p.id, At::End, cx);
            }
            true
        }
        "delete" if collapsed && c >= b.text.len() => {
            let Some(n) = next.filter(|n| doc::texty(&n.kind)) else { return false };
            let len = doc::src_plain(&b.text).chars().count();
            let joined = doc::norm(doc::src_to_runs(&b.text).into_iter().chain(doc::src_to_runs(&n.text)).collect());
            set_text(key, id, doc::runs_to_src(&joined), cx);
            with(cx, |s| ed_mut(s, key, |e| remove_block(e, &n.id)));
            changed(key, cx);
            render(key, cx);
            focus_block(key, id, At::Plain(len), cx);
            true
        }
        "up" if first => { if let Some(p) = prev { focus_block(key, &p.id, At::End, cx); true } else { false } }
        "down" if last => { if let Some(n) = next { focus_block(key, &n.id, At::Start, cx); true } else { false } }
        "escape" => { close_float(cx); false }
        _ => false,
    }
}

/// A block's (or cell's) source.
fn src_of(key: &str, id: &str, r: i32, c: i32, cx: &mut App) -> Option<String> {
    let b = cur_block(key, id, cx)?;
    if r >= 0 { b.rows.get(r as usize).and_then(|x| x.get(c as usize)).cloned() } else { Some(b.text) }
}

fn put_src(key: &str, id: &str, r: i32, c: i32, src: String, cx: &mut App) {
    with(cx, |s| ed_mut(s, key, |e| if let Some(b) = e.blocks.iter_mut().find(|b| b.id == id) {
        if r >= 0 { if let Some(cell) = b.rows.get_mut(r as usize).and_then(|x| x.get_mut(c as usize)) { *cell = src; } } else { b.text = src; }
    }));
}

/// Rewrite a block's (or cell's) runs, keeping the plain range selected.
#[allow(clippy::too_many_arguments)]
fn rewrite(key: &str, id: &str, r: i32, c: i32, runs: Vec<Run>, pa: usize, pb: usize, cx: &mut App) {
    let (src, map) = doc::runs_to_src_map(&runs);
    put_src(key, id, r, c, src, cx);
    changed(key, cx);
    render(key, cx);
    request_focus(key, id, r, c, doc::plain_to_src_at(&map, pa), doc::plain_to_src_at(&map, pb), cx);
}

#[allow(clippy::too_many_arguments)]
fn mark(key: &str, id: &str, r: i32, c: i32, a: usize, b: usize, m: Mark, cx: &mut App) {
    let Some(src) = src_of(key, id, r, c, cx) else { return };
    let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b)));
    if pa == pb { return; }
    let runs = doc::toggle(&doc::src_to_runs(&src), pa, pb, m);
    rewrite(key, id, r, c, runs, pa, pb, cx);
    // The format bar stays on the same selection.
    if let Some(Float::Fmt { key: k, id: i2, r: r2, c: c2, .. }) = with(cx, |s| s.float.clone()) {
        if k == key && i2 == id && r2 == r && c2 == c {
            let (_, map) = doc::runs_to_src_map(&doc::src_to_runs(&src_of(key, id, r, c, cx).unwrap_or_default()));
            with(cx, |s| if let Some(Float::Fmt { a, b, .. }) = s.float.as_mut() { *a = doc::plain_to_src_at(&map, pa); *b = doc::plain_to_src_at(&map, pb); });
        }
    }
}

/// Ctrl+V: plain text, URLs as links; a URL over selected text links it;
/// several lines become several blocks (pasteInto()).
#[allow(clippy::too_many_arguments)]
fn paste(key: &str, id: &str, r: i32, c: i32, a: usize, b: usize, cx: &mut App) -> bool {
    let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else { return false };
    let Some(src) = src_of(key, id, r, c, cx) else { return false };
    if matches!(&with(cx, |s| s.float.clone()), Some(Float::Fmt { .. })) { close_float(cx); }
    let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b)));
    let runs = doc::src_to_runs(&src);
    if doc::is_url(&text) && pa != pb {
        rewrite(key, id, r, c, doc::link(&runs, pa, pb, Some(text.trim().to_string())), pb, pb, cx);
        return true;
    }
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let (left, rest) = doc::split_runs(&runs, pa);
    let (_, right) = doc::split_runs(&rest, pb - pa);
    let lines: Vec<&str> = text.split('\n').collect();
    if r >= 0 || lines.len() == 1 {
        let ins = doc::linkify(&text);
        let n = doc::plain(&ins).chars().count();
        let all = doc::norm(left.into_iter().chain(ins).chain(right).collect());
        rewrite(key, id, r, c, all, pa + n, pa + n, cx);
        return true;
    }
    // The first line joins this block; the rest become paragraphs after it.
    let first = doc::norm(left.into_iter().chain(doc::linkify(lines[0])).collect());
    put_src(key, id, r, c, doc::runs_to_src(&first), cx);
    let mut last = id.to_string();
    for (i, l) in lines.iter().enumerate().skip(1) {
        let mut rr = doc::linkify(l);
        if i == lines.len() - 1 { rr.extend(right.clone()); }
        let nb = Block { id: doc::gen_id(), kind: "p".into(), text: doc::runs_to_src(&doc::norm(rr)), ..Default::default() };
        last = insert_after(key, Some(&last), nb, cx);
    }
    changed(key, cx);
    render(key, cx);
    focus_block(key, &last, At::End, cx);
    true
}

/// The "+" in a block's margin: a new line under it, and the slash menu
/// there (`at`: the "+" button's place; the line goes where the block ends).
pub(crate) fn add_below(key: &str, id: &str, at: Option<(f32, f32)>, cx: &mut App) {
    // Where the new line will be: under this block, its words' left edge
    // (the "+" sits in the margin, its width to the left of the words).
    let place = with(cx, |s| idx(s, key).map(|i| {
        let p = &s.places[i];
        let bottom = p.boxes.iter().find(|b| b.0 == id).map(|b| b.1 + b.2).unwrap_or(p.page_box.1 + f32::from(crundi_theme::T::SP_48));
        let x = match at { Some((x, _)) => x + f32::from(crundi_theme::T::SP_40), None => p.page_box.0 + f32::from(crundi_theme::T::SP_48) };
        (x, bottom)
    }));
    let n = insert_after(key, Some(id), Block::new("p", ""), cx);
    focus_block(key, &n, At::End, cx);
    if let Some(p) = place { with(cx, |s| s.caret_xy = p); }
    open_slash(key, &n, None, cx);
}

fn tail(key: &str, cx: &mut App) {
    let last = with(cx, |s| ed_mut(s, key, |e| e.blocks.last().cloned()).flatten());
    match last {
        Some(b) if b.kind == "p" && b.text.is_empty() => focus_block(key, &b.id, At::End, cx),
        _ => { let n = insert_after(key, last.as_ref().map(|b| b.id.as_str()), Block::new("p", ""), cx); focus_block(key, &n, At::Start, cx); }
    }
}

// ─── Block keys (the owner's: Alt+Up/Down, Ctrl+D, Ctrl+Shift+K, Tab) ───

/// Blocks lo..=hi one place up or down (Alt+Up / Alt+Down): the block past
/// the range moves to its other side. False at the page's edge.
fn move_range(key: &str, lo: usize, hi: usize, up: bool, cx: &mut App) -> bool {
    let ok = with(cx, |s| ed_mut(s, key, |e| {
        if hi >= e.blocks.len() || lo > hi { return false; }
        if up { if lo == 0 { return false; } let b = e.blocks.remove(lo - 1); e.blocks.insert(hi, b); }
        else { if hi + 1 >= e.blocks.len() { return false; } let b = e.blocks.remove(hi + 1); e.blocks.insert(lo, b); }
        true
    }).unwrap_or(false));
    if ok { changed(key, cx); render(key, cx); }
    ok
}

/// Copies of blocks lo..=hi after them (Ctrl+D); the copies' ids.
fn dup_range(key: &str, lo: usize, hi: usize, cx: &mut App) -> Vec<String> {
    let ids = with(cx, |s| ed_mut(s, key, |e| {
        if hi >= e.blocks.len() || lo > hi { return vec![]; }
        let copies: Vec<Block> = e.blocks[lo..=hi].iter().map(|b| Block { id: doc::gen_id(), ..b.clone() }).collect();
        let ids = copies.iter().map(|b| b.id.clone()).collect();
        for (k, b) in copies.into_iter().enumerate() { e.blocks.insert(hi + 1 + k, b); }
        ids
    }).unwrap_or_default());
    if !ids.is_empty() { changed(key, cx); render(key, cx); }
    ids
}

fn pos_of(key: &str, id: &str, cx: &mut App) -> Option<usize> { with(cx, |s| ed_mut(s, key, |e| e.blocks.iter().position(|b| b.id == id)).flatten()) }

/// The keys every block takes, whatever its kind, with the caret in it
/// (r, c: its cell; a, b: the caret, as source offsets): moved, duplicated
/// (the caret into the copy, at the same place), deleted.
#[allow(clippy::too_many_arguments)]
fn block_key(key: &str, id: &str, r: i32, c: i32, name: &str, a: usize, b: usize, cx: &mut App) -> Option<bool> {
    match name {
        "alt-up" | "alt-down" => {
            let i = pos_of(key, id, cx)?;
            close_float(cx);
            if move_range(key, i, i, name == "alt-up", cx) { request_focus(key, id, r, c, a, b, cx); }
            Some(true)
        }
        "ctrl-d" => {
            let i = pos_of(key, id, cx)?;
            close_float(cx);
            if let Some(n) = dup_range(key, i, i, cx).pop() { request_focus(key, &n, r, c, a, b, cx); }
            Some(true)
        }
        "ctrl-shift-k" => { close_float(cx); block_menu_do(key, id, "del", cx); Some(true) }
        _ => None,
    }
}

/// Tab / Shift+Tab on a list item: one level deeper (at most one below the
/// item above it, when that is a list item) or one level out.
fn nest(key: &str, id: &str, out: bool, cx: &mut App) {
    let (prev, _) = neighbours(key, id, cx);
    let Some(b) = cur_block(key, id, cx) else { return };
    let most = prev.filter(|p| doc::listy(&p.kind)).map(|p| p.level + 1).unwrap_or(0).min(doc::LEVEL_MAX);
    let level = if out { b.level.saturating_sub(1) } else if b.level < most { b.level + 1 } else { b.level };
    if level != b.level { edit_block(key, id, cx, |b| b.level = level); }
}

fn divider_key(key: &str, id: &str, name: &str, cx: &mut App) -> bool {
    if let Some(h) = block_key(key, id, -1, -1, name, 0, 0, cx) { return h; }
    let (prev, next) = neighbours(key, id, cx);
    match name {
        "backspace" | "delete" => {
            with(cx, |s| ed_mut(s, key, |e| remove_block(e, id)));
            changed(key, cx);
            render(key, cx);
            if let Some(p) = prev.or(next) { focus_block(key, &p.id, At::End, cx); }
            true
        }
        "enter" => { let n = insert_after(key, Some(id), Block::new("p", ""), cx); focus_block(key, &n, At::Start, cx); true }
        "up" => { if let Some(p) = prev { focus_block(key, &p.id, At::End, cx); true } else { false } }
        "down" => { if let Some(n) = next { focus_block(key, &n.id, At::Start, cx); true } else { false } }
        "ctrl-shift-a" => { select_all(key, cx); true }
        _ => false,
    }
}

fn focused(key: &str, id: &str, r: i32, c: i32, cx: &mut App) {
    let pressing = with(cx, |s| s.press.is_some());
    with(cx, |s| ed_mut(s, key, |e| {
        e.focus = Some(id.to_string());
        if r >= 0 { e.cell = (r as usize, c as usize); }
        // Back to typing in a block ends a block selection.
        if e.sel.is_some() && !pressing && id != "__sel" { e.sel = None; }
    }));
    let is_table = cur_block(key, id, cx).is_some_and(|b| b.kind == "table");
    with(cx, |s| if let Some(i) = idx(s, key) { s.places[i].focus_n += 1; s.places[i].cur_id = if is_table { id.to_string() } else { String::new() }; });
    render(key, cx);
}

fn blur(key: &str, id: &str, cx: &mut App) {
    let close = matches!(&with(cx, |s| s.float.clone()), Some(Float::Fmt { key: k, id: i, link: false, .. }) if k == key && i == id);
    if close { close_float(cx); }
    // The table's buttons go with its focus (a press on them keeps it; a
    // move to another of its cells sets it again).
    let n = with(cx, |s| idx(s, key).map(|i| s.places[i].focus_n)).unwrap_or(0);
    let (k, i) = (key.to_string(), id.to_string());
    after(cx, 200, move |cx| {
        let gone = with(cx, |s| idx(s, &k).map(|x| { let p = &mut s.places[x]; if p.focus_n == n && p.cur_id == i { p.cur_id.clear(); true } else { false } }).unwrap_or(false));
        if gone { render(&k, cx); }
    });
}

// ─── Code blocks ───

fn code_act(key: &str, id: &str, a: &str, x: f32, y: f32, cx: &mut App) {
    let Some(b) = cur_block(key, id, cx) else { return };
    match a {
        "wrap" => { edit_block(key, id, cx, |b| b.wrap = !b.wrap); }
        "copy" => { copy(&b.text, cx); toast(cx, "Code copied"); }
        "lang" => {
            close_float(cx);
            with(cx, |s| { s.float = Some(Float::Lang { key: key.into(), id: id.into(), query: String::new() }); s.menu.query.clear(); s.menu.gen_n += 1; });
            draw_lang(x - f32::from(crundi_theme::T::NOTES_MENU_W), y, cx);
        }
        _ => {}
    }
}

fn draw_lang(x: f32, y: f32, cx: &mut App) {
    let Some(Float::Lang { key, id, query }) = with(cx, |s| s.float.clone()) else { return };
    let cur = cur_block(&key, &id, cx).map(|b| doc::norm_lang(&b.lang)).unwrap_or_else(|| "plain".into());
    let items = doc::lang_items(&query);
    let act = items.iter().position(|l| l.0 == cur).unwrap_or(0) as i32;
    let rows: Vec<Mi> = items.iter().map(|l| mi(l.0, "", l.1, "", l.0 == cur, false, false)).collect();
    with(cx, |s| s.menu_ids = items.iter().map(|l| l.0.to_string()).collect());
    let (x, y) = if x < 0.0 { with(cx, |s| (s.menu.x, s.menu.y)) } else { (x, y) };
    show_menu(x, y, "", rows, if query.is_empty() { act } else { 0 }, true, &query, "No such language", cx);
}

// ─── Tables ───

fn cell_edited(key: &str, id: &str, r: i32, c: i32, t: &str, cx: &mut App) {
    if matches!(&with(cx, |s| s.float.clone()), Some(Float::Fmt { link: false, .. })) { close_float(cx); }
    put_src(key, id, r, c, t.to_string(), cx);
    changed(key, cx);
    render(key, cx);
}

#[allow(clippy::too_many_arguments)]
fn cell_key(key: &str, id: &str, r: i32, c: i32, name: &str, a: i32, cc: i32, cx: &mut App) -> bool {
    let Some(b) = cur_block(key, id, cx) else { return false };
    let (ru, cu) = (r.max(0) as usize, c.max(0) as usize);
    let cols = b.rows.first().map(|x| x.len()).unwrap_or(1);
    let (a, cc) = (a.max(0) as usize, cc.max(0) as usize);
    if let Some(h) = block_key(key, id, r, c, name, a, cc, cx) { return h; }
    match name {
        "tab" | "shift-tab" => {
            let flat = ru * cols + cu;
            if name == "tab" {
                if flat + 1 >= b.rows.len() * cols { table_act(key, id, "add-row", cx); return true; }
                let n = flat + 1;
                focus_cell_end(key, id, n / cols, n % cols, cx);
            } else if flat > 0 {
                let n = flat - 1;
                focus_cell_end(key, id, n / cols, n % cols, cx);
            }
            true
        }
        "enter" => {
            if ru + 1 < b.rows.len() { focus_cell_end(key, id, ru + 1, cu, cx); } else { let n = insert_after(key, Some(id), Block::new("p", ""), cx); focus_block(key, &n, At::Start, cx); }
            true
        }
        "ctrl-b" => { mark(key, id, r, c, a, cc, Mark::B, cx); true }
        "ctrl-i" => { mark(key, id, r, c, a, cc, Mark::I, cx); true }
        "ctrl-u" => { mark(key, id, r, c, a, cc, Mark::U, cx); true }
        "ctrl-e" => { mark(key, id, r, c, a, cc, Mark::Code, cx); true }
        "ctrl-shift-s" => { mark(key, id, r, c, a, cc, Mark::S, cx); true }
        "ctrl-k" => { open_fmt(key, id, r, c, a, cc, true, cx); true }
        "ctrl-v" => paste(key, id, r, c, a, cc, cx),
        "ctrl-shift-a" => { select_all(key, cx); true }
        _ => false,
    }
}

fn focus_cell_end(key: &str, id: &str, r: usize, c: usize, cx: &mut App) {
    let n = cur_block(key, id, cx).and_then(|b| b.rows.get(r).and_then(|x| x.get(c)).map(|s| s.len())).unwrap_or(0);
    request_focus(key, id, r as i32, c as i32, n, n, cx);
}

fn table_act(key: &str, id: &str, a: &str, cx: &mut App) {
    let Some(b) = cur_block(key, id, cx) else { return };
    let cols = b.rows.first().map(|x| x.len()).unwrap_or(2);
    let focused_here = with(cx, |s| ed_mut(s, key, |e| e.focus.as_deref() == Some(id)).unwrap_or(false));
    let (cr, cc) = with(cx, |s| ed_mut(s, key, |e| e.cell).unwrap_or((0, 0)));
    match a {
        "add-row" => {
            edit_block(key, id, cx, |b| b.rows.push(vec![String::new(); cols]));
            let n = b.rows.len();
            request_focus(key, id, n as i32, 0, 0, 0, cx);
        }
        "add-col" => {
            if cols >= 20 { toast_error(cx, "A table can have up to 20 columns here"); return; }
            edit_block(key, id, cx, |b| for r in b.rows.iter_mut() { r.push(String::new()); });
        }
        "del-row" => {
            if b.rows.len() <= 1 { return; }
            let i = if focused_here && cr < b.rows.len() { cr } else { b.rows.len() - 1 };
            edit_block(key, id, cx, |b| { b.rows.remove(i); });
        }
        "del-col" => {
            if cols <= 1 { return; }
            let i = if focused_here && cc < cols { cc } else { cols - 1 };
            edit_block(key, id, cx, |b| for r in b.rows.iter_mut() { if i < r.len() { r.remove(i); } });
        }
        "header" => { edit_block(key, id, cx, |b| b.header = !b.header); }
        _ => {}
    }
}

// ─── Block selection ───

fn sel_range(e: &Ed) -> Option<(usize, usize)> {
    let (a, f) = e.sel.as_ref()?;
    let i = e.blocks.iter().position(|b| &b.id == a)?;
    let j = e.blocks.iter().position(|b| &b.id == f)?;
    Some((i.min(j), i.max(j)))
}

fn set_sel(key: &str, a: &str, f: &str, cx: &mut App) {
    close_float(cx);
    with(cx, |s| ed_mut(s, key, |e| e.sel = Some((a.to_string(), f.to_string()))));
    request_focus(key, "__sel", -1, -1, 0, 0, cx);
}

fn clear_sel(key: &str, cx: &mut App) {
    with(cx, |s| ed_mut(s, key, |e| e.sel = None));
    render(key, cx);
}

fn selected(key: &str, cx: &mut App) -> Vec<Block> {
    with(cx, |s| ed_mut(s, key, |e| sel_range(e).map(|(lo, hi)| e.blocks[lo..=hi].to_vec()).unwrap_or_default()).unwrap_or_default())
}

fn select_all(key: &str, cx: &mut App) {
    let ends = with(cx, |s| ed_mut(s, key, |e| (e.blocks.first().map(|b| b.id.clone()), e.blocks.last().map(|b| b.id.clone()))).unwrap_or((None, None)));
    if let (Some(a), Some(b)) = ends { set_sel(key, &a, &b, cx); }
}

fn copy(text: &str, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    with(cx, |s| s.clipboard = text.to_string());
}

fn copy_md(md: &str, cx: &mut App) { if md.is_empty() { return; } copy(md, cx); toast(cx, "Copied as Markdown"); }

fn delete_selected(key: &str, cx: &mut App) {
    let r = with(cx, |s| ed_mut(s, key, |e| {
        let (lo, hi) = sel_range(e)?;
        e.blocks.drain(lo..=hi);
        e.sel = None;
        if e.blocks.is_empty() { e.blocks.push(Block::new("p", "")); }
        let before = lo.checked_sub(1).map(|i| e.blocks[i].id.clone());
        Some(match before { Some(b) => (b, true), None => (e.blocks[lo.min(e.blocks.len() - 1)].id.clone(), false) })
    }).flatten());
    let Some((id, end)) = r else { return };
    changed(key, cx);
    render(key, cx);
    focus_block(key, &id, if end { At::End } else { At::Start }, cx);
}

fn sel_act(key: &str, a: &str, cx: &mut App) {
    match a {
        "copy" => copy_md(&doc::to_markdown(&selected(key, cx)), cx),
        "all" => select_all(key, cx),
        "delete" => delete_selected(key, cx),
        "clear" => clear_sel(key, cx),
        _ => {}
    }
}

/// Keys while blocks are selected.
fn sel_key(key: &str, name: &str, cx: &mut App) -> bool {
    let Some((sa, sf)) = with(cx, |s| ed_mut(s, key, |e| e.sel.clone()).flatten()) else { return false };
    let ids: Vec<String> = with(cx, |s| ed_mut(s, key, |e| e.blocks.iter().map(|b| b.id.clone()).collect()).unwrap_or_default());
    match name {
        "escape" => { clear_sel(key, cx); focus_block(key, &sf, At::End, cx); true }
        "backspace" | "delete" => { delete_selected(key, cx); true }
        "ctrl-c" => { copy_md(&doc::to_markdown(&selected(key, cx)), cx); true }
        "ctrl-a" | "ctrl-shift-a" => { select_all(key, cx); true }
        "ctrl-shift-k" => { delete_selected(key, cx); true }
        "alt-up" | "alt-down" => {
            // The selection moves as one, and stays selected.
            if let Some((lo, hi)) = with(cx, |s| ed_mut(s, key, |e| sel_range(e)).flatten()) { move_range(key, lo, hi, name == "alt-up", cx); }
            true
        }
        "ctrl-d" => {
            // The copies, after the selection, are selected.
            if let Some((lo, hi)) = with(cx, |s| ed_mut(s, key, |e| sel_range(e)).flatten()) {
                let n = dup_range(key, lo, hi, cx);
                if let (Some(f), Some(l)) = (n.first(), n.last()) { set_sel(key, f, l, cx); }
            }
            true
        }
        "shift-up" | "shift-down" => {
            let k = ids.iter().position(|i| *i == sf).unwrap_or(0) as i64 + if name == "shift-up" { -1 } else { 1 };
            if k >= 0 && (k as usize) < ids.len() { set_sel(key, &sa, &ids[k as usize], cx); }
            true
        }
        "up" | "down" => {
            let sel = selected(key, cx);
            let t = if name == "up" { sel.first() } else { sel.last() }.map(|b| b.id.clone());
            clear_sel(key, cx);
            if let Some(t) = t { focus_block(key, &t, if name == "up" { At::Start } else { At::End }, cx); }
            true
        }
        "enter" => { clear_sel(key, cx); focus_block(key, &sf, At::End, cx); true }
        _ => false,
    }
}

/// A press in a block's margin (Shift: select from the caret's block).
/// Blocks are selected on this place's page.
fn has_sel(key: &str, cx: &mut App) -> bool { with(cx, |s| ed_mut(s, key, |e| e.sel.is_some()).unwrap_or(false)) }

fn blk_press(key: &str, id: &str, shift: bool, cx: &mut App) {
    if shift {
        let from = with(cx, |s| ed_mut(s, key, |e| e.sel.as_ref().map(|x| x.0.clone()).or_else(|| e.focus.clone())).flatten());
        if let Some(f) = from.filter(|f| f != id && f != "title") { set_sel(key, &f, id, cx); return; }
    }
    if with(cx, |s| ed_mut(s, key, |e| e.sel.is_some()).unwrap_or(false)) { clear_sel(key, cx); }
    with(cx, |s| s.press = Some((key.to_string(), id.to_string(), false)));
}

/// Dragging from a margin across blocks selects them.
fn blk_over(key: &str, y: f32, cx: &mut App) {
    let Some((k, from, moved)) = with(cx, |s| s.press.clone()) else { return };
    if k != key { return; }
    let under = with(cx, |s| idx(s, key).and_then(|i| s.places[i].boxes.iter().find(|b| y >= b.1 && y < b.1 + b.2).map(|b| b.0.clone())));
    let Some(under) = under else { return };
    if under != from || moved {
        let again = moved && with(cx, |s| ed_mut(s, key, |e| e.sel.as_ref().is_some_and(|x| x.1 == under)).unwrap_or(false));
        with(cx, |s| s.press = Some((k, from.clone(), true)));
        if !again { set_sel(key, &from, &under, cx); }
    }
}

// ─── Dragging blocks within the page to move them ───

/// What a grip drags: the place, the blocks, their Markdown.
fn drag_what(key: &str, id: &str, cx: &mut App) -> Option<(String, Vec<String>, String)> {
    Some(if key == "__fmt" {
        // The format bar's grip: the selected text.
        match with(cx, |s| s.float.clone()) {
            Some(Float::Fmt { key, id, r, c, a, b, .. }) => {
                let src = src_of(&key, &id, r, c, cx).unwrap_or_default();
                let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b)));
                let (_, rest) = doc::split_runs(&doc::src_to_runs(&src), pa);
                let (mid, _) = doc::split_runs(&rest, pb - pa);
                (key, vec![], doc::runs_md(&mid).trim().to_string())
            }
            _ => return None,
        }
    } else if id.is_empty() {
        let sel = selected(key, cx);
        (key.to_string(), sel.iter().map(|b| b.id.clone()).collect(), doc::to_markdown(&sel))
    } else {
        let sel = selected(key, cx);
        if sel.len() > 1 && sel.iter().any(|b| b.id == id) {
            (key.to_string(), sel.iter().map(|b| b.id.clone()).collect(), doc::to_markdown(&sel))
        } else {
            let b = cur_block(key, id, cx).into_iter().collect::<Vec<_>>();
            (key.to_string(), vec![id.to_string()], doc::to_markdown(&b))
        }
    })
}

fn label_of(text: &str) -> String { text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").chars().take(60).collect::<String>() }

/// A grip's drag (the shared drag layer's reference, "note": its Markdown):
/// a chat's box or a terminal takes the Markdown as it is; a page of these
/// notes moves the blocks.
pub(crate) fn drag_payload(key: &str, id: &str, cx: &mut App) -> Option<w::RefDrag> {
    let (_, _, text) = drag_what(key, id, cx)?;
    let label = label_of(&text);
    Some(w::RefDrag::new("note", &text, label))
}

/// The drag begins: the blocks in hand (a drop on a page moves them).
fn drag_start(key: &str, id: &str, cx: &mut App) {
    let Some((k, ids, text)) = drag_what(key, id, cx) else { return };
    let label = label_of(&text);
    with(cx, |s| { s.drag = Some(Drag { key: k, ids, spot: None }); s.drag_label = label; });
}

/// The place whose blocks are in hand.
pub(crate) fn drag_key(cx: &App) -> Option<String> { cx.try_global::<Handle>().and_then(|h| h.0.read(cx).drag.as_ref().map(|d| d.key.clone())) }

/// A reference from another panel (a file, a task, an idea…) dropped on a
/// page: its text goes in under the block at the pointer (the web's
/// dropText), and a toast says so.
pub(crate) fn drop_text(key: &str, y: f32, text: &str, cx: &mut App) {
    if text.trim().is_empty() { return; }
    let after = with(cx, |s| idx(s, key).and_then(|i| {
        let p = &s.places[i];
        p.boxes.iter().filter(|b| b.1 <= y).max_by(|a, b| a.1.total_cmp(&b.1)).map(|b| b.0.clone())
    }));
    let has_page = with(cx, |s| ed_mut(s, key, |_| ()).is_some());
    if !has_page { return; }
    let n = insert_after(key, after.as_deref(), Block::new("p", &doc::runs_to_src(&[doc::Run { text: text.to_string(), st: Default::default() }])), cx);
    focus_block(key, &n, At::End, cx);
    toast(cx, "Added to the note");
}

/// The pointer moved with a block in hand: where it would land (window px).
fn drag_move(_x: f32, y: f32, cx: &mut App) {
    let Some(d) = with(cx, |s| s.drag.clone()) else { return };
    let key = d.key.clone();
    let spot = with(cx, |s| idx(s, &key).and_then(|i| {
        let p = &s.places[i];
        let ed = p.ed.as_ref()?;
        if d.ids.is_empty() { return None; }
        let order: Vec<(usize, &(String, f32, f32))> = ed.blocks.iter().enumerate().filter(|(_, b)| !d.ids.contains(&b.id)).filter_map(|(k, b)| p.boxes.iter().find(|x| x.0 == b.id).map(|x| (k, x))).collect();
        if order.is_empty() { return None; }
        let mut spot = (order[order.len() - 1].0, false, order[order.len() - 1].1 .1 + order[order.len() - 1].1 .2);
        for (k, b) in &order { if y < b.1 + b.2 / 2.0 { spot = (*k, true, b.1); break; } }
        Some(spot)
    }));
    let old = with(cx, |s| idx(s, &key).and_then(|i| s.places[i].drop_y));
    with(cx, |s| {
        if let Some(dd) = s.drag.as_mut() { dd.spot = spot.map(|x| (x.0, x.1)); }
        if let Some(i) = idx(s, &key) { s.places[i].drop_y = spot.map(|x| x.2); }
    });
    if old != spot.map(|x| x.2) { render(&key, cx); }
}

fn drag_end(commit: bool, cx: &mut App) {
    let Some(d) = with(cx, |s| s.drag.take()) else { return };
    with(cx, |s| if let Some(i) = idx(s, &d.key) { s.places[i].drop_y = None; });
    render(&d.key, cx);
    if !commit { return; }
    if let Some((k, before)) = d.spot {
        let anchor = with(cx, |s| ed_mut(s, &d.key, |e| e.blocks.get(k).map(|b| b.id.clone())).flatten());
        with(cx, |s| ed_mut(s, &d.key, |e| {
            let moving: Vec<Block> = e.blocks.iter().filter(|b| d.ids.contains(&b.id)).cloned().collect();
            e.blocks.retain(|b| !d.ids.contains(&b.id));
            let at = anchor.and_then(|a| e.blocks.iter().position(|b| b.id == a)).map(|i| if before { i } else { i + 1 }).unwrap_or(e.blocks.len());
            for (j, b) in moving.into_iter().enumerate() { e.blocks.insert(at + j, b); }
        }));
        changed(&d.key, cx);
        render(&d.key, cx);
    }
    // (A chat's box or a terminal took the Markdown itself: the shared drag layer.)
}

// ─── Menus ───

fn mi(id: &str, k: &str, name: &str, d: &str, check: bool, danger: bool, sep: bool) -> Mi {
    Mi { id: id.into(), k: k.into(), name: name.into(), d: d.into(), check, danger, sep }
}

#[allow(clippy::too_many_arguments)]
fn show_menu(x: f32, y: f32, label: &str, items: Vec<Mi>, act: i32, query_on: bool, query: &str, empty: &str, cx: &mut App) {
    with(cx, |s| {
        let m = &mut s.menu;
        m.x = x;
        m.y = y;
        m.label = label.into();
        m.items = items;
        m.act = act;
        if !query_on { m.query.clear(); } else if m.query != query { m.query = query.into(); }
        if !m.query_on && query_on { m.gen_n += 1; }
        m.query_on = query_on;
        m.empty = empty.into();
        m.open = true;
    });
    render_float(cx);
}

fn close_float(cx: &mut App) {
    let k = with(cx, |s| { s.menu.open = false; s.menu.query_on = false; s.fmt.open = false; s.fmt.link = false; s.float.take().map(|f| f.key().to_string()) });
    if let Some(k) = k { render(&k, cx); }
}

fn close_float_of(key: &str, cx: &mut App) {
    let mine = with(cx, |s| s.float.as_ref().is_some_and(|f| f.key() == key));
    if mine { close_float(cx); }
}

/// The slash menu ("/" typed, or the + button): turn into, or add.
fn open_slash(key: &str, id: &str, start: Option<usize>, cx: &mut App) {
    close_float(cx);
    with(cx, |s| { s.float = Some(Float::Slash { key: key.into(), id: id.into(), start, query: String::new() }); s.menu.act = 0; });
    draw_slash(cx);
}

fn draw_slash(cx: &mut App) {
    let Some(Float::Slash { query, .. }) = with(cx, |s| s.float.clone()) else { return };
    let items = doc::slash_items(&query);
    with(cx, |s| s.menu_ids = items.iter().map(|t| t.0.to_string()).collect());
    let rows: Vec<Mi> = items.iter().map(|t| mi(t.0, t.1, t.2, t.3, false, false, false)).collect();
    let label = if query.is_empty() { "Turn this into, or add".to_string() } else { format!("Blocks matching \u{201c}{query}\u{201d}") };
    let (x, y) = with(cx, |s| s.caret_xy);
    let line = f32::from(crundi_theme::T::TEXT_15) * rd(cx, scale_of);
    let act = with(cx, |s| s.menu.act).clamp(0, (rows.len() as i32 - 1).max(0));
    show_menu(x, y + line * 1.6 + 6.0, &label, rows, if items.is_empty() { -1 } else { act }, false, "", "No such block", cx);
}

fn choose_slash(t: &str, cx: &mut App) {
    let Some(Float::Slash { key, id, start, query }) = with(cx, |s| s.float.clone()) else { return };
    close_float(cx);
    // Drop the "/query" that was typed, then convert (or add below when this block has text).
    if let (Some(st), Some(b)) = (start, cur_block(&key, &id, cx)) {
        let runs = doc::src_to_runs(&b.text);
        let plain: Vec<char> = doc::plain(&runs).chars().collect();
        let cut: Vec<char> = format!("/{query}").chars().collect();
        if plain.len() >= st + cut.len() && plain[st..st + cut.len()] == cut[..] {
            let (l, rest) = doc::split_runs(&runs, st);
            let (_, r) = doc::split_runs(&rest, cut.len());
            set_text(&key, &id, doc::runs_to_src(&doc::norm(l.into_iter().chain(r).collect())), cx);
        }
    }
    let Some(b) = cur_block(&key, &id, cx) else { return };
    let has_text = !doc::src_plain(&b.text).trim().is_empty();
    let target = if has_text && doc::texty(&b.kind) { insert_after(&key, Some(&id), Block::new("p", ""), cx) } else { id.clone() };
    convert(&key, &target, t, false, None, cx);
    if t == "divider" { let n = insert_after(&key, Some(&target), Block::new("p", ""), cx); focus_block(&key, &n, At::Start, cx); }
    else { focus_block(&key, &target, if t == "table" { At::Start } else { At::End }, cx); }
}

/// The grip's menu: Turn into, Move up, Move down, Duplicate, Delete.
fn block_menu(key: &str, id: &str, x: f32, y: f32, cx: &mut App) {
    close_float(cx);
    let Some(b) = cur_block(key, id, cx) else { return };
    let mut items = vec![];
    let mut ids = vec![];
    for t in doc::TYPES.iter().filter(|t| t.0 != b.kind) { items.push(mi(t.0, t.1, t.2, "", false, false, false)); ids.push(format!("to:{}", t.0)); }
    items.push(mi("up", "\u{2191}", "Move up", "", false, false, true)); ids.push("up".into());
    items.push(mi("down", "\u{2193}", "Move down", "", false, false, false)); ids.push("down".into());
    items.push(mi("dup", "\u{2398}", "Duplicate", "", false, false, false)); ids.push("dup".into());
    items.push(mi("del", "\u{2715}", "Delete", "", false, true, false)); ids.push("del".into());
    with(cx, |s| { s.float = Some(Float::Block { key: key.into(), id: id.into() }); s.menu_ids = ids; });
    show_menu(x, y, "Turn into", items, -1, false, "", "", cx);
}

fn block_menu_do(key: &str, id: &str, what: &str, cx: &mut App) {
    if let Some(t) = what.strip_prefix("to:") { convert(key, id, t, false, None, cx); focus_block(key, id, At::End, cx); return; }
    match what {
        "up" | "down" => {
            let moved = with(cx, |s| ed_mut(s, key, |e| {
                let i = e.blocks.iter().position(|b| b.id == id)?;
                let j = if what == "up" { i.checked_sub(1)? } else { (i + 1 < e.blocks.len()).then_some(i + 1)? };
                e.blocks.swap(i, j);
                Some(())
            }).flatten());
            if moved.is_some() { changed(key, cx); render(key, cx); }
        }
        "dup" => {
            let Some(mut b) = cur_block(key, id, cx) else { return };
            b.id = doc::gen_id();
            let n = insert_after(key, Some(id), b, cx);
            focus_block(key, &n, At::End, cx);
        }
        "del" => {
            let (prev, next) = neighbours(key, id, cx);
            with(cx, |s| ed_mut(s, key, |e| remove_block(e, id)));
            changed(key, cx);
            render(key, cx);
            if let Some(p) = prev.or(next) { focus_block(key, &p.id, At::End, cx); }
        }
        _ => {}
    }
}

fn menu_pick(i: i32, cx: &mut App) {
    let f = with(cx, |s| s.float.clone());
    let id = with(cx, |s| s.menu_ids.get(i.max(0) as usize).cloned());
    let Some(id) = id else { return };
    match f {
        Some(Float::Slash { .. }) => choose_slash(&id, cx),
        Some(Float::Block { key, id: bid }) => { close_float(cx); block_menu_do(&key, &bid, &id, cx); }
        Some(Float::Lang { key, id: bid, .. }) => {
            close_float(cx);
            edit_block(&key, &bid, cx, |b| b.lang = if id == "plain" { String::new() } else { id.clone() });
            focus_block(&key, &bid, At::End, cx);
        }
        Some(Float::Item { id: pid }) => {
            close_float(cx);
            if id == "trash" {
                let p = pid.clone();
                page_act("delete", &pid, cx, move |_, cx| {
                    if with(cx, |s| s.tab.current.as_deref() == Some(p.as_str())) { with(cx, |s| s.tab.current = None); tab_empty("Moved to the trash. Pick a page, or start a new one.", cx); }
                    tab_reload(cx);
                });
            }
        }
        _ => close_float(cx),
    }
}

fn menu_query(q: &str, cx: &mut App) {
    if let Some(Float::Lang { key, id, .. }) = with(cx, |s| s.float.clone()) {
        with(cx, |s| { s.float = Some(Float::Lang { key, id, query: q.to_string() }); s.menu.query = q.to_string(); });
        draw_lang(-1.0, 0.0, cx);
    }
}

fn menu_key(name: &str, cx: &mut App) -> bool {
    let n = with(cx, |s| s.menu_ids.len()) as i32;
    let act = with(cx, |s| s.menu.act);
    match name {
        "down" => { with(cx, |s| s.menu.act = (act + 1).min(n - 1).max(0)); render_float(cx); true }
        "up" => { with(cx, |s| s.menu.act = (act - 1).max(0)); render_float(cx); true }
        "enter" | "tab" => { if n > 0 { menu_pick(act.max(0), cx); } true }
        "escape" => {
            let f = with(cx, |s| s.float.clone());
            close_float(cx);
            if let Some(Float::Lang { key, id, .. }) = f { focus_block(&key, &id, At::End, cx); }
            true
        }
        _ => false,
    }
}

// ─── The format bar ───

/// The caret or selection moved in a block (x, y: the caret, window px).
#[allow(clippy::too_many_arguments)]
fn sel_moved(key: &str, id: &str, a: i32, c: i32, x: f32, y: f32, cx: &mut App) {
    with(cx, |s| s.caret_xy = (x, y));
    let (a, c) = (a.max(0) as usize, c.max(0) as usize);
    let (r, cc) = with(cx, |s| ed_mut(s, key, |e| if cur_block_kind(e, id) == "table" { (e.cell.0 as i32, e.cell.1 as i32) } else { (-1, -1) }).unwrap_or((-1, -1)));
    let kind = cur_block(key, id, cx).map(|b| b.kind).unwrap_or_default();
    if kind == "code" { return; }
    let fl = with(cx, |s| s.float.clone());
    if a != c {
        // Over the selection's middle, its top (the web's showFormatBar).
        if !matches!(fl, Some(Float::Fmt { link: true, .. })) { open_fmt(key, id, r, cc, a, c, false, cx); with(cx, |s| { let (ax, ay) = s.sel_anchor; s.fmt.x = (ax + x) / 2.; s.fmt.y = ay.min(y); }); }
    } else {
        with(cx, |s| s.sel_anchor = (x, y));
        if matches!(fl, Some(Float::Fmt { link: false, .. })) { close_float(cx); }
    }
}

fn cur_block_kind(e: &Ed, id: &str) -> String { block_of(e, id).map(|(_, b)| b.kind.clone()).unwrap_or_default() }

#[allow(clippy::too_many_arguments)]
fn open_fmt(key: &str, id: &str, r: i32, c: i32, a: usize, b: usize, link: bool, cx: &mut App) {
    let keep_menu = matches!(with(cx, |s| s.float.clone()), Some(Float::Fmt { .. }));
    if !keep_menu { close_float(cx); }
    with(cx, |s| s.float = Some(Float::Fmt { key: key.into(), id: id.into(), r, c, a, b, link }));
    let href = src_of(key, id, r, c, cx).map(|src| { let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b))); doc::link_at(&doc::src_to_runs(&src), pa, pb) }).unwrap_or_default();
    with(cx, |s| {
        let (x, y) = s.caret_xy;
        let f = &mut s.fmt;
        if !f.open { f.x = x; f.y = y; }
        f.link = link;
        f.link_value = href.clone();
        f.has_link = !href.is_empty();
        f.gen_n += 1;
        f.open = true;
    });
    render(key, cx);
}

fn fmt_act(a: &str, cx: &mut App) {
    let Some(Float::Fmt { key, id, r, c, a: sa, b: sb, .. }) = with(cx, |s| s.float.clone()) else { return };
    let m = match a { "bold" => Some(Mark::B), "italic" => Some(Mark::I), "underline" => Some(Mark::U), "strike" => Some(Mark::S), "code" => Some(Mark::Code), _ => None };
    if let Some(m) = m { mark(&key, &id, r, c, sa, sb, m, cx); return; }
    match a {
        "link" => open_fmt(&key, &id, r, c, sa, sb, true, cx),
        "copy" => {
            let src = src_of(&key, &id, r, c, cx).unwrap_or_default();
            let (pa, pb) = (doc::src_to_plain_at(&src, sa.min(sb)), doc::src_to_plain_at(&src, sa.max(sb)));
            let (_, rest) = doc::split_runs(&doc::src_to_runs(&src), pa);
            let (mid, _) = doc::split_runs(&rest, pb - pa);
            copy_md(doc::runs_md(&mid).trim(), cx);
        }
        "all" => select_all(&key, cx),
        _ => {}
    }
}

fn link_apply(url: &str, cx: &mut App) {
    let Some(Float::Fmt { key, id, r, c, a, b, .. }) = with(cx, |s| s.float.clone()) else { return };
    close_float(cx);
    let Some(src) = src_of(&key, &id, r, c, cx) else { return };
    let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b)));
    let runs = doc::src_to_runs(&src);
    let href = (!url.trim().is_empty()).then(|| doc::fix_url(url));
    // A collapsed caret inside a link: the whole link.
    let (pa, pb) = if pa == pb { link_span(&runs, pa).unwrap_or((pa, pb)) } else { (pa, pb) };
    if pa == pb { request_focus(&key, &id, r, c, a, b, cx); return; }
    rewrite(&key, &id, r, c, doc::link(&runs, pa, pb, href), pa, pb, cx);
}

fn link_cancel(cx: &mut App) {
    let f = with(cx, |s| s.float.clone());
    close_float(cx);
    if let Some(Float::Fmt { key, id, r, c, a, b, .. }) = f { request_focus(&key, &id, r, c, a, b, cx); }
}

fn link_span(runs: &[Run], at: usize) -> Option<(usize, usize)> {
    let mut k = 0;
    for r in runs {
        let n = r.text.chars().count();
        if r.st.href.is_some() && at >= k && at <= k + n { return Some((k, k + n)); }
        k += n;
    }
    None
}

fn open_link(u: &str, cx: &mut App) {
    if !(u.starts_with("http://") || u.starts_with("https://") || u.starts_with("mailto:")) { return; }
    if std::env::var_os("CRUNDI_TEST_PORT").is_some() { return; }
    cx.open_url(u);
}

// ─── For the test driver ───

/// Block i's id on a place.
fn bid(key: &str, i: i32, cx: &mut App) -> String {
    rd(cx, |s| idx(s, key).and_then(|x| s.places[x].ed.as_ref()).and_then(|e| e.blocks.get(i.max(0) as usize).map(|b| b.id.clone()))).unwrap_or_default()
}

/// The driver's "planning:notes-*" actions (native-gpui's names), and
/// "add-pane notes" (a new Notes pane, as "+" → Notes).
pub fn test_ui(_: &Stores, name: &str, args: &[String], _window: &mut Window, cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let n = |i: usize| a(i).parse::<i32>().unwrap_or(0);
    if name == "add-pane" && a(0) == "notes" { add_pane(cx); return Some(json!({ "ok": true })); }
    if !name.starts_with("notes-") { return None; }
    Some(match name {
        "notes-attach" => { attach(&a(0), cx); json!({ "ok": true }) }
        "notes-new" => { new_page(&a(0), cx); json!({ "ok": true }) }
        "notes-pick" => { pick(&a(0), &a(1), cx); json!({ "ok": true }) }
        "notes-pages" => { switch_to_picker(&a(0), cx); json!({ "ok": true }) }
        "notes-search" => { search(&a(0), &a(1), cx); json!({ "ok": true }) }
        "notes-view" => { set_view(&a(0), cx); json!({ "ok": true }) }
        "notes-font" => { set_font(n(0), cx); json!({ "ok": true }) }
        "notes-title" => { title_edited(&a(0), &a(1), cx); json!({ "ok": true }) }
        // Type into block i: its whole new source and the caret (byte) after it.
        "notes-type" => { let k = a(0); let id = bid(&k, n(1), cx); let t = a(2); let c = if a(3).is_empty() { t.len() as i32 } else { n(3) }; edited(&k, &id, &t, c, cx); json!({ "ok": true, "id": id }) }
        "notes-key" => { let k = a(0); let id = bid(&k, n(1), cx); let r = text_key(&k, &id, &a(2), n(3), n(4), a(5) != "false", a(6) != "false", cx); json!({ "ok": true, "handled": r }) }
        "notes-cell" => { let k = a(0); let id = bid(&k, n(1), cx); cell_edited(&k, &id, n(2), n(3), &a(4), cx); json!({ "ok": true }) }
        "notes-cell-key" => { let k = a(0); let id = bid(&k, n(1), cx); let r = cell_key(&k, &id, n(2), n(3), &a(4), n(5), n(6), cx); json!({ "ok": true, "handled": r }) }
        "notes-table" => { let k = a(0); let id = bid(&k, n(1), cx); table_act(&k, &id, &a(2), cx); json!({ "ok": true }) }
        "notes-check" => { let k = a(0); let id = bid(&k, n(1), cx); let v = a(2) == "true"; edit_block(&k, &id, cx, |b| b.checked = v); json!({ "ok": true }) }
        "notes-grip" => { let k = a(0); let id = bid(&k, n(1), cx); block_menu(&k, &id, 300.0, 300.0, cx); json!({ "ok": true }) }
        "notes-add-below" => { let k = a(0); let id = bid(&k, n(1), cx); add_below(&k, &id, None, cx); json!({ "ok": true }) }
        "notes-code" => { let k = a(0); let id = bid(&k, n(1), cx); code_act(&k, &id, &a(2), 600.0, 300.0, cx); json!({ "ok": true }) }
        "notes-menu-pick" => { let want = a(0); let i = rd(cx, |s| s.menu_ids.iter().position(|x| *x == want)).map(|i| i as i32).unwrap_or(-1); if i >= 0 { menu_pick(i, cx); } json!({ "ok": i >= 0 }) }
        "notes-menu-key" => { json!({ "ok": true, "handled": menu_key(&a(0), cx) }) }
        "notes-menu-query" => { menu_query(&a(0), cx); json!({ "ok": true }) }
        "notes-menu-close" => { close_float(cx); json!({ "ok": true }) }
        "notes-fmt" => { let k = a(0); let id = bid(&k, n(1), cx); with(cx, |s| s.caret_xy = (500.0, 300.0)); open_fmt(&k, &id, -1, -1, n(2) as usize, n(3) as usize, false, cx); json!({ "ok": true }) }
        "notes-fmt-act" => { fmt_act(&a(0), cx); json!({ "ok": true }) }
        "notes-link" => { link_apply(&a(0), cx); json!({ "ok": true }) }
        "notes-select" => { let k = a(0); let x = bid(&k, n(1), cx); let y = bid(&k, n(2), cx); set_sel(&k, &x, &y, cx); json!({ "ok": true }) }
        "notes-sel-act" => { sel_act(&a(0), &a(1), cx); json!({ "ok": true }) }
        "notes-sel-key" => { json!({ "ok": true, "handled": sel_key(&a(0), &a(1), cx) }) }
        "notes-move" => {
            // Move block i before block j (the grip's drag, done directly).
            let k = a(0); let id = bid(&k, n(1), cx);
            let to = n(2) as usize;
            with(cx, |s| s.drag = Some(Drag { key: k.clone(), ids: vec![id], spot: Some((to, true)) }));
            drag_end(true, cx);
            json!({ "ok": true })
        }
        "notes-item-menu" => { item_menu(&a(0), 400.0, 300.0, cx); json!({ "ok": true }) }
        "notes-trash" => { trash_act(&a(0), &a(1), cx); json!({ "ok": true }) }
        "notes-flush" => { flush(&a(0), cx); json!({ "ok": true }) }
        "notes-reload" => { tab_reload(cx); let keys: Vec<String> = rd(cx, |s| s.places.iter().filter(|p| !p.key.is_empty() && p.ed.is_none()).map(|p| p.key.clone()).collect()); for k in keys { reload_picker(&k, cx); } json!({ "ok": true }) }
        "notes-hosted" => { with(cx, |s| s.head_hosted = a(0) == "true"); render_all(cx); json!({ "ok": true }) }
        _ => return None,
    })
}

/// planningArea.notes for the driver (native-gpui's names).
pub fn state(_: &Stores, _window: &mut Window, cx: &mut App) -> Value {
    let s = store(cx);
    let s = s.read(cx);
    let places: Vec<Value> = s.places.iter().filter_map(|p| place_row(s, &p.key)).map(|p| json!({
        "key": p.key, "mode": p.mode, "msg": p.msg, "pageId": p.page_id, "title": p.title,
        "save": p.save_state, "saveText": p.save_text, "saveTip": p.save_tip, "selCount": p.sel_count,
        "focus": p.focus.0, "focusA": p.focus.3, "focusB": p.focus.4, "curId": p.cur_id, "drop": p.drop_y.is_some(),
        "headTitle": pane_title_in(s, &p.key),
        "blocks": p.blocks.iter().map(|r| json!({ "id": r.b.id, "kind": r.b.kind, "src": if r.b.kind == "table" || r.b.kind == "divider" { String::new() } else { r.b.text.clone() }, "num": r.num, "checked": r.b.checked,
            "lang": doc::lang_name(&doc::norm_lang(&r.b.lang)), "wrap": r.b.wrap, "header": r.b.header, "sel": r.sel, "menuOpen": r.menu_open,
            "rows": r.b.rows, "level": r.b.level })).collect::<Vec<_>>(),
        "items": p.items.iter().map(|i| json!({ "id": i.id, "title": i.title, "meta": i.meta, "busy": i.busy, "preview": i.preview })).collect::<Vec<_>>(),
        "listMsg": p.list_msg, "newBusy": p.new_busy,
    })).collect();
    // Where things are (window px): each block's box and "+", the page's box.
    let places: Vec<Value> = places.into_iter().map(|mut v| {
        let key = v["key"].as_str().unwrap_or("").to_string();
        if let Some(pl) = s.places.iter().find(|p| p.key == key) {
            v["boxes"] = json!(pl.boxes.iter().map(|b| json!({ "id": b.0, "y": b.1, "h": b.2 })).collect::<Vec<_>>());
            v["plus"] = json!(pl.plus.iter().map(|b| json!({ "id": b.0, "x": b.1, "y": b.2 })).collect::<Vec<_>>());
            v["pageBox"] = json!({ "x": pl.page_box.0, "y": pl.page_box.1, "w": pl.page_box.2, "h": pl.page_box.3 });
        }
        if let Some(view) = s.views.get(&key) { v["edit"] = view.read(cx).edit_state(cx); }
        v
    }).collect();
    let (items, list_msg) = tab_list(s);
    let tab_page = s.places.iter().find(|p| p.key.is_empty()).and_then(|p| p.ed.as_ref().map(|e| json!({ "blocks": e.blocks.iter().map(|b| b.to_json()).collect::<Vec<_>>(), "dirty": e.dirty, "version": e.version }))).unwrap_or(Value::Null);
    let float = match &s.float {
        Some(Float::Slash { .. }) => "slash", Some(Float::Block { .. }) => "block", Some(Float::Lang { .. }) => "lang", Some(Float::Item { .. }) => "item", Some(Float::Fmt { link: true, .. }) => "link", Some(Float::Fmt { .. }) => "fmt", None => "",
    };
    let m = &s.menu;
    json!({
        "ready": s.ready, "scale": scale_of(s), "font": s.font, "hosted": s.head_hosted,
        "tab": { "project": s.tab_project, "view": s.tab.view, "open": s.tab.open, "current": s.tab.current,
            "items": items.iter().map(|i| json!({ "id": i.id, "title": i.title, "meta": i.meta, "busy": i.busy, "cur": i.cur, "preview": i.preview })).collect::<Vec<_>>(),
            "listMsg": list_msg, "newBusy": s.tab.new_busy },
        "places": places,
        "tabPage": tab_page,
        "float": float,
        "menu": if m.open { json!({ "x": m.x, "y": m.y, "label": m.label, "act": m.act, "query": m.query, "queryOn": m.query_on, "empty": m.empty,
            "items": m.items.iter().map(|i| json!({ "id": i.id, "k": i.k, "name": i.name, "d": i.d, "check": i.check, "danger": i.danger })).collect::<Vec<_>>() }) } else { Value::Null },
        "fmt": if s.fmt.open { json!({ "link": s.fmt.link, "value": s.fmt.link_value, "hasLink": s.fmt.has_link }) } else { Value::Null },
        "drag": s.drag.is_some(),
        // The text this screen last copied (the driver has no clipboard state).
        "clipboard": s.clipboard,
    })
}
