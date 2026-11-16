//! The desktop area (AREAS.md): the browser drawer (beside the whole project
//! area, below the top bar, as the PC client's app/vendor/crundi-browser-drawer.js)
//! on the webview crate's pages, the native host's full browser protocol
//! (the server's browser_* tools drive this client's pages), and later the
//! Browsers tab, notifications and the tray, the Inbox, saved passwords,
//! the homepool node and Cru.
//!
//! - `drawer`: tabs per project, the address bar, devices, networks,
//!   annotating, history, clearing, sleep (the Slint client's logic);
//! - `ui`: the drawer's view;
//! - `host`: the native host (the Slint client's src/server.rs);
//! - `marks`, `pointer`: a chat's marks and pointer on a page;
//! - `rules`: the pure rules, unit-tested;
//! - `ctx`: what the logic reads of the window and asks of it.

mod browsers;
mod ctx;
pub mod drawer;
mod host;
mod marks;
mod passwords;
mod pointer;
pub mod rules;
mod ui;

use crundi_shell::{Area, AreaEvent, Drawer};
use crundi_webview as web;
use gpui::*;
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

pub use ui::BrowserDrawer;

/// The drawer and the Browsers tab's page, for the pump.
pub struct DrawerHandle(pub Entity<BrowserDrawer>, pub Entity<browsers::BrowsersPage>);
impl Global for DrawerHandle {}

/// Make the drawer and register it, the area and the native host (once,
/// after the window opens; only when CEF started).
pub fn init(window: &mut Window, cx: &mut App) {
    web::engine::set_hooks(web::Hooks {
        changed: |i| drawer::panel_changed(i),
        visited: |i, u| drawer::visited(i, u),
        popup: |i, u| drawer::popup(i, u),
        load_failed,
        annotating: || drawer::with(|d| d.annotating),
        ask_note: |i, n| drawer::ask_note(i, n),
    });
    web::net::set_link(host::send_binary, || host::features().iter().any(|f| f == "net-tunnel"));
    crundi_core::host::set(Box::new(host::Host));
    drawer::init();
    let view = cx.new(|cx| BrowserDrawer::new(window, cx));
    let bpage = cx.new(|cx| browsers::BrowsersPage::new(window, cx));
    cx.set_global(DrawerHandle(view.clone(), bpage.clone()));
    crundi_shell::register_drawer(cx, Drawer {
        view: view.into(),
        has_pages: Rc::new(|_| drawer::with(|d| d.projects.get(&d.project.to_lowercase()).is_some_and(|p| !p.tabs.is_empty()))),
    });
    crundi_shell::register(cx, Area::new("desktop")
        .tabs(&["browsers"], move |_, _, _| bpage.clone().into_any_element())
        .on_tab(|tab, _| if tab == "browsers" { browsers::load(); })
        .on_project(|alias, _| drawer::set_project(alias))
        .on_signed_in(|cx| signed_in(cx))
        .on_signed_out(|_| drawer::close())
        .on_event(|e, _| if let AreaEvent::Connected(true) = e { drawer::render(); })
        .test_ui(|name, args, window, cx| { refresh(window, cx); test_ui(name, args) })
        .props(|_, _| json!({ "areas": { "desktop": props() } })));
    // Browser-only mode (test/drive.mjs): a socket of its own to a server.
    if let Ok(url) = std::env::var("CRUNDI_SERVER") { if !url.is_empty() { host::start(url); } }
}

/// The pump (the app's, every 8 ms after CEF's work): the host's requests,
/// the effects the logic asked for, and the drawer's view.
pub fn pump(window: &mut Window, cx: &mut App) {
    refresh(window, cx);
    host::pump();
    for f in ctx::take_effects() { f(window, cx); }
    if let Some((d, b)) = cx.try_global::<DrawerHandle>().map(|d| (d.0.clone(), d.1.clone())) {
        let dirty = ctx::take_dirty();
        d.update(cx, |d, cx| d.pump(dirty, window, cx));
        if dirty { b.update(cx, |_, cx| cx.notify()); }
    }
}

