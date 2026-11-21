//! Kanban (columns, cards, subtasks, drag to move, Trash, History) and the
//! card dialog: the web's loadKanban, renderKanban, setupKanbanHandlers and
//! kanbanPost (src/webapp-html.js), as native-gpui's planning/kanban.rs has
//! them, on v2's rules.
//!
//! - `KStore` (an entity) holds the board as the server sent it and what the
//!   screen keeps (the view, the open menus, the blank card, the attachments
//!   strip); it derives the columns' cards (`board::columns`) once per change
//!   and notifies once. `Drag` (its own entity) holds a press and a drag on
//!   their way, so a pointer move redraws the dragged card's clone and the
//!   columns whose drop line moved, nothing else.
//! - view.rs: per host (the tab, a workbench pane) a cached toolbar, one
//!   cached view per column (a virtualised list of its cards), one list for
//!   the stacked board, the Trash and History lists, and the overlay (the
//!   drag's clone, the attachments strip). ask.rs: the card dialog.
//! - board.rs: the pure rules (unit-tested).

pub mod ask;
pub mod board;
mod card_el;
mod view;

use crate::util::{self, enc, prefs, s, truthy};
use ask::Choice;
use board::{Box4, Card, Col, Row};
use crundi_core::live::Event;
use crundi_shell::Stores;
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::Rc, sync::Arc};

pub use view::Host;

/// A press on a card that may become a drag.
pub(crate) struct Press { pub id: String, pub title: String, pub at: Point<Pixels>, pub card: Box4 }

/// A drag on its way: the card, where it was taken, the pointer.
#[derive(Clone)]
pub(crate) struct DragNow { pub id: String, pub title: String, pub grab: (f32, f32), pub w: f32, pub h: f32, pub x: f32, pub y: f32 }

/// The drag's state (its own entity: a move notifies its watchers only).
#[derive(Default)]
pub struct Drag {
    pub(crate) press: Option<Press>,
    pub(crate) drag: Option<DragNow>,
    /// Where it would land: (status, index, the card the line goes before).
    pub(crate) target: Option<(String, usize, String)>,
}

/// A card's attachments strip, against its paperclip (`btn`: the button's
/// box on the window; `w`: the card's width), as the web's placePop.
#[derive(Clone, PartialEq)]
pub(crate) struct Clip { pub task: String, pub btn: Box4, pub w: f32 }

/// Every column's and card's box on the window, as last painted (for the
/// drop target and the driver). Written while painting, never notified.
#[derive(Default)]
pub(crate) struct Boxes {
    /// (status, box): a column's, or in the stacked board each of its rows'.
    pub cols: Vec<(String, Box4)>,
    /// (status, id, box).
    pub cards: Vec<(String, String, Box4)>,
    /// A card at rest's controls: (card id, control, box) (for the driver).
    pub controls: Vec<(String, String, Box4)>,
    /// A press began on a card at rest: it stays at rest until the release
    /// (its click is its own, card_el.rs).
    pub pressing: bool,
    /// A press began on a subtask row (its own drag; the card's press skips it).
    pub sub_press: bool,
}

pub struct KStore {
    pub(crate) board: Option<Value>,
    pub(crate) project: String,
    /// "board", "trash", "history".
    pub(crate) view: String,
    /// "none" (no project), "loading", "ok", "error".
    pub(crate) state: String,
    pub(crate) error: String,
    pub(crate) menu_open: HashSet<String>,
    pub(crate) adding_task: Option<String>,
    pub(crate) adding_todo: Option<String>,
    pub(crate) media_task: HashMap<String, Vec<Value>>,
    pub(crate) media_todo: HashMap<String, Vec<Value>>,
    pub(crate) thumbs: HashMap<String, Option<Arc<Image>>>,
    load_gen: u64,
    pub(crate) clip: Option<Clip>,
    pub(crate) task_input: Option<Entity<w::InputState>>,
    pub(crate) todo_input: Option<Entity<w::InputState>>,
    subs: Vec<Subscription>,
    // What the views draw, derived once per change.
    pub(crate) cols: Vec<Rc<Col>>,
    pub(crate) trash: Rc<(Vec<Row>, Vec<Row>)>,
    pub(crate) history: Rc<Vec<Row>>,
    pub(crate) boxes: Rc<RefCell<Boxes>>,
    pub drag: Entity<Drag>,
    /// The driver's "kanban-all-live": every card with its live controls
    /// (to check that a card at rest looks the same as a live one).
    pub(crate) all_live: bool,
    /// A card shown from elsewhere (Cru's show_in_crundi): its id and the
    /// flash's count (each show flashes again), until the flash is over.
    pub(crate) flash: Option<(String, u64)>,
    /// The last load that came back (its load_gen).
    loaded_gen: u64,
}

