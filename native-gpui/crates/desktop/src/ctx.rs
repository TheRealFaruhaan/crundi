//! What the area's logic reads of the window (a copy of the session's
//! project, projects and chats, taken at each pump) and what it asks of it
//! (`on_app`: a toast, the drawer shown or hidden, a confirm, a server call),
//! run at the next pump with the window. The logic itself (drawer.rs,
//! host.rs) is the Slint client's, which reached these through crate::kit.

use crundi_widgets as w;
use gpui::{App, Window};
use serde_json::Value;
use std::cell::RefCell;

#[derive(Default, Clone)]
#[allow(dead_code)]
pub struct Ctx {
    pub signed_in: bool,
    pub base: String,
    pub project: String,
    pub projects: Vec<Value>,
    pub terminals: Vec<Value>,
    /// The workbench's pane keys, and the focused chat.
    pub panes: Vec<String>,
    pub focused: String,
    pub tab: String,
    /// The signed-in server's API (a clone: cheap).
    pub api: Option<crundi_core::api::Api>,
}

type Effect = Box<dyn FnOnce(&mut Window, &mut App)>;

thread_local! {
    static CTX: RefCell<Ctx> = RefCell::new(Ctx::default());
    static EFFECTS: RefCell<Vec<Effect>> = RefCell::new(Vec::new());
    static DIRTY: RefCell<bool> = const { RefCell::new(true) };
}

pub fn get() -> Ctx { CTX.with(|c| c.borrow().clone()) }
pub fn set(c: Ctx) { CTX.with(|x| *x.borrow_mut() = c); }
pub fn project() -> String { CTX.with(|c| c.borrow().project.clone()) }

/// Run with the window at the next pump.
pub fn on_app(f: impl FnOnce(&mut Window, &mut App) + 'static) { EFFECTS.with(|e| e.borrow_mut().push(Box::new(f))); }
pub fn take_effects() -> Vec<Effect> { EFFECTS.with(|e| std::mem::take(&mut *e.borrow_mut())) }

/// The drawer's state changed: draw it again.
pub fn dirty() { DIRTY.with(|d| *d.borrow_mut() = true); }
pub fn take_dirty() -> bool { DIRTY.with(|d| std::mem::replace(&mut *d.borrow_mut(), false)) }

pub fn toast(text: &str) { let t = text.to_string(); on_app(move |win, cx| w::toast(win, cx, t, w::Toast::Info)); }
pub fn toast_ok(text: &str) { let t = text.to_string(); on_app(move |win, cx| w::toast(win, cx, t, w::Toast::Success)); }
pub fn toast_error(text: &str) { let t = text.to_string(); on_app(move |win, cx| w::toast(win, cx, t, w::Toast::Error)); }

/// Ask first; `then(true)` on OK, `then(false)` on Cancel.
pub fn confirm(title: &str, ok: &str, danger: bool, then: impl Fn(bool) + 'static) {
    let (title, ok) = (title.to_string(), ok.to_string());
    on_app(move |win, cx| {
        let then = std::rc::Rc::new(then);
        let t2 = then.clone();
        w::confirm(win, cx, w::Confirm { title: title.into(), message: "".into(), ok: ok.into(), danger }, move |_, _| t2(true));
        let _ = then;
    });
}

/// A server call off the UI thread; `done` back on it.
pub fn call(work: impl FnOnce(&crundi_core::api::Api) -> crundi_core::api::ApiResult + Send + 'static, done: impl FnOnce(Result<Value, String>) + 'static) {
    on_app(move |_, cx| crundi_shell::call(cx, work, move |r, _| done(r)));
}

pub fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
pub fn n(v: &Value, k: &str) -> f64 { v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) }
pub fn truthy(v: &Value) -> bool { match v { Value::Bool(b) => *b, Value::Null => false, Value::Number(n) => n.as_f64() != Some(0.0), Value::String(s) => !s.is_empty(), _ => true } }

/// Percent-encode a URL component.
pub fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { o.push(b as char); } else { o.push_str(&format!("%{b:02X}")); }
    }
    o
}

/// Show a project tab (or the Inbox's page, "inbox").
pub fn show_tab(tab: &str) {
    let t = tab.to_string();
    on_app(move |_, cx| if let Some(s) = crundi_shell::shell(cx) { s.update(cx, |s, cx| s.show_tab(&t, cx)); });
}

/// Open another project.
pub fn select_project(alias: &str) {
    let a = alias.to_string();
    on_app(move |_, cx| crundi_shell::session::select_project(cx, &a));
}

/// Work with the signed-in API off the UI thread (bytes, say); `done` back on it.
pub fn bg<T: Send + 'static>(work: impl FnOnce(&crundi_core::api::Api) -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    on_app(move |_, cx| {
        let Some(api) = crundi_shell::api(cx) else { return };
        crundi_shell::run(cx, move || work(&api), move |r, _| done(r));
    });
}

/// Bring the window to the front (a notification's click).
pub fn raise() { on_app(|win, _| win.activate_window()); }

/// Calls from other threads (the microphone's, the speaker's), run on the
/// main thread at the next pump.
static MAIN: std::sync::Mutex<Vec<Box<dyn FnOnce() + Send>>> = std::sync::Mutex::new(Vec::new());
pub fn main_thread(f: impl FnOnce() + Send + 'static) { MAIN.lock().unwrap().push(Box::new(f)); }
pub fn take_main() -> Vec<Box<dyn FnOnce() + Send>> { std::mem::take(&mut *MAIN.lock().unwrap()) }