/// The session as the logic reads it (ctx), each pump.
fn refresh(_window: &mut Window, cx: &mut App) {
    let s = crundi_shell::session::get(cx);
    let mut c = ctx::Ctx {
        signed_in: s.api.is_some(), base: s.server.clone(), project: s.project.clone(),
        projects: s.projects.clone(), terminals: s.terminals.clone(), tab: crundi_shell::current_tab(cx), ..Default::default()
    };
    c.panes = crundi_workbench::workbench(cx).map(|wb| wb.update(cx, |wb, cx| wb.panes(cx))).unwrap_or_default()
        .iter().filter_map(|p| p.get("key").and_then(|k| k.as_str()).map(str::to_string)).collect();
    c.focused = crundi_chats::focused_chat(cx);
    if c.project != ctx::project() { let p = c.project.clone(); ctx::set(c); drawer::set_project(&p); } else { ctx::set(c); }
}

fn signed_in(cx: &mut App) {
    let base = crundi_shell::session::get(cx).server.clone();
    let local = base.contains("://localhost") || base.contains("://127.0.0.1") || base.contains("://[::1]");
    web::net::set_local(local);
    drawer::with(|d| { d.mode = if local { "local".into() } else { "remote".into() }; d.status = if local { "direct".into() } else { "offline".into() }; });
    drawer::render();
}

// ─── What the host and the pages tell the area ───

/// Registered: the server's features are known (the drawer's network labels).
fn host_up() {
    let tunnel = host::features().iter().any(|f| f == "net-tunnel");
    drawer::with(|d| if d.mode == "remote" { d.status = if tunnel { "tunnel".into() } else { "unsupported".into() }; });
    drawer::render();
}

fn host_down() {
    web::net::dropped();
    drawer::with(|d| if d.mode == "remote" { d.status = "offline".into(); });
    drawer::render();
}

/// A hidden browser opened or closed: the Browsers tab, when shown, loads again.
fn hidden_changed() { browsers::reload_soon(); }

/// The server's notification for this PC (phase 6.1 shows it): kept for the driver.
fn notify(p: &Value) {
    NOTIFIED.with(|n| n.borrow_mut().push(json!({ "title": ctx::s(p, "title"), "body": ctx::s(p, "body"), "url": ctx::s(p, "url") })));
}

thread_local! {
    static NOTIFIED: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
    /// The driver's evals: the last token, and the answers by token.
    static EVALS: RefCell<(u64, serde_json::Map<String, Value>)> = RefCell::new((0, serde_json::Map::new()));
}

/// A page's load failed: a drawer tab shows why.
fn load_failed(i: usize) {
    let e = web::engine::with(|st| st.panels[i].load_error.clone()).unwrap_or_default();
    if let Some(t) = drawer::tab_of_panel(i) {
        drawer::with(|d| if let Some(x) = d.projects.values_mut().flat_map(|p| p.tabs.iter_mut()).find(|x| x.id == t.id) { x.diag = e; });
        drawer::render();
    }
}

// ─── Homepool and Cru for the drawer (phase 6) ───

fn hp_name(_id: &str) -> Option<String> { None }
fn hp_refresh() {}
fn hp_offered() -> Vec<(String, String, bool)> { vec![] }
fn hp_available() -> bool { false }
fn hp_has_key() -> bool { false }
fn hp_exit_up(_net: &str) -> bool { false }
fn exit_text(_net: &str) -> String { "checking…".into() }
fn cru_available() -> bool { false }

// ─── The test driver ───

