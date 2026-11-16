//! The signed-in session (one Global): the server, the API, the live link,
//! the projects and the open project. Sign-in, resume and sign-out follow the
//! Slint client's app.rs (check_server, signed_in, start_session, sign_out);
//! server calls run off the UI thread (`call`). Parts read it through
//! `shell::api(cx)`, `shell::project(cx)` and friends.

use crate::registry;
use crundi_core::{api::{self, Api, ApiError, SignedIn}, live::{self, Event, Live}, store};
use gpui::{App, AsyncApp, Global};
use serde_json::Value;
use std::sync::mpsc;

/// Where sign-in is: which server, then signing in to it.
#[derive(Clone, Default, PartialEq, Debug)]
pub enum Step { #[default] Server, Login }

#[derive(Default)]
pub struct Session {
    pub api: Option<Api>,
    live: Option<Live>,
    events: Option<mpsc::Receiver<Event>>,
    pub online: bool,
    pub server: String,
    pub username: String,
    pub projects: Vec<Value>,
    pub project: String,
    /// The event stream's last terminals (chats and Claude terminals).
    pub terminals: Vec<Value>,
    // Sign-in.
    pub step: Step,
    pub signin_server: String,
    pub signin_error: String,
    pub signin_busy: bool,
    pub signin_checked: bool,
    pub offer_password: bool,
    pub offer_google: bool,
    pub offer_github: bool,
    pub offer_telegram: bool,
    pub app_login: bool,
}

impl Global for Session {}

pub fn get(cx: &App) -> &Session { cx.global::<Session>() }
pub fn with<R>(cx: &mut App, f: impl FnOnce(&mut Session) -> R) -> R { f(cx.global_mut::<Session>()) }

/// The API of the signed-in server (None when signed out).
pub fn api(cx: &App) -> Option<Api> { cx.try_global::<Session>().and_then(|s| s.api.clone()) }
/// The open project's alias ("" for none).
pub fn project(cx: &App) -> String { cx.try_global::<Session>().map(|s| s.project.clone()).unwrap_or_default() }
pub fn signed_in(cx: &App) -> bool { cx.try_global::<Session>().is_some_and(|s| s.api.is_some()) }

/// Send a message on /ws (subscribe-style); not resent after a reconnect.
pub fn ws_send(cx: &App, msg: &Value) {
    if let Some(l) = cx.try_global::<Session>().and_then(|s| s.live.as_ref()) { l.send(msg); }
}
/// Watch a chat (its history now, then its events), and stop.
pub fn subscribe(cx: &App, id: &str) { if let Some(l) = cx.try_global::<Session>().and_then(|s| s.live.as_ref()) { l.subscribe(id); } }
pub fn unsubscribe(cx: &App, id: &str) { if let Some(l) = cx.try_global::<Session>().and_then(|s| s.live.as_ref()) { l.unsubscribe(id); } }

/// A server call off the UI thread; `done` runs on it with the answer. An
/// ended sign-in signs out (and `done` is not called); a 404 reads "Not
/// supported by this server".
pub fn call(cx: &mut App, work: impl FnOnce(&Api) -> api::ApiResult + Send + 'static, done: impl FnOnce(Result<Value, String>, &mut App) + 'static) {
    let Some(api) = api(cx) else { return };
    run(cx, move || work(&api), move |r, cx| match r {
        Err(ApiError::SignedOut) => sign_out(cx, "Your sign-in ended. Sign in again."),
        Err(e) => done(Err(e.to_string()), cx),
        Ok(v) => done(Ok(v), cx),
    });
}

/// Blocking work off the UI thread, then `done` on it.
pub fn run<T: Send + 'static>(cx: &mut App, work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T, &mut App) + 'static) {
    let task = cx.background_executor().spawn(async move { work() });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let r = task.await;
        let _ = cx.update(|cx| done(r, cx));
    }).detach();
}

pub fn init(cx: &mut App) {
    cx.set_global(Session::default());
    let saved = store::load();
    with(cx, |s| s.signin_server = host_of(&saved.server));
    if saved.server.is_empty() {
        with(cx, |s| s.step = Step::Server);
    } else if !saved.refresh_token.is_empty() {
        with(cx, |s| s.step = Step::Login);
        resume(cx, saved);
    } else {
        check_server(cx, &saved.server);
    }
}

