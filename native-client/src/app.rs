//! The app: signing in, the session, projects, and the workbench with its
//! panes (src/mosaic.rs lays them out). Every chat in a pane has its own
//! conversation, kept live over /ws. Runs on the UI thread; anything that talks
//! to the server runs on a worker thread ([bg]) and comes back here.

use crate::{
    api::{self, Api, ApiError, SignedIn},
    chat::{Conversation, Shown},
    live::{self, Event, Live},
    mosaic::{Dir, Mosaic},
    store, EntryView, GutterView, MainWindow, PaneView, ProjectItem, ToolView,
};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, StyledText, VecModel, Weak};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

/// One chat shown in a pane.
struct ChatPaneState {
    conv: Conversation,
    rows: Vec<Row>,
    model: Rc<VecModel<EntryView>>,
    error: String,
}

/// What one row of a chat log shows (compared to rewrite only changed rows).
#[derive(Clone, PartialEq, Debug)]
enum Row {
    Entry(Shown),
    /// Claude's text, split at code fences: prose (markdown) or a code block.
    Prose(String, String),
    Code(String, String),
    Tools(String, Vec<Shown>),
}

pub struct App {
    ui: Weak<MainWindow>,
    api: Option<Api>,
    live: Option<Live>,
    projects: Vec<Value>,
    sessions: Vec<Value>,
    project: String,
    mosaic: Mosaic,
    chats: HashMap<String, ChatPaneState>,
    focused: u32,
    area: (f32, f32),
    pending: u32,
    project_rows: Rc<VecModel<ProjectItem>>,
    pane_rows: Rc<VecModel<PaneView>>,
    gutter_rows: Rc<VecModel<GutterView>>,
    oauth_cancel: Option<Arc<AtomicBool>>,
    sessions_busy: bool,
    sessions_dirty: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.borrow_mut().as_mut().map(f))
}

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

fn chat_key(id: &str) -> String {
    format!("live:{id}")
}

fn chat_of(key: &str) -> Option<&str> {
    key.strip_prefix("live:")
}

// ─── Starting ───

pub fn init(window: &MainWindow) {
    let project_rows = Rc::new(VecModel::<ProjectItem>::default());
    let pane_rows = Rc::new(VecModel::<PaneView>::default());
    let gutter_rows = Rc::new(VecModel::<GutterView>::default());
    window.set_projects(project_rows.clone().into());
    window.set_panes(pane_rows.clone().into());
    window.set_gutters(gutter_rows.clone().into());
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            ui: window.as_weak(),
            api: None,
            live: None,
            projects: vec![],
            sessions: vec![],
            project: String::new(),
            mosaic: Mosaic::default(),
            chats: HashMap::new(),
            focused: 0,
            area: (1100.0, 760.0),
            pending: 0,
            project_rows,
            pane_rows,
            gutter_rows,
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
    window.on_wb_area(|w, h| { with(|a| a.area = (w, h)); render(); });
    window.on_focus_pane(|id| { with(|a| a.focused = id as u32); render(); });
    window.on_close_pane(|id| close_pane(id as u32));
    window.on_gutter_drag(|sp, i, d, span| { with(|a| a.mosaic.drag(sp as u32, i as usize, d, span)); layout_changed(); });
    window.on_gutter_even(|sp, i| { with(|a| a.mosaic.even(sp as u32, i as usize)); layout_changed(); });
    window.on_split(|id, down| { with(|a| { a.mosaic.split(id as u32, if down { Dir::Col } else { Dir::Row }); }); layout_changed(); });
    window.on_launch_chat(|id, skip| launch_chat(id as u32, skip));
    window.on_add_pane(|k| add_pane(k.as_str()));
    window.on_send(|k, t| send(k.as_str(), t.to_string()));
    window.on_stop(|k| stop(k.as_str()));
    window.on_respond(|k, id, b, always| respond(k.as_str(), id.to_string(), b.to_string(), always));
    window.on_answer(|k, id, o| answer(k.as_str(), id.as_str(), o.as_str()));

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
        if s.server != base { s.project.clear(); s.layouts.clear(); }
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
        a.project = String::new();
    });
    if let Some(w) = ui() {
        w.set_signed_in(true);
        w.set_server(host_of(&base).into());
        w.set_username(username.into());
        w.set_signin_checked(false);
    }
    load_all(saved.project);
}