impl KStore {
    fn new(cx: &mut Context<Self>) -> Self {
        KStore {
            board: None, project: String::new(), view: "board".into(), state: "loading".into(), error: String::new(),
            menu_open: HashSet::new(), adding_task: None, adding_todo: None, media_task: HashMap::new(), media_todo: HashMap::new(),
            thumbs: HashMap::new(), load_gen: 0, clip: None, task_input: None, todo_input: None, subs: vec![],
            cols: vec![], trash: Rc::default(), history: Rc::default(), boxes: Rc::default(), drag: cx.new(|_| Drag::default()), all_live: false, flash: None, loaded_gen: 0,
        }
    }

    /// The columns, the Trash and the History from the board and what the
    /// screen keeps (once per change; a card that did not change keeps its
    /// Rc, so a column's list splices only the cards that did).
    fn derive(&mut self) {
        let Some(b) = self.board.as_ref() else { self.cols.clear(); self.trash = Rc::default(); self.history = Rc::default(); return };
        let mt: HashMap<String, usize> = self.media_task.iter().map(|(a, v)| (a.clone(), v.len())).collect();
        let mtd: HashMap<String, usize> = self.media_todo.iter().map(|(a, v)| (a.clone(), v.len())).collect();
        let ui = board::Ui { menu_open: Some(&self.menu_open), adding_task: self.adding_task.as_deref(), adding_todo: self.adding_todo.as_deref(), media_task: Some(&mt), media_todo: Some(&mtd) };
        let cols = board::columns(b, &ui);
        self.cols = cols.into_iter().enumerate().map(|(i, c)| match self.cols.get(i) { Some(old) if **old == c => old.clone(), _ => Rc::new(c) }).collect();
        let t = board::trash(b);
        if *self.trash != t { self.trash = Rc::new(t); }
        let h = board::history(b, board::local_time);
        if *self.history != h { self.history = Rc::new(h); }
    }

    fn task(&self, id: &str) -> Option<Value> {
        self.board.as_ref()?.get("tasks").and_then(|t| t.as_array()).and_then(|a| a.iter().find(|t| s(t, "id") == id)).cloned()
    }
}

struct Handle(Entity<KStore>);
impl Global for Handle {}

/// The store (handlers only; a view holds its handle).
pub fn store(cx: &App) -> Entity<KStore> { cx.global::<Handle>().0.clone() }
pub(crate) fn with<R>(cx: &mut App, f: impl FnOnce(&mut KStore) -> R) -> R { store(cx).update(cx, |k, _| f(k)) }
/// What changed is drawn: derive once, notify once.
pub(crate) fn refresh(cx: &mut App) { store(cx).update(cx, |k, cx| { k.derive(); cx.notify(); }); }
fn drag_with<R>(cx: &mut App, f: impl FnOnce(&mut Drag) -> R) -> R { let d = store(cx).read(cx).drag.clone(); d.update(cx, |d, cx| { let r = f(d); cx.notify(); r }) }

/// At start: the store and the workbench pane kind panel:kanban.
pub fn init(cx: &mut App) {
    let st = cx.new(KStore::new);
    cx.set_global(Handle(st));
    crundi_workbench::register_kind(cx, crundi_workbench::PaneKind {
        prefix: "panel:kanban",
        chat: false,
        make: Rc::new(|_, stores, window, cx| {
            let host = Host::new(stores, true, window, cx);
            // The board now (a pane opened, or restored with the layout).
            load(cx);
            crundi_workbench::PaneParts {
                body: Rc::new(move |cx| host.body(cx)),
                info: Rc::new(|_| crundi_workbench::HeadInfo { title: "Kanban".into(), long: String::new(), state: "idle".into() }),
                watch: Box::new(|_| vec![]),
            }
        }),
        close: None,
        zoom: None,
    });
    // The pane's head as the web's headHtmlPanel: its icon, Refresh.
    crundi_workbench::register_head(cx, "panel:kanban", crundi_workbench::KindHead::new().icon(crundi_theme::IconName::Kanban)
        .button(crundi_workbench::HeadButton::icon("refresh", crundi_theme::IconName::RefreshCw, "Refresh"))
        .on_action(|_, action, _, cx| if action == "refresh" { load(cx); }));
}

/// The Kanban tab's view (layout: its body places the cached parts).
pub fn tab(stores: &Stores, window: &mut Window, cx: &mut App) -> AnyView {
    let host = Host::new(stores, false, window, cx);
    load(cx);
    cx.new(|cx| view::Tab::new(host, cx)).into()
}

/// The board is on show: its tab, or a pane on the workbench.
fn visible(cx: &mut App) -> bool {
    let tab = util::current_tab(cx);
    tab == "kanban" || (tab == "workbench" && util::pane_shown(cx, "panel:kanban"))
}

pub fn on_batch(_: &Stores, batch: &[Event], cx: &mut App) {
    let p = crundi_shell::project(cx).to_lowercase();
    let mut again = false;
    for e in batch {
        match e {
            Event::Server(n, v) if n == "kanban" => { if !p.is_empty() && s(v, "project").to_lowercase() == p { again = true; } }
            Event::Connected(true) => again = true,
            _ => {}
        }
    }
    if again && visible(cx) { load(cx); }
}

