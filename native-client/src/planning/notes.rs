//! Notes (4.4): the Notes tab and Notes panes (app/vendor/crundi-notes.js:
//! mountManager, mountPicker, mountEditor; their host in src/webapp-html.js:
//! loadNotesTab, mountNotesCell, notesBusyIds, the text size, the SSE
//! `notes` event). The page model and every pure rule are in notes_doc.rs.
//!
//! One place shows a page: the tab (key "") or a workbench pane (its key,
//! panel:notes:<n>). A page is open in one place at a time ("one page, one
//! pane"). Each place has its own state here, keyed by its key.

#[path = "notes_doc.rs"]
pub mod doc;

use crate::{areas::AreaEvent, kit, MainWindow, PlanningNoteBlock, PlanningNoteRow, PlanningNotes, PlanningNotesItem, PlanningNotesMenuItem, PlanningNotesPane};
use doc::{Block, Mark, Run};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, StyledText, VecModel};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::Rc, time::{Duration, Instant}};

const FONT_BASE: i32 = 15;

/// A page open for editing (mountEditor's state).
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
    // What the view is told.
    focus: (String, i32, i32, i32, i32),
    focus_gen: i32,
    title_gen: i32,
    measure_gen: i32,
    drop_y: Option<f32>,
    cur_id: String,
    focus_n: u64,
    boxes: Vec<(String, f32, f32)>,
    page_box: (f32, f32, f32, f32),
    blocks: Rc<VecModel<PlanningNoteBlock>>,
    tables: HashMap<String, Table>,
    md_cache: HashMap<String, (String, StyledText)>,
}

/// A table's models, kept so a cell being edited is never rebuilt.
struct Table { rows: Rc<VecModel<PlanningNoteRow>>, cells: Vec<(Rc<VecModel<SharedString>>, Rc<VecModel<StyledText>>)> }

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

#[derive(Clone)]
struct Drag { key: String, ids: Vec<String>, text: String, spot: Option<(usize, bool)>, target: Option<String>, selecting: bool }

#[derive(Default)]
struct S {
    client_id: String,
    project: String,
    font: i32,
    places: Vec<Place>,
    model: Option<Rc<VecModel<PlanningNotesPane>>>,
    tab: Tab,
    float: Option<Float>,
    menu_ids: Vec<String>,
    drag: Option<Drag>,
    press: Option<(String, String, bool)>,
    caret_xy: (f32, f32),
    hint_at: Option<Instant>,
    last_tab: String,
    seen: HashSet<String>,
    hl: Vec<(crate::workspace::hl::Tok, String)>,
    hl_plain: String,
}

thread_local! { static S_: RefCell<S> = RefCell::new(S::default()); }

fn with<R>(f: impl FnOnce(&mut S) -> R) -> R { S_.with(|s| f(&mut s.borrow_mut())) }

fn ng(f: impl FnOnce(&PlanningNotes)) { if let Some(w) = kit::ui() { f(&w.global::<PlanningNotes>()); } }

fn place_new(key: &str) -> Place {
    Place {
        key: key.into(), page: None, title: String::new(), mode: if key.is_empty() { "empty".into() } else { "picker".into() }, msg: String::new(), ed: None, fresh: false,
        pages: vec![], search: String::new(), list_msg: String::new(), new_busy: false,
        focus: (String::new(), -1, -1, 0, 0), focus_gen: 0, title_gen: 0, measure_gen: 0, drop_y: None, cur_id: String::new(), focus_n: 0,
        boxes: vec![], page_box: (0.0, 0.0, 0.0, 0.0), blocks: Rc::new(VecModel::default()), tables: HashMap::new(), md_cache: HashMap::new(),
    }
}

fn hex(c: slint::Color) -> String { format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue()) }

pub fn init(w: &MainWindow) {
    let n = w.global::<PlanningNotes>();
    let m: Rc<VecModel<PlanningNotesPane>> = Rc::new(VecModel::default());
    n.set_panes(ModelRc::from(m.clone()));
    let th = w.global::<crate::Theme>();
    use crate::workspace::hl::Tok::*;
    let hl = vec![
        (Plain, hex(th.get_ws_hl_plain())), (Keyword, hex(th.get_ws_hl_keyword())), (Str, hex(th.get_ws_hl_string())),
        (Comment, hex(th.get_ws_hl_comment())), (Number, hex(th.get_ws_hl_number())), (Func, hex(th.get_ws_hl_func())),
        (Type, hex(th.get_ws_hl_type())), (Prop, hex(th.get_ws_hl_prop())), (Op, hex(th.get_ws_hl_op())),
        (Tag, hex(th.get_ws_hl_prop())), (Attr, hex(th.get_ws_hl_number())), (Heading, hex(th.get_ws_hl_func())),
    ];
    let font = super::prefs::get("crundi_notes_font").parse::<i32>().ok().filter(|f| (11..=26).contains(f)).unwrap_or(FONT_BASE);
    with(|s| {
        s.client_id = doc::gen_id();
        s.font = font;
        s.model = Some(m);
        s.tab.view = "pages".into();
        s.hl = hl;
        s.hl_plain = hex(th.get_text_primary());
        s.places.push(place_new(""));
    });
    n.set_scale(font as f32 / FONT_BASE as f32);
    n.on_attach(|k| attach(k.as_str()));
    n.on_font(set_font);
    n.on_pages(|k| switch_to_picker(k.as_str()));
    n.on_new_page(|k| new_page(k.as_str()));
    n.on_pick(|k, id| pick(k.as_str(), id.as_str()));
    n.on_search(|k, t| search(k.as_str(), t.as_str()));
    n.on_tab_view_set(|v| { with(|s| s.tab.view = v.to_string()); ng(|n| n.set_tab_view(v.clone())); draw_tab_list(); });
    n.on_item_menu(|id, x, y| item_menu(id.as_str(), x, y));
    n.on_trash_act(|a, id| trash_act(a.as_str(), id.as_str()));
    n.on_title_edited(|k, t| title_edited(k.as_str(), t.as_str()));
    n.on_title_key(|k, name| title_key(k.as_str(), name.as_str()));
    n.on_edited(|k, id, t, c| edited(k.as_str(), id.as_str(), t.as_str(), c));
    n.on_text_key(|k, id, name, a, c, f, l| text_key(k.as_str(), id.as_str(), name.as_str(), a, c, f, l));
    n.on_sel_moved(|k, id, a, c, x, y| sel_moved(k.as_str(), id.as_str(), a, c, x, y));
    n.on_blur(|k, id| blur(k.as_str(), id.as_str()));
    n.on_focused(|k, id, r, c| focused(k.as_str(), id.as_str(), r, c));
    n.on_check(|k, id, v| edit_block(k.as_str(), id.as_str(), |b| b.checked = v));
    n.on_add_below(|k, id| add_below(k.as_str(), id.as_str()));
    n.on_grip(|k, id, x, y| block_menu(k.as_str(), id.as_str(), x, y));
    n.on_code_act(|k, id, a, x, y| code_act(k.as_str(), id.as_str(), a.as_str(), x, y));
    n.on_cell_edited(|k, id, r, c, t| cell_edited(k.as_str(), id.as_str(), r, c, t.as_str()));
    n.on_cell_key(|k, id, r, c, name, a, cc| cell_key(k.as_str(), id.as_str(), r, c, name.as_str(), a, cc));
    n.on_table_act(|k, id, a| table_act(k.as_str(), id.as_str(), a.as_str()));
    n.on_divider_key(|k, id, name| divider_key(k.as_str(), id.as_str(), name.as_str()));
    n.on_link(|u| open_link(u.as_str()));
    n.on_tail(|k| tail(k.as_str()));
    n.on_blk_press(|k, id, shift| blk_press(k.as_str(), id.as_str(), shift));
    n.on_blk_over(|k, y| blk_over(k.as_str(), y));
    n.on_blk_release(|_| with(|s| s.press = None));
    n.on_sel_act(|k, a| sel_act(k.as_str(), a.as_str()));
    n.on_sel_key(|k, name| sel_key(k.as_str(), name.as_str()));
    n.on_drag_start(|k, id| drag_start(k.as_str(), id.as_str()));
    n.on_drag_move(|k, x, y| drag_move(k.as_str(), x, y));
    n.on_drag_end(|_, commit| drag_end(commit));
    n.on_block_box(|k, id, y, h| with(|s| if let Some(p) = s.places.iter_mut().find(|p| p.key == k.as_str()) { p.boxes.retain(|b| b.0 != id.as_str()); p.boxes.push((id.to_string(), y, h)); }));
    n.on_page_box(|k, x, y, w, h| with(|s| if let Some(p) = s.places.iter_mut().find(|p| p.key == k.as_str()) { p.page_box = (x, y, w, h); }));
    n.on_menu_pick(menu_pick);
    n.on_menu_query_edited(|q| menu_query(q.as_str()));
    n.on_menu_key(|name| menu_key(name.as_str()));
    n.on_menu_close(close_float);
    n.on_fmt(|a| fmt_act(a.as_str()));
    n.on_link_apply(|u| link_apply(u.as_str()));
    n.on_link_cancel(|| { let f = with(|s| s.float.clone()); close_float(); if let Some(Float::Fmt { key, id, r, c, a, b, .. }) = f { refocus_range(&key, &id, r, c, a, b); } });
    // Panes closed in the workbench: their page is saved and let go.
    let t = slint::Timer::default();
    t.start(slint::TimerMode::Repeated, Duration::from_millis(1000), prune);
    std::mem::forget(t);
    n.set_ready(true);
}

// ─── Places ───

fn idx(s: &S, key: &str) -> Option<usize> { s.places.iter().position(|p| p.key == key) }

fn panes_key() -> String { format!("notes_panes_{}", kit::project().to_lowercase()) }

fn saved_pane(key: &str) -> Option<(String, String)> {
    let v: Value = serde_json::from_str(&super::prefs::get(&panes_key())).unwrap_or(Value::Null);
    let p = v.get(key)?;
    let id = kit::s(p, "id");
    (!id.is_empty()).then(|| (id, kit::s(p, "title")))
}

fn save_pane(key: &str, page: Option<(&str, &str)>) {
    if key.is_empty() { return; }
    let k = panes_key();
    let mut v: Value = serde_json::from_str(&super::prefs::get(&k)).unwrap_or(json!({}));
    if !v.is_object() { v = json!({}); }
    match page { Some((id, t)) => v[key] = json!({ "id": id, "title": t }), None => { if let Some(o) = v.as_object_mut() { o.remove(key); } } }
    super::prefs::set(&k, &v.to_string());
}

/// A view came on show (the tab, or a pane by its key).
fn attach(key: &str) {
    if key.is_empty() { load_tab(); render(""); return; }
    let fresh = with(|s| {
        s.seen.insert(key.to_string());
        if idx(s, key).is_some() { return false; }
        s.places.push(place_new(key));
        true
    });
    if fresh {
        if let Some((id, title)) = saved_pane(key) {
            with(|s| { let i = idx(s, key).unwrap(); s.places[i].page = Some(id.clone()); s.places[i].title = title; });
            open_editor(key, &id, false);
        } else {
            reload_picker(key);
        }
    }
    render(key);
}

/// Every 1s: panes that left the workbench save and close.
fn prune() {
    let Some(w) = kit::ui() else { return };
    if w.global::<crate::Shell>().get_tab() != "workbench" { return; }
    let keys: HashSet<String> = w.get_panes().iter().map(|p| p.key.to_string()).collect();
    let gone: Vec<String> = with(|s| s.places.iter().filter(|p| !p.key.is_empty() && s.seen.contains(&p.key) && !keys.contains(&p.key)).map(|p| p.key.clone()).collect());
    for k in gone {
        flush(&k);
        save_pane(&k, None);
        with(|s| { s.places.retain(|p| p.key != k); s.seen.remove(&k); });
        sync_places();
        refresh_pickers();
    }
}

/// Pages open somewhere else (notesBusyIds): the panes' pages but `except`'s,
/// and the tab's page when `with_tab`.
fn busy_ids(except: &str, with_tab: bool) -> HashSet<String> {
    with(|s| {
        let mut out: HashSet<String> = s.places.iter().filter(|p| !p.key.is_empty() && p.key != except).filter_map(|p| p.page.clone()).collect();
        if with_tab && s.tab.open { if let Some(c) = &s.tab.current { out.insert(c.clone()); } }
        out
    })
}

/// The pane header's title for a notes pane: the page's ("Untitled" when it
/// has none), or "Notes" with no page. For workspace's pane header.
#[allow(dead_code)]
pub fn pane_title(key: &str) -> String {
    with(|s| s.places.iter().find(|p| p.key == key).and_then(|p| p.page.as_ref().map(|_| if p.title.trim().is_empty() { "Untitled".to_string() } else { p.title.clone() }))).unwrap_or_else(|| "Notes".into())
}

/// Whether a notes pane has a page open (the header's Pages button shows).
#[allow(dead_code)]
pub fn pane_has_page(key: &str) -> bool { with(|s| s.places.iter().any(|p| p.key == key && p.page.is_some())) }

// ─── Drawing ───

