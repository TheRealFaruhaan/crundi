//! The app: signing in, the session, projects, and the workbench with its
//! panes (src/mosaic.rs lays them out). Every chat in a pane has its own
//! conversation, kept live over /ws. Runs on the UI thread; anything that talks
//! to the server runs on a worker thread ([bg]) and comes back here.

use crate::{
    api::{self, Api, ApiError, SignedIn},
    attach, drafts::Drafts,
    chat::{Conversation, Shown},
    live::{self, Event, Live},
    mosaic::{Dir, Mosaic},
    head, store, Composer, EntryView, GutterView, MainWindow, ModeOption, PaneView, ProjectItem, ToolView,
};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, StyledText, VecModel, Weak};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

/// A pane dragged by its header: its leaf, and the leaf under the pointer.
/// (Where it was grabbed goes to the window, for the clone.)
#[derive(Clone, Copy, Debug)]
struct Drag {
    src: u32,
    target: u32,
}

/// One chat shown in a pane.
/// A chat's uploads (2.7): what its bar shows, and how many still run.
#[derive(Default)]
struct Upload {
    name: String,
    label: String,
    frac: f32,
    active: u32,
    /// Shown (also for a moment after the last ends), and which hide is due.
    shown: bool,
    hide_gen: u32,
}

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
    /// Claude's text in blocks: a paragraph, a list item (with its marker) or a
    /// heading, as markdown; or a fenced code block. The last flag: the
    /// message's last block (the gap after it is an entry's, 10px).
    Prose(String, String, &'static str, String, bool),
    Code(String, String, bool),
    /// Tool calls: one on its own, or a batch Claude Code summed up (the summary).
    Tool(String, Shown),
    Tools(String, String, Vec<Shown>),
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
    /// The last usage numbers, and when they came.
    usage: Option<(Value, std::time::Instant)>,
    /// The server signs apps in through the browser ("app-login").
    app_login: bool,
    /// The server can pre-authorise secrets for a chat ("chat-secret-preauth"),
    /// and each chat's grants (GET /api/secrets/chat, then "chat-secrets").
    secrets_cap: bool,
    grants: HashMap<String, Value>,
    /// The pane whose × was clicked once ("Sure?"), and which arming it is.
    armed: u32,
    arm_gen: u32,
    /// The pane in full screen, by its key (it survives re-renders).
    full: Option<String>,
    /// The leaf whose narrow header has its "…" menu open (one at a time).
    menu: u32,
    /// A pane being dragged by its header (2.3).
    drag: Option<Drag>,
    /// The last `state` event's `terminals` (chats and Claude terminals).
    terminals: Option<Vec<Value>>,
    /// A gutter held: its split and index, the pair's shares and px width at
    /// the press, and where the press was along the axis.
    gutter: Option<(u32, usize, f32, f32, f32, f32)>,
    /// The pane rows' keys, one row per pane (a chat keeps its row, and so
    /// its view, wherever it moves; the row eases to its new place).
    row_keys: Vec<String>,
    motion_gen: u32,
    /// Each chat's text size in px, and the size new chats start at
    /// (saved: store::Saved.font, font_default_chat).
    font: HashMap<String, i32>,
    font_default: i32,
    /// Each chat's draft as typed (shared by its views), and how often the
    /// app itself set it (a send clears it, a refused send gives it back):
    /// the views take it again on each.
    drafts: HashMap<String, String>,
    draft_gen: HashMap<String, i32>,
    /// Chats sent to while idle: shown working at once (the web's optimistic
    /// switch), until the server's own state comes.
    sending: HashSet<String>,
    /// The pane whose Stop was clicked once ("Sure?"), and which arming it is.
    stop_armed: Option<String>,
    stop_gen: u32,
    /// The pane whose mode picker is open (2.6), by key.
    picker: Option<String>,
    /// Each chat's uploads (2.7), by pane key: the bar and how many run.
    uploads: HashMap<String, Upload>,
    /// Each chat box's caret and the selection's other end (byte offsets):
    /// where an attached path goes.
    carets: HashMap<String, (usize, usize)>,
    /// The chat a file dragged from the system is over, and the files of a
    /// drop as they come (one event each).
    file_target: Option<String>,
    dropped: Vec<std::path::PathBuf>,
    /// The drafts of the server signed in to (2.8, saved per server), the
    /// chats whose composer is built (restored), and a write to come.
    saved_drafts: Drafts,
    drafts_server: String,
    restored: HashMap<String, String>,
    drafts_dirty: bool,
    /// Errors the client adds to a chat's log (a refused send).
    local_errors: u32,
    /// The 30 s re-count of timed grants, and the toast's 3.5 s.
    timers: Vec<slint::Timer>,
    toast_gen: u32,
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

/// JavaScript's truthiness, for flags the server may send as anything.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(x) => !x.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
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
            usage: None,
            app_login: false,
            secrets_cap: false,
            grants: HashMap::new(),
            armed: 0,
            arm_gen: 0,
            full: None,
            menu: 0,
            drag: None,
            terminals: None,
            gutter: None,
            row_keys: vec![],
            motion_gen: 0,
            font: HashMap::new(),
            font_default: head::FONT_BASE,
            drafts: HashMap::new(),
            draft_gen: HashMap::new(),
            sending: HashSet::new(),
            stop_armed: None,
            stop_gen: 0,
            local_errors: 0,
            picker: None,
            uploads: HashMap::new(),
            carets: HashMap::new(),
            file_target: None,
            dropped: vec![],
            saved_drafts: Drafts::default(),
            drafts_server: String::new(),
            restored: HashMap::new(),
            drafts_dirty: false,
            timers: vec![],
            toast_gen: 0,
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
    window.on_focus_pane(|id| focus_pane(id as u32));
    window.on_close_pane(|id| close_pane(id as u32));
    // A gutter drag, as the web's: the shares and the pair's px at the press,
    // then each move from there.
    window.on_gutter_press(|sp, i, at, span| {
        with(|a| {
            let s = a.mosaic.shares(sp as u32).unwrap_or_default();
            a.gutter = s.get(i as usize).zip(s.get(i as usize + 1)).map(|(x, y)| (sp as u32, i as usize, *x, *y, span - crate::mosaic::GUTTER, at));
        });
    });
    window.on_gutter_drag(|sp, i, at, _span| {
        let moved = with(|a| match a.gutter {
            Some((gs, gi, a0, b0, total, start)) if gs == sp as u32 && gi == i as usize => { a.mosaic.drag_from(gs, gi, a0, b0, total, at - start); true }
            _ => false,
        }).unwrap_or(false);
        if moved { layout_changed(); }
    });
    window.on_gutter_even(|sp, i| { animate(); with(|a| a.mosaic.even(sp as u32, i as usize)); layout_changed(); });
    window.on_split(|id, down| { animate(); with(|a| { a.mosaic.split(id as u32, if down { Dir::Col } else { Dir::Row }); }); layout_changed(); });
    window.on_pane_drag(|id, k, x, y| pane_drag(id as u32, k, x, y));
    window.on_chip(|id, k| chip(id as u32, k.as_str()));
    window.on_launch_chat(|id, skip| launch_chat(id as u32, skip));
    window.on_add_pane(|k| add_pane(k.as_str()));
    window.on_send(|k, t| send(k.as_str(), t.to_string()));
    window.on_stop(|k| stop(k.as_str()));
    window.on_respond(|k, id, b, always| respond(k.as_str(), id.to_string(), b.to_string(), always));
    window.on_answer(|k, id, o| answer(k.as_str(), id.as_str(), o.as_str()));
    window.on_pane_close(|id| pane_close(id as u32));
    window.on_pane_full(|id| pane_full(id as u32));
    window.on_pane_font(|id, d| pane_font(id as u32, d));
    window.on_pane_pin(|id| pane_pin(id as u32));
    window.on_pane_secrets(|_id| {
        // The chat-secrets dialog the lock opens is its own screen (2.12).
    });
    window.on_pane_rename(|id, t| pane_rename(id as u32, t.as_str()));
    window.on_pane_menu(|id, on| pane_menu(id as u32, on));
    window.on_pane_maximize(|id| { animate(); with(|a| a.mosaic.maximize(id as u32)); layout_changed(); });
    window.on_draft_changed(|k, t| set_draft(k.as_str(), t.as_str()));
    // The composers' shared controls (ui/chat.slint Composer). The Enter mode
    // is one setting for every chat, saved (the web's crundi_chat_enter).
    let c = window.global::<Composer>();
    c.set_ctrl_enter(store::load().chat_enter == "newline");
    c.on_set_ctrl_enter(set_chat_enter);
    c.on_copied(|full| toast(&format!("Session id copied: {full}"), false));
    // The schedule panel (2.10) is its own screen.
    c.on_schedule(|_| {});
    c.on_mode(|k| open_mode_picker(k.as_str()));
    c.on_caret(|k, c, an| { with(|a| a.carets.insert(k.to_string(), (c.max(0) as usize, an.max(0) as usize))); });
    c.on_attach(|k| pick_file(k.to_string()));
    c.on_paste(|k| paste(k.as_str()));
    window.on_file_target(|k| { with(|a| a.file_target = Some(k.to_string())); });
    window.on_mode_choose(|m| choose_mode(m.as_str()));
    window.on_mode_close(close_mode_picker);

    window.on_change_server(|| { if let Some(w) = ui() { w.set_signin_error("".into()); w.set_signin_step("server".into()); } });
    window.on_cancel_server(|| { if let Some(w) = ui() { w.set_signin_error("".into()); w.set_signin_step("login".into()); } });

    let saved = store::load();
    window.set_signin_server(host_of(&saved.server).into());
    if saved.server.is_empty() {
        window.set_signin_step("server".into());
    } else if !saved.refresh_token.is_empty() {
        window.set_signin_step("login".into());
        resume(saved);
    } else {
        check_server(&saved.server);
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
    bg(move || {
        let m = api::methods(&base)?;
        // Whether it signs apps in through the browser (an older server does not).
        let caps = api::handshake(&base).ok().and_then(|h| h.pointer("/server/capabilities").cloned()).unwrap_or(Value::Null);
        let app_login = caps.as_array().map(|c| c.iter().any(|x| x == "app-login")).unwrap_or(false);
        Ok::<_, ApiError>((base, m, app_login))
    }, |r| {
        busy(false, "");
        let Some(w) = ui() else { return };
        match r {
            Ok((base, m, app_login)) => {
                with(|a| a.app_login = app_login);
                let on = |k: &str| m.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
                // Telegram signs in through its web widget: only through the browser.
                w.set_offer_telegram(app_login && on("telegram"));
                w.set_signin_server(host_of(&base).into());
                w.set_offer_password(on("password"));
                w.set_offer_google(on("google"));
                w.set_offer_github(on("github"));
                w.set_signin_checked(true);
                w.set_signin_step("login".into());
                store::update(|s| { if s.server != base { s.refresh_token.clear(); } s.server = base.clone(); });
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
    if with(|a| a.app_login).unwrap_or(false) {
        return sign_in_browser(base);
    }
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

/// Through the browser, as other apps sign in with an OAuth service: the
/// server's own web page, where the person signs in with any method it offers
/// (Telegram, Google, GitHub, a password…) and allows this computer; this app
/// waits for its login (the server's src/app-login.js).
fn sign_in_browser(base: String) {
    let cancel = Arc::new(AtomicBool::new(false));
    with(|a| a.oauth_cancel = Some(cancel.clone()));
    bg(move || api::app_login_start(&base).map(|v| (base, v)), move |r| {
        let (base, v) = match r {
            Ok(x) => x,
            Err(e) => return busy(false, &e.to_string()),
        };
        let (url, poll_id, code) = (s(&v, "url"), s(&v, "pollId"), s(&v, "confirmCode"));
        if url.is_empty() || poll_id.is_empty() { return busy(false, "The server did not start the sign-in"); }
        // Tests (no desktop browser): CRUNDI_OPEN_URL_FILE gets the address instead.
        let opened = match std::env::var_os("CRUNDI_OPEN_URL_FILE") {
            Some(f) => std::fs::write(f, &url).map_err(|e| e.to_string()),
            None => open::that(&url).map_err(|e| e.to_string()),
        };
        if let Err(e) = opened { return busy(false, &format!("Could not open the browser: {e}")); }
        if let Some(w) = ui() { w.set_oauth_code(code.into()); w.set_signin_busy(false); }
        bg(move || {
            // The request lives ten minutes on the server.
            for _ in 0..300 {
                if cancel.load(Ordering::Relaxed) { return Err(ApiError::Status(0, String::new())); }
                std::thread::sleep(Duration::from_secs(2));
                match api::app_login_poll(&base, &poll_id) {
                    Ok(v) => match s(&v, "status").as_str() {
                        "approved" => {
                            let tokens = v.pointer("/sessions/page").and_then(api::Tokens::from_answer).ok_or(ApiError::Status(0, "The server sent no session".into()))?;
                            return Ok(SignedIn { base: base.clone(), tokens, username: s(&v, "username") });
                        }
                        "denied" => return Err(ApiError::Status(0, "The sign-in was denied in the browser.".into())),
                        "expired" => return Err(ApiError::Status(0, "The sign-in request expired. Try again.".into())),
                        _ => {}
                    },
                    Err(ApiError::Net(_)) => {}
                    Err(e) => return Err(e),
                }
            }
            Err(ApiError::Status(0, "The sign-in request expired. Try again.".into()))
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
    with(|a| {
        a.font = saved.font.clone();
        a.font_default = saved.font_default_chat.unwrap_or(head::FONT_BASE).clamp(head::FONT_MIN, head::FONT_MAX);
        // Timed grants run out: count them again every 30 s (as the web UI).
        let t = slint::Timer::default();
        t.start(slint::TimerMode::Repeated, Duration::from_secs(30), || { if with(|a| !a.grants.is_empty()).unwrap_or(false) { render(); } });
        a.timers = vec![t];
    });
    load_all(saved.project);
    load_secrets(base.clone());
    // The usage bars now, not at the server's next push (as the web UI: latest, then fresh).
    call(|api| api.get("/api/usage/latest").or_else(|_| api.get("/api/usage")), |r| {
        if let Ok(u) = r { with(|a| a.usage = Some((u, std::time::Instant::now()))); show_usage(); }
    });
    call(|api| api.get("/api/inbox/threads"), |r| {
        if let Ok(v) = r { if let (Some(n), Some(w)) = (v.get("unread").and_then(|n| n.as_i64()), ui()) { w.set_inbox_unread(n as i32); } }
    });
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
        a.terminals = None;
        a.chats.clear();
        a.project.clear();
        a.grants.clear();
        a.secrets_cap = false;
        a.full = None;
        a.armed = 0;
        a.timers.clear();
        a.mosaic = Mosaic::default();
        a.project_rows.set_vec(vec![]);
        a.pane_rows.set_vec(vec![]);
        a.row_keys.clear();
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
            // The live list is the event's `terminals` (as the web's
            // applyState): chats and Claude terminals, without a factory's
            // employees. The chats in it are the panes' list too.
            match state.get("terminals").and_then(|t| t.as_array()) {
                Some(t) => {
                    with(|a| {
                        a.sessions = t.iter().filter(|c| s(c, "kind") == "ui").cloned().collect();
                        a.terminals = Some(t.clone());
                    });
                    show_projects();
                    sync_panes();
                }
                None => load_sessions(),
            }
        }
        Event::Chat(m) => on_chat_message(&m),
        Event::Usage(u) => { with(|a| a.usage = Some((u, std::time::Instant::now()))); show_usage(); }
        Event::Inbox(v) => { if let Some(n) = v.get("unread").and_then(|n| n.as_i64()) { if let Some(w) = ui() { w.set_inbox_unread(n as i32); } } }
        Event::ChatSecrets(v) => {
            let sid = s(&v, "sessionId");
            if sid.is_empty() { return; }
            with(|a| { a.grants.insert(sid, v.get("grants").cloned().unwrap_or(Value::Null)); });
            render();
        }
    }
}

// ─── The top bar: usage ───

/// The usage bars (the web UI's renderUsage): per window, how much is used and
/// how much of the window has passed; labels, and how old the numbers are.
pub fn show_usage() {
    let Some((u, at)) = with(|a| a.usage.clone()).flatten() else { return };
    let Some(w) = ui() else { return };
    if !u.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) { return; }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let window = |k: &str, span: f64| -> (f32, f32, String) {
        let used = u.pointer(&format!("/{k}/utilization")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let resets = u.pointer(&format!("/{k}/resetsAt")).and_then(|v| v.as_str()).and_then(parse_iso).unwrap_or(0.0);
        let left = (resets - now).max(0.0);
        let passed = if resets > 0.0 { (100.0 * (1.0 - left / span)).clamp(0.0, 100.0) } else { 0.0 };
        let (d, h, m) = ((left / 86400.0) as u64, ((left % 86400.0) / 3600.0) as u64, ((left % 3600.0) / 60.0) as u64);
        let label = if d > 0 { format!("{d}d {h}h") } else if h > 0 { format!("{h}h {m}m") } else { format!("{m}m") };
        (used.min(100.0) as f32, passed as f32, label)
    };
    let (wu, wt, wl) = window("week", 7.0 * 86400.0);
    let (fu, ft, fl) = window("fiveHour", 5.0 * 3600.0);
    w.set_usage_on(true);
    w.set_wk_used(wu); w.set_wk_time(wt); w.set_wk_left(wl.into());
    w.set_fh_used(fu); w.set_fh_time(ft); w.set_fh_left(fl.into());
    let mins = at.elapsed().as_secs() / 60;
    w.set_usage_age(if mins == 0 { "just now".into() } else if mins < 60 { format!("{mins}m ago").into() } else { format!("{}h ago", mins / 60).into() });
}

/// Seconds since 1970 from an ISO date ("2026-11-19T01:59:59.000Z" or with an offset).
pub(crate) fn parse_iso(s: &str) -> Option<f64> {
    let (date, rest) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, mo, da) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let (time, zone) = match rest.find(['Z', '+', '-']) { Some(i) => (&rest[..i], &rest[i..]), None => (rest, "Z") };
    let mut t = time.split(':');
    let (hh, mm) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let ss: f64 = t.next().unwrap_or("0").parse().ok()?;
    let offset = if zone == "Z" || zone.is_empty() { 0 } else {
        let sign = if zone.starts_with('-') { -1 } else { 1 };
        let z = &zone[1..];
        let (zh, zm) = z.split_once(':').unwrap_or((z.get(..2).unwrap_or("0"), z.get(2..).unwrap_or("0")));
        sign * (zh.parse::<i64>().unwrap_or(0) * 3600 + zm.parse::<i64>().unwrap_or(0) * 60)
    };
    // Days from civil (Howard Hinnant).
    let (y2, m2) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some((days * 86400 + hh * 3600 + mm * 60 - offset) as f64 + ss)
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
    head::chat_state(c)
}

/// A project's dot, by the web UI's priority, over the `state` event's
/// `terminals` (exited chats, parked panes and a factory's employees don't
/// count).
fn project_state(a: &App, alias: &str) -> &'static str {
    head::project_dot(a.terminals.as_ref().unwrap_or(&a.sessions), alias)
}

fn show_projects() {
    with(|a| {
        let rows: Vec<ProjectItem> = a.projects.iter().map(|p| {
            let alias = s(p, "alias");
            ProjectItem {
                initials: head::project_initials(&{ let n = s(p, "name"); if n.is_empty() { alias.clone() } else { n } }).into(),
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
            // An exited chat keeps its pane (grey, "exited") until it is
            // closed, as the web's grid does.
            .filter(|c| s(c, "project") == a.project && s(c, "factory").is_empty())
            .map(|c| s(c, "id")).collect();
        a.mosaic.retain(|k| match chat_of(k) { Some(id) => live.iter().any(|l| l == id), None => true });
        for id in &live { a.mosaic.place(&chat_key(id)); }
        // Chats in panes: a conversation each, subscribed once.
        let mut new = vec![];
        let mut font_new: Vec<String> = vec![];
        for k in a.mosaic.keys() {
            if let Some(id) = chat_of(&k) {
                if !a.chats.contains_key(id) {
                    // A chat with no size yet starts at the default and keeps it.
                    if !a.font.contains_key(id) { a.font.insert(id.to_string(), a.font_default); font_new.push(id.to_string()); }
                    a.chats.insert(id.to_string(), ChatPaneState { conv: Conversation::default(), rows: vec![], model: Rc::new(VecModel::default()), error: String::new() });
                    new.push(id.to_string());
                }
            }
        }
        a.chats.retain(|id, _| live.contains(id));
        if a.full.as_deref().and_then(chat_of).is_some_and(|id| !live.iter().any(|l| l == id)) { a.full = None; }
        if !font_new.is_empty() {
            let (font, def) = (a.font.clone(), a.font_default);
            store::update(|s| { for id in &font_new { s.font.insert(id.clone(), *font.get(id).unwrap_or(&def)); } });
        }
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

/// The layout changed: saved, and drawn again. As the web UI's full
/// re-render, it disarms an armed ×, so a click after it never closes
/// without the warning.
fn layout_changed() {
    with(|a| a.armed = 0);
    save_layout();
    render();
}

/// The grid's sideways scrollbar (ui/theme.slint sp-6).
const SCROLLBAR: f32 = 6.0;

/// The grid's content size: as wide as the panes need (wider than the area,
/// it scrolls sideways), and then 6px less tall for the scrollbar under it.
fn grid(a: &App) -> (f32, f32) {
    let (w, h) = a.area;
    let cw = a.mosaic.content_width(w);
    (cw, if cw > w + 0.5 { h - SCROLLBAR } else { h })
}

/// Lay the panes out and put them on screen (only rows that changed are rewritten).
fn render() {
    let win = ui().map(|w| { let sz = w.window().size(); let k = w.window().scale_factor(); (sz.width as f32 / k, sz.height as f32 / k) }).unwrap_or((1400.0, 860.0));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let out = with(|a| {
        let (cw, h) = grid(a);
        let (leaves, gutters) = a.mosaic.layout(cw, h);
        // Chat composers built now take their drafts back (2.8).
        let chat_keys: Vec<String> = leaves.iter().filter_map(|l| l.key.clone()).filter(|k| chat_of(k).is_some_and(|id| a.chats.contains_key(id))).collect();
        restore_drafts(a, &chat_keys);
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
                pinned: l.pinned,
                armed: l.id == a.armed,
                narrow: head::narrow(l.w),
                drop: a.drag.is_some_and(|d| d.target == l.id) && !l.pinned,
                menu: l.id == a.menu && head::narrow(l.w),
                title_max: 160.0,
                fs: 1.0,
                ..Default::default()
            };
            if let Some(id) = chat_of(&key) {
                let item = a.sessions.iter().find(|c| s(c, "id") == id);
                p.kind = "chat".into();
                let short = item.map(|c| s(c, "title")).filter(|t| !t.is_empty()).unwrap_or_else(|| "New chat".into());
                let long = item.map(|c| s(c, "longTitle")).unwrap_or_default();
                let (shown, cap) = head::title_shown(&short, &long, l.w);
                p.title = shown.into();
                p.title_max = cap;
                p.short_title = short.into();
                // The lock: the server can, and the chat is the owner's own.
                p.secrets = a.secrets_cap && !item.and_then(|c| c.get("collaborator")).is_some_and(truthy);
                let n = head::grant_count(a.grants.get(id), now);
                p.secrets_n = n as i32;
                p.secrets_tip = head::secrets_tip(n).into();
                p.fs = head::font_scale(*a.font.get(id).unwrap_or(&a.font_default));
                if let Some(u) = a.uploads.get(&key) {
                    p.up_on = u.shown;
                    p.up_name = u.name.clone().into();
                    p.up_label = u.label.clone().into();
                    p.up_frac = u.frac;
                    p.up_busy = u.active > 0;
                }
                p.draft = a.drafts.get(&key).cloned().unwrap_or_default().into();
                p.draft_gen = *a.draft_gen.get(&key).unwrap_or(&0);
                if let Some(cs) = a.chats.get(id) {
                    p.entries = cs.model.clone().into();
                    // The header's state is the list's (the `state` event), as
                    // the web's: `exited` wins over the stream's last idle.
                    let st = match item { Some(c) => agent_state(c), None => cs.conv.state.clone() };
                    p.state = head::composer_state(&st, a.sending.contains(id)).into();
                    p.stop_armed = p.state == "working" && a.stop_armed.as_deref() == Some(key.as_str());
                    let mode = if cs.conv.mode.is_empty() { item.map(|c| s(c, "permissionMode")).unwrap_or_default() } else { cs.conv.mode.clone() };
                    let shown = head::shown_mode(&mode, chat_skip(cs, item));
                    p.mode = head::mode_info(shown).0.into();
                    p.mode_key = shown.into();
                    // Plan mode, and no plan of its own yet: the header's PLANNING.
                    p.planning = shown == "plan" && item.is_none_or(|c| s(c, "planStatus").is_empty());
                    p.session = cs.conv.session_id.chars().take(8).collect::<String>().into();
                    p.session_full = cs.conv.session_id.clone().into();
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
            // Only a chat's header folds its tools into "…" (a launcher or a
            // panel has just Pin and ×, shown at any width, as the web's).
            if p.kind != "chat" { p.narrow = false; p.menu = false; }
            p
        }).collect();
        // Stop disarms when its chat stops working (the turn ended, a question).
        if a.stop_armed.as_ref().is_some_and(|k| !panes.iter().any(|p| p.key.as_str() == k && p.state == "working")) { a.stop_armed = None; }
        // The pane in full screen: the whole window.
        let full = a.full.as_ref().and_then(|k| panes.iter().find(|p| p.key.as_str() == k)).map(|p| {
            let mut f = p.clone();
            f.full = true;
            f.narrow = false;
            f.menu = false;
            f.x = 0.0; f.y = 0.0; f.w = win.0; f.h = win.1;
            f.focused = true;
            let (shown, cap) = match chat_of(&p.key).and_then(|id| a.sessions.iter().find(|c| s(c, "id") == id)) {
                Some(c) => head::title_shown(&s(c, "title"), &s(c, "longTitle"), win.0),
                None => (p.title.to_string(), 160.0),
            };
            f.title = shown.into();
            f.title_max = cap;
            f
        });
        // The mode picker: its chat's list and mode; it closes with its pane.
        let picker = a.picker.as_ref().and_then(|k| panes.iter().find(|p| p.key.as_str() == k && p.kind == "chat")).map(|p| {
            let id = chat_of(p.key.as_str()).unwrap_or_default();
            let item = a.sessions.iter().find(|c| s(c, "id") == id);
            let skip = a.chats.get(id).is_some_and(|cs| chat_skip(cs, item));
            (head::modes(skip), p.mode_key.to_string())
        });
        if picker.is_none() { a.picker = None; }
        // The menu closes when its pane goes wide or away.
        if a.menu != 0 && !panes.iter().any(|p| p.id as u32 == a.menu && p.menu) { a.menu = 0; }
        let panes: Vec<PaneView> = panes.into_iter().map(|mut p| { p.full = false; if full.as_ref().is_some_and(|f| f.key == p.key) { p.armed = false; } p }).collect();
        // One row per pane, by key: rows of panes that went are removed, new
        // ones added at the end, the rest rewritten only where they changed.
        let row_key = |p: &PaneView| if p.key.is_empty() { format!("empty:{}", p.id) } else { p.key.to_string() };
        let now: Vec<String> = panes.iter().map(row_key).collect();
        // The keys follow the rows one for one; if they ever part, start over.
        if a.row_keys.len() != a.pane_rows.row_count() { a.row_keys.clear(); a.pane_rows.set_vec(vec![]); }
        let mut i = 0;
        while i < a.row_keys.len() {
            if now.contains(&a.row_keys[i]) { i += 1; } else { a.row_keys.remove(i); a.pane_rows.remove(i); }
        }
        for p in panes {
            let k = row_key(&p);
            match a.row_keys.iter().position(|r| *r == k) {
                Some(i) => { if a.pane_rows.row_data(i).as_ref() != Some(&p) { a.pane_rows.set_row_data(i, p); } }
                None => { a.row_keys.push(k); a.pane_rows.push(p); }
            }
        }
        let gs: Vec<GutterView> = gutters.iter().map(|g| GutterView {
            split: g.split as i32, index: g.index as i32, row: g.dir == Dir::Row,
            x: g.x, y: g.y, w: g.w, h: g.h, span: g.span, locked: g.locked,
        }).collect();
        // Rewritten in place while the count holds: a gutter being dragged
        // keeps its touch area (and so the press, and its release).
        if gs.len() == a.gutter_rows.row_count() {
            for (i, g) in gs.into_iter().enumerate() {
                if a.gutter_rows.row_data(i).as_ref() != Some(&g) { a.gutter_rows.set_row_data(i, g); }
            }
        } else {
            a.gutter_rows.set_vec(gs);
        }
        (cw, full, picker)
    });
    if let (Some(w), Some((cw, full, picker))) = (ui(), out) {
        w.set_content_width(cw);
        w.set_mode_picker_open(picker.is_some());
        if let Some((list, current)) = picker {
            let opts: Vec<ModeOption> = list.iter().map(|m| { let (n, d) = head::mode_info(m); ModeOption { key: (*m).into(), name: n.into(), desc: d.into() } }).collect();
            if w.get_mode_options().row_count() != opts.len() || w.get_mode_options().iter().zip(&opts).any(|(x, y)| x != *y) { w.set_mode_options(ModelRc::new(VecModel::from(opts))); }
            w.set_mode_start(list.iter().position(|m| *m == current).unwrap_or(0) as i32);
            w.set_mode_current(current.into());
        }
        w.set_full_on(full.is_some());
        if let Some(f) = full { if w.get_full_pane() != f { w.set_full_pane(f); } }
    }
}

/// The capability for the secrets lock, then every chat's grants.
fn load_secrets(base: String) {
    bg(move || api::handshake(&base).ok().and_then(|h| h.pointer("/server/capabilities").cloned()), |caps| {
        let on = caps.and_then(|c| c.as_array().map(|c| c.iter().any(|x| x == "chat-secret-preauth"))).unwrap_or(false);
        with(|a| a.secrets_cap = on);
        render();
        if !on { return; }
        call(|api| api.get("/api/secrets/chat"), |r| {
            let Ok(v) = r else { return };
            if let Some(chats) = v.get("chats").and_then(|c| c.as_object()) {
                with(|a| { for (sid, g) in chats { a.grants.insert(sid.clone(), g.clone()); } });
            }
            render();
        });
    });
}

/// A pane's key and its laid-out rectangle, by leaf.
fn leaf_rect(leaf: u32) -> Option<crate::mosaic::LeafRect> {
    with(|a| {
        let cw = a.mosaic.content_width(a.area.0);
        a.mosaic.layout(cw, grid(a).1).0.into_iter().find(|l| l.id == leaf)
    }).flatten()
}

/// The header's ×: the first click arms it ("Sure?", 3 s; one pane at a
/// time), the second closes.
pub fn pane_close(leaf: u32) {
    let armed = with(|a| a.armed == leaf).unwrap_or(false);
    if armed {
        with(|a| a.armed = 0);
        render();
        return close_pane(leaf);
    }
    let gen = with(|a| { a.armed = leaf; a.arm_gen += 1; a.arm_gen }).unwrap_or(0);
    render();
    slint::Timer::single_shot(Duration::from_millis(3000), move || {
        let again = with(|a| if a.armed == leaf && a.arm_gen == gen { a.armed = 0; true } else { false }).unwrap_or(false);
        if again { render(); }
    });
}

/// Full screen on or off for this pane (one at a time).
pub fn pane_full(leaf: u32) {
    let Some(key) = leaf_rect(leaf).and_then(|l| l.key) else { return };
    with(|a| { a.full = if a.full.as_deref() == Some(key.as_str()) { None } else { Some(key) }; a.armed = 0; a.menu = 0; });
    render();
}

/// Focus a pane (any press in it). While one pane is full screen, only it:
/// the panes under it are out of reach.
fn focus_pane(leaf: u32) {
    let changed = with(|a| {
        let covered = a.full.as_ref().is_some_and(|k| a.mosaic.leaf_of(k) != Some(leaf));
        if covered || a.focused == leaf { return false; }
        a.focused = leaf;
        true
    }).unwrap_or(false);
    if changed { render(); }
}

/// Open or close a narrow header's "…" menu. Opening one closes any other,
/// and focuses its pane (as any click in a pane does).
pub fn pane_menu(leaf: u32, open: bool) {
    with(|a| {
        if open { a.menu = leaf; a.focused = leaf; } else if a.menu == leaf { a.menu = 0; }
    });
    render();
}

/// A- / A+ (`d` = -1 / +1), or reset (`d` = 0, back to 13). Every change is
/// also the size new chats start at.
pub fn pane_font(leaf: u32, d: i32) {
    let Some(id) = leaf_rect(leaf).and_then(|l| l.key).and_then(|k| chat_of(&k).map(str::to_string)) else { return };
    let px = with(|a| {
        let now = *a.font.get(&id).unwrap_or(&a.font_default);
        let px = if d == 0 { head::FONT_BASE } else { head::font_step(now, d) };
        a.font.insert(id.clone(), px);
        a.font_default = px;
        px
    }).unwrap_or(head::FONT_BASE);
    store::update(|s| { s.font.insert(id.clone(), px); s.font_default_chat = Some(px); });
    render();
}

/// Pin this pane at its size now, or unpin it.
pub fn pane_pin(leaf: u32) {
    let Some(l) = leaf_rect(leaf) else { return };
    with(|a| a.mosaic.toggle_pin(leaf, l.w, l.h));
    layout_changed();
}

/// Rename a chat: the title changes at once (its long form goes), then the
/// server is told; an unchanged title sends nothing.
pub fn pane_rename(leaf: u32, input: &str) {
    let Some(id) = leaf_rect(leaf).and_then(|l| l.key).and_then(|k| chat_of(&k).map(str::to_string)) else { return };
    let current = with(|a| a.sessions.iter().find(|c| s(c, "id") == id).map(|c| s(c, "title"))).flatten().unwrap_or_default();
    let Some(title) = head::rename_value(input, &current) else { return };
    with(|a| if let Some(c) = a.sessions.iter_mut().find(|c| s(c, "id") == id) { c["title"] = json!(title); c["longTitle"] = json!(""); });
    render();
    call(move |api| api.post(&format!("/api/ui-sessions/{id}/rename"), &json!({ "title": title })), |_| {});
}

/// A toast, as the web UI's: 3.5 s.
pub fn toast(text: &str, error: bool) {
    let Some(w) = ui() else { return };
    w.set_toast(text.into());
    w.set_toast_error(error);
    let gen = with(|a| { a.toast_gen += 1; a.toast_gen }).unwrap_or(0);
    slint::Timer::single_shot(Duration::from_millis(3500), move || {
        if with(|a| a.toast_gen == gen).unwrap_or(false) { if let Some(w) = ui() { w.set_toast("".into()); } }
    });
}

/// Close a pane. A chat's pane closes the chat, as in the web UI: the
/// server first; when it refuses (409: a factory Manager's chat while its run
/// goes) the pane stays and a toast says why.
fn close_pane(leaf: u32) {
    let key = leaf_rect(leaf).and_then(|l| l.key);
    let Some(id) = key.as_deref().and_then(chat_of).map(str::to_string) else {
        animate();
        with(|a| a.mosaic.close(leaf));
        return layout_changed();
    };
    let Some(api) = api() else { return };
    let path = format!("/api/ui-sessions/{id}/close");
    bg(move || api.post(&path, &json!({})), move |r| match head::close_outcome(r) {
        head::Closed::Yes => {
            animate();
            with(|a| {
                if let Some(l) = a.mosaic.leaf_of(&chat_key(&id)) { a.mosaic.close(l); }
                a.font.remove(&id);
                // The draft stays (the web keeps it: a new chat in the project
                // can adopt it); the composer's claim goes when its pane does.
                if a.full.as_deref() == Some(chat_key(&id).as_str()) { a.full = None; }
                a.sessions.retain(|c| s(c, "id") != id);
            });
            store::update(|s| { s.font.remove(&id); });
            layout_changed();
            load_sessions();
        }
        head::Closed::SignedOut => sign_out("Your sign-in ended. Sign in again."),
        // Refused or failed: the pane stays, and a toast says why.
        head::Closed::No(why) => toast(&why, true),
    });
}

fn add_pane(kind: &str) {
    animate();
    if kind == "fit" { return fit(); }
    with(|a| {
        if let Some(preset) = kind.strip_prefix("layout:") {
            // The panes in display order: the chats in the server's order.
            let order: Vec<String> = a.sessions.iter().filter(|c| s(c, "project") == a.project).map(|c| chat_key(&s(c, "id"))).collect();
            // A preset makes new leaves: the focus stays with its pane.
            let cw = a.mosaic.content_width(a.area.0);
            let key = a.mosaic.layout(cw, a.area.1).0.into_iter().find(|l| l.id == a.focused).and_then(|l| l.key);
            a.mosaic = a.mosaic.preset_from(preset, &order);
            if let Some(l) = key.and_then(|k| a.mosaic.leaf_of(&k)) { a.focused = l; }
            return;
        }
        let key = if kind == "terminal" { a.pending += 1; format!("pend:{}", a.pending) } else { format!("panel:{kind}") };
        a.focused = a.mosaic.place(&key);
    });
    layout_changed();
}

/// Ease the panes to their new places for the next 220ms (a change of
/// structure or shares: not a gutter drag, not a window resize).
pub fn animate() {
    let gen = with(|a| { a.motion_gen += 1; a.motion_gen }).unwrap_or(0);
    if let Some(w) = ui() { w.set_wb_motion(true); }
    slint::Timer::single_shot(Duration::from_millis(260), move || {
        if with(|a| a.motion_gen == gen).unwrap_or(false) { if let Some(w) = ui() { w.set_wb_motion(false); } }
    });
}

/// Fit to screen: every split even, the grid back at the left; if it still
/// scrolls sideways after 60ms, an error toast says why.
fn fit() {
    with(|a| a.mosaic.fit());
    layout_changed();
    if let Some(w) = ui() { w.set_wb_scroll_home(w.get_wb_scroll_home() + 1); }
    slint::Timer::single_shot(Duration::from_millis(60), || {
        let over = with(|a| (a.mosaic.content_width(a.area.0) > a.area.0 + 2.0, a.mosaic.has_pin())).unwrap_or((false, false));
        if over.0 {
            toast(if over.1 { "Does not fit: the pinned panes plus the others at their minimum width are wider than the window" }
                else { "Does not fit: too many panes side by side for this window, even at their minimum width" }, true);
        }
    });
}

/// An empty pane's chip: Terminal puts a launcher in that pane (the web
/// puts it in the first empty one: DESIGN rule 5, PLAN.md). The chat it
/// starts then takes the same pane (launch_chat).
fn chip(leaf: u32, kind: &str) {
    if kind != "terminal" { return; }
    animate();
    with(|a| { a.pending += 1; let k = format!("pend:{}", a.pending); a.mosaic.assign(leaf, Some(k)); a.focused = leaf; });
    layout_changed();
}

/// The leaf at a point of the grid's content (none on a gutter or outside).
fn leaf_at(a: &App, x: f32, y: f32) -> Option<crate::mosaic::LeafRect> {
    let cw = a.mosaic.content_width(a.area.0);
    a.mosaic.layout(cw, grid(a).1).0.into_iter().find(|l| x >= l.x && x < l.x + l.w && y >= l.y && y < l.y + l.h)
}

/// A pane dragged by its header (0 start, 1 move, 2 release), at a point of
/// the grid's content. The leaf under the pointer lights up unless pinned;
/// the release swaps the two panes, or moves the pane into an empty one. A
/// pinned pane can be dragged but is refused on release.
pub fn pane_drag(leaf: u32, phase: i32, x: f32, y: f32) {
    match phase {
        0 => {
            let Some(l) = leaf_rect(leaf) else { return };
            let view = with(|a| { a.menu = 0; a.pane_rows.iter().find(|p| p.id as u32 == leaf) }).flatten();
            with(|a| a.drag = Some(Drag { src: leaf, target: leaf }));
            if let (Some(w), Some(v)) = (ui(), view) {
                w.set_drag_pane(v);
                w.set_drag_dx(x - l.x);
                w.set_drag_dy(y - l.y);
                w.set_drag_on(true);
            }
            render();
        }
        1 => {
            let changed = with(|a| {
                let t = leaf_at(a, x, y).map(|l| l.id).unwrap_or(0);
                match &mut a.drag { Some(d) if d.target != t => { d.target = t; true } _ => false }
            }).unwrap_or(false);
            if changed { render(); }
        }
        _ => {
            let Some(d) = with(|a| a.drag.take()).flatten() else { return };
            if let Some(w) = ui() { w.set_drag_on(false); }
            let target = with(|a| leaf_at(a, x, y).map(|l| l.id)).flatten();
            let result = with(|a| target.map(|t| a.mosaic.drop_on(d.src, t)).unwrap_or(crate::mosaic::Dropped::Nothing)).unwrap_or(crate::mosaic::Dropped::Nothing);
            match result {
                crate::mosaic::Dropped::Pinned => { toast("This pane is pinned. Unpin it to move it.", false); render(); }
                crate::mosaic::Dropped::Nothing => render(),
                // The pane that was pressed keeps the focus, in its new leaf.
                _ => { animate(); with(|a| if let Some(t) = target { a.focused = t; }); layout_changed(); }
            }
        }
    }
}

/// Esc while dragging: put it down where it was, nothing changed.
pub fn drag_cancel() {
    if with(|a| a.drag.take().is_some()).unwrap_or(false) {
        if let Some(w) = ui() { w.set_drag_on(false); }
        render();
    }
}

/// UI Mode in a launcher: a new chat in this project, in that pane.
fn launch_chat(leaf: u32, skip: bool) {
    let project = with(|a| a.project.clone()).unwrap_or_default();
    if project.is_empty() { return; }
    call(move |api| api.post("/api/ui-sessions/create", &json!({ "project": project, "skipPermissions": skip })), move |r| match r {
        Ok(v) => {
            let id = s(&v, "id");
            if id.is_empty() { return; }
            // The chat takes the launcher's leaf (its size and pin); if the
            // chat list already placed it elsewhere, that leaf goes.
            animate();
            with(|a| {
                let key = chat_key(&id);
                if let Some(other) = a.mosaic.leaf_of(&key) { if other != leaf { a.mosaic.close(other); } }
                a.mosaic.assign(leaf, Some(key));
                a.focused = leaf;
            });
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
            "ui-history" => { if let Some(sess) = m.get("session") { cs.conv.load(sess); } a.sending.remove(&id); true }
            "ui-event" => {
                // The server's own state replaces the optimistic one.
                let ty = m.get("event").map(|e| s(e, "type")).unwrap_or_default();
                if matches!(ty.as_str(), "state" | "exit" | "history") { a.sending.remove(&id); }
                m.get("event").map(|e| cs.conv.apply(e)).unwrap_or(false) || ty == "exit"
            }
            _ => false,
        }
    }).unwrap_or(false);
    if changed {
        show_chat(&id);
        render();
    }
}

/// Claude's text as rows: markdown blocks (paragraphs, list items, headings)
/// and fenced code blocks, so each gets the web chat's spacing.
fn split_blocks(id: &str, text: &str) -> Vec<Row> {
    let mut out: Vec<Row> = vec![];
    let mut para = String::new();
    let mut code: Option<String> = None;
    let n = |out: &Vec<Row>| format!("{id}#{}", out.len());
    let flush = |para: &mut String, out: &mut Vec<Row>| {
        if !para.trim().is_empty() { out.push(Row::Prose(n(out), para.trim_end().to_string(), "p", String::new(), false)); }
        para.clear();
    };
    for line in text.split('\n') {
        let t = line.trim_start();
        if t.starts_with("```") {
            match code.take() {
                Some(c) => out.push(Row::Code(n(&out), c.trim_end().to_string(), false)),
                None => { flush(&mut para, &mut out); code = Some(String::new()); }
            }
            continue;
        }
        if let Some(c) = &mut code { c.push_str(line); c.push('\n'); continue; }
        if t.is_empty() { flush(&mut para, &mut out); continue; }
        // A list item: "- ", "* ", "+ ", or "1. ".
        let bullet = ["- ", "* ", "+ "].iter().find_map(|b| t.strip_prefix(b)).map(|r| ("•".to_string(), r));
        let numbered = t.split_once(". ").filter(|(d, _)| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit())).map(|(d, r)| (format!("{d}."), r));
        if let Some((marker, rest)) = bullet.or(numbered) {
            flush(&mut para, &mut out);
            out.push(Row::Prose(n(&out), rest.to_string(), "li", marker, false));
            continue;
        }
        if let Some(h) = t.strip_prefix("#") {
            flush(&mut para, &mut out);
            out.push(Row::Prose(n(&out), format!("**{}**", h.trim_start_matches('#').trim()), "h", String::new(), false));
            continue;
        }
        para.push_str(line);
        para.push('\n');
    }
    if let Some(c) = code { out.push(Row::Code(n(&out), c, false)); }
    flush(&mut para, &mut out);
    match out.last_mut() {
        Some(Row::Prose(_, _, _, _, last)) | Some(Row::Code(_, _, last)) => *last = true,
        _ => {}
    }
    out
}

/// The log's rows: tool calls on their own, or folded into the batch Claude
/// Code summed up; Claude's text in blocks.
fn rows_of(shown: &[Shown]) -> Vec<Row> {
    let mut rows = vec![];
    let mut i = 0;
    while i < shown.len() {
        let e = &shown[i];
        if e.kind == "tool" && !e.group.is_empty() && !e.group_summary.is_empty() {
            let mut batch = vec![e.clone()];
            while i + 1 < shown.len() && shown[i + 1].kind == "tool" && shown[i + 1].group == e.group { i += 1; batch.push(shown[i].clone()); }
            rows.push(Row::Tools(e.id.clone(), e.group_summary.clone(), batch));
        } else if e.kind == "tool" {
            rows.push(Row::Tool(e.id.clone(), e.clone()));
        } else if e.kind == "assistant" {
            rows.extend(split_blocks(&e.id, &e.text));
        } else {
            rows.push(Row::Entry(e.clone()));
        }
        i += 1;
    }
    rows
}

/// Inline code gets the web chat's 5px padding inside its chip (thin spaces:
/// StyledText draws the chip around the text only).
fn pad_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut inside = false;
    for c in text.chars() {
        if c == '`' {
            if inside { out.push('\u{2009}'); out.push('`'); } else { out.push('`'); out.push('\u{2009}'); }
            inside = !inside;
        } else {
            out.push(c);
        }
    }
    out
}

fn md(text: &str) -> StyledText {
    let text = &pad_code(text);
    StyledText::from_markdown(text).unwrap_or_else(|_| StyledText::from_markdown(&text.replace(['<', '>', '*', '_', '`', '[', ']'], "")).unwrap_or_default())
}

fn tool_view(t: &Shown) -> ToolView {
    ToolView { title: t.title.clone().into(), detail: t.detail.clone().into(), text: t.text.clone().into(), status: t.status.clone().into(), error: t.error }
}

fn to_view(r: &Row) -> EntryView {
    match r {
        Row::Prose(id, t, kind, marker, last) => EntryView {
            id: id.clone().into(), kind: "assistant".into(), text: t.clone().into(), md: md(t),
            status: (*kind).into(), title: marker.clone().into(),
            // Paragraphs 8px apart, list items 4px (their 2px margins), the message 10px.
            gap: if *last { 10.0 } else if *kind == "li" { 2.0 } else { 8.0 },
            ..Default::default()
        },
        Row::Code(id, t, last) => EntryView { id: id.clone().into(), kind: "code".into(), text: t.clone().into(), gap: if *last { 10.0 } else { 8.0 }, ..Default::default() },
        Row::Tool(id, t) => EntryView {
            id: id.clone().into(), kind: "tool".into(), status: t.status.clone().into(), error: t.error,
            tools: ModelRc::new(VecModel::from(vec![tool_view(t)])), gap: 10.0, ..Default::default()
        },
        Row::Tools(id, summary, tools) => EntryView {
            id: id.clone().into(),
            kind: "tools".into(),
            title: summary.clone().into(),
            detail: format!("{} tool{}", tools.len(), if tools.len() == 1 { "" } else { "s" }).into(),
            status: if tools.iter().any(|t| t.status == "running") { "running".into() } else { "done".into() },
            error: tools.iter().any(|t| t.error),
            tools: ModelRc::new(VecModel::from(tools.iter().map(tool_view).collect::<Vec<_>>())),
            gap: 10.0,
            ..Default::default()
        },
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
            gap: 10.0,
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

/// Send (or queue) what the box holds: a blank box does nothing. The box
/// and the draft are cleared, and an idle chat shows working at once (the
/// web's optimistic switch). A refused send puts an error in the log, the
/// text back in the box, and the chat back to its real state (the web stays
/// "working…" and loses the text: a deliberate difference, PLAN.md).
pub fn send(key: &str, text: String) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let text = text.trim_end().to_string();
    if text.trim().is_empty() { return; }
    let k = key.to_string();
    with(|a| {
        save_draft_in(a, &k, "");
        *a.draft_gen.entry(k.clone()).or_insert(0) += 1;
        if let Some(cs) = a.chats.get_mut(&id) { cs.error.clear(); }
        a.sending.insert(id.clone());
    });
    render();
    write_drafts_soon();
    let (id2, body) = (id.clone(), text.clone());
    call(move |api| api.post(&format!("/api/ui-sessions/{id}/send"), &json!({ "text": body })), move |r| {
        let refused = match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(s(&v, "error")),
            Ok(_) => None,
            Err(e) => Some(e),
        };
        if let Some(e) = refused { send_refused(&k, &id2, if e.is_empty() { "Failed to send".into() } else { e }, text); }
    });
}

fn send_refused(key: &str, id: &str, error: String, text: String) {
    with(|a| {
        a.sending.remove(id);
        // The text comes back, unless something new was typed meanwhile.
        if a.drafts.get(key).is_none_or(|d| d.is_empty()) {
            save_draft_in(a, key, &text);
            *a.draft_gen.entry(key.to_string()).or_insert(0) += 1;
        }
        a.local_errors += 1;
        let n = a.local_errors;
        if let Some(cs) = a.chats.get_mut(id) { cs.conv.entries.push(json!({ "id": format!("local-error-{n}"), "kind": "error", "text": error })); }
    });
    show_chat(id);
    render();
    write_drafts_soon();
}

/// Stop: the first click arms it ("Sure?") for 3 s; a second click within
/// them interrupts the turn (the server then reports idle).
pub fn stop(key: &str) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let armed = with(|a| a.stop_armed.as_deref() == Some(key)).unwrap_or(false);
    if !armed {
        let gen = with(|a| { a.stop_armed = Some(key.to_string()); a.stop_gen += 1; a.stop_gen }).unwrap_or(0);
        render();
        slint::Timer::single_shot(Duration::from_secs(3), move || {
            if with(|a| a.stop_gen == gen && a.stop_armed.take().is_some()).unwrap_or(false) { render(); }
        });
        return;
    }
    with(|a| a.stop_armed = None);
    render();
    call(move |api| api.post(&format!("/api/ui-sessions/{id}/interrupt"), &json!({})), |_| {});
}

/// The Enter mode, for every chat, saved.
fn set_chat_enter(ctrl: bool) {
    if let Some(w) = ui() { w.global::<Composer>().set_ctrl_enter(ctrl); }
    store::update(|s| s.chat_enter = if ctrl { "newline".into() } else { String::new() });
}

// ─── Drafts (2.8, src/drafts.rs) ───

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The drafts of the server signed in to, loaded (and pruned) when it
/// changes.
fn drafts_for_server(a: &mut App) {
    let server = a.api.as_ref().map(|x| x.base().to_string()).unwrap_or_default();
    if server == a.drafts_server { return; }
    a.drafts_server = server.clone();
    a.saved_drafts = Drafts::from_saved(store::load().drafts.get(&server).unwrap_or(&Value::Null));
    a.saved_drafts.prune(now_ms());
    a.restored.clear();
}

/// A chat's project alias.
fn chat_project(a: &App, id: &str) -> String {
    a.sessions.iter().find(|c| s(c, "id") == id).map(|c| s(c, "project")).unwrap_or_else(|| a.project.clone())
}

/// The box's text changed: the pane's draft, and the saved one (own, and
/// the project's copy by the claim rules).
fn save_draft_in(a: &mut App, key: &str, text: &str) {
    a.drafts.insert(key.to_string(), text.to_string());
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    drafts_for_server(a);
    let alias = chat_project(a, &id);
    a.saved_drafts.save(&id, &alias, text, now_ms());
    a.drafts_dirty = true;
}

fn set_draft(key: &str, text: &str) {
    let changed = with(|a| { if a.drafts.get(key).map(String::as_str) == Some(text) { return false; } save_draft_in(a, key, text); true }).unwrap_or(false);
    if changed { write_drafts_soon(); }
}

/// The drafts go to disk at most every menu-linger while typing (and at
/// quit: [flush_drafts]).
fn write_drafts_soon() {
    slint::Timer::single_shot(Duration::from_millis(350), flush_drafts);
}

pub fn flush_drafts() {
    let out = with(|a| {
        if !a.drafts_dirty || a.drafts_server.is_empty() { return None; }
        a.drafts_dirty = false;
        Some((a.drafts_server.clone(), a.saved_drafts.to_saved()))
    }).flatten();
    if let Some((server, v)) = out { store::update(|s| { s.drafts.insert(server, v); }); }
}

/// The chat panes on screen now: a newly built composer takes its draft
/// back (own, else the project's unclaimed copy); one that went lets its
/// claim go.
fn restore_drafts(a: &mut App, keys: &[String]) {
    if a.api.is_none() { return; }
    drafts_for_server(a);
    let now = now_ms();
    for k in keys {
        if a.restored.contains_key(k) { continue; }
        let Some(id) = chat_of(k).map(str::to_string) else { continue };
        let alias = chat_project(a, &id);
        a.restored.insert(k.clone(), alias.clone());
        let found = a.saved_drafts.restore(&id, &alias, now);
        a.drafts_dirty = true;
        if let Some(t) = found {
            if a.drafts.get(k).is_none_or(|d| d.is_empty()) {
                a.drafts.insert(k.clone(), t);
                *a.draft_gen.entry(k.clone()).or_insert(0) += 1;
            }
        }
    }
    let gone: Vec<(String, String)> = a.restored.iter().filter(|(k, _)| !keys.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect();
    for (k, alias) in gone {
        a.restored.remove(&k);
        if let Some(id) = chat_of(&k) { a.saved_drafts.release(id, &alias); }
    }
}

// ─── Attachments (2.7, src/attach.rs) ───

/// The attach button: the system's file picker (one file, any type), off the
/// UI thread; the picked file always uploads.
fn pick_file(key: String) {
    if with(|a| a.uploads.get(&key).is_some_and(|u| u.active > 0)).unwrap_or(false) { return; }
    std::thread::spawn(move || {
        let Some(path) = rfd::FileDialog::new().pick_file() else { return };
        let _ = slint::invoke_from_event_loop(move || upload_file(&key, &path));
    });
}

/// Upload a file from disk to a chat's project.
pub fn upload_file(key: &str, path: &std::path::Path) {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    match std::fs::read(path) {
        Ok(bytes) => upload(key, name, bytes),
        Err(e) => toast(&format!("Upload failed: {e}"), true),
    }
}

/// Upload bytes as `name`: the bar, the upload off the UI thread, then the
/// path at the caret and a toast, or the error's toast.
pub fn upload(key: &str, name: String, bytes: Vec<u8>) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let project = with(|a| chat_project(a, &id)).unwrap_or_default();
    if project.is_empty() { return toast("No project for this chat", true); }
    let Some(api) = api() else { return };
    with(|a| {
        let u = a.uploads.entry(key.to_string()).or_default();
        u.active += 1;
        u.shown = true;
        u.name = name.clone();
        u.label = "0%".into();
        u.frac = 0.0;
    });
    render();
    let k = key.to_string();
    bg(move || {
        use base64::Engine;
        let body = serde_json::to_vec(&json!({ "project": project, "name": name, "data": base64::engine::general_purpose::STANDARD.encode(&bytes) })).unwrap_or_default();
        // Tell the bar as the body goes out (only when the number changes).
        let last = std::sync::atomic::AtomicU64::new(u64::MAX);
        let (pk, pname) = (k.clone(), name.clone());
        let progress = move |sent: u64, total: u64| {
            let pct = if total == 0 { 0 } else if sent >= total { 101 } else { sent * 100 / total };
            if last.swap(pct, Ordering::Relaxed) == pct { return; }
            let (k, n) = (pk.clone(), pname.clone());
            let _ = slint::invoke_from_event_loop(move || upload_progress(&k, &n, sent, total));
        };
        let r = api.post_with_progress("/api/attachments/upload", &body, &progress);
        (k, name, r)
    }, |(k, name, r)| {
        let answer = match r {
            Err(ApiError::SignedOut) => return sign_out("Your sign-in ended. Sign in again."),
            Err(_) => attach::upload_answer(0, "", &name),
            Ok((status, body)) => attach::upload_answer(status, &body, &name),
        };
        upload_done(&k, answer);
    });
}

fn upload_progress(key: &str, name: &str, sent: u64, total: u64) {
    with(|a| if let Some(u) = a.uploads.get_mut(key) {
        u.name = name.to_string();
        u.label = attach::progress_label(sent, total);
        u.frac = if total == 0 { 0.0 } else { (sent as f64 / total as f64) as f32 };
    });
    render();
}

fn upload_done(key: &str, answer: Result<(String, String), String>) {
    let gen = with(|a| {
        let u = a.uploads.entry(key.to_string()).or_default();
        u.active = u.active.saturating_sub(1);
        u.hide_gen += 1;
        (u.active, u.hide_gen)
    });
    match answer {
        Ok((path, name)) => { insert_text(key, &path); toast(&format!("Attached: {name}"), false); }
        Err(e) => toast(&e, true),
    }
    // The bar stays a moment after the chat's last upload, then goes.
    if let Some((0, gen)) = gen {
        let k = key.to_string();
        slint::Timer::single_shot(Duration::from_millis(350), move || {
            if with(|a| a.uploads.get(&k).is_some_and(|u| u.active == 0 && u.hide_gen == gen)).unwrap_or(false) {
                with(|a| a.uploads.remove(&k));
                render();
            }
        });
    }
    render();
}

/// Put text (a path) into a chat's box at its caret, replacing a selection,
/// spaced as the web's; the box takes the focus with the caret after it.
pub fn insert_text(key: &str, path: &str) {
    let done = with(|a| {
        if !a.chats.contains_key(chat_of(key)?) { return None; }
        let text = a.drafts.get(key).cloned().unwrap_or_default();
        let (c, an) = a.carets.get(key).copied().unwrap_or((text.len(), text.len()));
        let (new, caret) = attach::insert_path(&text, c, an, path);
        save_draft_in(a, key, &new);
        a.carets.insert(key.to_string(), (caret, caret));
        Some((new, caret))
    }).flatten();
    let Some((new, caret)) = done else { return };
    if let Some(w) = ui() {
        let c = w.global::<Composer>();
        c.set_insert_key(key.into());
        c.set_insert_text(new.into());
        c.set_insert_caret(caret as i32);
        c.set_insert_gen(c.get_insert_gen() + 1);
    }
    render();
    write_drafts_soon();
}

/// Files dropped on a chat: on this machine, their own paths go in (one
/// insertion, a toast); otherwise each uploads. `upload` forces the upload
/// (the test driver's remote).
pub fn drop_files(key: &str, paths: Vec<std::path::PathBuf>, upload_anyway: bool) {
    if paths.is_empty() || chat_of(key).is_none() { return; }
    let local = with(|a| a.api.as_ref().is_some_and(|x| attach::is_local(x.base()))).unwrap_or(false);
    if local && !upload_anyway {
        let joined = paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(" ");
        insert_text(key, &joined);
        toast(&attach::added_toast(paths.len()), false);
    } else {
        for p in paths { upload_file(key, &p); }
    }
}

/// The system started or stopped dragging files over the window.
pub fn file_hover(on: bool) {
    if let Some(w) = ui() { w.global::<crate::Pointer>().set_file_drag(on); }
    if !on { with(|a| a.file_target = None); }
}

/// One dropped file (the system sends one event each): those that arrive
/// together are one drop, on the chat the drag was over (else the focused
/// chat; full screen, its chat).
pub fn file_dropped(path: std::path::PathBuf) {
    let first = with(|a| { a.dropped.push(path); a.dropped.len() == 1 }).unwrap_or(false);
    if !first { return; }
    slint::Timer::single_shot(Duration::from_millis(0), || {
        let (paths, key) = with(|a| {
            let key = a.full.clone().or_else(|| a.file_target.clone()).unwrap_or_else(|| chat_key(&focused_chat()));
            (std::mem::take(&mut a.dropped), key)
        }).unwrap_or_default();
        file_hover(false);
        drop_files(&key, paths, false);
    });
}

/// Ctrl+V in a chat's box: a picture (as image.png) or a copied file
/// uploads, and the key is taken; anything else is the ordinary paste.
fn paste(key: &str) -> bool {
    let Ok(mut cb) = arboard::Clipboard::new() else { return false };
    if let Ok(files) = cb.get().file_list() {
        if let Some(f) = files.into_iter().next() { upload_file(key, &f); return true; }
    }
    if let Ok(img) = cb.get_image() {
        if let Some(png) = encode_png(img.width as u32, img.height as u32, &img.bytes) {
            upload(key, "image.png".into(), png);
            return true;
        }
    }
    false
}

/// RGBA pixels as a PNG file.
fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut wr = e.write_header().ok()?;
        wr.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// The test driver puts a pane's caret (and selection) somewhere.
pub fn set_caret(key: &str, at: usize, to: usize) {
    let text = with(|a| { a.carets.insert(key.to_string(), (at, to)); a.drafts.get(key).cloned().unwrap_or_default() }).unwrap_or_default();
    if let Some(w) = ui() {
        let c = w.global::<Composer>();
        c.set_insert_key(key.into());
        c.set_insert_text(text.into());
        c.set_insert_caret(at as i32);
        c.set_insert_gen(c.get_insert_gen() + 1);
    }
}

/// A pane's upload bar, for the test driver: (shown, name, label, busy).
pub fn upload_of(key: &str) -> (bool, String, String, bool) {
    with(|a| a.uploads.get(key).map(|u| (u.shown, u.name.clone(), u.label.clone(), u.active > 0))).flatten().unwrap_or_default()
}

/// The saved drafts of the server signed in to, for the test driver.
pub fn saved_drafts() -> Value {
    with(|a| { drafts_for_server(a); a.saved_drafts.to_saved() }).unwrap_or(Value::Null)
}

/// A pane's draft as typed (the pane rows carry it only as of their last
/// render; typing doesn't re-render).
pub fn draft_of(key: &str) -> String {
    with(|a| a.drafts.get(key).cloned().unwrap_or_default()).unwrap_or_default()
}

/// A chat launched with skip permissions (Bypass all in its list): its
/// snapshot says, or the list's entry.
fn chat_skip(cs: &ChatPaneState, item: Option<&Value>) -> bool {
    cs.conv.skip || item.and_then(|c| c.get("skipPermissions")).is_some_and(truthy)
}

/// The mode button: the picker for that pane's chat (one at a time).
fn open_mode_picker(key: &str) {
    with(|a| a.picker = Some(key.to_string()));
    render();
}

fn close_mode_picker() {
    with(|a| a.picker = None);
    render();
}

/// A mode chosen in the picker: it closes; another mode shows at once and
/// goes to the server. Refused or failed, the button goes back and a toast
/// says why (the web keeps showing the mode it asked for: a deliberate
/// difference, PLAN.md).
fn choose_mode(mode: &str) {
    let Some(key) = with(|a| a.picker.take()).flatten() else { return };
    let Some(id) = chat_of(&key).map(str::to_string) else { render(); return };
    let mode = mode.to_string();
    let old = with(|a| {
        let item = a.sessions.iter().find(|c| s(c, "id") == id).cloned();
        let cs = a.chats.get_mut(&id)?;
        let now = if cs.conv.mode.is_empty() { item.as_ref().map(|c| s(c, "permissionMode")).unwrap_or_default() } else { cs.conv.mode.clone() };
        let now = head::shown_mode(&now, chat_skip(cs, item.as_ref())).to_string();
        if now == mode { return None; }
        cs.conv.mode = mode.clone();
        Some(now)
    }).flatten();
    render();
    let Some(old) = old else { return };
    let (id2, m2) = (id.clone(), mode.clone());
    call(move |api| api.post(&format!("/api/ui-sessions/{id}/permission-mode"), &json!({ "mode": mode })), move |r| {
        let err = match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(s(&v, "error")),
            Ok(_) => None,
            Err(e) => Some(e),
        };
        let Some(e) = err else { return };
        // Back to the mode it had, unless something changed it meanwhile.
        with(|a| if let Some(cs) = a.chats.get_mut(&id2) { if cs.conv.mode == m2 { cs.conv.mode = old; } });
        render();
        toast(&head::mode_error(&e), true);
    });
}

/// Ask a pane's composer to take the focus (the box).
pub fn focus_composer(key: &str) {
    if let Some(w) = ui() {
        let c = w.global::<Composer>();
        c.set_focus_key(key.into());
        c.set_focus_gen(c.get_focus_gen() + 1);
    }
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
