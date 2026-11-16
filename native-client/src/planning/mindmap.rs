//! The Mind map (4.3): the mindmap tab and panel. The web's loadMindmap,
//! renderMindmap, layoutMindmap, the keyboard (mmStep, the Delete warning,
//! mmStartDraft / mmDraftCommit, mmStartEdit / mmFinishEdit), zoom, the
//! filter, the project chips, drag to rearrange (mindmapDragHandlers), the
//! idea dialog, the link picker and the shortcut sheet (src/webapp-html.js).
//! The map is global (every project's ideas), loaded whole after each change
//! and on the server's `mindmap` event. Pure rules: mindmap_tree.rs.

#[path = "mindmap_tree.rs"]
mod tree;
#[path = "mindmap_help.rs"]
mod help;

use crate::areas::AreaEvent;
use crate::{kit, MainWindow, PlanningMindmap, PlanningMmBox, PlanningMmChip, PlanningMmHelp, PlanningMmKey, PlanningMmKeyLine, PlanningMmLinkRow, PlanningMmNode, PlanningMmThumb};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, collections::{HashMap, HashSet, VecDeque}, rc::Rc, time::{Duration, Instant}};
use tree::{Draft, DropAt, Kind, Layout, Node, Target};

struct Save { tmp: String, text: String, parent: Option<String> }

#[derive(Default)]
struct M {
    nodes: Vec<Node>,
    loaded: bool,
    collapsed: HashSet<String>,
    hidden: HashSet<String>,
    search: String,
    saved_collapsed: Option<HashSet<String>>,
    zoom_init: bool,
    heights: HashMap<String, f32>,
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
    click_id: String,
    click_at: Option<Instant>,
    click_gen: u64,
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
    // The idea dialog.
    detail: Option<String>,
    d_notes: Vec<String>,
    d_media: Vec<Value>,
    thumbs: HashMap<String, Option<slint::Image>>,
    // The link picker.
    link_id: String,
    l_keys: Vec<String>,
    l_board: Value,
    l_gen: u64,
    // The shortcut sheet: which rows are open.
    h_open: Vec<bool>,
    // Models set once (README: lists are never replaced wholesale).
    m_nodes: Option<Rc<VecModel<PlanningMmNode>>>,
    m_chips: Option<Rc<VecModel<PlanningMmChip>>>,
    m_rows: Option<Rc<VecModel<PlanningMmLinkRow>>>,
    m_thumbs: Option<Rc<VecModel<PlanningMmThumb>>>,
    m_help: Option<Rc<VecModel<PlanningMmHelp>>>,
    m_edit_notes: Option<Rc<VecModel<SharedString>>>,
}

thread_local! { static S: RefCell<M> = RefCell::new(M::default()); }

fn with<R>(f: impl FnOnce(&mut M) -> R) -> R { S.with(|s| f(&mut s.borrow_mut())) }

fn mg(f: impl FnOnce(&PlanningMindmap)) { if let Some(w) = kit::ui() { f(&w.global::<PlanningMindmap>()); } }

fn mg_get<R: Default>(f: impl FnOnce(&PlanningMindmap) -> R) -> R { kit::ui().map(|w| f(&w.global::<PlanningMindmap>())).unwrap_or_default() }

/// Give the canvas the keys, once a text box being closed has gone (a
/// focus asked for while the focused box is still there is lost with it).
fn focus_canvas() {
    slint::Timer::single_shot(Duration::from_millis(40), || mg(|g| g.set_canvas_gen(g.get_canvas_gen() + 1)));
}

fn modal(on: bool) { if let Some(w) = kit::ui() { w.global::<crate::Shell>().set_modal_planning(on); } }

/// The layout's sizes, from the theme (MM_COL_W, MM_NODE_W, MM_PAD, the gap
/// between stacked ideas, the height before an idea is measured).
fn dims() -> tree::Dims {
    let Some(w) = kit::ui() else { return tree::Dims::default() };
    let t = w.global::<crate::Theme>();
    tree::Dims { col: t.get_mindmap_col(), node: t.get_mindmap_node(), pad: t.get_sp_24(), gap: t.get_sp_16(), min_h: t.get_sp_48() }
}

pub fn init(w: &MainWindow) {
    let g = w.global::<PlanningMindmap>();
    let nodes = kit::model::<PlanningMmNode>();
    let chips = kit::model::<PlanningMmChip>();
    let rows = kit::model::<PlanningMmLinkRow>();
    let thumbs = kit::model::<PlanningMmThumb>();
    let hrows = kit::model::<PlanningMmHelp>();
    let en = kit::model::<SharedString>();
    g.set_nodes(kit::rc(&nodes));
    g.set_chips(kit::rc(&chips));
    g.set_l_rows(kit::rc(&rows));
    g.set_d_thumbs(kit::rc(&thumbs));
    g.set_h_rows(kit::rc(&hrows));
    g.set_d_edit_notes(kit::rc(&en));
    g.set_edges("M 0 0".into());
    g.set_dashes("M 0 0".into());
    with(|s| { s.m_nodes = Some(nodes); s.m_chips = Some(chips); s.m_rows = Some(rows); s.m_thumbs = Some(thumbs); s.m_help = Some(hrows); s.m_edit_notes = Some(en); });
    g.on_act(|a, id| act(a.as_str(), id.as_str()));
    g.on_searched(|q| set_search(q.as_str()));
    g.on_canvas_press(canvas_press);
    g.on_node_click(|id| node_click(id.as_str()));
    g.on_node_dbl(|id| { let id = id.to_string(); if !mg_get(|g| g.get_d_open()) && with(|s| s.click_id.is_empty() && s.edit.as_deref() != Some(id.as_str())) { open_detail(&id); } });
    g.on_measured(|id, h| measured(id.as_str(), h));
    g.on_wheel_zoom(wheel_zoom);
    g.on_key(|t, c, s, a| key(t.as_str(), c, s, a));
    g.on_draft_key(|t, c, s, a| draft_key(t.as_str(), c, s, a));
    g.on_edit_key(|t, c, s, a| edit_key(t.as_str(), c, s, a));
    // Clicked elsewhere (the filter box, a dialog): the box closes; an edit saves.
    g.on_draft_blur(|| { let gen = with(|s| s.draft_gen); slint::Timer::single_shot(Duration::ZERO, move || { if with(|s| s.draft.is_some() && s.draft_gen == gen) { end_draft(); } }); });
    g.on_edit_blur(|| { let gen = with(|s| s.edit_gen); slint::Timer::single_shot(Duration::ZERO, move || { if with(|s| s.edit.is_some() && s.edit_gen == gen) { finish_edit(true); } }); });
    g.on_hover(|id, on| hover(id.as_str(), on));
    g.on_drag_start(|id| drag_start(id.as_str()));
    g.on_drag_move(drag_move);
    g.on_drag_end(drag_end);
    g.on_d_act(|a| d_act(a.as_str()));
    g.on_d_thumb(|id| open_media(id.as_str()));
    g.on_d_note_edit(|i, v| with(|s| if let Some(n) = s.d_notes.get_mut(i as usize) { *n = v.to_string(); }));
    g.on_d_note_del(|i| {
        with(|s| if (i as usize) < s.d_notes.len() { s.d_notes.remove(i as usize); });
        // A fresh model: the boxes after it show their own words again.
        let notes = with(|s| s.d_notes.clone());
        let en = kit::model::<SharedString>();
        en.set_vec(notes.into_iter().map(SharedString::from).collect::<Vec<_>>());
        mg(|g| { g.set_d_note_focus(-1); g.set_d_edit_notes(kit::rc(&en)); });
        with(|s| s.m_edit_notes = Some(en));
    });
    g.on_l_act(|a, x, y| l_act(a.as_str(), x.as_str(), y.as_str()));
    g.on_h_toggle(|i| { with(|s| if let Some(o) = s.h_open.get_mut(i as usize) { *o = !*o; }); fill_help(); });
    g.on_h_act(|a| match a.as_str() {
        "all" => {
            let open = with(|s| { let open = s.h_open.iter().any(|o| !*o); for o in s.h_open.iter_mut() { *o = open; } open });
            mg(|g| g.set_h_all(open));
            fill_help();
        }
        _ => { mg(|g| g.set_h_open(false)); modal(false); }
    });
}

/// Whether the map is on show: its tab, or a pane on the workbench.
pub fn visible() -> bool {
    let Some(w) = kit::ui() else { return false };
    let tab = w.global::<crate::Shell>().get_tab();
    tab == "mindmap" || (tab == "workbench" && w.get_panes().iter().any(|p| p.key == "panel:mindmap"))
}

