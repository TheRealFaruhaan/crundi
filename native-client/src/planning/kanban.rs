//! Kanban (4.1) and its card dialog (4.2): the web's loadKanban,
//! renderKanban, setupKanbanHandlers and kanbanPost (src/webapp-html.js).
//! The board is loaded whole after every change and on the server's
//! `kanban` event, and drawn again where it changed (kit::sync_rows).

use super::board::{self, Box4};
use super::{ask, prefs, Choice};
use crate::{kit, MainWindow, PlanningCard, PlanningColumn, PlanningKanban, PlanningRow, PlanningThumb, PlanningTodo};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::Rc};

#[derive(Default)]
struct K {
    board: Option<Value>,
    project: String,
    view: String,
    menu_open: HashSet<String>,
    adding_task: Option<String>,
    adding_todo: Option<String>,
    media_task: HashMap<String, Vec<Value>>,
    media_todo: HashMap<String, Vec<Value>>,
    thumbs: HashMap<String, Option<slint::Image>>,
    load_gen: u64,
    // A drag: the boxes reported at its start, and where it would land.
    col_boxes: Vec<(String, Box4)>,
    card_boxes: Vec<(String, String, Box4)>,
    target: Option<(String, usize, String)>,
    flash: Option<String>,
    last_error: String,
    // Models set once (README: lists are never replaced wholesale).
    cols: Option<Rc<VecModel<PlanningColumn>>>,
    del_tasks: Option<Rc<VecModel<PlanningRow>>>,
    del_todos: Option<Rc<VecModel<PlanningRow>>>,
    hist: Option<Rc<VecModel<PlanningRow>>>,
}

thread_local! { static S: RefCell<K> = RefCell::new(K { view: "board".into(), ..Default::default() }); }

fn with<R>(f: impl FnOnce(&mut K) -> R) -> R { S.with(|s| f(&mut s.borrow_mut())) }

/// Run `f` with the Kanban global (on the UI thread).
fn kg(f: impl FnOnce(&PlanningKanban)) {
    if let Some(w) = kit::ui() { f(&w.global::<PlanningKanban>()); }
}

pub fn init(w: &MainWindow) {
    let k = w.global::<PlanningKanban>();
    let cols = kit::model::<PlanningColumn>();
    let dt = kit::model::<PlanningRow>();
    let dd = kit::model::<PlanningRow>();
    let h = kit::model::<PlanningRow>();
    k.set_columns(kit::rc(&cols));
    k.set_deleted_tasks(kit::rc(&dt));
    k.set_deleted_todos(kit::rc(&dd));
    k.set_history(kit::rc(&h));
    with(|s| { s.cols = Some(cols); s.del_tasks = Some(dt); s.del_todos = Some(dd); s.hist = Some(h); });
    k.on_act(|a, t, x| act(a.as_str(), t.as_str(), x.as_str()));
    k.on_toggle_todo(|t, td, v| { let (t, td) = (t.to_string(), td.to_string()); post(json!({ "action": "updateTodo", "taskId": t, "todoId": td, "done": v }), |_| load()); });
    k.on_todo_menu(|t, td, x, y| todo_menu(t.as_str(), td.as_str(), x, y));
    k.on_todo_media(|t, td| todo_media(t.as_str(), td.as_str()));
    k.on_clip(|t, x, y, w| clip_open(t.as_str(), x, y, w, false));
    k.on_clip_close(clip_close);
    k.on_clip_add(|t| { let t = t.to_string(); clip_close(); media_modal("Task media", json!({ "scope": "project", "kind": "all", "linkFilter": { "type": "task", "taskId": t }, "uploadLink": { "type": "task", "taskId": t }, "uploadProject": kit::project() })); });
    k.on_thumb_open(|id| { let id = id.to_string(); clip_close(); open_media(&id); });
    k.on_task_commit(|st, text, keep| task_commit(st.to_string(), text.to_string(), keep));
    k.on_task_cancel(|| { with(|s| s.adding_task = None); kg(|k| k.set_task_draft("".into())); render(); });
    k.on_todo_commit(|t, text, keep| todo_commit(t.to_string(), text.to_string(), keep));
    k.on_todo_cancel(|| { with(|s| s.adding_todo = None); kg(|k| k.set_todo_draft("".into())); render(); });
    k.on_drag_start(|id| drag_start(id.as_str()));
    k.on_drag_move(drag_move);
    k.on_drag_end(drag_end);
    k.on_col_box(|st, x, y, w, h| with(|s| { s.col_boxes.retain(|(c, _)| c != st.as_str()); s.col_boxes.push((st.to_string(), Box4 { x, y, w, h })); }));
    k.on_card_box(|st, id, x, y, w, h| with(|s| { s.card_boxes.retain(|(_, c, _)| c != id.as_str()); s.card_boxes.push((st.to_string(), id.to_string(), Box4 { x, y, w, h })); }));
}