fn md_for(cache: &mut HashMap<String, (String, StyledText)>, ck: &str, markup: String) -> StyledText {
    if let Some((m, st)) = cache.get(ck) { if *m == markup { return st.clone(); } }
    let st = StyledText::from_markdown(&markup).unwrap_or_else(|_| StyledText::from_plain_text(&markup));
    cache.insert(ck.to_string(), (markup, st.clone()));
    st
}

fn text_md(cache: &mut HashMap<String, (String, StyledText)>, ck: &str, src: &str, done: bool, bold: bool) -> StyledText {
    let markup = doc::styled(src, done, bold);
    if let Some((m, st)) = cache.get(ck) { if *m == markup { return st.clone(); } }
    let st = StyledText::from_markdown(&markup).or_else(|_| StyledText::from_markdown(&doc::styled_plain(src))).unwrap_or_else(|_| StyledText::from_plain_text(&doc::src_plain(src)));
    cache.insert(ck.to_string(), (markup, st.clone()));
    st
}

fn code_markup(s: &S, b: &Block) -> String {
    let lang = crate::workspace::hl::lang_of(&doc::hl_name(&doc::norm_lang(&b.lang)));
    let colour = |t: crate::workspace::hl::Tok| s.hl.iter().find(|(k, _)| *k == t).map(|(_, c)| c.clone()).unwrap_or_else(|| s.hl_plain.clone());
    crate::workspace::hl::highlight(lang, &b.text, &colour).join("\n")
}

fn block_row(s: &S, p: &mut Place, b: &Block, num: &str, sel: (bool, bool, bool), menu: bool) -> PlanningNoteBlock {
    let md = match b.kind.as_str() {
        "code" => { let m = code_markup(s, b); md_for(&mut p.md_cache, &b.id, m) }
        "table" | "divider" => StyledText::default(),
        k => text_md(&mut p.md_cache, &b.id, &b.text, k == "todo" && b.checked, matches!(k, "h1" | "h2" | "h3")),
    };
    let rows = if b.kind == "table" { table_model(p, b) } else { ModelRc::default() };
    PlanningNoteBlock {
        id: b.id.clone().into(), kind: b.kind.clone().into(), src: b.text.clone().into(), md, ph: doc::placeholder(&b.kind).into(), num: num.into(), checked: b.checked,
        lang_name: doc::lang_name(&doc::norm_lang(&b.lang)).into(), wrap: b.wrap, header: b.header, rows,
        sel: sel.0, sel_top: sel.1, sel_bottom: sel.2, menu_open: menu,
    }
}

fn table_model(p: &mut Place, b: &Block) -> ModelRc<PlanningNoteRow> {
    let t = p.tables.entry(b.id.clone()).or_insert_with(|| Table { rows: Rc::new(VecModel::default()), cells: vec![] });
    while t.cells.len() < b.rows.len() { t.cells.push((Rc::new(VecModel::default()), Rc::new(VecModel::default()))); }
    t.cells.truncate(b.rows.len());
    let mut cache = std::mem::take(&mut p.md_cache);
    let t = p.tables.get_mut(&b.id).unwrap();
    for (ri, r) in b.rows.iter().enumerate() {
        let (cm, mm) = &t.cells[ri];
        kit::sync_rows(cm, r.iter().map(|c| SharedString::from(c.as_str())).collect());
        let mds: Vec<StyledText> = r.iter().enumerate().map(|(ci, c)| text_md(&mut cache, &format!("{}:{ri}:{ci}", b.id), c, false, b.header && ri == 0)).collect();
        kit::sync_rows(mm, mds);
    }
    p.md_cache = cache;
    let rows: Vec<PlanningNoteRow> = t.cells.iter().map(|(c, m)| PlanningNoteRow { cells: ModelRc::from(c.clone()), mds: ModelRc::from(m.clone()) }).collect();
    kit::sync_rows(&t.rows, rows);
    ModelRc::from(t.rows.clone())
}

/// A keyed sync: rows keep their place (and their view, with its focus)
/// where the key is the same one; others are inserted, moved or dropped.
fn sync_keyed<T: Clone + PartialEq + 'static>(m: &VecModel<T>, rows: Vec<T>, key: impl Fn(&T) -> SharedString) {
    let mut i = 0;
    while i < rows.len() {
        let want = key(&rows[i]);
        if i < m.row_count() {
            let cur = m.row_data(i).unwrap();
            if key(&cur) == want {
                if cur != rows[i] { m.set_row_data(i, rows[i].clone()); }
                i += 1;
                continue;
            }
            // The one here is not wanted any more: drop it.
            if !rows[i..].iter().any(|r| key(r) == key(&cur)) { m.remove(i); continue; }
            // Wanted later: bring the one wanted here (or a new one) in front of it.
            if let Some(j) = (i + 1..m.row_count()).find(|j| m.row_data(*j).is_some_and(|r| key(&r) == want)) { m.remove(j); }
            m.insert(i, rows[i].clone());
            i += 1;
            continue;
        }
        m.push(rows[i].clone());
        i += 1;
    }
    while m.row_count() > rows.len() { m.remove(m.row_count() - 1); }
}

fn sync_blocks(m: &VecModel<PlanningNoteBlock>, rows: Vec<PlanningNoteBlock>) { sync_keyed(m, rows, |b| b.id.clone()); }

fn pane_row(s: &mut S, i: usize) -> PlanningNotesPane {
    let float_menu = match &s.float { Some(Float::Block { key, id }) if *key == s.places[i].key => Some(id.clone()), _ => None };
    let mut p = std::mem::replace(&mut s.places[i], place_new("\u{0}"));
    let info = p.ed.as_ref().map(|ed| (doc::numbers(&ed.blocks), sel_range(ed), ed.blocks.clone(), (ed.save_state.clone(), ed.save_text.clone(), ed.save_tip.clone())));
    let (blocks, sel_count, save) = match info {
        Some((nums, range, bl, save)) => {
            let (lo, hi) = range.unwrap_or((usize::MAX, 0));
            let rows: Vec<PlanningNoteBlock> = bl.iter().enumerate().map(|(k, b)| {
                let inn = k >= lo && k <= hi;
                block_row(s, &mut p, b, &nums[k], (inn, inn && k == lo, inn && k == hi), float_menu.as_deref() == Some(b.id.as_str()))
            }).collect();
            let n = if lo == usize::MAX { 0 } else { hi - lo + 1 };
            (rows, n, save)
        }
        None => (vec![], 0, Default::default()),
    };
    sync_blocks(&p.blocks, blocks);
    let busy = busy_ids_in(s, &p.key, true);
    let now = now();
    let items: Vec<PlanningNotesItem> = doc::filter(&p.pages, None, &p.search).iter().map(|v| item_of(v, busy.contains(&kit::s(v, "id")), false, now)).collect();
    let list_msg = if !p.list_msg.is_empty() { p.list_msg.clone() } else if items.is_empty() && !p.key.is_empty() && p.mode == "picker" { if p.pages.is_empty() { "No pages yet in this project.".into() } else { "No page matches.".into() } } else { String::new() };
    let row = PlanningNotesPane {
        key: p.key.clone().into(), mode: p.mode.clone().into(), msg: p.msg.clone().into(), page_id: p.ed.as_ref().map(|e| e.page_id.clone()).unwrap_or_default().into(),
        title: p.ed.as_ref().map(|e| e.title.clone()).unwrap_or_default().into(), title_gen: p.title_gen,
        save_state: save.0.into(), save_text: save.1.into(), save_tip: save.2.into(),
        blocks: ModelRc::from(p.blocks.clone()), sel_count: sel_count as i32,
        focus_id: p.focus.0.clone().into(), focus_r: p.focus.1, focus_c: p.focus.2, focus_a: p.focus.3, focus_b: p.focus.4, focus_gen: p.focus_gen,
        cur_id: p.cur_id.clone().into(), drop_on: p.drop_y.is_some(), drop_y: p.drop_y.unwrap_or(0.0), measure_gen: p.measure_gen,
        items: ModelRc::new(VecModel::from(items)), list_msg: list_msg.into(), new_busy: p.new_busy,
    };
    s.places[i] = p;
    row
}

fn busy_ids_in(s: &S, except: &str, with_tab: bool) -> HashSet<String> {
    let mut out: HashSet<String> = s.places.iter().filter(|p| !p.key.is_empty() && p.key != except).filter_map(|p| p.page.clone()).collect();
    if with_tab && s.tab.open && !except.is_empty() { if let Some(c) = &s.tab.current { out.insert(c.clone()); } }
    out
}

fn now() -> f64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0) }

fn item_of(v: &Value, busy: bool, cur: bool, now: f64) -> PlanningNotesItem {
    let t = kit::s(v, "title");
    let at = kit::parse_iso(&kit::s(v, "updatedAt")).unwrap_or(0.0);
    PlanningNotesItem {
        id: kit::s(v, "id").into(), title: if t.is_empty() { "Untitled".into() } else { t.into() }, preview: kit::s(v, "preview").replace('\n', " ").into(),
        meta: if busy { "Open in another pane".into() } else { format!("Edited {}", doc::ago(now, at)).into() }, busy, cur,
    }
}

/// Draw one place (its row of the panes model).
fn render(key: &str) {
    let Some(m) = with(|s| s.model.clone()) else { return };
    let row = with(|s| idx(s, key).map(|i| (i, pane_row(s, i))));
    let Some((i, row)) = row else { return };
    if m.row_data(i).is_some_and(|r| r.key == row.key) {
        if m.row_data(i).as_ref() != Some(&row) { m.set_row_data(i, row); }
    } else {
        sync_places();
    }
}

/// The panes model in the places' order (keyed: a pane added or closed
/// leaves the others' views as they are).
fn sync_places() {
    let Some(m) = with(|s| s.model.clone()) else { return };
    let n = with(|s| s.places.len());
    let rows: Vec<PlanningNotesPane> = (0..n).map(|i| with(|s| pane_row(s, i))).collect();
    sync_keyed(&m, rows, |p| p.key.clone());
}

#[allow(dead_code)]
fn render_all() { let keys: Vec<String> = with(|s| s.places.iter().map(|p| p.key.clone()).collect()); for k in keys { render(&k); } }

fn refresh_pickers() {
    let keys: Vec<String> = with(|s| s.places.iter().filter(|p| !p.key.is_empty() && p.page.is_none()).map(|p| p.key.clone()).collect());
    for k in keys { render(&k); }
    draw_tab_list();
}

// ─── The text size (shared by every page) ───

fn set_font(dir: i32) {
    let f = with(|s| { s.font = if dir == 0 { FONT_BASE } else { (s.font + dir.signum()).clamp(11, 26) }; s.font });
    super::prefs::set("crundi_notes_font", &if f == FONT_BASE { String::new() } else { f.to_string() });
    ng(|n| n.set_scale(f as f32 / FONT_BASE as f32));
}

// ─── Lists ───

fn fetch_pages(include_deleted: bool, done: impl FnOnce(Result<Vec<Value>, String>) + 'static) {
    let p = kit::project();
    let path = format!("/api/notes?project={}{}", kit::enc(&p), if include_deleted { "&includeDeleted=1" } else { "" });
    let id = super::park(move |v| {
        let r = if let Some(e) = v.get("__err").and_then(|e| e.as_str()) { Err(e.to_string()) }
            else if kit::truthy(&v["ok"]) { Ok(v.get("pages").and_then(|x| x.as_array()).cloned().unwrap_or_default()) }
            else { Err({ let e = kit::s(&v, "error"); if e.is_empty() { "Could not list pages".into() } else { e } }) };
        done(r);
    });
    kit::call(move |api| api.get(&path), move |r| super::unpark(id, r.unwrap_or_else(|e| json!({ "__err": e }))));
}

fn reload_picker(key: &str) {
    if kit::project().is_empty() { return; }
    let k = key.to_string();
    fetch_pages(false, move |r| {
        with(|s| if let Some(i) = idx(s, &k) { match r { Ok(ps) => { s.places[i].pages = ps; s.places[i].list_msg.clear(); } Err(e) => s.places[i].list_msg = e } });
        render(&k);
    });
}

fn search(key: &str, q: &str) {
    if key.is_empty() { with(|s| s.tab.search = q.to_string()); ng(|n| n.set_tab_search(q.into())); draw_tab_list(); return; }
    with(|s| if let Some(i) = idx(s, key) { s.places[i].search = q.to_string(); });
    render(key);
}

/// POST /api/notes with the project and our client id.
fn post(mut body: Value, then: impl FnOnce(Value) + 'static) {
    body["project"] = json!(kit::project());
    body["clientId"] = json!(with(|s| s.client_id.clone()));
    let id = super::park(then);
    kit::call(move |api| api.post("/api/notes", &body), move |r| super::unpark(id, r.unwrap_or_else(|e| json!({ "ok": false, "__net": true, "error": e }))));
}

