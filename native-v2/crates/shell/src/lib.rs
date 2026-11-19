//! The shell (ARCHITECTURE.md, AREAS.md): the stores every area shares
//! (`Stores`: the session, the projects, the top bar's data, where the
//! window is), the live hub, the areas' registry, and the window's views:
//! the Shell (layout only) with the TopBar, the Sidebar, the ProjectBar, each
//! tab's body and the drawer's place side by side as cached views, and the
//! SignIn view while signed out.

pub mod live_hub;
pub mod nav;
pub mod projects;
pub mod registry;
pub mod session;
pub mod usage;
mod usage_rows;
mod views;

pub use nav::Nav;
pub use projects::{Project, ProjectPicked, Projects};
pub use registry::{register, run_action, Area, TABS};
pub use session::{Session, SessionEvent, Step};
pub use usage::Usage;
pub use views::{shell, Drawer, Shell};

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

/// The stores (after `init`).
pub fn stores(cx: &App) -> Stores { cx.global::<StoresHandle>().0.clone() }

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
            s.projects.update(cx, |p, cx| p.clear(cx));
            s.usage.update(cx, |u, cx| u.clear(cx));
            for a in registry::list(cx) { if let Some(f) = &a.on_signed_out { f(&s, cx); } }
        }
    }).detach();
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
/// every 30 s while signed in (the web's updateTgBadge).
fn load_status(s: Stores, cx: &mut App) {
    call(cx, |api| api.get("/api/status"), move |r, cx| {
        if let Ok(d) = r {
            let configured = d.get("botConfigured").and_then(|v| v.as_bool()).unwrap_or(true);
            let connected = d.get("botConnected").and_then(|v| v.as_bool()).unwrap_or(false);
            let name = d.get("botUsername").and_then(|v| v.as_str()).unwrap_or("").to_string();
            s.usage.update(cx, |u, cx| u.set_tg((configured, connected, name), cx));
        }
        let t = cx.background_executor().timer(std::time::Duration::from_secs(30));
        cx.spawn(async move |cx: &mut AsyncApp| { t.await; let _ = cx.update(|cx| if s.session.read(cx).signed_in() { load_status(s, cx) }); }).detach();
    });
}
