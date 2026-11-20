//! The Mind map: the mindmap tab and the workbench's "panel:mindmap" pane.
//! The web's loadMindmap, renderMindmap, layoutMindmap, the keyboard (mmStep,
//! the Delete warning, mmStartDraft / mmDraftCommit, mmStartEdit /
//! mmFinishEdit), zoom, the filter, the project chips, drag to rearrange
//! (mindmapDragHandlers), the idea dialog, the link picker and the shortcut
//! sheet (src/webapp-html.js), as native-gpui's planning/mindmap.rs has them.
//!
//! On v2's rules: the map's state is one entity (`M`, its handle in a
//! Global read by handlers only), laid out once per change (`render`: the
//! tree, every idea's height from its words, the links); the map's pan is
//! its own entity (`Pan`), so a scroll redraws the map alone. draw.rs: the
//! views (the toolbar, the dots, the map, which draws only the ideas and
//! links on screen, and the three dialogs in the modal layer). tree.rs and
//! help.rs: the pure rules (unit-tested). The map is global (every
//! project's ideas), loaded whole after each change and on the server's
//! `mindmap` event.

pub(crate) mod tree;
pub(crate) mod help;
mod draw;
mod idea_el;

use crate::util::{after, enc, in_window, truthy};
use crundi_core::live::Event;
use crundi_shell::Stores;
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet, VecDeque}, sync::Arc, time::{Duration, Instant}};
use tree::{Draft, DropAt, Kind, Layout, Node, Target};

struct Save { tmp: String, text: String, parent: Option<String> }

/// A box on the canvas as drawn (the Slint client's PlanningMmNode): `kind`
/// "node", "ghost" (the "add idea" under `parent`), "draft" or "drop"; `x`
/// its left and `y` its middle, unscaled; `h` its height.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Row {
    id: String,
    kind: &'static str,
    parent: String,
    x: f32,
    y: f32,
    h: f32,
    text: String,
    root: bool,
    linked: bool,
    scoped: bool,
    pending: bool,
    chip: String,
    chip_text: String,
    chip_tip: String,
    todo: i32,
    notes: usize,
    kids: bool,
    collapsed: bool,
    hit: bool,
    dim: bool,
    hidden: bool,
    lifted: bool,
    target: bool,
    flash: bool,
}

#[derive(Clone, Debug, Default)]
struct Chip { key: String, label: String, off: bool }

#[derive(Clone, Debug, Default)]
struct LinkRow { task: String, todo: String, text: String, status: String, done: bool }

#[derive(Clone, Default)]
struct Thumb { id: String, name: String, kind: String, image: Option<Arc<Image>> }

/// A press on an idea, not yet a click or a drag (window coordinates).
#[derive(Clone)]
struct Press { id: String, x: f32, y: f32, moved: bool }

/// The text boxes and focus places, made once the window is there.
struct Ui {
    draft: Entity<w::TextareaState>,
    edit: Entity<w::TextareaState>,
    search: Entity<w::InputState>,
    d_text: Entity<w::TextareaState>,
    d_notes: Vec<Entity<w::TextareaState>>,
    l_search: Entity<w::InputState>,
    canvas: FocusHandle,
    d_focus: FocusHandle,
    l_focus: FocusHandle,
    h_focus: FocusHandle,
    subs: Vec<Subscription>,
}

#[derive(Default)]
pub struct M {
    nodes: Vec<Node>,
    loaded: bool,
    collapsed: HashSet<String>,
    hidden: HashSet<String>,
    search: String,
    saved_collapsed: Option<HashSet<String>>,
    zoom_init: bool,
    heights: HashMap<String, f32>,
    /// Bumped each time the map is laid out (the views' caches key on it).
    pub(crate) lay_gen: u64,
    /// How many times a drawn idea's height differed from its estimate and
    /// laid the map out again (for the driver).
    relayouts: u64,
    /// What each idea's height estimate was made from (its words, its chips).
    est_sig: HashMap<String, u64>,
    lay: Layout,
    draft: Option<Draft>,
    edit: Option<String>,
    // Bumped when a box opens (a late "lost the keys" from the one before is old news).
    draft_gen: u64,
    edit_gen: u64,
    sel: String,
    anchor: String,
    del_armed: String,
    del_gen: u64,
    // A drag: where it would land, the place shown open, and the one wanted
    // (held a moment before the map makes room for it).
    drop: Option<DropAt>,
    target: Option<Target>,
    want: String,
    want_gen: u64,
    last_drag: Option<Instant>,
    click_gen: u64,
    press: Option<Press>,
    // Ideas typed and on their way to the server, saved one after another.
    saving: usize,
    reload_wanted: bool,
    tmp_seq: u64,
    real_id: HashMap<String, String>,
    queue: VecDeque<Save>,
    busy: bool,
    load_gen: u64,
    relayout: bool,
    flash: String,
    flash_gen: u64,
    // What the screen shows (the Slint global's properties).
    state: String,
    error: String,
    bar: bool,
    chips: Vec<Chip>,
    hidden_count: usize,
    rows: Vec<Row>,
    edges: String,
    base_w: f32,
    base_h: f32,
    zoom: f32,
    pan: Option<Entity<Pan>>,
    view_w: f32,
    view_h: f32,
    /// The canvas's top left in the window.
    canvas_x: f32,
    canvas_y: f32,
    draft_on: bool,
    draft_x: f32,
    draft_y: f32,
    edit_on: bool,
    edit_node: Option<Row>,
    ring_on: bool,
    ring_x: f32,
    ring_y: f32,
    ring_h: f32,
    ring_warn: bool,
    ring_msg: String,
    hover: Option<Row>,
    drag_id: String,
    drag_x: f32,
    drag_y: f32,
    grab_x: f32,
    grab_y: f32,
    drag_text: String,
    // The idea dialog.
    detail: Option<String>,
    d_open: bool,
    d_editing: bool,
    d_title: String,
    d_link_kind: String,
    d_link_main: String,
    d_link_text: String,
    d_link_st: String,
    d_link_todo: i32,
    d_link_label: String,
    d_view_notes: Vec<String>,
    d_notes: Vec<String>,
    d_media: Vec<Value>,
    d_thumbs: Vec<Thumb>,
    pics: HashMap<String, Option<Arc<Image>>>,
    // The link picker.
    link_id: String,
    l_open: bool,
    l_keys: Vec<String>,
    l_names: Vec<SharedString>,
    l_sel: usize,
    l_locked: bool,
    l_locked_text: String,
    l_scope_label: String,
    l_loading: bool,
    l_rows: Vec<LinkRow>,
    l_board: Value,
    l_gen: u64,
    // The shortcut sheet: shown, which rows are open, all of them.
    h_shown: bool,
    h_open: Vec<bool>,
    h_all: bool,
    ui: Option<Ui>,
    /// The driver's "mindmap-all-div": every idea drawn as its div (to
    /// check that an idea at rest looks the same as its div).
    pub(crate) all_div: bool,
    /// The ideas the map drew in its last frame (for the driver).
    pub(crate) drawn: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}

impl M {
    /// The pan's entity (made with the store).
    pub(crate) fn pan(&self) -> Entity<Pan> { self.pan.clone().expect("the map's pan") }
}

/// The store's handle (handlers only; a view holds the handle).
struct Handle(Entity<M>);
impl Global for Handle {}

pub(crate) fn store(cx: &App) -> Entity<M> { cx.global::<Handle>().0.clone() }

fn with<R>(cx: &mut App, f: impl FnOnce(&mut M) -> R) -> R { store(cx).update(cx, |s, _| f(s)) }

/// The map's pan (its scroll, in canvas pixels): its own entity, so a
/// scroll redraws the map alone.
#[derive(Default)]
pub struct Pan { pub x: f32, pub y: f32 }

// ─── Small helpers ───

fn s_of(v: &Value, k: &str) -> String { crate::util::s(v, k) }

fn toast_error(window: &mut Window, cx: &mut App, text: &str) { w::toast(window, cx, text.to_string(), w::Toast::Error); }

/// The layout's sizes, from the theme.
/// An idea's height before it is drawn (unscaled), from its words as the
/// card wraps them (two lines at most) and its chips: the map lays out every
/// idea, and draws (and so measures) only those on screen.
/// `chips`: 0 none, 1 a row of chips, 2 one with an inherited chip (its
/// dashed border is 1 px more each side).
fn estimate(text: &str, chips: u8, cx: &App) -> f32 {
    let font = Font { family: T::UI_FONT.into(), weight: T::W_SEMIBOLD, ..crundi_theme::ui_font() };
    let mut wrapper = cx.text_system().line_wrapper(font, T::TEXT_13);
    let wrap = T::MINDMAP_NODE - (T::SP_10 + px(1.)) * 2. - px(2.);
    let mut lines = 0usize;
    for para in text.split('\n') {
        lines += 1 + wrapper.wrap_line(&[LineFragment::text(para)], wrap, IndentAdjustment::NoIndent).count();
        if lines >= 2 { break; }
    }
    // As drawn and measured: the card's padding box (its border is outside
    // it), each line of words at its line height rounded to whole pixels.
    let lh = (f32::from(T::TEXT_13) * T::LH_UI).round();
    // A chip's row: its words' line (the text's φ, snapped), its 1 px
    // padding (an inherited one's dashed border is 1 px more each side).
    let chip = if chips == 2 { 2. } else { 0. } + if chips > 0 { f32::from(T::SP_6) + (f32::from(T::TEXT_11) * crate::paint::PHI).round().max(f32::from(T::ICON_SM)) + 2. } else { 0. };
    f32::from(T::SP_8) * 2. + lines.clamp(1, 2) as f32 * lh + chip
}

fn dims() -> tree::Dims {
    tree::Dims { col: f32::from(T::MINDMAP_COL), node: f32::from(T::MINDMAP_NODE), pad: f32::from(T::SP_24), gap: f32::from(T::SP_16), min_h: f32::from(T::SP_48) }
}
use crundi_theme::T;

fn draft_text(cx: &App) -> String { cx.try_global::<Handle>().map(|h| h.0.read(cx)).and_then(|s| s.ui.as_ref()).map(|u| u.draft.read(cx).value().to_string()).unwrap_or_default() }
fn edit_text(cx: &App) -> String { cx.try_global::<Handle>().map(|h| h.0.read(cx)).and_then(|s| s.ui.as_ref()).map(|u| u.edit.read(cx).value().to_string()).unwrap_or_default() }

fn set_area(e: Option<Entity<w::TextareaState>>, v: &str, window: &mut Window, cx: &mut App) {
    if let Some(e) = e { let v = v.to_string(); e.update(cx, |st, cx| st.set_value(v, window, cx)); }
}
fn set_line(e: Option<Entity<w::InputState>>, v: &str, window: &mut Window, cx: &mut App) {
    if let Some(e) = e { let v = v.to_string(); e.update(cx, |st, cx| st.set_value(v, window, cx)); }
}

/// The keys go to the canvas (a closed text box takes them with it).
fn focus_canvas(window: &mut Window, cx: &mut App) {
    if let Some(f) = with(cx, |s| s.ui.as_ref().map(|u| u.canvas.clone())) { f.focus(window, cx); }
}

fn focus_draft(window: &mut Window, cx: &mut App) {
    if let Some(e) = with(cx, |s| s.ui.as_ref().map(|u| u.draft.clone())) { e.update(cx, |st, cx| st.focus(window, cx)); }
}