pub fn on_project(_: &Stores, alias: &str, cx: &mut App) {
    let changed = with(cx, |k| {
        let c = k.project != alias;
        k.project = alias.to_string();
        if c { k.board = None; k.menu_open.clear(); k.adding_task = None; k.adding_todo = None; k.clip = None; k.state = if alias.is_empty() { "none".into() } else { "loading".into() }; }
        c
    });
    if changed { refresh(cx); }
    load(cx);
}

pub fn on_signed_in(stores: &Stores, cx: &mut App) { let p = crundi_shell::project(cx); on_project(stores, &p, cx); }

pub fn on_signed_out(_: &Stores, cx: &mut App) {
    with(cx, |k| {
        k.board = None; k.menu_open.clear(); k.adding_task = None; k.adding_todo = None; k.media_task.clear(); k.media_todo.clear();
        k.thumbs.clear(); k.view = "board".into(); k.state = "loading".into(); k.clip = None; k.project.clear();
    });
    drag_with(cx, |d| { d.drag = None; d.press = None; d.target = None; });
    if ask::is_open(cx) { cx.defer(|cx| util::in_window(cx, ask::cancel)); }
    refresh(cx);
}

/// Load the board now; its load's number (for `show_card`'s `after`).
pub fn load_now(cx: &mut App) -> u64 { load(cx); store(cx).read(cx).load_gen }

/// The board again, from the server (with its media).
pub fn load(cx: &mut App) {
    let p = crundi_shell::project(cx);
    if p.is_empty() { with(cx, |k| { k.board = None; k.state = "none".into(); }); return refresh(cx); }
    let trash = with(cx, |k| k.view == "trash");
    let lg = with(cx, |k| { k.load_gen += 1; k.load_gen });
    let path = format!("/api/kanban?project={}{}", enc(&p), if trash { "&includeDeleted=1" } else { "" });
    let mpath = format!("/api/media/list?project={}&includeGeneral=1&kind=kanban", enc(&p));
    crundi_shell::call(cx, move |api| {
        let b = api.get(&path)?;
        let m = api.get(&mpath).unwrap_or(Value::Null);
        Ok(json!({ "b": b, "m": m }))
    }, move |r, cx| {
        if with(cx, |k| k.load_gen != lg) || crundi_shell::project(cx) != p { return; }
        with(cx, |k| k.loaded_gen = lg);
        match r {
            Err(e) => with(cx, |k| { k.board = None; k.error = format!("Failed to load board: {e}"); k.state = "error".into(); }),
            Ok(v) => {
                let b = &v["b"];
                if !truthy(&b["ok"]) {
                    let e = s(b, "error");
                    with(cx, |k| { k.error = if e.is_empty() { "Failed to load".into() } else { e }; k.state = "error".into(); });
                } else {
                    let items = v["m"].get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    let (bt, btd) = board::media_groups(&items);
                    with(cx, |k| { k.board = Some(b["board"].clone()); k.media_task = bt; k.media_todo = btd; k.state = "ok".into(); });
                    load_thumbs(cx);
                }
            }
        }
        refresh(cx);
    });
}

/// POST /api/kanban with the project; a refusal says so in a toast.
pub(crate) fn post(cx: &mut App, mut payload: Value, then: impl FnOnce(Value, &mut App) + 'static) {
    payload["project"] = json!(crundi_shell::project(cx));
    crundi_shell::call(cx, move |api| api.post("/api/kanban", &payload), move |r, cx| {
        let v = match r {
            Ok(v) => { if !truthy(&v["ok"]) { let e = s(&v, "error"); util::toast(cx, if e.is_empty() { "Action failed" } else { &e }, w::Toast::Error); } v }
            Err(e) => { util::toast(cx, &format!("Action failed: {e}"), w::Toast::Error); json!({ "ok": false }) }
        };
        then(v, cx);
    });
}

fn reload(_: Value, cx: &mut App) { load(cx); }

fn status_choice(b: &Value, value: &str, label: &str) -> Choice {
    Choice { label: label.into(), options: board::statuses(b).into_iter().map(|s| { let l = board::label(&s); (s, l) }).collect(), value: value.into() }
}

// ─── The blank card's and the new subtask's lines ───

fn input_value(e: &Option<Entity<w::InputState>>, cx: &App) -> String { e.as_ref().map(|i| i.read(cx).value().to_string()).unwrap_or_default() }

fn clear_input(e: Option<Entity<w::InputState>>, window: &mut Window, cx: &mut App) {
    if let Some(i) = e { i.update(cx, |s, cx| s.set_value("", window, cx)); }
}

