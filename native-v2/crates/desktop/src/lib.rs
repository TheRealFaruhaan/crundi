//! The desktop area (AREAS.md): the browser drawer (beside the whole project
//! area, below the top bar, as the PC client's app/vendor/crundi-browser-drawer.js)
//! on the webview crate's pages, the native host's full browser protocol
//! (the server's browser_* tools drive this client's pages), the Browsers
//! tab, notifications and the tray, the Inbox, the chat bubble, saved
//! passwords, the homepool node and Cru.
//!
//! - `drawer`: tabs per project, the address bar, devices, networks,
//!   annotating, history, clearing, sleep (the Slint client's logic);
//! - `ui`: the drawer's view;
//! - `host`: the native host (the Slint client's src/server.rs);
//! - `marks`, `pointer`: a chat's marks and pointer on a page;
//! - `rules`: the pure rules, unit-tested;
//! - `ctx`: what the logic reads of the window and asks of it.
//!
//! **How it runs (ARCHITECTURE.md rules 1, 2, 6).** The logic is native-gpui's
//! (the Slint client's before it): plain state on the UI thread, no GPUI.
//! Nothing polls. Whatever has work for it (the browser host's frames and
//! events, the native host's requests, a deferred call, a timer, an effect
//! asked through `ctx`, a microphone's thread) wakes the area's pump task
//! (`crundi_webview::wake`), which runs `pump` at most once a frame and then
//! sleeps until the next wake or the next deadline (`next_due`). The views
//! render from their own copies, taken in the pump when the logic said
//! something changed (`ctx::dirty`, a panel's `changed`), and notify
//! themselves only when their copy moved. A page's new frame notifies its
//! `PageView` alone.
mod browsers;
mod bubble;
mod ctx;
mod cru;
mod cru_blocks;
mod cru_field;
mod cru_frame;
mod cru_kid;
mod cru_screen;
mod cru_ui;
pub mod homepool;
pub mod drawer;
mod host;
mod hosts;
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
pub mod wake;

use crundi_shell::{Area, Drawer, Stores};
use crundi_core::live::Event;
use crundi_webview as web;
use futures::StreamExt as _;
use gpui::*;
use serde_json::{json, Value};
use std::{cell::RefCell, time::{Duration, Instant}};

pub use ui::BrowserDrawer;

/// The area's views (handles only, set once when the window opens).
#[derive(Clone)]
pub struct Views {
    pub drawer: Entity<BrowserDrawer>,
    pub browsers: Entity<browsers::BrowsersPage>,
    pub inbox: Entity<inbox_ui::InboxPage>,
    pub cru: Entity<cru_ui::CruScreen>,
}
struct ViewsHandle(Views);
/// Cru's screen, made at init (an overlay: registered before the Shell is made).
struct CruView(Entity<cru_ui::CruScreen>);
impl Global for CruView {}
impl Global for ViewsHandle {}
fn views(cx: &App) -> Option<Views> { cx.try_global::<ViewsHandle>().map(|v| v.0.clone()) }

/// The pump's channel's sending half (`crundi_webview::wake` sends on it).
static WAKE: std::sync::OnceLock<futures::channel::mpsc::UnboundedSender<()>> = std::sync::OnceLock::new();