/// The edit box has the keys, the cursor after its words.
fn focus_edit_end(window: &mut Window, cx: &mut App) {
    let Some(e) = with(cx, |s| s.ui.as_ref().map(|u| u.edit.clone())) else { return };
    e.update(cx, |st, cx| {
        let v = st.value().to_string();
        let line = v.split('\n').count().saturating_sub(1) as u32;
        let col = v.rsplit('\n').next().unwrap_or("").encode_utf16().count() as u32;
        st.set_cursor_position(gpui_component::input::Position { line, character: col }, window, cx);
    });
}

/// The text boxes and focus places, once (the view has the window).
fn ensure(window: &mut Window, cx: &mut App) {
    if with(cx, |s| s.ui.is_some()) { return; }
    let draft = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(1, 7).placeholder("New idea"));
    let edit = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(1, 7));
    let search = cx.new(|cx| w::InputState::new(window, cx).placeholder("Filter ideas…"));
    let d_text = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(2, 6));
    let l_search = cx.new(|cx| w::InputState::new(window, cx).placeholder("Search tasks & subtasks…"));
    let subs = vec![
        cx.subscribe(&search, |st, e: &w::InputEvent, cx| if matches!(e, w::InputEvent::Change) {
            let q = st.read(cx).value().to_string();
            in_window(cx, move |window, cx| set_search(&q, window, cx));
        }),
        // Clicked elsewhere (the filter box, a dialog): the box closes; an edit saves.
        cx.subscribe(&draft, |_, e: &w::InputEvent, cx| if matches!(e, w::InputEvent::Blur) {
            let g_ = with(cx, |s| s.draft_gen);
            after(cx, 0, move |window, cx| if with(cx, |s| s.draft.is_some() && s.draft_gen == g_) && !draft_focused(window, cx) { end_draft(window, cx); });
        }),
        cx.subscribe(&edit, |_, e: &w::InputEvent, cx| if matches!(e, w::InputEvent::Blur) {
            let g_ = with(cx, |s| s.edit_gen);
            after(cx, 0, move |window, cx| if with(cx, |s| s.edit.is_some() && s.edit_gen == g_) && !edit_focused(window, cx) { finish_edit(true, window, cx); });
        }),
        cx.subscribe(&l_search, |_, e: &w::InputEvent, cx| if matches!(e, w::InputEvent::Change) { link_list(cx); redraw(cx); }),
    ];
    let ui = Ui {
        draft, edit, search, d_text, d_notes: vec![], l_search,
        canvas: cx.focus_handle(), d_focus: cx.focus_handle(), l_focus: cx.focus_handle(), h_focus: cx.focus_handle(),
        subs,
    };
    with(cx, |s| s.ui = Some(ui));
}

fn draft_focused(window: &Window, cx: &App) -> bool { cx.try_global::<Handle>().map(|h| h.0.read(cx)).and_then(|s| s.ui.as_ref()).is_some_and(|u| u.draft.focus_handle(cx).is_focused(window)) }
fn edit_focused(window: &Window, cx: &App) -> bool { cx.try_global::<Handle>().map(|h| h.0.read(cx)).and_then(|s| s.ui.as_ref()).is_some_and(|u| u.edit.focus_handle(cx).is_focused(window)) }
fn canvas_focused(window: &Window, cx: &App) -> bool { cx.try_global::<Handle>().map(|h| h.0.read(cx)).and_then(|s| s.ui.as_ref()).is_some_and(|u| u.canvas.is_focused(window)) }

// ─── The part ───

// Tab and Shift+Tab on the map (the ring on an idea: a new idea under it, or
// beside it). The window's root binds Tab to "focus next", and bindings win
// over key-down handlers, so the map binds them in its own, deeper context.
actions!(mindmap, [MapTab, MapBackTab]);
pub(crate) const MAP_CONTEXT: &str = "Mindmap";

/// At start: the store, the map's keys, the workbench pane kind (panel:mindmap).
pub fn init(cx: &mut App) {
    let m = cx.new(|cx| M { state: "loading".into(), zoom: 1.0, pan: Some(cx.new(|_| Pan::default())), ..Default::default() });
    cx.set_global(Handle(m));
    cx.bind_keys([KeyBinding::new("tab", MapTab, Some(MAP_CONTEXT)), KeyBinding::new("shift-tab", MapBackTab, Some(MAP_CONTEXT))]);
    crundi_workbench::register_kind(cx, crundi_workbench::PaneKind {
        prefix: "panel:mindmap",
        chat: false,
        make: std::rc::Rc::new(|_, _, window, cx| {
            let host = host(window, cx);
            crundi_workbench::PaneParts {
                body: std::rc::Rc::new(move |cx| host.body(cx)),
                info: std::rc::Rc::new(|_| crundi_workbench::HeadInfo { title: "Mindmap".into(), long: String::new(), state: "idle".into() }),
                watch: Box::new(|_| vec![]),
            }
        }),
        close: None,
        zoom: None,
    });
}

/// A place the map shows (the tab, a pane): its views; loads the map the
/// first time it shows (a pane restored with the layout).
fn host(window: &mut Window, cx: &mut App) -> draw::Host {
    ensure(window, cx);
    let signed_in = crundi_shell::stores(cx).session.read(cx).signed_in();
    if with(cx, |s| !s.loaded && s.load_gen == 0 && s.state == "loading") && signed_in { load(cx); }
    draw::Host::new(window, cx)
}

/// The Mindmap tab's view (layout: its body places the cached parts).
pub fn tab(_: &Stores, window: &mut Window, cx: &mut App) -> AnyView {
    let h = host(window, cx);
    load(cx);
    cx.new(|_| draw::Tab(h)).into()
}

/// Whether the map is on show: its tab, or a pane on the workbench.
fn visible(cx: &mut App) -> bool {
    let tab = crate::util::current_tab(cx);
    tab == "mindmap" || (tab == "workbench" && crate::util::pane_shown(cx, "panel:mindmap"))
}

pub fn on_batch(_: &Stores, batch: &[Event], cx: &mut App) {
    let again = batch.iter().any(|e| matches!(e, Event::Server(n, _) if n == "mindmap") || matches!(e, Event::Connected(true)));
    if again && visible(cx) { load(cx); }
}

pub fn on_project(_: &Stores, _alias: &str, cx: &mut App) {
    if with(cx, |s| s.loaded) { in_window(cx, render); }
}

pub fn on_signed_in(_: &Stores, cx: &mut App) { if visible(cx) { load(cx); } }

pub fn on_signed_out(_: &Stores, cx: &mut App) {
    with(cx, |s| {
        s.nodes.clear(); s.loaded = false; s.draft = None; s.edit = None; s.sel.clear(); s.anchor.clear(); s.drop = None; s.target = None;
        s.queue.clear(); s.saving = 0; s.busy = false; s.detail = None; s.heights.clear(); s.zoom_init = false; s.pics.clear();
        s.state = "loading".into(); s.d_open = false; s.l_open = false; s.h_shown = false; s.load_gen = 0;
        s.draft_on = false; s.edit_on = false; s.hover = None; s.ring_on = false; s.drag_id.clear(); s.rows.clear(); s.press = None;
    });
    redraw(cx);
}

// ─── Loading ───

/// The map again, from the server. Ideas typed a moment ago and still on
/// their way are not in what it would send back: wait for them.
fn load(cx: &mut App) { load_from("/api/mindmap", cx); }

/// The map again (its tab or pane came into view).
pub fn reload(cx: &mut App) { if with(cx, |s| s.load_gen > 0 || s.loaded) { load(cx); } }

fn load_from(path: &str, cx: &mut App) {
    if with(cx, |s| { if s.saving > 0 { s.reload_wanted = true; true } else { false } }) { return; }
    let g_ = with(cx, |s| { s.load_gen += 1; s.load_gen });
    let path = path.to_string();
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
        if with(cx, |s| s.load_gen != g_) { return; }
        if with(cx, |s| { if s.saving > 0 { s.reload_wanted = true; true } else { false } }) { return; }
        match r {
            Err(e) => {
                with(cx, |s| { s.loaded = false; s.error = format!("Failed to load mindmap: {e}"); s.state = "error".into(); });
                redraw(cx);
            }
            Ok(v) => {
                let nodes = if truthy(&v["ok"]) { tree::parse(&v) } else { vec![] };
                with(cx, |s| {
                    s.nodes = nodes;
                    s.loaded = true;
                    if let Some(e) = s.edit.clone() { if !s.nodes.iter().any(|n| n.id == e) { s.edit = None; } }
                    if s.edit.is_none() { s.edit_on = false; }
                });
                in_window(cx, |window, cx| { render(window, cx); refresh_detail(cx); });
            }
        }
    });
}

/// POST /api/mindmap; a refusal says so in a toast (mindmapPost).
fn post(payload: Value, cx: &mut App, then: impl FnOnce(Value, &mut Window, &mut App) + 'static) {
    crundi_shell::call(cx, move |api| api.post("/api/mindmap", &payload), move |r, cx| {
        in_window(cx, move |window, cx| {
            let v = match r {
                Ok(v) => { if !truthy(&v["ok"]) { let e = s_of(&v, "error"); toast_error(window, cx, if e.is_empty() { "Action failed" } else { &e }); } v }
                Err(e) => { toast_error(window, cx, &format!("Action failed: {e}")); json!({ "ok": false }) }
            };
            then(v, window, cx);
        });
    });
}

// ─── Drawing ───

fn project_names(cx: &App) -> HashMap<String, String> {
    crundi_shell::stores(cx).projects.read(cx).list.iter().map(|p| (p.alias.clone(), if p.name.is_empty() { p.alias.clone() } else { p.name.clone() })).collect()
}

fn name_of(names: &HashMap<String, String>, alias: &str) -> String { names.get(alias).cloned().unwrap_or_else(|| alias.to_string()) }

/// An idea's chip (the web's nodeHtml): its link, its project, or the
/// project it is in through its parent.
fn chip(n: &Node, names: &HashMap<String, String>) -> (String, String, String, i32) {
    let i = &n.info;
    if n.linked {
        if !i.is_null() && !truthy(&i["missing"]) {
            if s_of(i, "kind") == "todo" {
                let t = [s_of(i, "text"), s_of(i, "taskTitle"), "subtask".into()].into_iter().find(|x| !x.is_empty()).unwrap_or_default();
                return ("link".into(), tree::trunc(&t, 20), String::new(), if truthy(&i["done"]) { 2 } else { 1 });
            }
            let t = [s_of(i, "title"), s_of(i, "project"), "task".into()].into_iter().find(|x| !x.is_empty()).unwrap_or_default();
            return ("link".into(), tree::trunc(&t, 20), String::new(), 0);
        }
        return ("missing".into(), "removed".into(), String::new(), 0);
    }
    if !n.project.is_empty() { return ("scope".into(), tree::trunc(&name_of(names, &n.project), 18), String::new(), 0); }
    if !n.eff.is_empty() {
        let nm = name_of(names, &n.eff);
        return ("inherited".into(), format!("\u{21b3} {}", tree::trunc(&nm, 16)), format!("In {nm} (inherited from parent)"), 0);
    }
    (String::new(), String::new(), String::new(), 0)
}

