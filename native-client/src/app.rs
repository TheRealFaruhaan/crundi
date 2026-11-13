//! The app: signing in, the session, projects and chats, the open chat. Runs
//! on the UI thread; anything that talks to the server goes to a worker
//! thread ([bg]) and comes back here with its answer.

use crate::{
    api::{self, Api, ApiError, SignedIn},
    chat::{Conversation, Shown},
    live::{self, Event, Live},
    store, ChatItem, EntryView, MainWindow, ProjectItem,
};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Weak};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub struct App {
    ui: Weak<MainWindow>,
    api: Option<Api>,
    live: Option<Live>,
    /// The server's lists: /api/projects and /api/ui-sessions.
    projects: Vec<Value>,
    sessions: Vec<Value>,
    project: String,
    chat: String,
    conv: Conversation,
    shown: Vec<Shown>,
    entries: Rc<VecModel<EntryView>>,
    project_rows: Rc<VecModel<ProjectItem>>,
    chat_rows: Rc<VecModel<ChatItem>>,
    oauth_cancel: Option<Arc<AtomicBool>>,
    /// The chat list is being fetched; fetch again when it lands.
    sessions_busy: bool,
    sessions_dirty: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.borrow_mut().as_mut().map(f))
}

/// Work off the UI thread, then [done] back on it.
fn bg<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + Send + 'static) {
    std::thread::spawn(move || {
        let r = work();
        let _ = slint::invoke_from_event_loop(move || done(r));
    });
}

fn ui() -> Option<MainWindow> {
    with(|a| a.ui.upgrade()).flatten()
}

fn host_of(base: &str) -> String {
    base.split("://").nth(1).unwrap_or(base).to_string()
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    }
}

// ─── Starting ───

pub fn init(window: &MainWindow) {
    let entries = Rc::new(VecModel::<EntryView>::default());
    let project_rows = Rc::new(VecModel::<ProjectItem>::default());
    let chat_rows = Rc::new(VecModel::<ChatItem>::default());
    window.set_entries(entries.clone().into());
    window.set_projects(project_rows.clone().into());
    window.set_chats(chat_rows.clone().into());
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            ui: window.as_weak(),
            api: None,
            live: None,
            projects: vec![],
            sessions: vec![],
            project: String::new(),
            chat: String::new(),
            conv: Conversation::default(),
            shown: vec![],
            entries,
            project_rows,
            chat_rows,
            oauth_cancel: None,
            sessions_busy: false,
            sessions_dirty: false,
        })
    });

    window.on_check_server(|s| check_server(s.as_str()));
    window.on_sign_in_pair(|l| sign_in_pair(l.to_string()));
    window.on_sign_in_password(|s, p, c| sign_in_password(s.to_string(), p.to_string(), c.to_string()));
    window.on_sign_in_oauth(|s, p| sign_in_oauth(s.to_string(), p.to_string()));
    window.on_cancel_sign_in(cancel_oauth);
    window.on_sign_out(|| sign_out(""));
    window.on_select_project(|a| select_project(a.as_str()));
    window.on_select_chat(|id| select_chat(id.as_str()));
    window.on_new_chat(new_chat);
    window.on_send(|t| send(t.to_string()));
    window.on_stop(stop);
    window.on_respond(|id, b, always| respond(id.to_string(), b.to_string(), always));
    window.on_answer(|id, o| answer(id.as_str(), o.as_str()));

    // A saved login: pick it up again.
    let saved = store::load();
    window.set_signin_server(host_of(&saved.server).into());
    if !saved.server.is_empty() && !saved.refresh_token.is_empty() {
        resume(saved);
    }
}

fn resume(saved: store::Saved) {
    let Some(w) = ui() else { return };
    w.set_signin_busy(true);
    w.set_signin_error("".into());
    bg(move || api::refresh(&saved.server, &saved.refresh_token).map(|t| (saved, t)), |r| {
        let Some(w) = ui() else { return };
        w.set_signin_busy(false);
        match r {
            Ok((saved, tokens)) => start_session(SignedIn { base: saved.server, tokens, username: saved.username }),
            Err(ApiError::SignedOut) => {
                store::update(|s| s.refresh_token.clear());
                w.set_signin_error("Your sign-in ended. Sign in again.".into());
            }
            Err(e) => w.set_signin_error(format!("{e}. Press Continue to try again.").into()),
        }
    });
}

// ─── Signing in ───

fn busy(on: bool, error: &str) {
    if let Some(w) = ui() {
        w.set_signin_busy(on);
        w.set_signin_error(error.into());
    }
}

