//! Planning 4.1, 4.2: Kanban (columns, cards, todos, drag to move) and the card dialog (phase 4 on GPUI).
//! Port of the Slint client's native-client/src/planning/kanban.rs, board.rs and media_bridge.rs (the
//! web's loadKanban, renderKanban, setupKanbanHandlers and kanbanPost), checked by
//! native-gpui/test/planning/kanban.mjs. The board is loaded whole after every change and on the
//! server's `kanban` event. kanban_board.rs: the pure rules (unit-tested); kanban_view.rs: the screen;
//! kanban_ask.rs: the ask dialog (Add task, Edit task). One developer owns this file and kanban_*.rs.

#[path = "kanban_board.rs"] pub mod board;
#[path = "kanban_ask.rs"] pub mod ask;
#[path = "kanban_view.rs"] mod draw;

use ask::Choice;
use board::Box4;
use crundi_shell::AreaEvent;
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, rc::Rc, sync::Arc};

/// A press on a card that may become a drag.
pub(crate) struct Press { pub id: String, pub title: String, pub at: Point<Pixels>, pub card: Box4 }

/// A drag on its way: the card, where it was taken, the pointer.
pub(crate) struct Drag { pub id: String, pub title: String, pub grab: (f32, f32), pub w: f32, pub h: f32, pub x: f32, pub y: f32 }

/// A card's attachments strip, under its paperclip (x, y: the button's bottom right).
pub(crate) struct Clip { pub task: String, pub x: f32, pub y: f32, pub w: f32 }

#[derive(Default)]
pub(crate) struct K {
    pub board: Option<Value>,
    pub project: String,
    pub view: String,
    /// "none" (no project), "loading", "ok", "error".
    pub state: String,
    pub error: String,
    pub menu_open: HashSet<String>,
    pub adding_task: Option<String>,
    pub adding_todo: Option<String>,
    pub media_task: HashMap<String, Vec<Value>>,
    pub media_todo: HashMap<String, Vec<Value>>,
    pub thumbs: HashMap<String, Option<Arc<Image>>>,
    pub load_gen: u64,
    // Every card's and column's box on the window, as last drawn.
    pub col_boxes: Vec<(String, Box4)>,
    pub card_boxes: Vec<(String, String, Box4)>,
    pub press: Option<Press>,
    pub drag: Option<Drag>,
    pub target: Option<(String, usize, String)>,
    pub clip: Option<Clip>,
    pub task_input: Option<Entity<w::InputState>>,
    pub todo_input: Option<Entity<w::InputState>>,
    pub subs: Vec<Subscription>,
}
impl Global for K {}

pub(crate) fn st(cx: &App) -> &K { cx.global::<K>() }
pub(crate) fn with<R>(cx: &mut App, f: impl FnOnce(&mut K) -> R) -> R { f(cx.global_mut::<K>()) }
/// Draw the board again where it shows (its tab, its pane), not the window
/// (a refresh drew every pane again).
pub(crate) fn refresh(cx: &mut App) {
    crundi_shell::redraw_tab(cx, "kanban");
    if let Some(wb) = crundi_workbench::workbench(cx) { wb.update(cx, |w, cx| w.redraw_kind("panel:kanban", cx)); }
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn truthy(v: &Value) -> bool { v.as_bool().unwrap_or(false) }
fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// At start: the state, and the workbench pane kind panel:kanban.
pub fn init(cx: &mut App) {
    cx.set_global(K { view: "board".into(), state: "loading".into(), ..Default::default() });
    crundi_workbench::register_kind(cx, crundi_workbench::PaneKind {
        prefix: "panel:kanban",
        render: Rc::new(|key, window, cx| view("kanban", key, window, cx)),
        title: Rc::new(|_, _| ("Kanban".to_string(), "idle".to_string())),
    });
}

/// The tab's page (or a pane's body: `key` is the pane's key, "" in a tab).
pub fn view(_tab: &str, key: &str, window: &mut Window, cx: &mut App) -> AnyElement { draw::view(!key.is_empty(), window, cx) }

/// The board is on show: its tab, or a pane on the workbench.
fn visible(cx: &mut App) -> bool {
    let tab = crundi_shell::current_tab(cx);
    if tab == "kanban" { return true; }
    if tab != "workbench" { return false; }
    crundi_workbench::workbench(cx).is_some_and(|wb| wb.update(cx, |w, _| w.mosaic().keys().iter().any(|k| k == "panel:kanban")))
}

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    match e {
        AreaEvent::Server("kanban", v) => {
            let p = crundi_shell::project(cx).to_lowercase();
            if !p.is_empty() && s(v, "project").to_lowercase() == p && visible(cx) { load(cx); }
        }
        AreaEvent::Connected(true) => if visible(cx) { load(cx); },
        _ => {}
    }
}