pub fn signed_in() { if visible() { load(); } }

pub fn signed_out() {
    with(|s| {
        s.nodes.clear(); s.loaded = false; s.draft = None; s.edit = None; s.sel.clear(); s.anchor.clear(); s.drop = None; s.target = None;
        s.queue.clear(); s.saving = 0; s.busy = false; s.detail = None; s.heights.clear(); s.zoom_init = false; s.thumbs.clear();
    });
    mg(|g| {
        g.set_state("loading".into()); g.set_d_open(false); g.set_l_open(false); g.set_h_open(false);
        g.set_draft_on(false); g.set_edit_on(false); g.set_hover_on(false); g.set_ring_on(false); g.set_drag_id("".into());
    });
    if let Some(m) = with(|s| s.m_nodes.clone()) { kit::sync_rows(&m, vec![]); }
}

pub fn project(_alias: &str) { if with(|s| s.loaded) { render(); } }

pub fn tab_shown(tab: &str) {
    if tab == "mindmap" || (tab == "workbench" && visible()) { load(); }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Server("mindmap", _) => { if visible() { load(); } }
        AreaEvent::Connected(true) => { if visible() { load(); } }
        _ => {}
    }
}

// ─── Loading ───

/// The map again, from the server. Ideas typed a moment ago and still on
/// their way are not in what it would send back: wait for them.
pub fn load() { load_from("/api/mindmap"); }

fn load_from(path: &str) {
    if with(|s| { if s.saving > 0 { s.reload_wanted = true; true } else { false } }) { return; }
    let gen = with(|s| { s.load_gen += 1; s.load_gen });
    let path = path.to_string();
    kit::call(move |api| api.get(&path), move |r| {
        if with(|s| s.load_gen != gen) { return; }
        if with(|s| { if s.saving > 0 { s.reload_wanted = true; true } else { false } }) { return; }
        match r {
            Err(e) => {
                with(|s| s.loaded = false);
                mg(|g| { g.set_error(format!("Failed to load mindmap: {e}").into()); g.set_state("error".into()); });
            }
            Ok(v) => {
                let nodes = if kit::truthy(&v["ok"]) { tree::parse(&v) } else { vec![] };
                with(|s| {
                    s.nodes = nodes;
                    s.loaded = true;
                    if let Some(e) = s.edit.clone() { if !s.nodes.iter().any(|n| n.id == e) { s.edit = None; } }
                });
                if with(|s| s.edit.is_none()) { mg(|g| g.set_edit_on(false)); }
                render();
                refresh_detail();
            }
        }
    });
}

/// POST /api/mindmap; a refusal says so in a toast (mindmapPost).
fn post(payload: Value, then: impl FnOnce(Value) + 'static) {
    let id = super::park(then);
    kit::call(move |api| api.post("/api/mindmap", &payload), move |r| {
        let v = match r {
            Ok(v) => { if !kit::truthy(&v["ok"]) { let e = kit::s(&v, "error"); kit::toast_error(if e.is_empty() { "Action failed" } else { &e }); } v }
            Err(e) => { kit::toast_error(&format!("Action failed: {e}")); json!({ "ok": false }) }
        };
        super::unpark(id, v);
    });
}

// ─── Drawing ───

fn project_name(alias: &str) -> String {
    kit::projects().iter().find(|p| kit::s(p, "alias") == alias).map(|p| kit::s(p, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| alias.to_string())
}

/// An idea's chip (the web's nodeHtml): its link, its project, or the
/// project it is in through its parent.
fn chip(n: &Node) -> (String, String, String, i32) {
    let i = &n.info;
    if n.linked {
        if !i.is_null() && !kit::truthy(&i["missing"]) {
            if kit::s(i, "kind") == "todo" {
                let t = [kit::s(i, "text"), kit::s(i, "taskTitle"), "subtask".into()].into_iter().find(|x| !x.is_empty()).unwrap_or_default();
                return ("link".into(), tree::trunc(&t, 20), String::new(), if kit::truthy(&i["done"]) { 2 } else { 1 });
            }
            let t = [kit::s(i, "title"), kit::s(i, "project"), "task".into()].into_iter().find(|x| !x.is_empty()).unwrap_or_default();
            return ("link".into(), tree::trunc(&t, 20), String::new(), 0);
        }
        return ("missing".into(), "removed".into(), String::new(), 0);
    }
    if !n.project.is_empty() { return ("scope".into(), tree::trunc(&project_name(&n.project), 18), String::new(), 0); }
    if !n.eff.is_empty() {
        let nm = project_name(&n.eff);
        return ("inherited".into(), format!("\u{21b3} {}", tree::trunc(&nm, 16)), format!("In {nm} (inherited from parent)"), 0);
    }
    (String::new(), String::new(), String::new(), 0)
}

fn row_of(p: &tree::Placed, s: &M, q: &str) -> PlanningMmNode {
    let n = s.nodes.iter().find(|n| n.id == p.id);
    let kind = match p.kind { Kind::Node => "node", Kind::Ghost => "ghost", Kind::Draft => "draft", Kind::Drop => "drop" };
    let mut r = PlanningMmNode {
        id: p.id.clone().into(), kind: kind.into(), parent: p.parent.clone().unwrap_or_default().into(),
        x: p.x, y: p.y, h: p.h, root: p.root, kids: p.kids, collapsed: s.collapsed.contains(&p.id),
        hidden: p.kind == Kind::Ghost && !q.is_empty(),
        ..Default::default()
    };
    if let Some(n) = n {
        let (c, ct, tip, todo) = chip(n);
        r.text = n.text.clone().into();
        r.linked = n.linked;
        r.scoped = !n.linked && !n.project.is_empty();
        r.pending = n.pending;
        r.chip = c.into(); r.chip_text = ct.into(); r.chip_tip = tip.into(); r.todo = todo;
        r.notes = n.notes.len() as i32;
        if !q.is_empty() { let hit = tree::matches(n, q); r.hit = hit; r.dim = !hit; }
        r.lifted = mg_get(|g| g.get_drag_id().to_string()) == p.id;
        r.target = matches!(&s.target, Some(Target::Child(t)) if *t == p.id);
        r.flash = s.flash == p.id;
    }
    r
}

/// Lay the map out again and draw it where it changed.
fn render() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningMindmap>();
    let d = dims();
    let (rows, chips, lay) = with(|s| {
        let mut collapsed = std::mem::take(&mut s.collapsed);
        let mut lay = tree::layout(&s.nodes, &mut collapsed, &s.hidden, s.draft.as_ref(), s.drop.as_ref(), &s.heights, &d);
        if lay.draft_lost {
            s.draft = None;
            lay = tree::layout(&s.nodes, &mut collapsed, &s.hidden, None, s.drop.as_ref(), &s.heights, &d);
        }
        s.collapsed = collapsed;
        let q = s.search.trim().to_lowercase();
        let rows: Vec<PlanningMmNode> = lay.items.iter().map(|p| row_of(p, s, &q)).collect();
        let keys = tree::project_keys(&s.nodes);
        let chips: Vec<PlanningMmChip> = keys.iter().map(|k| PlanningMmChip { key: k.clone().into(), label: if k.is_empty() { "General".into() } else { project_name(k).into() }, off: s.hidden.contains(k) }).collect();
        s.lay = lay.clone();
        (rows, chips, lay)
    });
    let empty = with(|s| s.nodes.is_empty() && s.draft.is_none());
    let loaded = with(|s| s.loaded);
    if loaded { g.set_state(if empty { "empty" } else { "ok" }.into()); }
    g.set_bar(!chips.is_empty());
    g.set_hidden_count(chips.iter().filter(|c| c.off).count() as i32);
    if let Some(m) = with(|s| s.m_chips.clone()) { kit::sync_rows(&m, chips); }
    if let Some(m) = with(|s| s.m_nodes.clone()) { kit::sync_rows(&m, rows.clone()); }
    g.set_edges(if lay.edges.is_empty() { "M 0 0".into() } else { lay.edges.clone().into() });
    g.set_dashes(if lay.dashes.is_empty() { "M 0 0".into() } else { lay.dashes.clone().into() });
    g.set_base_w(lay.w);
    g.set_base_h(lay.h);
    // The first time it shows: fit it to the canvas's height.
    if !with(|s| s.zoom_init) && g.get_view_h() > 0.0 && !empty && with(|s| !s.heights.is_empty()) {
        let avail = g.get_view_h() - d.gap;
        g.set_zoom(if lay.h > avail { (avail / lay.h).max(0.25) } else { 1.0 });
        with(|s| s.zoom_init = true);
    }
    // The box being typed in, where its idea will sit.
    match lay.get(tree::DRAFT) {
        Some(p) => { g.set_draft_x(p.x); g.set_draft_y(p.y); if !g.get_draft_on() { g.set_draft_text("".into()); g.set_draft_on(true); } }
        None => { if g.get_draft_on() { g.set_draft_on(false); focus_canvas(); } }
    }
    place_ring();
    // The ideas opened up follow their place.
    let find = |id: &str| rows.iter().find(|r| r.id == id && r.kind == "node").cloned();
    if g.get_hover_on() { match find(g.get_hover_node().id.as_str()) { Some(r) => g.set_hover_node(r), None => g.set_hover_on(false) } }
    if let Some(e) = with(|s| s.edit.clone()) { match find(&e) { Some(r) => g.set_edit_node(r), None => { with(|s| s.edit = None); g.set_edit_on(false); } } }
}