fn new_page(key: &str) {
    let k = key.to_string();
    let busy = with(|s| if k.is_empty() { let b = s.tab.new_busy; s.tab.new_busy = true; b } else { idx(s, &k).map(|i| { let b = s.places[i].new_busy; s.places[i].new_busy = true; b }).unwrap_or(true) });
    if busy { return; }
    if k.is_empty() { ng(|n| n.set_tab_new_busy(true)); } else { render(&k); }
    post(json!({ "action": "create", "title": "" }), move |d| {
        let ok = kit::truthy(&d["ok"]);
        with(|s| if k.is_empty() { s.tab.new_busy = false; } else if let Some(i) = idx(s, &k) { s.places[i].new_busy = false; });
        if !ok {
            let e = kit::s(&d, "error");
            kit::toast_error(if e.is_empty() { "Could not create a page" } else { &e });
            if k.is_empty() { ng(|n| n.set_tab_new_busy(false)); } else { render(&k); }
            return;
        }
        let page = d["page"].clone();
        let id = kit::s(&page, "id");
        if k.is_empty() {
            ng(|n| { n.set_tab_new_busy(false); n.set_tab_view("pages".into()); });
            with(|s| { s.tab.view = "pages".into(); s.tab.pages.insert(0, json!({ "id": id, "title": "", "preview": "", "updatedAt": kit::s(&page, "updatedAt") })); });
            tab_open(&id, true);
        } else {
            on_pick(&k, &id, "", true);
        }
    });
}

/// The picker's choice (or the tab's list).
fn pick(key: &str, id: &str) {
    if key.is_empty() { if with(|s| s.tab.current.as_deref() != Some(id) && s.tab.view == "pages") { tab_open(id, false); } return; }
    if busy_ids(key, true).contains(id) { kit::toast_error("That page is open in another pane"); return; }
    let title = with(|s| idx(s, key).and_then(|i| s.places[i].pages.iter().find(|p| kit::s(p, "id") == id).map(|p| kit::s(p, "title")))).unwrap_or_default();
    on_pick(key, id, &title, false);
}

fn on_pick(key: &str, id: &str, title: &str, fresh: bool) {
    if busy_ids(key, true).contains(id) { kit::toast_error("That page is open in another pane"); return; }
    with(|s| if let Some(i) = idx(s, key) { s.places[i].page = Some(id.to_string()); s.places[i].title = title.to_string(); });
    save_pane(key, Some((id, title)));
    open_editor(key, id, fresh);
    refresh_pickers();
}

/// "Pages" in a pane's header: back to the picker (the page stays as it is).
pub fn switch_to_picker(key: &str) {
    if key.is_empty() { return; }
    flush(key);
    close_float_of(key);
    with(|s| if let Some(i) = idx(s, key) { let p = &mut s.places[i]; p.page = None; p.title.clear(); p.ed = None; p.mode = "picker".into(); p.search.clear(); p.tables.clear(); p.md_cache.clear(); });
    save_pane(key, None);
    reload_picker(key);
    render(key);
    refresh_pickers();
}

// ─── The tab (mountManager) ───

fn load_tab() {
    let p = kit::project();
    if p.is_empty() {
        close_tab();
        ng(|n| n.set_tab_project(false));
        return;
    }
    ng(|n| n.set_tab_project(true));
    if with(|s| s.tab.open && s.tab.project == p) { return; }
    close_tab();
    let last = super::prefs::get(&format!("crundi_notes_last_{p}"));
    let initial = (!last.is_empty() && !busy_ids("", false).contains(&last)).then_some(last);
    with(|s| { s.tab.open = true; s.tab.project = p.clone(); s.tab.current = None; s.tab.pages.clear(); s.tab.list_msg.clear(); });
    match initial {
        Some(id) => tab_open(&id, false),
        None => { with(|s| if let Some(i) = idx(s, "") { s.places[i].mode = "empty".into(); s.places[i].msg = "Loading…".into(); }); render(""); }
    }
    tab_reload();
    refresh_pickers();
}

/// Leaving the tab closes its page (saving it first), so the tab never holds
/// a page out of a workbench pane while it is not even on screen.
fn close_tab() {
    if !with(|s| s.tab.open) { return; }
    flush("");
    close_float_of("");
    with(|s| {
        s.tab.open = false;
        s.tab.current = None;
        if let Some(i) = idx(s, "") { let p = &mut s.places[i]; p.ed = None; p.mode = "empty".into(); p.msg.clear(); p.tables.clear(); p.md_cache.clear(); }
    });
    render("");
    refresh_pickers();
}

fn tab_open(id: &str, focus: bool) {
    if busy_ids("", false).contains(id) { kit::toast_error("That page is open in a workbench pane. One page, one pane."); return; }
    flush("");
    close_float_of("");
    with(|s| s.tab.current = Some(id.to_string()));
    super::prefs::set(&format!("crundi_notes_last_{}", kit::project()), id);
    open_editor("", id, focus);
    draw_tab_list();
    refresh_pickers();
}

fn tab_reload() {
    let gen = with(|s| { s.tab.list_gen += 1; s.tab.list_gen });
    fetch_pages(true, move |r| {
        if with(|s| s.tab.list_gen != gen || !s.tab.open) { return; }
        match r {
            Ok(ps) => {
                with(|s| { s.tab.pages = ps; s.tab.list_msg.clear(); });
                draw_tab_list();
                let (cur, view) = with(|s| (s.tab.current.clone(), s.tab.view.clone()));
                if cur.is_none() && view == "pages" {
                    let busy = busy_ids("", false);
                    let (first, any) = with(|s| {
                        let live: Vec<&Value> = s.tab.pages.iter().filter(|p| !p.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false)).collect();
                        (live.iter().map(|p| kit::s(p, "id")).find(|id| !busy.contains(id)), !live.is_empty())
                    });
                    match first {
                        Some(id) => tab_open(&id, false),
                        None => tab_empty(if any { "Every page is open in a workbench pane." } else { "No pages yet. Start one with New page." }),
                    }
                }
            }
            Err(e) => { with(|s| s.tab.list_msg = e); draw_tab_list(); }
        }
    });
}

fn tab_empty(msg: &str) {
    flush("");
    with(|s| if let Some(i) = idx(s, "") { let p = &mut s.places[i]; p.ed = None; p.mode = "empty".into(); p.msg = msg.into(); });
    render("");
}

fn draw_tab_list() {
    let (items, msg) = with(|s| {
        let busy = busy_ids_in(s, "", false);
        let trash = s.tab.view == "trash";
        let shown = doc::filter(&s.tab.pages, Some(trash), &s.tab.search);
        let now = now();
        let items: Vec<PlanningNotesItem> = shown.iter().map(|v| {
            let id = kit::s(v, "id");
            let cur = !trash && s.tab.current.as_deref() == Some(id.as_str());
            item_of(v, busy.contains(&id) && !cur, cur, now)
        }).collect();
        let msg = if !s.tab.list_msg.is_empty() { s.tab.list_msg.clone() } else if items.is_empty() {
            if trash { "The trash is empty.".into() } else if s.tab.pages.iter().any(|p| !p.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false)) { "No page matches.".into() } else { "No pages yet. Start one with New page.".into() }
        } else { String::new() };
        (items, msg)
    });
    ng(|n| { n.set_tab_items(ModelRc::new(VecModel::from(items))); n.set_tab_list_msg(msg.into()); });
}

fn item_menu(id: &str, x: f32, y: f32) {
    if with(|s| s.tab.view == "trash") { return; }
    close_float();
    with(|s| { s.float = Some(Float::Item { id: id.to_string() }); s.menu_ids = vec!["trash".into()]; });
    show_menu(x, y, "", vec![mi("trash", "\u{2715}", "Move to trash", "", false, true, false)], -1, false, "", "");
}

fn page_act(action: &str, id: &str, then: impl FnOnce(Value) + 'static) {
    post(json!({ "action": action, "id": id }), move |d| {
        if !kit::truthy(&d["ok"]) { let e = kit::s(&d, "error"); kit::toast_error(if e.is_empty() { "Failed" } else { &e }); }
        then(d);
    });
}

