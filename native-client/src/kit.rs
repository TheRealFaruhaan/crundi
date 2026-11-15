//! What every area's Rust uses (AREAS.md): the server, the current project,
//! the live link, toasts, the shared dialogs (confirm, prompt, the context
//! menu) and small helpers. Areas call these and never reach into app.rs.
//! Only the Lead changes this file.

// The areas use these as they come; not every helper is used yet.
#![allow(dead_code, unused_imports)]

use crate::{MainWindow, MenuEntry};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};

pub use crate::api::{Api, ApiError, ApiResult};

// ─── The app ───

/// The window (on the UI thread; None before it starts or after it closes).
pub fn ui() -> Option<MainWindow> {
    crate::app::window()
}

/// The signed-in server's API (None when signed out).
pub fn api() -> Option<Api> {
    crate::app::api_now()
}

/// The server's base URL ("https://host"), "" when signed out.
pub fn base() -> String {
    api().map(|a| a.base().to_string()).unwrap_or_default()
}

/// A server call off the UI thread; `done` runs on the UI thread with the
/// answer (an ended sign-in signs out on its own and `done` is not called).
/// The error is the server's words, or "Not supported by this server" for a
/// 404 (an older server: say so and don't break, README "Adding a screen").
pub fn call(work: impl FnOnce(&Api) -> ApiResult + Send + 'static, done: impl FnOnce(Result<Value, String>) + Send + 'static) {
    crate::app::call_api(work, done)
}

/// Any blocking work off the UI thread, then `done` on it.
pub fn bg<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + Send + 'static) {
    std::thread::spawn(move || {
        let r = work();
        let _ = slint::invoke_from_event_loop(move || done(r));
    });
}

/// The current project's alias ("" for none).
pub fn project() -> String {
    crate::app::with(|a| a.project.clone()).unwrap_or_default()
}

/// The projects as the server lists them ({alias, name, path, …}).
pub fn projects() -> Vec<Value> {
    crate::app::with(|a| a.projects.clone()).unwrap_or_default()
}

/// The last "state" event (projects, terminals, services…), if any.
pub fn terminals() -> Option<Vec<Value>> {
    crate::app::with(|a| a.terminals.clone()).flatten()
}

/// Send a message on /ws (subscribe-style). Not resent after a reconnect:
/// send it again on AreaEvent::Connected(true).
pub fn ws_send(msg: &Value) {
    crate::app::with(|a| if let Some(l) = &a.live { l.send(msg); });
}

/// Show a tab (as a click on it).
pub fn show_tab(tab: &str) {
    if let Some(w) = ui() { w.set_current_tab(tab.into()); }
}

/// The toast: info, error (red edge) or success (green edge).
pub fn toast(text: &str) { crate::app::toast(text, false); }
pub fn toast_error(text: &str) { crate::app::toast(text, true); }
pub fn toast_ok(text: &str) { crate::app::toast_ok(text); }

// ─── JSON and text ───

/// A field as text ("" when missing or null; numbers and bools as written).
pub fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    }
}

/// JavaScript's truthiness.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(x) => !x.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
    }
}

/// A number field (0 when missing).
pub fn n(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0)
}

/// encodeURIComponent.
pub fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~!*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// "path?alias=<current project>" (most project endpoints).
pub fn with_project(path: &str) -> String {
    let sep = if path.contains('?') { '&' } else { '?' };
    format!("{path}{sep}alias={}", enc(&project()))
}

/// An ISO date as seconds since 1970.
pub fn parse_iso(s: &str) -> Option<f64> {
    crate::app::parse_iso(s)
}

/// "just now", "5m ago", "3h ago", "2d ago" (the web's relative times).
pub fn ago(secs_since_epoch: f64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    ago_at(now, secs_since_epoch)
}

pub fn ago_at(now: f64, then: f64) -> String {
    let d = (now - then).max(0.0);
    if d < 60.0 { "just now".into() }
    else if d < 3600.0 { format!("{}m ago", (d / 60.0) as u64) }
    else if d < 86400.0 { format!("{}h ago", (d / 3600.0) as u64) }
    else { format!("{}d ago", (d / 86400.0) as u64) }
}

/// 1.2 KB, 3.4 MB (the web's formatSize).
pub fn size(bytes: f64) -> String {
    if bytes < 1024.0 { format!("{} B", bytes as u64) }
    else if bytes < 1024.0 * 1024.0 { format!("{:.1} KB", bytes / 1024.0) }
    else if bytes < 1024.0 * 1024.0 * 1024.0 { format!("{:.1} MB", bytes / 1024.0 / 1024.0) }
    else { format!("{:.1} GB", bytes / 1024.0 / 1024.0 / 1024.0) }
}

