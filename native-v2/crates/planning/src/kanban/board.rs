//! Kanban's pure rules (native-gpui's kanban_board.rs, from the Slint client's src/planning/board.rs; the web's renderKanbanBoard / renderKanbanCard,
//! renderKanbanTrash, renderKanbanHistory and kanbanDropIndex): the board
//! as the server sends it, turned into what the screen shows. Unit-tested.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const STATUS_LABELS: [(&str, &str); 4] = [("backlog", "Backlog"), ("todo", "To Do"), ("in_progress", "In Progress"), ("done", "Done")];

/// A status's label (the key itself when unknown).
pub fn label(st: &str) -> String {
    STATUS_LABELS.iter().find(|(k, _)| *k == st).map(|(_, l)| l.to_string()).unwrap_or_else(|| st.to_string())
}

fn st(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// The board's columns' statuses (the server's, or the four known ones).
pub fn statuses(board: &Value) -> Vec<String> {
    board.get("statuses").and_then(|s| s.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<_>>())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| STATUS_LABELS.iter().map(|(k, _)| k.to_string()).collect())
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Todo { pub id: String, pub text: String, pub done: bool, pub media: usize }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Card {
    pub id: String, pub title: String, pub desc: String, pub mm: usize, pub media: usize,
    pub compact: bool, pub menu_open: bool, pub adding: bool,
    pub todos: Vec<Todo>, pub done: usize, pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Col { pub status: String, pub label: String, pub other: bool, pub cards: Vec<Card>, pub adding: bool }

/// What the screen keeps across reloads.
#[derive(Default)]
pub struct Ui<'a> {
    pub menu_open: Option<&'a HashSet<String>>,
    pub adding_todo: Option<&'a str>,
    pub adding_task: Option<&'a str>,
    pub media_task: Option<&'a HashMap<String, usize>>,
    pub media_todo: Option<&'a HashMap<String, usize>>,
}

pub fn card(t: &Value, ui: &Ui) -> Card {
    let id = st(t, "id");
    let todos: Vec<Todo> = t.get("todos").and_then(|x| x.as_array()).map(|a| a.iter()
        .filter(|td| !td.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false))
        .map(|td| { let tid = st(td, "id"); Todo { media: ui.media_todo.and_then(|m| m.get(&tid)).copied().unwrap_or(0), id: tid, text: st(td, "text"), done: td.get("done").and_then(|d| d.as_bool()).unwrap_or(false) } })
        .collect()).unwrap_or_default();
    let adding = ui.adding_todo == Some(id.as_str());
    Card {
        title: st(t, "title"),
        desc: st(t, "description"),
        mm: t.get("mindmapNodes").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0),
        media: ui.media_task.and_then(|m| m.get(&id)).copied().unwrap_or(0),
        compact: todos.is_empty() && !adding,
        menu_open: ui.menu_open.map(|m| m.contains(&id)).unwrap_or(false),
        adding,
        done: todos.iter().filter(|x| x.done).count(),
        total: todos.len(),
        todos,
        id,
    }
}

/// The columns, then the yellow "Other" one for tasks whose status is not
/// a column (they never silently disappear).
pub fn columns(board: &Value, ui: &Ui) -> Vec<Col> {
    let sts = statuses(board);
    let tasks: Vec<&Value> = board.get("tasks").and_then(|x| x.as_array()).map(|a| a.iter().filter(|t| !t.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false)).collect()).unwrap_or_default();
    let mut out: Vec<Col> = sts.iter().map(|s| Col {
        status: s.clone(), label: label(s), other: false,
        cards: tasks.iter().filter(|t| st(t, "status") == *s).map(|t| card(t, ui)).collect(),
        adding: ui.adding_task == Some(s.as_str()),
    }).collect();
    let orphans: Vec<Card> = tasks.iter().filter(|t| !sts.contains(&st(t, "status"))).map(|t| card(t, ui)).collect();
    if !orphans.is_empty() {
        out.push(Col { status: "backlog".into(), label: "Other".into(), other: true, cards: orphans, adding: false });
    }
    out
}

/// A Trash row: text, the task's title it was in (todos), the ids.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Row { pub text: String, pub sub: String, pub task: String, pub todo: String }

/// Deleted tasks, and deleted todos (inside any task, deleted or not).
pub fn trash(board: &Value) -> (Vec<Row>, Vec<Row>) {
    let arr = |k: &str| board.get(k).and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let deleted = arr("deletedTasks");
    let tasks_rows = deleted.iter().map(|t| Row { text: st(t, "title"), sub: String::new(), task: st(t, "id"), todo: String::new() }).collect();
    let mut todos = vec![];
    for t in arr("tasks").iter().chain(deleted.iter()) {
        for td in t.get("todos").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            if td.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false) {
                todos.push(Row { text: st(&td, "text"), sub: st(t, "title"), task: st(t, "id"), todo: st(&td, "id") });
            }
        }
    }
    (tasks_rows, todos)
}