pub fn sign_out(why: &str) {
    let (api, live) = with(|a| (a.api.take(), a.live.take())).unwrap_or((None, None));
    if let Some(l) = live { l.stop(); }
    if let Some(api) = api {
        if why.is_empty() { std::thread::spawn(move || { let _ = api.post("/api/auth/logout", &json!({})); }); }
    }
    store::update(|s| s.refresh_token.clear());
    with(|a| {
        a.projects.clear();
        a.sessions.clear();
        a.chats.clear();
        a.project.clear();
        a.mosaic = Mosaic::default();
        a.project_rows.set_vec(vec![]);
        a.pane_rows.set_vec(vec![]);
        a.gutter_rows.set_vec(vec![]);
    });
    if let Some(w) = ui() {
        w.set_signed_in(false);
        w.set_current_project("".into());
        w.set_signin_error(why.into());
    }
}

fn api() -> Option<Api> {
    with(|a| a.api.clone()).flatten()
}

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
                if !p.is_empty() { with(|a| a.projects = p.clone()); show_projects(); }
            }
            load_sessions();
        }
        Event::Chat(m) => on_chat_message(&m),
    }
}

// ─── Projects ───

/// Projects and chats, then the project to show.
fn load_all(want: String) {
    call(|api| api.get("/api/projects"), move |r| {
        let Ok(v) = r else { return };
        let list = v.get("projects").and_then(|p| p.as_array()).cloned().unwrap_or_default();
        with(|a| a.projects = list.clone());
        let pick = if list.iter().any(|p| s(p, "alias") == want) { want.clone() } else { list.first().map(|p| s(p, "alias")).unwrap_or_default() };
        call(|api| api.get("/api/ui-sessions"), move |r| {
            if let Ok(v) = r { with(|a| a.sessions = v.get("sessions").and_then(|s| s.as_array()).cloned().unwrap_or_default()); }
            show_projects();
            select_project(&pick);
        });
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
        sync_panes();
        if again { load_sessions(); }
    });
}

fn agent_state(c: &Value) -> String {
    if s(c, "status") == "exited" { return "exited".into(); }
    let pending = c.get("pending").and_then(|p| p.as_array()).map(|p| !p.is_empty()).unwrap_or(false);
    if pending { return "needs-input".into(); }
    let st = s(c, "agentState");
    if st.is_empty() { "idle".into() } else { st }
}

/// A project's dot, by the web UI's priority.
fn project_state(a: &App, alias: &str) -> &'static str {
    let mine: Vec<String> = a.sessions.iter().filter(|c| s(c, "project") == alias && s(c, "status") != "exited").map(agent_state).collect();
    if mine.iter().any(|x| x == "needs-input") { "needs-input" }
    else if mine.iter().any(|x| x == "working") { "working" }
    else if mine.iter().any(|x| x == "waiting") { "waiting" }
    else if !mine.is_empty() { "running" }
    else { "" }
}

fn show_projects() {
    with(|a| {
        let rows: Vec<ProjectItem> = a.projects.iter().map(|p| {
            let alias = s(p, "alias");
            ProjectItem {
                name: { let n = s(p, "name"); if n.is_empty() { alias.clone() } else { n } }.into(),
                state: project_state(a, &alias).into(),
                alias: alias.into(),
            }
        }).collect();
        let same = rows.len() == a.project_rows.row_count() && rows.iter().enumerate().all(|(i, r)| a.project_rows.row_data(i).as_ref() == Some(r));
        if !same { a.project_rows.set_vec(rows); }
    });
}

pub fn select_project(alias: &str) {
    // Leave the old project's chats; load this one's layout.
    let (old_chats, name) = with(|a| {
        let old: Vec<String> = a.chats.keys().cloned().collect();
        a.chats.clear();
        a.project = alias.to_string();
        let saved = store::load();
        a.mosaic = saved.layouts.get(alias).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
        a.focused = 0;
        // New launchers count on from the saved ones (pend:<n>).
        a.pending = a.mosaic.keys().iter().filter_map(|k| k.strip_prefix("pend:").and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0);
        let name = a.projects.iter().find(|p| s(p, "alias") == alias).map(|p| { let n = s(p, "name"); if n.is_empty() { alias.to_string() } else { n } }).unwrap_or_default();
        (old, name)
    }).unwrap_or_default();
    with(|a| if let Some(l) = &a.live { for id in &old_chats { l.unsubscribe(id); } });
    if let Some(w) = ui() {
        w.set_current_project(alias.into());
        w.set_project_name(name.into());
    }
    store::update(|s| s.project = alias.to_string());
    sync_panes();
}