/// The line (made once): Enter adds and stays open, leaving it adds and closes.
fn line(todo: bool, window: &mut Window, cx: &mut App) -> Entity<w::InputState> {
    if let Some(i) = with(cx, |k| if todo { k.todo_input.clone() } else { k.task_input.clone() }) { return i; }
    let ph = if todo { "New subtask, Enter to add" } else { "Task title, Enter to add" };
    let i = cx.new(|cx| w::InputState::new(window, cx).placeholder(ph));
    let sub = window.subscribe(&i, cx, move |e, ev: &w::InputEvent, window, cx| {
        let text = e.read(cx).value().to_string();
        match ev {
            w::InputEvent::PressEnter { .. } => { if !text.trim().is_empty() { commit(todo, text, true, window, cx); } }
            w::InputEvent::Blur => {
                let open = with(cx, |k| if todo { k.adding_todo.is_some() } else { k.adding_task.is_some() });
                if open { commit(todo, text, false, window, cx); }
            }
            _ => {}
        }
    });
    with(cx, |k| { if todo { k.todo_input = Some(i.clone()) } else { k.task_input = Some(i.clone()) }; k.subs.push(sub); });
    i
}

fn focus_line(todo: bool, window: &mut Window, cx: &mut App) {
    let i = line(todo, window, cx);
    i.update(cx, |s, cx| s.focus(window, cx));
}

/// The blank card's (or the subtask row's) text: add it; `keep` leaves the line open.
pub(crate) fn commit(todo: bool, text: String, keep: bool, window: &mut Window, cx: &mut App) {
    let text = text.trim().to_string();
    let (target, inp) = with(cx, |k| if todo { (k.adding_todo.clone(), k.todo_input.clone()) } else { (k.adding_task.clone(), k.task_input.clone()) });
    let target = target.unwrap_or_default();
    clear_input(inp, window, cx);
    if !keep { with(cx, |k| if todo { k.adding_todo = None } else { k.adding_task = None }); }
    refresh(cx);
    if text.is_empty() || target.is_empty() { return; }
    let body = if todo { json!({ "action": "addTodo", "taskId": target, "text": text }) } else { json!({ "action": "addTask", "title": text, "status": target }) };
    post(cx, body, reload);
}

/// Escape on the line: dropped, closed.
pub(crate) fn cancel_line(todo: bool, window: &mut Window, cx: &mut App) {
    let inp = with(cx, |k| { if todo { k.adding_todo = None } else { k.adding_task = None }; if todo { k.todo_input.clone() } else { k.task_input.clone() } });
    clear_input(inp, window, cx);
    refresh(cx);
}

// ─── The actions (the web's data-kact) ───

