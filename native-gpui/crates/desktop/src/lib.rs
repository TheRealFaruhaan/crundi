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
mod bubble;
mod ctx;
mod cru;
mod cru_ui;
pub mod homepool;
pub mod drawer;
mod host;
mod inbox;
mod inbox_ui;
mod notify;
mod marks;
mod passwords;
mod pointer;
mod pwstore;
pub mod rules;
pub mod startup;
mod tray;
mod ui;

use crundi_shell::{Area, AreaEvent, Drawer};
use crundi_webview as web;
use gpui::*;
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

pub use ui::BrowserDrawer;

/// The drawer and the Browsers tab's page, for the pump.
pub struct DrawerHandle(pub Entity<BrowserDrawer>, pub Entity<browsers::BrowsersPage>, pub Entity<inbox_ui::InboxPage>, pub Entity<cru_ui::CruScreen>);
impl Global for DrawerHandle {}

/// Make the drawer and register it, the area and the native host (once,
/// after the window opens; only when CEF started).
pub fn init(window: &mut Window, cx: &mut App) {
    web::engine::set_hooks(web::Hooks {
        changed: |i| drawer::panel_changed(i),
        visited: |i, u| { drawer::visited(i, u); if let Some(t) = drawer::tab_of_panel(i) { passwords::visited(&t.id, u); } },
        popup: |i, u| drawer::popup(i, u),
        load_failed,
        annotating: || drawer::with(|d| d.annotating),
        ask_note: |i, n| drawer::ask_note(i, n),
        console: passwords::console,
    });
    web::net::set_link(host::send_binary, || host::features().iter().any(|f| f == "net-tunnel"));
    crundi_core::host::set(Box::new(host::Host));
    passwords::init();
    drawer::init();
    let view = cx.new(|cx| BrowserDrawer::new(window, cx));
    let bpage = cx.new(|cx| browsers::BrowsersPage::new(window, cx));
    let ipage = cx.new(inbox_ui::InboxPage::new);
    let cscreen = cx.new(|cx| cru_ui::CruScreen::new(window, cx));
    crundi_shell::register_overlay(cx, cscreen.clone().into());
    homepool::init();
    tray::start(false);
    cx.set_global(DrawerHandle(view.clone(), bpage.clone(), ipage.clone(), cscreen));
    crundi_shell::register_drawer(cx, Drawer {
        view: view.into(),
        has_pages: Rc::new(|_| drawer::with(|d| d.projects.get(&d.project.to_lowercase()).is_some_and(|p| !p.tabs.is_empty()))),
    });
    // The top bar's Cru button (the shell's hook).
    crundi_shell::register_action(cx, "cru", |_| cru::click());
    crundi_shell::register_action(cx, "cru-hover", |cx| cru::hover(crundi_chat_composer::voice::any_recording(cx)));
    crundi_shell::register_action(cx, "cru-leave", |_| cru::leave());
    crundi_shell::register_action(cx, "cru-ptt", |_| cru::ptt_down());
    crundi_shell::register_action(cx, "cru-ptt-up", |_| cru::ptt_up());
    crundi_shell::register(cx, Area::new("desktop")
        .tabs(&["browsers", "inbox"], move |tab, _, _| if tab == "inbox" { ipage.clone().into_any_element() } else { bpage.clone().into_any_element() })
        .on_tab(|tab, _| match tab { "browsers" => browsers::load(), "inbox" => inbox::load(), _ => {} })
        .on_project(|alias, _| drawer::set_project(alias))
        .on_signed_in(|cx| signed_in(cx))
        .on_signed_out(|_| { drawer::close(); inbox::signed_out(); })
        .on_event(|e, _| match e {
            AreaEvent::Connected(true) => { drawer::render(); inbox::load(); cru::load_unread(); tray::set_online(true); }
            AreaEvent::Connected(false) => tray::set_online(false),
            AreaEvent::Ws("voice-report", v) => cru::report(v),
            AreaEvent::Server("inbox", v) => inbox::event(v),
            _ => {}
        })
        .test_ui(|name, args, window, cx| { refresh(window, cx); test_ui(name, args) })
        .props(|_, cx| { let mut p = props(); p["bubble"] = bubble::props(cx); json!({ "areas": { "desktop": p } }) }));
    // The pane header's Bubble button (Workspace's) opens a chat's bubble through this hook.
    crundi_shell::register_bubble(cx, |chat, cx| bubble::open(chat, cx));
    // Browser-only mode (test/drive.mjs): a socket of its own to a server.
    if let Ok(url) = std::env::var("CRUNDI_SERVER") { if !url.is_empty() { host::start(url); } }
}