fn trash_act(a: &str, id: &str) {
    let id = id.to_string();
    match a {
        "restore" => page_act("restore", &id, |_| tab_reload()),
        "purge" => {
            let title = with(|s| s.tab.pages.iter().find(|p| kit::s(p, "id") == id).map(|p| kit::s(p, "title"))).unwrap_or_default();
            let msg = format!("Delete \u{201c}{}\u{201d} for good? This cannot be undone.", if title.is_empty() { "Untitled" } else { &title });
            kit::confirm(kit::Confirm { title: "Delete for good", message: &msg, ok: "Delete for good", danger: true }, move |ok| { if ok { page_act("purge", &id, |_| tab_reload()); } });
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

fn open_editor(key: &str, id: &str, focus: bool) {
    with(|s| if let Some(i) = idx(s, key) {
        let p = &mut s.places[i];
        let mut ed = Ed { page_id: id.to_string(), ..Default::default() };
        set_save(&mut ed, "busy", "Loading…");
        p.ed = Some(ed);
        p.mode = "editor".into();
        p.msg.clear();
        p.fresh = focus;
        p.tables.clear();
        p.md_cache.clear();
        p.title_gen += 1;
        p.cur_id.clear();
    });
    render(key);
    load_page(key);
}

fn load_page(key: &str) {
    let Some((id, gen)) = with(|s| ed_mut(s, key, |e| { e.load_gen += 1; set_save(e, "busy", "Loading…"); (e.page_id.clone(), e.load_gen) })) else { return };
    render(key);
    let path = format!("/api/notes/page?project={}&id={}", kit::enc(&kit::project()), kit::enc(&id));
    let k = key.to_string();
    let pid = super::park(move |v| on_loaded(&k, &id, gen, v));
    kit::call(move |api| api.get(&path), move |r| super::unpark(pid, r.unwrap_or_else(|e| json!({ "__err": e }))));
}

fn on_loaded(key: &str, id: &str, gen: u64, d: Value) {
    let live = with(|s| ed_mut(s, key, |e| e.page_id == id && e.load_gen == gen).unwrap_or(false));
    if !live { return; }
    if let Some(e) = d.get("__err").and_then(|e| e.as_str()) {
        with(|s| ed_mut(s, key, |ed| set_save(ed, "err", "Could not load")));
        kit::toast_error(e);
        render(key);
        return;
    }
    if !kit::truthy(&d["ok"]) {
        let e = kit::s(&d, "error");
        with(|s| { ed_mut(s, key, |ed| set_save(ed, "err", if e.is_empty() { "Not found" } else { &e })); if let Some(i) = idx(s, key) { s.places[i].mode = "msg".into(); s.places[i].msg = if e.is_empty() { "This page could not be opened.".into() } else { e.clone() }; } });
        render(key);
        // A pane whose page is gone goes back to its picker (onMissing).
        if !key.is_empty() { switch_to_picker(key); }
        return;
    }
    let page = &d["page"];
    if page.get("deleted").and_then(|x| x.as_bool()).unwrap_or(false) {
        with(|s| { ed_mut(s, key, |ed| set_save(ed, "err", "In the trash")); if let Some(i) = idx(s, key) { s.places[i].mode = "msg".into(); s.places[i].msg = "This page is in the trash. Restore it from the Notes tab to edit it.".into(); } });
        render(key);
        return;
    }
    show_page(key, page);
    let fresh = with(|s| idx(s, key).map(|i| std::mem::take(&mut s.places[i].fresh)).unwrap_or(false));
    if fresh {
        let empty_title = with(|s| ed_mut(s, key, |e| e.title.is_empty()).unwrap_or(true));
        if empty_title { focus_title(key); } else { focus_first(key); }
    }
}

fn show_page(key: &str, page: &Value) {
    let title = kit::s(page, "title");
    let blocks = doc::blocks_from(&page["blocks"]);
    let updated = kit::s(page, "updatedAt");
    with(|s| {
        if let Some(i) = idx(s, key) {
            let p = &mut s.places[i];
            p.mode = "editor".into();
            p.title_gen += 1;
            p.tables.clear();
            p.md_cache.clear();
            if let Some(ed) = p.ed.as_mut() {
                ed.version = page.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
                ed.title = title.clone();
                ed.blocks = blocks;
                ed.loaded = true;
                ed.dirty = false;
                ed.sel = None;
                set_save(ed, "ok", "Saved");
                ed.save_tip = kit::parse_iso(&updated).map(|t| format!("Last edited {}", doc::ago(now(), t))).unwrap_or_default();
            }
        }
    });
    on_title(key, &title);
    render(key);
}

/// The page's title changed (the pane header, the tab's list).
fn on_title(key: &str, title: &str) {
    let id = with(|s| idx(s, key).and_then(|i| { s.places[i].title = title.to_string(); s.places[i].ed.as_ref().map(|e| e.page_id.clone()) }));
    if key.is_empty() {
        if let Some(id) = id { with(|s| if let Some(p) = s.tab.pages.iter_mut().find(|p| kit::s(p, "id") == id) { p["title"] = json!(title); }); draw_tab_list(); }
    } else if let Some(id) = id {
        save_pane(key, Some((&id, title)));
    }
}

/// Something on the page changed: save it soon (changed()).
fn changed(key: &str) {
    let gen = with(|s| ed_mut(s, key, |e| {
        if !e.loaded { return None; }
        e.dirty = true;
        set_save(e, "busy", "Editing…");
        e.save_gen += 1;
        Some(e.save_gen)
    }).flatten());
    let Some(gen) = gen else { return };
    schedule_save(key, gen, 700);
}

fn schedule_save(key: &str, gen: u64, ms: u64) {
    let k = key.to_string();
    slint::Timer::single_shot(Duration::from_millis(ms), move || {
        if with(|s| ed_mut(s, &k, |e| e.save_gen == gen).unwrap_or(false)) { save(&k); }
    });
}

fn save(key: &str) {
    let body = with(|s| ed_mut(s, key, |e| {
        if !e.dirty || !e.loaded { return None; }
        if e.saving { e.save_gen += 1; return Some(Err(e.save_gen)); }
        e.saving = true;
        e.dirty = false;
        set_save(e, "busy", "Saving…");
        Some(Ok(json!({ "action": "save", "id": e.page_id, "title": e.title.trim(), "blocks": e.blocks.iter().map(|b| b.to_json()).collect::<Vec<_>>(), "baseVersion": e.version })))
    }).flatten());
    let Some(body) = body else { return };
    let body = match body { Ok(b) => b, Err(gen) => { schedule_save(key, gen, 400); return; } };
    render(key);
    let k = key.to_string();
    let page_id = kit::s(&body, "id");
    let mine = body.clone();
    post(body, move |d| {
        let live = with(|s| ed_mut(s, &k, |e| e.page_id == page_id).unwrap_or(false));
        if !live { return; }
        if kit::truthy(&d["ok"]) {
            let again = with(|s| ed_mut(s, &k, |e| {
                e.saving = false;
                e.version = d.get("version").and_then(|v| v.as_i64()).unwrap_or(e.version + 1);
                set_save(e, "ok", "Saved");
                e.save_tip = "Saved just now".into();
                if e.dirty { e.save_gen += 1; Some(e.save_gen) } else { None }
            }).flatten());
            if let Some(g) = again { schedule_save(&k, g, 400); }
            render(&k);
            return;
        }
        if kit::truthy(&d["conflict"]) { with(|s| ed_mut(s, &k, |e| e.saving = false)); on_conflict(&k, &mine, &d["page"]); return; }
        if kit::truthy(&d["__net"]) {
            let g = with(|s| ed_mut(s, &k, |e| { e.saving = false; e.dirty = true; set_save(e, "err", "Offline \u{2014} will retry"); e.save_gen += 1; e.save_gen }));
            if let Some(g) = g { schedule_save(&k, g, 4000); }
            render(&k);
            return;
        }
        let e = kit::s(&d, "error");
        with(|s| ed_mut(s, &k, |ed| { ed.saving = false; ed.dirty = true; set_save(ed, "err", if e.is_empty() { "Not saved" } else { &e }); }));
        kit::toast_error(if e.is_empty() { "Could not save the page" } else { &e });
        render(&k);
    });
}

/// Someone else saved this page since we loaded it. Nothing is thrown away:
/// what was typed here becomes a copy, and this place shows theirs.
fn on_conflict(key: &str, mine: &Value, theirs: &Value) {
    let t = mine["title"].as_str().unwrap_or("");
    post(json!({ "action": "create", "title": format!("{} (your copy)", if t.is_empty() { "Untitled" } else { t }), "blocks": mine["blocks"] }), |d| {
        let name = d["page"]["title"].as_str().unwrap_or("a copy").to_string();
        kit::toast_error(&format!("This page was changed elsewhere. Your version was saved as \u{201c}{name}\u{201d}."));
    });
    show_page(key, theirs);
}

/// Save now what waits (closing a pane, switching pages).
fn flush(key: &str) {
    let go = with(|s| ed_mut(s, key, |e| { if e.dirty { e.save_gen += 1; true } else { false } }).unwrap_or(false));
    if go { save(key); }
}

/// A save from elsewhere: reload when nothing here is unsaved.
fn remote_change(key: &str, info: &Value) {
    let me = with(|s| s.client_id.clone());
    let reload = with(|s| ed_mut(s, key, |e| {
        if kit::s(info, "id") != e.page_id || kit::s(info, "by") == me { return false; }
        let a = kit::s(info, "action");
        if a == "delete" || a == "purge" { return true; }
        if let Some(v) = info.get("version").and_then(|v| v.as_i64()) { if v <= e.version { return false; } }
        !e.dirty && !e.saving
    }).unwrap_or(false));
    if reload { load_page(key); }
}

// ─── Focus ───

fn request_focus(key: &str, id: &str, r: i32, c: i32, a: usize, b: usize) {
    with(|s| if let Some(i) = idx(s, key) { let p = &mut s.places[i]; p.focus = (id.to_string(), r, c, a as i32, b as i32); p.focus_gen += 1; });
    render(key);
}

fn block_of<'a>(ed: &'a Ed, id: &str) -> Option<(usize, &'a Block)> { ed.blocks.iter().enumerate().find(|(_, b)| b.id == id) }

/// Put the caret in a block: at its start, its end, or a plain offset.
#[derive(Clone, Copy)]
enum At { Start, End, Plain(usize) }

fn focus_block(key: &str, id: &str, at: At) {
    let t = with(|s| ed_mut(s, key, |e| block_of(e, id).map(|(_, b)| b.clone())).flatten());
    let Some(b) = t else { return };
    match b.kind.as_str() {
        "divider" => request_focus(key, id, -1, -1, 0, 0),
        "table" => {
            let (r, c) = match at { At::End => (b.rows.len().saturating_sub(1), b.rows.last().map(|r| r.len().saturating_sub(1)).unwrap_or(0)), _ => (0, 0) };
            let src = b.rows.get(r).and_then(|x| x.get(c)).cloned().unwrap_or_default();
            let o = if matches!(at, At::End) { src.len() } else { 0 };
            request_focus(key, id, r as i32, c as i32, o, o);
        }
        "code" => { let o = if matches!(at, At::Start) { 0 } else { b.text.len() }; request_focus(key, id, -1, -1, o, o); }
        _ => {
            let o = match at {
                At::Start => 0,
                At::End => b.text.len(),
                At::Plain(k) => { let (_, map) = doc::runs_to_src_map(&doc::src_to_runs(&b.text)); doc::plain_to_src_at(&map, k) }
            };
            request_focus(key, id, -1, -1, o, o);
        }
    }
}

fn focus_title(key: &str) {
    let n = with(|s| ed_mut(s, key, |e| e.title.len()).unwrap_or(0));
    request_focus(key, "title", -1, -1, n, n);
}

fn focus_first(key: &str) {
    if let Some(id) = with(|s| ed_mut(s, key, |e| e.blocks.first().map(|b| b.id.clone())).flatten()) { focus_block(key, &id, At::End); }
}

fn hint_select_all() {
    let go = with(|s| { let ok = s.hint_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(8)); if ok { s.hint_at = Some(Instant::now()); } ok });
    if go { kit::toast(&format!("{m}+A selects this block\u{2019}s text. {m}+Shift+A selects the whole page.", m = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" })); }
}

// ─── Block edits ───

/// Change a block in place (the checkbox, a table cell).
fn edit_block(key: &str, id: &str, f: impl FnOnce(&mut Block)) {
    let ok = with(|s| ed_mut(s, key, |e| e.blocks.iter_mut().find(|b| b.id == id).map(f).is_some()).unwrap_or(false));
    if ok { changed(key); render(key); }
}

fn insert_after(key: &str, after: Option<&str>, b: Block) -> String {
    let id = b.id.clone();
    with(|s| ed_mut(s, key, |e| {
        let at = after.and_then(|a| e.blocks.iter().position(|x| x.id == a)).map(|i| i + 1).unwrap_or(e.blocks.len());
        e.blocks.insert(at, b);
    }));
    changed(key);
    render(key);
    id
}

fn remove_block(e: &mut Ed, id: &str) {
    e.blocks.retain(|b| b.id != id);
    if e.blocks.is_empty() { e.blocks.push(Block::new("p", "")); }
}

/// Turn a block into another type (convert()), with extra settings.
fn convert(key: &str, id: &str, kind: &str, checked: bool, lang: Option<String>) {
    with(|s| ed_mut(s, key, |e| if let Some(b) = e.blocks.iter_mut().find(|b| b.id == id) {
        let mut nb = b.converted(kind);
        if checked { nb.checked = true; }
        if let Some(l) = lang { nb.lang = l; nb.text.clear(); }
        *b = nb;
    }));
    with(|s| if let Some(i) = idx(s, key) { s.places[i].md_cache.remove(id); s.places[i].tables.remove(id); });
    changed(key);
    render(key);
}

fn neighbours(key: &str, id: &str) -> (Option<Block>, Option<Block>) {
    with(|s| ed_mut(s, key, |e| {
        let i = e.blocks.iter().position(|b| b.id == id);
        match i { Some(i) => (i.checked_sub(1).map(|j| e.blocks[j].clone()), e.blocks.get(i + 1).cloned()), None => (None, None) }
    }).unwrap_or((None, None)))
}

fn cur_block(key: &str, id: &str) -> Option<Block> { with(|s| ed_mut(s, key, |e| block_of(e, id).map(|(_, b)| b.clone())).flatten()) }

fn set_text(key: &str, id: &str, text: String) {
    with(|s| ed_mut(s, key, |e| if let Some(b) = e.blocks.iter_mut().find(|b| b.id == id) { b.text = text; }));
}

fn title_edited(key: &str, t: &str) {
    // The title is one line: a pasted line break becomes a space.
    let clean = if t.contains(['\n', '\r']) { t.split_whitespace().collect::<Vec<_>>().join(" ") } else { t.to_string() };
    let reset = clean != t;
    with(|s| { ed_mut(s, key, |e| e.title = clean.clone()); if reset { if let Some(i) = idx(s, key) { s.places[i].title_gen += 1; } } });
    on_title(key, clean.trim());
    changed(key);
    render(key);
}

fn title_key(key: &str, name: &str) -> bool {
    match name {
        "enter" | "down" => { focus_first(key); true }
        "ctrl-a" => { hint_select_all(); false }
        "ctrl-shift-a" => { select_all(key); true }
        _ => false,
    }
}

fn edited(key: &str, id: &str, text: &str, caret: i32) {
    let Some(b) = cur_block(key, id) else { return };
    if matches!(&with(|s| s.float.clone()), Some(Float::Fmt { link: false, .. })) { close_float(); }
    set_text(key, id, text.to_string());
    if b.kind == "code" { changed(key); render(key); return; }
    // Markdown-style shortcuts at the start of a block.
    if let Some(sc) = doc::shortcut(&b.kind, text) {
        close_float();
        match sc {
            doc::Shortcut::To(k, ch) => { set_text(key, id, String::new()); convert(key, id, k, ch, None); focus_block(key, id, At::Start); }
            doc::Shortcut::Divider => {
                set_text(key, id, String::new());
                convert(key, id, "divider", false, None);
                let nid = insert_after(key, Some(id), Block::new("p", ""));
                focus_block(key, &nid, At::Start);
            }
            doc::Shortcut::Code(lang) => { convert(key, id, "code", false, Some(lang)); focus_block(key, id, At::Start); }
        }
        return;
    }
    changed(key);
    render(key);
    // The slash menu: follow it, or open it on a "/" typed at the start or after a space.
    let plain = doc::src_plain(text);
    let cp = doc::src_to_plain_at(text, caret.max(0) as usize);
    let fl = with(|s| s.float.clone());
    match fl {
        Some(Float::Slash { key: k, id: bid, start, .. }) if k == key && bid == id => {
            match start.and_then(|st| doc::slash_query(&plain, st, cp).map(|q| (st, q))) {
                Some((st, q)) => { with(|s| s.float = Some(Float::Slash { key: k, id: bid, start: Some(st), query: q })); draw_slash(); }
                None => if start.is_some() { close_float(); },
            }
        }
        _ => if doc::slash_opens(&plain, cp) { open_slash(key, id, Some(cp - 1)); },
    }
}

fn text_key(key: &str, id: &str, name: &str, anchor: i32, caret: i32, first: bool, last: bool) -> bool {
    // The slash menu takes its keys first.
    if let Some(Float::Slash { key: k, id: bid, .. }) = with(|s| s.float.clone()) {
        if k == key && bid == id && matches!(name, "up" | "down" | "enter" | "tab" | "escape") { return menu_key(name); }
    }
    let Some(b) = cur_block(key, id) else { return false };
    let (a, c) = (anchor.max(0) as usize, caret.max(0) as usize);
    let collapsed = a == c;
    match name {
        "ctrl-shift-a" => { select_all(key); return true; }
        "ctrl-a" => { if b.kind != "code" { hint_select_all(); } return false; }
        _ => {}
    }
    let (prev, next) = neighbours(key, id);
    if b.kind == "code" {
        return match name {
            "tab" => { let mut t = b.text.clone(); let (lo, hi) = (a.min(c).min(t.len()), a.max(c).min(t.len())); t.replace_range(lo..hi, "  "); set_text(key, id, t); changed(key); render(key); request_focus(key, id, -1, -1, lo + 2, lo + 2); true }
            "shift-enter" | "ctrl-enter" => { let n = insert_after(key, Some(id), Block::new("p", "")); focus_block(key, &n, At::Start); true }
            "backspace" if b.text.is_empty() => { convert(key, id, "p", false, None); focus_block(key, id, At::Start); true }
            "up" if first && collapsed => { if let Some(p) = prev { focus_block(key, &p.id, At::End); true } else { false } }
            "down" if last && collapsed => {
                match next { Some(n) => focus_block(key, &n.id, At::Start), None => { let n = insert_after(key, Some(id), Block::new("p", "")); focus_block(key, &n, At::Start); } }
                true
            }
            "shift-up" if first && collapsed && prev.is_some() => { set_sel(key, id, &prev.unwrap().id); true }
            "shift-down" if last && collapsed && next.is_some() => { set_sel(key, id, &next.unwrap().id); true }
            _ => false,
        };
    }
    match name {
        "shift-up" if first && collapsed && prev.is_some() => { set_sel(key, id, &prev.unwrap().id); true }
        "shift-down" if last && collapsed && next.is_some() => { set_sel(key, id, &next.unwrap().id); true }
        "ctrl-b" => { mark(key, id, -1, -1, a, c, Mark::B); true }
        "ctrl-i" => { mark(key, id, -1, -1, a, c, Mark::I); true }
        "ctrl-u" => { mark(key, id, -1, -1, a, c, Mark::U); true }
        "ctrl-e" => { mark(key, id, -1, -1, a, c, Mark::Code); true }
        "ctrl-shift-s" => { mark(key, id, -1, -1, a, c, Mark::S); true }
        "ctrl-k" => { open_fmt(key, id, -1, -1, a, c, true); true }
        "ctrl-v" => paste(key, id, -1, -1, a, c),
        "enter" => {
            close_float();
            if doc::listy(&b.kind) && doc::src_plain(&b.text).trim().is_empty() {
                // An empty list item ends the list.
                convert(key, id, "p", false, None);
                focus_block(key, id, At::Start);
                return true;
            }
            let runs = doc::src_to_runs(&b.text);
            let (lo, hi) = (doc::src_to_plain_at(&b.text, a.min(c)), doc::src_to_plain_at(&b.text, a.max(c)));
            let (left, rest) = doc::split_runs(&runs, lo);
            let (_, right) = doc::split_runs(&rest, hi - lo);
            set_text(key, id, doc::runs_to_src(&left));
            let kind = if doc::listy(&b.kind) { b.kind.as_str() } else { "p" };
            let nb = Block { id: doc::gen_id(), kind: kind.into(), text: doc::runs_to_src(&right), ..Default::default() };
            let n = insert_after(key, Some(id), nb);
            focus_block(key, &n, At::Start);
            true
        }
        "backspace" if collapsed && c == 0 => {
            if b.kind != "p" { convert(key, id, "p", false, None); focus_block(key, id, At::Start); return true; }
            let Some(p) = prev else { return false };
            if p.kind == "divider" { with(|s| ed_mut(s, key, |e| remove_block(e, &p.id))); changed(key); render(key); return true; }
            if doc::texty(&p.kind) {
                let len = doc::src_plain(&p.text).chars().count();
                let joined = doc::norm(doc::src_to_runs(&p.text).into_iter().chain(doc::src_to_runs(&b.text)).collect());
                set_text(key, &p.id, doc::runs_to_src(&joined));
                with(|s| ed_mut(s, key, |e| remove_block(e, id)));
                changed(key);
                render(key);
                focus_block(key, &p.id, At::Plain(len));
            } else if doc::src_plain(&b.text).is_empty() {
                with(|s| ed_mut(s, key, |e| remove_block(e, id)));
                changed(key);
                render(key);
                focus_block(key, &p.id, At::End);
            } else {
                focus_block(key, &p.id, At::End);
            }
            true
        }
        "delete" if collapsed && c >= b.text.len() => {
            let Some(n) = next.filter(|n| doc::texty(&n.kind)) else { return false };
            let len = doc::src_plain(&b.text).chars().count();
            let joined = doc::norm(doc::src_to_runs(&b.text).into_iter().chain(doc::src_to_runs(&n.text)).collect());
            set_text(key, id, doc::runs_to_src(&joined));
            with(|s| ed_mut(s, key, |e| remove_block(e, &n.id)));
            changed(key);
            render(key);
            focus_block(key, id, At::Plain(len));
            true
        }
        "up" if first => { if let Some(p) = prev { focus_block(key, &p.id, At::End); true } else { false } }
        "down" if last => { if let Some(n) = next { focus_block(key, &n.id, At::Start); true } else { false } }
        "escape" => { close_float(); false }
        _ => false,
    }
}

/// The plain range a..b (byte offsets of the source) of a block or cell.
fn src_of(key: &str, id: &str, r: i32, c: i32) -> Option<String> {
    let b = cur_block(key, id)?;
    if r >= 0 { b.rows.get(r as usize).and_then(|x| x.get(c as usize)).cloned() } else { Some(b.text) }
}

fn put_src(key: &str, id: &str, r: i32, c: i32, src: String) {
    with(|s| ed_mut(s, key, |e| if let Some(b) = e.blocks.iter_mut().find(|b| b.id == id) {
        if r >= 0 { if let Some(cell) = b.rows.get_mut(r as usize).and_then(|x| x.get_mut(c as usize)) { *cell = src; } } else { b.text = src; }
    }));
}

/// Rewrite a block's (or cell's) runs, keeping the plain range selected.
fn rewrite(key: &str, id: &str, r: i32, c: i32, runs: Vec<Run>, pa: usize, pb: usize) {
    let (src, map) = doc::runs_to_src_map(&runs);
    put_src(key, id, r, c, src);
    changed(key);
    render(key);
    request_focus(key, id, r, c, doc::plain_to_src_at(&map, pa), doc::plain_to_src_at(&map, pb));
}

fn mark(key: &str, id: &str, r: i32, c: i32, a: usize, b: usize, m: Mark) {
    let Some(src) = src_of(key, id, r, c) else { return };
    let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b)));
    if pa == pb { return; }
    let runs = doc::toggle(&doc::src_to_runs(&src), pa, pb, m);
    rewrite(key, id, r, c, runs, pa, pb);
    // The format bar stays on the same selection.
    if let Some(Float::Fmt { key: k, id: i2, r: r2, c: c2, .. }) = with(|s| s.float.clone()) {
        if k == key && i2 == id && r2 == r && c2 == c {
            let (_, map) = doc::runs_to_src_map(&doc::src_to_runs(&src_of(key, id, r, c).unwrap_or_default()));
            with(|s| if let Some(Float::Fmt { a, b, .. }) = s.float.as_mut() { *a = doc::plain_to_src_at(&map, pa); *b = doc::plain_to_src_at(&map, pb); });
        }
    }
}

fn refocus_range(key: &str, id: &str, r: i32, c: i32, a: usize, b: usize) { request_focus(key, id, r, c, a, b); }

/// Ctrl+V: plain text, URLs as links; a URL over selected text links it;
/// several lines become several blocks (pasteInto()).
fn paste(key: &str, id: &str, r: i32, c: i32, a: usize, b: usize) -> bool {
    let Some(text) = arboard::Clipboard::new().ok().and_then(|mut cb| cb.get_text().ok()) else { return false };
    let Some(src) = src_of(key, id, r, c) else { return false };
    if matches!(&with(|s| s.float.clone()), Some(Float::Fmt { .. })) { close_float(); }
    let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b)));
    let runs = doc::src_to_runs(&src);
    if doc::is_url(&text) && pa != pb {
        rewrite(key, id, r, c, doc::link(&runs, pa, pb, Some(text.trim().to_string())), pb, pb);
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
        rewrite(key, id, r, c, all, pa + n, pa + n);
        return true;
    }
    // The first line joins this block; the rest become paragraphs after it.
    let first = doc::norm(left.into_iter().chain(doc::linkify(lines[0])).collect());
    put_src(key, id, r, c, doc::runs_to_src(&first));
    let mut last = id.to_string();
    for (i, l) in lines.iter().enumerate().skip(1) {
        let mut rr = doc::linkify(l);
        if i == lines.len() - 1 { rr.extend(right.clone()); }
        let nb = Block { id: doc::gen_id(), kind: "p".into(), text: doc::runs_to_src(&doc::norm(rr)), ..Default::default() };
        last = insert_after(key, Some(&last), nb);
    }
    changed(key);
    render(key);
    focus_block(key, &last, At::End);
    true
}