pub(crate) fn act(a: &str, task_id: &str, extra: &str, window: &mut Window, cx: &mut App) {
    let b = with(cx, |k| k.board.clone()).unwrap_or(Value::Null);
    match a {
        "view-board" | "view-trash" | "view-history" => {
            with(cx, |k| k.view = a.trim_start_matches("view-").to_string());
            refresh(cx);
            load(cx);
        }
        "media" => media_kind(cx),
        "card-menu" => { with(cx, |k| if !k.menu_open.remove(task_id) { k.menu_open.insert(task_id.to_string()); }); refresh(cx); }
        "add-task-inline" => {
            let stt = extra.to_string();
            let draft = with(cx, |k| k.task_input.clone());
            let draft = input_value(&draft, cx);
            let prev = with(cx, |k| { let p = k.adding_task.clone(); k.adding_task = Some(stt.clone()); p });
            let inp = with(cx, |k| k.task_input.clone());
            clear_input(inp, window, cx);
            // Another column's open card keeps what was typed in it.
            if let Some(prev) = prev.filter(|p| *p != stt && !draft.trim().is_empty()) {
                post(cx, json!({ "action": "addTask", "title": draft.trim(), "status": prev }), reload);
            }
            focus_line(false, window, cx);
            refresh(cx);
        }
        "add-subtask" => {
            let t = task_id.to_string();
            let draft = with(cx, |k| k.todo_input.clone());
            let draft = input_value(&draft, cx);
            let prev = with(cx, |k| { k.menu_open.remove(&t); let p = k.adding_todo.clone(); k.adding_todo = Some(t.clone()); p });
            let inp = with(cx, |k| k.todo_input.clone());
            clear_input(inp, window, cx);
            if let Some(prev) = prev.filter(|p| *p != t && !draft.trim().is_empty()) {
                post(cx, json!({ "action": "addTodo", "taskId": prev, "text": draft.trim() }), reload);
            }
            focus_line(true, window, cx);
            refresh(cx);
        }
        "add-task" => {
            let sts = board::statuses(&b);
            let mut last = prefs::get("kanban_add_status");
            if !sts.contains(&last) { last = sts.first().cloned().unwrap_or_default(); }
            ask::ask(window, cx, "Add task", "Task title", "", false, "Add task", Some(status_choice(&b, &last, "Column")), |r, _, cx| {
                let Some((v, ch)) = r else { return };
                if v.trim().is_empty() { return; }
                prefs::set("kanban_add_status", &ch);
                post(cx, json!({ "action": "addTask", "title": v.trim(), "status": ch }), reload);
            });
        }
        "edit-task" => {
            let Some(t) = with(cx, |k| k.task(task_id)) else { return };
            let id = task_id.to_string();
            let cur = s(&t, "status");
            ask::ask(window, cx, "Edit task", "Task title", &s(&t, "title"), false, "Next", Some(status_choice(&b, &cur, "Column")), move |r, window, cx| {
                let Some((title, ch)) = r else { return };
                ask::ask(window, cx, "Edit task", "Description", &s(&t, "description"), true, "Save", None, move |d, _, cx| {
                    let Some((description, _)) = d else { return };
                    let id2 = id.clone();
                    post(cx, json!({ "action": "updateTask", "taskId": id, "title": title.trim(), "description": description }), move |_, cx| {
                        if !ch.is_empty() && ch != cur { post(cx, json!({ "action": "moveTask", "taskId": id2, "status": ch }), reload); } else { load(cx); }
                    });
                });
            });
        }
        "del-task" => post(cx, json!({ "action": "deleteTask", "taskId": task_id }), reload),
        "restore-task" => post(cx, json!({ "action": "restoreTask", "taskId": task_id }), reload),
        "restore-todo" => post(cx, json!({ "action": "restoreTodo", "taskId": task_id, "todoId": extra }), reload),
        "brainstorm" => {
            let Some(t) = with(cx, |k| k.task(task_id)) else { return };
            let id = task_id.to_string();
            let p = crundi_shell::project(cx);
            let todos: Vec<(String, String)> = t.get("todos").and_then(|x| x.as_array()).map(|a| a.iter().filter(|td| !truthy(&td["deleted"])).map(|td| (s(td, "id"), s(td, "text"))).collect()).unwrap_or_default();
            let title = s(&t, "title");
            crundi_shell::call(cx, move |api| {
                let r = api.post("/api/mindmap", &json!({ "action": "addNode", "text": title, "project": p, "taskId": id }))?;
                if truthy(&r["ok"]) {
                    let node = s(&r["node"], "id");
                    for (tid, text) in &todos {
                        api.post("/api/mindmap", &json!({ "action": "addNode", "text": text, "parentId": node, "project": p, "taskId": id, "todoId": tid }))?;
                    }
                }
                Ok(json!({ "r": r, "n": todos.len() }))
            }, |r, cx| match r {
                Ok(v) if truthy(&v["r"]["ok"]) => {
                    let n = v["n"].as_u64().unwrap_or(0);
                    util::toast(cx, &format!("Added to Mindmap{}", if n > 0 { format!(" with {n} subtask(s)") } else { String::new() }), w::Toast::Success);
                    util::show_tab(cx, "mindmap");
                }
                Ok(v) => { let e = s(&v["r"], "error"); util::toast(cx, if e.is_empty() { "Action failed" } else { &e }, w::Toast::Error); }
                Err(e) => util::toast(cx, &format!("Action failed: {e}"), w::Toast::Error),
            });
        }
        "goto-mindmap" => util::show_tab(cx, "mindmap"),
        _ => {}
    }
}

/// A control's box as last painted (a card's paperclip, a subtask's "…"):
/// where its pop opens.
pub(crate) fn control_box(task: &str, ctl: &str, cx: &App) -> Option<Box4> {
    store(cx).read(cx).boxes.borrow().controls.iter().find(|(t, c, _)| t == task && c == ctl).map(|x| x.2)
}

/// A click on a control of a card at rest (card_el.rs): what the live one does.
pub(crate) fn click_control(task: &str, ctl: &str, btn: Box4, card_w: Pixels, window: &mut Window, cx: &mut App) {
    match ctl.split(':').collect::<Vec<_>>().as_slice() {
        ["clip"] => clip_open(task, btn, f32::from(card_w), cx),
        ["menu"] => act("card-menu", task, "", window, cx),
        ["mm"] => act("goto-mindmap", task, "", window, cx),
        ["sub"] => act("add-subtask", task, "", window, cx),
        ["tmedia", td] => media_todo(task, td, cx),
        ["tick", td, done] => toggle_todo(task, td, *done != "true", cx),
        ["more", td] => todo_menu(task, td, btn, cx),
        _ => {}
    }
}

pub(crate) fn toggle_todo(t: &str, td: &str, done: bool, cx: &mut App) {
    post(cx, json!({ "action": "updateTodo", "taskId": t, "todoId": td, "done": done }), reload);
}