pub fn on_project(alias: &str, cx: &mut App) {
    let changed = with(cx, |k| {
        let c = k.project != alias;
        k.project = alias.to_string();
        if c { k.board = None; k.menu_open.clear(); k.adding_task = None; k.adding_todo = None; k.clip = None; k.state = if alias.is_empty() { "none".into() } else { "loading".into() }; }
        c
    });
    if changed { refresh(cx); }
    load(cx);
}

pub fn on_signed_in(cx: &mut App) { let p = crundi_shell::project(cx); on_project(&p, cx); }

pub fn on_signed_out(cx: &mut App) {
    with(cx, |k| {
        k.board = None; k.menu_open.clear(); k.adding_task = None; k.adding_todo = None; k.media_task.clear(); k.media_todo.clear();
        k.thumbs.clear(); k.view = "board".into(); k.state = "loading".into(); k.clip = None; k.drag = None; k.press = None; k.project.clear();
    });
    if ask::is_open(cx) {
        cx.defer(|cx| { if let Some(h) = cx.windows().first().copied() { let _ = h.update(cx, |_, window, cx| ask::cancel(window, cx)); } });
    }
    refresh(cx);
}

pub fn on_tab(tab: &str, cx: &mut App) { if tab == "kanban" { load(cx); } }

/// The board again, from the server (with its media), then drawn.
pub fn load(cx: &mut App) {
    let p = crundi_shell::project(cx);
    if p.is_empty() { with(cx, |k| { k.board = None; k.state = "none".into(); }); refresh(cx); return; }
    let trash = st(cx).view == "trash";
    let lg = with(cx, |k| { k.load_gen += 1; k.load_gen });
    let path = format!("/api/kanban?project={}{}", enc(&p), if trash { "&includeDeleted=1" } else { "" });
    let mpath = format!("/api/media/list?project={}&includeGeneral=1&kind=kanban", enc(&p));
    crundi_shell::call(cx, move |api| {
        let b = api.get(&path)?;
        let m = api.get(&mpath).unwrap_or(Value::Null);
        Ok(json!({ "b": b, "m": m }))
    }, move |r, cx| {
        if st(cx).load_gen != lg || crundi_shell::project(cx) != p { return; }
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

/// A toast, from anywhere (deferred to the window).
pub(crate) fn toast(cx: &mut App, text: &str, kind: w::Toast) {
    let t = text.to_string();
    cx.defer(move |cx| { if let Some(h) = cx.windows().first().copied() { let _ = h.update(cx, |_, window, cx| w::toast(window, cx, t, kind)); } });
}

pub(crate) fn show_tab(cx: &mut App, tab: &str) {
    let t = tab.to_string();
    cx.defer(move |cx| { if let Some(sh) = crundi_shell::shell(cx) { sh.update(cx, |s, cx| s.show_tab(&t, cx)); } });
}

/// POST /api/kanban with the project; a refusal says so in a toast.
pub(crate) fn post(cx: &mut App, mut payload: Value, then: impl FnOnce(Value, &mut App) + 'static) {
    payload["project"] = json!(crundi_shell::project(cx));
    crundi_shell::call(cx, move |api| api.post("/api/kanban", &payload), move |r, cx| {
        let v = match r {
            Ok(v) => { if !truthy(&v["ok"]) { let e = s(&v, "error"); toast(cx, if e.is_empty() { "Action failed" } else { &e }, w::Toast::Error); } v }
            Err(e) => { toast(cx, &format!("Action failed: {e}"), w::Toast::Error); json!({ "ok": false }) }
        };
        then(v, cx);
    });
}

fn reload(_: Value, cx: &mut App) { load(cx); }

fn task(b: &Value, id: &str) -> Option<Value> {
    b.get("tasks").and_then(|t| t.as_array()).and_then(|a| a.iter().find(|t| s(t, "id") == id)).cloned()
}

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
    if let Some(i) = if todo { st(cx).todo_input.clone() } else { st(cx).task_input.clone() } { return i; }
    let ph = if todo { "New subtask, Enter to add" } else { "Task title, Enter to add" };
    let i = cx.new(|cx| w::InputState::new(window, cx).placeholder(ph));
    let sub = window.subscribe(&i, cx, move |e, ev: &w::InputEvent, window, cx| {
        let text = e.read(cx).value().to_string();
        match ev {
            w::InputEvent::PressEnter { .. } => { if !text.trim().is_empty() { commit(todo, text, true, window, cx); } }
            w::InputEvent::Blur => {
                let open = if todo { st(cx).adding_todo.is_some() } else { st(cx).adding_task.is_some() };
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
    let target = if todo { st(cx).adding_todo.clone() } else { st(cx).adding_task.clone() }.unwrap_or_default();
    let inp = if todo { st(cx).todo_input.clone() } else { st(cx).task_input.clone() };
    clear_input(inp, window, cx);
    if !keep { with(cx, |k| if todo { k.adding_todo = None } else { k.adding_task = None }); }
    refresh(cx);
    if text.is_empty() || target.is_empty() { return; }
    let body = if todo { json!({ "action": "addTodo", "taskId": target, "text": text }) } else { json!({ "action": "addTask", "title": text, "status": target }) };
    post(cx, body, reload);
}

/// Escape on the line: dropped, closed.
pub(crate) fn cancel_line(todo: bool, window: &mut Window, cx: &mut App) {
    with(cx, |k| if todo { k.adding_todo = None } else { k.adding_task = None });
    let inp = if todo { st(cx).todo_input.clone() } else { st(cx).task_input.clone() };
    clear_input(inp, window, cx);
    refresh(cx);
}

// ─── The actions (the web's data-kact) ───

pub(crate) fn act(a: &str, task_id: &str, extra: &str, window: &mut Window, cx: &mut App) {
    let b = st(cx).board.clone().unwrap_or(Value::Null);
    match a {
        "view-board" | "view-trash" | "view-history" => {
            with(cx, |k| k.view = a.trim_start_matches("view-").to_string());
            refresh(cx);
            load(cx);
        }
        "media" => media_modal(cx),
        "card-menu" => { with(cx, |k| if !k.menu_open.remove(task_id) { k.menu_open.insert(task_id.to_string()); }); refresh(cx); }
        "add-task-inline" => {
            let stt = extra.to_string();
            let draft = input_value(&st(cx).task_input, cx);
            let prev = st(cx).adding_task.clone();
            with(cx, |k| k.adding_task = Some(stt.clone()));
            clear_input(st(cx).task_input.clone(), window, cx);
            // Another column's open card keeps what was typed in it.
            if let Some(prev) = prev.filter(|p| *p != stt && !draft.trim().is_empty()) {
                post(cx, json!({ "action": "addTask", "title": draft.trim(), "status": prev }), reload);
            }
            focus_line(false, window, cx);
            refresh(cx);
        }
        "add-subtask" => {
            let t = task_id.to_string();
            let draft = input_value(&st(cx).todo_input, cx);
            let prev = with(cx, |k| { k.menu_open.remove(&t); let p = k.adding_todo.clone(); k.adding_todo = Some(t.clone()); p });
            clear_input(st(cx).todo_input.clone(), window, cx);
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
            let Some(t) = task(&b, task_id) else { return };
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
            let Some(t) = task(&b, task_id) else { return };
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
                    toast(cx, &format!("Added to Mindmap{}", if n > 0 { format!(" with {n} subtask(s)") } else { String::new() }), w::Toast::Success);
                    show_tab(cx, "mindmap");
                }
                Ok(v) => { let e = s(&v["r"], "error"); toast(cx, if e.is_empty() { "Action failed" } else { &e }, w::Toast::Error); }
                Err(e) => toast(cx, &format!("Action failed: {e}"), w::Toast::Error),
            });
        }
        "goto-mindmap" => show_tab(cx, "mindmap"),
        _ => {}
    }
}

pub(crate) fn toggle_todo(t: &str, td: &str, done: bool, cx: &mut App) {
    post(cx, json!({ "action": "updateTodo", "taskId": t, "todoId": td, "done": done }), reload);
}

/// The subtask's "…": the shared menu at (x, y), its right edge there (the web's .kt-pop).
pub(crate) fn todo_menu(t: &str, td: &str, x: f32, y: f32, cx: &mut App) {
    let n = st(cx).media_todo.get(td).map(|v| v.len()).unwrap_or(0);
    let (t, td) = (t.to_string(), td.to_string());
    let attach = if n > 0 { format!("Attach media  {n}") } else { "Attach media".to_string() };
    let items = vec![
        w::MenuEntry::new("media", attach).icon(w::IconName::Paperclip),
        w::MenuEntry::new("rename", "Rename").icon(w::IconName::Pencil),
        w::MenuEntry::new("delete", "Delete").icon(w::IconName::Trash).danger().sep(),
    ];
    let mx = (x - f32::from(crundi_theme::T::MENU_MIN_W)).max(0.);
    w::menu_at(cx, point(px(mx), px(y)), items, move |id, window, cx| match id.as_str() {
        "media" => media_modal(cx),
        "rename" => {
            let cur = st(cx).board.as_ref().and_then(|b| task(b, &t)).and_then(|x| x.get("todos").and_then(|a| a.as_array()).and_then(|a| a.iter().find(|y| s(y, "id") == td)).map(|y| s(y, "text"))).unwrap_or_default();
            let (t, td) = (t.clone(), td.clone());
            w::ask(window, cx, w::Ask { title: "Edit subtask".into(), message: "Subtask".into(), ok: "OK".into(), prompt: Some(cur), ..Default::default() }, move |r, _, cx| {
                if let Some(v) = r.filter(|v| !v.trim().is_empty()) { post(cx, json!({ "action": "updateTodo", "taskId": t, "todoId": td, "text": v.trim() }), reload); }
            });
        }
        "delete" => post(cx, json!({ "action": "deleteTodo", "taskId": t, "todoId": td }), reload),
        _ => {}
    });
    refresh(cx);
}

// ─── Media (the Media modal and viewer are Workspace's: media_bridge) ───

/// The web's openMediaModal: not on GPUI yet (shared/requests.md), so the Media tab.
pub(crate) fn media_modal(cx: &mut App) {
    with(cx, |k| k.clip = None);
    toast(cx, "Media opens in the Media tab", w::Toast::Info);
    show_tab(cx, "media");
}

pub(crate) fn clip_open(t: &str, x: f32, y: f32, wd: f32, cx: &mut App) {
    let same = st(cx).clip.as_ref().is_some_and(|c| c.task == t);
    with(cx, |k| k.clip = if same { None } else { Some(Clip { task: t.to_string(), x, y, w: wd }) });
    refresh(cx);
}

pub(crate) fn clip_close(cx: &mut App) { with(cx, |k| k.clip = None); refresh(cx); }

/// The task attachments' pictures, each loaded once.
fn load_thumbs(cx: &mut App) {
    let items: Vec<Value> = st(cx).media_task.values().flatten().filter(|it| s(it, "kind") == "image").cloned().collect();
    for it in items {
        let id = s(&it, "id");
        if st(cx).thumbs.contains_key(&id) { continue; }
        with(cx, |k| k.thumbs.insert(id.clone(), None));
        let path = format!("/api/media/raw/{}", enc(&id));
        let Some(api) = crundi_shell::api(cx) else { return };
        crundi_shell::run(cx, move || api.get_bytes(&path).ok(), move |r, cx| {
            if let Some((200, bytes)) = r {
                if let Some(f) = image_format(&bytes) { with(cx, |k| k.thumbs.insert(id, Some(Arc::new(Image::from_bytes(f, bytes))))); refresh(cx); }
            }
        });
    }
}

fn image_format(b: &[u8]) -> Option<ImageFormat> {
    Some(if b.starts_with(b"\x89PNG") { ImageFormat::Png } else if b.starts_with(&[0xff, 0xd8]) { ImageFormat::Jpeg } else if b.starts_with(b"GIF8") { ImageFormat::Gif }
        else if b.len() > 12 && &b[8..12] == b"WEBP" { ImageFormat::Webp } else if b.starts_with(b"BM") { ImageFormat::Bmp } else { return None })
}

/// A thumbnail opened: the viewer is Workspace's (not on GPUI yet), so the Media tab.
pub(crate) fn open_media(_id: &str, cx: &mut App) { media_modal(cx); }

// ─── Drag to move ───

pub(crate) fn press(id: &str, title: &str, at: Point<Pixels>, cx: &mut App) {
    let card = st(cx).card_boxes.iter().find(|(_, c, _)| c == id).map(|x| x.2).unwrap_or_default();
    with(cx, |k| k.press = Some(Press { id: id.into(), title: title.into(), at, card }));
}

/// The pointer moved while a card is held: past a few px it is a drag.
pub(crate) fn pointer_move(p: Point<Pixels>, cx: &mut App) {
    let (x, y) = (f32::from(p.x), f32::from(p.y));
    if st(cx).drag.is_none() {
        let Some(pr) = st(cx).press.as_ref() else { return };
        let (dx, dy) = ((x - f32::from(pr.at.x)).abs(), (y - f32::from(pr.at.y)).abs());
        let th = f32::from(crundi_theme::T::SP_6);
        if dx <= th && dy <= th { return; }
        let d = Drag { id: pr.id.clone(), title: pr.title.clone(), grab: (f32::from(pr.at.x) - pr.card.x, f32::from(pr.at.y) - pr.card.y), w: pr.card.w, h: pr.card.h, x, y };
        with(cx, |k| { k.drag = Some(d); k.clip = None; k.target = None; });
    }
    with(cx, |k| {
        let Some(d) = k.drag.as_mut() else { return };
        d.x = x; d.y = y;
        let id = d.id.clone();
        let order: Vec<(String, String)> = k.board.as_ref().map(|b| board::columns(b, &board::Ui::default()).into_iter().filter(|c| !c.other).flat_map(|c| c.cards.into_iter().map(move |cd| (c.status.clone(), cd.id))).collect()).unwrap_or_default();
        let cards: Vec<(String, String, Box4)> = order.iter().filter_map(|(st, cid)| k.card_boxes.iter().find(|(_, c, _)| c == cid).map(|(_, _, b)| (st.clone(), cid.clone(), *b))).collect();
        k.target = board::drop_target(&k.col_boxes, &cards, &id, x, y);
    });
    refresh(cx);
}

pub(crate) fn pointer_up(cx: &mut App) {
    let (d, t) = with(cx, |k| { k.press = None; (k.drag.take(), k.target.take()) });
    if let (Some(d), Some((stt, i, _))) = (d.as_ref(), t) {
        post(cx, json!({ "action": "moveTask", "taskId": d.id, "status": stt, "index": i }), reload);
    }
    if d.is_some() { refresh(cx); }
}

// ─── Small saved settings (the web's localStorage) ───

pub mod prefs {
    use serde_json::Value;
    fn path() -> std::path::PathBuf { crundi_core::store::config_dir().join("planning.json") }
    fn all() -> Value { std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Object(Default::default())) }
    pub fn get(k: &str) -> String { all().get(k).and_then(|v| v.as_str()).unwrap_or("").to_string() }
    pub fn set(k: &str, v: &str) {
        let mut a = all();
        a[k] = Value::String(v.to_string());
        let _ = std::fs::write(path(), a.to_string());
    }
}

// ─── For the test driver ───

/// The driver's "planning:<name>" actions this screen answers (the Slint client's names).
pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let ok = json!({ "ok": true });
    Some(match name {
        "ask-answer" => ask::driver_answer(args, window, cx),
        // The shared prompt and menu (w::ask / w::menu_at): the driver's own
        // dialog-answer and menu-pick don't reach them in this tree yet (planning-asks.md).
        "dialog-answer" => json!({ "ok": w::ask_answer(a(0) == "true", args.get(1).cloned(), window, cx) }),
        "menu-pick" => { if !w::menu_pick(&a(0), window, cx) { return None; } ok }
        "kanban-act" => { act(&a(0), &a(1), &a(2), window, cx); ok }
        "kanban-toggle" => { toggle_todo(&a(0), &a(1), a(2) == "true", cx); ok }
        "kanban-type-task" => { let i = line(false, window, cx); let v = a(0); i.update(cx, |s, cx| s.set_value(v, window, cx)); ok }
        "kanban-type-todo" => { let i = line(true, window, cx); let v = a(0); i.update(cx, |s, cx| s.set_value(v, window, cx)); ok }
        "kanban-commit-task" => { commit(false, a(0), a(1) == "true", window, cx); ok }
        "kanban-commit-todo" => { commit(true, a(0), a(1) == "true", window, cx); ok }
        "kanban-todo-menu" => { todo_menu(&a(0), &a(1), a(2).parse().unwrap_or(400.0), a(3).parse().unwrap_or(300.0), cx); ok }
        "kanban-clip" => { clip_open(&a(0), a(1).parse().unwrap_or(600.0), a(2).parse().unwrap_or(300.0), 300.0, cx); ok }
        // The boxes are taken every frame (kanban_view.rs): a redraw is a measure.
        "kanban-measure" => { refresh(cx); ok }
        "kanban-move" => { let i = a(2).parse::<usize>().unwrap_or(0); post(cx, json!({ "action": "moveTask", "taskId": a(0), "status": a(1), "index": i }), reload); ok }
        "kanban-reload" => { load(cx); ok }
        _ => return None,
    })
}

/// This screen's props: planningArea.kanban and .ask (the Slint client's names), and meanwhile the
/// window-wide pointerModal, dialog and menu the driver doesn't carry yet in this tree.
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    let k = st(cx);
    let mt: HashMap<String, usize> = k.media_task.iter().map(|(a, v)| (a.clone(), v.len())).collect();
    let mtd: HashMap<String, usize> = k.media_todo.iter().map(|(a, v)| (a.clone(), v.len())).collect();
    let ui = board::Ui { menu_open: Some(&k.menu_open), adding_task: k.adding_task.as_deref(), adding_todo: k.adding_todo.as_deref(), media_task: Some(&mt), media_todo: Some(&mtd) };
    let (cols, dt, dd, hist) = match &k.board {
        Some(b) => (board::columns(b, &ui), board::trash(b).0, board::trash(b).1, board::history(b, board::local_time)),
        None => (vec![], vec![], vec![], vec![]),
    };
    let kanban = json!({
        "state": k.state, "error": k.error, "view": k.view, "trash": dt.len(),
        "columns": cols.iter().map(|c| json!({ "status": c.status, "label": c.label, "other": c.other, "adding": c.adding,
            "cards": c.cards.iter().map(|cd| json!({ "id": cd.id, "title": cd.title, "desc": cd.desc, "mm": cd.mm, "media": cd.media, "compact": cd.compact, "menuOpen": cd.menu_open, "adding": cd.adding, "done": cd.done, "total": cd.total,
                "todos": cd.todos.iter().map(|t| json!({ "id": t.id, "text": t.text, "done": t.done, "media": t.media })).collect::<Vec<_>>() })).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "deletedTasks": dt.iter().map(|r| json!({ "text": r.text, "task": r.task })).collect::<Vec<_>>(),
        "deletedTodos": dd.iter().map(|r| json!({ "text": r.text, "sub": r.sub, "task": r.task, "todo": r.todo })).collect::<Vec<_>>(),
        "history": hist.iter().map(|r| json!({ "text": r.text, "ts": r.sub })).collect::<Vec<_>>(),
        "taskDraft": input_value(&k.task_input, cx), "todoDraft": input_value(&k.todo_input, cx),
        "drag": match &k.drag { None => Value::Null, Some(d) => { let t = k.target.clone(); json!({ "id": d.id, "status": t.as_ref().map(|x| x.0.clone()).unwrap_or_default(), "index": t.as_ref().map(|x| x.1 as i64).unwrap_or(-1), "before": t.map(|x| x.2).unwrap_or_default() }) } },
        "clip": match &k.clip { None => Value::Null, Some(c) => json!({ "task": c.task, "items": k.media_task.get(&c.task).map(|v| v.iter().map(|it| { let id = s(it, "id"); json!({ "kind": s(it, "kind"), "image": k.thumbs.get(&id).is_some_and(|x| x.is_some()), "id": id }) }).collect::<Vec<_>>()).unwrap_or_default() }) },
        "boxes": {
            "cols": k.col_boxes.iter().map(|(st, b)| json!({ "status": st, "x": b.x, "y": b.y, "w": b.w, "h": b.h })).collect::<Vec<_>>(),
            "cards": k.card_boxes.iter().map(|(st, id, b)| json!({ "status": st, "id": id, "x": b.x, "y": b.y, "w": b.w, "h": b.h })).collect::<Vec<_>>(),
        },
    });
    let askp = ask::props(cx);
    let dialog = w::ask_props(cx);
    let menu = w::menu_props(cx);
    json!({ "kanban": kanban, "pointerModal": !askp.is_null() || !dialog.is_null(), "ask": askp, "dialog": dialog, "menu": menu })
}