fn row_of(p: &tree::Placed, s: &M, q: &str, names: &HashMap<String, String>) -> Row {
    let n = s.nodes.iter().find(|n| n.id == p.id);
    let kind = match p.kind { Kind::Node => "node", Kind::Ghost => "ghost", Kind::Draft => "draft", Kind::Drop => "drop" };
    let mut r = Row {
        id: p.id.clone(), kind, parent: p.parent.clone().unwrap_or_default(),
        x: p.x, y: p.y, h: p.h, root: p.root, kids: p.kids, collapsed: s.collapsed.contains(&p.id),
        hidden: p.kind == Kind::Ghost && !q.is_empty(),
        ..Default::default()
    };
    if let Some(n) = n {
        let (c, ct, tip, todo) = chip(n, names);
        r.text = n.text.clone();
        r.linked = n.linked;
        r.scoped = !n.linked && !n.project.is_empty();
        r.pending = n.pending;
        r.chip = c; r.chip_text = ct; r.chip_tip = tip; r.todo = todo;
        r.notes = n.notes.len();
        if !q.is_empty() { let hit = tree::matches(n, q); r.hit = hit; r.dim = !hit; }
        r.lifted = s.drag_id == p.id;
        r.target = matches!(&s.target, Some(Target::Child(t)) if *t == p.id);
        r.flash = s.flash == p.id;
    }
    r
}

/// Lay the map out again.
fn render(window: &mut Window, cx: &mut App) {
    let names = project_names(cx);
    // Every idea has a height before it is drawn (an estimate from its
    // words; the drawn ones correct it as they paint: `measured`).
    let todo: Vec<(String, String, u8, u64)> = with(cx, |s| s.nodes.iter().filter_map(|n| {
        let inherited = !n.linked && n.project.is_empty() && !n.eff.is_empty();
        let chips = if inherited { 2 } else if n.linked || !n.project.is_empty() || !n.notes.is_empty() { 1 } else { 0 };
        let sig = { use std::hash::{Hash, Hasher}; let mut h = std::collections::hash_map::DefaultHasher::new(); n.text.hash(&mut h); chips.hash(&mut h); h.finish() };
        (s.est_sig.get(&n.id) != Some(&sig)).then(|| (n.id.clone(), n.text.clone(), chips, sig))
    }).collect());
    if !todo.is_empty() {
        let est: Vec<(String, f32, u64)> = todo.iter().map(|(id, t, c, sig)| (id.clone(), estimate(t, *c, cx), *sig)).collect();
        // Each idea's "add idea" box too (its padding box: sp-40 less its border).
        let ghost = f32::from(T::SP_40) - 2.;
        with(cx, |s| for (id, h, sig) in est {
            s.heights.entry(format!("{}{id}", tree::ADD)).or_insert(ghost);
            s.heights.insert(id.clone(), h);
            s.est_sig.insert(id, sig);
        });
    }
    let d = dims();
    let (closed, open_now) = with(cx, |s| {
        let mut collapsed = std::mem::take(&mut s.collapsed);
        let mut lay = tree::layout(&s.nodes, &mut collapsed, &s.hidden, s.draft.as_ref(), s.drop.as_ref(), &s.heights, &d);
        if lay.draft_lost {
            s.draft = None;
            lay = tree::layout(&s.nodes, &mut collapsed, &s.hidden, None, s.drop.as_ref(), &s.heights, &d);
        }
        s.collapsed = collapsed;
        let q = s.search.trim().to_lowercase();
        s.rows = lay.items.iter().map(|p| row_of(p, s, &q, &names)).collect();
        let keys = tree::project_keys(&s.nodes);
        s.chips = keys.iter().map(|k| Chip { key: k.clone(), label: if k.is_empty() { "General".into() } else { name_of(&names, k) }, off: s.hidden.contains(k) }).collect();
        s.edges = lay.edges.clone();
        s.lay_gen += 1;
        s.base_w = lay.w;
        s.base_h = lay.h;
        s.lay = lay;
        let empty = s.nodes.is_empty() && s.draft.is_none();
        if s.loaded { s.state = if empty { "empty" } else { "ok" }.into(); }
        s.bar = !s.chips.is_empty();
        s.hidden_count = s.chips.iter().filter(|c| c.off).count();
        // The first time it shows: fit it to the canvas's height.
        if !s.zoom_init && s.view_h > 0.0 && !empty && !s.heights.is_empty() {
            let avail = s.view_h - d.gap;
            s.zoom = if s.lay.h > avail { (avail / s.lay.h).max(0.25) } else { 1.0 };
            s.zoom_init = true;
        }
        // The box being typed in, where its idea will sit.
        let (mut closed, mut open_now) = (false, false);
        match s.lay.get(tree::DRAFT).cloned() {
            Some(p) => { s.draft_x = p.x; s.draft_y = p.y; if !s.draft_on { s.draft_on = true; open_now = true; } }
            None => { if s.draft_on { s.draft_on = false; closed = true; } }
        }
        place_ring(s);
        // The ideas opened up follow their place.
        let find = |id: &str, rows: &[Row]| rows.iter().find(|r| r.id == id && r.kind == "node").cloned();
        if let Some(h) = s.hover.clone() { s.hover = find(&h.id, &s.rows); }
        if let Some(e) = s.edit.clone() { match find(&e, &s.rows) { Some(r) => s.edit_node = Some(r), None => { s.edit = None; s.edit_on = false; s.edit_node = None; } } }
        (closed, open_now)
    });
    if open_now { focus_draft(window, cx); }
    if closed && with(cx, |s| s.edit.is_none()) { focus_canvas(window, cx); }
    redraw(cx);
}

/// The ring on the idea the keyboard is on.
fn place_ring(s: &mut M) {
    let p = if s.sel.is_empty() { None } else { s.lay.get(&s.sel).filter(|p| p.kind == Kind::Node).cloned() };
    match p {
        Some(p) => { s.ring_x = p.x; s.ring_y = p.y; s.ring_h = p.h; s.ring_on = true; }
        None => { s.ring_on = false; s.ring_warn = false; }
    }
}

/// An idea's height as drawn (the draw's report, unscaled): a change lays
/// the map out again (once, after this frame's reports).
fn measured(id: &str, h: f32, cx: &mut App) {
    let changed = with(cx, |s| {
        let z = s.zoom.max(0.1);
        let old = s.heights.get(id).copied().unwrap_or(-1.0);
        // (A box's height moves by a pixel as pixel snapping places it: not a change.)
        if (old - h).abs() <= 1.0 / z + 0.25 { return false; }
        s.heights.insert(id.to_string(), h);
        if s.relayout { return false; }
        s.relayout = true;
        s.relayouts += 1;
        log::debug!("mindmap: {id} measured {h} (estimate or last {old})");
        true
    });
    if changed { cx.defer(|cx| in_window(cx, |window, cx| { with(cx, |s| s.relayout = false); render(window, cx); })); }
}

/// The canvas's place and size as drawn (window coordinates).
fn canvas_measured(b: Bounds<Pixels>, cx: &mut App) {
    let (x, y, vw, vh) = (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height));
    let first = with(cx, |s| {
        let changed = (s.view_w - vw).abs() > 0.5 || (s.view_h - vh).abs() > 0.5;
        s.canvas_x = x; s.canvas_y = y; s.view_w = vw; s.view_h = vh;
        changed && !s.zoom_init
    });
    if first { cx.defer(|cx| in_window(cx, render)); }
}

// ─── The toolbar, the chips, the filter ───

fn act(a: &str, id: &str, window: &mut Window, cx: &mut App) {
    match a {
        "add-root" => start_draft(None, None, window, cx),
        "media" => media_modal(window, cx),
        "help" => open_help(window, cx),
        "toggle" => { with(cx, |s| if !s.collapsed.remove(id) { s.collapsed.insert(id.to_string()); }); render(window, cx); }
        "ghost" => start_draft(Some(id.to_string()), None, window, cx),
        "chip" => { with(cx, |s| if !s.hidden.remove(id) { s.hidden.insert(id.to_string()); }); render(window, cx); }
        "chip-all" => { with(cx, |s| s.hidden.clear()); render(window, cx); }
        _ => {}
    }
}

/// The filter: starting one opens every folded idea (so matches show);
/// clearing it folds them back as they were.
fn set_search(q: &str, window: &mut Window, cx: &mut App) {
    let shown = with(cx, |s| {
        let had = !s.search.trim().is_empty();
        let has = !q.trim().is_empty();
        s.search = q.to_string();
        if has && !had { s.saved_collapsed = Some(std::mem::take(&mut s.collapsed)); }
        else if !has && had { if let Some(c) = s.saved_collapsed.take() { s.collapsed.extend(c); } }
        s.ui.as_ref().map(|u| u.search.clone())
    });
    if let Some(e) = shown { if e.read(cx).value().as_ref() != q { set_line(Some(e), q, window, cx); } }
    render(window, cx);
}

// ─── The ring and the keyboard ───

fn select(id: &str, follow: bool, window: &mut Window, cx: &mut App) {
    let canvas = with(cx, |s| {
        if !s.del_armed.is_empty() && s.del_armed != id { disarm(s); }
        s.sel = id.to_string();
        if !id.is_empty() { s.anchor = id.to_string(); }
        if s.del_armed.is_empty() { s.ring_warn = false; }
        place_ring(s);
        s.draft.is_none() && s.edit.is_none()
    });
    if canvas { focus_canvas(window, cx); }
    if follow { scroll_to(id, cx); }
    redraw(cx);
}

fn scroll_now(cx: &App) -> (f32, f32) {
    let p = store(cx).read(cx).pan().read(cx);
    (-p.x, -p.y)
}

/// Bring an idea comfortably into view, and only as far as needed.
fn scroll_to(id: &str, cx: &mut App) {
    let Some(p) = with(cx, |s| s.lay.get(id).cloned()) else { return };
    let (z, vw, vh) = with(cx, |s| (s.zoom, s.view_w, s.view_h));
    let (m, nw) = (f32::from(T::MINDMAP_SCROLL_MARGIN), dims().node);
    if vw <= 0.0 || vh <= 0.0 { return; }
    let (x0, x1) = (p.x * z, p.x * z + nw * z);
    let (yc, hh) = (p.y * z, p.h * z / 2.0);
    let (sx, sy) = scroll_now(cx);
    let (mut sl, mut st) = (-sx, -sy);
    if x0 - m < sl { sl = x0 - m; } else if x1 + m > sl + vw { sl = x1 + m - vw; }
    if yc - hh - m < st { st = yc - hh - m; } else if yc + hh + m > st + vh { st = yc + hh + m - vh; }
    set_scroll(sl, st, cx);
}

fn set_scroll(sl: f32, st: f32, cx: &mut App) {
    let (x, y, pan) = with(cx, |s| {
        let z = s.zoom;
        let cw = s.view_w.max(s.base_w * z);
        let ch = s.view_h.max(s.base_h * z);
        (sl.clamp(0.0, (cw - s.view_w).max(0.0)), st.clamp(0.0, (ch - s.view_h).max(0.0)), s.pan())
    });
    pan.update(cx, |p, cx| if p.x != x || p.y != y { p.x = x; p.y = y; cx.notify(); });
}

/// A wheel (or a touchpad's swipe) over the map, as GPUI gives its delta
/// (up is +): it scrolls both ways, as the web's overflow: auto canvas.
pub(crate) fn scroll_by(dx: f32, dy: f32, cx: &mut App) {
    let (sx, sy) = scroll_now(cx);
    set_scroll(-sx - dx, -sy - dy, cx);
}

