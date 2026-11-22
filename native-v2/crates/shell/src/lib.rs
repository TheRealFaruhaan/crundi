//! The shell (ARCHITECTURE.md, AREAS.md): the stores every area shares
//! (`Stores`: the session, the projects, the top bar's data, where the
//! window is), the live hub, the areas' registry, and the window's views:
//! the Shell (layout only) with the TopBar, the Sidebar, the ProjectBar, each
//! tab's body and the drawer's place side by side as cached views, and the
//! SignIn view while signed out.

pub mod conn;
pub mod lifecycle;
pub mod live_hub;
pub mod nav;
pub mod projects;
// Cru's push-to-talk shortcut (ptt.json), ported for the Desktop area (additive).
pub mod ptt;
pub mod registry;
pub mod session;
mod signin;
pub mod slots;
// Where things are on screen, for Mini (the chat area's), and the logo's place while it is out.
pub mod spots;
pub mod usage;
mod usage_rows;
mod views;

pub use nav::Nav;
pub use projects::{Project, ProjectPicked, Projects};
pub use registry::{register, run_action, Area, TABS};
pub use session::{Session, SessionEvent, Step};
pub use slots::{register_login_restore, register_tab_badge, register_top_item};
pub use usage::Usage;
pub use views::{shell, Drawer, Shell};

/// A toast from code that has no window at hand (a server call's answer).
pub fn toast(cx: &mut App, text: impl Into<SharedString>, kind: crundi_widgets::Toast) {
    let text: SharedString = text.into();
    cx.defer(move |cx| {
        let Some(h) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
        let _ = h.update(cx, move |_, window, cx| crundi_widgets::toast(window, cx, text, kind));
    });
}

use crundi_core::api::{Api, ApiError, ApiResult};
use gpui::*;
use serde_json::Value;

/// The shared stores' handles, made once by the app and handed to every
/// area's `init` (ARCHITECTURE.md: handles, not lookups). Cheap to clone.
#[derive(Clone)]
pub struct Stores {
    pub session: Entity<Session>,
    pub projects: Entity<Projects>,
    pub usage: Entity<Usage>,
    pub nav: Entity<Nav>,
}

/// The handles for code that has no view to hold them (the driver, hooks
/// run from a server call's answer). Read in handlers only, never in render.
struct StoresHandle(Stores);
impl Global for StoresHandle {}

/// A path clicked in a chat, for the "open-path" action (the Files area
/// opens it in the file viewer, as the web's openPathFromChat).
pub struct OpenPath(pub String);
impl Global for OpenPath {}

/// Layers an area draws over the whole window (the Files area's file
/// windows), registered at init, before the window opens. Each is a cached
/// view placed over the signed-in tree; it draws nothing (and takes no
/// pointer) while it has nothing to show.
#[derive(Default)]
pub struct Overlays(pub Vec<AnyView>);
impl Global for Overlays {}
pub fn register_overlay(cx: &mut App, v: AnyView) { cx.default_global::<Overlays>().0.push(v); }
/// Layers drawn over the dialogs too (`modal`, `ask`): the web's kid screen,
/// PC lock and Cru panel sit above any open dialog (z-index 9000 and up
/// against the dialogs' 210), so a dialog left open never covers them (F212).
#[derive(Default)]
pub struct TopOverlays(pub Vec<AnyView>);
impl Global for TopOverlays {}
pub fn register_top_overlay(cx: &mut App, v: AnyView) { cx.default_global::<TopOverlays>().0.push(v); }

/// What Cru's mark in the top bar shows (Desktop sets it, `set_cru_mark`):
/// Cru live (listening, thinking, talking) and its unread reports. An
/// entity the TopBar observes: it redraws only when this changed.
#[derive(Clone, Default, PartialEq)]
pub struct CruMark { pub live: bool, pub unread: i64 }
struct CruMarkHandle(Entity<CruMark>);
impl Global for CruMarkHandle {}
pub fn cru_mark(cx: &mut App) -> Entity<CruMark> {
    if let Some(h) = cx.try_global::<CruMarkHandle>() { return h.0.clone(); }
    let e = cx.new(|_| CruMark::default());
    cx.set_global(CruMarkHandle(e.clone()));
    e
}
pub fn set_cru_mark(cx: &mut App, live: bool, unread: i64) {
    let m = CruMark { live, unread };
    let e = cru_mark(cx);
    if *e.read(cx) != m { e.update(cx, |x, cx| { *x = m; cx.notify(); }); }
}