// ─── Lists ───

/// A list's model, set once on its property; then `sync_rows` rewrites only
/// the rows that changed (README: lists are never replaced wholesale, so
/// scroll and hover stay put).
pub fn model<T: Clone + 'static>() -> Rc<VecModel<T>> {
    Rc::new(VecModel::default())
}

pub fn rc<T: Clone + 'static>(m: &Rc<VecModel<T>>) -> ModelRc<T> {
    ModelRc::from(m.clone())
}

pub fn sync_rows<T: Clone + PartialEq + 'static>(m: &VecModel<T>, rows: Vec<T>) {
    let n = rows.len();
    for (i, r) in rows.into_iter().enumerate() {
        if i < m.row_count() {
            if m.row_data(i).as_ref() != Some(&r) { m.set_row_data(i, r); }
        } else {
            m.push(r);
        }
    }
    while m.row_count() > n { m.remove(m.row_count() - 1); }
}

// ─── The shared dialogs (ui/dialogs.slint) ───

type Answer = Box<dyn FnOnce(bool, String)>;

thread_local! {
    static DIALOG: RefCell<Option<Answer>> = const { RefCell::new(None) };
    static MENU: RefCell<Option<Box<dyn FnOnce(String)>>> = const { RefCell::new(None) };
}

/// Set up the dialogs' callbacks (areas::init).
pub(crate) fn init(w: &MainWindow) {
    let d = w.global::<crate::DialogUi>();
    d.on_answer(|ok, value| dialog_answer(ok, value.to_string()));
    let m = w.global::<crate::MenuUi>();
    m.on_picked(|id| { let f = MENU.with(|x| x.borrow_mut().take()); if let Some(f) = f { f(id.to_string()); } });
    m.on_closed(|| { MENU.with(|x| x.borrow_mut().take()); });
}

/// What a confirm says: its title, its words, the action's label, and
/// whether the action is destructive (red).
pub struct Confirm<'a> {
    pub title: &'a str,
    pub message: &'a str,
    pub ok: &'a str,
    pub danger: bool,
}

/// Ask first (the web's confirm()): `then(true)` on OK or Enter, `then(false)`
/// on Cancel, ×, Esc or a click outside. A second open replaces the first
/// (which hears false).
pub fn confirm(c: Confirm, then: impl FnOnce(bool) + 'static) {
    open_dialog(c.title, c.message, c.ok, c.danger, None, false, "", Box::new(move |ok, _| then(ok)));
}

/// Ask for text (the web's .input-modal): `then(Some(text))` on OK or Enter
/// (Ctrl/Cmd+Enter when multiline), `then(None)` on cancel.
pub fn prompt(title: &str, message: &str, value: &str, placeholder: &str, ok: &str, multiline: bool, then: impl FnOnce(Option<String>) + 'static) {
    open_dialog(title, message, ok, false, Some(value), multiline, placeholder, Box::new(move |okd, v| then(okd.then_some(v))));
}

#[allow(clippy::too_many_arguments)]
fn open_dialog(title: &str, message: &str, ok: &str, danger: bool, value: Option<&str>, multiline: bool, placeholder: &str, f: Answer) {
    let old = DIALOG.with(|x| x.borrow_mut().replace(f));
    if let Some(old) = old { old(false, String::new()); }
    let Some(w) = ui() else { return };
    let d = w.global::<crate::DialogUi>();
    d.set_title(title.into());
    d.set_message(message.into());
    d.set_ok_label(if ok.is_empty() { "OK".into() } else { ok.into() });
    d.set_cancel_label("Cancel".into());
    d.set_danger(danger);
    d.set_prompt(value.is_some());
    d.set_multiline(multiline);
    d.set_value(value.unwrap_or("").into());
    d.set_placeholder(placeholder.into());
    d.set_gen(d.get_gen() + 1);
    d.set_open(true);
}

/// The dialog's answer (its buttons, keys, and the test driver).
pub fn dialog_answer(ok: bool, value: String) {
    if let Some(w) = ui() { w.global::<crate::DialogUi>().set_open(false); }
    let f = DIALOG.with(|x| x.borrow_mut().take());
    if let Some(f) = f { f(ok, value); }
}