/// The pump (the app's, every 8 ms after CEF's work): the host's requests,
/// the effects the logic asked for, and the drawer's view.
pub fn pump(window: &mut Window, cx: &mut App) {
    refresh(window, cx);
    web::frames::drop_old(window);
    host::pump();
    passwords::pump();
    cru::tick();
    let (live, unread) = cru::mark();
    crundi_shell::set_cru_mark(cx, live, unread);
    for p in tray::take_picks() { tray::act(&p); }
    if let Some((what, arg)) = BUBBLE_ASK.with(|b| b.borrow_mut().take()) {
        match what {
            "open" => { let id = if arg.is_empty() { crundi_chats::focused_chat(cx) } else { arg }; bubble::open(&id, cx); }
            "toggle" => bubble::toggle(cx),
            "close" => bubble::close(cx),
            _ => bubble::send(&arg, cx),
        }
    }
    for f in ctx::take_effects() { f(window, cx); }
    notify::pump();
    for f in ctx::take_main() { f(); }
    if let Some((d, b, i, c)) = cx.try_global::<DrawerHandle>().map(|d| (d.0.clone(), d.1.clone(), d.2.clone(), d.3.clone())) {
        let dirty = ctx::take_dirty();
        d.update(cx, |d, cx| d.pump(dirty, window, cx));
        c.update(cx, |c, cx| c.pump(window, cx));
        if dirty { b.update(cx, |_, cx| cx.notify()); i.update(cx, |_, cx| cx.notify()); c.update(cx, |_, cx| cx.notify()); }
    }
}

/// The session as the logic reads it (ctx), each pump.
fn refresh(_window: &mut Window, cx: &mut App) {
    let s = crundi_shell::session::get(cx);
    let mut c = ctx::Ctx {
        signed_in: s.api.is_some(), base: s.server.clone(), project: s.project.clone(),
        projects: s.projects.clone(), terminals: s.terminals.clone(), tab: crundi_shell::current_tab(cx), api: s.api.clone(), ..Default::default()
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

/// The server's notification for this PC: the system's (notify.rs).
fn notify(p: &Value) { notify::from_server(p); }

/// Go to a chat (a notification's click, the Inbox's Open chat): its
/// project, the workbench. (Focusing its composer waits for the chats'
/// focus_composer, shared/requests.md.)
fn go_to_chat(chat: &str) {
    let c = ctx::get();
    let project = c.terminals.iter().find(|t| ctx::s(t, "id") == chat).map(|t| ctx::s(t, "project")).unwrap_or_default();
    if !project.is_empty() && !project.eq_ignore_ascii_case(&c.project) { ctx::select_project(&project); }
    ctx::show_tab("workbench");
}

thread_local! {
    /// The driver's evals: the last token, and the answers by token.
    /// A bubble action from the driver, for the next pump (it needs the App).
    static BUBBLE_ASK: RefCell<Option<(&'static str, String)>> = const { RefCell::new(None) };
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
fn cru_available() -> bool { cru::available() }

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
        "inbox" => { inbox::open_page(); ok() }
        "inbox-open" => { inbox::open_thread(&a(0)); ok() }
        "inbox-mark-all" => { inbox::mark_all(); ok() }
        "inbox-mark-read" => { inbox::mark_read(); ok() }
        "inbox-delete-thread" => { inbox::delete_thread(); ok() }
        "inbox-delete" => { inbox::delete_item(&a(0)); ok() }
        "inbox-open-chat" => { inbox::open_chat(); ok() }
        "notify-click" => { notify::clicked(&a(0)); ok() }
        "cru" => { cru::toggle(); ok() }
        "cru-keyboard" => { cru::set_keyboard(a(0) != "false"); ok() }
        "cru-send" => { cru::ask(&a(0), true); ok() }
        "cru-menu" => { cru::menu_pick(&a(0)); ok() }
        "pw-save" => { if let Some(t) = drawer::active_tab() { passwords::save(&t.id, &a(0)); } ok() }
        "pw-never" => { if let Some(t) = drawer::active_tab() { passwords::never(&t.id); } ok() }
        "pw-dismiss" => { if let Some(t) = drawer::active_tab() { passwords::dismiss(&t.id); } ok() }
        "pw-fill" => { if let Some(t) = drawer::active_tab() { passwords::fill(&t.id, &a(0)); } ok() }
        "pw-site" => { passwords::site_menu(); ok() }
        "pw-show" => { passwords::show(&a(0)); ok() }
        "pw-unlock" => { passwords::unlock(&a(0)); ok() }
        "pw-remove" => { passwords::remove(&a(0)); ok() }
        "pw-unnever" => { passwords::unnever(&a(0)); ok() }
        "bubble-open" => { let id = Some(a(0)).filter(|x| !x.is_empty()); BUBBLE_ASK.with(|b| *b.borrow_mut() = Some(("open", id.unwrap_or_default()))); ok() }
        "bubble-toggle" => { BUBBLE_ASK.with(|b| *b.borrow_mut() = Some(("toggle", String::new()))); ok() }
        "bubble-close" => { BUBBLE_ASK.with(|b| *b.borrow_mut() = Some(("close", String::new()))); ok() }
        "bubble-send" => { BUBBLE_ASK.with(|b| *b.borrow_mut() = Some(("send", a(0)))); ok() }
        "bubble-snap" => Some(json!({ "ok": bubble::snap(&a(0)) })),
        "tray-pick" => { tray::test_pick(&a(0)); ok() }
        "startup" => Some(json!({ "ok": true, "enabled": startup::enabled() })),
        "homepool-status" => Some(json!({ "ok": true, "status": homepool::status() })),
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
        "inbox": inbox::props(), "cru": cru::props(), "homepool": homepool::status(), "tray": tray::props(), "passwords": passwords::props(), "notifications": notify::props(), "hidden": hidden,
        "host": { "features": host::features() },
    })
}