/// Whether the board is on show: its tab, or a pane on the workbench.
pub fn visible() -> bool {
    let Some(w) = kit::ui() else { return false };
    let tab = w.global::<crate::Shell>().get_tab();
    use slint::Model;
    tab == "kanban" || (tab == "workbench" && w.get_panes().iter().any(|p| p.key == "panel:kanban"))
}

pub fn signed_out() {
    with(|s| { s.board = None; s.menu_open.clear(); s.adding_task = None; s.adding_todo = None; s.media_task.clear(); s.media_todo.clear(); s.thumbs.clear(); s.view = "board".into(); });
    kg(|k| { k.set_state("loading".into()); k.set_view("board".into()); k.set_clip_task("".into()); k.set_drag_id("".into()); });
    render();
}

pub fn project(alias: &str) {
    let changed = with(|s| { let c = s.project != alias; s.project = alias.to_string(); if c { s.board = None; s.menu_open.clear(); s.adding_task = None; s.adding_todo = None; } c });
    if changed { kg(|k| { k.set_state(if alias.is_empty() { "none" } else { "loading" }.into()); k.set_clip_task("".into()); }); }
    load();
}

/// The board again, from the server (then its media, then drawn).
pub fn load() {
    let p = kit::project();
    if p.is_empty() {
        with(|s| s.board = None);
        kg(|k| k.set_state("none".into()));
        return;
    }
    let trash = with(|s| s.view == "trash");
    let gen = with(|s| { s.load_gen += 1; s.load_gen });
    let path = format!("/api/kanban?project={}{}", kit::enc(&p), if trash { "&includeDeleted=1" } else { "" });
    let mpath = format!("/api/media/list?project={}&includeGeneral=1&kind=kanban", kit::enc(&p));
    kit::call(move |api| {
        let b = api.get(&path)?;
        let m = api.get(&mpath).unwrap_or(Value::Null);
        Ok(json!({ "b": b, "m": m }))
    }, move |r| {
        if with(|s| s.load_gen != gen) || kit::project() != p { return; }
        match r {
            Err(e) => { with(|s| { s.board = None; s.last_error = e.clone(); }); kg(|k| { k.set_error(format!("Failed to load board: {e}").into()); k.set_state("error".into()); }); }
            Ok(v) => {
                let b = &v["b"];
                if !kit::truthy(&b["ok"]) {
                    let e = kit::s(b, "error");
                    kg(|k| { k.set_error(if e.is_empty() { "Failed to load".into() } else { e.into() }); k.set_state("error".into()); });
                    return;
                }
                let items = v["m"].get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                let (bt, btd) = board::media_groups(&items);
                with(|s| { s.board = Some(b["board"].clone()); s.media_task = bt; s.media_todo = btd; });
                kg(|k| k.set_state("ok".into()));
                render();
                refresh_clip();
            }
        }
    });
}