/// The subtask's "…": the shared menu under the button, its right edge on
/// the button's (the web's .kt-pop, placePop).
pub(crate) fn todo_menu(t: &str, td: &str, btn: Box4, cx: &mut App) {
    let (x, y) = (btn.x + btn.w, btn.y + btn.h + 4.);
    let n = with(cx, |k| k.media_todo.get(td).map(|v| v.len()).unwrap_or(0));
    let (t, td) = (t.to_string(), td.to_string());
    let attach = if n > 0 { format!("Attach media  {n}") } else { "Attach media".to_string() };
    let items = vec![
        w::MenuEntry::new("media", attach).icon(w::IconName::Paperclip),
        w::MenuEntry::new("rename", "Rename").icon(w::IconName::Pencil),
        w::MenuEntry::new("delete", "Delete").icon(w::IconName::Trash).danger().sep(),
    ];
    let mx = (x - f32::from(crundi_theme::T::MENU_MIN_W)).max(0.);
    w::menu_at(cx, point(px(mx), px(y)), items, move |id, window, cx| match id.as_str() {
        "media" => media_todo(&t, &td, cx),
        "rename" => {
            let cur = with(cx, |k| k.task(&t)).and_then(|x| x.get("todos").and_then(|a| a.as_array()).and_then(|a| a.iter().find(|y| s(y, "id") == td)).map(|y| s(y, "text"))).unwrap_or_default();
            let (t, td) = (t.clone(), td.clone());
            w::ask(window, cx, w::Ask { title: "Edit subtask".into(), message: "Subtask".into(), ok: "OK".into(), prompt: Some(cur), ..Default::default() }, move |r, _, cx| {
                if let Some(v) = r.filter(|v| !v.trim().is_empty()) { post(cx, json!({ "action": "updateTodo", "taskId": t, "todoId": td, "text": v.trim() }), reload); }
            });
        }
        "delete" => post(cx, json!({ "action": "deleteTodo", "taskId": t, "todoId": td }), reload),
        _ => {}
    });
}

// ─── Media (the web's openMediaModal: crate::media) ───

fn media_open(title: &str, st: crate::media::Scope, cx: &mut App) {
    if with(cx, |k| k.clip.take().is_some()) { refresh(cx); }
    // An upload or a delete there shows on the board's strips (loadKanban).
    crate::media::open(title, st, Some(std::rc::Rc::new(|cx: &mut App| load(cx))), None, cx);
}

/// The toolbar's Media: the project's Kanban media.
pub(crate) fn media_kind(cx: &mut App) { media_open("Media \u{00b7} Kanban", crate::media::Scope::kind("kanban"), cx); }

/// A task's media (the attachments' "+", Attach a file): uploads linked to it.
pub(crate) fn media_task(t: &str, cx: &mut App) {
    let p = crundi_shell::project(cx);
    media_open("Task media", crate::media::Scope::linked(json!({ "type": "task", "taskId": t }), Some(p)), cx);
}

/// A subtask's media (its paperclip, its menu's Attach media).
pub(crate) fn media_todo(t: &str, td: &str, cx: &mut App) {
    let p = crundi_shell::project(cx);
    media_open("Subtask media", crate::media::Scope::linked(json!({ "type": "todo", "taskId": t, "todoId": td }), Some(p)), cx);
}

/// The paperclip: its strip opens (or, open, closes).
pub(crate) fn clip_open(t: &str, btn: Box4, wd: f32, cx: &mut App) {
    with(cx, |k| { let same = k.clip.as_ref().is_some_and(|c| c.task == t); k.clip = if same { None } else { Some(Clip { task: t.to_string(), btn, w: wd }) }; });
    refresh(cx);
}

pub(crate) fn clip_close(cx: &mut App) { with(cx, |k| k.clip = None); refresh(cx); }

/// The task attachments' pictures, each loaded once.
fn load_thumbs(cx: &mut App) {
    let items: Vec<Value> = with(cx, |k| k.media_task.values().flatten().filter(|it| s(it, "kind") == "image").cloned().collect());
    for it in items {
        let id = s(&it, "id");
        if with(cx, |k| k.thumbs.contains_key(&id)) { continue; }
        with(cx, |k| k.thumbs.insert(id.clone(), None));
        let path = format!("/api/media/raw/{}", enc(&id));
        let Some(api) = crundi_shell::api(cx) else { return };
        crundi_shell::run(cx, move || api.get_bytes(&path).ok(), move |r, cx| {
            if let Some((200, bytes)) = r {
                if let Some(f) = util::image_format(&bytes) { with(cx, |k| k.thumbs.insert(id, Some(Arc::new(Image::from_bytes(f, bytes))))); refresh(cx); }
            }
        });
    }
}

/// A thumbnail in the attachments strip opened: the lightbox (openMediaLightbox).
pub(crate) fn open_media(id: &str, cx: &mut App) {
    let it = with(cx, |k| { let c = k.clip.take(); c.and_then(|c| k.media_task.get(&c.task).and_then(|v| v.iter().find(|x| s(x, "id") == id).cloned())) });
    refresh(cx);
    if let Some(it) = it { crate::media::lightbox(it, None, cx); }
}

// ─── Drag to move ───