fn add_below(key: &str, id: &str) {
    let n = insert_after(key, Some(id), Block::new("p", ""));
    focus_block(key, &n, At::End);
    open_slash(key, &n, None);
}

fn tail(key: &str) {
    let last = with(|s| ed_mut(s, key, |e| e.blocks.last().cloned()).flatten());
    match last {
        Some(b) if b.kind == "p" && b.text.is_empty() => focus_block(key, &b.id, At::End),
        _ => { let n = insert_after(key, last.as_ref().map(|b| b.id.as_str()), Block::new("p", "")); focus_block(key, &n, At::Start); }
    }
}

fn divider_key(key: &str, id: &str, name: &str) -> bool {
    let (prev, next) = neighbours(key, id);
    match name {
        "backspace" | "delete" => {
            with(|s| ed_mut(s, key, |e| remove_block(e, id)));
            changed(key);
            render(key);
            if let Some(p) = prev.or(next) { focus_block(key, &p.id, At::End); }
            true
        }
        "enter" => { let n = insert_after(key, Some(id), Block::new("p", "")); focus_block(key, &n, At::Start); true }
        "up" => { if let Some(p) = prev { focus_block(key, &p.id, At::End); true } else { false } }
        "down" => { if let Some(n) = next { focus_block(key, &n.id, At::Start); true } else { false } }
        "ctrl-shift-a" => { select_all(key); true }
        _ => false,
    }
}

fn focused(key: &str, id: &str, r: i32, c: i32) {
    let pressing = with(|s| s.press.is_some());
    with(|s| ed_mut(s, key, |e| {
        e.focus = Some(id.to_string());
        if r >= 0 { e.cell = (r as usize, c as usize); }
        // Back to typing in a block ends a block selection.
        if e.sel.is_some() && !pressing && id != "__sel" { e.sel = None; }
    }));
    let is_table = cur_block(key, id).is_some_and(|b| b.kind == "table");
    with(|s| if let Some(i) = idx(s, key) { s.places[i].focus_n += 1; s.places[i].cur_id = if is_table { id.to_string() } else { String::new() }; });
    render(key);
}

fn blur(key: &str, id: &str) {
    let close = matches!(&with(|s| s.float.clone()), Some(Float::Fmt { key: k, id: i, link: false, .. }) if k == key && i == id);
    if close { close_float(); }
    // The table's buttons go with its focus (a press on them keeps it; a
    // move to another of its cells sets it again).
    let n = with(|s| idx(s, key).map(|i| s.places[i].focus_n)).unwrap_or(0);
    let (k, i) = (key.to_string(), id.to_string());
    slint::Timer::single_shot(Duration::from_millis(200), move || {
        let gone = with(|s| idx(s, &k).map(|x| { let p = &mut s.places[x]; if p.focus_n == n && p.cur_id == i { p.cur_id.clear(); true } else { false } }).unwrap_or(false));
        if gone { render(&k); }
    });
}

// ─── Code blocks ───

fn code_act(key: &str, id: &str, a: &str, x: f32, y: f32) {
    let Some(b) = cur_block(key, id) else { return };
    match a {
        "wrap" => { edit_block(key, id, |b| b.wrap = !b.wrap); }
        "copy" => { if copy(&b.text) { kit::toast("Code copied"); } else { kit::toast_error("Could not copy"); } }
        "lang" => {
            close_float();
            with(|s| s.float = Some(Float::Lang { key: key.into(), id: id.into(), query: String::new() }));
            ng(|n| { n.set_menu_query("".into()); n.set_menu_gen(n.get_menu_gen() + 1); });
            draw_lang(x - with_menu_w(), y);
        }
        _ => {}
    }
}

fn with_menu_w() -> f32 { kit::ui().map(|w| w.global::<crate::Theme>().get_notes_menu_w()).unwrap_or(220.0) }

fn draw_lang(x: f32, y: f32) {
    let Some(Float::Lang { key, id, query }) = with(|s| s.float.clone()) else { return };
    let cur = cur_block(&key, &id).map(|b| doc::norm_lang(&b.lang)).unwrap_or_else(|| "plain".into());
    let items = doc::lang_items(&query);
    let act = items.iter().position(|l| l.0 == cur).unwrap_or(0) as i32;
    let rows: Vec<PlanningNotesMenuItem> = items.iter().map(|l| mi(l.0, "", l.1, "", l.0 == cur, false, false)).collect();
    with(|s| s.menu_ids = items.iter().map(|l| l.0.to_string()).collect());
    let (x, y) = if x < 0.0 { ng_get_xy() } else { (x, y) };
    show_menu(x, y, "", rows, if query.is_empty() { act } else { 0 }, true, &query, "No such language");
}

fn ng_get_xy() -> (f32, f32) { kit::ui().map(|w| { let n = w.global::<PlanningNotes>(); (n.get_menu_x(), n.get_menu_y()) }).unwrap_or((0.0, 0.0)) }

// ─── Tables ───

fn cell_edited(key: &str, id: &str, r: i32, c: i32, t: &str) {
    if matches!(&with(|s| s.float.clone()), Some(Float::Fmt { link: false, .. })) { close_float(); }
    put_src(key, id, r, c, t.to_string());
    changed(key);
    render(key);
}