fn render() {
    let Some(w) = kit::ui() else { return };
    let k = w.global::<PlanningKanban>();
    with(|s| {
        k.set_view(s.view.clone().into());
        let Some(b) = s.board.clone() else {
            if let Some(m) = &s.cols { kit::sync_rows(m, vec![]); }
            return;
        };
        let mt: HashMap<String, usize> = s.media_task.iter().map(|(k, v)| (k.clone(), v.len())).collect();
        let mtd: HashMap<String, usize> = s.media_todo.iter().map(|(k, v)| (k.clone(), v.len())).collect();
        let ui = board::Ui { menu_open: Some(&s.menu_open), adding_task: s.adding_task.as_deref(), adding_todo: s.adding_todo.as_deref(), media_task: Some(&mt), media_todo: Some(&mtd) };
        let cols = board::columns(&b, &ui);
        let flash = s.flash.clone();
        let rows: Vec<PlanningColumn> = cols.into_iter().map(|c| PlanningColumn {
            status: c.status.into(), label: c.label.into(), other: c.other, adding: c.adding,
            cards: ModelRc::new(VecModel::from(c.cards.into_iter().map(|cd| PlanningCard {
                flash: flash.as_deref() == Some(cd.id.as_str()),
                id: cd.id.into(), title: cd.title.into(), desc: cd.desc.into(), mm: cd.mm as i32, media: cd.media as i32,
                compact: cd.compact, menu_open: cd.menu_open, adding: cd.adding, done: cd.done as i32, total: cd.total as i32,
                todos: ModelRc::new(VecModel::from(cd.todos.into_iter().map(|t| PlanningTodo { id: t.id.into(), text: t.text.into(), done: t.done, media: t.media as i32 }).collect::<Vec<_>>())),
            }).collect::<Vec<_>>())),
        }).collect();
        if let Some(m) = &s.cols { kit::sync_rows(m, rows); }
        let row = |r: board::Row| PlanningRow { text: r.text.into(), sub: r.sub.into(), task: r.task.into(), todo: r.todo.into() };
        let (dt, dd) = board::trash(&b);
        k.set_trash_count(dt.len() as i32);
        if let Some(m) = &s.del_tasks { kit::sync_rows(m, dt.into_iter().map(row).collect()); }
        if let Some(m) = &s.del_todos { kit::sync_rows(m, dd.into_iter().map(row).collect()); }
        if let Some(m) = &s.hist { kit::sync_rows(m, board::history(&b, board::local_time).into_iter().map(row).collect()); }
    });
}

/// POST /api/kanban with the project; a refusal says so in a toast.
pub fn post(mut payload: Value, then: impl FnOnce(Value) + 'static) {
    payload["project"] = json!(kit::project());
    let id = super::park(then);
    kit::call(move |api| api.post("/api/kanban", &payload), move |r| {
        let v = match r {
            Ok(v) => { if !kit::truthy(&v["ok"]) { let e = kit::s(&v, "error"); kit::toast_error(if e.is_empty() { "Action failed" } else { &e }); } v }
            Err(e) => { kit::toast_error(&format!("Action failed: {e}")); json!({ "ok": false }) }
        };
        super::unpark(id, v);
    });
}

fn task<'a>(b: &'a Value, id: &str) -> Option<&'a Value> {
    b.get("tasks").and_then(|t| t.as_array()).and_then(|a| a.iter().find(|t| kit::s(t, "id") == id))
}

fn status_choice(b: &Value, value: &str, label: &str) -> Choice {
    Choice { label: label.into(), options: board::statuses(b).into_iter().map(|s| { let l = board::label(&s); (s, l) }).collect(), value: value.into() }
}