fn check_server(input: &str) {
    let base = api::normalize_base(input);
    // The saved login for this server comes first.
    let saved = store::load();
    if saved.server == base && !saved.refresh_token.is_empty() {
        return resume(saved);
    }
    busy(true, "");
    bg(move || api::methods(&base).map(|m| (base, m)), |r| {
        busy(false, "");
        let Some(w) = ui() else { return };
        match r {
            Ok((base, m)) => {
                let on = |k: &str| m.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
                w.set_signin_server(host_of(&base).into());
                w.set_offer_password(on("password"));
                w.set_offer_google(on("google"));
                w.set_offer_github(on("github"));
                w.set_signin_checked(true);
                if on("setupRequired") { w.set_signin_error("This server has no sign-in set up yet: set one up in its web page first.".into()); }
            }
            Err(e) => w.set_signin_error(e.to_string().into()),
        }
    });
}

fn signed_in(r: Result<SignedIn, ApiError>) {
    busy(false, "");
    match r {
        Ok(si) => start_session(si),
        Err(e) => busy(false, &e.to_string()),
    }
}

fn sign_in_pair(link: String) {
    busy(true, "");
    bg(move || api::pair_claim(&link), signed_in);
}

fn sign_in_password(server: String, password: String, code: String) {
    busy(true, "");
    let base = api::normalize_base(&server);
    bg(move || api::password_login(&base, &password, &code), signed_in);
}

fn sign_in_oauth(server: String, provider: String) {
    busy(true, "");
    let base = api::normalize_base(&server);
    let cancel = Arc::new(AtomicBool::new(false));
    with(|a| a.oauth_cancel = Some(cancel.clone()));
    bg(move || api::oauth_start(&base, &provider).map(|v| (base, v)), move |r| {
        let (base, v) = match r {
            Ok(x) => x,
            Err(e) => return busy(false, &e.to_string()),
        };
        let (url, poll_id, code) = (s(&v, "url"), s(&v, "pollId"), s(&v, "confirmCode"));
        if url.is_empty() || poll_id.is_empty() { return busy(false, "The server did not start the sign-in"); }
        if let Err(e) = open::that(&url) { return busy(false, &format!("Could not open the browser: {e}")); }
        if let Some(w) = ui() { w.set_oauth_code(if code.is_empty() { "…".into() } else { code.into() }); w.set_signin_busy(false); }
        bg(move || {
            // Up to 10 minutes, every 2 seconds.
            for _ in 0..300 {
                if cancel.load(Ordering::Relaxed) { return Err(ApiError::Status(0, String::new())); }
                std::thread::sleep(Duration::from_secs(2));
                match api::oauth_poll(&base, &poll_id) {
                    Ok(v) => match s(&v, "status").as_str() {
                        "done" => return api::Tokens::from_answer(&v).map(|t| SignedIn { base: base.clone(), tokens: t, username: String::new() }).ok_or(ApiError::Status(0, "The server sent no session".into())),
                        "failed" | "expired" => return Err(ApiError::Status(0, format!("Sign-in {}", s(&v, "status")))),
                        _ => {}
                    },
                    Err(ApiError::Net(_)) => {}
                    Err(e) => return Err(e),
                }
            }
            Err(ApiError::Status(0, "Sign-in timed out".into()))
        }, |r| {
            if let Some(w) = ui() { w.set_oauth_code("".into()); }
            match r {
                Err(ApiError::Status(0, m)) if m.is_empty() => busy(false, ""),
                r => signed_in(r),
            }
        });
    });
}

fn cancel_oauth() {
    with(|a| if let Some(c) = a.oauth_cancel.take() { c.store(true, Ordering::Relaxed); });
    if let Some(w) = ui() { w.set_oauth_code("".into()); }
    busy(false, "");
}

// ─── The session ───

fn start_session(si: SignedIn) {
    let SignedIn { base, tokens, username } = si;
    let saved = store::load();
    let username = if username.is_empty() && saved.server == base { saved.username.clone() } else { username };
    store::update(|s| {
        if s.server != base { s.project.clear(); s.chat.clear(); }
        s.server = base.clone();
        s.refresh_token = tokens.refresh.clone();
        s.username = username.clone();
    });
    let api = Api::new(&base, tokens, |t| store::update(|s| s.refresh_token = t.refresh.clone()));
    let live = live::start(api.clone(), on_live);
    with(|a| {
        if let Some(old) = a.live.take() { old.stop(); }
        a.api = Some(api);
        a.live = Some(live);
        a.project = saved.project.clone();
        a.chat = String::new();
    });
    if let Some(w) = ui() {
        w.set_signed_in(true);
        w.set_server(host_of(&base).into());
        w.set_username(username.into());
        w.set_signin_checked(false);
    }
    load_projects(saved.chat);
    load_sessions();
}