/// For the test driver: the open dialog, or null.
pub fn dialog_of() -> Value {
    let Some(w) = ui() else { return Value::Null };
    let d = w.global::<crate::DialogUi>();
    if !d.get_open() { return Value::Null; }
    json!({ "title": d.get_title().as_str(), "message": d.get_message().as_str(), "ok": d.get_ok_label().as_str(), "danger": d.get_danger(), "prompt": d.get_prompt(), "value": d.get_value().as_str() })
}

/// One item of a context menu.
#[derive(Clone, Default)]
pub struct Item {
    pub id: String,
    pub label: String,
    pub icon: Option<slint::Image>,
    pub danger: bool,
    pub sep: bool,
    pub disabled: bool,
}

/// An item: `item("rename", "Rename")`, then `.icon(...)`, `.danger()`, `.sep()`.
pub fn item(id: &str, label: &str) -> Item {
    Item { id: id.into(), label: label.into(), ..Default::default() }
}

impl Item {
    pub fn icon(mut self, i: slint::Image) -> Item { self.icon = Some(i); self }
    pub fn danger(mut self) -> Item { self.danger = true; self }
    /// A line before this item.
    pub fn sep(mut self) -> Item { self.sep = true; self }
    pub fn disabled(mut self, off: bool) -> Item { self.disabled = off; self }
}

/// An icon from ui/icons.slint by its getter (for menu items):
/// `kit::icon(|i| i.get_trash())`.
pub fn icon(get: impl Fn(&crate::Icons) -> slint::Image) -> slint::Image {
    ui().map(|w| get(&w.global::<crate::Icons>())).unwrap_or_default()
}

/// Open the context menu at a window point (a ListRow's `context(x, y)`);
/// `then(id)` when an item is picked (nothing when it is dismissed).
pub fn menu(x: f32, y: f32, items: Vec<Item>, then: impl FnOnce(String) + 'static) {
    MENU.with(|m| *m.borrow_mut() = Some(Box::new(then)));
    let Some(w) = ui() else { return };
    let rows: Vec<MenuEntry> = items.into_iter().map(|i| MenuEntry {
        id: i.id.into(), label: i.label.into(), has_icon: i.icon.is_some(), icon: i.icon.unwrap_or_default(),
        danger: i.danger, sep: i.sep, enabled: !i.disabled,
    }).collect();
    let m = w.global::<crate::MenuUi>();
    m.set_items(ModelRc::new(VecModel::from(rows)));
    m.set_x(x);
    m.set_y(y);
    m.set_open(true);
}

/// The test driver's pick of a menu item by id.
pub fn menu_pick(id: &str) {
    let Some(w) = ui() else { return };
    let m = w.global::<crate::MenuUi>();
    if !m.get_open() { return; }
    m.set_open(false);
    m.invoke_picked(id.into());
}

/// For the test driver: the open menu's items, or null.
pub fn menu_of() -> Value {
    let Some(w) = ui() else { return Value::Null };
    let m = w.global::<crate::MenuUi>();
    if !m.get_open() { return Value::Null; }
    json!({ "x": m.get_x(), "y": m.get_y(), "items": m.get_items().iter().map(|i| json!({ "id": i.id.as_str(), "label": i.label.as_str(), "danger": i.danger, "enabled": i.enabled })).collect::<Vec<_>>() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_times() {
        assert_eq!(ago_at(1000.0, 990.0), "just now");
        assert_eq!(ago_at(1000.0, 1000.0 - 300.0), "5m ago");
        assert_eq!(ago_at(100000.0, 100000.0 - 7200.0), "2h ago");
        assert_eq!(ago_at(1e6, 1e6 - 3.0 * 86400.0), "3d ago");
    }

    #[test]
    fn sizes() {
        assert_eq!(size(12.0), "12 B");
        assert_eq!(size(2048.0), "2.0 KB");
        assert_eq!(size(3.5 * 1024.0 * 1024.0), "3.5 MB");
    }

    #[test]
    fn encoding() {
        assert_eq!(enc("a b/c"), "a%20b%2Fc");
        assert_eq!(enc("x(y)!"), "x(y)!");
    }

    #[test]
    fn fields() {
        let v = json!({ "a": "x", "b": 3, "c": null, "d": "4.5" });
        assert_eq!(s(&v, "a"), "x");
        assert_eq!(s(&v, "b"), "3");
        assert_eq!(s(&v, "c"), "");
        assert_eq!(n(&v, "d"), 4.5);
        assert!(truthy(&json!(1)) && !truthy(&json!("")) && !truthy(&Value::Null));
    }
}