/// The ring on the idea the keyboard is on.
fn place_ring() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningMindmap>();
    let p = with(|s| if s.sel.is_empty() { None } else { s.lay.get(&s.sel).filter(|p| p.kind == Kind::Node).cloned() });
    match p {
        Some(p) => { g.set_ring_x(p.x); g.set_ring_y(p.y); g.set_ring_h(p.h); g.set_ring_on(true); }
        None => { g.set_ring_on(false); g.set_ring_warn(false); }
    }
}

/// An idea's height as drawn: a change lays the map out again (once, after
/// this round of reports).
fn measured(id: &str, h: f32) {
    let z = mg_get(|g| g.get_zoom()).max(0.1);
    let changed = with(|s| {
        let old = s.heights.get(id).copied().unwrap_or(-1.0);
        if (old - h).abs() <= 0.5 / z + 0.25 { return false; }
        s.heights.insert(id.to_string(), h);
        if s.relayout { return false; }
        s.relayout = true;
        true
    });
    if changed {
        slint::Timer::single_shot(Duration::ZERO, || { with(|s| s.relayout = false); render(); });
    }
}

// ─── The toolbar, the chips, the filter ───

pub fn act(a: &str, id: &str) {
    match a {
        "add-root" => start_draft(None, None),
        "media" => media_modal("Media \u{00b7} Mindmap", json!({ "scope": "project", "kind": "mindmap" })),
        "help" => open_help(),
        "toggle" => {
            with(|s| if !s.collapsed.remove(id) { s.collapsed.insert(id.to_string()); });
            render();
        }
        "ghost" => start_draft(Some(id.to_string()), None),
        "chip" => { with(|s| if !s.hidden.remove(id) { s.hidden.insert(id.to_string()); }); render(); }
        "chip-all" => { with(|s| s.hidden.clear()); render(); }
        _ => {}
    }
}

/// The filter: starting one opens every folded idea (so matches show);
/// clearing it folds them back as they were.
fn set_search(q: &str) {
    with(|s| {
        let had = !s.search.trim().is_empty();
        let has = !q.trim().is_empty();
        s.search = q.to_string();
        if has && !had { s.saved_collapsed = Some(std::mem::take(&mut s.collapsed)); }
        else if !has && had { if let Some(c) = s.saved_collapsed.take() { s.collapsed.extend(c); } }
    });
    mg(|g| if g.get_search().as_str() != q { g.set_search(q.into()); });
    render();
}

// ─── The ring and the keyboard ───

fn select(id: &str, follow: bool) {
    with(|s| { if !s.del_armed.is_empty() && s.del_armed != id { disarm(s); } s.sel = id.to_string(); if !id.is_empty() { s.anchor = id.to_string(); } });
    if with(|s| s.del_armed.is_empty()) { mg(|g| g.set_ring_warn(false)); }
    place_ring();
    if with(|s| s.draft.is_none() && s.edit.is_none()) { focus_canvas(); }
    if follow { scroll_to(id); }
}

/// Bring an idea comfortably into view, and only as far as needed.
fn scroll_to(id: &str) {
    let Some(p) = with(|s| s.lay.get(id).cloned()) else { return };
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningMindmap>();
    let (z, m, nw) = (g.get_zoom(), 70.0, dims().node);
    let (vw, vh) = (g.get_view_w(), g.get_view_h());
    if vw <= 0.0 || vh <= 0.0 { return; }
    let (x0, x1) = (p.x * z, p.x * z + nw * z);
    let (yc, hh) = (p.y * z, p.h * z / 2.0);
    let (mut sl, mut st) = (-g.get_scroll_x(), -g.get_scroll_y());
    if x0 - m < sl { sl = x0 - m; } else if x1 + m > sl + vw { sl = x1 + m - vw; }
    if yc - hh - m < st { st = yc - hh - m; } else if yc + hh + m > st + vh { st = yc + hh + m - vh; }
    set_scroll(sl, st);
}

fn set_scroll(sl: f32, st: f32) {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningMindmap>();
    let z = g.get_zoom();
    let cw = g.get_view_w().max(g.get_base_w() * z);
    let ch = g.get_view_h().max(g.get_base_h() * z);
    g.set_scroll_x(-sl.clamp(0.0, (cw - g.get_view_w()).max(0.0)));
    g.set_scroll_y(-st.clamp(0.0, (ch - g.get_view_h()).max(0.0)));
}

fn clear_sel() {
    with(|s| if !s.sel.is_empty() { s.anchor = std::mem::take(&mut s.sel); });
    place_ring();
}

fn disarm(s: &mut M) { s.del_armed.clear(); s.del_gen += 1; }

fn disarm_now() {
    with(disarm);
    mg(|g| { g.set_ring_warn(false); g.set_ring_msg("".into()); });
}

/// Delete with the ring on an idea: the first press warns, a second while
/// it is amber deletes it and everything under it.
fn delete_key() {
    let id = with(|s| s.sel.clone());
    let Some(n) = with(|s| s.nodes.iter().find(|x| x.id == id).cloned()) else { return };
    if n.pending || with(|s| s.lay.get(&id).is_none()) { return; }
    let gone = with(|s| tree::subtree(&s.nodes, &id));
    if with(|s| s.del_armed != id) {
        disarm_now();
        let gen = with(|s| { s.del_armed = id.clone(); s.del_gen += 1; s.del_gen });
        let under = gone.len() - 1;
        let msg = format!("Press Delete again to delete{}", if under > 0 { format!(" it and the {under}{} under it", if under == 1 { " idea" } else { " ideas" }) } else { String::new() });
        mg(|g| { g.set_ring_msg(msg.into()); g.set_ring_warn(true); });
        slint::Timer::single_shot(Duration::from_secs(3), move || { if with(|s| s.del_gen == gen) { disarm_now(); } });
        return;
    }
    disarm_now();
    // Where the ring goes next: below on this level, else above, else the parent.
    let next = with(|s| {
        let st = |d: &str| tree::step(&s.nodes, &s.hidden, &s.collapsed, &s.lay, &id, d).0;
        let n2 = st("down");
        if !n2.is_empty() { n2 } else { let u = st("up"); if !u.is_empty() { u } else { n.parent.clone().unwrap_or_default() } }
    });
    with(|s| { s.nodes.retain(|x| !gone.contains(&x.id)); s.sel = if gone.contains(&next) { String::new() } else { next.clone() }; });
    render();
    let sel = with(|s| s.sel.clone());
    select(&sel, true);
    post(json!({ "action": "deleteNode", "id": id }), |_| load());
}

fn dir_of(t: &str) -> Option<&'static str> {
    match t { "\u{F700}" => Some("up"), "\u{F701}" => Some("down"), "\u{F702}" => Some("left"), "\u{F703}" => Some("right"), _ => None }
}

const RET: &str = "\n";
const TAB: &str = "\t";
const BACKTAB: &str = "\u{19}";
const ESC: &str = "\u{1b}";
const DEL: &str = "\u{7f}";

/// Ctrl+Shift and an arrow: walk the tree. An idea being edited is saved
/// and the editing moves along; a new idea being typed is kept if it has
/// words, then the walk goes on from it.
fn mm_step(dir: &str) {
    let mut from = with(|s| if !s.sel.is_empty() { s.sel.clone() } else if s.lay.get(&s.anchor).is_some() { s.anchor.clone() } else { String::new() });
    let mut was_editing = false;
    if let Some(e) = with(|s| s.edit.clone()) { from = e; was_editing = true; finish_edit(true); }
    else if let Some(dr) = with(|s| s.draft.clone()) {
        let text = mg_get(|g| g.get_draft_text().to_string());
        let origin = dr.after.clone().or(dr.parent.clone()).unwrap_or_default();
        if !text.trim().is_empty() { draft_commit("next"); from = with(|s| s.draft.as_ref().and_then(|d| d.after.clone())).unwrap_or(origin); } else { from = origin; }
        end_draft();
        was_editing = true;
    }
    let (to, unfold) = with(|s| tree::step(&s.nodes, &s.hidden, &s.collapsed, &s.lay, &from, dir));
    if unfold { with(|s| s.collapsed.remove(&from)); render(); }
    let at = if to.is_empty() { from } else { to };
    select(&at, true);
    if was_editing && !at.is_empty() { start_edit(&at); }
}