/// Sign out here (and on the server, unless it already ended the login).
pub fn sign_out(why: &str) {
    let (api, live) = with(|a| (a.api.take(), a.live.take())).unwrap_or((None, None));
    if let Some(l) = live { l.stop(); }
    if let Some(api) = api {
        if why.is_empty() { std::thread::spawn(move || { let _ = api.post("/api/auth/logout", &json!({})); }); }
    }
    store::update(|s| { s.refresh_token.clear(); });
    with(|a| {
        a.projects.clear();
        a.sessions.clear();
        a.chat.clear();
        a.conv = Conversation::default();
        a.shown.clear();
        a.entries.set_vec(vec![]);
        a.project_rows.set_vec(vec![]);
        a.chat_rows.set_vec(vec![]);
    });
    if let Some(w) = ui() {
        w.set_signed_in(false);
        w.set_current_chat("".into());
        w.set_signin_error(why.into());
    }
}

fn api() -> Option<Api> {
    with(|a| a.api.clone()).flatten()
}

/// A call on a worker thread; signed-out answers sign out here too.
fn call(work: impl FnOnce(&Api) -> api::ApiResult + Send + 'static, done: impl FnOnce(Result<Value, String>) + Send + 'static) {
    let Some(api) = api() else { return };
    bg(move || work(&api), move |r| match r {
        Err(ApiError::SignedOut) => sign_out("Your sign-in ended. Sign in again."),
        Err(e) => done(Err(e.to_string())),
        Ok(v) => done(Ok(v)),
    });
}

fn on_live(e: Event) {
    match e {
        Event::Connected(on) => { if let Some(w) = ui() { w.set_online(on); } }
        Event::SignedOut => sign_out("Your sign-in ended. Sign in again."),
        Event::State(state) => {
            if let Some(p) = state.get("projects").and_then(|p| p.as_array()) {
                if !p.is_empty() {
                    with(|a| a.projects = p.clone());
                    show_projects();
                }
            }
            load_sessions();
        }
        Event::Chat(m) => on_chat_message(&m),
    }
}

// ─── Projects and chats ───

fn load_projects(reopen_chat: String) {
    call(|api| api.get("/api/projects"), move |r| {
        let Ok(v) = r else { return };
        let list = v.get("projects").and_then(|p| p.as_array()).cloned().unwrap_or_default();
        let first = list.first().map(|p| s(p, "alias")).unwrap_or_default();
        let pick = with(|a| {
            a.projects = list.clone();
            if list.iter().any(|p| s(p, "alias") == a.project) { a.project.clone() } else { first.clone() }
        }).unwrap_or_default();
        select_project(&pick);
        if !reopen_chat.is_empty() { select_chat(&reopen_chat); }
    });
}

fn load_sessions() {
    let go = with(|a| {
        if a.sessions_busy { a.sessions_dirty = true; return false; }
        a.sessions_busy = true;
        true
    }).unwrap_or(false);
    if !go { return; }
    call(|api| api.get("/api/ui-sessions"), |r| {
        let again = with(|a| {
            a.sessions_busy = false;
            if let Ok(v) = &r { a.sessions = v.get("sessions").and_then(|s| s.as_array()).cloned().unwrap_or_default(); }
            std::mem::take(&mut a.sessions_dirty)
        }).unwrap_or(false);
        show_projects();
        show_chats();
        if again { load_sessions(); }
    });
}

fn chat_state(c: &Value) -> String {
    if s(c, "status") == "exited" { return "exited".into(); }
    let st = s(c, "agentState");
    if st.is_empty() { "idle".into() } else { st }
}

fn pending(c: &Value) -> i32 {
    c.get("pending").and_then(|p| p.as_array()).map(|p| p.len() as i32).unwrap_or(0)
}

fn show_projects() {
    with(|a| {
        let rows: Vec<ProjectItem> = a.projects.iter().map(|p| {
            let alias = s(p, "alias");
            let mine: Vec<&Value> = a.sessions.iter().filter(|c| s(c, "project") == alias && s(c, "factory").is_empty()).collect();
            ProjectItem {
                name: { let n = s(p, "name"); if n.is_empty() { alias.clone() } else { n } }.into(),
                chats: mine.len() as i32,
                attention: mine.iter().filter(|c| pending(c) > 0 || chat_state(c) == "needs-input").count() as i32,
                alias: alias.into(),
            }
        }).collect();
        a.project_rows.set_vec(rows);
    });
}