// ─── The workbench ───

/// The panes follow the project's live chats: every running chat has a pane
/// (new ones are added to the right), and panes of chats that are gone close.
fn sync_panes() {
    let subscribe = with(|a| {
        if a.project.is_empty() { return vec![]; }
        let live: Vec<String> = a.sessions.iter()
            .filter(|c| s(c, "project") == a.project && s(c, "status") != "exited" && s(c, "factory").is_empty())
            .map(|c| s(c, "id")).collect();
        a.mosaic.retain(|k| match chat_of(k) { Some(id) => live.iter().any(|l| l == id), None => true });
        for id in &live { a.mosaic.place(&chat_key(id)); }
        // Chats in panes: a conversation each, subscribed once.
        let mut new = vec![];
        for k in a.mosaic.keys() {
            if let Some(id) = chat_of(&k) {
                if !a.chats.contains_key(id) {
                    a.chats.insert(id.to_string(), ChatPaneState { conv: Conversation::default(), rows: vec![], model: Rc::new(VecModel::default()), error: String::new() });
                    new.push(id.to_string());
                }
            }
        }
        a.chats.retain(|id, _| live.contains(id));
        new
    }).unwrap_or_default();
    with(|a| if let Some(l) = &a.live { for id in &subscribe { l.subscribe(id); } });
    save_layout();
    render();
}

fn save_layout() {
    let (project, m) = with(|a| (a.project.clone(), serde_json::to_value(&a.mosaic).ok())).unwrap_or_default();
    if project.is_empty() { return; }
    if let Some(m) = m { store::update(|s| { s.layouts.insert(project, m); }); }
}

fn layout_changed() {
    save_layout();
    render();
}

fn mode_label(m: &str) -> &'static str {
    match m {
        "plan" => "Plan mode",
        "acceptEdits" => "Accept edits",
        "auto" => "Auto",
        "dontAsk" => "Don't ask",
        "bypassPermissions" => "Skip permissions",
        _ => "Ask permissions",
    }
}

/// Lay the panes out and put them on screen (only rows that changed are rewritten).
fn render() {
    let content_w = with(|a| {
        let (w, h) = a.area;
        let cw = a.mosaic.content_width(w);
        let (leaves, gutters) = a.mosaic.layout(cw, h);
        if a.focused == 0 || !leaves.iter().any(|l| l.id == a.focused) {
            a.focused = leaves.first().map(|l| l.id).unwrap_or(0);
        }
        let panes: Vec<PaneView> = leaves.iter().map(|l| {
            let key = l.key.clone().unwrap_or_default();
            let mut p = PaneView {
                id: l.id as i32,
                key: key.clone().into(),
                kind: "empty".into(),
                x: l.x, y: l.y, w: l.w, h: l.h,
                focused: l.id == a.focused,
                ..Default::default()
            };
            if let Some(id) = chat_of(&key) {
                let item = a.sessions.iter().find(|c| s(c, "id") == id);
                p.kind = "chat".into();
                p.title = item.map(|c| { let t = s(c, "longTitle"); if t.is_empty() { s(c, "title") } else { t } }).filter(|t| !t.is_empty()).unwrap_or_else(|| "New chat".into()).into();
                if let Some(cs) = a.chats.get(id) {
                    p.entries = cs.model.clone().into();
                    p.state = { let st = if cs.conv.state.is_empty() { item.map(agent_state).unwrap_or_default() } else { cs.conv.state.clone() }; if st.is_empty() { "idle".into() } else { st.into() } };
                    p.mode = mode_label(&if cs.conv.mode.is_empty() { item.map(|c| s(c, "permissionMode")).unwrap_or_default() } else { cs.conv.mode.clone() }).into();
                    p.session = cs.conv.session_id.chars().take(8).collect::<String>().into();
                    p.model = cs.conv.model.clone().into();
                    p.cost = if cs.conv.cost > 0.0 { format!("${:.4}", cs.conv.cost).into() } else { SharedString::default() };
                    p.error = cs.error.clone().into();
                }
            } else if key.starts_with("pend:") {
                p.kind = "launcher".into();
                p.title = "New terminal".into();
            } else if let Some(kind) = key.strip_prefix("panel:") {
                p.kind = "panel".into();
                let mut c = kind.chars();
                p.title = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default().into();
            }
            p
        }).collect();
        if panes.len() == a.pane_rows.row_count() {
            for (i, p) in panes.into_iter().enumerate() {
                if a.pane_rows.row_data(i).as_ref() != Some(&p) { a.pane_rows.set_row_data(i, p); }
            }
        } else {
            a.pane_rows.set_vec(panes);
        }
        let gs: Vec<GutterView> = gutters.iter().map(|g| GutterView {
            split: g.split as i32, index: g.index as i32, row: g.dir == Dir::Row,
            x: g.x, y: g.y, w: g.w, h: g.h, span: g.span,
        }).collect();
        let same = gs.len() == a.gutter_rows.row_count() && gs.iter().enumerate().all(|(i, g)| a.gutter_rows.row_data(i).as_ref() == Some(g));
        if !same { a.gutter_rows.set_vec(gs); }
        cw
    });
    if let (Some(w), Some(cw)) = (ui(), content_w) { w.set_content_width(cw); }
}