fn cell_key(key: &str, id: &str, r: i32, c: i32, name: &str, a: i32, cc: i32) -> bool {
    let Some(b) = cur_block(key, id) else { return false };
    let (ru, cu) = (r.max(0) as usize, c.max(0) as usize);
    let cols = b.rows.first().map(|x| x.len()).unwrap_or(1);
    let (a, cc) = (a.max(0) as usize, cc.max(0) as usize);
    match name {
        "tab" | "shift-tab" => {
            let flat = ru * cols + cu;
            if name == "tab" {
                if flat + 1 >= b.rows.len() * cols { table_act(key, id, "add-row"); return true; }
                let n = flat + 1;
                focus_cell_end(key, id, n / cols, n % cols);
            } else if flat > 0 {
                let n = flat - 1;
                focus_cell_end(key, id, n / cols, n % cols);
            }
            true
        }
        "enter" => {
            if ru + 1 < b.rows.len() { focus_cell_end(key, id, ru + 1, cu); } else { let n = insert_after(key, Some(id), Block::new("p", "")); focus_block(key, &n, At::Start); }
            true
        }
        "ctrl-b" => { mark(key, id, r, c, a, cc, Mark::B); true }
        "ctrl-i" => { mark(key, id, r, c, a, cc, Mark::I); true }
        "ctrl-u" => { mark(key, id, r, c, a, cc, Mark::U); true }
        "ctrl-e" => { mark(key, id, r, c, a, cc, Mark::Code); true }
        "ctrl-shift-s" => { mark(key, id, r, c, a, cc, Mark::S); true }
        "ctrl-k" => { open_fmt(key, id, r, c, a, cc, true); true }
        "ctrl-v" => paste(key, id, r, c, a, cc),
        "ctrl-shift-a" => { select_all(key); true }
        _ => false,
    }
}

fn focus_cell_end(key: &str, id: &str, r: usize, c: usize) {
    let n = cur_block(key, id).and_then(|b| b.rows.get(r).and_then(|x| x.get(c)).map(|s| s.len())).unwrap_or(0);
    request_focus(key, id, r as i32, c as i32, n, n);
}

fn table_act(key: &str, id: &str, a: &str) {
    let Some(b) = cur_block(key, id) else { return };
    let cols = b.rows.first().map(|x| x.len()).unwrap_or(2);
    let focused_here = with(|s| ed_mut(s, key, |e| e.focus.as_deref() == Some(id)).unwrap_or(false));
    let (cr, cc) = with(|s| ed_mut(s, key, |e| e.cell).unwrap_or((0, 0)));
    match a {
        "add-row" => {
            edit_block(key, id, |b| b.rows.push(vec![String::new(); cols]));
            let n = b.rows.len();
            request_focus(key, id, n as i32, 0, 0, 0);
        }
        "add-col" => {
            if cols >= 20 { kit::toast_error("A table can have up to 20 columns here"); return; }
            edit_block(key, id, |b| for r in b.rows.iter_mut() { r.push(String::new()); });
        }
        "del-row" => {
            if b.rows.len() <= 1 { return; }
            let i = if focused_here && cr < b.rows.len() { cr } else { b.rows.len() - 1 };
            edit_block(key, id, |b| { b.rows.remove(i); });
        }
        "del-col" => {
            if cols <= 1 { return; }
            let i = if focused_here && cc < cols { cc } else { cols - 1 };
            edit_block(key, id, |b| for r in b.rows.iter_mut() { if i < r.len() { r.remove(i); } });
        }
        "header" => {
            with(|s| if let Some(p) = idx(s, key) { s.places[p].md_cache.retain(|k, _| !k.starts_with(&format!("{id}:0:"))); });
            edit_block(key, id, |b| b.header = !b.header);
        }
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

fn set_sel(key: &str, a: &str, f: &str) {
    close_float();
    with(|s| ed_mut(s, key, |e| e.sel = Some((a.to_string(), f.to_string()))));
    request_focus(key, "__sel", -1, -1, 0, 0);
}

fn clear_sel(key: &str) {
    with(|s| ed_mut(s, key, |e| e.sel = None));
    render(key);
}

fn selected(key: &str) -> Vec<Block> {
    with(|s| ed_mut(s, key, |e| sel_range(e).map(|(lo, hi)| e.blocks[lo..=hi].to_vec()).unwrap_or_default()).unwrap_or_default())
}

fn select_all(key: &str) {
    let ends = with(|s| ed_mut(s, key, |e| (e.blocks.first().map(|b| b.id.clone()), e.blocks.last().map(|b| b.id.clone()))).unwrap_or((None, None)));
    if let (Some(a), Some(b)) = ends { set_sel(key, &a, &b); }
}

fn copy(text: &str) -> bool {
    match arboard::Clipboard::new() {
        Ok(mut cb) => { let ok = cb.set_text(text.to_string()).is_ok(); if ok { crate::app::keep_clipboard(cb); } ok }
        Err(_) => false,
    }
}

fn copy_md(md: &str) { if md.is_empty() { return; } if copy(md) { kit::toast("Copied as Markdown"); } else { kit::toast_error("Could not copy"); } }

fn delete_selected(key: &str) {
    let r = with(|s| ed_mut(s, key, |e| {
        let (lo, hi) = sel_range(e)?;
        e.blocks.drain(lo..=hi);
        e.sel = None;
        if e.blocks.is_empty() { e.blocks.push(Block::new("p", "")); }
        let before = lo.checked_sub(1).map(|i| e.blocks[i].id.clone());
        Some(match before { Some(b) => (b, true), None => (e.blocks[lo.min(e.blocks.len() - 1)].id.clone(), false) })
    }).flatten());
    let Some((id, end)) = r else { return };
    changed(key);
    render(key);
    focus_block(key, &id, if end { At::End } else { At::Start });
}

fn sel_act(key: &str, a: &str) {
    match a {
        "copy" => copy_md(&doc::to_markdown(&selected(key))),
        "all" => select_all(key),
        "delete" => delete_selected(key),
        "clear" => clear_sel(key),
        _ => {}
    }
}

/// Keys while blocks are selected.
fn sel_key(key: &str, name: &str) -> bool {
    let Some((sa, sf)) = with(|s| ed_mut(s, key, |e| e.sel.clone()).flatten()) else { return false };
    let ids: Vec<String> = with(|s| ed_mut(s, key, |e| e.blocks.iter().map(|b| b.id.clone()).collect()).unwrap_or_default());
    match name {
        "escape" => { clear_sel(key); focus_block(key, &sf, At::End); true }
        "backspace" | "delete" => { delete_selected(key); true }
        "ctrl-c" => { copy_md(&doc::to_markdown(&selected(key))); true }
        "ctrl-a" | "ctrl-shift-a" => { select_all(key); true }
        "shift-up" | "shift-down" => {
            let k = ids.iter().position(|i| *i == sf).unwrap_or(0) as i64 + if name == "shift-up" { -1 } else { 1 };
            if k >= 0 && (k as usize) < ids.len() { set_sel(key, &sa, &ids[k as usize]); }
            true
        }
        "up" | "down" => {
            let sel = selected(key);
            let t = if name == "up" { sel.first() } else { sel.last() }.map(|b| b.id.clone());
            clear_sel(key);
            if let Some(t) = t { focus_block(key, &t, if name == "up" { At::Start } else { At::End }); }
            true
        }
        "enter" => { clear_sel(key); focus_block(key, &sf, At::End); true }
        _ => false,
    }
}

/// A press in a block's margin (Shift: select from the caret's block).
fn blk_press(key: &str, id: &str, shift: bool) {
    if shift {
        let from = with(|s| ed_mut(s, key, |e| e.sel.as_ref().map(|x| x.0.clone()).or_else(|| e.focus.clone())).flatten());
        if let Some(f) = from.filter(|f| f != id && f != "title") { set_sel(key, &f, id); return; }
    }
    if with(|s| ed_mut(s, key, |e| e.sel.is_some()).unwrap_or(false)) { clear_sel(key); }
    with(|s| s.press = Some((key.to_string(), id.to_string(), false)));
    measure(key);
}

/// Dragging from a margin across blocks selects them.
fn blk_over(key: &str, y: f32) {
    let Some((k, from, moved)) = with(|s| s.press.clone()) else { return };
    if k != key { return; }
    let under = with(|s| idx(s, key).and_then(|i| s.places[i].boxes.iter().find(|b| y >= b.1 && y < b.1 + b.2).map(|b| b.0.clone())));
    let Some(under) = under else { return };
    if under != from || moved {
        with(|s| s.press = Some((k, from.clone(), true)));
        set_sel(key, &from, &under);
    }
}

fn measure(key: &str) {
    with(|s| if let Some(i) = idx(s, key) { s.places[i].boxes.clear(); s.places[i].measure_gen += 1; });
    render(key);
}

// ─── Dragging blocks: within the page to move them, onto a chat or a
// terminal to send them as Markdown ───

fn drag_start(key: &str, id: &str) {
    close_float();
    let (k, ids, text) = if key == "__fmt" {
        // The format bar's grip: the selected text.
        match with(|s| s.float.clone()) {
            Some(Float::Fmt { key, id, r, c, a, b, .. }) => {
                let src = src_of(&key, &id, r, c).unwrap_or_default();
                let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b)));
                let (_, rest) = doc::split_runs(&doc::src_to_runs(&src), pa);
                let (mid, _) = doc::split_runs(&rest, pb - pa);
                (key, vec![], doc::runs_md(&mid).trim().to_string())
            }
            _ => return,
        }
    } else if id.is_empty() {
        let sel = selected(key);
        (key.to_string(), sel.iter().map(|b| b.id.clone()).collect(), doc::to_markdown(&sel))
    } else {
        let sel = selected(key);
        if sel.len() > 1 && sel.iter().any(|b| b.id == id) {
            (key.to_string(), sel.iter().map(|b| b.id.clone()).collect(), doc::to_markdown(&sel))
        } else {
            let b = cur_block(key, id).into_iter().collect::<Vec<_>>();
            (key.to_string(), vec![id.to_string()], doc::to_markdown(&b))
        }
    };
    let label = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").chars().take(60).collect::<String>();
    with(|s| s.drag = Some(Drag { key: k.clone(), ids, text, spot: None, target: None, selecting: false }));
    ng(|n| { n.set_drag_label(label.into()); });
    measure(&k);
}

fn drag_move(_key: &str, x: f32, y: f32) {
    let Some(d) = with(|s| s.drag.clone()) else { return };
    ng(|n| { n.set_drag_on(true); n.set_drag_x(x); n.set_drag_y(y); });
    let _ = d.selecting;
    let key = d.key.clone();
    // Over this page: where the blocks would land.
    let spot = with(|s| idx(s, &key).and_then(|i| {
        let p = &s.places[i];
        let (bx, by, bw, bh) = p.page_box;
        if d.ids.is_empty() || x < bx || x > bx + bw || y < by || y > by + bh { return None; }
        let ed = p.ed.as_ref()?;
        let order: Vec<(usize, &(String, f32, f32))> = ed.blocks.iter().enumerate().filter(|(_, b)| !d.ids.contains(&b.id)).filter_map(|(k, b)| p.boxes.iter().find(|x| x.0 == b.id).map(|x| (k, x))).collect();
        if order.is_empty() { return None; }
        let mut spot = (order[order.len() - 1].0, false, order[order.len() - 1].1 .1 + order[order.len() - 1].1 .2);
        for (k, b) in &order { if y < b.1 + b.2 / 2.0 { spot = (*k, true, b.1); break; } }
        Some(spot)
    }));
    let target = if spot.is_none() { pane_at(&key, x, y) } else { None };
    with(|s| {
        if let Some(dd) = s.drag.as_mut() { dd.spot = spot.map(|x| (x.0, x.1)); dd.target = target.clone(); }
        if let Some(i) = idx(s, &key) { s.places[i].drop_y = spot.map(|x| x.2); }
    });
    render(&key);
}

/// A chat or terminal pane under a window point (the panes are placed in
/// the workbench; this page's own place there gives its origin).
fn pane_at(key: &str, x: f32, y: f32) -> Option<String> {
    if key.is_empty() { return None; }
    let w = kit::ui()?;
    let panes: Vec<_> = w.get_panes().iter().collect();
    let me = panes.iter().find(|p| p.key == key)?;
    let (ex, ey, _, _) = with(|s| idx(s, key).map(|i| s.places[i].page_box)).unwrap_or_default();
    let head = w.global::<crate::Theme>().get_pane_head();
    let inset = head + if w.global::<PlanningNotes>().get_head_hosted() { 0.0 } else { head };
    let (ox, oy) = (ex - me.x, ey - me.y - inset);
    let (px, py) = (x - ox, y - oy);
    panes.iter().find(|p| p.key != key && (p.kind == "chat" || p.kind == "terminal") && px >= p.x && px < p.x + p.w && py >= p.y && py < p.y + p.h).map(|p| p.key.to_string())
}