/// Register the area (before the window opens): its tabs, the shell's
/// actions, the live events it hears, the driver's actions and state.
pub fn init(cx: &mut App) {
    web::engine::set_hooks(web::Hooks {
        changed: |i| drawer::panel_changed(i),
        visited: |i, u| { drawer::visited(i, u); if let Some(t) = drawer::tab_of_panel(i) { passwords::visited(&t.id, u); } },
        popup: |i, u| drawer::popup(i, u),
        load_failed,
        annotating: || drawer::with(|d| d.annotating),
        ask_note: |i, n| drawer::ask_note(i, n),
        console: passwords::console,
        reload: drawer::reload_panel,
    });
    // The browser host stays while the drawer is open, even with no page.
    web::set_keep_alive(|| drawer::with(|d| d.open));
    web::net::set_link(host::send_binary, || host::features().iter().any(|f| f == "net-tunnel"));
    crundi_core::host::set(Box::new(host::Host));
    let (tx, rx) = futures::channel::mpsc::unbounded::<()>();
    let _ = WAKE.set(tx);
    web::set_waker(|| { if let Some(t) = WAKE.get() { let _ = t.unbounded_send(()); } });
    PUMP_RX.with(|r| *r.borrow_mut() = Some(rx));
    passwords::init();
    hosts::init(cx);
    drawer::init();
    homepool::init();
    wake::init();
    tray::start(false);
    // Cru's screen: a layer over the window (the shell reads its overlays when it makes the Shell,
    // before the window's views are made); its typing box comes with the window (init_window).
    let cru_view = cx.new(cru_ui::CruScreen::new);
    crundi_shell::register_overlay(cx, cru_view.clone().into());
    cx.set_global(CruView(cru_view));
    crundi_shell::register(cx, Area::new("desktop")
        .tab("browsers", |_, _, cx| views(cx).map(|v| v.browsers.into()).expect("the desktop's views"))
        // (The page loads as it comes on show: InboxPage hears the tab.)
        .tab("inbox", |_, _, cx| views(cx).map(|v| v.inbox.into()).expect("the desktop's views"))
        // The project bar's Browser button: the drawer's own open (page and host).
        .action("browser", |_, _, _| { if ctx::project().is_empty() { ctx::toast_error("Select a project first") } else { drawer::open() } })
        .action("cru", |_, _, _| cru::click())
        // The top bar's Cru mark: rested on (a moment), left; push to talk held and let go.
        // (A chat's voice holding the mic would keep a hover from opening Cru: v2's chats have none yet.)
        .action("cru-hover", |_, _, _| cru::hover(false))
        .action("cru-leave", |_, _, _| cru::leave())
        .action("cru-ptt", |_, _, _| cru::ptt_down())
        .action("cru-ptt-up", |_, _, _| cru::ptt_up())
        .on_project(|_, alias, _| drawer::set_project(alias))
        .on_signed_in(|s, cx| signed_in(s, cx))
        .on_signed_out(|_, _| { drawer::close(); inbox::signed_out(); })
        .on_batch(|_, batch, _| {
            for e in batch { match e {
                Event::Connected(true) => { drawer::render(); inbox::load(); cru::load_unread(); tray::set_online(true); }
                Event::Connected(false) => tray::set_online(false),
                Event::Ws(t, v) if t == "voice-report" => cru::report(v),
                Event::Server(t, v) | Event::Ws(t, v) if t == "voice-kid" => cru_kid::state(v),
                _ => {}
            } }
            // The batch's inbox events, as one (one reload, one copy).
            let ibx: Vec<&Value> = batch.iter().filter_map(|e| if let Event::Inbox(v) = e { Some(v) } else { None }).collect();
            inbox::events(&ibx);
        })
        .test_ui(|_, name, args, window, cx| {
            if name == "drawer-focus" {
                let d = views(cx)?.drawer;
                let ok = d.update(cx, |d, cx| d.focus_address(window, cx));
                return Some(json!({ "ok": ok }));
            }
            let r = test_ui(name, args);
            // The driver's action runs now, as a click would: its effects too.
            if r.is_some() { pump(window, cx); }
            r
        })
        .state(|_, _, cx| { let mut p = props(); p["bubble"] = bubble::props(cx); p["chatFrames"] = hosts::props(); json!({ "areas": { "desktop": p } }) }));
    // Browser-only mode (test/drive.mjs): a socket of its own to a server.
    if let Ok(url) = std::env::var("CRUNDI_SERVER") { if !url.is_empty() { host::start(url); } }
}