fn clear_sel(cx: &mut App) {
    with(cx, |s| { if !s.sel.is_empty() { s.anchor = std::mem::take(&mut s.sel); } place_ring(s); });
}

fn disarm(s: &mut M) { s.del_armed.clear(); s.del_gen += 1; }

fn disarm_now(cx: &mut App) { with(cx, |s| { disarm(s); s.ring_warn = false; s.ring_msg.clear(); }); }

/// Delete with the ring on an idea: the first press warns, a second while
/// it is amber deletes it and everything under it.
fn delete_key(window: &mut Window, cx: &mut App) {
    let id = with(cx, |s| s.sel.clone());
    let Some(n) = with(cx, |s| s.nodes.iter().find(|x| x.id == id).cloned()) else { return };
    if n.pending || with(cx, |s| s.lay.get(&id).is_none()) { return; }
    let gone = with(cx, |s| tree::subtree(&s.nodes, &id));
    if with(cx, |s| s.del_armed != id) {
        disarm_now(cx);
        let under = gone.len() - 1;
        let msg = format!("Press Delete again to delete{}", if under > 0 { format!(" it and the {under}{} under it", if under == 1 { " idea" } else { " ideas" }) } else { String::new() });
        let g_ = with(cx, |s| { s.del_armed = id.clone(); s.del_gen += 1; s.ring_msg = msg; s.ring_warn = true; s.del_gen });
        after(cx, 3000, move |_, cx| { if with(cx, |s| s.del_gen == g_) { disarm_now(cx); redraw(cx); } });
        redraw(cx);
        return;
    }
    disarm_now(cx);
    // Where the ring goes next: below on this level, else above, else the parent.
    let next = with(cx, |s| {
        let st = |d: &str| tree::step(&s.nodes, &s.hidden, &s.collapsed, &s.lay, &id, d).0;
        let n2 = st("down");
        if !n2.is_empty() { n2 } else { let u = st("up"); if !u.is_empty() { u } else { n.parent.clone().unwrap_or_default() } }
    });
    with(cx, |s| { s.nodes.retain(|x| !gone.contains(&x.id)); s.sel = if gone.contains(&next) { String::new() } else { next.clone() }; });
    render(window, cx);
    let sel = with(cx, |s| s.sel.clone());
    select(&sel, true, window, cx);
    post(json!({ "action": "deleteNode", "id": id }), cx, |_, _, cx| load(cx));
}

/// A key as the map reads it: GPUI's name (and the driver's raw characters).
pub(crate) struct Key<'a> { pub key: &'a str, pub ctrl: bool, pub shift: bool, pub alt: bool }

impl Key<'_> {
    fn dir(&self) -> Option<&'static str> { match self.key { "up" => Some("up"), "down" => Some("down"), "left" => Some("left"), "right" => Some("right"), _ => None } }
    fn ret(&self) -> bool { self.key == "enter" }
    fn tab(&self) -> bool { self.key == "tab" && !self.shift }
    fn backtab(&self) -> bool { self.key == "tab" && self.shift }
    fn esc(&self) -> bool { self.key == "escape" }
    fn del(&self) -> bool { self.key == "delete" || self.key == "\u{7f}" }
    fn plus(&self) -> bool { self.key == "+" || self.key == "=" }
    fn minus(&self) -> bool { self.key == "-" || self.key == "_" || self.key == "\u{2212}" }
}

/// Ctrl+Shift and an arrow: walk the tree. An idea being edited is saved
/// and the editing moves along; a new idea being typed is kept if it has
/// words, then the walk goes on from it.
fn mm_step(dir: &str, window: &mut Window, cx: &mut App) {
    let mut from = with(cx, |s| if !s.sel.is_empty() { s.sel.clone() } else if s.lay.get(&s.anchor).is_some() { s.anchor.clone() } else { String::new() });
    let mut was_editing = false;
    if let Some(e) = with(cx, |s| s.edit.clone()) { from = e; was_editing = true; finish_edit(true, window, cx); }
    else if let Some(dr) = with(cx, |s| s.draft.clone()) {
        let text = draft_text(cx);
        let origin = dr.after.clone().or(dr.parent.clone()).unwrap_or_default();
        if !text.trim().is_empty() { draft_commit("next", window, cx); from = with(cx, |s| s.draft.as_ref().and_then(|d| d.after.clone())).unwrap_or(origin); } else { from = origin; }
        end_draft(window, cx);
        was_editing = true;
    }
    let (to, unfold) = with(cx, |s| tree::step(&s.nodes, &s.hidden, &s.collapsed, &s.lay, &from, dir));
    if unfold { with(cx, |s| s.collapsed.remove(&from)); render(window, cx); }
    let at = if to.is_empty() { from } else { to };
    select(&at, true, window, cx);
    if was_editing && !at.is_empty() { start_edit(&at, window, cx); }
}

/// Ctrl and + or −: zoom about the middle of the canvas.
fn zoom_step(plus: bool, window: &mut Window, cx: &mut App) {
    let (z, vw, vh) = with(cx, |s| (s.zoom, s.view_w, s.view_h));
    set_zoom(z * if plus { 1.2 } else { 1.0 / 1.2 }, vw / 2.0, vh / 2.0, window, cx);
}

/// The map's keys with no text box open.
fn map_key(k: &Key, window: &mut Window, cx: &mut App) -> bool {
    if k.ctrl && k.shift && !k.alt { if let Some(d) = k.dir() { mm_step(d, window, cx); return true; } }
    let (edit, draft, sel) = with(cx, |s| (s.edit.is_some(), s.draft.is_some(), s.sel.clone()));
    if k.del() && !k.alt && !k.ctrl && !edit && !draft && !sel.is_empty() { delete_key(window, cx); return true; }
    if with(cx, |s| !s.del_armed.is_empty()) { disarm_now(cx); redraw(cx); }
    if k.esc() && !edit && !draft && !sel.is_empty() { clear_sel(cx); redraw(cx); return true; }
    if k.ctrl && !k.alt && (k.plus() || k.minus()) { zoom_step(k.plus(), window, cx); return true; }
    if edit || draft || sel.is_empty() || k.alt { return false; }
    let Some(n) = with(cx, |s| s.nodes.iter().find(|x| x.id == sel).cloned()) else { return false };
    if n.pending || with(cx, |s| s.lay.get(&sel).is_none()) { return false; }
    if k.ret() && k.ctrl { start_draft(n.parent.clone(), Some(n.id.clone()), window, cx); return true; }
    if k.ctrl { return false; }
    if k.ret() && !k.shift { start_edit(&n.id, window, cx); return true; }
    if k.tab() || k.backtab() { tab_from(&n, k.backtab(), window, cx); return true; }
    if k.plus() || k.minus() {
        let has_kids = with(cx, |s| s.nodes.iter().any(|x| x.parent.as_deref() == Some(n.id.as_str())));
        let minus = k.minus();
        if !has_kids || with(cx, |s| s.collapsed.contains(&n.id)) == minus { return false; }
        with(cx, |s| if minus { s.collapsed.insert(n.id.clone()); } else { s.collapsed.remove(&n.id); });
        render(window, cx);
        return true;
    }
    false
}

/// A key for the map: the box with the keys decides which handler reads it.
pub(crate) fn key(k: &Key, window: &mut Window, cx: &mut App) -> bool {
    if draft_focused(window, cx) && with(cx, |s| s.draft.is_some()) { return draft_key(k, window, cx); }
    if edit_focused(window, cx) && with(cx, |s| s.edit.is_some()) { return edit_key(k, window, cx); }
    if canvas_focused(window, cx) { return map_key(k, window, cx); }
    false
}

/// Tab: a new idea nested under this one; Shift+Tab: one at its parent's
/// level, after the parent (from the top: a new top-level idea).
fn tab_from(n: &Node, back: bool, window: &mut Window, cx: &mut App) {
    let par = with(cx, |s| n.parent.as_ref().and_then(|p| s.nodes.iter().find(|x| &x.id == p).cloned()));
    if !back { start_draft(Some(n.id.clone()), None, window, cx); }
    else if let Some(p) = par { start_draft(p.parent.clone(), Some(p.id.clone()), window, cx); }
    else { start_draft(None, Some(n.id.clone()), window, cx); }
}

// ─── Adding ideas in place ───

fn start_draft(parent: Option<String>, after: Option<String>, window: &mut Window, cx: &mut App) {
    if with(cx, |s| s.edit.is_some()) { finish_edit(true, window, cx); }
    let e = with(cx, |s| {
        if !s.sel.is_empty() { s.anchor = std::mem::take(&mut s.sel); }
        if let Some(a) = after.clone().or(parent.clone()) { s.anchor = a; }
        s.draft = Some(Draft { parent, after });
        s.draft_gen += 1;
        s.hover = None;
        s.ui.as_ref().map(|u| u.draft.clone())
    });
    set_area(e, "", window, cx);
    render(window, cx);
    focus_draft(window, cx);
}

fn end_draft(window: &mut Window, cx: &mut App) {
    if with(cx, |s| s.draft.take()).is_none() { return; }
    render(window, cx);
}

fn draft_key(k: &Key, window: &mut Window, cx: &mut App) -> bool {
    if k.ctrl && k.shift && !k.alt { if let Some(d) = k.dir() { mm_step(d, window, cx); return true; } }
    if k.ret() && k.ctrl { draft_commit("next", window, cx); return true; }
    if k.ret() && !k.shift {
        // Save this one and stop adding, the ring left on it.
        let had = !draft_text(cx).trim().is_empty();
        if had { draft_commit("next", window, cx); }
        let saved = if had { with(cx, |s| s.draft.as_ref().and_then(|d| d.after.clone())) } else { None };
        end_draft(window, cx);
        if let Some(sv) = saved { select(&sv, false, window, cx); }
        return true;
    }
    if k.tab() { draft_commit("nested", window, cx); return true; }
    if k.backtab() { draft_commit("up", window, cx); return true; }
    if k.esc() { end_draft(window, cx); return true; }
    false
}

/// Keep what was typed: shown at once (pending), sent in the order typed
/// (the next idea may hang off one whose id is not known yet); then the box
/// moves on: the next at this level, nested under it, or one level out.
fn draft_commit(mode: &str, window: &mut Window, cx: &mut App) {
    let Some(dr) = with(cx, |s| s.draft.clone()) else { return };
    let text = draft_text(cx).trim().to_string();
    let parent_node = with(cx, |s| dr.parent.as_ref().and_then(|p| s.nodes.iter().find(|n| &n.id == p).cloned()));
    if text.is_empty() {
        // Nothing typed: Shift+Tab still steps out; anything else closes.
        if mode == "up" { if let Some(p) = parent_node { with(cx, |s| s.draft = Some(Draft { parent: p.parent.clone(), after: Some(p.id.clone()) })); render(window, cx); return; } }
        end_draft(window, cx);
        return;
    }
    let e = with(cx, |s| {
        s.tmp_seq += 1;
        let tmp = format!("tmp{}{}", std::process::id(), s.tmp_seq);
        let local = Node { id: tmp.clone(), text: text.clone(), parent: dr.parent.clone(), pending: true, eff: parent_node.as_ref().map(|p| p.eff.clone()).unwrap_or_default(), ..Default::default() };
        let at = dr.after.as_ref().and_then(|a| s.nodes.iter().position(|n| &n.id == a));
        match at { Some(i) => s.nodes.insert(i + 1, local), None => s.nodes.push(local) }
        s.anchor = tmp.clone();
        s.draft = Some(match (mode, &parent_node) {
            ("nested", _) => Draft { parent: Some(tmp.clone()), after: None },
            ("up", Some(p)) => Draft { parent: p.parent.clone(), after: Some(p.id.clone()) },
            _ => Draft { parent: dr.parent.clone(), after: Some(tmp.clone()) },
        });
        s.saving += 1;
        s.queue.push_back(Save { tmp, text, parent: dr.parent.clone() });
        s.ui.as_ref().map(|u| u.draft.clone())
    });
    set_area(e, "", window, cx);
    render(window, cx);
    focus_draft(window, cx);
    pump(cx);
}