fn show_chats() {
    let title = with(|a| {
        let mut mine: Vec<&Value> = a.sessions.iter().filter(|c| s(c, "project") == a.project && s(c, "factory").is_empty()).collect();
        mine.sort_by_key(|c| c.get("order").and_then(|o| o.as_i64()).unwrap_or(0));
        let rows: Vec<ChatItem> = mine.iter().map(|c| ChatItem {
            id: s(c, "id").into(),
            title: s(c, "title").into(),
            state: chat_state(c).into(),
            pending: pending(c),
        }).collect();
        // Rewrite only what changed: the list is redrawn on every server event.
        let same = rows.len() == a.chat_rows.row_count() && rows.iter().enumerate().all(|(i, r)| a.chat_rows.row_data(i).as_ref() == Some(r));
        if !same { a.chat_rows.set_vec(rows); }
        a.sessions.iter().find(|c| s(c, "id") == a.chat).map(|c| s(c, "title"))
    }).flatten();
    if let (Some(w), Some(t)) = (ui(), title) {
        if !t.is_empty() { w.set_chat_title(t.into()); }
    }
}

pub fn select_project(alias: &str) {
    let changed = with(|a| { let c = a.project != alias; a.project = alias.to_string(); c }).unwrap_or(false);
    if let Some(w) = ui() { w.set_current_project(alias.into()); }
    if changed { select_chat(""); }
    store::update(|s| s.project = alias.to_string());
    show_chats();
}

pub fn select_chat(id: &str) {
    let (old, live_ok) = with(|a| (std::mem::replace(&mut a.chat, id.to_string()), a.live.is_some())).unwrap_or_default();
    with(|a| {
        if let Some(l) = &a.live {
            if !old.is_empty() && old != id { l.unsubscribe(&old); }
            if !id.is_empty() { l.subscribe(id); }
        }
        a.conv = Conversation::default();
        a.shown.clear();
        a.entries.set_vec(vec![]);
        // A chat in another project: show that project.
        if let Some(c) = a.sessions.iter().find(|c| s(c, "id") == id) {
            let p = s(c, "project");
            if !p.is_empty() { a.project = p; }
        }
    });
    let project = with(|a| a.project.clone()).unwrap_or_default();
    if let Some(w) = ui() {
        w.set_current_chat(id.into());
        w.set_current_project(project.into());
        w.set_chat_error("".into());
        w.set_chat_state("".into());
        w.set_chat_title("".into());
    }
    let _ = live_ok;
    store::update(|s| s.chat = id.to_string());
    show_chats();
}

pub fn new_chat() {
    let project = with(|a| a.project.clone()).unwrap_or_default();
    if project.is_empty() { return; }
    call(move |api| api.post("/api/ui-sessions/create", &json!({ "project": project })), |r| match r {
        Ok(v) => {
            let id = s(&v, "id");
            load_sessions();
            if !id.is_empty() { select_chat(&id); }
        }
        Err(e) => if let Some(w) = ui() { w.set_chat_error(format!("Could not start a chat: {e}").into()); },
    });
}

// ─── The open chat ───

fn on_chat_message(m: &Value) {
    let id = s(m, "id");
    let changed = with(|a| {
        if id != a.chat { return false; }
        match s(m, "type").as_str() {
            "ui-history" => { if let Some(sess) = m.get("session") { a.conv.load(sess); } true }
            "ui-event" => m.get("event").map(|e| a.conv.apply(e)).unwrap_or(false),
            _ => false,
        }
    }).unwrap_or(false);
    if changed { show_entries(); }
}

/// Put the conversation on screen, rewriting only the entries that changed
/// (a streamed reply changes one entry many times a second).
fn show_entries() {
    let (title, state) = with(|a| {
        let now = a.conv.view();
        for (i, e) in now.iter().enumerate() {
            if a.shown.get(i) == Some(e) { continue; }
            let row = to_row(e);
            if i < a.entries.row_count() { a.entries.set_row_data(i, row); } else { a.entries.push(row); }
        }
        while a.entries.row_count() > now.len() { a.entries.remove(a.entries.row_count() - 1); }
        a.shown = now;
        (a.conv.title.clone(), a.conv.state.clone())
    }).unwrap_or_default();
    if let Some(w) = ui() {
        if !title.is_empty() { w.set_chat_title(title.into()); }
        w.set_chat_state(state.into());
    }
}