/// The window opened (the app calls this with the Shell made): the views,
/// the drawer in the shell's place, and the pump task.
pub fn init_window(shell: &Entity<crundi_shell::Shell>, window: &mut Window, cx: &mut App) {
    let drawer = cx.new(|cx| BrowserDrawer::new(window, cx));
    drawer.update(cx, |d, _| d.set_shell(shell.downgrade()));
    let browsers = cx.new(|cx| browsers::BrowsersPage::new(window, cx));
    let inbox = cx.new(inbox_ui::InboxPage::new);
    let cru = cx.global::<CruView>().0.clone();
    cru.update(cx, |c, cx| c.attach(window, cx));
    cx.set_global(ViewsHandle(Views { drawer: drawer.clone(), browsers, inbox, cru }));
    let left = drawer::with(|d| d.side == "left");
    shell.update(cx, |s, cx| s.set_drawer_view(Some(Drawer { view: drawer.into(), left }), cx));
    // The drawer's width is the person's and is kept across relaunches
    // (VD st.width in localStorage): the shell's split moves Nav's, which
    // is kept here half a second after the last move.
    let nav = crundi_shell::stores(cx).nav;
    let saved = px(drawer::with(|d| d.width));
    nav.update(cx, |n, cx| if n.drawer_w != saved { n.drawer_w = saved; cx.notify(); });
    cx.observe(&nav, |nav, cx| {
        // Leaving Settings stops "Hey Cru" training.
        let tab = nav.read(cx).tab.clone();
        if LAST_TAB.with(|t| t.replace(tab.clone())) == "settings" && tab != "settings" { wake::left_settings(); }
        let w = f32::from(nav.read(cx).drawer_w);
        if (drawer::with(|d| d.width) - w).abs() < 0.5 { return; }
        drawer::with(|d| d.width = w);
        WIDTH_SEQ.with(|s| s.set(s.get() + 1));
        let seq = WIDTH_SEQ.with(|s| s.get());
        web::later(Duration::from_millis(500), move || if WIDTH_SEQ.with(|s| s.get()) == seq { drawer::persist(); });
    }).detach();
    let handle = window.window_handle();
    let Some(mut rx) = PUMP_RX.with(|r| r.borrow_mut().take()) else { return };
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        // Asleep until woken or until something falls due; never a tick.
        let due = next_due();
        match due {
            Some(at) => {
                let wait = at.saturating_duration_since(Instant::now());
                let t = cx.background_executor().timer(wait);
                futures::pin_mut!(t);
                if let futures::future::Either::Left((None, _)) = futures::future::select(rx.next(), t).await { break; }
            }
            None => if rx.next().await.is_none() { break },
        }
        while rx.try_recv().is_ok() {}
        if cx.update_window(handle, |_, window, cx| pump(window, cx)).is_err() { break; }
        // At most one pump a frame, however fast the host sends.
        cx.background_executor().timer(FRAME).await;
    }).detach();
    web::wake();
}

thread_local! {
    /// Counts the drawer's width changes: the last one in half a second is kept.
    static WIDTH_SEQ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// The tab shown last (to see Settings left).
    static LAST_TAB: RefCell<String> = const { RefCell::new(String::new()) };
}

/// One frame at 144 Hz, rounded up (the live hub's).
const FRAME: Duration = crundi_shell::live_hub::FRAME;

/// When the pump must run with nothing waking it: the webview's timers and
/// deadlines, and the area's own (Cru's, the voice's).
fn next_due() -> Option<Instant> {
    [web::next_due(), cru::next_due(), wake::next_due(), notify::next_due(), drawer::next_due()].into_iter().flatten().min()
}

/// The pump (woken, at most once a frame): CEF's deferred calls and the
/// host's events, the native host's requests, the effects the logic asked
/// for, and the views' copies.
pub fn pump(window: &mut Window, cx: &mut App) {
    PUMPS.with(|p| p.set(p.get() + 1));
    refresh(cx);
    web::pump();
    web::frames::drop_old(window);
    hosts::pump(window, cx);
    host::pump();
    passwords::pump();
    cru_screen::pump();
    wake::tick();
    cru::tick();
    // The top bar's Cru mark: live, unread (it redraws only when they changed).
    let (live, unread) = cru::mark();
    crundi_shell::set_cru_mark(cx, live, unread);
    for p in tray::take_picks() { tray::act(&p); }
    // The tray's menu follows the kid lock and "Hey Cru" (told only of a change).
    tray::refresh();
    if let Some((what, arg)) = BUBBLE_ASK.with(|b| b.borrow_mut().take()) {
        match what {
            "open" => { let id = if arg.is_empty() { focused_chat(cx) } else { arg }; bubble::open(&id, cx); }
            "toggle" => bubble::toggle(cx),
            "close" => bubble::close(cx),
            _ => bubble::send(&arg, cx),
        }
    }
    for f in ctx::take_effects() { f(window, cx); }
    // The sidebar's Inbox button: its count (notifies only on a change).
    let (supported, unread) = inbox::with(|i| (i.supported, i.unread));
    crundi_shell::set_inbox_badge(cx, supported, unread);
    notify::pump();
    for f in ctx::take_main() { f(); }
    // The drawer open or closed: the shell's place for it.
    let open = drawer::with(|d| d.open);
    let s = crundi_shell::stores(cx);
    if s.nav.read(cx).drawer_open != open { s.nav.update(cx, |n, cx| n.set_drawer(open, cx)); }
    if let Some(v) = views(cx) {
        let dirty = ctx::take_dirty();
        v.drawer.update(cx, |d, cx| d.pump(dirty, window, cx));
        v.cru.update(cx, |c, cx| c.pump(dirty, window, cx));
        if dirty {
            v.browsers.update(cx, |b, cx| b.pump(window, cx));
            v.inbox.update(cx, |i, cx| i.pump(cx));
        }
    }
}