/// The sidebar's Inbox button (the web's ibxBadge; Desktop sets it,
/// `set_inbox_badge`): hidden where the server has no Inbox, its unread
/// count. An entity the Sidebar observes: it redraws only when this changed.
#[derive(Clone, PartialEq)]
pub struct InboxBadge { pub supported: bool, pub unread: i64 }
impl Default for InboxBadge { fn default() -> Self { InboxBadge { supported: true, unread: 0 } } }
struct InboxBadgeHandle(Entity<InboxBadge>);
impl Global for InboxBadgeHandle {}
pub fn inbox_badge(cx: &mut App) -> Entity<InboxBadge> {
    if let Some(h) = cx.try_global::<InboxBadgeHandle>() { return h.0.clone(); }
    let e = cx.new(|_| InboxBadge::default());
    cx.set_global(InboxBadgeHandle(e.clone()));
    e
}
pub fn set_inbox_badge(cx: &mut App, supported: bool, unread: i64) {
    let b = InboxBadge { supported, unread };
    let e = inbox_badge(cx);
    if *e.read(cx) != b { e.update(cx, |x, cx| { *x = b; cx.notify(); }); }
}

/// Where Cru's mark was last painted (Cru's screen opens under it, at its
/// right, as the web's anchor). Set in the top bar's paint; read in handlers.
#[derive(Clone, Copy)]
struct CruAnchor(Bounds<Pixels>);
impl Global for CruAnchor {}
pub fn cru_anchor(cx: &App) -> Option<Bounds<Pixels>> { cx.try_global::<CruAnchor>().map(|a| a.0) }
pub(crate) fn set_cru_anchor(cx: &mut App, b: Bounds<Pixels>) { if cru_anchor(cx) != Some(b) { cx.set_global(CruAnchor(b)); } }

/// The server step over the signed-in app (the tray's "Server…", Electron's
/// showServerScreen; F131), its box holding the saved address; `false` goes
/// back to the app (its Cancel). Signed out, it is the sign-in's server step.
pub fn show_server_screen(on: bool, window: &mut Window, cx: &mut App) {
    if on { if let Some(sh) = views::shell(cx) { let si = sh.read(cx).sign_in.clone(); si.update(cx, |v, cx| v.fill_server(window, cx)); } }
    let s = stores(cx);
    s.session.update(cx, |x, cx| x.show_server_over(on, cx));
}

/// The stores (after `init`).
pub fn stores(cx: &App) -> Stores { cx.global::<StoresHandle>().0.clone() }

/// The window's ×, and the system's close (Alt+F4, the taskbar's Close): an
/// area's "window-close" action decides (the Desktop area's tray: hide to it,
/// as Electron's mainWindow 'close'; kept while the kid screen is locked);
/// with nobody to decide, the app quits (F113).
pub fn close_window(window: &mut Window, cx: &mut App) {
    let s = stores(cx);
    if !registry::run_action(&s, "window-close", window, cx) { cx.quit() }
}

/// The API of the signed-in server (None when signed out).
pub fn api(cx: &App) -> Option<Api> { cx.try_global::<StoresHandle>().and_then(|h| h.0.session.read(cx).api.clone()) }

/// The open project's alias ("" for none).
pub fn project(cx: &App) -> String { cx.try_global::<StoresHandle>().map(|h| h.0.projects.read(cx).open.clone()).unwrap_or_default() }

/// Watch a chat on /ws (its history now, then its events), and stop.
pub fn subscribe(cx: &App, id: &str) { if let Some(l) = cx.try_global::<StoresHandle>().and_then(|h| h.0.session.read(cx).live()) { l.subscribe(id); } }
pub fn unsubscribe(cx: &App, id: &str) { if let Some(l) = cx.try_global::<StoresHandle>().and_then(|h| h.0.session.read(cx).live()) { l.unsubscribe(id); } }
/// Send a message on /ws (subscribe-style; not resent after a reconnect).
pub fn ws_send(cx: &App, msg: &Value) { if let Some(l) = cx.try_global::<StoresHandle>().and_then(|h| h.0.session.read(cx).live()) { l.send(msg); } }