fn close_pane(leaf: u32) {
    let key = with(|a| {
        let (leaves, _) = a.mosaic.layout(1000.0, 1000.0);
        leaves.iter().find(|l| l.id == leaf).and_then(|l| l.key.clone())
    }).flatten();
    with(|a| a.mosaic.close(leaf));
    layout_changed();
    // Closing a chat's pane closes the chat, as in the web UI.
    if let Some(id) = key.as_deref().and_then(chat_of).map(str::to_string) {
        call(move |api| api.post(&format!("/api/ui-sessions/{id}/close"), &json!({})), |_| load_sessions());
    }
}

fn add_pane(kind: &str) {
    with(|a| {
        if let Some(preset) = kind.strip_prefix("layout:") {
            a.mosaic = Mosaic::preset(preset, a.mosaic.keys());
            return;
        }
        if kind == "fit" {
            a.mosaic = { let keys = a.mosaic.keys(); let mut m = Mosaic::default(); for k in keys { m.place(&k); } m };
            return;
        }
        let key = if kind == "terminal" { a.pending += 1; format!("pend:{}", a.pending) } else { format!("panel:{kind}") };
        a.focused = a.mosaic.place(&key);
    });
    layout_changed();
}

/// UI Mode in a launcher: a new chat in this project, in that pane.
fn launch_chat(leaf: u32, skip: bool) {
    let project = with(|a| a.project.clone()).unwrap_or_default();
    if project.is_empty() { return; }
    call(move |api| api.post("/api/ui-sessions/create", &json!({ "project": project, "skipPermissions": skip })), move |r| match r {
        Ok(v) => {
            let id = s(&v, "id");
            if id.is_empty() { return; }
            with(|a| { a.mosaic.assign(leaf, Some(chat_key(&id))); a.focused = leaf; });
            save_layout();
            load_sessions();
        }
        Err(e) => eprintln!("[native] could not start a chat: {e}"),
    });
}

// ─── Chats in panes ───

fn on_chat_message(m: &Value) {
    let id = s(m, "id");
    let changed = with(|a| {
        let Some(cs) = a.chats.get_mut(&id) else { return false };
        match s(m, "type").as_str() {
            "ui-history" => { if let Some(sess) = m.get("session") { cs.conv.load(sess); } true }
            "ui-event" => m.get("event").map(|e| cs.conv.apply(e)).unwrap_or(false),
            _ => false,
        }
    }).unwrap_or(false);
    if changed {
        show_chat(&id);
        render();
    }
}