/// The focused chat in the workbench ("" for none).
fn focused_chat(cx: &mut App) -> String {
    crundi_workbench::workbench(cx).and_then(|wb| wb.update(cx, |wb, _| wb.focused_key())).and_then(|k| k.strip_prefix("live:").map(str::to_string)).unwrap_or_default()
}

/// The session as the logic reads it (ctx), each pump: cheap reads only.
fn refresh(cx: &mut App) {
    let s = crundi_shell::stores(cx);
    let sess = s.session.read(cx);
    let api = sess.api.clone();
    let p = s.projects.read(cx);
    let mut c = ctx::Ctx {
        signed_in: api.is_some(),
        // The API's own base: normalised, with its scheme (https://…).
        base: api.as_ref().map(|a| a.base().to_string()).unwrap_or_else(|| sess.server.clone()),
        project: p.open.clone(),
        projects: p.list.iter().map(|x| json!({ "alias": x.alias, "name": x.name })).collect(),
        terminals: p.terminals.clone(),
        tab: s.nav.read(cx).tab.clone(),
        api,
        ..Default::default()
    };
    let (panes, focused) = crundi_workbench::workbench(cx).map(|wb| wb.update(cx, |wb, _| {
        let keys = wb.keys();
        let f = wb.focused_key().and_then(|k| k.strip_prefix("live:").map(str::to_string)).unwrap_or_default();
        (keys, f)
    })).unwrap_or_default();
    c.panes = panes;
    c.focused = focused;
    if c.project != ctx::project() { let p = c.project.clone(); ctx::set(c); drawer::set_project(&p); } else { ctx::set(c); }
}

fn signed_in(s: &Stores, cx: &mut App) {
    let base = s.session.read(cx).server.clone();
    let local = base.contains("://localhost") || base.contains("://127.0.0.1") || base.contains("://[::1]");
    web::net::set_local(local);
    // A frame's https or loopback page carries the server's origin as its referrer.
    web::frames::set_origin(&s.session.read(cx).api.as_ref().map(|a| a.base().to_string()).unwrap_or(base.clone()));
    drawer::with(|d| { d.mode = if local { "local".into() } else { "remote".into() }; d.status = if local { "direct".into() } else { "offline".into() }; });
    drawer::render();
    cru_kid::load();
}