fn drag_end(commit: bool) {
    let Some(d) = with(|s| s.drag.take()) else { return };
    ng(|n| n.set_drag_on(false));
    with(|s| if let Some(i) = idx(s, &d.key) { s.places[i].drop_y = None; });
    render(&d.key);
    if !commit { return; }
    if let Some((k, before)) = d.spot {
        let anchor = with(|s| ed_mut(s, &d.key, |e| e.blocks.get(k).map(|b| b.id.clone())).flatten());
        with(|s| ed_mut(s, &d.key, |e| {
            let moving: Vec<Block> = e.blocks.iter().filter(|b| d.ids.contains(&b.id)).cloned().collect();
            e.blocks.retain(|b| !d.ids.contains(&b.id));
            let at = anchor.and_then(|a| e.blocks.iter().position(|b| b.id == a)).map(|i| if before { i } else { i + 1 }).unwrap_or(e.blocks.len());
            for (j, b) in moving.into_iter().enumerate() { e.blocks.insert(at + j, b); }
        }));
        changed(&d.key);
        render(&d.key);
        return;
    }
    if let Some(t) = d.target {
        if t.starts_with("live:") {
            crate::app::insert_text(&t, &d.text);
            crate::app::focus_composer(&t);
            kit::toast("Added to chat");
        } else if let Some(id) = t.strip_prefix("term:") {
            let data = if d.text.contains('\n') { format!("\u{1b}[200~{}\u{1b}[201~", d.text) } else { format!("{} ", d.text) };
            kit::ws_send(&json!({ "type": "input", "id": id, "data": data }));
        }
    }
}

// ─── Menus ───

fn mi(id: &str, k: &str, name: &str, d: &str, check: bool, danger: bool, sep: bool) -> PlanningNotesMenuItem {
    PlanningNotesMenuItem { id: id.into(), k: k.into(), name: name.into(), d: d.into(), check, danger, sep }
}

#[allow(clippy::too_many_arguments)]
fn show_menu(x: f32, y: f32, label: &str, items: Vec<PlanningNotesMenuItem>, act: i32, query_on: bool, query: &str, empty: &str) {
    ng(|n| {
        n.set_menu_x(x);
        n.set_menu_y(y);
        n.set_menu_label(label.into());
        n.set_menu_items(ModelRc::new(VecModel::from(items)));
        n.set_menu_act(act);
        n.set_menu_query_on(query_on);
        if !query_on { n.set_menu_query("".into()); } else if n.get_menu_query() != query { n.set_menu_query(query.into()); }
        n.set_menu_empty(empty.into());
        n.set_menu_open(true);
    });
}

fn close_float() {
    let f = with(|s| s.float.take());
    ng(|n| { n.set_menu_open(false); n.set_fmt_open(false); n.set_fmt_link(false); });
    if let Some(Float::Block { key, .. }) = f { render(&key); }
}

fn close_float_of(key: &str) {
    let mine = with(|s| match &s.float { Some(Float::Slash { key: k, .. }) | Some(Float::Block { key: k, .. }) | Some(Float::Lang { key: k, .. }) | Some(Float::Fmt { key: k, .. }) => k == key, Some(Float::Item { .. }) => key.is_empty(), None => false });
    if mine { close_float(); }
}

/// The slash menu ("/" typed, or the + button): turn into, or add.
fn open_slash(key: &str, id: &str, start: Option<usize>) {
    close_float();
    with(|s| s.float = Some(Float::Slash { key: key.into(), id: id.into(), start, query: String::new() }));
    draw_slash();
}

fn draw_slash() {
    let Some(Float::Slash { key, query, .. }) = with(|s| s.float.clone()) else { return };
    let items = doc::slash_items(&query);
    with(|s| s.menu_ids = items.iter().map(|t| t.0.to_string()).collect());
    let rows: Vec<PlanningNotesMenuItem> = items.iter().map(|t| mi(t.0, t.1, t.2, t.3, false, false, false)).collect();
    let label = if query.is_empty() { "Turn this into, or add".to_string() } else { format!("Blocks matching \u{201c}{query}\u{201d}") };
    let (x, y) = with(|s| s.caret_xy);
    let line = kit::ui().map(|w| w.global::<crate::Theme>().get_text_15()).unwrap_or(15.0) * with(|s| s.font) as f32 / FONT_BASE as f32;
    let act = kit::ui().map(|w| w.global::<PlanningNotes>().get_menu_act()).unwrap_or(0).clamp(0, (rows.len() as i32 - 1).max(0));
    let _ = key;
    show_menu(x, y + line * 1.6 + 6.0, &label, rows, if items.is_empty() { -1 } else { act }, false, "", "No such block");
}

fn choose_slash(t: &str) {
    let Some(Float::Slash { key, id, start, query }) = with(|s| s.float.clone()) else { return };
    close_float();
    // Drop the "/query" that was typed, then convert (or add below when this block has text).
    if let (Some(st), Some(b)) = (start, cur_block(&key, &id)) {
        let runs = doc::src_to_runs(&b.text);
        let plain: Vec<char> = doc::plain(&runs).chars().collect();
        let cut: Vec<char> = format!("/{query}").chars().collect();
        if plain.len() >= st + cut.len() && plain[st..st + cut.len()] == cut[..] {
            let (l, rest) = doc::split_runs(&runs, st);
            let (_, r) = doc::split_runs(&rest, cut.len());
            set_text(&key, &id, doc::runs_to_src(&doc::norm(l.into_iter().chain(r).collect())));
        }
    }
    let Some(b) = cur_block(&key, &id) else { return };
    let has_text = !doc::src_plain(&b.text).trim().is_empty();
    let target = if has_text && doc::texty(&b.kind) { insert_after(&key, Some(&id), Block::new("p", "")) } else { id.clone() };
    convert(&key, &target, t, false, None);
    if t == "divider" { let n = insert_after(&key, Some(&target), Block::new("p", "")); focus_block(&key, &n, At::Start); }
    else { focus_block(&key, &target, if t == "table" { At::Start } else { At::End }); }
}

/// The grip's menu: Turn into, Move up, Move down, Duplicate, Delete.
fn block_menu(key: &str, id: &str, x: f32, y: f32) {
    close_float();
    let Some(b) = cur_block(key, id) else { return };
    let mut items = vec![];
    let mut ids = vec![];
    for t in doc::TYPES.iter().filter(|t| t.0 != b.kind) { items.push(mi(t.0, t.1, t.2, "", false, false, false)); ids.push(format!("to:{}", t.0)); }
    items.push(mi("up", "\u{2191}", "Move up", "", false, false, true)); ids.push("up".into());
    items.push(mi("down", "\u{2193}", "Move down", "", false, false, false)); ids.push("down".into());
    items.push(mi("dup", "\u{2398}", "Duplicate", "", false, false, false)); ids.push("dup".into());
    items.push(mi("del", "\u{2715}", "Delete", "", false, true, false)); ids.push("del".into());
    with(|s| { s.float = Some(Float::Block { key: key.into(), id: id.into() }); s.menu_ids = ids; });
    show_menu(x, y, "Turn into", items, -1, false, "", "");
    render(key);
}

fn block_menu_do(key: &str, id: &str, what: &str) {
    if let Some(t) = what.strip_prefix("to:") { convert(key, id, t, false, None); focus_block(key, id, At::End); return; }
    match what {
        "up" | "down" => {
            let moved = with(|s| ed_mut(s, key, |e| {
                let i = e.blocks.iter().position(|b| b.id == id)?;
                let j = if what == "up" { i.checked_sub(1)? } else { (i + 1 < e.blocks.len()).then_some(i + 1)? };
                e.blocks.swap(i, j);
                Some(())
            }).flatten());
            if moved.is_some() { changed(key); render(key); }
        }
        "dup" => {
            let Some(mut b) = cur_block(key, id) else { return };
            b.id = doc::gen_id();
            let n = insert_after(key, Some(id), b);
            focus_block(key, &n, At::End);
        }
        "del" => {
            let (prev, next) = neighbours(key, id);
            with(|s| ed_mut(s, key, |e| remove_block(e, id)));
            changed(key);
            render(key);
            if let Some(p) = prev.or(next) { focus_block(key, &p.id, At::End); }
        }
        _ => {}
    }
}

fn menu_pick(i: i32) {
    let f = with(|s| s.float.clone());
    let id = with(|s| s.menu_ids.get(i.max(0) as usize).cloned());
    let Some(id) = id else { return };
    match f {
        Some(Float::Slash { .. }) => choose_slash(&id),
        Some(Float::Block { key, id: bid }) => { close_float(); block_menu_do(&key, &bid, &id); }
        Some(Float::Lang { key, id: bid, .. }) => {
            close_float();
            with(|s| if let Some(x) = idx(s, &key) { s.places[x].md_cache.remove(&bid); });
            edit_block(&key, &bid, |b| b.lang = if id == "plain" { String::new() } else { id.clone() });
            focus_block(&key, &bid, At::End);
        }
        Some(Float::Item { id: pid }) => {
            close_float();
            if id == "trash" {
                let p = pid.clone();
                page_act("delete", &pid, move |_| {
                    if with(|s| s.tab.current.as_deref() == Some(p.as_str())) { with(|s| s.tab.current = None); tab_empty("Moved to the trash. Pick a page, or start a new one."); }
                    tab_reload();
                });
            }
        }
        _ => close_float(),
    }
}

fn menu_query(q: &str) {
    if let Some(Float::Lang { key, id, .. }) = with(|s| s.float.clone()) {
        with(|s| s.float = Some(Float::Lang { key, id, query: q.to_string() }));
        draw_lang(-1.0, 0.0);
    }
}

fn menu_key(name: &str) -> bool {
    let n = with(|s| s.menu_ids.len()) as i32;
    let act = kit::ui().map(|w| w.global::<PlanningNotes>().get_menu_act()).unwrap_or(0);
    match name {
        "down" => { ng(|g| g.set_menu_act((act + 1).min(n - 1).max(0))); true }
        "up" => { ng(|g| g.set_menu_act((act - 1).max(0))); true }
        "enter" | "tab" => { if n > 0 { menu_pick(act.max(0)); } true }
        "escape" => {
            let f = with(|s| s.float.clone());
            close_float();
            if let Some(Float::Lang { key, id, .. }) = f { focus_block(&key, &id, At::End); }
            true
        }
        _ => false,
    }
}

// ─── The format bar ───

fn sel_moved(key: &str, id: &str, a: i32, c: i32, x: f32, y: f32) {
    with(|s| s.caret_xy = (x, y));
    let (a, c) = (a.max(0) as usize, c.max(0) as usize);
    let (r, cc) = with(|s| ed_mut(s, key, |e| if cur_block_kind(e, id) == "table" { (e.cell.0 as i32, e.cell.1 as i32) } else { (-1, -1) }).unwrap_or((-1, -1)));
    let kind = cur_block(key, id).map(|b| b.kind).unwrap_or_default();
    if kind == "code" { return; }
    let fl = with(|s| s.float.clone());
    if a != c {
        if !matches!(fl, Some(Float::Fmt { link: true, .. })) { open_fmt(key, id, r, cc, a, c, false); ng(|n| { n.set_fmt_x(x); n.set_fmt_y(y); }); }
    } else if matches!(fl, Some(Float::Fmt { link: false, .. })) {
        close_float();
    }
}

fn cur_block_kind(e: &Ed, id: &str) -> String { block_of(e, id).map(|(_, b)| b.kind.clone()).unwrap_or_default() }

fn open_fmt(key: &str, id: &str, r: i32, c: i32, a: usize, b: usize, link: bool) {
    let keep_menu = matches!(with(|s| s.float.clone()), Some(Float::Fmt { .. }));
    if !keep_menu { close_float(); }
    with(|s| s.float = Some(Float::Fmt { key: key.into(), id: id.into(), r, c, a, b, link }));
    let (x, y) = with(|s| s.caret_xy);
    let href = src_of(key, id, r, c).map(|src| { let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b))); doc::link_at(&doc::src_to_runs(&src), pa, pb) }).unwrap_or_default();
    ng(|n| {
        if !n.get_fmt_open() { n.set_fmt_x(x); n.set_fmt_y(y); }
        n.set_fmt_link(link);
        n.set_fmt_link_value(href.clone().into());
        n.set_fmt_has_link(!href.is_empty());
        n.set_fmt_gen(n.get_fmt_gen() + 1);
        n.set_fmt_open(true);
    });
}

fn fmt_act(a: &str) {
    let Some(Float::Fmt { key, id, r, c, a: sa, b: sb, .. }) = with(|s| s.float.clone()) else { return };
    let m = match a { "bold" => Some(Mark::B), "italic" => Some(Mark::I), "underline" => Some(Mark::U), "strike" => Some(Mark::S), "code" => Some(Mark::Code), _ => None };
    if let Some(m) = m { mark(&key, &id, r, c, sa, sb, m); return; }
    match a {
        "link" => open_fmt(&key, &id, r, c, sa, sb, true),
        "copy" => {
            let src = src_of(&key, &id, r, c).unwrap_or_default();
            let (pa, pb) = (doc::src_to_plain_at(&src, sa.min(sb)), doc::src_to_plain_at(&src, sa.max(sb)));
            let (_, rest) = doc::split_runs(&doc::src_to_runs(&src), pa);
            let (mid, _) = doc::split_runs(&rest, pb - pa);
            copy_md(doc::runs_md(&mid).trim());
        }
        "all" => select_all(&key),
        _ => {}
    }
}