pub(crate) fn press(id: &str, title: &str, at: Point<Pixels>, cx: &mut App) {
    let card = with(cx, |k| k.boxes.borrow().cards.iter().find(|(_, c, _)| c == id).map(|x| x.2).unwrap_or_default());
    drag_with(cx, |d| d.press = Some(Press { id: id.into(), title: title.into(), at, card }));
}

/// The pointer moved while a card is held: past a few px it is a drag.
pub(crate) fn pointer_move(p: Point<Pixels>, cx: &mut App) {
    let (x, y) = (f32::from(p.x), f32::from(p.y));
    let st = store(cx);
    let dr = st.read(cx).drag.clone();
    let started = {
        let d = dr.read(cx);
        if d.drag.is_none() {
            let Some(pr) = d.press.as_ref() else { return };
            let (dx, dy) = ((x - f32::from(pr.at.x)).abs(), (y - f32::from(pr.at.y)).abs());
            let th = f32::from(crundi_theme::T::SP_6);
            if dx <= th && dy <= th { return; }
            Some(DragNow { id: pr.id.clone(), title: pr.title.clone(), grab: (f32::from(pr.at.x) - pr.card.x, f32::from(pr.at.y) - pr.card.y), w: pr.card.w, h: pr.card.h, x, y })
        } else { None }
    };
    if started.is_some() && with(cx, |k| k.clip.take().is_some()) { refresh(cx); }
    let (cols, boxes) = { let k = st.read(cx); (k.cols.clone(), k.boxes.clone()) };
    dr.update(cx, |d, cx| {
        if let Some(n) = started { d.drag = Some(n); d.target = None; }
        let Some(dg) = d.drag.as_mut() else { return };
        dg.x = x; dg.y = y;
        let id = dg.id.clone();
        let b = boxes.borrow();
        d.target = board::drop_target_in(&cols, &b.cols, &b.cards, &id, x, y);
        cx.notify();
    });
}

pub(crate) fn pointer_up(cx: &mut App) {
    with(cx, |k| k.boxes.borrow_mut().pressing = false);
    let (d, t) = drag_with(cx, |k| { k.press = None; (k.drag.take(), k.target.take()) });
    if let (Some(d), Some((stt, i, _))) = (d.as_ref(), t) {
        post(cx, json!({ "action": "moveTask", "taskId": d.id, "status": stt, "index": i }), reload);
    }
}

// ─── For the test driver ───

/// The driver's actions this screen answers (native-gpui's names).
/// Show a card from elsewhere (Cru's show_in_crundi, the web's cruUiGo card:
/// flashKanbanTarget, then its Edit dialog when `open`). None: the board of
/// this project is not loaded yet (ask again); Some(false): no such card.
/// Added by Desktop (F150), for the Kanban tab already on show.
/// `after`: a load from `load_now` that must have come back first (the
/// web's `await loadKanban()`: a card made a moment ago is on the board).
pub fn show_card(id: &str, open: bool, after: u64, window: &mut Window, cx: &mut App) -> Option<bool> {
    let project = crundi_shell::project(cx);
    let (loaded, has) = { let k = store(cx).read(cx); (k.board.is_some() && k.project == project && k.loaded_gen >= after, k.task(id).is_some()) };
    if !loaded { return None; }
    if !has { return Some(false); }
    let n = store(cx).update(cx, |k, cx| { let n = k.flash.as_ref().map(|f| f.1).unwrap_or(0) + 1; k.flash = Some((id.to_string(), n)); cx.notify(); n });
    crate::util::after(cx, 2400, move |_, cx| { store(cx).update(cx, |k, cx| if k.flash.as_ref().is_some_and(|f| f.1 == n) { k.flash = None; cx.notify(); }); });
    if open { act("edit-task", id, "", window, cx); }
    Some(true)
}

pub fn test_ui(_: &Stores, name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let ok = json!({ "ok": true });
    Some(match name {
        "ask-answer" => ask::driver_answer(args, window, cx),
        "kanban-act" => { act(&a(0), &a(1), &a(2), window, cx); ok }
        "kanban-toggle" => { toggle_todo(&a(0), &a(1), a(2) == "true", cx); ok }
        "kanban-type-task" => { let i = line(false, window, cx); let v = a(0); i.update(cx, |s, cx| s.set_value(v, window, cx)); ok }
        "kanban-type-todo" => { let i = line(true, window, cx); let v = a(0); i.update(cx, |s, cx| s.set_value(v, window, cx)); ok }
        "kanban-commit-task" => { commit(false, a(0), a(1) == "true", window, cx); ok }
        "kanban-commit-todo" => { commit(true, a(0), a(1) == "true", window, cx); ok }
        "kanban-todo-menu" => { let b = control_box(&a(0), &format!("more:{}", a(1)), cx).unwrap_or(Box4 { x: a(2).parse().unwrap_or(400.0), y: a(3).parse().unwrap_or(300.0), w: 0., h: 0. }); todo_menu(&a(0), &a(1), b, cx); ok }
        "kanban-clip" => { let b = control_box(&a(0), "clip", cx).unwrap_or(Box4 { x: a(1).parse().unwrap_or(600.0), y: a(2).parse().unwrap_or(300.0), w: 0., h: 0. }); clip_open(&a(0), b, 300.0, cx); ok }
        // The boxes are taken as the views paint: nothing to do but answer.
        "kanban-measure" => ok,
        "kanban-move" => { let i = a(2).parse::<usize>().unwrap_or(0); post(cx, json!({ "action": "moveTask", "taskId": a(0), "status": a(1), "index": i }), reload); ok }
        "kanban-reload" => { load(cx); ok }
        "kanban-all-live" => { let on = a(0) == "true"; store(cx).update(cx, |k, cx| { k.all_live = on; cx.notify(); }); ok }
        _ => return None,
    })
}