/// Send the next idea in the queue, one at a time.
fn pump(cx: &mut App) {
    if with(cx, |s| s.busy) { return; }
    let Some(sv) = with(cx, |s| { let x = s.queue.pop_front(); if x.is_some() { s.busy = true; } x }) else { return };
    let parent = with(cx, |s| sv.parent.as_ref().map(|i| s.real_id.get(i).cloned().unwrap_or_else(|| i.clone())));
    let mut body = json!({ "action": "addNode", "text": sv.text });
    if let Some(p) = &parent { body["parentId"] = json!(p); }
    post(body, cx, move |d, window, cx| {
        let new_id = if truthy(&d["ok"]) { s_of(&d["node"], "id") } else { String::new() };
        if new_id.is_empty() {
            // Not saved (the toast said why): off the map, with anything hung under it.
            with(cx, |s| {
                let gone = tree::subtree(&s.nodes, &sv.tmp);
                s.nodes.retain(|n| !gone.contains(&n.id));
                s.queue.retain(|q| !gone.contains(&q.tmp));
                if let Some(dr) = &s.draft { if dr.parent.as_ref().is_some_and(|p| gone.contains(p)) || dr.after.as_ref().is_some_and(|a| gone.contains(a)) { s.draft = None; } }
            });
            saved_one(window, cx);
            return;
        }
        // A new idea is added at the end of its level: if it belongs earlier, move it there.
        let (at, last) = with(cx, |s| {
            let sibs: Vec<&Node> = s.nodes.iter().filter(|n| n.parent == sv.parent).collect();
            (sibs.iter().position(|n| n.id == sv.tmp), sibs.len())
        });
        let tmp = sv.tmp.clone();
        let nid = new_id.clone();
        let finish = move |window: &mut Window, cx: &mut App| {
            with(cx, |s| {
                s.real_id.insert(tmp.clone(), nid.clone());
                for n in s.nodes.iter_mut() {
                    if n.id == tmp { n.id = nid.clone(); n.pending = false; }
                    if n.parent.as_deref() == Some(tmp.as_str()) { n.parent = Some(nid.clone()); }
                }
                if let Some(dr) = s.draft.as_mut() {
                    if dr.parent.as_deref() == Some(tmp.as_str()) { dr.parent = Some(nid.clone()); }
                    if dr.after.as_deref() == Some(tmp.as_str()) { dr.after = Some(nid.clone()); }
                }
                for q in s.queue.iter_mut() { if q.parent.as_deref() == Some(tmp.as_str()) { q.parent = Some(nid.clone()); } }
                if s.sel == tmp { s.sel = nid.clone(); }
                if s.anchor == tmp { s.anchor = nid.clone(); }
                if let Some(h) = s.heights.remove(&tmp) { s.heights.insert(nid.clone(), h); }
            });
            saved_one(window, cx);
        };
        match at {
            Some(i) if i + 1 < last => {
                let mut mv = json!({ "action": "moveNode", "id": new_id, "index": i });
                mv["parentId"] = parent.map(Value::String).unwrap_or(Value::Null);
                post(mv, cx, move |_, window, cx| finish(window, cx));
            }
            _ => finish(window, cx),
        }
    });
}

fn saved_one(window: &mut Window, cx: &mut App) {
    let done = with(cx, |s| { s.busy = false; s.saving = s.saving.saturating_sub(1); s.saving == 0 });
    render(window, cx);
    if done { with(cx, |s| s.reload_wanted = false); load(cx); } else { pump(cx); }
}

// ─── Editing an idea in place ───

fn start_edit(id: &str, window: &mut Window, cx: &mut App) {
    if with(cx, |s| s.edit.is_some()) { finish_edit(true, window, cx); }
    let Some(n) = with(cx, |s| s.nodes.iter().find(|x| x.id == id).cloned()) else { return };
    if n.pending { return; }
    let e = with(cx, |s| {
        s.draft = None; s.sel = id.to_string(); s.anchor = id.to_string(); s.edit = Some(id.to_string()); s.edit_gen += 1; s.hover = None;
        s.ui.as_ref().map(|u| u.edit.clone())
    });
    set_area(e, &n.text, window, cx);
    render(window, cx);
    with(cx, |s| s.edit_on = true);
    focus_edit_end(window, cx);
    redraw(cx);
}

/// Save (or not) the idea being edited; Escape puts the old words back.
fn finish_edit(save: bool, window: &mut Window, cx: &mut App) {
    let Some(id) = with(cx, |s| s.edit.take()) else { return };
    let text = edit_text(cx).trim().to_string();
    with(cx, |s| { s.edit_on = false; s.edit_node = None; });
    focus_canvas(window, cx);
    let cur = with(cx, |s| s.nodes.iter().find(|n| n.id == id).map(|n| n.text.clone()));
    if !save || cur.is_none() || text.is_empty() || cur.as_deref() == Some(text.as_str()) { render(window, cx); return; }
    with(cx, |s| if let Some(n) = s.nodes.iter_mut().find(|n| n.id == id) { n.text = text.clone(); });
    render(window, cx);
    post(json!({ "action": "updateNode", "id": id, "text": text }), cx, |_, _, cx| load(cx));
}

fn edit_key(k: &Key, window: &mut Window, cx: &mut App) -> bool {
    if k.ctrl && k.shift && !k.alt { if let Some(d) = k.dir() { mm_step(d, window, cx); return true; } }
    let Some(id) = with(cx, |s| s.edit.clone()) else { return false };
    let Some(n) = with(cx, |s| s.nodes.iter().find(|x| x.id == id).cloned()) else { return false };
    if k.ret() && k.ctrl { finish_edit(true, window, cx); start_draft(n.parent.clone(), Some(n.id.clone()), window, cx); return true; }
    if k.ret() && !k.shift { finish_edit(true, window, cx); select(&id, false, window, cx); return true; }
    if k.esc() { finish_edit(false, window, cx); return true; }
    if k.tab() || k.backtab() { finish_edit(true, window, cx); tab_from(&n, k.backtab(), window, cx); return true; }
    false
}

// ─── Pointer ───

fn canvas_press(window: &mut Window, cx: &mut App) {
    disarm_now(cx);
    if with(cx, |s| s.draft.is_some()) { end_draft(window, cx); }
    if with(cx, |s| s.edit.is_some()) { finish_edit(true, window, cx); }
    clear_sel(cx);
    redraw(cx);
}

/// A press on an idea (window coordinates): a click or the start of a drag.
fn node_down(id: &str, x: f32, y: f32, cx: &mut App) {
    with(cx, |s| s.press = Some(Press { id: id.to_string(), x, y, moved: false }));
}

/// The press let go over an idea: the end of a drag, or a click.
fn node_up(id: &str, clicks: usize, window: &mut Window, cx: &mut App) {
    if with(cx, |s| !s.drag_id.is_empty()) { drag_end(true, window, cx); return; }
    let p = with(cx, |s| s.press.take());
    if p.is_some_and(|p| p.id == id && !p.moved) { node_click(id, clicks, window, cx); }
}

/// One click edits an idea in place, two open its details (the edit waits
/// a moment for a second click).
fn node_click(id: &str, clicks: usize, window: &mut Window, cx: &mut App) {
    if with(cx, |s| s.last_drag.is_some_and(|t| t.elapsed() <= Duration::from_millis(300))) { return; }
    if with(cx, |s| s.edit.as_deref() == Some(id)) { return; }
    disarm_now(cx);
    if with(cx, |s| s.draft.is_some()) { end_draft(window, cx); }
    let g_ = with(cx, |s| { s.click_gen += 1; s.click_gen });
    if clicks >= 2 {
        if with(cx, |s| s.edit.is_some()) { finish_edit(true, window, cx); }
        if with(cx, |s| !s.d_open) { open_detail(id, window, cx); }
        return;
    }
    let id = id.to_string();
    after(cx, 240, move |window, cx| {
        if with(cx, |s| s.click_gen != g_) { return; }
        if with(cx, |s| s.lay.get(&id).is_some()) { start_edit(&id, window, cx); }
    });
}

/// The pointer over an idea: a long one opens up to show all of it.
fn hover(r: &Row, on: bool, clamped: bool, cx: &mut App) {
    let changed = with(cx, |s| {
        if !s.drag_id.is_empty() { return false; }
        let before = s.hover.as_ref().map(|h| h.id.clone());
        if on { if clamped && s.edit.as_deref() != Some(r.id.as_str()) { s.hover = Some(r.clone()); } }
        else if s.hover.as_ref().is_some_and(|h| h.id == r.id) { s.hover = None; }
        s.hover.as_ref().map(|h| h.id.clone()) != before
    });
    // Only the map is drawn again, and only when what is hovered changed:
    // the pointer crosses ideas all the time while the map scrolls, and a
    // refresh of the window drew every pane again each time (the owner, 19 Nov).
    if changed { redraw(cx); }
}

/// What the map shows changed: the views that show it (they observe the
/// store) draw again, nothing else.
pub(crate) fn redraw(cx: &mut App) { store(cx).update(cx, |_, cx| cx.notify()); }

/// The press moving over the canvas (window coordinates): a drag once it
/// has gone sp-6 from where it started.
fn pointer_move(x: f32, y: f32, window: &mut Window, cx: &mut App) {
    let Some(p) = with(cx, |s| s.press.clone()) else { return };
    let (sx, sy) = scroll_now(cx);
    let (z, ox, oy) = with(cx, |s| (s.zoom.max(0.01), s.canvas_x, s.canvas_y));
    let (ux, uy) = ((x - ox - sx) / z, (y - oy - sy) / z);
    if !p.moved {
        let far = f32::from(T::SP_6);
        if (x - p.x).abs() <= far && (y - p.y).abs() <= far { return; }
        let Some(r) = with(cx, |s| s.rows.iter().find(|r| r.id == p.id && r.kind == "node").cloned()) else { return };
        if r.pending || with(cx, |s| s.edit.as_deref() == Some(p.id.as_str())) { return; }
        let (px0, py0) = ((p.x - ox - sx) / z, (p.y - oy - sy) / z);
        with(cx, |s| {
            if let Some(pp) = s.press.as_mut() { pp.moved = true; }
            s.grab_x = px0 - r.x;
            s.grab_y = py0 - (r.y - r.h / 2.0).max(f32::from(T::SP_4));
            s.drag_text = r.text.clone();
        });
        drag_start(&p.id, window, cx);
    }
    drag_move(ux, uy, cx);
    redraw(cx);
}