/// Claude's text as rows: prose (markdown) and fenced code blocks.
fn split_fences(id: &str, text: &str) -> Vec<Row> {
    let mut out = vec![];
    let mut prose = String::new();
    let mut code: Option<String> = None;
    for line in text.split('\n') {
        if line.trim_start().starts_with("```") {
            match code.take() {
                Some(c) => out.push(Row::Code(format!("{id}#{}", out.len()), c.trim_end().to_string())),
                None => {
                    if !prose.trim().is_empty() { out.push(Row::Prose(format!("{id}#{}", out.len()), prose.trim_end().to_string())); }
                    prose.clear();
                    code = Some(String::new());
                }
            }
            continue;
        }
        match &mut code {
            Some(c) => { c.push_str(line); c.push('\n'); }
            None => { prose.push_str(line); prose.push('\n'); }
        }
    }
    if let Some(c) = code { out.push(Row::Code(format!("{id}#{}", out.len()), c)); }
    if !prose.trim().is_empty() { out.push(Row::Prose(format!("{id}#{}", out.len()), prose.trim_end().to_string())); }
    out
}

/// The log's rows: consecutive tool calls in one group ("N tools"), Claude's
/// text split at code fences.
fn rows_of(shown: &[Shown]) -> Vec<Row> {
    let mut rows = vec![];
    let mut group: Vec<Shown> = vec![];
    let flush = |group: &mut Vec<Shown>, rows: &mut Vec<Row>| {
        if !group.is_empty() {
            rows.push(Row::Tools(group[0].id.clone(), std::mem::take(group)));
        }
    };
    for e in shown {
        if e.kind == "tool" { group.push(e.clone()); continue; }
        flush(&mut group, &mut rows);
        if e.kind == "assistant" { rows.extend(split_fences(&e.id, &e.text)); } else { rows.push(Row::Entry(e.clone())); }
    }
    flush(&mut group, &mut rows);
    rows
}

fn md(text: &str) -> StyledText {
    StyledText::from_markdown(text).unwrap_or_else(|_| StyledText::from_markdown(&text.replace(['<', '>', '*', '_', '`', '[', ']'], "")).unwrap_or_default())
}

fn to_view(r: &Row) -> EntryView {
    match r {
        Row::Prose(id, t) => EntryView { id: id.clone().into(), kind: "assistant".into(), text: t.clone().into(), md: md(t), ..Default::default() },
        Row::Code(id, t) => EntryView { id: id.clone().into(), kind: "code".into(), text: t.clone().into(), ..Default::default() },
        Row::Tools(id, tools) => {
            let running = tools.iter().any(|t| t.status == "running");
            let error = tools.iter().any(|t| t.error);
            let first = &tools[0];
            EntryView {
                id: id.clone().into(),
                kind: "tools".into(),
                title: (if !first.note.is_empty() { first.note.clone() } else if first.detail.is_empty() { first.title.clone() } else { format!("{} {}", first.title, first.detail) }).into(),
                detail: format!("{} tool{}", tools.len(), if tools.len() == 1 { "" } else { "s" }).into(),
                status: if running { "running".into() } else { "done".into() },
                error,
                tools: ModelRc::new(VecModel::from(tools.iter().map(|t| ToolView {
                    title: t.title.clone().into(),
                    detail: t.detail.clone().into(),
                    text: t.text.clone().into(),
                    status: t.status.clone().into(),
                    error: t.error,
                }).collect::<Vec<_>>())),
                ..Default::default()
            }
        }
        Row::Entry(e) => EntryView {
            id: e.id.clone().into(),
            kind: e.kind.into(),
            text: e.text.clone().into(),
            title: e.title.clone().into(),
            detail: e.detail.clone().into(),
            status: e.status.clone().into(),
            error: e.error,
            request_id: e.request_id.clone().into(),
            options: ModelRc::new(VecModel::from(e.options.iter().map(|o| SharedString::from(o.as_str())).collect::<Vec<_>>())),
            ..Default::default()
        },
    }
}

/// A chat's log on screen, rewriting only the rows that changed.
fn show_chat(id: &str) {
    with(|a| {
        let Some(cs) = a.chats.get_mut(id) else { return };
        let now = rows_of(&cs.conv.view());
        for (i, r) in now.iter().enumerate() {
            if cs.rows.get(i) == Some(r) { continue; }
            let v = to_view(r);
            if i < cs.model.row_count() { cs.model.set_row_data(i, v); } else { cs.model.push(v); }
        }
        while cs.model.row_count() > now.len() { cs.model.remove(cs.model.row_count() - 1); }
        cs.rows = now;
    });
}