thread_local! {
    /// Pumps run so far (the driver's state: an idle window pumps nothing).
    static PUMPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PUMP_RX: RefCell<Option<futures::channel::mpsc::UnboundedReceiver<()>>> = const { RefCell::new(None) };
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
        // Opening goes through the shell's Browser button, as a click does.
        "browser" => { if drawer::with(|d| d.open) { drawer::close() } else if ctx::project().is_empty() { ctx::toast_error("Select a project first") } else { drawer::open() }; ok() }
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
        // The browser host: its state; stopped now (the idle stop's path).
        "browser-host" => Some(json!({ "ok": true, "host": web::host_props() })),
        "browser-host-stop" => { web::stop_host(); ok() }
        // A drawer tab's Reload (its page crashed).
        "reload" => match drawer::find(&a(0)).and_then(|(_, t)| t.panel) { Some(i) => { web::engine::reload(i); ok() } None => Some(json!({ "ok": false, "error": "no page" })) },
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
        "cru-send" => { cru::utter(&a(0), true); ok() }
        "cru-utter" => { cru::utter(&a(0), false); ok() }
        "cru-cancel" => { cru::cancel_words(); ok() }
        "cru-drop" => { cru::drop_live(&a(0)); ok() }
        "cru-menu" => { cru::menu_pick(&a(0)); ok() }
        // The ⋯ menu shown or hidden; words as the microphone hears them; the mic tapped; the history again.
        "cru-menu-open" => { cru::with(|c| c.menu = a(0) != "false"); cru::render(); ok() }
        "cru-hear" => { cru::test_hear(&a(0)); ok() }
        // A state as the microphone and the server would bring it (the blocked notice cleared).
        "cru-state" => { cru::test_state(&a(0)); ok() }
        "cru-mic" => { cru::mic(); ok() }
        "cru-attach" => { cru::attach(); ok() }
        "cru-history" => { cru::history(None); ok() }
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
        // Cru's live screen: an event as the voice socket brings it ({type: "screen", screen}).
        "cru-event" => { if let Ok(v) = serde_json::from_str::<Value>(&a(0)) { cru_screen::event(&v); } ok() }
        "cru-screen-close" => { cru_screen::close_screen(); ok() }
        "cru-screen-full" => { cru_screen::full_view(); ok() }
        "cru-screen-normal" => { cru_screen::normal_view(); ok() }
        // A message as Cru's page posts it.
        "cru-frame-message" => { if let Ok(v) = serde_json::from_str::<Value>(&a(0)) { cru_screen::message(&v); } ok() }
        "kid-start" => { cru_kid::start(&Some(a(0)).filter(|x| !x.is_empty()).unwrap_or_else(|| "tray".into())); ok() }
        "kid-device" => { cru_kid::test_mark(a(0) != "false"); ok() }
        "kid-state" => { if let Ok(v) = serde_json::from_str::<Value>(&a(0)) { cru_kid::state(&v); } ok() }
        "kid-pin" => { cru_kid::pin_open(); ok() }
        "kid-key" => { cru_kid::pin_key(&a(0)); ok() }
        "kid-mute" => { cru_kid::mute(); ok() }
        // "Hey Cru": the status, the setting, training from recordings, a recording heard, the conditions.
        "wake-status" => Some(json!({ "ok": true, "status": wake::status() })),
        "wake-set" => { wake::set(a(0) != "false"); ok() }
        "wake-train-start" => { wake::train_start(); ok() }
        "wake-train-stop" => { wake::train_stop(); ok() }
        "wake-take" => match std::fs::read(a(0)).ok().and_then(|b| wake::engine::read_wav(&b)) { Some(s) => { wake::take(s); ok() } None => Some(json!({ "ok": false, "error": "no such recording" })) },
        "wake-hear" => match std::fs::read(a(0)).ok().and_then(|b| wake::engine::read_wav(&b)) { Some(s) => Some(json!({ "ok": wake::hear(s) })), None => Some(json!({ "ok": false, "error": "no such recording" })) },
        "wake-cond" => { let b = |x: String| match x.as_str() { "true" => Some(true), "false" => Some(false), _ => None }; wake::test_conditions(b(a(0)), b(a(1))); ok() }
        "wake-forget" => { wake::forget(); ok() }
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

/// Each part's next deadline in ms from now (the driver's state).
fn due_ms() -> Value {
    let ms = |d: Option<Instant>| d.map(|t| t.saturating_duration_since(Instant::now()).as_millis() as u64);
    json!({ "web": ms(web::next_due()), "cru": ms(cru::next_due()), "wake": ms(wake::next_due()), "notify": ms(notify::next_due()), "drawer": ms(drawer::next_due()) })
}

/// What the test driver sees of the area (the Slint client's names).
fn props() -> Value {
    let active = drawer::active_tab().and_then(|t| t.panel).map(|i| i as i64).unwrap_or(-1);
    let hidden: Vec<Value> = web::engine::with(|st| st.panels.iter().enumerate().filter(|(_, p)| !p.closed && p.key.is_some()).map(|(i, p)| json!({ "key": p.key, "panel": i, "url": p.url, "frames": p.frames, "paused": p.hidden_sent == Some(true) })).collect());
    json!({
        "pumps": PUMPS.with(|p| p.get()),
        // Each part's next deadline, in ms from now (null: none): what may still wake the pump.
        "due": due_ms(),
        "drawer": drawer::props(), "drawerActive": active, "evals": EVALS.with(|e| Value::Object(e.borrow().1.clone())),
        "browsers": browsers::props(),
        "inbox": inbox::props(), "cru": cru::props(), "cruScreen": cru_screen::props(), "kid": cru_kid::props(), "wake": wake::status(), "homepool": homepool::status(), "tray": tray::props(), "passwords": passwords::props(), "notifications": notify::props(), "hidden": hidden,
        "host": { "features": host::features() },
        "browserHost": web::host_props(),
    })
}