fn to_row(e: &Shown) -> EntryView {
    EntryView {
        id: e.id.clone().into(),
        kind: e.kind.into(),
        text: e.text.clone().into(),
        title: e.title.clone().into(),
        detail: e.detail.clone().into(),
        status: e.status.clone().into(),
        error: e.error,
        request_id: e.request_id.clone().into(),
        options: ModelRc::new(VecModel::from(e.options.iter().map(|o| SharedString::from(o.as_str())).collect::<Vec<_>>())),
    }
}

fn chat_error(e: String) {
    if let Some(w) = ui() { w.set_chat_error(e.into()); }
}

pub fn send(text: String) {
    let text = text.trim_end().to_string();
    if text.trim().is_empty() { return; }
    let chat = with(|a| a.chat.clone()).unwrap_or_default();
    if let Some(w) = ui() { w.set_draft("".into()); w.set_chat_error("".into()); }
    if chat.is_empty() {
        // No chat yet: start one in this project, then send.
        let project = with(|a| a.project.clone()).unwrap_or_default();
        return call(move |api| api.post("/api/ui-sessions/create", &json!({ "project": project })), move |r| match r {
            Ok(v) => {
                let id = s(&v, "id");
                load_sessions();
                select_chat(&id);
                send(text);
            }
            Err(e) => chat_error(format!("Could not start a chat: {e}")),
        });
    }
    let keep = text.clone();
    call(move |api| api.post(&format!("/api/ui-sessions/{chat}/send"), &json!({ "text": text })), move |r| {
        if let Err(e) = r {
            // Give the words back, so nothing is lost.
            if let Some(w) = ui() { if w.get_draft().is_empty() { w.set_draft(keep.into()); } }
            chat_error(format!("Not sent: {e}"));
        }
    });
}

pub fn stop() {
    let chat = with(|a| a.chat.clone()).unwrap_or_default();
    if chat.is_empty() { return; }
    call(move |api| api.post(&format!("/api/ui-sessions/{chat}/interrupt"), &json!({})), |r| if let Err(e) = r { chat_error(e) });
}

pub fn respond(request_id: String, behavior: String, always: bool) {
    let chat = with(|a| a.chat.clone()).unwrap_or_default();
    let mut body = json!({ "requestId": request_id, "behavior": behavior });
    if always { body["always"] = json!(true); }
    // Dismissing a question says so, as the web UI does.
    let is_question = with(|a| a.conv.entries.iter().any(|e| s(e, "requestId") == request_id && s(e, "kind") == "question")).unwrap_or(false);
    if is_question && behavior == "deny" { body["message"] = json!("The user dismissed the question."); }
    call(move |api| api.post(&format!("/api/ui-sessions/{chat}/respond"), &body), |r| if let Err(e) = r { chat_error(e) });
}

/// A choice on a question (AskUserQuestion): allow, with the answer filled in.
pub fn answer(entry_id: &str, option: &str) {
    let found = with(|a| {
        let e = a.conv.entries.iter().find(|e| s(e, "id") == entry_id)?;
        let mut input = e.get("input").cloned().unwrap_or(json!({}));
        let question = input.pointer("/questions/0/question").and_then(|q| q.as_str()).unwrap_or("").to_string();
        input["answers"] = json!({ question: option });
        Some((a.chat.clone(), s(e, "requestId"), input))
    }).flatten();
    let Some((chat, request_id, input)) = found else { return };
    call(move |api| api.post(&format!("/api/ui-sessions/{chat}/respond"), &json!({ "requestId": request_id, "behavior": "allow", "updatedInput": input })), |r| if let Err(e) = r { chat_error(e) });
}

// ─── For the test driver (src/testctl.rs) ───

pub fn snapshot_state() -> Value {
    with(|a| {
        json!({
            "signedIn": a.api.is_some(),
            "server": a.api.as_ref().map(|x| x.base().to_string()),
            "project": a.project,
            "chat": a.chat,
            "projects": a.projects.iter().map(|p| s(p, "alias")).collect::<Vec<_>>(),
            "chats": a.chat_rows.iter().map(|c| json!({ "id": c.id.as_str(), "title": c.title.as_str(), "state": c.state.as_str(), "pending": c.pending })).collect::<Vec<_>>(),
            "chatState": a.conv.state,
            "entries": a.shown.iter().map(|e| json!({ "id": e.id, "kind": e.kind, "text": e.text, "title": e.title, "status": e.status, "requestId": e.request_id, "options": e.options })).collect::<Vec<_>>(),
        })
    }).unwrap_or(Value::Null)
}