/// The press let go off an idea: the end of a drag, if one.
fn pointer_up(window: &mut Window, cx: &mut App) {
    if with(cx, |s| !s.drag_id.is_empty()) { drag_end(true, window, cx); }
    with(cx, |s| s.press = None);
}

/// Ctrl and the wheel: zoom about the pointer (`delta` up is +; `px`, `py`
/// in the canvas's view).
fn wheel_zoom(delta: f32, px_: f32, py_: f32, window: &mut Window, cx: &mut App) {
    let dy = -delta;
    let k = if dy.abs() < 40.0 { 0.006 } else { 0.0012 };
    let step = (-dy * k).clamp(-0.2, 0.2);
    let z = with(cx, |s| s.zoom);
    set_zoom(z * step.exp(), px_, py_, window, cx);
}

/// A pinch (`delta` 0.1: 10% larger) zooms about the point (`px`, `py` in the canvas's view).
pub(crate) fn pinch_zoom(delta: f32, px_: f32, py_: f32, window: &mut Window, cx: &mut App) {
    let z = with(cx, |s| s.zoom);
    set_zoom(z * (1.0 + delta).max(0.05), px_, py_, window, cx);
}

/// Zoom to `z` keeping the point (cx, cy) of the canvas's view still.
fn set_zoom(z: f32, cx_: f32, cy_: f32, _window: &mut Window, cx: &mut App) {
    let (sx, sy) = scroll_now(cx);
    let prev = with(cx, |s| s.zoom.max(0.01));
    let z = z.clamp(0.15, 3.0);
    let (ox, oy) = (cx_ - sx, cy_ - sy);
    let r = z / prev;
    with(cx, |s| { s.zoom = z; s.zoom_init = true; });
    set_scroll(ox * r - cx_, oy * r - cy_, cx);
    redraw(cx);
}

// ─── Drag to rearrange ───

fn drag_start(id: &str, window: &mut Window, cx: &mut App) {
    if with(cx, |s| s.edit.is_some()) { finish_edit(true, window, cx); }
    if with(cx, |s| s.draft.is_some()) { end_draft(window, cx); }
    clear_sel(cx);
    with(cx, |s| { s.target = None; s.want.clear(); s.want_gen += 1; s.drag_id = id.to_string(); s.hover = None; });
    render(window, cx);
}

fn drop_key(t: &Option<(Option<String>, usize)>) -> String { t.as_ref().map(|(p, i)| format!("{}#{i}", p.clone().unwrap_or_default())).unwrap_or_default() }

fn drag_move(x: f32, y: f32, cx: &mut App) {
    let id = with(cx, |s| { s.drag_x = x; s.drag_y = y; s.drag_id.clone() });
    if id.is_empty() { return; }
    let nw = dims().node;
    let t = with(cx, |s| tree::drop_target(&s.nodes, &s.lay, nw, &id, x, y));
    let (target, show) = match t {
        Target::Stay => return,
        Target::Back => (None, None),
        Target::Sib(p, i) => (Some(Target::Sib(p.clone(), i)), Some((p, i))),
        other => (Some(other), None),
    };
    let outline_changed = with(cx, |s| {
        let was = matches!(&s.target, Some(Target::Child(_))).then(|| s.target.clone());
        s.target = target.clone();
        was != matches!(&target, Some(Target::Child(_))).then(|| target.clone())
    });
    if outline_changed { in_window_later(cx); }
    // The gap opens once a new place has been held for a moment.
    let k = drop_key(&show);
    if with(cx, |s| s.want == k) { return; }
    let g_ = with(cx, |s| { s.want = k.clone(); s.want_gen += 1; s.want_gen });
    after(cx, 70, move |window, cx| {
        if with(cx, |s| s.want_gen != g_ || s.drag_id.is_empty()) { return; }
        let now = with(cx, |s| drop_key(&s.drop.as_ref().map(|d| (d.parent.clone(), d.index))));
        if now == k { return; }
        with(cx, |s| {
            let h = s.heights.get(&id).copied().unwrap_or(40.0);
            s.drop = show.map(|(p, i)| DropAt { parent: p, index: i, drag: id.clone(), h });
        });
        render(window, cx);
    });
}

/// Lay out again once this event is done (from where the window is busy).
fn in_window_later(cx: &mut App) { cx.defer(|cx| in_window(cx, render)); }

fn drag_end(commit: bool, window: &mut Window, cx: &mut App) {
    let id = with(cx, |s| std::mem::take(&mut s.drag_id));
    let t = with(cx, |s| { s.last_drag = Some(Instant::now()); s.want.clear(); s.want_gen += 1; s.drop = None; s.press = None; s.target.take() });
    let Some(t) = t.filter(|_| commit && !id.is_empty()) else { render(window, cx); return };
    // Put it there at once, then tell the server and take its word.
    let (parent, index) = match &t { Target::Root => (None, None), Target::Child(p) => (Some(p.clone()), None), Target::Sib(p, i) => (p.clone(), Some(*i)), _ => { render(window, cx); return; } };
    with(cx, |s| {
        if tree::apply_move(&mut s.nodes, &id, parent.as_deref(), index) { if let Some(p) = &parent { s.collapsed.remove(p); } }
    });
    render(window, cx);
    let mut body = json!({ "action": "moveNode", "id": id, "parentId": parent });
    if let Some(i) = index { body["index"] = json!(i); }
    post(body, cx, |_, _, cx| load(cx));
}

// ─── Media (the Media modal and viewer are Workspace's) ───

/// The toolbar's Media (openMediaModal 'Media · Mindmap').
fn media_modal(_window: &mut Window, cx: &mut App) {
    crate::media::open("Media \u{00b7} Mindmap", crate::media::Scope::kind("mindmap"), None, None, cx);
}

/// The idea dialog's Media and paperclip (openNodeMediaModal): the idea's
/// media, uploads linked to it; the dialog shows again when it closes.
fn node_media(id: &str, window: &mut Window, cx: &mut App) {
    close_detail(window, cx);
    let id = id.to_string();
    let back: crate::media::Back = { let id = id.clone(); std::rc::Rc::new(move |window, cx| open_detail(&id, window, cx)) };
    crate::media::open("Idea media", crate::media::Scope::linked(json!({ "type": "node", "nodeId": id }), None), None, Some(back), cx);
}

// ─── The idea dialog ───

fn open_detail(id: &str, window: &mut Window, cx: &mut App) {
    if with(cx, |s| !s.nodes.iter().any(|n| n.id == id)) { return; }
    let f = with(cx, |s| {
        s.detail = Some(id.to_string()); s.d_media.clear(); s.d_thumbs.clear(); s.d_editing = false; s.d_open = true; s.hover = None;
        s.ui.as_ref().map(|u| u.d_focus.clone())
    });
    draw::open_dialog(draw::Dlg::Detail, window, cx);
    if let Some(f) = f { f.focus(window, cx); }
    refresh_detail(cx);
    load_detail_media(cx);
    redraw(cx);
}

/// The dialog's view of its idea, from the latest map.
fn refresh_detail(cx: &mut App) {
    let names = project_names(cx);
    with(cx, |s| {
        let Some(id) = s.detail.clone() else { return };
        let Some(n) = s.nodes.iter().find(|n| n.id == id).cloned() else { return };
        s.d_title = if n.text.is_empty() { "Idea".into() } else { n.text.clone() };
        let i = &n.info;
        let (kind, main, text, st, todo) = if !n.linked {
            if !n.project.is_empty() { ("scope", format!("Scoped to project \u{201c}{}\u{201d}", name_of(&names, &n.project)), String::new(), "(no task linked)".to_string(), 0) }
            else { ("", String::new(), String::new(), String::new(), 0) }
        } else if !i.is_null() && !truthy(&i["missing"]) {
            if s_of(i, "kind") == "todo" { ("link", "Subtask".to_string(), s_of(i, "text"), format!("\u{00b7} in {}", s_of(i, "taskTitle")), if truthy(&i["done"]) { 2 } else { 1 }) }
            else { ("link", format!("Kanban: {} \u{00b7} {}", s_of(i, "project"), s_of(i, "title")), String::new(), format!("[{}{}]", s_of(i, "status").replacen('_', " ", 1), if truthy(&i["deleted"]) { ", deleted" } else { "" }), 0) }
        } else { ("missing", "Linked item no longer exists".to_string(), String::new(), String::new(), 0) };
        s.d_link_kind = kind.into();
        s.d_link_main = main;
        s.d_link_text = text;
        s.d_link_st = st;
        s.d_link_todo = todo;
        s.d_view_notes = n.notes.clone();
        s.d_link_label = if n.linked || !n.project.is_empty() { "Unlink".into() } else { "Link / scope".into() };
    });
}

/// The strip of media hung on the idea (loadMmDetailMedia): ten at most.
fn load_detail_media(cx: &mut App) {
    let Some(id) = with(cx, |s| s.detail.clone()) else { return };
    let p = crundi_shell::project(cx);
    let path = format!("/api/media/list?{}linkType=node&nodeId={}", if p.is_empty() { String::new() } else { format!("project={}&includeGeneral=1&", enc(&p)) }, enc(&id));
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
        if with(cx, |s| s.detail.as_deref() != Some(id.as_str())) { return; }
        let items: Vec<Value> = r.ok().filter(|v| truthy(&v["ok"])).and_then(|v| v.get("items").and_then(|x| x.as_array()).cloned()).unwrap_or_default().into_iter().take(10).collect();
        with(cx, |s| s.d_media = items);
        fill_thumbs(cx);
    });
}

fn format_of(b: &[u8]) -> ImageFormat {
    if b.starts_with(b"\x89PNG") { ImageFormat::Png } else if b.starts_with(&[0xff, 0xd8]) { ImageFormat::Jpeg } else if b.starts_with(b"GIF") { ImageFormat::Gif } else if b.len() > 12 && &b[8..12] == b"WEBP" { ImageFormat::Webp } else { ImageFormat::Png }
}

fn fill_thumbs(cx: &mut App) {
    let items = with(cx, |s| s.d_media.clone());
    with(cx, |s| {
        s.d_thumbs = items.iter().map(|it| {
            let id = s_of(it, "id");
            Thumb { image: s.pics.get(&id).cloned().flatten(), name: s_of(it, "originalName"), kind: s_of(it, "kind"), id }
        }).collect();
    });
    redraw(cx);
    // Pictures load once each, then the strip is drawn again.
    for it in items {
        let id = s_of(&it, "id");
        if s_of(&it, "kind") != "image" || with(cx, |s| s.pics.contains_key(&id)) { continue; }
        with(cx, |s| s.pics.insert(id.clone(), None));
        let Some(api) = crundi_shell::api(cx) else { return };
        let path = format!("/api/media/raw/{}", enc(&id));
        crundi_shell::run(cx, move || api.get_bytes(&path), move |r, cx| {
            if let Ok((200, b)) = r { if !b.is_empty() { let f = format_of(&b); with(cx, |s| s.pics.insert(id, Some(Arc::new(Image::from_bytes(f, b))))); fill_thumbs(cx); } }
        });
    }
}

/// A thumbnail in the idea dialog's strip: the lightbox (openMediaLightbox), then the dialog again.
fn open_media(id: &str, window: &mut Window, cx: &mut App) {
    let Some((it, node)) = with(cx, |s| s.d_media.iter().find(|x| s_of(x, "id") == id).cloned().zip(s.detail.clone())) else { return };
    if s_of(&it, "kind") != "other" { close_detail(window, cx); }
    let back: crate::media::Back = std::rc::Rc::new(move |window, cx| open_detail(&node, window, cx));
    crate::media::lightbox(it, Some(back), cx);
}