/// The driver's "desktop:<name>" actions (the Slint client's names).
fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let ok = || Some(json!({ "ok": true }));
    match name {
        "ping" => Some(json!({ "ok": true, "area": "desktop" })),
        "browser" => { drawer::toggle(); ok() }
        "drawer" => { if a(0) == "true" { drawer::open() } else { drawer::close() }; ok() }
        "new-tab" => match drawer::new_tab(&ctx::project(), &Some(a(0)).filter(|u| !u.is_empty()).unwrap_or_else(|| drawer::START.into()), &Some(a(1)).filter(|u| !u.is_empty()).unwrap_or_else(|| "client".into()), true, true) {
            Ok(id) => { drawer::show_active(); Some(json!({ "ok": true, "id": id })) }
            Err(e) => Some(json!({ "ok": false, "error": e })),
        },
        "close-tab" => { drawer::close_tab(&a(0)); ok() }
        "activate" => { drawer::activate(&a(0)); ok() }
        "go" => { drawer::go(&a(0)); ok() }
        "pop" => { drawer::pop_open(&a(0)); ok() }
        "pick" => { drawer::pop_pick(&a(0)); ok() }
        "annotate" => { drawer::annotate(a(0) != "false"); ok() }
        "tool" => { drawer::set_tool(&a(0)); ok() }
        "note" => { drawer::note(&a(0), a(1) != "false"); ok() }
        "send" => { drawer::send(a(0) == "shot"); ok() }
        "panel" => { drawer::open_panel(&a(0)); ok() }
        "panel-close" => { drawer::panel_close(); ok() }
        "history-query" => { drawer::history_query(&a(0)); ok() }
        "clear-run" => { drawer::clear_run(); ok() }
        "hide" => { drawer::close(); ok() }
        "close-all" => { drawer::close_all(); ok() }
        "url-typed" => { drawer::url_typed(&a(0)); ok() }
        // A real click on a tab's page, at page px.
        "click" => {
            let Some(i) = drawer::find(&a(0)).and_then(|(_, t)| t.panel) else { return Some(json!({ "ok": false, "error": "no page" })) };
            let r = web::engine::mouse(i, a(1).parse().unwrap_or(0.0), a(2).parse().unwrap_or(0.0), "click");
            Some(json!({ "ok": r.is_ok() }))
        }
        "browsers-open" => { browsers::open(&a(0)); ok() }
        "browsers-shot" => { browsers::shot(&a(0)); ok() }
        "browsers-go" => { browsers::go(&a(0), &a(1)); ok() }
        "browsers-close" => { browsers::close(&a(0)); ok() }
        // Run code in a tab's page; the answer comes in props.evals[token].
        "eval" => {
            let Some(i) = drawer::find(&a(0)).and_then(|(_, t)| t.panel) else { return Some(json!({ "ok": false, "error": "no page" })) };
            let token = EVALS.with(|e| { let mut e = e.borrow_mut(); e.0 += 1; e.0 });
            web::engine::eval(i, &a(1), Box::new(move |r| {
                let v = match r { Ok(v) => json!({ "ok": true, "result": v }), Err(e) => json!({ "ok": false, "error": e }) };
                EVALS.with(|e| { e.borrow_mut().1.insert(token.to_string(), v); });
            }));
            Some(json!({ "ok": true, "token": token.to_string() }))
        }
        _ => None,
    }
}

/// What the test driver sees of the area (the Slint client's names).
fn props() -> Value {
    let active = drawer::active_tab().and_then(|t| t.panel).map(|i| i as i64).unwrap_or(-1);
    let hidden: Vec<Value> = web::engine::with(|st| st.panels.iter().enumerate().filter(|(_, p)| !p.closed && p.key.is_some()).map(|(i, p)| json!({ "key": p.key, "panel": i, "url": p.url })).collect());
    json!({
        "drawer": drawer::props(), "drawerActive": active, "evals": EVALS.with(|e| Value::Object(e.borrow().1.clone())),
        "browsers": browsers::props(),
        "notifications": NOTIFIED.with(|n| Value::Array(n.borrow().clone())), "hidden": hidden,
        "host": { "features": host::features() },
    })
}