/// A server call off the UI thread; `done` runs on it with the answer. An
/// ended sign-in signs out (and `done` is not called); a 404 reads "Not
/// supported by this server".
pub fn call(cx: &mut App, work: impl FnOnce(&Api) -> ApiResult + Send + 'static, done: impl FnOnce(Result<Value, String>, &mut App) + 'static) {
    let Some(api) = api(cx) else { return };
    run(cx, move || work(&api), move |r, cx| match r {
        Err(ApiError::SignedOut) => { let s = stores(cx); s.session.update(cx, |s, cx| s.sign_out("Your sign-in ended. Sign in again.", cx)); }
        Err(e) => done(Err(e.to_string()), cx),
        Ok(v) => done(Ok(v), cx),
    });
}

/// Blocking work off the UI thread, then `done` on it.
pub fn run<T: Send + 'static>(cx: &mut App, work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T, &mut App) + 'static) {
    let task = cx.background_spawn(async move { work() });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let r = task.await;
        let _ = cx.update(|cx| done(r, cx));
    }).detach();
}

/// Make the stores and wire them (called once by the app, before the areas'
/// init): sign-in and sign-out start and stop the live hub, load the
/// projects and the top bar's data, and tell the areas; a project picked
/// tells the areas once the update is over.
pub fn init(cx: &mut App) -> Stores {
    let stores = Stores {
        session: cx.new(|_| Session::default()),
        projects: cx.new(|_| Projects::default()),
        usage: cx.new(|_| Usage::default()),
        nav: cx.new(|_| Nav::default()),
    };
    cx.set_global(StoresHandle(stores.clone()));
    let s = stores.clone();
    cx.subscribe(&stores.session, move |_, e: &SessionEvent, cx| match e {
        SessionEvent::SignedIn => {
            // The live link's channel: one hub per session.
            if let Some(rx) = s.session.update(cx, |x, _| x.take_events()) {
                let hub = live_hub::run(s.clone(), rx, cx);
                cx.set_global(Hub(Some(hub)));
            }
            for a in registry::list(cx) { if let Some(f) = &a.on_signed_in { f(&s, cx); } }
            load_projects(&s, cx);
            // The project mode: in "single" a project cannot be removed (it is a folder).
            call(cx, |api| api.get("/api/projects/config"), {
                let s = s.clone();
                move |r, cx| { let m = r.ok().and_then(|v| v.get("mode").and_then(|m| m.as_str()).map(String::from)).unwrap_or_else(|| "multi".into()); s.projects.update(cx, |p, cx| p.set_mode(&m, cx)); }
            });
            call(cx, |api| api.get("/api/usage/latest").or_else(|_| api.get("/api/usage")), {
                let s = s.clone();
                move |r, cx| if let Ok(u) = r { s.usage.update(cx, |x, cx| x.set_usage(u, cx)); }
            });
            load_status(s.clone(), cx);
            // The Docker tab only where Docker is installed (the web's
            // checkDockerPresence: one call per sign-in; shown until it answers).
            call(cx, |api| api.get("/api/docker/status"), {
                let s = s.clone();
                move |r, cx| if let Ok(v) = r { let on = v.get("installed").and_then(|i| i.as_bool()).unwrap_or(false); s.nav.update(cx, |n, cx| n.set_docker(on, cx)); }
            });
        }
        SessionEvent::SignedOut => {
            cx.set_global(Hub(None));
            let c = conn::conn(cx);
            c.update(cx, |c, cx| c.reset(cx));
            s.projects.update(cx, |p, cx| p.clear(cx));
            s.usage.update(cx, |u, cx| u.clear(cx));
            for a in registry::list(cx) { if let Some(f) = &a.on_signed_out { f(&s, cx); } }
        }
        SessionEvent::ServerScreen => {}
    }).detach();
    // The connection badge and line follow the live link.
    let c = conn::conn(cx);
    cx.observe(&stores.session, move |s, cx| { let on = s.read(cx).online; c.update(cx, |c, cx| c.link(on, cx)); }).detach();
    // The driver's look at the shell's own parts, and its sign-in actions.
    register(cx, Area::new("shell")
        .state(|stores, _, cx| views::driver_state(stores, cx))
        .test_ui(|stores, name, args, window, cx| {
            let a = |i: usize| args.get(i).cloned().unwrap_or_default();
            let session = stores.session.clone();
            match name {
                "signin-collab" => session.update(cx, |s, cx| s.show_collab(a(0) != "false", cx)),
                "sign-in-collab" => session.update(cx, |s, cx| s.sign_in_collab(&a(0), &a(1), cx)),
                "set-up-password" => session.update(cx, |s, cx| s.set_up_password(&a(0), cx)),
                "finish-enrol" => session.update(cx, |s, cx| s.finish_enrol(cx)),
                "login-restore" => match slots::restore(cx) { Some((_, open, _)) => { let b = session.read(cx).signin_base(); open(cx, &b) } None => return Some(serde_json::json!({ "ok": false, "error": "no restore registered" })) },
                // The link as a dropped connection would leave it (the badge's timers run as for real).
                "server-screen" => show_server_screen(a(0) != "false", window, cx),
                "server-screen-save" => session.update(cx, |s, cx| s.save_server_over(&a(0), cx)),
                "conn-link" => { let c = conn::conn(cx); c.update(cx, |c, cx| c.link(a(0) == "true", cx)) }
                "remove-project" => {
                    let n = stores.projects.read(cx).list.iter().find(|p| p.alias == a(0)).map(|p| p.name.clone()).unwrap_or_else(|| a(0));
                    views::remove_project(stores, a(0), n, window, cx)
                }
                _ => return None,
            }
            Some(serde_json::json!({ "ok": true }))
        }));
    // The tab bar's scope follows whether a project is open.
    let s = stores.clone();
    let mut was: Option<bool> = None;
    cx.observe(&stores.projects, move |p, cx| {
        let open = !p.read(cx).open.is_empty();
        if was != Some(open) { let closed = was == Some(true); was = Some(open); s.nav.update(cx, |n, cx| n.sync_scope(open, closed, cx)); }
    }).detach();
    let s = stores.clone();
    cx.subscribe(&stores.projects, move |_, e: &ProjectPicked, cx| {
        let (alias, s) = (e.0.clone(), s.clone());
        cx.defer(move |cx| for a in registry::list(cx) { if let Some(f) = &a.on_project { f(&s, &alias, cx); } });
    }).detach();
    stores
}