fn is_plus(t: &str) -> bool { t == "+" || t == "=" }
fn is_minus(t: &str) -> bool { t == "-" || t == "_" || t == "\u{2212}" }

/// Ctrl and + or −: zoom about the middle of the canvas.
fn zoom_step(plus: bool) {
    let (z, vw, vh) = mg_get(|g| (g.get_zoom(), g.get_view_w(), g.get_view_h()));
    set_zoom(z * if plus { 1.2 } else { 1.0 / 1.2 }, vw / 2.0, vh / 2.0);
}

/// The map's keys with no text box open (and those the boxes pass on).
fn key(t: &str, ctrl: bool, shift: bool, alt: bool) -> bool {
    if ctrl && shift && !alt { if let Some(d) = dir_of(t) { mm_step(d); return true; } }
    let (edit, draft, sel) = with(|s| (s.edit.is_some(), s.draft.is_some(), s.sel.clone()));
    if t == DEL && !alt && !ctrl && !edit && !draft && !sel.is_empty() { delete_key(); return true; }
    let modifier = t == "\u{10}" || t == "\u{11}" || t == "\u{12}" || t == "\u{13}" || t == "\u{14}" || t == "\u{15}" || t == "\u{16}" || t == "\u{17}";
    if !with(|s| s.del_armed.is_empty()) && !modifier { disarm_now(); }
    if t == ESC && !edit && !draft && !sel.is_empty() { clear_sel(); return true; }
    if ctrl && !alt && (is_plus(t) || is_minus(t)) { zoom_step(is_plus(t)); return true; }
    if edit || draft || sel.is_empty() || alt { return false; }
    let Some(n) = with(|s| s.nodes.iter().find(|x| x.id == sel).cloned()) else { return false };
    if n.pending || with(|s| s.lay.get(&sel).is_none()) { return false; }
    if t == RET && ctrl { start_draft(n.parent.clone(), Some(n.id.clone())); return true; }
    if ctrl { return false; }
    if t == RET && !shift { start_edit(&n.id); return true; }
    if t == TAB || t == BACKTAB { tab_from(&n, t == BACKTAB); return true; }
    if is_plus(t) || is_minus(t) {
        let has_kids = with(|s| s.nodes.iter().any(|x| x.parent.as_deref() == Some(n.id.as_str())));
        let minus = is_minus(t);
        if !has_kids || with(|s| s.collapsed.contains(&n.id)) == minus { return false; }
        with(|s| if minus { s.collapsed.insert(n.id.clone()); } else { s.collapsed.remove(&n.id); });
        render();
        return true;
    }
    false
}

/// Tab: a new idea nested under this one; Shift+Tab: one at its parent's
/// level, after the parent (from the top: a new top-level idea).
fn tab_from(n: &Node, back: bool) {
    let par = with(|s| n.parent.as_ref().and_then(|p| s.nodes.iter().find(|x| &x.id == p).cloned()));
    if !back { start_draft(Some(n.id.clone()), None); }
    else if let Some(p) = par { start_draft(p.parent.clone(), Some(p.id.clone())); }
    else { start_draft(None, Some(n.id.clone())); }
}

// ─── Adding ideas in place ───

fn start_draft(parent: Option<String>, after: Option<String>) {
    if with(|s| s.edit.is_some()) { finish_edit(true); }
    with(|s| {
        if !s.sel.is_empty() { s.anchor = std::mem::take(&mut s.sel); }
        if let Some(a) = after.clone().or(parent.clone()) { s.anchor = a; }
        s.draft = Some(Draft { parent, after });
        s.draft_gen += 1;
    });
    mg(|g| { g.set_draft_text("".into()); g.set_focus_gen(g.get_focus_gen() + 1); g.set_hover_on(false); });
    render();
}

fn end_draft() {
    if with(|s| s.draft.take()).is_none() { return; }
    render();
}

fn draft_key(t: &str, ctrl: bool, shift: bool, alt: bool) -> bool {
    if ctrl && shift && !alt { if let Some(d) = dir_of(t) { mm_step(d); return true; } }
    if t == RET && ctrl { draft_commit("next"); return true; }
    if t == RET && !shift {
        // Save this one and stop adding, the ring left on it.
        let had = !mg_get(|g| g.get_draft_text().to_string()).trim().is_empty();
        if had { draft_commit("next"); }
        let saved = if had { with(|s| s.draft.as_ref().and_then(|d| d.after.clone())) } else { None };
        end_draft();
        if let Some(sv) = saved { select(&sv, false); }
        return true;
    }
    if t == TAB { draft_commit("nested"); return true; }
    if t == BACKTAB { draft_commit("up"); return true; }
    if t == ESC { end_draft(); return true; }
    false
}

/// Keep what was typed: shown at once (pending), sent in the order typed
/// (the next idea may hang off one whose id is not known yet); then the box
/// moves on: the next at this level, nested under it, or one level out.
fn draft_commit(mode: &str) {
    let Some(dr) = with(|s| s.draft.clone()) else { return };
    let text = mg_get(|g| g.get_draft_text().to_string()).trim().to_string();
    let parent_node = with(|s| dr.parent.as_ref().and_then(|p| s.nodes.iter().find(|n| &n.id == p).cloned()));
    if text.is_empty() {
        // Nothing typed: Shift+Tab still steps out; anything else closes.
        if mode == "up" { if let Some(p) = parent_node { with(|s| s.draft = Some(Draft { parent: p.parent.clone(), after: Some(p.id.clone()) })); render(); return; } }
        end_draft();
        return;
    }
    let tmp = with(|s| { s.tmp_seq += 1; format!("tmp{}{}", std::process::id(), s.tmp_seq) });
    let local = Node { id: tmp.clone(), text: text.clone(), parent: dr.parent.clone(), pending: true, eff: parent_node.as_ref().map(|p| p.eff.clone()).unwrap_or_default(), ..Default::default() };
    with(|s| {
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
    });
    mg(|g| { g.set_draft_text("".into()); g.set_focus_gen(g.get_focus_gen() + 1); });
    render();
    pump();
}

/// Send the next idea in the queue, one at a time.
fn pump() {
    if with(|s| s.busy) { return; }
    let Some(sv) = with(|s| { let x = s.queue.pop_front(); if x.is_some() { s.busy = true; } x }) else { return };
    let real = |id: &Option<String>| with(|s| id.as_ref().map(|i| s.real_id.get(i).cloned().unwrap_or_else(|| i.clone())));
    let parent = real(&sv.parent);
    let mut body = json!({ "action": "addNode", "text": sv.text });
    if let Some(p) = &parent { body["parentId"] = json!(p); }
    post(body, move |d| {
        let new_id = if kit::truthy(&d["ok"]) { kit::s(&d["node"], "id") } else { String::new() };
        if new_id.is_empty() {
            // Not saved (the toast said why): off the map, with anything hung under it.
            with(|s| {
                let gone = tree::subtree(&s.nodes, &sv.tmp);
                s.nodes.retain(|n| !gone.contains(&n.id));
                s.queue.retain(|q| !gone.contains(&q.tmp));
                if let Some(dr) = &s.draft { if dr.parent.as_ref().is_some_and(|p| gone.contains(p)) || dr.after.as_ref().is_some_and(|a| gone.contains(a)) { s.draft = None; } }
            });
            saved_one();
            return;
        }
        // A new idea is added at the end of its level: if it belongs earlier, move it there.
        let (at, last) = with(|s| {
            let sibs: Vec<&Node> = s.nodes.iter().filter(|n| n.parent == sv.parent).collect();
            (sibs.iter().position(|n| n.id == sv.tmp), sibs.len())
        });
        let tmp = sv.tmp.clone();
        let finish = move || {
            with(|s| {
                s.real_id.insert(tmp.clone(), new_id.clone());
                for n in s.nodes.iter_mut() {
                    if n.id == tmp { n.id = new_id.clone(); n.pending = false; }
                    if n.parent.as_deref() == Some(tmp.as_str()) { n.parent = Some(new_id.clone()); }
                }
                if let Some(dr) = s.draft.as_mut() {
                    if dr.parent.as_deref() == Some(tmp.as_str()) { dr.parent = Some(new_id.clone()); }
                    if dr.after.as_deref() == Some(tmp.as_str()) { dr.after = Some(new_id.clone()); }
                }
                for q in s.queue.iter_mut() { if q.parent.as_deref() == Some(tmp.as_str()) { q.parent = Some(new_id.clone()); } }
                if s.sel == tmp { s.sel = new_id.clone(); }
                if s.anchor == tmp { s.anchor = new_id.clone(); }
                if let Some(h) = s.heights.remove(&tmp) { s.heights.insert(new_id.clone(), h); }
            });
            saved_one();
        };
        match at {
            Some(i) if i + 1 < last => {
                let mut mv = json!({ "action": "moveNode", "id": kit::s(&d["node"], "id"), "index": i });
                mv["parentId"] = parent.map(Value::String).unwrap_or(Value::Null);
                post(mv, move |_| finish());
            }
            _ => finish(),
        }
    });
}