/// A toolbar or card action (the web's data-kact).
pub fn act(a: &str, task_id: &str, extra: &str) {
    let b = with(|s| s.board.clone()).unwrap_or(Value::Null);
    match a {
        "view-board" | "view-trash" | "view-history" => {
            with(|s| s.view = a.trim_start_matches("view-").to_string());
            kg(|k| k.set_view(a.trim_start_matches("view-").into()));
            load();
        }
        "media" => media_modal("Media \u{00b7} Kanban", json!({ "scope": "project", "kind": "kanban" })),
        "card-menu" => {
            with(|s| if !s.menu_open.remove(task_id) { s.menu_open.insert(task_id.to_string()); });
            render();
        }
        "add-task-inline" => {
            let st = extra.to_string();
            let draft = kg_get(|k| k.get_task_draft().to_string());
            let prev = with(|s| s.adding_task.clone());
            kg(|k| k.set_task_draft("".into()));
            with(|s| s.adding_task = Some(st.clone()));
            // Another column's open card keeps what was typed in it.
            if let Some(prev) = prev.filter(|p| *p != st && !draft.trim().is_empty()) {
                post(json!({ "action": "addTask", "title": draft.trim(), "status": prev }), |_| load());
            }
            render();
            bump_focus();
        }
        "add-subtask" => {
            let t = task_id.to_string();
            let draft = kg_get(|k| k.get_todo_draft().to_string());
            let prev = with(|s| { s.menu_open.remove(&t); s.adding_todo.clone() });
            kg(|k| k.set_todo_draft("".into()));
            with(|s| s.adding_todo = Some(t.clone()));
            if let Some(prev) = prev.filter(|p| *p != t && !draft.trim().is_empty()) {
                post(json!({ "action": "addTodo", "taskId": prev, "text": draft.trim() }), |_| load());
            }
            render();
            bump_focus();
        }
        "add-task" => {
            let sts = board::statuses(&b);
            let mut last = prefs::get("kanban_add_status");
            if !sts.contains(&last) { last = sts.first().cloned().unwrap_or_default(); }
            ask("Add task", "Task title", "", false, "Add task", Some(status_choice(&b, &last, "Column")), |r| {
                let Some((v, ch)) = r else { return };
                if v.trim().is_empty() { return; }
                prefs::set("kanban_add_status", &ch);
                post(json!({ "action": "addTask", "title": v.trim(), "status": ch }), |_| load());
            });
        }
        "edit-task" => {
            let Some(t) = task(&b, task_id).cloned() else { return };
            let id = task_id.to_string();
            let cur = kit::s(&t, "status");
            ask("Edit task", "Task title", &kit::s(&t, "title"), false, "Next", Some(status_choice(&b, &cur, "Column")), move |r| {
                let Some((title, ch)) = r else { return };
                ask("Edit task", "Description", &kit::s(&t, "description"), true, "Save", None, move |d| {
                    let Some((description, _)) = d else { return };
                    let id2 = id.clone();
                    post(json!({ "action": "updateTask", "taskId": id, "title": title.trim(), "description": description }), move |_| {
                        if !ch.is_empty() && ch != cur { post(json!({ "action": "moveTask", "taskId": id2, "status": ch }), |_| load()); } else { load(); }
                    });
                });
            });
        }
        "del-task" => post(json!({ "action": "deleteTask", "taskId": task_id }), |_| load()),
        "restore-task" => post(json!({ "action": "restoreTask", "taskId": task_id }), |_| load()),
        "restore-todo" => post(json!({ "action": "restoreTodo", "taskId": task_id, "todoId": extra }), |_| load()),
        "brainstorm" => {
            let Some(t) = task(&b, task_id).cloned() else { return };
            let id = task_id.to_string();
            let p = kit::project();
            let todos: Vec<(String, String)> = t.get("todos").and_then(|x| x.as_array()).map(|a| a.iter().filter(|td| !td.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false)).map(|td| (kit::s(td, "id"), kit::s(td, "text"))).collect()).unwrap_or_default();
            let title = kit::s(&t, "title");
            kit::call(move |api| {
                let r = api.post("/api/mindmap", &json!({ "action": "addNode", "text": title, "project": p, "taskId": id }))?;
                if kit::truthy(&r["ok"]) {
                    let node = kit::s(&r["node"], "id");
                    for (tid, text) in &todos {
                        api.post("/api/mindmap", &json!({ "action": "addNode", "text": text, "parentId": node, "project": p, "taskId": id, "todoId": tid }))?;
                    }
                }
                Ok(json!({ "r": r, "n": todos.len() }))
            }, |r| match r {
                Ok(v) if kit::truthy(&v["r"]["ok"]) => {
                    let n = v["n"].as_u64().unwrap_or(0);
                    kit::toast_ok(&format!("Added to Mindmap{}", if n > 0 { format!(" with {n} subtask(s)") } else { String::new() }));
                    kit::show_tab("mindmap");
                }
                Ok(v) => kit::toast_error(&{ let e = kit::s(&v["r"], "error"); if e.is_empty() { "Action failed".to_string() } else { e } }),
                Err(e) => kit::toast_error(&format!("Action failed: {e}")),
            });
        }
        "goto-mindmap" => kit::show_tab("mindmap"),
        _ => {}
    }
}

fn kg_get<R: Default>(f: impl FnOnce(&PlanningKanban) -> R) -> R {
    kit::ui().map(|w| f(&w.global::<PlanningKanban>())).unwrap_or_default()
}

fn bump_focus() { kg(|k| k.set_focus_gen(k.get_focus_gen() + 1)); }

fn task_commit(st: String, text: String, keep: bool) {
    let title = text.trim().to_string();
    kg(|k| k.set_task_draft("".into()));
    if !keep { with(|s| if s.adding_task.as_deref() == Some(st.as_str()) { s.adding_task = None; }); }
    let done = move |_| { load(); if keep { bump_focus(); } };
    if title.is_empty() { render(); return; }
    post(json!({ "action": "addTask", "title": title, "status": st }), done);
}