/// This screen's state for the driver (native-gpui's names).
pub fn state(_: &Stores, window: &mut Window, cx: &mut App) -> Value {
    let vp = window.viewport_size();
    let st = store(cx);
    let k = st.read(cx);
    let d = k.drag.read(cx);
    let (dt, dd) = (&k.trash.0, &k.trash.1);
    let b = k.boxes.borrow();
    json!({
        "state": k.state, "error": k.error, "view": k.view, "trash": dt.len(), "flash": k.flash.as_ref().map(|f| f.0.clone()),
        "columns": k.cols.iter().map(|c| json!({ "status": c.status, "label": c.label, "other": c.other, "adding": c.adding,
            "cards": c.cards.iter().map(|cd| card_json(cd)).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "deletedTasks": dt.iter().map(|r| json!({ "text": r.text, "task": r.task })).collect::<Vec<_>>(),
        "deletedTodos": dd.iter().map(|r| json!({ "text": r.text, "sub": r.sub, "task": r.task, "todo": r.todo })).collect::<Vec<_>>(),
        "history": k.history.iter().map(|r| json!({ "text": r.text, "ts": r.sub })).collect::<Vec<_>>(),
        "taskDraft": input_value(&k.task_input, cx), "todoDraft": input_value(&k.todo_input, cx),
        "drag": match &d.drag { None => Value::Null, Some(g) => { let t = d.target.clone(); json!({ "id": g.id, "status": t.as_ref().map(|x| x.0.clone()).unwrap_or_default(), "index": t.as_ref().map(|x| x.1 as i64).unwrap_or(-1), "before": t.map(|x| x.2).unwrap_or_default() }) } },
        "clip": match &k.clip { None => Value::Null, Some(c) => json!({ "task": c.task,
            "pop": (pop_json(view::pop_rect(c, vp))), "items": k.media_task.get(&c.task).map(|v| v.iter().map(|it| { let id = s(it, "id"); json!({ "kind": s(it, "kind"), "image": k.thumbs.get(&id).is_some_and(|x| x.is_some()), "id": id }) }).collect::<Vec<_>>()).unwrap_or_default() }) },
        "boxes": {
            "cols": merged_cols(&b.cols).iter().map(|(st, b)| json!({ "status": st, "x": b.x, "y": b.y, "w": b.w, "h": b.h })).collect::<Vec<_>>(),
            "cards": b.cards.iter().map(|(st, id, b)| json!({ "status": st, "id": id, "x": b.x, "y": b.y, "w": b.w, "h": b.h })).collect::<Vec<_>>(),
            "controls": b.controls.iter().map(|(id, c, b)| json!({ "id": id, "control": c, "x": b.x, "y": b.y, "w": b.w, "h": b.h })).collect::<Vec<_>>(),
        },
    })
}

fn pop_json(r: Bounds<Pixels>) -> Value { json!({ "x": f32::from(r.origin.x), "y": f32::from(r.origin.y), "w": f32::from(r.size.width), "h": f32::from(r.size.height) }) }

fn card_json(cd: &Card) -> Value {
    json!({ "id": cd.id, "title": cd.title, "desc": cd.desc, "mm": cd.mm, "media": cd.media, "compact": cd.compact, "menuOpen": cd.menu_open, "adding": cd.adding, "done": cd.done, "total": cd.total,
        "todos": cd.todos.iter().map(|t| json!({ "id": t.id, "text": t.text, "done": t.done, "media": t.media })).collect::<Vec<_>>() })
}

/// One box per column (the stacked board paints a column as several rows).
fn merged_cols(cols: &[(String, Box4)]) -> Vec<(String, Box4)> {
    let mut out: Vec<(String, Box4)> = vec![];
    for (s, b) in cols {
        match out.iter_mut().find(|(x, _)| x == s) {
            Some((_, a)) => { let (x2, y2) = ((a.x + a.w).max(b.x + b.w), (a.y + a.h).max(b.y + b.h)); a.x = a.x.min(b.x); a.y = a.y.min(b.y); a.w = x2 - a.x; a.h = y2 - a.y; }
            None => out.push((s.clone(), *b)),
        }
    }
    out
}