fn host_of(base: &str) -> String { base.split("://").nth(1).unwrap_or(base).to_string() }

fn busy(cx: &mut App, on: bool, error: &str) {
    with(cx, |s| { s.signin_busy = on; s.signin_error = error.to_string(); });
    cx.refresh_windows();
}

/// The saved login, picked up again (a refresh with the saved token).
fn resume(cx: &mut App, saved: store::Saved) {
    busy(cx, true, "");
    let (base, token, username) = (saved.server.clone(), saved.refresh_token.clone(), saved.username.clone());
    run(cx, move || api::refresh(&base, &token).map(|tokens| SignedIn { base, tokens, username }), |r, cx| match r {
        Ok(si) => { busy(cx, false, ""); start_session(cx, si); }
        // Refused: sign in again on that server.
        Err(_) => { store::update(|s| s.refresh_token.clear()); busy(cx, false, ""); let srv = store::load().server; check_server(cx, &srv); }
    });
}

/// The server step: what this server offers for signing in.
pub fn check_server(cx: &mut App, input: &str) {
    let base = api::normalize_base(input);
    let saved = store::load();
    if saved.server == base && !saved.refresh_token.is_empty() { return resume(cx, saved); }
    busy(cx, true, "");
    run(cx, move || {
        let m = api::methods(&base)?;
        let caps = api::handshake(&base).ok().and_then(|h| h.pointer("/server/capabilities").cloned()).unwrap_or(Value::Null);
        let app_login = caps.as_array().map(|c| c.iter().any(|x| x == "app-login")).unwrap_or(false);
        Ok::<_, ApiError>((base, m, app_login))
    }, |r, cx| {
        busy(cx, false, "");
        match r {
            Ok((base, m, app_login)) => {
                let on = |k: &str| m.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
                with(cx, |s| {
                    s.app_login = app_login;
                    // Telegram signs in through its web widget: only through the browser.
                    s.offer_telegram = app_login && on("telegram");
                    s.signin_server = host_of(&base);
                    s.offer_password = on("password");
                    s.offer_google = on("google");
                    s.offer_github = on("github");
                    s.signin_checked = true;
                    s.step = Step::Login;
                    if on("setupRequired") { s.signin_error = "This server has no sign-in set up yet: set one up in its web page first.".into(); }
                });
                store::update(|s| { if s.server != base { s.refresh_token.clear(); } s.server = base.clone(); });
            }
            Err(e) => with(cx, |s| s.signin_error = e.to_string()),
        }
        cx.refresh_windows();
    });
}

/// Sign in with the password and the authenticator's code.
pub fn sign_in_password(cx: &mut App, server: &str, password: &str, code: &str) {
    let (base, password, code) = (api::normalize_base(server), password.to_string(), code.to_string());
    busy(cx, true, "");
    run(cx, move || api::password_login(&base, &password, &code), finish_sign_in);
}

/// Sign in with a pairing link from a signed-in session.
pub fn sign_in_pair(cx: &mut App, link: &str) {
    let link = link.to_string();
    busy(cx, true, "");
    run(cx, move || api::pair_claim(&link), finish_sign_in);
}

fn finish_sign_in(r: Result<SignedIn, ApiError>, cx: &mut App) {
    match r {
        Ok(si) => { busy(cx, false, ""); start_session(cx, si); }
        Err(e) => busy(cx, false, &e.to_string()),
    }
}