/// The history, newest first: (message, when as the local time).
pub fn history(board: &Value, fmt: impl Fn(&str) -> String) -> Vec<Row> {
    let mut h: Vec<Row> = board.get("history").and_then(|x| x.as_array()).map(|a| a.iter().map(|e| Row { text: st(e, "message"), sub: fmt(&st(e, "ts")), task: String::new(), todo: String::new() }).collect()).unwrap_or_default();
    h.reverse();
    h
}

/// An ISO time as the web's toLocaleString (day/month/year, 24-hour).
pub fn local_time(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y, %H:%M:%S").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

/// A box on the window (x, y, w, h).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Box4 { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

impl Box4 {
    pub fn has(&self, x: f32, y: f32) -> bool { x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h }
}

/// Where a dragged card lands: the column under the pointer, and the index
/// among that column's other cards (the first whose middle is below the
/// pointer), with the id of the card the line goes before ("" at the end).
/// `cards` are (status, id, box) in the order shown.
pub fn drop_target(cols: &[(String, Box4)], cards: &[(String, String, Box4)], dragged: &str, x: f32, y: f32) -> Option<(String, usize, String)> {
    let (status, _) = cols.iter().find(|(_, b)| b.has(x, y))?;
    let others: Vec<&(String, String, Box4)> = cards.iter().filter(|(s, id, _)| s == status && id != dragged).collect();
    for (i, (_, id, b)) in others.iter().enumerate() {
        if y < b.y + b.h / 2.0 { return Some((status.clone(), i, id.clone())); }
    }
    Some((status.clone(), others.len(), String::new()))
}

/// Where a dragged card lands, by the column's whole order (a column's
/// list draws only the cards on screen, so only those have boxes): the
/// column under the pointer, the index among its other cards (the first
/// whose middle is below the pointer; a card past the drawn ones, below the
/// viewport, counts as below), and the card the line goes before.
pub fn drop_target_in(cols: &[std::rc::Rc<Col>], col_boxes: &[(String, Box4)], cards: &[(String, String, Box4)], dragged: &str, x: f32, y: f32) -> Option<(String, usize, String)> {
    let (status, _) = col_boxes.iter().find(|(_, b)| b.has(x, y))?;
    let col = cols.iter().find(|c| !c.other && c.status == *status)?;
    let others: Vec<&Card> = col.cards.iter().filter(|c| c.id != dragged).collect();
    let mut seen = false;
    for (i, c) in others.iter().enumerate() {
        match cards.iter().find(|(s, id, _)| s == status && *id == c.id) {
            Some((_, _, b)) => { seen = true; if y < b.y + b.h / 2.0 { return Some((status.clone(), i, c.id.clone())); } }
            None => if seen { return Some((status.clone(), i, c.id.clone())); },
        }
    }
    Some((status.clone(), others.len(), String::new()))
}

/// Media items grouped by their link: (by task id, by todo id) counts and
/// lists (the web's loadKanbanMedia).
pub fn media_groups(items: &[Value]) -> (HashMap<String, Vec<Value>>, HashMap<String, Vec<Value>>) {
    let mut by_task: HashMap<String, Vec<Value>> = HashMap::new();
    let mut by_todo: HashMap<String, Vec<Value>> = HashMap::new();
    for it in items {
        let Some(l) = it.get("link") else { continue };
        match l.get("type").and_then(|t| t.as_str()) {
            Some("task") => by_task.entry(st(l, "taskId")).or_default().push(it.clone()),
            Some("todo") => by_todo.entry(st(l, "todoId")).or_default().push(it.clone()),
            _ => {}
        }
    }
    (by_task, by_todo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn board() -> Value {
        json!({
            "statuses": ["backlog", "todo", "in_progress", "done"],
            "tasks": [
                { "id": "a", "title": "A", "status": "backlog", "todos": [{ "id": "t1", "text": "one", "done": true }, { "id": "t2", "text": "two", "done": false }, { "id": "t3", "text": "gone", "deleted": true }], "mindmapNodes": [{}, {}] },
                { "id": "b", "title": "B", "status": "backlog", "description": "desc" },
                { "id": "c", "title": "C", "status": "weird" }
            ],
            "deletedTasks": [{ "id": "d", "title": "D", "todos": [{ "id": "t9", "text": "x", "deleted": true }] }],
            "history": [{ "ts": "2026-11-15T10:00:00Z", "message": "first" }, { "ts": "2026-11-15T11:00:00Z", "message": "second" }]
        })
    }

    #[test]
    fn columns_and_cards() {
        let b = board();
        let menu: HashSet<String> = ["b".to_string()].into();
        let mt: HashMap<String, usize> = [("a".to_string(), 3)].into();
        let ui = Ui { menu_open: Some(&menu), adding_task: Some("todo"), media_task: Some(&mt), ..Default::default() };
        let cols = columns(&b, &ui);
        assert_eq!(cols.len(), 5);
        assert_eq!(cols[0].label, "Backlog");
        assert_eq!(cols[0].cards.len(), 2);
        let a = &cols[0].cards[0];
        assert_eq!((a.done, a.total, a.mm, a.media, a.compact), (1, 2, 2, 3, false));
        let bb = &cols[0].cards[1];
        assert!(bb.compact && bb.menu_open && bb.desc == "desc");
        assert!(cols[1].adding && !cols[0].adding);
        assert!(cols[4].other && cols[4].label == "Other" && cols[4].cards[0].id == "c");
    }

    #[test]
    fn adding_a_subtask_opens_the_footer() {
        let b = board();
        let ui = Ui { adding_todo: Some("b"), ..Default::default() };
        let c = &columns(&b, &ui)[0].cards[1];
        assert!(c.adding && !c.compact);
    }

    #[test]
    fn trash_and_history() {
        let (t, td) = trash(&board());
        assert_eq!(t, vec![Row { text: "D".into(), sub: "".into(), task: "d".into(), todo: "".into() }]);
        assert_eq!(td.len(), 2);
        assert_eq!((td[0].text.as_str(), td[0].sub.as_str()), ("gone", "A"));
        let h = history(&board(), |s| s.to_string());
        assert_eq!(h[0].text, "second");
    }

    #[test]
    fn drops() {
        let cols = vec![("backlog".to_string(), Box4 { x: 0.0, y: 0.0, w: 100.0, h: 500.0 }), ("todo".to_string(), Box4 { x: 110.0, y: 0.0, w: 100.0, h: 500.0 })];
        let cards = vec![
            ("backlog".to_string(), "a".to_string(), Box4 { x: 0.0, y: 40.0, w: 100.0, h: 60.0 }),
            ("backlog".to_string(), "b".to_string(), Box4 { x: 0.0, y: 110.0, w: 100.0, h: 60.0 }),
        ];
        assert_eq!(drop_target(&cols, &cards, "a", 50.0, 20.0), Some(("backlog".into(), 0, "b".into())));
        assert_eq!(drop_target(&cols, &cards, "a", 50.0, 400.0), Some(("backlog".into(), 1, "".into())));
        assert_eq!(drop_target(&cols, &cards, "x", 50.0, 60.0), Some(("backlog".into(), 0, "a".into())));
        assert_eq!(drop_target(&cols, &cards, "x", 50.0, 120.0), Some(("backlog".into(), 1, "b".into())));
        assert_eq!(drop_target(&cols, &cards, "a", 150.0, 10.0), Some(("todo".into(), 0, "".into())));
        assert_eq!(drop_target(&cols, &cards, "a", 300.0, 10.0), None);
    }

    #[test]
    fn drops_by_the_whole_order() {
        let b = board();
        let cols: Vec<std::rc::Rc<Col>> = columns(&b, &Ui::default()).into_iter().map(std::rc::Rc::new).collect();
        let cb = vec![("backlog".to_string(), Box4 { x: 0.0, y: 0.0, w: 100.0, h: 500.0 })];
        // Only "b" is drawn (a scrolled list): above its middle lands before it, at index 1.
        let cards = vec![("backlog".to_string(), "b".to_string(), Box4 { x: 0.0, y: 100.0, w: 100.0, h: 60.0 })];
        assert_eq!(drop_target_in(&cols, &cb, &cards, "x", 50.0, 110.0), Some(("backlog".into(), 1, "b".into())));
        assert_eq!(drop_target_in(&cols, &cb, &cards, "x", 50.0, 400.0), Some(("backlog".into(), 2, "".into())));
        assert_eq!(drop_target_in(&cols, &cb, &cards, "a", 50.0, 110.0), Some(("backlog".into(), 0, "b".into())));
    }

    #[test]
    fn media() {
        let items = vec![json!({ "id": "m1", "link": { "type": "task", "taskId": "a" } }), json!({ "id": "m2", "link": { "type": "todo", "taskId": "a", "todoId": "t1" } }), json!({ "id": "m3" })];
        let (t, td) = media_groups(&items);
        assert_eq!(t["a"].len(), 1);
        assert_eq!(td["t1"].len(), 1);
    }
}