fn link_apply(url: &str) {
    let Some(Float::Fmt { key, id, r, c, a, b, .. }) = with(|s| s.float.clone()) else { return };
    close_float();
    let Some(src) = src_of(&key, &id, r, c) else { return };
    let (pa, pb) = (doc::src_to_plain_at(&src, a.min(b)), doc::src_to_plain_at(&src, a.max(b)));
    let runs = doc::src_to_runs(&src);
    let href = (!url.trim().is_empty()).then(|| doc::fix_url(url));
    // A collapsed caret inside a link: the whole link.
    let (pa, pb) = if pa == pb { link_span(&runs, pa).unwrap_or((pa, pb)) } else { (pa, pb) };
    if pa == pb { refocus_range(&key, &id, r, c, a, b); return; }
    rewrite(&key, &id, r, c, doc::link(&runs, pa, pb, href), pa, pb);
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

fn open_link(u: &str) {
    if !(u.starts_with("http://") || u.starts_with("https://") || u.starts_with("mailto:")) { return; }
    if std::env::var_os("CRUNDI_TEST_PORT").is_some() { return; }
    let u = u.to_string();
    std::thread::spawn(move || { let _ = open::that(&u); });
}

// ─── The registry's functions ───

pub fn signed_in() { project(&kit::project()); }

pub fn signed_out() {
    close_float();
    with(|s| {
        for p in s.places.iter_mut() { let k = p.key.clone(); *p = place_new(&k); }
        s.tab = Tab { view: "pages".into(), ..Default::default() };
        s.project.clear();
    });
    sync_places();
    ng(|n| { n.set_tab_items(ModelRc::default()); n.set_tab_list_msg("".into()); n.set_drag_on(false); });
}

pub fn project(alias: &str) {
    let changed = with(|s| { let c = s.project != alias; s.project = alias.to_string(); c });
    if !changed { return; }
    // Save what waits, then every place starts again for the new project.
    let keys: Vec<String> = with(|s| s.places.iter().map(|p| p.key.clone()).collect());
    for k in &keys { flush(k); }
    close_float();
    close_tab();
    with(|s| for p in s.places.iter_mut() {
        let key = p.key.clone();
        *p = place_new(&key);
    });
    for k in keys.iter().filter(|k| !k.is_empty()) {
        if let Some((id, title)) = saved_pane(k) {
            with(|s| if let Some(i) = idx(s, k) { s.places[i].page = Some(id.clone()); s.places[i].title = title; });
            open_editor(k, &id, false);
        } else {
            reload_picker(k);
        }
    }
    let shown = kit::ui().map(|w| w.global::<crate::Shell>().get_tab() == "notes").unwrap_or(false);
    if shown { load_tab(); }
    ng(|n| n.set_tab_project(!alias.is_empty()));
    sync_places();
}

pub fn tab_shown(tab: &str) {
    let was = with(|s| std::mem::replace(&mut s.last_tab, tab.to_string()));
    if tab == "notes" { load_tab(); }
    else if was == "notes" { close_tab(); }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Server("notes", v) => {
            let p = kit::project().to_lowercase();
            if p.is_empty() || kit::s(v, "project") != p { return; }
            let v = (*v).clone();
            let keys: Vec<(String, bool)> = with(|s| s.places.iter().map(|x| (x.key.clone(), x.ed.is_some())).collect());
            for (k, has_ed) in keys {
                if k.is_empty() { continue; }
                if has_ed { remote_change(&k, &v); } else { reload_picker(&k); }
            }
            if with(|s| s.tab.open) {
                remote_change("", &v);
                // Our own typing saves often: refresh the list once it settles.
                let mine = kit::s(&v, "by") == with(|s| s.client_id.clone()) && kit::s(&v, "action") == "save";
                if mine {
                    let g = with(|s| { s.tab.list_gen += 1; s.tab.list_gen });
                    slint::Timer::single_shot(Duration::from_millis(1500), move || { if with(|s| s.tab.list_gen == g) { tab_reload(); } });
                } else { tab_reload(); }
            }
        }
        AreaEvent::Connected(true) => {
            let keys: Vec<String> = with(|s| s.places.iter().filter(|p| !p.key.is_empty() && p.ed.is_none()).map(|p| p.key.clone()).collect());
            for k in keys { reload_picker(&k); }
            if with(|s| s.tab.open) { tab_reload(); }
        }
        _ => {}
    }
}

// ─── For the test driver ───

pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let n = |i: usize| a(i).parse::<i32>().unwrap_or(0);
    let bid = |key: &str, i: usize| with(|s| ed_mut(s, key, |e| e.blocks.get(i).map(|b| b.id.clone())).flatten()).unwrap_or_default();
    Some(match name {
        "notes-attach" => { attach(&a(0)); json!({ "ok": true }) }
        "notes-new" => { new_page(&a(0)); json!({ "ok": true }) }
        "notes-pick" => { pick(&a(0), &a(1)); json!({ "ok": true }) }
        "notes-pages" => { switch_to_picker(&a(0)); json!({ "ok": true }) }
        "notes-search" => { search(&a(0), &a(1)); json!({ "ok": true }) }
        "notes-view" => { with(|s| s.tab.view = a(0)); ng(|g| g.set_tab_view(a(0).into())); draw_tab_list(); json!({ "ok": true }) }
        "notes-font" => { set_font(n(0)); json!({ "ok": true }) }
        "notes-title" => { title_edited(&a(0), &a(1)); json!({ "ok": true }) }
        // Type into block i: its whole new source and the caret (byte) after it.
        "notes-type" => { let k = a(0); let id = bid(&k, n(1) as usize); let t = a(2); let c = if a(3).is_empty() { t.len() as i32 } else { n(3) }; edited(&k, &id, &t, c); json!({ "ok": true, "id": id }) }
        "notes-key" => { let k = a(0); let id = bid(&k, n(1) as usize); let r = text_key(&k, &id, &a(2), n(3), n(4), a(5) != "false", a(6) != "false"); json!({ "ok": true, "handled": r }) }
        "notes-cell" => { let k = a(0); let id = bid(&k, n(1) as usize); cell_edited(&k, &id, n(2), n(3), &a(4)); json!({ "ok": true }) }
        "notes-cell-key" => { let k = a(0); let id = bid(&k, n(1) as usize); let r = cell_key(&k, &id, n(2), n(3), &a(4), n(5), n(6)); json!({ "ok": true, "handled": r }) }
        "notes-table" => { let k = a(0); let id = bid(&k, n(1) as usize); table_act(&k, &id, &a(2)); json!({ "ok": true }) }
        "notes-check" => { let k = a(0); let id = bid(&k, n(1) as usize); edit_block(&k, &id, |b| b.checked = a(2) == "true"); json!({ "ok": true }) }
        "notes-grip" => { let k = a(0); let id = bid(&k, n(1) as usize); block_menu(&k, &id, 300.0, 300.0); json!({ "ok": true }) }
        "notes-add-below" => { let k = a(0); let id = bid(&k, n(1) as usize); add_below(&k, &id); json!({ "ok": true }) }
        "notes-code" => { let k = a(0); let id = bid(&k, n(1) as usize); code_act(&k, &id, &a(2), 600.0, 300.0); json!({ "ok": true }) }
        "notes-menu-pick" => { let i = with(|s| s.menu_ids.iter().position(|x| *x == a(0))).map(|i| i as i32).unwrap_or(-1); if i >= 0 { menu_pick(i); } json!({ "ok": i >= 0 }) }
        "notes-menu-key" => { json!({ "ok": true, "handled": menu_key(&a(0)) }) }
        "notes-menu-query" => { menu_query(&a(0)); json!({ "ok": true }) }
        "notes-menu-close" => { close_float(); json!({ "ok": true }) }
        "notes-fmt" => { let k = a(0); let id = bid(&k, n(1) as usize); with(|s| s.caret_xy = (500.0, 300.0)); open_fmt(&k, &id, -1, -1, n(2) as usize, n(3) as usize, false); json!({ "ok": true }) }
        "notes-fmt-act" => { fmt_act(&a(0)); json!({ "ok": true }) }
        "notes-link" => { link_apply(&a(0)); json!({ "ok": true }) }
        "notes-select" => { let k = a(0); let x = bid(&k, n(1) as usize); let y = bid(&k, n(2) as usize); set_sel(&k, &x, &y); json!({ "ok": true }) }
        "notes-sel-act" => { sel_act(&a(0), &a(1)); json!({ "ok": true }) }
        "notes-sel-key" => { json!({ "ok": true, "handled": sel_key(&a(0), &a(1)) }) }
        "notes-move" => {
            // Move block i before block j (the grip's drag, done directly).
            let k = a(0); let id = bid(&k, n(1) as usize);
            with(|s| s.drag = Some(Drag { key: k.clone(), ids: vec![id], text: String::new(), spot: Some((n(2) as usize, true)), target: None, selecting: false }));
            drag_end(true);
            json!({ "ok": true })
        }
        "notes-item-menu" => { item_menu(&a(0), 400.0, 300.0); json!({ "ok": true }) }
        "notes-trash" => { trash_act(&a(0), &a(1)); json!({ "ok": true }) }
        "notes-flush" => { flush(&a(0)); json!({ "ok": true }) }
        "notes-reload" => { tab_reload(); let keys: Vec<String> = with(|s| s.places.iter().filter(|p| !p.key.is_empty() && p.ed.is_none()).map(|p| p.key.clone()).collect()); for k in keys { reload_picker(&k); } json!({ "ok": true }) }
        "notes-hosted" => { ng(|g| g.set_head_hosted(a(0) == "true")); json!({ "ok": true }) }
        _ => return None,
    })
}

pub fn props() -> Value {
    let Some(w) = kit::ui() else { return Value::Null };
    let g = w.global::<PlanningNotes>();
    let places: Vec<Value> = g.get_panes().iter().map(|p| json!({
        "key": p.key.as_str(), "mode": p.mode.as_str(), "msg": p.msg.as_str(), "pageId": p.page_id.as_str(), "title": p.title.as_str(),
        "save": p.save_state.as_str(), "saveText": p.save_text.as_str(), "saveTip": p.save_tip.as_str(), "selCount": p.sel_count,
        "focus": p.focus_id.as_str(), "focusA": p.focus_a, "focusB": p.focus_b, "curId": p.cur_id.as_str(), "drop": p.drop_on,
        "headTitle": pane_title(p.key.as_str()),
        "blocks": p.blocks.iter().map(|b| json!({ "id": b.id.as_str(), "kind": b.kind.as_str(), "src": b.src.as_str(), "num": b.num.as_str(), "checked": b.checked,
            "lang": b.lang_name.as_str(), "wrap": b.wrap, "header": b.header, "sel": b.sel, "menuOpen": b.menu_open,
            "rows": b.rows.iter().map(|r| r.cells.iter().map(|c| c.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "items": p.items.iter().map(|i| json!({ "id": i.id.as_str(), "title": i.title.as_str(), "meta": i.meta.as_str(), "busy": i.busy, "preview": i.preview.as_str() })).collect::<Vec<_>>(),
        "listMsg": p.list_msg.as_str(), "newBusy": p.new_busy,
    })).collect();
    let saved: Value = with(|s| {
        let k = s.places.iter().find(|p| p.key.is_empty()).and_then(|p| p.ed.as_ref().map(|e| json!({ "blocks": e.blocks.iter().map(|b| b.to_json()).collect::<Vec<_>>(), "dirty": e.dirty, "version": e.version })));
        k.unwrap_or(Value::Null)
    });
    let float = with(|s| match &s.float {
        Some(Float::Slash { .. }) => "slash", Some(Float::Block { .. }) => "block", Some(Float::Lang { .. }) => "lang", Some(Float::Item { .. }) => "item", Some(Float::Fmt { link: true, .. }) => "link", Some(Float::Fmt { .. }) => "fmt", None => "",
    });
    json!({
        "ready": g.get_ready(), "scale": g.get_scale(), "font": with(|s| s.font), "hosted": g.get_head_hosted(),
        "tab": { "project": g.get_tab_project(), "view": g.get_tab_view().as_str(), "open": with(|s| s.tab.open), "current": with(|s| s.tab.current.clone()),
            "items": g.get_tab_items().iter().map(|i| json!({ "id": i.id.as_str(), "title": i.title.as_str(), "meta": i.meta.as_str(), "busy": i.busy, "cur": i.cur, "preview": i.preview.as_str() })).collect::<Vec<_>>(),
            "listMsg": g.get_tab_list_msg().as_str(), "newBusy": g.get_tab_new_busy() },
        "places": places,
        "tabPage": saved,
        "float": float,
        "menu": if g.get_menu_open() { json!({ "label": g.get_menu_label().as_str(), "act": g.get_menu_act(), "query": g.get_menu_query().as_str(), "queryOn": g.get_menu_query_on(), "empty": g.get_menu_empty().as_str(),
            "items": g.get_menu_items().iter().map(|i| json!({ "id": i.id.as_str(), "k": i.k.as_str(), "name": i.name.as_str(), "d": i.d.as_str(), "check": i.check, "danger": i.danger })).collect::<Vec<_>>() }) } else { Value::Null },
        "fmt": if g.get_fmt_open() { json!({ "link": g.get_fmt_link(), "value": g.get_fmt_link_value().as_str(), "hasLink": g.get_fmt_has_link() }) } else { Value::Null },
        "drag": g.get_drag_on(),
    })
}