fn close_detail(window: &mut Window, cx: &mut App) {
    let was = with(cx, |s| { let w = s.d_open; s.detail = None; s.d_open = false; s.d_editing = false; w });
    // The modal layer closes (and its on_close finds the dialog closed).
    if was && w::modal_open(cx) { w::close_dialog(window, cx); }
    focus_canvas(window, cx);
    redraw(cx);
}

/// A note's box in the dialog's editing (its words go to d_notes).
fn note_box(text: &str, window: &mut Window, cx: &mut App) -> (Entity<w::TextareaState>, Subscription) {
    let e = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(2, 6).placeholder("Note…").default_value(text.to_string()));
    let sub = cx.subscribe(&e, |e, ev: &w::InputEvent, cx| if matches!(ev, w::InputEvent::Change) {
        let v = e.read(cx).value().to_string();
        with(cx, |s| {
            let i = s.ui.as_ref().and_then(|u| u.d_notes.iter().position(|x| x == &e));
            if let Some(n) = i.and_then(|i| s.d_notes.get_mut(i)) { *n = v; }
        });
    });
    (e, sub)
}

fn d_note_del(i: usize, _window: &mut Window, cx: &mut App) {
    with(cx, |s| {
        if i < s.d_notes.len() { s.d_notes.remove(i); }
        if let Some(u) = s.ui.as_mut() { if i < u.d_notes.len() { u.d_notes.remove(i); } }
    });
    redraw(cx);
}

fn d_act(a: &str, window: &mut Window, cx: &mut App) {
    let Some(id) = with(cx, |s| s.detail.clone()) else { return };
    match a {
        "close" => close_detail(window, cx),
        "edit" => {
            let n = with(cx, |s| s.nodes.iter().find(|n| n.id == id).cloned()).unwrap_or_default();
            let boxes: Vec<(Entity<w::TextareaState>, Subscription)> = n.notes.iter().map(|t| note_box(t, window, cx)).collect();
            let e = with(cx, |s| {
                s.d_notes = n.notes.clone();
                s.d_editing = true;
                let u = s.ui.as_mut()?;
                u.d_notes = boxes.iter().map(|b| b.0.clone()).collect();
                Some(u.d_text.clone())
            });
            with(cx, |s| if let Some(u) = s.ui.as_mut() { u.subs.extend(boxes.into_iter().map(|b| b.1)); });
            set_area(e.clone(), &n.text, window, cx);
            if let Some(e) = e { e.update(cx, |st, cx| st.focus(window, cx)); }
            redraw(cx);
        }
        "add-note" => {
            let (e, sub) = note_box("", window, cx);
            with(cx, |s| { s.d_notes.push(String::new()); if let Some(u) = s.ui.as_mut() { u.d_notes.push(e.clone()); u.subs.push(sub); } });
            e.update(cx, |st, cx| st.focus(window, cx));
            redraw(cx);
        }
        "save" => {
            let text = with(cx, |s| s.ui.as_ref().map(|u| u.d_text.clone())).map(|e| e.read(cx).value().to_string()).unwrap_or_default().trim().to_string();
            if text.is_empty() { toast_error(window, cx, "Idea text is required"); return; }
            let notes: Vec<String> = with(cx, |s| s.d_notes.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect());
            post(json!({ "action": "updateNode", "id": id, "text": text, "notes": notes }), cx, |_, window, cx| { close_detail(window, cx); load(cx); });
        }
        "child" => { close_detail(window, cx); start_draft(Some(id), None, window, cx); }
        "link" => {
            close_detail(window, cx);
            let linked = with(cx, |s| s.nodes.iter().find(|n| n.id == id).map(|n| n.linked || !n.project.is_empty())).unwrap_or(false);
            // "Unlink" clears a task link and a project scope alike.
            if linked { post(json!({ "action": "unlinkNode", "id": id }), cx, |_, _, cx| load(cx)); } else { open_link(&id, window, cx); }
        }
        "media" => node_media(&id, window, cx),
        "delete" => {
            close_detail(window, cx);
            w::confirm(window, cx, w::Confirm { title: "".into(), message: "Delete this idea and all its sub-ideas?".into(), ok: "Delete".into(), danger: true }, move |_, cx| {
                post(json!({ "action": "deleteNode", "id": id }), cx, |_, _, cx| load(cx));
            });
        }
        _ => {}
    }
}

// ─── The link picker ───

fn open_link(id: &str, window: &mut Window, cx: &mut App) {
    let (node, parent) = with(cx, |s| {
        let n = s.nodes.iter().find(|n| n.id == id).cloned();
        let p = n.as_ref().and_then(|n| n.parent.as_ref()).and_then(|p| s.nodes.iter().find(|x| &x.id == p).cloned());
        (n, p)
    });
    if node.is_none() { return; }
    // A child strictly keeps its parent's project.
    let locked = parent.map(|p| p.eff).unwrap_or_default();
    let mut keys: Vec<String> = vec![];
    let mut names: Vec<SharedString> = vec![];
    for p in crundi_shell::stores(cx).projects.read(cx).list.clone() {
        names.push(if p.name.is_empty() { p.alias.clone().into() } else { p.name.clone().into() });
        keys.push(p.alias);
    }
    if !locked.is_empty() && !keys.contains(&locked) { keys.insert(0, locked.clone()); names.insert(0, locked.clone().into()); }
    let cur = crundi_shell::project(cx);
    let sel = if !locked.is_empty() { keys.iter().position(|k| *k == locked).unwrap_or(0) } else { keys.iter().position(|k| *k == cur).unwrap_or(0) };
    let lname = name_of(&project_names(cx), &locked);
    let e = with(cx, |s| {
        s.link_id = id.to_string(); s.l_keys = keys; s.l_board = Value::Null;
        s.l_names = names; s.l_sel = sel; s.l_locked = !locked.is_empty();
        s.l_locked_text = format!("Scoped to \u{201c}{lname}\u{201d} (inherited from parent) \u{2014} can only link tasks in this project.");
        s.l_open = true; s.l_rows.clear();
        s.ui.as_ref().map(|u| u.l_search.clone())
    });
    set_line(e.clone(), "", window, cx);
    draw::open_dialog(draw::Dlg::Link, window, cx);
    if let Some(e) = e { e.update(cx, |st, cx| st.focus(window, cx)); }
    load_link_board(cx);
    redraw(cx);
}

fn l_project(cx: &mut App) -> String { with(cx, |s| s.l_keys.get(s.l_sel).cloned()).unwrap_or_default() }

fn load_link_board(cx: &mut App) {
    let proj = l_project(cx);
    let label = format!("Scope to \u{201c}{}\u{201d} (no task)", name_of(&project_names(cx), &proj));
    let g_ = with(cx, |s| { s.l_scope_label = label; s.l_loading = true; s.l_gen += 1; s.l_gen });
    let path = format!("/api/kanban?project={}", enc(&proj));
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
        if with(cx, |s| s.l_gen != g_) { return; }
        let b = r.ok().filter(|v| truthy(&v["ok"])).map(|v| v["board"].clone()).unwrap_or(json!({ "tasks": [] }));
        with(cx, |s| { s.l_board = b; s.l_loading = false; });
        link_list(cx);
        redraw(cx);
    });
}

fn link_list(cx: &mut App) {
    let q = with(cx, |s| s.ui.as_ref().map(|u| u.l_search.clone())).map(|e| e.read(cx).value().to_string()).unwrap_or_default().trim().to_lowercase();
    with(cx, |s| {
        let mut rows = vec![];
        for t in s.l_board.get("tasks").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            let title = s_of(&t, "title");
            let t_match = q.is_empty() || title.to_lowercase().contains(&q);
            let todos: Vec<Value> = t.get("todos").and_then(|x| x.as_array()).cloned().unwrap_or_default().into_iter().filter(|td| !truthy(&td["deleted"])).collect();
            let hits: Vec<&Value> = todos.iter().filter(|td| q.is_empty() || s_of(td, "text").to_lowercase().contains(&q)).collect();
            if !t_match && hits.is_empty() { continue; }
            let tid = s_of(&t, "id");
            rows.push(LinkRow { task: tid.clone(), todo: String::new(), text: title, status: s_of(&t, "status").replacen('_', " ", 1), done: false });
            let shown: Vec<&Value> = if q.is_empty() { todos.iter().collect() } else { hits };
            for td in shown { rows.push(LinkRow { task: tid.clone(), todo: s_of(td, "id"), text: s_of(td, "text"), status: String::new(), done: truthy(&td["done"]) }); }
        }
        s.l_rows = rows;
    });
}

fn close_link(window: &mut Window, cx: &mut App) {
    let was = with(cx, |s| std::mem::replace(&mut s.l_open, false));
    if was && w::modal_open(cx) { w::close_dialog(window, cx); }
    focus_canvas(window, cx);
    redraw(cx);
}

fn l_act(a: &str, x: &str, y: &str, window: &mut Window, cx: &mut App) {
    let id = with(cx, |s| s.link_id.clone());
    match a {
        "project" => { with(cx, |s| s.l_sel = x.parse().unwrap_or(0)); load_link_board(cx); redraw(cx); }
        "search" => { link_list(cx); redraw(cx); }
        "scope" => {
            let proj = l_project(cx);
            if proj.is_empty() { return; }
            post(json!({ "action": "scopeProject", "id": id, "project": proj }), cx, |d, window, cx| { if truthy(&d["ok"]) { close_link(window, cx); load(cx); } });
        }
        "pick" => {
            let mut body = json!({ "action": "linkNode", "id": id, "project": l_project(cx), "taskId": x });
            if !y.is_empty() { body["todoId"] = json!(y); }
            post(body, cx, |d, window, cx| { if truthy(&d["ok"]) { close_link(window, cx); load(cx); } });
        }
        _ => close_link(window, cx),
    }
}

// ─── The shortcut sheet ───

fn open_help(window: &mut Window, cx: &mut App) {
    let n = help::rows().len();
    let f = with(cx, |s| { if s.h_open.len() != n { s.h_open = vec![false; n]; } s.h_shown = true; s.ui.as_ref().map(|u| u.h_focus.clone()) });
    draw::open_dialog(draw::Dlg::Help, window, cx);
    if let Some(f) = f { f.focus(window, cx); }
    redraw(cx);
}

fn h_toggle(i: usize, cx: &mut App) { with(cx, |s| if let Some(o) = s.h_open.get_mut(i) { *o = !*o; }); }

fn h_act(a: &str, window: &mut Window, cx: &mut App) {
    if a == "all" {
        // Only the rows with a picture count (the others have nothing to open).
        let dias: Vec<bool> = help::rows().iter().map(|r| matches!(r, help::Row::Item { dia: Some(_), .. })).collect();
        with(cx, |s| {
            let open = s.h_open.iter().zip(&dias).any(|(o, d)| *d && !*o);
            for (o, d) in s.h_open.iter_mut().zip(&dias) { if *d { *o = open; } }
            s.h_all = open;
        });
    } else {
        let was = with(cx, |s| std::mem::replace(&mut s.h_shown, false));
        if was && w::modal_open(cx) { w::close_dialog(window, cx); }
        focus_canvas(window, cx);
    }
    redraw(cx);
}