/// The live hub's task (dropped on sign-out: the hub stops).
struct Hub(#[allow(dead_code)] Option<Task<()>>);
impl Global for Hub {}

/// The projects, then the one to show (the saved one, else the first).
pub fn load_projects(stores: &Stores, cx: &mut App) {
    let s = stores.clone();
    call(cx, |api| api.get("/api/projects"), move |r, cx| {
        let list = r.ok().and_then(|v| v.as_array().cloned().or_else(|| v.get("projects").and_then(|p| p.as_array()).cloned())).unwrap_or_default();
        let want = s.session.read(cx).want_project.clone();
        s.projects.update(cx, |p, cx| {
            p.set_projects(list, cx);
            let pick = p.list.iter().find(|x| x.alias == want).or_else(|| p.list.first()).map(|x| x.alias.clone());
            if let Some(a) = pick { p.pick(&a, cx); }
        });
        // No project to open: the tab bar's global scope, on Info.
        if s.projects.read(cx).open.is_empty() { s.nav.update(cx, |n, cx| n.sync_scope(false, true, cx)); }
    });
}

/// The Telegram bot's status for the top bar (GET /api/status), then again
/// every 15 s while signed in (the web's updateTgBadge).
fn load_status(s: Stores, cx: &mut App) {
    call(cx, |api| api.get("/api/status"), move |r, cx| {
        if let Ok(d) = r {
            let configured = d.get("botConfigured").and_then(|v| v.as_bool()).unwrap_or(true);
            let connected = d.get("botConnected").and_then(|v| v.as_bool()).unwrap_or(false);
            let name = d.get("botUsername").and_then(|v| v.as_str()).unwrap_or("").to_string();
            s.usage.update(cx, |u, cx| u.set_tg((configured, connected, name), cx));
        }
        let t = cx.background_executor().timer(std::time::Duration::from_secs(15));
        cx.spawn(async move |cx: &mut AsyncApp| { t.await; let _ = cx.update(|cx| if s.session.read(cx).signed_in() { load_status(s, cx) }); }).detach();
    });
}