fn saved_one() {
    let done = with(|s| { s.busy = false; s.saving = s.saving.saturating_sub(1); s.saving == 0 });
    render();
    if done { with(|s| s.reload_wanted = false); load(); } else { pump(); }
}

// ─── Editing an idea in place ───

fn start_edit(id: &str) {
    if with(|s| s.edit.is_some()) { finish_edit(true); }
    let Some(n) = with(|s| s.nodes.iter().find(|x| x.id == id).cloned()) else { return };
    if n.pending { return; }
    with(|s| { s.draft = None; s.sel = id.to_string(); s.anchor = id.to_string(); s.edit = Some(id.to_string()); s.edit_gen += 1; });
    mg(|g| { g.set_edit_text(n.text.clone().into()); g.set_hover_on(false); });
    render();
    mg(|g| { g.set_edit_on(true); g.set_focus_gen(g.get_focus_gen() + 1); });
}

/// Save (or not) the idea being edited; Escape puts the old words back.
fn finish_edit(save: bool) {
    let Some(id) = with(|s| s.edit.take()) else { return };
    let text = mg_get(|g| g.get_edit_text().to_string()).trim().to_string();
    mg(|g| g.set_edit_on(false));
    focus_canvas();
    let cur = with(|s| s.nodes.iter().find(|n| n.id == id).map(|n| n.text.clone()));
    if !save || cur.is_none() || text.is_empty() || cur.as_deref() == Some(text.as_str()) { render(); return; }
    with(|s| if let Some(n) = s.nodes.iter_mut().find(|n| n.id == id) { n.text = text.clone(); });
    render();
    post(json!({ "action": "updateNode", "id": id, "text": text }), |_| load());
}

fn edit_key(t: &str, ctrl: bool, shift: bool, alt: bool) -> bool {
    if ctrl && shift && !alt { if let Some(d) = dir_of(t) { mm_step(d); return true; } }
    let Some(id) = with(|s| s.edit.clone()) else { return false };
    let Some(n) = with(|s| s.nodes.iter().find(|x| x.id == id).cloned()) else { return false };
    if t == RET && ctrl { finish_edit(true); start_draft(n.parent.clone(), Some(n.id.clone())); return true; }
    if t == RET && !shift { finish_edit(true); select(&id, false); return true; }
    if t == ESC { finish_edit(false); return true; }
    if t == TAB || t == BACKTAB { finish_edit(true); tab_from(&n, t == BACKTAB); return true; }
    false
}

// ─── Pointer ───

fn canvas_press() {
    disarm_now();
    if with(|s| s.draft.is_some()) { end_draft(); }
    if with(|s| s.edit.is_some()) { finish_edit(true); }
    clear_sel();
}

/// One click edits an idea in place, two open its details (the edit waits
/// a moment for a second click).
fn node_click(id: &str) {
    if with(|s| s.last_drag.is_some_and(|t| t.elapsed() <= Duration::from_millis(300))) { return; }
    if with(|s| s.edit.as_deref() == Some(id)) { return; }
    disarm_now();
    if with(|s| s.draft.is_some()) { end_draft(); }
    let now = Instant::now();
    let second = with(|s| s.click_id == id && s.click_at.is_some_and(|t| now.duration_since(t) < Duration::from_millis(350)));
    if second {
        with(|s| { s.click_id.clear(); s.click_gen += 1; });
        if with(|s| s.edit.is_some()) { finish_edit(true); }
        open_detail(id);
        return;
    }
    let gen = with(|s| { s.click_id = id.to_string(); s.click_at = Some(now); s.click_gen += 1; s.click_gen });
    let id = id.to_string();
    slint::Timer::single_shot(Duration::from_millis(240), move || {
        if with(|s| s.click_gen != gen) { return; }
        with(|s| s.click_id.clear());
        if with(|s| s.lay.get(&id).is_some()) { start_edit(&id); }
    });
}

fn hover(id: &str, on: bool) {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningMindmap>();
    if !g.get_drag_id().is_empty() { return; }
    if on {
        let r = g.get_nodes().iter().find(|r| r.id == id && r.kind == "node");
        if let Some(r) = r { g.set_hover_node(r); g.set_hover_on(true); }
    } else if id.is_empty() || g.get_hover_node().id == id {
        g.set_hover_on(false);
    }
}

/// Ctrl and the wheel: zoom about the pointer (`delta` Slint's: up is +).
fn wheel_zoom(delta: f32, px: f32, py: f32) {
    let dy = -delta;
    let k = if dy.abs() < 40.0 { 0.006 } else { 0.0012 };
    let step = (-dy * k).clamp(-0.2, 0.2);
    let z = mg_get(|g| g.get_zoom());
    set_zoom(z * step.exp(), px, py);
}

/// Zoom to `z` keeping the point (cx, cy) of the canvas's view still.
fn set_zoom(z: f32, cx: f32, cy: f32) {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningMindmap>();
    let prev = g.get_zoom().max(0.01);
    let z = z.clamp(0.15, 3.0);
    let (ox, oy) = (cx - g.get_scroll_x(), cy - g.get_scroll_y());
    let r = z / prev;
    g.set_zoom(z);
    with(|s| s.zoom_init = true);
    set_scroll(ox * r - cx, oy * r - cy);
}

// ─── Drag to rearrange ───

fn drag_start(id: &str) {
    if with(|s| s.edit.is_some()) { finish_edit(true); }
    if with(|s| s.draft.is_some()) { end_draft(); }
    clear_sel();
    with(|s| { s.target = None; s.want.clear(); s.want_gen += 1; });
    mg(|g| g.set_drag_id(id.into()));
    render();
}

fn drop_key(t: &Option<(Option<String>, usize)>) -> String { t.as_ref().map(|(p, i)| format!("{}#{i}", p.clone().unwrap_or_default())).unwrap_or_default() }

fn drag_move(x: f32, y: f32) {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningMindmap>();
    g.set_drag_x(x);
    g.set_drag_y(y);
    let id = g.get_drag_id().to_string();
    if id.is_empty() { return; }
    let nw = dims().node;
    let t = with(|s| tree::drop_target(&s.nodes, &s.lay, nw, &id, x, y));
    let (target, show) = match t {
        Target::Stay => return,
        Target::Back => (None, None),
        Target::Sib(p, i) => (Some(Target::Sib(p.clone(), i)), Some((p, i))),
        other => (Some(other), None),
    };
    let outline_changed = with(|s| {
        let was = matches!(&s.target, Some(Target::Child(_))).then(|| s.target.clone());
        s.target = target.clone();
        was != matches!(&target, Some(Target::Child(_))).then(|| target.clone())
    });
    if outline_changed { render(); }
    // The gap opens once a new place has been held for a moment.
    let k = drop_key(&show);
    if with(|s| s.want == k) { return; }
    let gen = with(|s| { s.want = k.clone(); s.want_gen += 1; s.want_gen });
    slint::Timer::single_shot(Duration::from_millis(70), move || {
        if with(|s| s.want_gen != gen) || mg_get(|g| g.get_drag_id().is_empty()) { return; }
        let now = with(|s| drop_key(&s.drop.as_ref().map(|d| (d.parent.clone(), d.index))));
        if now == k { return; }
        with(|s| {
            let h = s.heights.get(&id).copied().unwrap_or(40.0);
            s.drop = show.map(|(p, i)| DropAt { parent: p, index: i, drag: id.clone(), h });
        });
        render();
    });
}