// ─── Jumping to an idea (the web's flashMindmapNode) ───

/// Show an idea: unfold what hides it, put the ring on it and flash it;
/// `open` opens its details too. For the Media "jump to source".
pub fn flash_node(id: &str, open: bool, window: &mut Window, cx: &mut App) {
    let id = id.to_string();
    let go = move |window: &mut Window, cx: &mut App| {
        with(cx, |s| {
            let mut cur = s.nodes.iter().find(|n| n.id == id).and_then(|n| n.parent.clone());
            let mut hops = 0;
            while let Some(p) = cur { s.collapsed.remove(&p); hops += 1; if hops > s.nodes.len() { break; } cur = s.nodes.iter().find(|n| n.id == p).and_then(|n| n.parent.clone()); }
        });
        let g_ = with(cx, |s| { s.flash = id.clone(); s.flash_gen += 1; s.flash_gen });
        render(window, cx);
        if with(cx, |s| s.lay.get(&id).is_some()) { select(&id, true, window, cx); }
        after(cx, 2400, move |window, cx| { if with(cx, |s| s.flash_gen == g_) { with(cx, |s| s.flash.clear()); render(window, cx); } });
        if open { open_detail(&id, window, cx); }
    };
    if with(cx, |s| s.loaded) { go(window, cx); } else {
        crate::util::show_tab(cx, "mindmap");
        after(cx, 600, go);
    }
}

// ─── For the test driver ───

fn row_json(r: &Row) -> Value {
    // Key by key (one json! of all of them reaches the macro's recursion limit).
    let pairs: Vec<(&str, Value)> = vec![("id", json!(r.id)), ("kind", json!(r.kind)), ("parent", json!(r.parent)), ("x", json!(r.x)), ("y", json!(r.y)), ("h", json!(r.h)),
        ("text", json!(r.text)), ("root", json!(r.root)), ("linked", json!(r.linked)), ("scoped", json!(r.scoped)), ("pending", json!(r.pending)),
        ("chip", json!(r.chip)), ("chipText", json!(r.chip_text)), ("todo", json!(r.todo)), ("notes", json!(r.notes)), ("kids", json!(r.kids)), ("collapsed", json!(r.collapsed)),
        ("hit", json!(r.hit)), ("dim", json!(r.dim)), ("hidden", json!(r.hidden)), ("lifted", json!(r.lifted)), ("target", json!(r.target)), ("flash", json!(r.flash))];
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// The driver's "planning:<name>" actions (the Slint client's names).
pub fn test_ui(_: &Stores, name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    if !name.starts_with("mindmap-") { return None; }
    ensure(window, cx);
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let id_of = |text: &str, cx: &mut App| with(cx, |s| s.nodes.iter().find(|n| n.text == text).map(|n| n.id.clone())).unwrap_or_default();
    Some(match name {
        "mindmap-act" => { act(&a(0), &a(1), window, cx); json!({ "ok": true }) }
        "mindmap-reload" => { load(cx); json!({ "ok": true }) }
        "mindmap-load-path" => { load_from(&a(0), cx); json!({ "ok": true }) }
        "mindmap-search" => { set_search(&a(0), window, cx); json!({ "ok": true }) }
        "mindmap-click" => { let id = id_of(&a(0), cx); node_click(&id, 1, window, cx); json!({ "ok": true, "id": id }) }
        "mindmap-detail" => { let id = id_of(&a(0), cx); open_detail(&id, window, cx); json!({ "ok": true, "id": id }) }
        "mindmap-select" => { let id = id_of(&a(0), cx); select(&id, true, window, cx); json!({ "ok": true, "id": id }) }
        "mindmap-canvas-press" => { canvas_press(window, cx); json!({ "ok": true }) }
        "mindmap-d-act" => { d_act(&a(0), window, cx); json!({ "ok": true }) }
        "mindmap-d-text" => { let e = with(cx, |s| s.ui.as_ref().map(|u| u.d_text.clone())); set_area(e, &a(0), window, cx); json!({ "ok": true }) }
        "mindmap-d-note" => {
            let i: usize = a(0).parse().unwrap_or(0);
            let e = with(cx, |s| { if let Some(n) = s.d_notes.get_mut(i) { *n = a(1); } s.ui.as_ref().and_then(|u| u.d_notes.get(i).cloned()) });
            set_area(e, &a(1), window, cx);
            json!({ "ok": true })
        }
        "mindmap-l-act" => { l_act(&a(0), &a(1), &a(2), window, cx); json!({ "ok": true }) }
        "mindmap-l-search" => { let e = with(cx, |s| s.ui.as_ref().map(|u| u.l_search.clone())); set_line(e, &a(0), window, cx); link_list(cx); json!({ "ok": true }) }
        "mindmap-h-toggle" => { h_toggle(a(0).parse().unwrap_or(0), cx); json!({ "ok": true }) }
        "mindmap-h-act" => { h_act(&a(0), window, cx); json!({ "ok": true }) }
        "mindmap-zoom" => { let (vw, vh) = with(cx, |s| (s.view_w, s.view_h)); set_zoom(a(0).parse().unwrap_or(1.0), vw / 2.0, vh / 2.0, window, cx); json!({ "ok": true }) }
        "mindmap-wheel" => { wheel_zoom(a(0).parse().unwrap_or(0.0), a(1).parse().unwrap_or(0.0), a(2).parse().unwrap_or(0.0), window, cx); json!({ "ok": true }) }
        // The canvas's place is measured as it is drawn (every frame here).
        "mindmap-measure" => json!({ "ok": true }),
        // Pan by (dx, dy) canvas pixels (+: further right, further down).
        "mindmap-all-div" => { let on = a(0) == "true"; with(cx, |s| s.all_div = on); redraw(cx); json!({ "ok": true }) }
        "mindmap-pan" => { scroll_by(-a(0).parse().unwrap_or(0.0), -a(1).parse().unwrap_or(0.0), cx); json!({ "ok": true }) }
        "mindmap-flash" => { let id = id_of(&a(0), cx); flash_node(&id, a(1) == "true", window, cx); json!({ "ok": true }) }
        _ => return None,
    })
}

/// This screen's state for the driver (planningArea.mindmap; native-gpui's names).
pub fn state(_: &Stores, _window: &mut Window, cx: &mut App) -> Value {
    let (dt, et, d_text, l_q) = (draft_text(cx), edit_text(cx), cx.try_global::<Handle>().map(|h| h.0.read(cx)).and_then(|s| s.ui.as_ref()).map(|u| u.d_text.read(cx).value().to_string()).unwrap_or_default(),
        cx.try_global::<Handle>().map(|h| h.0.read(cx)).and_then(|s| s.ui.as_ref()).map(|u| u.l_search.read(cx).value().to_string()).unwrap_or_default());
    let _ = l_q;
    let help_rows: Vec<Value> = help::rows().into_iter().enumerate().map(|(i, r)| match r {
        help::Row::Head(h) => json!({ "head": h, "text": "", "dia": false, "open": false }),
        help::Row::Item { text, dia, .. } => json!({ "head": "", "text": text, "dia": dia.is_some(), "open": cx.try_global::<Handle>().map(|h| h.0.read(cx)).and_then(|s| s.h_open.get(i).copied()).unwrap_or(false) }),
    }).collect();
    let Some(s) = cx.try_global::<Handle>().map(|h| h.0.read(cx)) else { return json!({}) };
    let drawn: Vec<String> = s.drawn.borrow().clone();
    let (z, (sx, sy)) = (s.zoom, { let p = s.pan().read(cx); (-p.x, -p.y) });
    let nw = dims().node;
    // Each box in window coordinates (for a real drag in the test).
    let nodes: Vec<Value> = s.rows.iter().map(|r| {
        let mut v = row_json(r);
        let top = (r.y - r.h / 2.0).max(f32::from(T::SP_4));
        v["box"] = json!({ "x": s.canvas_x + sx + r.x * z, "y": s.canvas_y + sy + top * z, "w": nw * z, "h": r.h * z });
        v
    }).collect();
    let draft = s.draft.as_ref().map(|d| json!({ "parent": d.parent, "after": d.after, "on": s.draft_on, "text": dt }));
    let edit = s.edit.as_ref().map(|e| json!({ "id": e, "on": s.edit_on, "text": et }));
    let ring = if s.ring_on { json!({ "warn": s.ring_warn, "msg": s.ring_msg }) } else { Value::Null };
    let detail = if s.d_open {
        let pairs: Vec<(&str, Value)> = vec![("id", json!(s.detail)), ("title", json!(s.d_title)), ("linkKind", json!(s.d_link_kind)), ("linkMain", json!(s.d_link_main)), ("linkText", json!(s.d_link_text)),
            ("linkSt", json!(s.d_link_st)), ("linkLabel", json!(s.d_link_label)), ("notes", json!(s.d_view_notes)), ("editing", json!(s.d_editing)), ("text", json!(d_text)), ("editNotes", json!(s.d_notes)), ("thumbs", json!(s.d_thumbs.len()))];
        Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    } else { Value::Null };
    let rows: Vec<Value> = s.l_rows.iter().map(|r| json!({ "task": r.task, "todo": r.todo, "text": r.text, "status": r.status, "done": r.done })).collect();
    let names: Vec<String> = s.l_names.iter().map(|t| t.to_string()).collect();
    let link = if s.l_open { json!({ "names": names, "sel": s.l_sel, "locked": s.l_locked, "lockedText": s.l_locked_text, "scope": s.l_scope_label, "loading": s.l_loading, "rows": rows }) } else { Value::Null };
    let help = if s.h_shown { json!({ "all": s.h_all, "rows": help_rows }) } else { Value::Null };
    let chips: Vec<Value> = s.chips.iter().map(|c| json!({ "key": c.key, "label": c.label, "off": c.off })).collect();
    let mut m = serde_json::Map::new();
    let mut put = |k: &str, v: Value| { m.insert(k.to_string(), v); };
    put("state", json!(s.state)); put("error", json!(s.error)); put("search", json!(s.search));
    put("bar", json!(s.bar)); put("hiddenCount", json!(s.hidden_count)); put("chips", json!(chips));
    put("nodes", json!(nodes)); put("edges", json!(s.edges)); put("baseW", json!(s.base_w)); put("baseH", json!(s.base_h));
    put("zoom", json!(z)); put("scroll", json!([sx, sy])); put("view", json!([s.view_w, s.view_h])); put("canvas", json!([s.canvas_x, s.canvas_y]));
    put("draft", json!(draft)); put("edit", json!(edit));
    put("sel", json!(s.sel)); put("anchor", json!(s.anchor)); put("saving", json!(s.saving));
    put("collapsed", json!(s.collapsed.iter().collect::<Vec<_>>())); put("hidden", json!(s.hidden.iter().collect::<Vec<_>>()));
    put("ring", ring); put("hover", s.hover.as_ref().map(|h| json!(h.id)).unwrap_or(Value::Null));
    put("drag", if s.drag_id.is_empty() { Value::Null } else { json!({ "id": s.drag_id }) });
    put("detail", detail); put("link", link); put("help", help);
    put("dialogOpen", json!(s.d_open || s.l_open || s.h_shown));
    put("relayouts", json!(s.relayouts));
    // The ideas the map drew in its last frame (it draws only those on screen).
    put("drawn", json!(drawn));
    Value::Object(m)
}