fn start_session(cx: &mut App, si: SignedIn) {
    let SignedIn { base, tokens, username } = si;
    let saved = store::load();
    let username = if username.is_empty() && saved.server == base { saved.username.clone() } else { username };
    store::update(|s| {
        if s.server != base { s.project.clear(); s.layouts.clear(); }
        s.server = base.clone();
        s.refresh_token = tokens.refresh.clone();
        s.username = username.clone();
    });
    let api = Api::new(&base, tokens, |t| store::update(|s| s.refresh_token = t.refresh.clone()));
    let (tx, rx) = mpsc::channel();
    let live = live::start(api.clone(), move |e| { let _ = tx.send(e); });
    let old = with(cx, |s| {
        s.api = Some(api);
        s.events = Some(rx);
        s.server = host_of(&base);
        s.username = username.clone();
        s.signin_checked = false;
        s.project.clear();
        s.live.replace(live)
    });
    if let Some(old) = old { old.stop(); }
    registry::each(cx, |a, cx| if let Some(f) = &a.on_signed_in { f(cx) });
    load_projects(cx, saved.project);
    cx.refresh_windows();
}

/// The projects, then the one to show (the saved one, else the first).
pub fn load_projects(cx: &mut App, want: String) {
    call(cx, |api| api.get("/api/projects"), move |r, cx| {
        let list = r.ok().and_then(|v| v.as_array().cloned().or_else(|| v.get("projects").and_then(|p| p.as_array()).cloned())).unwrap_or_default();
        with(cx, |s| s.projects = list.clone());
        let alias = |p: &Value| p.get("alias").and_then(|a| a.as_str()).unwrap_or("").to_string();
        let pick = list.iter().map(alias).find(|a| *a == want).or_else(|| list.first().map(alias)).unwrap_or_default();
        if !pick.is_empty() { select_project(cx, &pick); }
        cx.refresh_windows();
    });
}

pub fn select_project(cx: &mut App, alias: &str) {
    with(cx, |s| s.project = alias.to_string());
    store::update(|s| s.project = alias.to_string());
    let a = alias.to_string();
    registry::each(cx, move |ar, cx| if let Some(f) = &ar.on_project { f(&a, cx) });
    cx.refresh_windows();
}

/// The open project's name ("" for none).
pub fn project_name(cx: &App) -> String {
    let s = get(cx);
    s.projects.iter().find(|p| p.get("alias").and_then(|a| a.as_str()) == Some(s.project.as_str()))
        .map(|p| p.get("name").and_then(|n| n.as_str()).filter(|n| !n.is_empty()).unwrap_or(&s.project).to_string())
        .unwrap_or_default()
}

/// Sign out (why: "" for the person's own choice, which also tells the server).
pub fn sign_out(cx: &mut App, why: &str) {
    let (api, live) = with(cx, |s| (s.api.take(), s.live.take()));
    if let Some(l) = live { l.stop(); }
    if let (Some(api), true) = (api, why.is_empty()) {
        std::thread::spawn(move || { let _ = api.post("/api/auth/logout", &serde_json::json!({})); });
    }
    store::update(|s| s.refresh_token.clear());
    with(cx, |s| {
        s.events = None;
        s.projects.clear();
        s.project.clear();
        s.terminals.clear();
        s.online = false;
        s.signin_error = why.to_string();
        s.step = Step::Login;
    });
    registry::each(cx, |a, cx| if let Some(f) = &a.on_signed_out { f(cx) });
    cx.refresh_windows();
}

/// Hand what the live link heard to the parts (the app calls this on every pump).
pub fn pump(cx: &mut App) {
    let mut evs = vec![];
    if let Some(rx) = cx.try_global::<Session>().and_then(|s| s.events.as_ref()) {
        while let Ok(e) = rx.try_recv() { evs.push(e); }
    }
    if evs.is_empty() { return; }
    for e in evs {
        match &e {
            Event::SignedOut => { sign_out(cx, "Your sign-in ended. Sign in again."); return; }
            Event::Connected(on) => with(cx, |s| s.online = *on),
            Event::State(st) => {
                if let Some(p) = st.get("projects").and_then(|p| p.as_array()) { if !p.is_empty() { with(cx, |s| s.projects = p.clone()); } }
                if let Some(t) = st.get("terminals").and_then(|t| t.as_array()) { with(cx, |s| s.terminals = t.clone()); }
            }
            _ => {}
        }
        if let Some(ae) = registry::AreaEvent::from(&e) {
            for a in registry::list(cx) { if let Some(f) = &a.on_event { f(&ae, cx); } }
        }
    }
    cx.refresh_windows();
}