fn drag_end(commit: bool) {
    let id = mg_get(|g| g.get_drag_id().to_string());
    mg(|g| g.set_drag_id("".into()));
    let t = with(|s| { s.last_drag = Some(Instant::now()); s.want.clear(); s.want_gen += 1; s.drop = None; s.target.take() });
    let Some(t) = t.filter(|_| commit && !id.is_empty()) else { render(); return };
    // Put it there at once, then tell the server and take its word.
    let (parent, index) = match &t { Target::Root => (None, None), Target::Child(p) => (Some(p.clone()), None), Target::Sib(p, i) => (p.clone(), Some(*i)), _ => { render(); return; } };
    with(|s| {
        if tree::apply_move(&mut s.nodes, &id, parent.as_deref(), index) { if let Some(p) = &parent { s.collapsed.remove(p); } }
    });
    render();
    let mut body = json!({ "action": "moveNode", "id": id, "parentId": parent });
    if let Some(i) = index { body["index"] = json!(i); }
    post(body, |_| load());
}

// ─── Media (the Media modal and viewer are workspace's) ───

fn media_modal(title: &str, state: Value) {
    if !super::media_bridge::open_modal(title, &state) { kit::toast("Media opens in the Media tab"); kit::show_tab("media"); }
}

fn open_media(id: &str) {
    let it = with(|s| s.d_media.iter().find(|x| kit::s(x, "id") == id).cloned());
    if let Some(it) = it { if !super::media_bridge::open_item(&it) { kit::toast("Media opens in the Media tab"); kit::show_tab("media"); } }
}

// ─── The idea dialog ───

fn open_detail(id: &str) {
    if with(|s| !s.nodes.iter().any(|n| n.id == id)) { return; }
    with(|s| { s.detail = Some(id.to_string()); s.d_media.clear(); });
    mg(|g| { g.set_d_editing(false); g.set_d_open(true); g.set_hover_on(false); });
    if let Some(m) = with(|s| s.m_thumbs.clone()) { kit::sync_rows(&m, vec![]); }
    modal(true);
    refresh_detail();
    load_detail_media();
}

/// The dialog's view of its idea, from the latest map.
fn refresh_detail() {
    let Some(id) = with(|s| s.detail.clone()) else { return };
    let Some(n) = with(|s| s.nodes.iter().find(|n| n.id == id).cloned()) else { return };
    let Some(w) = kit::ui() else { return };
    let g = w.global::<PlanningMindmap>();
    g.set_d_title(if n.text.is_empty() { "Idea".into() } else { n.text.clone().into() });
    let i = &n.info;
    let (kind, main, text, st, todo) = if !n.linked {
        if !n.project.is_empty() { ("scope", format!("Scoped to project \u{201c}{}\u{201d}", project_name(&n.project)), String::new(), "(no task linked)".to_string(), 0) }
        else { ("", String::new(), String::new(), String::new(), 0) }
    } else if !i.is_null() && !kit::truthy(&i["missing"]) {
        if kit::s(i, "kind") == "todo" { ("link", "Subtask".to_string(), kit::s(i, "text"), format!("\u{00b7} in {}", kit::s(i, "taskTitle")), if kit::truthy(&i["done"]) { 2 } else { 1 }) }
        else { ("link", format!("Kanban: {} \u{00b7} {}", kit::s(i, "project"), kit::s(i, "title")), String::new(), format!("[{}{}]", kit::s(i, "status").replacen('_', " ", 1), if kit::truthy(&i["deleted"]) { ", deleted" } else { "" }), 0) }
    } else { ("missing", "Linked item no longer exists".to_string(), String::new(), String::new(), 0) };
    g.set_d_link_kind(kind.into());
    g.set_d_link_main(main.into());
    g.set_d_link_text(text.into());
    g.set_d_link_st(st.into());
    g.set_d_link_todo(todo);
    g.set_d_notes(ModelRc::new(VecModel::from(n.notes.iter().map(|t| SharedString::from(t.as_str())).collect::<Vec<_>>())));
    g.set_d_link_label(if n.linked || !n.project.is_empty() { "Unlink".into() } else { "Link / scope".into() });
}

/// The strip of media hung on the idea (loadMmDetailMedia): ten at most.
fn load_detail_media() {
    let Some(id) = with(|s| s.detail.clone()) else { return };
    let p = kit::project();
    let path = format!("/api/media/list?{}linkType=node&nodeId={}", if p.is_empty() { String::new() } else { format!("project={}&includeGeneral=1&", kit::enc(&p)) }, kit::enc(&id));
    kit::call(move |api| api.get(&path), move |r| {
        if with(|s| s.detail.as_deref() != Some(id.as_str())) { return; }
        let items: Vec<Value> = r.ok().filter(|v| kit::truthy(&v["ok"])).and_then(|v| v.get("items").and_then(|x| x.as_array()).cloned()).unwrap_or_default().into_iter().take(10).collect();
        with(|s| s.d_media = items);
        fill_thumbs();
    });
}

fn fill_thumbs() {
    let items = with(|s| s.d_media.clone());
    let rows: Vec<PlanningMmThumb> = items.iter().map(|it| {
        let id = kit::s(it, "id");
        let img = with(|s| s.thumbs.get(&id).cloned().flatten());
        PlanningMmThumb { has_image: img.is_some(), image: img.unwrap_or_default(), name: kit::s(it, "originalName").into(), kind: kit::s(it, "kind").into(), id: id.into() }
    }).collect();
    if let Some(m) = with(|s| s.m_thumbs.clone()) { m.set_vec(rows); }
    // Pictures load once each, then the strip is drawn again.
    for it in items {
        let id = kit::s(&it, "id");
        if kit::s(&it, "kind") != "image" || with(|s| s.thumbs.contains_key(&id)) { continue; }
        with(|s| s.thumbs.insert(id.clone(), None));
        let Some(api) = kit::api() else { return };
        let path = format!("/api/media/raw/{}", kit::enc(&id));
        kit::bg(move || match api.get_bytes(&path) {
            Ok((200, bytes)) => image::load_from_memory(&bytes).ok().map(|img| {
                let img = img.thumbnail(96, 96).to_rgba8();
                let mut pb = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(img.width(), img.height());
                pb.make_mut_bytes().copy_from_slice(img.as_raw());
                pb
            }),
            _ => None,
        }, move |pb| {
            if let Some(pb) = pb { with(|s| s.thumbs.insert(id, Some(slint::Image::from_rgba8(pb)))); fill_thumbs(); }
        });
    }
}

fn close_detail() {
    with(|s| s.detail = None);
    mg(|g| { g.set_d_open(false); g.set_d_editing(false); });
    modal(false);
}

fn d_act(a: &str) {
    let Some(id) = with(|s| s.detail.clone()) else { return };
    match a {
        "close" => close_detail(),
        "edit" => {
            let n = with(|s| s.nodes.iter().find(|n| n.id == id).cloned()).unwrap_or_default();
            with(|s| s.d_notes = n.notes.clone());
            let en = kit::model::<SharedString>();
            en.set_vec(n.notes.iter().map(|t| SharedString::from(t.as_str())).collect::<Vec<_>>());
            mg(|g| { g.set_d_text(n.text.clone().into()); g.set_d_note_focus(-1); g.set_d_edit_notes(kit::rc(&en)); g.set_d_editing(true); });
            with(|s| s.m_edit_notes = Some(en));
        }
        "add-note" => {
            let i = with(|s| { s.d_notes.push(String::new()); s.d_notes.len() - 1 });
            mg(|g| g.set_d_note_focus(i as i32));
            if let Some(m) = with(|s| s.m_edit_notes.clone()) { m.push(SharedString::new()); }
        }
        "save" => {
            let text = mg_get(|g| g.get_d_text().to_string()).trim().to_string();
            if text.is_empty() { kit::toast_error("Idea text is required"); return; }
            let notes: Vec<String> = with(|s| s.d_notes.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect());
            post(json!({ "action": "updateNode", "id": id, "text": text, "notes": notes }), |_| { close_detail(); load(); });
        }
        "child" => { close_detail(); start_draft(Some(id), None); }
        "link" => {
            close_detail();
            let linked = with(|s| s.nodes.iter().find(|n| n.id == id).map(|n| n.linked || !n.project.is_empty())).unwrap_or(false);
            // "Unlink" clears a task link and a project scope alike.
            if linked { post(json!({ "action": "unlinkNode", "id": id }), |_| load()); } else { open_link(&id); }
        }
        "media" => media_modal("Idea media", json!({ "scope": "project", "kind": "all", "linkFilter": { "type": "node", "nodeId": id }, "uploadLink": { "type": "node", "nodeId": id } })),
        "delete" => {
            close_detail();
            kit::confirm(kit::Confirm { title: "", message: "Delete this idea and all its sub-ideas?", ok: "Delete", danger: true }, move |yes| {
                if yes { post(json!({ "action": "deleteNode", "id": id }), |_| load()); }
            });
        }
        _ => {}
    }
}