fn chat_error(id: &str, e: String) {
    with(|a| if let Some(cs) = a.chats.get_mut(id) { cs.error = e; });
    render();
}

pub fn send(key: &str, text: String) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let text = text.trim_end().to_string();
    if text.trim().is_empty() { return; }
    chat_error(&id, String::new());
    let id2 = id.clone();
    call(move |api| api.post(&format!("/api/ui-sessions/{id}/send"), &json!({ "text": text })), move |r| {
        if let Err(e) = r { chat_error(&id2, format!("Not sent: {e}")); }
    });
}

pub fn stop(key: &str) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let id2 = id.clone();
    call(move |api| api.post(&format!("/api/ui-sessions/{id}/interrupt"), &json!({})), move |r| if let Err(e) = r { chat_error(&id2, e) });
}

pub fn respond(key: &str, request_id: String, behavior: String, always: bool) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let mut body = json!({ "requestId": request_id, "behavior": behavior });
    if always { body["always"] = json!(true); }
    let is_question = with(|a| a.chats.get(&id).map(|cs| cs.conv.entries.iter().any(|e| s(e, "requestId") == request_id && s(e, "kind") == "question")).unwrap_or(false)).unwrap_or(false);
    if is_question && behavior == "deny" { body["message"] = json!("The user dismissed the question."); }
    let id2 = id.clone();
    call(move |api| api.post(&format!("/api/ui-sessions/{id}/respond"), &body), move |r| if let Err(e) = r { chat_error(&id2, e) });
}

pub fn answer(key: &str, entry_id: &str, option: &str) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let found = with(|a| {
        let cs = a.chats.get(&id)?;
        let e = cs.conv.entries.iter().find(|e| s(e, "id") == entry_id)?;
        let mut input = e.get("input").cloned().unwrap_or(json!({}));
        let question = input.pointer("/questions/0/question").and_then(|q| q.as_str()).unwrap_or("").to_string();
        input["answers"] = json!({ question: option });
        Some((s(e, "requestId"), input))
    }).flatten();
    let Some((request_id, input)) = found else { return };
    let id2 = id.clone();
    call(move |api| api.post(&format!("/api/ui-sessions/{id}/respond"), &json!({ "requestId": request_id, "behavior": "allow", "updatedInput": input })), move |r| if let Err(e) = r { chat_error(&id2, e) });
}

// ─── For the test driver (src/testctl.rs) ───

/// The focused pane's chat, or the first chat pane.
pub fn focused_chat() -> String {
    with(|a| {
        let (leaves, _) = a.mosaic.layout(1000.0, 1000.0);
        let key = leaves.iter().find(|l| l.id == a.focused).and_then(|l| l.key.clone()).filter(|k| k.starts_with("live:"))
            .or_else(|| leaves.iter().find_map(|l| l.key.clone().filter(|k| k.starts_with("live:"))));
        key.and_then(|k| chat_of(&k).map(str::to_string)).unwrap_or_default()
    }).unwrap_or_default()
}

/// A new chat in a new pane, as the "+" menu's Terminal then UI Mode.
pub fn new_chat_pane() {
    add_pane("terminal");
    let leaf = with(|a| a.focused).unwrap_or(0);
    launch_chat(leaf, false);
}

pub fn snapshot_state() -> Value {
    let chat = focused_chat();
    with(|a| {
        let cs = a.chats.get(&chat);
        json!({
            "signedIn": a.api.is_some(),
            "server": a.api.as_ref().map(|x| x.base().to_string()),
            "project": a.project,
            "chat": chat,
            "panes": a.mosaic.keys(),
            "projects": a.projects.iter().map(|p| s(p, "alias")).collect::<Vec<_>>(),
            "chats": a.sessions.iter().filter(|c| s(c, "project") == a.project).map(|c| json!({ "id": s(c, "id"), "title": s(c, "title"), "state": agent_state(c) })).collect::<Vec<_>>(),
            "chatState": cs.map(|c| c.conv.state.clone()).unwrap_or_default(),
            "entries": cs.map(|c| c.conv.view().iter().map(|e| json!({ "id": e.id, "kind": e.kind, "text": e.text, "title": e.title, "status": e.status, "requestId": e.request_id, "options": e.options })).collect::<Vec<_>>()).unwrap_or_default(),
        })
    }).unwrap_or(Value::Null)
}