fn todo_commit(t: String, text: String, keep: bool) {
    let text = text.trim().to_string();
    kg(|k| k.set_todo_draft("".into()));
    if !keep { with(|s| if s.adding_todo.as_deref() == Some(t.as_str()) { s.adding_todo = None; }); }
    if text.is_empty() { render(); return; }
    post(json!({ "action": "addTodo", "taskId": t, "text": text }), move |_| { load(); if keep { bump_focus(); } });
}

fn todo_menu(t: &str, td: &str, x: f32, y: f32) {
    let n = with(|s| s.media_todo.get(td).map(|v| v.len()).unwrap_or(0));
    let (t, td) = (t.to_string(), td.to_string());
    let attach = if n > 0 { format!("Attach media  {n}") } else { "Attach media".to_string() };
    // The web's .kt-pop opens under the button, its right edge on the button's.
    let items = vec![
        kit::item("media", &attach).icon(kit::icon(|i| i.get_paperclip())),
        kit::item("rename", "Rename").icon(kit::icon(|i| i.get_pencil())),
        kit::item("delete", "Delete").icon(kit::icon(|i| i.get_trash())).danger().sep(),
    ];
    let mx = x - kit::ui().map(|w| w.global::<crate::Theme>().get_menu_min_w()).unwrap_or(0.0);
    kit::menu(mx.max(0.0), y, items, move |id| match id.as_str() {
        "media" => todo_media(&t, &td),
        "rename" => {
            let cur = with(|s| s.board.as_ref().and_then(|b| task(b, &t)).and_then(|x| x.get("todos").and_then(|a| a.as_array()).and_then(|a| a.iter().find(|y| kit::s(y, "id") == td)).map(|y| kit::s(y, "text")))).unwrap_or_default();
            kit::prompt("Edit subtask", "Subtask", &cur, "", "OK", false, move |r| {
                if let Some(v) = r.filter(|v| !v.trim().is_empty()) { post(json!({ "action": "updateTodo", "taskId": t, "todoId": td, "text": v.trim() }), |_| load()); }
            });
        }
        "delete" => post(json!({ "action": "deleteTodo", "taskId": t, "todoId": td }), |_| load()),
        _ => {}
    });
}

fn todo_media(t: &str, td: &str) {
    media_modal("Subtask media", json!({ "scope": "project", "kind": "all", "linkFilter": { "type": "todo", "taskId": t, "todoId": td }, "uploadLink": { "type": "todo", "taskId": t, "todoId": td }, "uploadProject": kit::project() }));
}

// ─── Media (the Media modal and viewer are workspace's) ───

fn media_modal(title: &str, state: Value) {
    if !crate::planning::media_bridge::open_modal(title, &state) { kit::toast("Media opens in the Media tab"); kit::show_tab("media"); }
}

fn open_media(id: &str) {
    let it = with(|s| s.media_task.values().chain(s.media_todo.values()).flatten().find(|x| kit::s(x, "id") == id).cloned());
    if let Some(it) = it { if !crate::planning::media_bridge::open_item(&it) { kit::toast("Media opens in the Media tab"); kit::show_tab("media"); } }
}

/// The server's media changed (an upload or delete in the Media modal).
#[allow(dead_code)]
pub fn media_changed() { if visible() { load(); } }

// ─── The attachments strip ───

fn clip_open(t: &str, x: f32, y: f32, w: f32, keep: bool) {
    let open = kg_get(|k| k.get_clip_task().to_string());
    if !keep && open == t { clip_close(); return; }
    kg(|k| { k.set_clip_task(t.into()); k.set_clip_x(x); k.set_clip_y(y); k.set_clip_w(w); });
    fill_clip(t);
}

fn clip_close() { kg(|k| k.set_clip_task("".into())); }

fn refresh_clip() {
    let t = kg_get(|k| k.get_clip_task().to_string());
    if !t.is_empty() { fill_clip(&t); }
}