// ─── The link picker ───

fn open_link(id: &str) {
    let (node, parent) = with(|s| {
        let n = s.nodes.iter().find(|n| n.id == id).cloned();
        let p = n.as_ref().and_then(|n| n.parent.as_ref()).and_then(|p| s.nodes.iter().find(|x| &x.id == p).cloned());
        (n, p)
    });
    if node.is_none() { return; }
    // A child strictly keeps its parent's project.
    let locked = parent.map(|p| p.eff).unwrap_or_default();
    let mut keys: Vec<String> = vec![];
    let mut names: Vec<SharedString> = vec![];
    for p in kit::projects() {
        let a = kit::s(&p, "alias");
        let nm = kit::s(&p, "name");
        names.push(if nm.is_empty() { a.clone().into() } else { nm.into() });
        keys.push(a);
    }
    if !locked.is_empty() && !keys.contains(&locked) { keys.insert(0, locked.clone()); names.insert(0, locked.clone().into()); }
    let cur = kit::project();
    let sel = if !locked.is_empty() { keys.iter().position(|k| *k == locked).unwrap_or(0) } else { keys.iter().position(|k| *k == cur).unwrap_or(0) };
    with(|s| { s.link_id = id.to_string(); s.l_keys = keys; s.l_board = Value::Null; });
    mg(|g| {
        g.set_l_names(ModelRc::new(VecModel::from(names)));
        g.set_l_sel(sel as i32);
        g.set_l_locked(!locked.is_empty());
        g.set_l_locked_text(format!("Scoped to \u{201c}{}\u{201d} (inherited from parent) \u{2014} can only link tasks in this project.", project_name(&locked)).into());
        g.set_l_search("".into());
        g.set_l_open(true);
    });
    modal(true);
    load_link_board();
}

fn l_project() -> String { let i = mg_get(|g| g.get_l_sel()); with(|s| s.l_keys.get(i.max(0) as usize).cloned()).unwrap_or_default() }

fn load_link_board() {
    let proj = l_project();
    mg(|g| { g.set_l_scope_label(format!("Scope to \u{201c}{}\u{201d} (no task)", project_name(&proj)).into()); g.set_l_loading(true); });
    let gen = with(|s| { s.l_gen += 1; s.l_gen });
    let path = format!("/api/kanban?project={}", kit::enc(&proj));
    kit::call(move |api| api.get(&path), move |r| {
        if with(|s| s.l_gen != gen) { return; }
        let b = r.ok().filter(|v| kit::truthy(&v["ok"])).map(|v| v["board"].clone()).unwrap_or(json!({ "tasks": [] }));
        with(|s| s.l_board = b);
        mg(|g| g.set_l_loading(false));
        link_list();
    });
}

fn link_list() {
    let q = mg_get(|g| g.get_l_search().to_string()).trim().to_lowercase();
    let b = with(|s| s.l_board.clone());
    let mut rows = vec![];
    for t in b.get("tasks").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let title = kit::s(&t, "title");
        let t_match = q.is_empty() || title.to_lowercase().contains(&q);
        let todos: Vec<Value> = t.get("todos").and_then(|x| x.as_array()).cloned().unwrap_or_default().into_iter().filter(|td| !kit::truthy(&td["deleted"])).collect();
        let hits: Vec<&Value> = todos.iter().filter(|td| q.is_empty() || kit::s(td, "text").to_lowercase().contains(&q)).collect();
        if !t_match && hits.is_empty() { continue; }
        let tid = kit::s(&t, "id");
        rows.push(PlanningMmLinkRow { task: tid.clone().into(), todo: "".into(), text: title.into(), status: kit::s(&t, "status").replacen('_', " ", 1).into(), done: false });
        let shown: Vec<&Value> = if q.is_empty() { todos.iter().collect() } else { hits };
        for td in shown {
            rows.push(PlanningMmLinkRow { task: tid.clone().into(), todo: kit::s(td, "id").into(), text: kit::s(td, "text").into(), status: "".into(), done: kit::truthy(&td["done"]) });
        }
    }
    if let Some(m) = with(|s| s.m_rows.clone()) { kit::sync_rows(&m, rows); }
}

fn close_link() { mg(|g| g.set_l_open(false)); modal(false); }

fn l_act(a: &str, x: &str, y: &str) {
    let id = with(|s| s.link_id.clone());
    match a {
        "project" => { mg(|g| g.set_l_sel(x.parse().unwrap_or(0))); load_link_board(); }
        "search" => link_list(),
        "scope" => {
            let proj = l_project();
            if proj.is_empty() { return; }
            post(json!({ "action": "scopeProject", "id": id, "project": proj }), |d| { if kit::truthy(&d["ok"]) { close_link(); load(); } });
        }
        "pick" => {
            let mut body = json!({ "action": "linkNode", "id": id, "project": l_project(), "taskId": x });
            if !y.is_empty() { body["todoId"] = json!(y); }
            post(body, |d| { if kit::truthy(&d["ok"]) { close_link(); load(); } });
        }
        _ => close_link(),
    }
}

// ─── The shortcut sheet ───

fn open_help() {
    if with(|s| s.h_open.is_empty()) {
        let n = help::rows().len();
        with(|s| s.h_open = vec![false; n]);
    }
    fill_help();
    mg(|g| g.set_h_open(true));
    modal(true);
}

fn fill_help() {
    let Some(w) = kit::ui() else { return };
    let t = w.global::<crate::Theme>();
    let (cw, rh, bw, bh, pad) = (t.get_mindmap_dia_col(), t.get_mindmap_dia_row(), t.get_mindmap_dia_box_w(), t.get_mindmap_dia_box_h(), t.get_sp_6());
    let open = with(|s| s.h_open.clone());
    let rows: Vec<PlanningMmHelp> = help::rows().into_iter().enumerate().map(|(i, r)| match r {
        help::Row::Head(h) => PlanningMmHelp { head: h.into(), ..Default::default() },
        help::Row::Item { keys, text, dia } => {
            let lines: Vec<PlanningMmKeyLine> = keys.into_iter().map(|l| PlanningMmKeyLine { keys: ModelRc::new(VecModel::from(l.into_iter().map(|(k, t)| PlanningMmKey { kind: k.into(), text: t.into() }).collect::<Vec<_>>())) }).collect();
            let mut row = PlanningMmHelp { lines: ModelRc::new(VecModel::from(lines)), text: text.into(), open: open.get(i).copied().unwrap_or(false), ..Default::default() };
            if let Some(d) = dia {
                let (dw, dh, path, dash) = help::dia_paths(&d, cw, rh, bw, bh, pad);
                row.dia = true;
                row.dia_w = dw; row.dia_h = dh;
                row.path = if path.is_empty() { "M 0 0".into() } else { path.into() };
                row.dash = if dash.is_empty() { "M 0 0".into() } else { dash.into() };
                row.note = d.note.into();
                row.boxes = ModelRc::new(VecModel::from(d.boxes.iter().map(|b| PlanningMmBox { col: b.0, row: b.1, label: b.2.into(), kind: b.3.into() }).collect::<Vec<_>>()));
            }
            row
        }
    }).collect();
    if let Some(m) = with(|s| s.m_help.clone()) {
        if m.row_count() == rows.len() {
            // Only the open flags change: keep the rows as they are.
            for (i, r) in rows.into_iter().enumerate() { if m.row_data(i).map(|x| x.open) != Some(r.open) { m.set_row_data(i, r); } }
        } else { m.set_vec(rows); }
    }
}

// ─── Jumping to an idea (the web's flashMindmapNode) ───

/// Show an idea: unfold what hides it, put the ring on it and flash it;
/// `open` opens its details too. For the Media "jump to source".
#[allow(dead_code)]
pub fn flash_node(id: &str, open: bool) {
    let id = id.to_string();
    let go = move || {
        let unfold: Vec<String> = with(|s| {
            let mut out = vec![];
            let mut cur = s.nodes.iter().find(|n| n.id == id).and_then(|n| n.parent.clone());
            let mut hops = 0;
            while let Some(p) = cur { if s.collapsed.contains(&p) { out.push(p.clone()); } hops += 1; if hops > s.nodes.len() { break; } cur = s.nodes.iter().find(|n| n.id == p).and_then(|n| n.parent.clone()); }
            out
        });
        if !unfold.is_empty() { with(|s| for p in &unfold { s.collapsed.remove(p); }); }
        let gen = with(|s| { s.flash = id.clone(); s.flash_gen += 1; s.flash_gen });
        render();
        if with(|s| s.lay.get(&id).is_some()) { select(&id, true); }
        slint::Timer::single_shot(Duration::from_millis(2400), move || { if with(|s| s.flash_gen == gen) { with(|s| s.flash.clear()); render(); } });
        if open { open_detail(&id); }
    };
    if with(|s| s.loaded) { go(); } else { kit::show_tab("mindmap"); slint::Timer::single_shot(Duration::from_millis(600), go); }
}

// ─── For the test driver ───

fn row_json(r: &PlanningMmNode) -> Value {
    json!({ "id": r.id.as_str(), "kind": r.kind.as_str(), "parent": r.parent.as_str(), "x": r.x, "y": r.y, "h": r.h, "text": r.text.as_str(), "root": r.root, "linked": r.linked, "scoped": r.scoped, "pending": r.pending,
        "chip": r.chip.as_str(), "chipText": r.chip_text.as_str(), "todo": r.todo, "notes": r.notes, "kids": r.kids, "collapsed": r.collapsed, "hit": r.hit, "dim": r.dim, "hidden": r.hidden, "lifted": r.lifted, "target": r.target, "flash": r.flash })
}

pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let id_of = |text: &str| with(|s| s.nodes.iter().find(|n| n.text == text).map(|n| n.id.clone())).unwrap_or_default();
    Some(match name {
        "mindmap-act" => { act(&a(0), &a(1)); json!({ "ok": true }) }
        "mindmap-reload" => { load(); json!({ "ok": true }) }
        "mindmap-load-path" => { load_from(&a(0)); json!({ "ok": true }) }
        "mindmap-search" => { set_search(&a(0)); json!({ "ok": true }) }
        "mindmap-click" => { let id = id_of(&a(0)); node_click(&id); json!({ "ok": true, "id": id }) }
        "mindmap-detail" => { let id = id_of(&a(0)); open_detail(&id); json!({ "ok": true, "id": id }) }
        "mindmap-select" => { let id = id_of(&a(0)); select(&id, true); json!({ "ok": true, "id": id }) }
        "mindmap-canvas-press" => { canvas_press(); json!({ "ok": true }) }
        "mindmap-d-act" => { d_act(&a(0)); json!({ "ok": true }) }
        "mindmap-d-text" => { mg(|g| g.set_d_text(a(0).into())); json!({ "ok": true }) }
        "mindmap-d-note" => { let i: usize = a(0).parse().unwrap_or(0); with(|s| if let Some(n) = s.d_notes.get_mut(i) { *n = a(1); }); json!({ "ok": true }) }
        "mindmap-l-act" => { l_act(&a(0), &a(1), &a(2)); json!({ "ok": true }) }
        "mindmap-l-search" => { mg(|g| g.set_l_search(a(0).into())); link_list(); json!({ "ok": true }) }
        "mindmap-h-toggle" => { let i: usize = a(0).parse().unwrap_or(0); with(|s| if let Some(o) = s.h_open.get_mut(i) { *o = !*o; }); fill_help(); json!({ "ok": true }) }
        "mindmap-h-act" => { if a(0) == "all" { let open = with(|s| { let o = s.h_open.iter().any(|x| !*x); for x in s.h_open.iter_mut() { *x = o; } o }); mg(|g| g.set_h_all(open)); fill_help(); } else { mg(|g| g.set_h_open(false)); modal(false); } json!({ "ok": true }) }
        "mindmap-zoom" => { let (vw, vh) = mg_get(|g| (g.get_view_w(), g.get_view_h())); set_zoom(a(0).parse().unwrap_or(1.0), vw / 2.0, vh / 2.0); json!({ "ok": true }) }
        "mindmap-wheel" => { wheel_zoom(a(0).parse().unwrap_or(0.0), a(1).parse().unwrap_or(0.0), a(2).parse().unwrap_or(0.0)); json!({ "ok": true }) }
        "mindmap-measure" => { mg(|g| g.set_measure_gen(g.get_measure_gen() + 1)); json!({ "ok": true }) }
        "mindmap-flash" => { let id = id_of(&a(0)); flash_node(&id, a(1) == "true"); json!({ "ok": true }) }
        _ => return None,
    })
}

/// This screen's part of the area's props (props.areas.planning.mindmap).
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return Value::Null };
    let g = w.global::<PlanningMindmap>();
    let (z, sx, sy, cx, cy) = (g.get_zoom(), g.get_scroll_x(), g.get_scroll_y(), g.get_canvas_x(), g.get_canvas_y());
    let nw = dims().node;
    let (draft, edit, sel, anchor, saving, collapsed, hidden, detail) = with(|s| (s.draft.clone(), s.edit.clone(), s.sel.clone(), s.anchor.clone(), s.saving, s.collapsed.iter().cloned().collect::<Vec<_>>(), s.hidden.iter().cloned().collect::<Vec<_>>(), s.detail.clone()));
    // Each box in window coordinates (for a real drag in the test).
    let nodes: Vec<Value> = g.get_nodes().iter().map(|r| {
        let mut v = row_json(&r);
        let top = (r.y - r.h / 2.0).max(4.0);
        v["box"] = json!({ "x": cx + sx + r.x * z, "y": cy + sy + top * z, "w": nw * z, "h": r.h * z });
        v
    }).collect();
    json!({
        "state": g.get_state().as_str(), "error": g.get_error().as_str(), "search": g.get_search().as_str(),
        "bar": g.get_bar(), "hiddenCount": g.get_hidden_count(),
        "chips": g.get_chips().iter().map(|c| json!({ "key": c.key.as_str(), "label": c.label.as_str(), "off": c.off })).collect::<Vec<_>>(),
        "nodes": nodes, "edges": g.get_edges().as_str(), "baseW": g.get_base_w(), "baseH": g.get_base_h(),
        "zoom": z, "scroll": [sx, sy], "view": [g.get_view_w(), g.get_view_h()], "canvas": [cx, cy],
        "draft": draft.map(|d| json!({ "parent": d.parent, "after": d.after, "on": g.get_draft_on(), "text": g.get_draft_text().as_str() })),
        "edit": edit.map(|e| json!({ "id": e, "on": g.get_edit_on(), "text": g.get_edit_text().as_str() })),
        "sel": sel, "anchor": anchor, "saving": saving, "collapsed": collapsed, "hidden": hidden,
        "ring": if g.get_ring_on() { json!({ "warn": g.get_ring_warn(), "msg": g.get_ring_msg().as_str() }) } else { Value::Null },
        "hover": if g.get_hover_on() { json!(g.get_hover_node().id.as_str()) } else { Value::Null },
        "drag": if g.get_drag_id().is_empty() { Value::Null } else { json!({ "id": g.get_drag_id().as_str() }) },
        "detail": if g.get_d_open() { json!({ "id": detail, "title": g.get_d_title().as_str(), "linkKind": g.get_d_link_kind().as_str(), "linkMain": g.get_d_link_main().as_str(), "linkText": g.get_d_link_text().as_str(), "linkSt": g.get_d_link_st().as_str(), "linkLabel": g.get_d_link_label().as_str(),
            "notes": g.get_d_notes().iter().map(|t| t.to_string()).collect::<Vec<_>>(), "editing": g.get_d_editing(), "text": g.get_d_text().as_str(),
            "editNotes": with(|s| s.d_notes.clone()), "thumbs": g.get_d_thumbs().row_count() }) } else { Value::Null },
        "link": if g.get_l_open() { json!({ "names": g.get_l_names().iter().map(|t| t.to_string()).collect::<Vec<_>>(), "sel": g.get_l_sel(), "locked": g.get_l_locked(), "lockedText": g.get_l_locked_text().as_str(), "scope": g.get_l_scope_label().as_str(), "loading": g.get_l_loading(),
            "rows": g.get_l_rows().iter().map(|r| json!({ "task": r.task.as_str(), "todo": r.todo.as_str(), "text": r.text.as_str(), "status": r.status.as_str(), "done": r.done })).collect::<Vec<_>>() }) } else { Value::Null },
        "help": if g.get_h_open() { json!({ "all": g.get_h_all(), "rows": g.get_h_rows().iter().map(|r| json!({ "head": r.head.as_str(), "text": r.text.as_str(), "dia": r.dia, "open": r.open })).collect::<Vec<_>>() }) } else { Value::Null },
    })
}