fn fill_clip(t: &str) {
    let items = with(|s| s.media_task.get(t).cloned().unwrap_or_default());
    let rows: Vec<PlanningThumb> = items.iter().map(|it| {
        let id = kit::s(it, "id");
        let img = with(|s| s.thumbs.get(&id).cloned().flatten());
        PlanningThumb { has_image: img.is_some(), image: img.unwrap_or_default(), name: kit::s(it, "originalName").into(), kind: kit::s(it, "kind").into(), id: id.into() }
    }).collect();
    kg(|k| k.set_clip_items(ModelRc::new(VecModel::from(rows))));
    // Pictures load once each, then the strip is drawn again.
    for it in items {
        let id = kit::s(&it, "id");
        if kit::s(&it, "kind") != "image" || with(|s| s.thumbs.contains_key(&id)) { continue; }
        with(|s| s.thumbs.insert(id.clone(), None));
        let Some(api) = kit::api() else { return };
        let path = format!("/api/media/raw/{}", kit::enc(&id));
        let t = t.to_string();
        kit::bg(move || match api.get_bytes(&path) {
            Ok((200, bytes)) => image::load_from_memory(&bytes).ok().map(|img| {
                let img = img.thumbnail(96, 96).to_rgba8();
                let mut pb = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(img.width(), img.height());
                pb.make_mut_bytes().copy_from_slice(img.as_raw());
                pb
            }),
            _ => None,
        }, move |pb| {
            if let Some(pb) = pb { with(|s| s.thumbs.insert(id, Some(slint::Image::from_rgba8(pb)))); }
            if kg_get(|k| k.get_clip_task().to_string()) == t { fill_clip_rows_only(&t); }
        });
    }
}

fn fill_clip_rows_only(t: &str) {
    let items = with(|s| s.media_task.get(t).cloned().unwrap_or_default());
    let rows: Vec<PlanningThumb> = items.iter().map(|it| {
        let id = kit::s(it, "id");
        let img = with(|s| s.thumbs.get(&id).cloned().flatten());
        PlanningThumb { has_image: img.is_some(), image: img.unwrap_or_default(), name: kit::s(it, "originalName").into(), kind: kit::s(it, "kind").into(), id: id.into() }
    }).collect();
    kg(|k| k.set_clip_items(ModelRc::new(VecModel::from(rows))));
}

// ─── Drag to move ───

fn drag_start(id: &str) {
    clip_close();
    with(|s| { s.col_boxes.clear(); s.card_boxes.clear(); s.target = None; });
    kg(|k| { k.set_drag_id(id.into()); k.set_drop_status("".into()); k.set_drop_index(-1); k.set_measure_gen(k.get_measure_gen() + 1); });
}

fn drag_move(x: f32, y: f32) {
    let Some(w) = kit::ui() else { return };
    let k = w.global::<PlanningKanban>();
    k.set_drag_x(x);
    k.set_drag_y(y);
    let id = k.get_drag_id().to_string();
    let t = with(|s| {
        // Cards in the order shown (the model's), with their reported boxes.
        let order: Vec<(String, String)> = s.board.as_ref().map(|b| board::columns(b, &board::Ui::default()).into_iter().flat_map(|c| c.cards.into_iter().map(move |cd| (c.status.clone(), cd.id))).collect()).unwrap_or_default();
        let cards: Vec<(String, String, Box4)> = order.iter().filter_map(|(st, cid)| s.card_boxes.iter().find(|(_, c, _)| c == cid).map(|(_, _, b)| (st.clone(), cid.clone(), *b))).collect();
        let t = board::drop_target(&s.col_boxes, &cards, &id, x, y);
        s.target = t.clone();
        t
    });
    match t {
        Some((st, i, before)) => { k.set_drop_status(st.into()); k.set_drop_index(i as i32); k.set_drop_before(before.into()); }
        None => { k.set_drop_status("".into()); k.set_drop_index(-1); }
    }
}

fn drag_end(commit: bool) {
    let id = kg_get(|k| k.get_drag_id().to_string());
    kg(|k| { k.set_drag_id("".into()); k.set_drop_status("".into()); k.set_drop_index(-1); });
    let t = with(|s| s.target.take());
    if !commit || id.is_empty() { return; }
    if let Some((st, i, _)) = t {
        post(json!({ "action": "moveTask", "taskId": id, "status": st, "index": i }), |_| load());
    }
}

// ─── For the test driver ───

pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    Some(match name {
        "kanban-act" => { act(&a(0), &a(1), &a(2)); json!({ "ok": true }) }
        "kanban-toggle" => { let (t, td, v) = (a(0), a(1), a(2) == "true"); post(json!({ "action": "updateTodo", "taskId": t, "todoId": td, "done": v }), |_| load()); json!({ "ok": true }) }
        "kanban-type-task" => { kg(|k| k.set_task_draft(a(0).into())); json!({ "ok": true }) }
        "kanban-type-todo" => { kg(|k| k.set_todo_draft(a(0).into())); json!({ "ok": true }) }
        "kanban-commit-task" => { let st = with(|s| s.adding_task.clone()).unwrap_or_default(); task_commit(st, a(0), a(1) == "true"); json!({ "ok": true }) }
        "kanban-commit-todo" => { let t = with(|s| s.adding_todo.clone()).unwrap_or_default(); todo_commit(t, a(0), a(1) == "true"); json!({ "ok": true }) }
        "kanban-todo-menu" => { todo_menu(&a(0), &a(1), a(2).parse().unwrap_or(400.0), a(3).parse().unwrap_or(300.0)); json!({ "ok": true }) }
        "kanban-clip" => { clip_open(&a(0), a(1).parse().unwrap_or(600.0), a(2).parse().unwrap_or(300.0), 300.0, false); json!({ "ok": true }) }
        "kanban-measure" => { kg(|k| k.set_measure_gen(k.get_measure_gen() + 1)); json!({ "ok": true }) }
        "kanban-move" => { let (id, st, i) = (a(0), a(1), a(2).parse::<usize>().unwrap_or(0)); post(json!({ "action": "moveTask", "taskId": id, "status": st, "index": i }), |_| load()); json!({ "ok": true }) }
        "kanban-reload" => { load(); json!({ "ok": true }) }
        _ => return None,
    })
}

pub fn props() -> Value {
    let Some(w) = kit::ui() else { return Value::Null };
    let k = w.global::<PlanningKanban>();
    use slint::Model;
    let boxes = with(|s| (s.col_boxes.iter().map(|(st, b)| json!({ "status": st, "x": b.x, "y": b.y, "w": b.w, "h": b.h })).collect::<Vec<_>>(),
        s.card_boxes.iter().map(|(st, id, b)| json!({ "status": st, "id": id, "x": b.x, "y": b.y, "w": b.w, "h": b.h })).collect::<Vec<_>>()));
    json!({
        "state": k.get_state().as_str(), "error": k.get_error().as_str(), "view": k.get_view().as_str(), "trash": k.get_trash_count(),
        "columns": k.get_columns().iter().map(|c| json!({ "status": c.status.as_str(), "label": c.label.as_str(), "other": c.other, "adding": c.adding,
            "cards": c.cards.iter().map(|cd| json!({ "id": cd.id.as_str(), "title": cd.title.as_str(), "desc": cd.desc.as_str(), "mm": cd.mm, "media": cd.media, "compact": cd.compact, "menuOpen": cd.menu_open, "adding": cd.adding, "done": cd.done, "total": cd.total,
                "todos": cd.todos.iter().map(|t| json!({ "id": t.id.as_str(), "text": t.text.as_str(), "done": t.done, "media": t.media })).collect::<Vec<_>>() })).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "deletedTasks": k.get_deleted_tasks().iter().map(|r| json!({ "text": r.text.as_str(), "task": r.task.as_str() })).collect::<Vec<_>>(),
        "deletedTodos": k.get_deleted_todos().iter().map(|r| json!({ "text": r.text.as_str(), "sub": r.sub.as_str(), "task": r.task.as_str(), "todo": r.todo.as_str() })).collect::<Vec<_>>(),
        "history": k.get_history().iter().map(|r| json!({ "text": r.text.as_str(), "ts": r.sub.as_str() })).collect::<Vec<_>>(),
        "taskDraft": k.get_task_draft().as_str(), "todoDraft": k.get_todo_draft().as_str(),
        "drag": if k.get_drag_id().is_empty() { Value::Null } else { json!({ "id": k.get_drag_id().as_str(), "status": k.get_drop_status().as_str(), "index": k.get_drop_index(), "before": k.get_drop_before().as_str() }) },
        "clip": if k.get_clip_task().is_empty() { Value::Null } else { json!({ "task": k.get_clip_task().as_str(), "items": k.get_clip_items().iter().map(|t| json!({ "id": t.id.as_str(), "kind": t.kind.as_str(), "image": t.has_image })).collect::<Vec<_>>() }) },
        "boxes": { "cols": boxes.0, "cards": boxes.1 },
    })
}
