//! The app: signing in, the session, projects, and the workbench with its
//! panes (src/mosaic.rs lays them out). Every chat in a pane has its own
//! conversation, kept live over /ws. Runs on the UI thread; anything that talks
//! to the server runs on a worker thread ([bg]) and comes back here.

use crate::{
    api::{self, Api, ApiError, SignedIn},
    attach, drafts::Drafts,
    chat::{self, Conversation, Handed, Shown},
    live::{self, Event, Live},
    mosaic::{Dir, Mosaic},
    agents, head, media, questions, sched, secrets, slash, sreq, store, think, tools, Composer, QAct, QOpt, QView, EntryView, GutterView, MainWindow, ModeOption, PaneView, ProjectItem, SchedItem, SecGrant, SecPick, SlashItem, SreqCard, ToolView, ToolAct, DiffLine, TodoItem, MediaItem, DlRow, AnsBlock, AgentRow,
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

/// The chat secrets dialog (2.12): for which chat, the secrets to pick, what
/// is ticked and typed (dropped on close), the duration, the state.
struct SecDlg {
    chat: String,
    /// (id, name, lock); None while loading.
    secrets: Option<Vec<(String, String, String)>>,
    chosen: Vec<String>,
    pins: HashMap<String, String>,
    bad: String,
    duration: String,
    busy: bool,
    err: String,
    revoking: HashSet<String>,
    unsupported: bool,
    pin_gen: i32,
}

/// A chat's schedule panel (2.10), by pane key: the form, the list.
struct Sched {
    open: bool,
    text: String,
    text_gen: i32,
    trigger: String,
    at: String,
    at_gen: i32,
    note: String,
    items: Rc<VecModel<SchedItem>>,
    /// The limit warmer is on (Limit resets can be chosen); None: not known yet.
    warm: Option<bool>,
    /// The list's own line: "" (items or none to say), loading, an error.
    list: String,
    /// The server has no schedule (an older one).
    unsupported: bool,
    /// The list has answered (nothing shows until it does, as the web).
    loaded: bool,
}

struct ChatPaneState {
    conv: Conversation,
    rows: Vec<Row>,
    model: Rc<VecModel<EntryView>>,
    error: String,
    /// Tool cards and groups open (2.14): entry ids, and "g:<group>". Kept
    /// while the pane shows the chat; an open card stays open when its
    /// result arrives (the web's rebuild closes it: a named difference).
    open: HashSet<String>,
    /// Thinking blocks open or folded (2.15, src/think.rs).
    think: think::Think,
    /// Counts what arrived from the server (the log follows the bottom on it).
    gen: i32,
    /// Side questions asked in this chat (2.21): local, in the log where asked.
    asides: Vec<Aside>,
    /// The agent dock's sheet (2.18): a list (commands: true) or one record's
    /// panel (its toolUseId, opened from the list?), and the panel's rows.
    sheet_list: Option<bool>,
    /// The plan panel open (2.19).
    plan_open: bool,
    sheet_panel: Option<(String, bool)>,
    panel_model: Rc<VecModel<EntryView>>,
}

/// A side question (/btw): its id, the entry it follows, the question, and
/// pending | done | error with the answer or the error.
#[derive(Clone, Debug, PartialEq)]
pub struct Aside { id: u32, anchor: String, question: String, state: String, text: String }

/// What one row of a chat log shows (compared to rewrite only changed rows).
#[derive(Clone, PartialEq, Debug)]
enum Row {
    Entry(Shown),
    /// Claude's text in blocks: a paragraph, a list item (with its marker) or a
    /// heading, as markdown; or a fenced code block. The last flag: the
    /// message's last block (the gap after it is an entry's, 10px).
    Prose(String, String, &'static str, String, bool),
    Code(String, String, bool),
    /// Tool calls (2.14): one card (open?); a batch Claude Code summed up
    /// (its head's summary, the members with their open state, the group
    /// open?); a call that started a sub-agent (its agent record).
    Tool(String, Shown, bool),
    /// A thinking block (2.15), open?
    Think(Shown, bool),
    /// A side question's card (2.21).
    Aside(Aside),
    Tools(String, String, Vec<(Shown, bool)>, bool),
    Agent(String, Value),
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
    /// Lines sent while a chat needs input (2.9): held here, by chat id, and
    /// sent as one message when the prompt is answered or the turn ends.
    held: HashMap<String, Vec<String>>,
    flush_pending: bool,
    /// A box's text set by code, to show (src set_box / show_box).
    box_set: Option<(String, String, usize, bool)>,
    /// Each chat box's slash menu (2.11), by pane key: its items, the
    /// selection; none while closed.
    slash: HashMap<String, (Rc<VecModel<SlashItem>>, usize)>,
    /// The chat secrets dialog, open (2.12).
    sec: Option<SecDlg>,
    /// Secret requests waiting on the owner (2.12b).
    sreq: Sreq,
    /// The question cards (2.13), by "<chat>:<entry>": the picks and the
    /// card's models (kept, so redraws leave its fields alone).
    qcards: HashMap<String, QCard>,
    /// The last link a reply opened (the test driver reads it).
    last_opened: String,
    /// Media Claude showed (2.16m), by "<chat>|<stored name>": loading,
    /// ready (with the picture), gone, empty (would not decode), fetching
    /// (a video or audio on its way to the system player).
    media: HashMap<String, (String, Option<slint::Image>)>,
    /// The picture viewer: the chat's pictures' keys, and which is shown.
    viewer: Option<(Vec<String>, usize)>,
    /// Downloads (the window's download rows), newest last.
    dls: Vec<Dl>,
    dl_rows: Rc<VecModel<DlRow>>,
    dl_seq: u32,
    /// Each chat's schedule panel (2.10), by pane key.
    sched: HashMap<String, Sched>,
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
    drafts_timer: bool,
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
            sched: HashMap::new(),
            sec: None,
            sreq: Sreq::default(),
            qcards: HashMap::new(),
            last_opened: String::new(),
            media: HashMap::new(),
            viewer: None,
            dls: vec![],
            dl_rows: Rc::new(VecModel::default()),
            dl_seq: 0,
            slash: HashMap::new(),
            held: HashMap::new(),
            flush_pending: false,
            box_set: None,
            uploads: HashMap::new(),
            carets: HashMap::new(),
            file_target: None,
            dropped: vec![],
            saved_drafts: Drafts::default(),
            drafts_server: String::new(),
            restored: HashMap::new(),
            drafts_dirty: false,
            drafts_timer: false,
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
    // Tool cards (2.14): a head opens or closes its card, or its group.
    window.global::<ToolAct>().on_toggle(|c, id| tool_toggle(c.as_str(), id.as_str()));
    // A link in a reply: the system browser. Copy: the clipboard, "Copied".
    window.global::<ToolAct>().on_open_link(|u| open_link(u.as_str()));
    // The dock (2.17) and side questions (2.21), by pane key / chat.
    window.global::<ToolAct>().on_sug_send(|k| if let Some(c) = chat_of(k.as_str()) { suggestion_send(c); });
    window.global::<ToolAct>().on_sug_dismiss(|k| if let Some(c) = chat_of(k.as_str()) { suggestion_dismiss(c); });
    window.global::<ToolAct>().on_recap_dismiss(|k| if let Some(c) = chat_of(k.as_str()) { recap_dismiss(c); });
    // The agent dock (2.18).
    window.global::<ToolAct>().on_agent_badge(|k, task| if let Some(c) = chat_of(k.as_str()) { agent_badge(c, task); });
    // (By the chat's id from the log's row; by the pane's key from the list.)
    window.global::<ToolAct>().on_agent_open(|c, id, from_list| { let c = chat_of(c.as_str()).unwrap_or(c.as_str()).to_string(); agent_open(&c, id.as_str(), from_list); });
    window.global::<ToolAct>().on_agent_back(|k| if let Some(c) = chat_of(k.as_str()) { agent_back(c); });
    window.global::<ToolAct>().on_agent_close(|k| if let Some(c) = chat_of(k.as_str()) { agent_close(c); });
    window.global::<ToolAct>().on_agent_dismiss(|k, id| if let Some(c) = chat_of(k.as_str()) { agent_dismiss(c, vec![id.to_string()]); });
    window.global::<ToolAct>().on_agent_dismiss_all(|k, task| if let Some(c) = chat_of(k.as_str()) { agent_dismiss_all(c, task); });
    // The goal and the plan (2.19).
    window.global::<ToolAct>().on_plan_toggle(|k| if let Some(c) = chat_of(k.as_str()) { plan_toggle(c); });
    window.global::<ToolAct>().on_goal_clear(|k| if let Some(c) = chat_of(k.as_str()) { goal_clear(c); });
    window.global::<ToolAct>().on_collab_cancel(|id| { let body = json!({ "id": id.as_str() }); call(move |api| api.post("/api/collab/cancel", &body), |_| {}); });
    window.global::<ToolAct>().on_aside_close(|c, id| aside_close(c.as_str(), id.parse().unwrap_or(0)));
    // Media (2.16m): a picture's viewer, play, Download, a frame's Open.
    window.global::<ToolAct>().on_view(|k| view_picture(k.as_str()));
    window.global::<ToolAct>().on_play(|c, n, f| play_media(c.as_str(), n.as_str(), f.as_str()));
    window.global::<ToolAct>().on_download(|c, i, f| download_file(c.as_str(), i.as_str(), f.as_str()));
    window.global::<ToolAct>().on_open_frame(|c, n, u| open_frame(c.as_str(), n.as_str(), u.as_str()));
    window.on_viewer_step(viewer_step);
    window.on_viewer_close(viewer_close);
    window.on_dl_close(dl_close);
    // A click in a user's bubble on one of its URLs.
    window.global::<ToolAct>().on_click_at(|t, at| if let Some(u) = crate::md::url_at(t.as_str(), at.max(0) as usize) { open_link(&u); });
    window.global::<ToolAct>().on_copy(|t| if copy_text(t.as_str()) { toast_ok_plain("Copied"); } else { toast("Could not copy", true); });
    // Question cards (2.13).
    {
        let qa = window.global::<QAct>();
        // A pick (a press, Space) also gives the choice the keyboard: the radio
        // becomes its group's Tab stop once picked, then takes the focus.
        qa.on_pick(|c, e, qi, oi| q_act(c.as_str(), e.as_str(), move |k| { let label = k.label(qi, oi); let q = k.qs[qi as usize].clone(); let p = &mut k.picks[qi as usize]; let had = !p.other.is_empty(); questions::pick(&q, p, &label); if had { k.other_gen[qi as usize] += 1; } *k.focus_gen.entry((qi as usize, oi as usize)).or_insert(0) += 1; k.note_focus = None; }));
        qa.on_note_open(|c, e, qi, oi| q_act(c.as_str(), e.as_str(), move |k| { let label = k.label(qi, oi); let q = k.qs[qi as usize].clone(); let p = &mut k.picks[qi as usize]; let had = !p.other.is_empty(); questions::open_note(&q, p, &label); if had && p.other.is_empty() { k.other_gen[qi as usize] += 1; } k.note_focus = Some((qi as usize, label)); }));
        qa.on_note_typed(|c, e, qi, t| q_act(c.as_str(), e.as_str(), move |k| { if let Some(p) = k.picks.get_mut(qi as usize) { p.note = t.to_string(); } }));
        qa.on_note_remove(|c, e, qi| q_act(c.as_str(), e.as_str(), move |k| { if let Some(p) = k.picks.get_mut(qi as usize) { questions::remove_note(p); } k.note_focus = None; }));
        qa.on_other_typed(|c, e, qi, t| q_act(c.as_str(), e.as_str(), move |k| { if let Some(p) = k.picks.get_mut(qi as usize) { questions::type_other(p, t.as_str()); } k.note_focus = None; }));
        qa.on_move(|c, e, qi, oi, d| q_act(c.as_str(), e.as_str(), move |k| {
            let n = k.qs[qi as usize].options.len() as i32;
            if n == 0 { return; }
            let to = ((oi + d) % n + n) % n;
            let label = k.label(qi, to);
            let q = k.qs[qi as usize].clone();
            let p = &mut k.picks[qi as usize];
            let had = !p.other.is_empty();
            questions::pick(&q, p, &label);
            if had { k.other_gen[qi as usize] += 1; }
            *k.focus_gen.entry((qi as usize, to as usize)).or_insert(0) += 1;
            k.note_focus = None;
        }));
        qa.on_submit(|c, e| q_answer(c.to_string(), e.to_string(), false));
        qa.on_dismiss(|c, e| q_answer(c.to_string(), e.to_string(), true));
    }
    // Secret requests (2.12b).
    window.set_sreq_cards(ModelRc::from(with(|a| a.sreq.cards.clone()).unwrap_or_default()));
    window.on_sreq_approve(|id| sreq_arm(id.as_str()));
    window.on_sreq_confirm(sreq_confirm);
    window.on_sreq_reject(|id| sreq_reject(id.to_string()));
    window.on_sreq_typed(|t| sreq_typed(t.as_str()));
    window.on_sreq_toggle_allow(|| { with(|a| a.sreq.allow_on = !a.sreq.allow_on); render(); });
    window.on_sreq_set_duration(|d| { with(|a| a.sreq.duration = d.to_string()); render(); });
    window.on_sreq_escape(|| { with(|a| a.sreq.err.clear()); render(); });
    // The chat secrets dialog (2.12).
    window.on_pane_secrets(|id| sec_open(id as u32));
    window.on_sec_close(sec_close);
    window.on_sec_tick(|id| sec_tick(id.as_str()));
    window.on_sec_typed(|id, t| sec_typed(id.as_str(), t.as_str()));
    window.on_sec_duration_set(|d| { with(|a| if let Some(d0) = a.sec.as_mut() { d0.duration = d.to_string(); }); render(); });
    window.on_sec_grant(sec_grant);
    window.on_sec_revoke(|id| sec_revoke(Some(id.to_string())));
    window.on_sec_revoke_all(|| sec_revoke(None));
    window.on_sec_next_field(|cur, back| with(|a| {
        let Some(d) = a.sec.as_ref() else { return SharedString::default() };
        let order: Vec<&String> = d.secrets.iter().flatten().map(|x| &x.0).filter(|id| d.chosen.contains(id)).collect();
        if order.is_empty() { return SharedString::default(); }
        let i = order.iter().position(|x| x.as_str() == cur.as_str());
        let n = order.len();
        let next = match (i, back) { (None, false) => 0, (None, true) => n - 1, (Some(i), false) => (i + 1) % n, (Some(i), true) => (i + n - 1) % n };
        order[next].as_str().into()
    }).unwrap_or_default());
    window.on_pane_rename(|id, t| pane_rename(id as u32, t.as_str()));
    window.on_pane_menu(|id, on| pane_menu(id as u32, on));
    window.on_pane_maximize(|id| { animate(); with(|a| a.mosaic.maximize(id as u32)); layout_changed(); });
    window.on_draft_changed(|k, t| { set_draft(k.as_str(), t.as_str()); slash_edit(k.as_str(), t.as_str()); });
    // The composers' shared controls (ui/chat.slint Composer). The Enter mode
    // is one setting for every chat, saved (the web's crundi_chat_enter).
    let c = window.global::<Composer>();
    c.set_ctrl_enter(store::load().chat_enter == "newline");
    c.on_set_ctrl_enter(set_chat_enter);
    c.on_copied(|full| toast(&format!("Session id copied: {full}"), false));
    c.on_mode(|k| open_mode_picker(k.as_str()));
    c.on_caret(|k, c, an| { with(|a| a.carets.insert(k.to_string(), (c.max(0) as usize, an.max(0) as usize))); });
    c.on_attach(|k| pick_file(k.to_string()));
    c.on_paste(|k, si| paste(k.as_str(), si));
    c.on_take_back(|k| take_back(k.as_str()));
    // The slash menu (2.11).
    c.on_slash_move(|k, by| slash_move(k.as_str(), by));
    c.on_slash_choose(|k, i| slash_choose(k.as_str(), i));
    c.on_slash_close(|k| { if with(|a| a.slash.remove(k.as_str()).is_some()).unwrap_or(false) { render(); } });
    // The schedule panel (2.10).
    c.on_schedule(|k| sched_toggle(k.as_str()));
    c.on_sched_close(|k| { with(|a| if let Some(sc) = a.sched.get_mut(k.as_str()) { sc.open = false; }); render(); });
    c.on_sched_trigger(|k, t| sched_trigger(k.as_str(), t.as_str()));
    c.on_sched_text(|k, t| { with(|a| if let Some(sc) = a.sched.get_mut(k.as_str()) { sc.text = t.to_string(); }); });
    c.on_sched_at(|k, t| { with(|a| if let Some(sc) = a.sched.get_mut(k.as_str()) { sc.at = t.to_string(); }); });
    c.on_sched_save(|k| sched_save(k.as_str()));
    c.on_sched_delete(|k, id| sched_delete(k.as_str(), id.to_string()));
    c.on_sched_pick(|k, y, mo, d, h, mi| {
        if let Some(t) = sched::picked(y, mo.max(0) as u32, d.max(0) as u32, h.max(0) as u32, mi.max(0) as u32) {
            with(|a| if let Some(sc) = a.sched.get_mut(k.as_str()) { sc.at = t; sc.at_gen += 1; });
            render();
        }
    });
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
    load_sreqs();
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
        a.sreq.forget();
        a.sreq.list.clear();
        a.sreq.armed = None;
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
        Event::Connected(on) => {
            if let Some(w) = ui() { w.set_online(on); }
            // Back online: requests may have come and gone meanwhile.
            if on { load_sreqs(); }
        }
        Event::SecretRequests(v) => sreq_list(v.get("requests").unwrap_or(&Value::Null)),
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
                    a.chats.insert(id.to_string(), ChatPaneState { conv: Conversation::default(), rows: vec![], model: Rc::new(VecModel::default()), error: String::new(), open: HashSet::new(), think: think::Think::default(), gen: 0, asides: vec![], sheet_list: None, plan_open: false, sheet_panel: None, panel_model: Rc::new(VecModel::default()) });
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
                p.content_gen = a.chats.get(id).map(|cs| cs.gen).unwrap_or(0);
                // The agent dock (2.18): the badges, and the sheet.
                if let Some(cs) = a.chats.get(id) { agent_pane(&mut p, cs); goal_pane(&mut p, cs); }
                // The dock (2.17): the suggestion while idle and running; the recap.
                if let Some(cs) = a.chats.get(id) {
                    if chat_state_now(a, id) == "idle" { p.sug_text = cs.conv.suggestion.as_str().into(); }
                    if let Some((t, why)) = &cs.conv.recap { p.recap_text = t.as_str().into(); p.recap_caption = if why == "resumed" { "WHERE THIS CHAT STANDS" } else { "WHILE YOU WERE AWAY" }.into(); }
                }
                // The drawers (2.9): held lines, and the handed-over ones.
                if let Some(h) = a.held.get(id) { p.q_held = h.join("\n").into(); p.q_held_n = h.len() as i32; }
                if let Some((kind, body)) = a.chats.get(id).and_then(|cs| cs.conv.handed()) {
                    p.q_sent = body.into();
                    p.q_sent_kind = match kind { Handed::Recallable => "recallable", Handed::Started => "started", Handed::Waiting => "waiting" }.into();
                }
                // The schedule panel (2.10).
                if let Some(sc) = a.sched.get(&key) {
                    p.s_open = sc.open;
                    p.s_text = sc.text.clone().into();
                    p.s_text_gen = sc.text_gen;
                    p.s_trigger = sc.trigger.clone().into();
                    p.s_at = sc.at.clone().into();
                    p.s_at_gen = sc.at_gen;
                    p.s_note = sc.note.clone().into();
                    p.s_items = sc.items.clone().into();
                    p.s_limit_off = sc.warm == Some(false);
                    p.s_list = sc.list.clone().into();
                    p.s_unsupported = sc.unsupported;
                }
                if let Some((items, sel)) = a.slash.get(&key) {
                    p.slash_open = true;
                    p.slash_items = items.clone().into();
                    p.slash_sel = *sel as i32;
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
    // Held lines whose chat no longer needs input go out (after the frame,
    // so this never runs inside a render).
    if with(|a| !a.held.is_empty() && !std::mem::replace(&mut a.flush_pending, true)).unwrap_or(false) {
        slint::Timer::single_shot(Duration::from_millis(0), flush_held);
    }
    if let Some(w) = ui() { with(|a| show_sec(&w, a)); with(|a| show_sreq(&w, a)); }
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
pub fn toast(text: &str, error: bool) { toast_kind(text, error, false); }

/// A toast of success (a green edge, the web's `.toast.success`).
pub fn toast_ok(text: &str) { toast_kind(text, false, true); }

fn toast_kind(text: &str, error: bool, ok: bool) {
    let Some(w) = ui() else { return };
    w.set_toast(text.into());
    w.set_toast_error(error);
    w.set_toast_ok(ok);
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
                // Held lines go into the draft (the web drops them).
                if let Some(h) = a.held.remove(&id) {
                    sync_held(a, &id);
                    let key = chat_key(&id);
                    let cur = a.drafts.get(&key).cloned().unwrap_or_default();
                    save_draft_in(a, &key, &chat::given_back(&h.join("\n"), &cur));
                }
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
            "ui-history" => { if let Some(sess) = m.get("session") { cs.conv.load(sess); } a.sending.remove(&id); cs.sheet_list = None; cs.sheet_panel = None; true }
            "ui-event" => {
                // Thinking blocks (2.15): one that starts streaming opens (the
                // others the rules opened fold); a finished one folds later.
                if let Some(ev) = m.get("event") {
                    let ty = s(ev, "type");
                    if ty == "entry" {
                        if let Some(en) = ev.get("entry") {
                            if s(en, "kind") == "thinking" && en.get("streaming").and_then(|v| v.as_bool()) == Some(true) { cs.think.started(&s(en, "id")); }
                        }
                    } else if ty == "patch" && ev.pointer("/patch/streaming").and_then(|v| v.as_bool()) == Some(false) {
                        let tid = s(ev, "id");
                        if cs.conv.entries.iter().any(|e| s(e, "id") == tid && s(e, "kind") == "thinking") {
                            let chat = id.clone();
                            slint::Timer::single_shot(Duration::from_millis(think::LINGER_MS), move || {
                                let known = with(|a| a.chats.get_mut(&chat).map(|cs| cs.think.linger_over(&tid)).is_some()).unwrap_or(false);
                                if known { show_chat(&chat); }
                            });
                        }
                    }
                }
                // The server's own state replaces the optimistic one.
                let ty = m.get("event").map(|e| s(e, "type")).unwrap_or_default();
                // (An idle that comes before the send's working doesn't undo it.)
                let st = m.get("event").map(|e| s(e, "state")).unwrap_or_default();
                if (ty == "state" && st != "idle") || matches!(ty.as_str(), "exit" | "history") { a.sending.remove(&id); }
                m.get("event").map(|e| cs.conv.apply(e)).unwrap_or(false) || ty == "exit"
            }
            _ => false,
        }
    }).unwrap_or(false);
    if changed {
        with(|a| if let Some(cs) = a.chats.get_mut(&id) { cs.gen += 1; });
        // An open agent panel follows its transcript (2.18).
        with(|a| if let Some(cs) = a.chats.get(&id) { if cs.sheet_panel.is_some() { panel_rows(cs, &id, &a.media); } });
        show_chat(&id);
        render();
    }
}

/// Claude's text as rows: markdown blocks (paragraphs, list items, headings)
/// and fenced code blocks, so each gets the web chat's spacing.
fn split_blocks(id: &str, text: &str) -> Vec<Row> {
    use crate::md::{self, Line};
    let mut out: Vec<Row> = vec![];
    let mut para: Vec<String> = vec![];
    let mut code: Option<String> = None;
    // A numbered list counts from 1 (the web's <ol>), until it ends.
    let mut num = 0;
    let n = |out: &Vec<Row>| format!("{id}#{}", out.len());
    let flush = |para: &mut Vec<String>, out: &mut Vec<Row>| {
        if !para.is_empty() {
            let refs: Vec<&str> = para.iter().map(String::as_str).collect();
            out.push(Row::Prose(n(out), md::paragraph(&refs), "p", String::new(), false));
        }
        para.clear();
    };
    for raw in text.split('\n') {
        if let Some(c) = &mut code {
            if md::line(raw) == Line::Fence {
                // One trailing newline dropped.
                let body = c.strip_suffix('\n').unwrap_or(c).to_string();
                code = None;
                out.push(Row::Code(n(&out), body, false));
            } else { c.push_str(raw); c.push('\n'); }
            continue;
        }
        let l = md::line(raw);
        if !matches!(l, Line::Numbered(_)) { num = 0; }
        match l {
            Line::Fence => { flush(&mut para, &mut out); code = Some(String::new()); }
            Line::Blank => flush(&mut para, &mut out),
            Line::Heading(top, h) => { flush(&mut para, &mut out); out.push(Row::Prose(n(&out), format!("**{}**", md::linkify(h)), if top { "h" } else { "h4" }, String::new(), false)); }
            Line::Rule => { flush(&mut para, &mut out); out.push(Row::Prose(n(&out), String::new(), "hr", String::new(), false)); }
            Line::Quote(q) => { flush(&mut para, &mut out); out.push(Row::Prose(n(&out), md::linkify(q), "quote", String::new(), false)); }
            Line::Bullet(r) => { flush(&mut para, &mut out); out.push(Row::Prose(n(&out), md::linkify(r), "li", "•".into(), false)); }
            Line::Numbered(r) => { flush(&mut para, &mut out); num += 1; out.push(Row::Prose(n(&out), md::linkify(r), "li", format!("{num}."), false)); }
            Line::Text(t) => para.push(t.to_string()),
        }
    }
    if let Some(c) = code { out.push(Row::Code(n(&out), c.strip_suffix('\n').unwrap_or(&c).to_string(), false)); }
    flush(&mut para, &mut out);
    // A list's last item: the list's 8 below it (F94).
    // (A numbered list and a bulleted one are two lists, as the web's <ol>
    // and <ul>.)
    for k in 0..out.len() {
        let bullet = |m: &str| m == "•";
        let next_same = match (out.get(k), out.get(k + 1)) {
            (Some(Row::Prose(_, _, "li", m, _)), Some(Row::Prose(_, _, "li", n, _))) => bullet(m) == bullet(n),
            _ => false,
        };
        if let Row::Prose(_, _, kind, _, _) = &mut out[k] { if *kind == "li" && !next_same { *kind = "lie"; } }
    }
    match out.last_mut() {
        Some(Row::Prose(_, _, _, _, last)) | Some(Row::Code(_, _, last)) => *last = true,
        _ => {}
    }
    out
}

/// The log's rows (2.14): a call that started a sub-agent is an agent row
/// (never folded); calls Claude Code summed up fold behind one group head,
/// drawn where the group's first call is (its other calls take no room
/// there); any other call is a card. Claude's text in blocks.
fn rows_of(shown: &[Shown], agents: &[Value], open: &HashSet<String>, think: &think::Think, asides: &[Aside]) -> Vec<Row> {
    let agent_of = |e: &Shown| -> Option<&Value> {
        if e.tool_use_id.is_empty() { return None; }
        agents.iter().find(|a| s(a, "toolUseId") == e.tool_use_id && s(a, "kind") != "task")
    };
    let grouped = |e: &Shown| e.kind == "tool" && !e.group.is_empty() && !e.group_summary.is_empty() && agent_of(e).is_none();
    let mut groups: Vec<(String, Vec<&Shown>)> = vec![];
    for e in shown.iter().filter(|e| grouped(e)) {
        match groups.iter_mut().find(|g| g.0 == e.group) { Some(g) => g.1.push(e), None => groups.push((e.group.clone(), vec![e])) }
    }
    let mut drawn: HashSet<String> = HashSet::new();
    let mut rows = vec![];
    // Side questions: each after the entry that was last when it was asked
    // (before everything when the log was empty; at the end when that entry
    // is gone).
    let ids: HashSet<&str> = shown.iter().map(|e| e.id.as_str()).collect();
    for x in asides.iter().filter(|x| x.anchor.is_empty()) { rows.push(Row::Aside(x.clone())); }
    for e in shown {
        let after: Vec<Row> = asides.iter().filter(|x| x.anchor == e.id).map(|x| Row::Aside(x.clone())).collect();
        if e.kind == "tool" {
            if let Some(a) = agent_of(e) {
                rows.push(Row::Agent(e.id.clone(), a.clone()));
            } else if grouped(e) {
                if !drawn.insert(e.group.clone()) { continue; }
                let members = groups.iter().find(|g| g.0 == e.group).map(|g| g.1.clone()).unwrap_or_default();
                let gid = format!("g:{}", e.group);
                rows.push(Row::Tools(gid.clone(), e.group_summary.clone(), members.into_iter().map(|m| (m.clone(), open.contains(&m.id))).collect(), open.contains(&gid)));
            } else {
                rows.push(Row::Tool(e.id.clone(), e.clone(), open.contains(&e.id)));
            }
        } else if e.kind == "assistant" {
            rows.extend(split_blocks(&e.id, &e.text));
        } else if e.kind == "thinking" {
            rows.push(Row::Think(e.clone(), think.open(&e.id)));
        } else {
            rows.push(Row::Entry(e.clone()));
        }
        rows.extend(after);
    }
    for x in asides.iter().filter(|x| !x.anchor.is_empty() && !ids.contains(x.anchor.as_str())) { rows.push(Row::Aside(x.clone())); }
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

/// A card as the view draws it: the head (the summary on one line, as the
/// web's HTML collapses its whitespace) and the body.
fn tool_view(t: &Shown, open: bool, mc: &MediaCtx) -> ToolView {
    let mut v = ToolView {
        id: t.id.as_str().into(), title: t.title.as_str().into(),
        detail: t.detail.split_whitespace().collect::<Vec<_>>().join(" ").into(),
        text: t.text.as_str().into(), status: t.status.as_str().into(), error: t.error, open,
        ..Default::default()
    };
    match &t.body {
        tools::Body::Bash { command, description } => { v.kind = "bash".into(); v.args = tools::expand_tabs(command).into(); v.note = description.as_str().into(); }
        tools::Body::Write(c) => { v.kind = "write".into(); v.args = tools::expand_tabs(c).into(); }
        tools::Body::Diff(lines) => { v.kind = "diff".into(); v.lines = ModelRc::new(VecModel::from(lines.iter().map(|(l, add)| DiffLine { text: tools::expand_tabs(l).into(), add: *add }).collect::<Vec<_>>())); }
        tools::Body::Todos(items) => { v.kind = "todos".into(); v.todos = ModelRc::new(VecModel::from(items.iter().map(|(t, st)| TodoItem { text: t.as_str().into(), state: st.as_str().into() }).collect::<Vec<_>>())); }
        tools::Body::Json => { v.kind = "json".into(); v.args = t.args.as_str().into(); }
    }
    // What a show_* call showed, under its card (done, not failed).
    if t.status != "running" && !t.error {
        if let Some(res) = t.raw.as_str() {
            let (items, cap) = media::items(res);
            let several = items.iter().filter(|i| matches!(i.kind.as_str(), "image" | "video" | "embed")).count() > 1;
            v.media_stack = items.iter().all(|i| matches!(i.kind.as_str(), "file" | "audio"));
            v.media_caption = if several || items.is_empty() { cap.as_str().into() } else { SharedString::default() };
            v.media = ModelRc::new(VecModel::from(items.iter().map(|i| mc.item(i, several)).collect::<Vec<_>>()));
        }
    }
    v
}

/// What tool_view needs to draw media: the chat, what is cached, and which
/// pictures to fetch.
struct MediaCtx<'a> {
    chat: String,
    cache: &'a HashMap<String, (String, Option<slint::Image>)>,
    want: std::cell::RefCell<Vec<String>>,
}

impl MediaCtx<'_> {
    fn item(&self, i: &media::Item, several: bool) -> MediaItem {
        let (w, h) = match i.kind.as_str() { "embed" => media::frame_size(i), "image" | "video" => media::box_size(i.w, i.h, several), _ => (0.0, 0.0) };
        // A picture, or a video's poster: from the cache, else fetched.
        // (A stored frame's HTML is asked for too: a 404 is "gone".)
        let pic = if i.kind == "image" || (i.kind == "embed" && !i.name.is_empty()) { i.name.clone() } else if i.kind == "video" { i.poster.clone() } else { String::new() };
        let key = format!("{}|{}", self.chat, pic);
        let (state, image) = if pic.is_empty() { ("ready".to_string(), None) } else {
            match self.cache.get(&key) {
                Some((st, img)) => (st.clone(), img.clone()),
                None => { self.want.borrow_mut().push(key.clone()); ("loading".into(), None) }
            }
        };
        // A video or audio on its way to the player.
        let play_key = format!("{}|{}", self.chat, i.name);
        let fetching = self.cache.get(&play_key).is_some_and(|(st, _)| st == "fetching");
        let (cap, link) = if i.kind == "embed" { media::frame_caption(i) } else { (media::item_caption(i), String::new()) };
        MediaItem {
            kind: i.kind.as_str().into(), key: key.into(), name: i.name.as_str().into(), id: i.id.as_str().into(),
            filename: i.filename.as_str().into(), w: w as f32, h: h as f32, caption: cap.into(), link: link.into(),
            url: if i.url.is_empty() { i.src.as_str().into() } else { i.url.as_str().into() },
            state: state.into(), image: image.unwrap_or_default(), fetching,
            line: { let sz = media::fmt_bytes(i.size); if i.caption.is_empty() { sz } else { format!("{sz} · {}", i.caption) } }.into(),
            duration: if i.duration > 0.0 { media::fmt_duration(i.duration).into() } else { SharedString::default() },
            chat: self.chat.as_str().into(),
        }
    }
}

fn to_view(r: &Row, mc: &MediaCtx) -> EntryView {
    match r {
        Row::Prose(id, t, kind, marker, last) => EntryView {
            id: id.clone().into(), kind: "assistant".into(), text: t.clone().into(), md: md(t),
            status: (*kind).into(), title: marker.clone().into(),
            // Paragraphs and quotes 8 apart, list items 4 (their 2 margins), a
            // heading 6 below (12 above: its own padding), `##` and rules
            // none; the message's last block 10.
            gap: if *last { 10.0 } else { match *kind { "li" => 2.0, "lie" => 8.0, "h" => 6.0, "h4" | "hr" => 0.0, _ => 8.0 } },
            ..Default::default()
        },
        Row::Code(id, t, last) => EntryView { id: id.clone().into(), kind: "code".into(), text: t.clone().into(), gap: if *last { 10.0 } else { 8.0 }, ..Default::default() },
        Row::Tool(id, t, open) => EntryView {
            id: id.clone().into(), kind: "tool".into(), status: t.status.clone().into(), error: t.error, open: *open,
            tools: ModelRc::new(VecModel::from(vec![tool_view(t, *open, mc)])), gap: 10.0, ..Default::default()
        },
        Row::Tools(id, summary, tools, open) => EntryView {
            id: id.clone().into(),
            kind: "tools".into(),
            title: summary.split_whitespace().collect::<Vec<_>>().join(" ").into(),
            detail: format!("{} tool{}", tools.len(), if tools.len() == 1 { "" } else { "s" }).into(),
            status: if tools.iter().any(|t| t.0.status == "running") { "running".into() } else { "done".into() },
            error: tools.iter().any(|t| t.0.error),
            open: *open,
            tools: ModelRc::new(VecModel::from(tools.iter().map(|(t, o)| tool_view(t, *o, mc)).collect::<Vec<_>>())),
            gap: 10.0,
            ..Default::default()
        },
        Row::Aside(x) => EntryView {
            id: format!("aside-{}", x.id).into(), kind: "aside".into(), text: x.question.as_str().into(), status: x.state.as_str().into(),
            detail: if x.state == "error" { x.text.as_str().into() } else { SharedString::default() },
            md: if x.state == "done" { md(&crate::md::linkify(&x.text)) } else { StyledText::default() },
            blocks: if x.state == "done" { ModelRc::new(VecModel::from(split_blocks("aside", &x.text).into_iter().filter_map(|r| match r {
                Row::Prose(_, t, kind, marker, _) => Some(AnsBlock { kind: if kind.starts_with("li") { "li".into() } else { "p".into() }, marker: marker.into(), md: md(&t), text: t.into() }),
                Row::Code(_, t, _) => Some(AnsBlock { kind: "code".into(), marker: SharedString::default(), md: StyledText::default(), text: t.into() }),
                _ => None,
            }).collect::<Vec<_>>())) } else { ModelRc::default() },
            request_id: x.id.to_string().into(), gap: 10.0,
            ..Default::default()
        },
        Row::Think(e, open) => EntryView {
            id: e.id.clone().into(), kind: "thinking".into(), text: e.text.clone().into(),
            detail: e.detail.clone().into(), status: e.status.clone().into(), open: *open, gap: 10.0,
            ..Default::default()
        },
        Row::Agent(id, a) => EntryView {
            id: id.clone().into(),
            kind: "agent".into(),
            request_id: s(a, "toolUseId").into(),
            title: { let t = s(a, "subagentType"); if t.is_empty() { "Agent".into() } else { t.into() } },
            detail: s(a, "description").split_whitespace().collect::<Vec<_>>().join(" ").into(),
            // Running while its status is running or none; failed; else done.
            status: { let st = s(a, "status"); if st.is_empty() || st == "running" { "running".into() } else if st == "failed" { "failed".into() } else { "done".into() } },
            error: s(a, "status") == "failed",
            gap: 10.0,
            ..Default::default()
        },
        Row::Entry(e) => EntryView {
            id: e.id.clone().into(),
            // A user's message: its URLs as links (2.16, F92).
            md: if e.kind == "user" { StyledText::from_markdown(&crate::md::plain_links(&e.text)).unwrap_or_default() } else { StyledText::default() },
            kind: e.kind.into(),
            text: e.text.clone().into(),
            title: e.title.clone().into(),
            detail: e.detail.clone().into(),
            status: e.status.clone().into(),
            error: e.error,
            request_id: e.request_id.clone().into(),
            options: ModelRc::new(VecModel::from(e.options.iter().map(|o| SharedString::from(o.as_str())).collect::<Vec<_>>())),
            // A permission card's reason, input, tag and (closed) what it asked.
            note: e.note.as_str().into(),
            args: e.args.as_str().into(),
            tag: e.group_summary.as_str().into(),
            sub: e.group.as_str().into(),
            gap: 10.0,
            ..Default::default()
        },
    }
}

/// A chat's log on screen, rewriting only the rows that changed.
fn show_chat(id: &str) {
    let wanted = with(|a| {
        drafts_for_server(a);
        let mut wanted: Vec<String> = vec![];
        let Some(cs) = a.chats.get_mut(id) else { return wanted };
        let now = rows_of(&cs.conv.view(), &cs.conv.agents, &cs.open, &cs.think, &cs.asides);
        for (i, r) in now.iter().enumerate() {
            if cs.rows.get(i) == Some(r) { continue; }
            let mc = MediaCtx { chat: id.to_string(), cache: &a.media, want: std::cell::RefCell::new(vec![]) };
            let mut v = to_view(r, &mc);
            wanted.extend(mc.want.into_inner());
            v.chat = id.into();
            if let Row::Entry(e) = r { if e.kind == "question" { q_view(&mut a.qcards, &a.saved_drafts, id, e, &mut v); } }
            if i < cs.model.row_count() { cs.model.set_row_data(i, v); } else { cs.model.push(v); }
        }
        while cs.model.row_count() > now.len() { cs.model.remove(cs.model.row_count() - 1); }
        cs.rows = now;
        for k in &wanted { a.media.insert(k.clone(), ("loading".into(), None)); }
        wanted
    }).unwrap_or_default();
    for k in wanted { fetch_media(k); }
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
    // A side question (/btw …, 2.21): asked at once in any state, never
    // queued or held.
    if let Some(q) = crate::md::btw(&text) {
        if let Some(id) = chat_of(key).map(str::to_string) { ask_aside(key, &id, text.trim_end().to_string(), q); }
        return;
    }
    send_plain(key, text);
}

fn send_plain(key: &str, text: String) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let text = text.trim_end().to_string();
    if text.trim().is_empty() { return; }
    let k = key.to_string();
    let state = with(|a| {
        save_draft_in(a, &k, "");
        *a.draft_gen.entry(k.clone()).or_insert(0) += 1;
        if let Some(cs) = a.chats.get_mut(&id) { cs.error.clear(); }
        let st = chat_state_now(a, &id);
        // Needs input: held here, sent as one when the prompt is answered (2.9).
        if st == "needs-input" { a.held.entry(id.clone()).or_default().push(text.clone()); sync_held(a, &id); }
        st
    }).unwrap_or_default();
    match state.as_str() {
        "needs-input" => { render(); write_drafts_soon(); }
        // Working: handed over at once, a slot of its own in the drawer.
        "working" => post_send(&k, &id, text, true),
        // Idle: a plain send; the chat shows working at once.
        _ => {
            with(|a| a.sending.insert(id.clone()));
            post_send(&k, &id, text, false);
        }
    }
}

/// A chat's state as its composer reads it now (the list's, the stream's,
/// and working at once after a send).
fn chat_state_now(a: &App, id: &str) -> String {
    let st = match a.sessions.iter().find(|c| s(c, "id") == id) { Some(c) => agent_state(c), None => a.chats.get(id).map(|cs| cs.conv.state.clone()).unwrap_or_default() };
    head::composer_state(&st, a.sending.contains(id))
}

/// POST /send. Handed over (Claude busy), its slot shows at once; the answer
/// gives it its id, or drops it (the server was idle: it's in the log). A
/// refused send gives the text back in front of the box's (and an error row).
fn post_send(key: &str, id: &str, text: String, handed: bool) {
    if handed { with(|a| if let Some(cs) = a.chats.get_mut(id) { cs.conv.add_slot(&text); }); }
    render();
    write_drafts_soon();
    let stub = crate::testctl::take_stub("send");
    let (k, id, path) = (key.to_string(), id.to_string(), format!("/api/ui-sessions/{id}/send"));
    let body = text.clone();
    call(move |api| match stub { Some(v) => Ok(v), None => api.post(&path, &json!({ "text": body })) }, move |r| {
        match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) != Some(false) => {
                let injected = v.get("injected").and_then(|x| x.as_bool()).unwrap_or(false);
                with(|a| if let Some(cs) = a.chats.get_mut(&id) {
                    if injected && !cs.conv.slots.iter().any(|x| x.text == text && x.uuid.is_empty()) { cs.conv.add_slot(&text); }
                    cs.conv.sent(&text, injected, &s(&v, "uuid"));
                });
                render();
            }
            Ok(v) => { let e = s(&v, "error"); send_refused(&k, &id, if e.is_empty() { "Failed to send".into() } else { e }, text); }
            Err(e) => send_refused(&k, &id, e, text),
        }
    });
}

fn send_refused(key: &str, id: &str, error: String, text: String) {
    with(|a| {
        a.sending.remove(id);
        // Its slot goes; the text comes back in front of what the box holds.
        let cur = a.drafts.get(key).cloned().unwrap_or_default();
        let back = chat::given_back(&text, &cur);
        set_box(a, key, &back, back.len(), false);
        if let Some(cs) = a.chats.get_mut(id) { cs.conv.drop_slot(&text); }
        push_error(a, id, &error);
    });
    show_box();
    show_chat(id);
    render();
    write_drafts_soon();
}

/// A chat's held lines changed: the saved copy follows (written with the
/// drafts), so they outlive the app.
fn sync_held(a: &mut App, id: &str) {
    drafts_for_server(a);
    let text = a.held.get(id).map(|h| h.join("\n")).unwrap_or_default();
    a.saved_drafts.set_held(id, &text, now_ms());
    a.drafts_dirty = true;
}

/// The one way code sets a chat box's text (2.7–2.9): the draft (saved),
/// the caret at `caret`, and the box shows both; `focus`: the box takes the
/// focus too (an attached path, a take-back), not for a give-back.
fn set_box(a: &mut App, key: &str, text: &str, caret: usize, focus: bool) {
    save_draft_in(a, key, text);
    a.carets.insert(key.to_string(), (caret, caret));
    a.box_set = Some((key.to_string(), text.to_string(), caret, focus));
}

/// Show what [set_box] set (outside the app's borrow).
fn show_box() {
    let Some((key, text, caret, focus)) = with(|a| a.box_set.take()).flatten() else { return };
    if let Some(w) = ui() {
        let c = w.global::<Composer>();
        c.set_insert_key(key.into());
        c.set_insert_text(text.into());
        c.set_insert_caret(caret as i32);
        c.set_insert_focus(focus);
        c.set_insert_gen(c.get_insert_gen() + 1);
    }
}

// ─── The chat secrets dialog (2.12, src/secrets.rs) ───

const SEC_UNSUPPORTED: &str = "This server does not support this yet. Update the server to use it.";

/// The header's lock: the dialog for that pane's chat (its header menu
/// closed first). The grants known now at once; then the chat's own and the
/// secrets to pick, from the server.
fn sec_open(leaf: u32) {
    let Some(id) = leaf_rect(leaf).and_then(|l| l.key).as_deref().and_then(chat_of).map(str::to_string) else { return };
    let cap = with(|a| {
        a.menu = 0;
        a.sec = Some(SecDlg {
            chat: id.clone(), secrets: None, chosen: vec![], pins: HashMap::new(), bad: String::new(), duration: "1h".into(),
            busy: false, err: String::new(), revoking: HashSet::new(), unsupported: !a.secrets_cap, pin_gen: 0,
        });
        if !a.secrets_cap { if let Some(d) = a.sec.as_mut() { d.err = SEC_UNSUPPORTED.into(); } }
        a.secrets_cap
    }).unwrap_or(false);
    render();
    if !cap { return; }
    let Some(api) = api() else { return };
    let (api2, id2, id3) = (api.clone(), id.clone(), id.clone());
    bg(move || api.get(&format!("/api/secrets/chat?sessionId={}", enc(&id2))), move |r| {
        with(|a| match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => { if let Some(d) = a.sec.as_mut().filter(|d| d.chat == id3) { d.err = s(&v, "error"); } }
            Ok(v) => { a.grants.insert(id3.clone(), v.get("grants").cloned().unwrap_or(Value::Null)); }
            Err(ApiError::Status(_, e)) if !e.is_empty() => { if let Some(d) = a.sec.as_mut().filter(|d| d.chat == id3) { d.err = e; } }
            Err(_) => {}
        });
        render();
    });
    bg(move || api2.get("/api/secrets"), move |r| {
        with(|a| if let Some(d) = a.sec.as_mut().filter(|d| d.chat == id) {
            d.secrets = Some(match r {
                Ok(v) => v.get("secrets").and_then(|x| x.as_array()).cloned().unwrap_or_default().iter()
                    .map(|x| (s(x, "id"), s(x, "name"), { let l = s(x, "lock"); if l.is_empty() { "passphrase".into() } else { l } })).collect(),
                Err(_) => vec![],
            });
        });
        render();
    });
}

/// Esc from the system (src/main.rs), wherever the focus is: an open secrets
/// dialog closes (its own key handler can miss it when the focused field
/// went with a grant).
pub fn escape() {
    if with(|a| a.sec.is_some()).unwrap_or(false) { sec_close(); }
}

/// Closing: everything typed is overwritten and dropped.
fn sec_close() {
    with(|a| if let Some(mut d) = a.sec.take() { for v in d.pins.values_mut() { v.clear(); v.shrink_to_fit(); } });
    render();
}

fn sec_tick(id: &str) {
    with(|a| if let Some(d) = a.sec.as_mut() {
        if let Some(i) = d.chosen.iter().position(|x| x == id) {
            d.chosen.remove(i);
            if let Some(mut p) = d.pins.remove(id) { p.clear(); }
            if d.bad == id { d.bad.clear(); }
        } else {
            d.chosen.push(id.to_string());
        }
    });
    render();
}

/// Typed in a lock field: kept (a PIN takes digits only, at most 6; the
/// field is reset to what is kept when it took something else).
fn sec_typed(id: &str, text: &str) {
    let changed = with(|a| {
        let d = a.sec.as_mut()?;
        let lock = d.secrets.iter().flatten().find(|x| x.0 == id).map(|x| x.2.clone()).unwrap_or_default();
        let kept = secrets::typed(&lock, text);
        let reset = kept != text;
        d.pins.insert(id.to_string(), kept);
        if reset { d.pin_gen += 1; }
        Some(reset)
    }).flatten().unwrap_or(false);
    if changed { render(); }
}

/// The raw answer of a POST (a refusal's body names the secret).
fn post_raw(api: &Api, path: &str, body: &Value) -> Result<(u16, Value), ApiError> {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    let (status, text) = api.post_with_progress(path, &bytes, &|_, _| {})?;
    if status == 0 { return Err(ApiError::Net("Network error".into())); }
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

/// Pre-authorise: the two checks here, then the server (all or nothing).
fn sec_grant() {
    let req = with(|a| {
        let d = a.sec.as_mut()?;
        if d.busy || d.unsupported { return None; }
        if d.chosen.is_empty() { d.err = "Choose at least one secret.".into(); return None; }
        let locks: HashMap<String, String> = d.secrets.iter().flatten().map(|x| (x.0.clone(), x.2.clone())).collect();
        if let Some(bad) = d.chosen.iter().find(|id| !secrets::lock_ok(locks.get(*id).map(String::as_str).unwrap_or(""), d.pins.get(*id).map(String::as_str).unwrap_or(""))) {
            d.bad = bad.clone();
            d.err = "Enter the PIN or passphrase of each secret chosen.".into();
            return None;
        }
        d.bad.clear();
        d.err.clear();
        d.busy = true;
        Some(json!({ "action": "grant", "sessionId": d.chat, "duration": d.duration,
            "secrets": d.chosen.iter().map(|id| json!({ "id": id, "pin": d.pins.get(id).cloned().unwrap_or_default() })).collect::<Vec<_>>() }))
    }).flatten();
    render();
    let Some(body) = req else { return };
    let Some(api) = api() else { return };
    let chat = s(&body, "sessionId");
    bg(move || post_raw(&api, "/api/secrets/chat", &body), move |r| {
        let toast_ok = with(|a| {
            if let Ok((_, v)) = &r { if let Some(g) = v.get("grants") { a.grants.insert(chat.clone(), g.clone()); } }
            let d = a.sec.as_mut().filter(|d| d.chat == chat)?;
            d.busy = false;
            match r {
                Ok((st, v)) if st < 300 && v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {
                    for p in d.pins.values_mut() { p.clear(); }
                    d.pins.clear();
                    d.chosen.clear();
                    d.bad.clear();
                    d.pin_gen += 1;
                    Some(true)
                }
                Ok((404, _)) => { d.err = SEC_UNSUPPORTED.into(); Some(false) }
                Ok((_, v)) => {
                    let e = s(&v, "error");
                    let sid = s(&v, "secretId");
                    match d.secrets.iter().flatten().find(|x| x.0 == sid && !sid.is_empty()).map(|x| x.1.clone()) {
                        Some(name) => {
                            d.pins.insert(sid.clone(), String::new());
                            d.bad = sid;
                            d.pin_gen += 1;
                            d.err = format!("{name}: {e}");
                        }
                        None => d.err = if e.is_empty() { "Nothing could be pre-authorised for this chat.".into() } else { e },
                    }
                    Some(false)
                }
                Err(e) => { d.err = e.to_string(); Some(false) }
            }
        }).flatten();
        render();
        if toast_ok == Some(true) { toast("Pre-authorised for this chat", false); }
    });
}

/// Revoke one (Some) or all (None): no confirmation; the button pending.
fn sec_revoke(one: Option<String>) {
    let req = with(|a| {
        let d = a.sec.as_mut()?;
        let tag = one.clone().unwrap_or_else(|| "*all*".into());
        if !d.revoking.insert(tag.clone()) { return None; }
        let name = one.as_ref().and_then(|id| a.grants.get(&d.chat).and_then(|g| g.as_array()).and_then(|g| g.iter().find(|x| s(x, "secretId") == *id)).map(|x| s(x, "name"))).unwrap_or_default();
        let body = match &one { Some(id) => json!({ "action": "revoke", "sessionId": d.chat, "secretId": id }), None => json!({ "action": "revokeAll", "sessionId": d.chat }) };
        Some((d.chat.clone(), tag, name, body))
    }).flatten();
    render();
    let Some((chat, tag, name, body)) = req else { return };
    let Some(api) = api() else { return };
    let all = one.is_none();
    bg(move || post_raw(&api, "/api/secrets/chat", &body), move |r| {
        let msg = with(|a| {
            if let Some(d) = a.sec.as_mut() { d.revoking.remove(&tag); }
            match r {
                Ok((st, v)) if st < 300 && v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {
                    if let Some(g) = v.get("grants") { a.grants.insert(chat.clone(), g.clone()); }
                    (if all { "Revoked all for this chat".to_string() } else { format!("Revoked {name}") }, false)
                }
                Ok((404, _)) => (SEC_UNSUPPORTED.to_string(), true),
                Ok((_, v)) => { let e = s(&v, "error"); (if !e.is_empty() { e } else if all { "Could not revoke them".into() } else { "Could not revoke it".into() }, true) }
                Err(e) => (e.to_string(), true),
            }
        }).unwrap_or_default();
        render();
        toast(&msg.0, msg.1);
    });
}

/// The dialog on screen (in render, after the panes).
fn show_sec(w: &MainWindow, a: &App) {
    w.set_sec_open(a.sec.is_some());
    let Some(d) = a.sec.as_ref() else { return };
    let now = chrono::Local::now();
    let now_s = now.timestamp_millis() as f64 / 1000.0;
    let grants: Vec<SecGrant> = a.grants.get(&d.chat).and_then(|g| g.as_array()).cloned().unwrap_or_default().iter()
        .filter(|g| match g.get("expiresAt").and_then(|e| e.as_str()) { Some(at) if !at.is_empty() => parse_iso(at).map(|t| t > now_s).unwrap_or(true), _ => true })
        .map(|g| SecGrant {
            id: s(g, "secretId").into(), name: s(g, "name").into(),
            when: secrets::when(g.get("expiresAt").and_then(|e| e.as_str()), now).into(),
            busy: d.revoking.contains(&s(g, "secretId")),
        }).collect();
    w.set_sec_grants(ModelRc::new(VecModel::from(grants)));
    let picks: Vec<SecPick> = d.secrets.iter().flatten().map(|(id, name, lock)| SecPick {
        id: id.into(), name: name.into(), lock: lock.into(), chosen: d.chosen.contains(id), bad: d.bad == *id,
        pin: d.pins.get(id).cloned().unwrap_or_default().into(),
    }).collect();
    if w.get_sec_picks().row_count() != picks.len() || w.get_sec_picks().iter().zip(&picks).any(|(x, y)| x != *y) { w.set_sec_picks(ModelRc::new(VecModel::from(picks))); }
    w.set_sec_pick_line(match &d.secrets { None => "Loading the secrets…".into(), Some(v) if v.is_empty() => "There are no secrets yet. Add them under Secrets.".into(), _ => SharedString::default() });
    w.set_sec_duration(d.duration.clone().into());
    w.set_sec_err(d.err.clone().into());
    w.set_sec_busy(d.busy);
    w.set_sec_revoking_all(d.revoking.contains("*all*"));
    w.set_sec_unsupported(d.unsupported);
    w.set_sec_pin_gen(d.pin_gen);
}

/// The test driver's view of the dialog.
pub fn sec_of() -> Value {
    with(|a| a.sec.as_ref().map(|d| json!({
        "chat": d.chat, "loading": d.secrets.is_none(), "secrets": d.secrets.iter().flatten().map(|x| x.1.clone()).collect::<Vec<_>>(),
        "chosen": d.chosen, "bad": d.bad,
        // How much is typed in each field, by name (never what).
        "typed": d.secrets.iter().flatten().filter_map(|x| d.pins.get(&x.0).map(|p| (x.1.clone(), json!(p.chars().count())))).collect::<serde_json::Map<String, Value>>(), "duration": d.duration, "busy": d.busy, "err": d.err, "unsupported": d.unsupported,
        "grants": a.grants.get(&d.chat).and_then(|g| g.as_array()).cloned().unwrap_or_default().iter().map(|g| json!({ "name": s(g, "name"), "when": secrets::when(g.get("expiresAt").and_then(|e| e.as_str()), chrono::Local::now()) })).collect::<Vec<_>>(),
    }))).flatten().unwrap_or(Value::Null)
}

/// The test driver ticks a secret by its name, or types in its field.
pub fn sec_by_name(name: &str) -> String {
    with(|a| a.sec.as_ref().and_then(|d| d.secrets.iter().flatten().find(|x| x.1 == name).map(|x| x.0.clone()))).flatten().unwrap_or_default()
}

// ─── The slash menu (2.11, src/slash.rs) ───

/// The box was edited: the menu opens on a whole `/word` with matches (the
/// first selected), else closes.
fn slash_edit(key: &str, text: &str) {
    let changed = with(|a| {
        let items = chat_of(key).and_then(|id| a.chats.get(id)).zip(slash::query(text))
            .map(|(cs, q)| slash::filter(&cs.conv.slash, q)).unwrap_or_default();
        if items.is_empty() { return a.slash.remove(key).is_some(); }
        let rows: Vec<SlashItem> = items.into_iter().map(|(n, d)| SlashItem { name: format!("/{n}").into(), desc: d.into() }).collect();
        match a.slash.get_mut(key) {
            Some((m, sel)) => { m.set_vec(rows); *sel = 0; }
            None => { a.slash.insert(key.to_string(), (Rc::new(VecModel::from(rows)), 0)); }
        }
        true
    }).unwrap_or(false);
    if changed { render(); }
}

/// ↑ ↓ in an open menu: one step, stopping at the ends.
fn slash_move(key: &str, by: i32) {
    with(|a| if let Some((m, sel)) = a.slash.get_mut(key) { *sel = slash::moved(*sel, by, m.row_count()); });
    render();
}

/// Enter, Tab or a press on an item: the box becomes "/name " (the caret at
/// the end, the draft saved), and the menu closes.
fn slash_choose(key: &str, index: i32) {
    let done = with(|a| {
        let (m, sel) = a.slash.remove(key)?;
        let i = if index < 0 { sel } else { index as usize };
        let name = m.row_data(i)?.name.to_string();
        let text = slash::chosen(name.trim_start_matches('/'));
        set_box(a, key, &text, text.len(), true);
        Some(())
    }).flatten();
    if done.is_some() { show_box(); render(); write_drafts_soon(); }
}

/// The test driver's view of a pane's menu: its names and the selection.
pub fn slash_of(key: &str) -> Value {
    with(|a| a.slash.get(key).map(|(m, sel)| json!({ "items": m.iter().map(|i| i.name.to_string()).collect::<Vec<_>>(), "descs": m.iter().map(|i| i.desc.to_string()).collect::<Vec<_>>(), "sel": sel }))).flatten().unwrap_or(Value::Null)
}

// ─── Scheduled messages (2.10, src/sched.rs) ───

/// The clock: the panel opens (with the composer's text, At a time, now +
/// 30 minutes, the list loading) or closes. Nothing of the form is kept.
pub fn sched_toggle(key: &str) {
    if chat_of(key).is_none() { return; }
    let opened = with(|a| {
        if a.sched.get(key).is_some_and(|sc| sc.open) {
            if let Some(sc) = a.sched.get_mut(key) { sc.open = false; }
            return false;
        }
        let draft = a.drafts.get(key).cloned().unwrap_or_default();
        let (tg, ag) = a.sched.get(key).map(|sc| (sc.text_gen, sc.at_gen)).unwrap_or((0, 0));
        a.sched.insert(key.to_string(), Sched {
            open: true, text: draft, text_gen: tg + 1, trigger: "time".into(),
            at: sched::default_at(chrono::Local::now()), at_gen: ag + 1, note: String::new(),
            items: Rc::new(VecModel::default()), warm: None, list: String::new(), unsupported: false, loaded: false,
        });
        true
    }).unwrap_or(false);
    render();
    if opened { sched_load(key); }
}

/// The list (this chat's pending items) and whether the warmer is on.
fn sched_load(key: &str) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let project = with(|a| chat_project(a, &id)).unwrap_or_default();
    let stub = crate::testctl::take_stub("sched-list");
    let k = key.to_string();
    let path = format!("/api/chat-schedule?project={}&chat={}", enc(&project), enc(&id));
    let Some(api) = api() else { return };
    bg(move || match stub { Some(v) => stub_result(v), None => api.get(&path) }, move |r| {
        with(|a| {
            let Some(sc) = a.sched.get_mut(&k) else { return };
            sc.loaded = true;
            match r {
                Ok(v) => {
                    let items: Vec<SchedItem> = v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default().iter().enumerate().map(|(i, it)| SchedItem {
                        id: s(it, "id").into(), when: sched::when_label(it).into(), text: s(it, "text").into(), next: i == 0,
                    }).collect();
                    sc.list = if items.is_empty() { "Nothing scheduled for this project.".into() } else { String::new() };
                    sc.items.set_vec(items);
                    let warm = v.get("limitWarmup").and_then(|w| w.as_bool()).unwrap_or(false);
                    sc.warm = Some(warm);
                    if !warm && sc.trigger == "limit-reset" { sc.trigger = "time".into(); sc.note.clear(); }
                }
                // An older server: no limit warmer to speak of either (the web
                // reads limitWarmup missing as off).
                Err(ApiError::Status(404, _)) => { sc.unsupported = true; sc.warm = Some(false); if sc.trigger == "limit-reset" { sc.trigger = "time".into(); } sc.list = "Scheduled messages are not supported by this server.".into(); sc.items.set_vec(vec![]); }
                Err(ApiError::SignedOut) => {}
                Err(_) => { sc.list = "Could not load scheduled messages.".into(); sc.items.set_vec(vec![]); }
            }
        });
        render();
    });
}

/// A test stub as a server answer: an `ok:false` with a status is that error.
fn stub_result(v: Value) -> api::ApiResult {
    match v.get("status").and_then(|x| x.as_u64()) {
        Some(st) if st >= 400 => Err(ApiError::Status(st as u16, s(&v, "error"))),
        _ if v.get("net").is_some() => Err(ApiError::Net(s(&v, "net"))),
        _ => Ok(v),
    }
}

/// A query parameter, percent-encoded.
fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

pub fn sched_trigger(key: &str, t: &str) {
    with(|a| if let Some(sc) = a.sched.get_mut(key) {
        if t == "limit-reset" && sc.warm == Some(false) { return; }
        sc.trigger = t.to_string();
        sc.note = sched::trigger_note(t).to_string();
    });
    render();
}

/// Schedule: checked here (an empty message, no time), then the server's
/// word. Done: the form's text and note clear (trigger and time stay), the
/// composer empties if it holds the same text, the list reloads.
pub fn sched_save(key: &str) {
    let Some(id) = chat_of(key).map(str::to_string) else { return };
    let req = with(|a| {
        let project = chat_project(a, &id);
        let sc = a.sched.get_mut(key)?;
        if sc.unsupported { return None; }
        match sched::check(&sc.text, &sc.trigger, &sc.at) {
            Err(n) => { sc.note = n.into(); None }
            Ok(trigger) => Some(json!({ "project": project, "text": sc.text.trim(), "trigger": trigger, "chat": id })),
        }
    }).flatten();
    render();
    let Some(body) = req else { return };
    let stub = crate::testctl::take_stub("sched-save");
    let Some(api) = api() else { return };
    let (k, sent) = (key.to_string(), s(&body, "text"));
    bg(move || match stub { Some(v) => stub_result(v), None => api.post("/api/chat-schedule", &body) }, move |r| {
        let ok = with(|a| {
            let note = match &r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some({ let e = s(v, "error"); if e.is_empty() { "Could not schedule that.".to_string() } else { e } }),
                Ok(_) => None,
                Err(ApiError::Status(404, _)) => Some("Not supported by this server.".into()),
                Err(ApiError::Status(_, e)) if !e.is_empty() => Some(e.clone()),
                Err(ApiError::Status(..)) => Some("Could not schedule that.".into()),
                Err(e) => Some(format!("Could not schedule that: {e}")),
            };
            let Some(sc) = a.sched.get_mut(&k) else { return false };
            match note {
                Some(n) => { sc.note = n; false }
                None => {
                    sc.text.clear();
                    sc.text_gen += 1;
                    sc.note.clear();
                    // The composer, holding what was scheduled, empties.
                    if a.drafts.get(&k).is_some_and(|d| d.trim() == sent) { set_box(a, &k, "", 0, false); }
                    true
                }
            }
        }).unwrap_or(false);
        show_box();
        render();
        if ok { write_drafts_soon(); sched_load(&k); }
    });
}

/// ✕ on an item: no confirmation; the list reloads whatever the answer.
pub fn sched_delete(key: &str, item: String) {
    let Some(api) = api() else { return };
    let k = key.to_string();
    bg(move || api.delete(&format!("/api/chat-schedule/{}", enc(&item))), move |_| sched_load(&k));
}

/// The test driver types into the panel's message box or its time field.
pub fn sched_set(key: &str, field: &str, text: &str) {
    with(|a| if let Some(sc) = a.sched.get_mut(key) {
        if field == "at" { sc.at = text.into(); sc.at_gen += 1; } else { sc.text = text.into(); sc.text_gen += 1; }
    });
    render();
}

/// The test driver's view of a pane's panel.
pub fn sched_of(key: &str) -> Value {
    with(|a| a.sched.get(key).map(|sc| json!({
        "open": sc.open, "text": sc.text, "trigger": sc.trigger, "at": sc.at, "note": sc.note, "list": sc.list,
        "limitOff": sc.warm == Some(false), "unsupported": sc.unsupported, "loaded": sc.loaded,
        "items": sc.items.iter().map(|i| json!({ "id": i.id.as_str(), "when": i.when.as_str(), "text": i.text.as_str(), "next": i.next })).collect::<Vec<_>>(),
    }))).flatten().unwrap_or(Value::Null)
}

/// An error row in a chat's log (the client's own).
fn push_error(a: &mut App, id: &str, error: &str) {
    a.local_errors += 1;
    let n = a.local_errors;
    if let Some(cs) = a.chats.get_mut(id) { cs.conv.entries.push(json!({ "id": format!("local-error-{n}"), "kind": "error", "text": error })); }
}

/// Held lines go out as one message once their chat no longer needs input
/// (the prompt answered, or the turn over: the web's ticker and idle
/// flush). An exited chat's go back into its box instead (the web loses
/// them: a named difference).
fn flush_held() {
    with(|a| a.flush_pending = false);
    let due: Vec<(String, String, String)> = with(|a| {
        let ids: Vec<String> = a.held.keys().cloned().collect();
        ids.into_iter().filter_map(|id| {
            let st = chat_state_now(a, &id);
            if st == "needs-input" { return None; }
            let text = a.held.remove(&id)?.join("\n");
            sync_held(a, &id);
            Some((id, text, st))
        }).collect()
    }).unwrap_or_default();
    for (id, text, st) in due {
        let key = chat_key(&id);
        if st == "exited" {
            with(|a| { let cur = a.drafts.get(&key).cloned().unwrap_or_default(); let back = chat::given_back(&text, &cur); set_box(a, &key, &back, back.len(), false); });
            show_box();
            render();
            write_drafts_soon();
        } else {
            post_send(&key, &id, text, true);
        }
    }
}

/// Take back (2.9): a click on a drawer, or ↑ on the box's first row. The
/// held lines at once, and every handed-over message that can still be
/// recalled (POST /cancel-queued each); then the box holds the cancelled
/// texts, the held text, then its own. False: nothing to take back.
pub fn take_back(key: &str) -> bool {
    let Some(id) = chat_of(key).map(str::to_string) else { return false };
    let (held, recall) = with(|a| {
        let held = a.held.remove(&id).map(|h| h.join("\n")).unwrap_or_default();
        sync_held(a, &id);
        (held, a.chats.get(&id).map(|cs| cs.conv.recallable()).unwrap_or_default())
    }).unwrap_or_default();
    if held.is_empty() && recall.is_empty() { return false; }
    if recall.is_empty() { taken_back(key, &id, vec![], held, None); return true; }
    render();
    let Some(api) = api() else { return true };
    let stubs: Vec<Option<Value>> = recall.iter().map(|_| crate::testctl::take_stub("cancel-queued")).collect();
    let (k, id2) = (key.to_string(), id.clone());
    bg(move || {
        recall.into_iter().zip(stubs).map(|((uuid, text), stub)| {
            let r = match stub { Some(v) => Ok(v), None => api.post(&format!("/api/ui-sessions/{id2}/cancel-queued"), &json!({ "uuid": uuid })) };
            let r = match r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Err({ let e = s(&v, "error"); if e.is_empty() { "Could not take it back".to_string() } else { e } }),
                Ok(v) => Ok(v.get("cancelled").and_then(|c| c.as_bool()).unwrap_or(false)),
                Err(ApiError::Status(404, _)) => Err("Taking back is not supported by this server".into()),
                Err(e) => Err(e.to_string()),
            };
            (uuid, text, r)
        }).collect::<Vec<_>>()
    }, move |results| {
        let mut cancelled = vec![];
        let mut error = None;
        with(|a| if let Some(cs) = a.chats.get_mut(&id) {
            for (uuid, text, r) in results {
                match r {
                    Ok(true) => { cs.conv.slots.retain(|x| x.uuid != uuid); cancelled.push(text); }
                    // Too late: the CLI has it (no error).
                    Ok(false) => { if let Some(x) = cs.conv.slots.iter_mut().find(|x| x.uuid == uuid) { x.state = "started".into(); } }
                    Err(e) => { error.get_or_insert(e); }
                }
            }
        });
        taken_back(&k, &id, cancelled, held, error);
    });
    true
}

fn taken_back(key: &str, id: &str, cancelled: Vec<String>, held: String, error: Option<String>) {
    with(|a| {
        let cur = a.drafts.get(key).cloned().unwrap_or_default();
        let new = chat::taken_back(&cancelled, &held, &cur);
        set_box(a, key, &new, new.len(), true);
        if let Some(e) = &error { push_error(a, id, e); }
    });
    show_box();
    if error.is_some() { show_chat(id); }
    render();
    write_drafts_soon();
}

/// Quitting: every chat's held lines go into its draft (the web loses them:
/// a named difference), to be back in the box after a restart.
pub fn park_held() {
    with(|a| {
        let held: Vec<(String, Vec<String>)> = a.held.drain().collect();
        for (id, lines) in held {
            sync_held(a, &id);
            let key = chat_key(&id);
            let cur = a.drafts.get(&key).cloned().unwrap_or_default();
            save_draft_in(a, &key, &chat::given_back(&lines.join("\n"), &cur));
        }
    });
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
    // One write pending at a time, not one per keystroke.
    if with(|a| std::mem::replace(&mut a.drafts_timer, true)).unwrap_or(true) { return; }
    slint::Timer::single_shot(Duration::from_millis(350), || {
        with(|a| a.drafts_timer = false);
        flush_drafts();
    });
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
        let mut found = a.saved_drafts.restore(&id, &alias, now);
        // Held lines an earlier run left (it quit or was stopped): in front of
        // the draft, as a give-back (2.9).
        if !a.held.contains_key(&id) {
            if let Some(h) = a.saved_drafts.take_held(&id) {
                let t = chat::given_back(&h, found.as_deref().unwrap_or(""));
                a.saved_drafts.save(&id, &alias, &t, now);
                found = Some(t);
            }
        }
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
        let Some(path) = crate::testctl::take_pick().or_else(|| rfd::FileDialog::new().pick_file()) else { return };
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
        set_box(a, key, &new, caret, true);
        Some(())
    }).flatten();
    if done.is_none() { return; }
    show_box();
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
        // The focused chat, found first: focused_chat() borrows the app too
        // (a nested borrow panicked: F59).
        let focused = chat_key(&focused_chat());
        let (paths, key) = with(|a| {
            let key = a.full.clone().or_else(|| a.file_target.clone()).unwrap_or(focused);
            (std::mem::take(&mut a.dropped), key)
        }).unwrap_or_default();
        file_hover(false);
        drop_files(&key, paths, false);
    });
}

/// Ctrl+V (or Shift+Insert) in a chat's box: a picture (as image.png) or a
/// copied file uploads, and the key is taken; anything else is the ordinary
/// paste. Slint's input pastes on Ctrl+V only, so Shift+Insert's text is put
/// in here, at the caret.
fn paste(key: &str, shift_insert: bool) -> bool {
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
    if shift_insert {
        if let Ok(t) = cb.get_text() { put_text(key, &t); }
        return true;
    }
    false
}

/// Text into a chat's box at its caret, replacing a selection, as typed
/// (a paste).
fn put_text(key: &str, text: &str) {
    let done = with(|a| {
        let cur = a.drafts.get(key).cloned().unwrap_or_default();
        let (c, an) = a.carets.get(key).copied().unwrap_or((cur.len(), cur.len()));
        let fix = |i: usize| { let mut i = i.min(cur.len()); while !cur.is_char_boundary(i) { i -= 1; } i };
        let (s0, s1) = (fix(c.min(an)), fix(c.max(an)));
        let new = format!("{}{}{}", &cur[..s0], text, &cur[s1..]);
        set_box(a, key, &new, s0 + text.len(), true);
    });
    if done.is_none() { return; }
    show_box();
    write_drafts_soon();
}

/// The test driver's clipboard: kept open, since on X11 the clipboard's
/// contents live as long as their owner.
pub fn keep_clipboard(cb: arboard::Clipboard) {
    thread_local! { static KEEP: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) }; }
    KEEP.with(|k| *k.borrow_mut() = Some(cb));
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
    with(|a| { let t = a.drafts.get(key).cloned().unwrap_or_default(); set_box(a, key, &t, at, true); a.carets.insert(key.to_string(), (at, to)); });
    show_box();
}

/// A pane's upload bar, for the test driver: (shown, name, label, busy).
pub fn upload_of(key: &str) -> (bool, String, String, bool) {
    with(|a| a.uploads.get(key).map(|u| (u.shown, u.name.clone(), u.label.clone(), u.active > 0))).flatten().unwrap_or_default()
}

/// The saved drafts of the server signed in to, for the test driver.
pub fn saved_drafts() -> Value {
    with(|a| { drafts_for_server(a); a.saved_drafts.to_saved() }).unwrap_or(Value::Null)
}

/// The test driver plays a ui-event into a chat, as the server would.
pub fn play_event(id: &str, event: Value) {
    on_chat_message(&json!({ "type": "ui-event", "id": id, "event": event }));
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

// ─── Secret requests (2.12b, src/sreq.rs, ui/sreq.slint) ───

/// The requests waiting on the owner, which card holds the field, and that
/// card's state. What is typed lives here only while its card is armed.
pub struct Sreq {
    list: Vec<sreq::Req>,
    armed: Option<String>,
    pin: String,
    allow_on: bool,
    /// The last duration chosen (kept across cards while the app runs).
    duration: String,
    err: String,
    busy: bool,
    /// Armed by a click (its field takes the focus), not automatically.
    focus_armed: bool,
    pin_gen: i32,
    sel_gen: i32,
    cards: Rc<VecModel<SreqCard>>,
}

impl Default for Sreq {
    fn default() -> Self {
        Sreq { list: vec![], armed: None, pin: String::new(), allow_on: false, duration: "1h".into(), err: String::new(), busy: false, focus_armed: false, pin_gen: 0, sel_gen: 0, cards: Rc::new(VecModel::default()) }
    }
}

impl Sreq {
    /// What was typed: overwritten, then dropped; the field emptied.
    fn forget(&mut self) {
        // SAFETY of meaning, not memory: the bytes are overwritten in place
        // before the string is emptied, so the lock does not linger.
        let n = self.pin.len();
        self.pin.clear();
        self.pin.extend(std::iter::repeat('\0').take(n));
        self.pin.clear();
        self.pin.shrink_to_fit();
        self.pin_gen += 1;
    }
    /// A new card holds the field: nothing typed, unticked, no error.
    fn arm(&mut self, id: Option<String>, focus: bool) {
        if self.armed != id { self.forget(); }
        self.armed = id;
        self.allow_on = false;
        self.err.clear();
        self.focus_armed = focus;
    }
}

/// The list at start (and after a reconnect): GET /api/secrets.
fn load_sreqs() {
    call(|api| api.get("/api/secrets"), |r| {
        if let Ok(v) = r { sreq_list(v.get("requests").unwrap_or(&Value::Null)); }
    });
}

/// The server's whole list (start, or its `secret-requests` event). The armed
/// card stays armed while its request is there (what is typed, the tick and
/// the error with it); else the first is armed, without the focus.
fn sreq_list(v: &Value) {
    let list = sreq::parse(v);
    with(|a| {
        let r = &mut a.sreq;
        match sreq::armed(&list, r.armed.as_deref()) {
            Some((_, true)) => {}
            Some((id, false)) => r.arm(Some(id), false),
            None => r.arm(None, false),
        }
        r.list = list;
    });
    render();
}

/// The cards: rows rewritten only where they changed (a redraw must not
/// take the focus or what is typed from the armed field).
fn show_sreq(w: &MainWindow, a: &App) {
    let r = &a.sreq;
    let rows: Vec<SreqCard> = r.list.iter().map(|q| SreqCard {
        id: q.id.as_str().into(), run: q.run, name: q.name.as_str().into(), project: q.project.as_str().into(), reason: q.reason.as_str().into(),
        command: q.command.as_str().into(), env: q.env.as_str().into(), lock: q.lock.as_str().into(), armed: r.armed.as_deref() == Some(q.id.as_str()),
    }).collect();
    let m = &r.cards;
    // Gone: removed where they are; new: at the end (the server's order is
    // oldest first); then the changed ones in place.
    let mut i = 0;
    while i < m.row_count() {
        let id = m.row_data(i).map(|c| c.id).unwrap_or_default();
        if rows.iter().any(|c| c.id == id) { i += 1; } else { m.remove(i); }
    }
    let same_order = m.iter().zip(rows.iter()).all(|(x, y)| x.id == y.id);
    if !same_order { m.set_vec(rows.clone()); }
    for (k, c) in rows.iter().enumerate() {
        if k < m.row_count() { if m.row_data(k).as_ref() != Some(c) { m.set_row_data(k, c.clone()); } } else { m.push(c.clone()); }
    }
    let armed = r.armed.as_deref().and_then(|id| r.list.iter().find(|q| q.id == id));
    let collab = armed.map(|q| a.sessions.iter().chain(a.terminals.iter().flatten()).find(|c| s(c, "id") == q.session).and_then(|c| c.get("collaborator")).is_some_and(truthy)).unwrap_or(false);
    w.set_sreq_allow(armed.is_some_and(|q| sreq::can_allow(q, a.secrets_cap, collab)));
    w.set_sreq_allow_on(r.allow_on);
    w.set_sreq_duration(r.duration.as_str().into());
    w.set_sreq_err(r.err.as_str().into());
    w.set_sreq_busy(r.busy);
    w.set_sreq_pin(r.pin.as_str().into());
    w.set_sreq_pin_gen(r.pin_gen);
    w.set_sreq_focus_armed(r.focus_armed);
    w.set_sreq_sel_gen(r.sel_gen);
}

/// Approve on a card without the field: it takes the field, and the focus.
fn sreq_arm(id: &str) {
    with(|a| if a.sreq.list.iter().any(|q| q.id == id) && a.sreq.armed.as_deref() != Some(id) {
        a.sreq.arm(Some(id.to_string()), true);
    });
    render();
}

fn sreq_typed(text: &str) {
    let changed = with(|a| {
        let lock = a.sreq.armed.as_deref().and_then(|id| a.sreq.list.iter().find(|q| q.id == id)).map(|q| q.lock.clone()).unwrap_or_default();
        let clean = secrets::typed(&lock, text);
        let changed = clean != text;
        a.sreq.pin = clean;
        if changed { a.sreq.pin_gen += 1; }
        changed
    }).unwrap_or(false);
    if changed { render(); }
}

/// Approve on the armed card (or Enter in its field): the local check, then
/// the server. A refusal keeps the text (focused, selected) and says why.
fn sreq_confirm() {
    let job = with(|a| {
        let caps = a.secrets_cap;
        let r = &mut a.sreq;
        if r.busy { return None; }
        let q = r.armed.as_deref().and_then(|id| r.list.iter().find(|q| q.id == id))?.clone();
        if let Err(e) = sreq::check(&q.lock, &r.pin) {
            r.err = e.to_string();
            r.sel_gen += 1;
            return Some(Err(()));
        }
        let collab = a.sessions.iter().chain(a.terminals.iter().flatten()).find(|c| s(c, "id") == q.session).and_then(|c| c.get("collaborator")).is_some_and(truthy);
        let r = &mut a.sreq;
        let allow = (r.allow_on && sreq::can_allow(&q, caps, collab)).then(|| r.duration.clone());
        let body = sreq::approve_body(&q, &r.pin, allow.as_deref());
        r.busy = true;
        Some(Ok((q.id.clone(), body, allow.is_some())))
    }).flatten();
    let Some(job) = job else { return };
    render();
    let Ok((id, mut body, allow)) = job else { return };
    let Some(api) = api() else { return };
    bg(move || {
        let r = post_raw(&api, "/api/secrets", &body).map_err(|e| e.to_string());
        // The lock in the body: overwritten once sent.
        if let Some(p) = body.get_mut("pin") { *p = json!(""); }
        r
    }, move |r| {
        match sreq::answer(r, allow) {
            Ok(msg) => {
                with(|a| {
                    a.sreq.busy = false;
                    // The next card is armed (not focused): the server's event
                    // takes this one away.
                    let next = a.sreq.list.iter().find(|q| q.id != id).map(|q| q.id.clone());
                    if a.sreq.armed.as_deref() == Some(id.as_str()) { a.sreq.arm(next, false); }
                    a.sreq.list.retain(|q| q.id != id);
                });
                toast_ok(msg);
            }
            Err(e) => with(|a| {
                a.sreq.busy = false;
                if a.sreq.armed.as_deref() == Some(id.as_str()) { a.sreq.err = e; a.sreq.sel_gen += 1; }
            }).unwrap_or(()),
        }
        render();
    });
}

/// Reject, on any card: at once, no confirmation. The server's event takes
/// the card away; a failure says so (the web says nothing).
fn sreq_reject(id: String) {
    let Some(api) = api() else { return };
    with(|a| if a.sreq.armed.as_deref() == Some(id.as_str()) { a.sreq.err.clear(); });
    let body = json!({ "action": "deny", "reqId": id });
    bg(move || post_raw(&api, "/api/secrets", &body), move |r| match r {
        Ok((_, v)) if v.get("ok").and_then(|o| o.as_bool()) != Some(false) => {
            with(|a| {
                if a.sreq.armed.as_deref() == Some(id.as_str()) {
                    let next = a.sreq.list.iter().find(|q| q.id != id).map(|q| q.id.clone());
                    a.sreq.arm(next, false);
                }
                a.sreq.list.retain(|q| q.id != id);
            });
            toast("Rejected", false);
            render();
        }
        Ok((_, v)) => toast(v.get("error").and_then(|e| e.as_str()).unwrap_or("Could not reject it"), true),
        Err(e) => toast(&e.to_string(), true),
    });
}

/// The test driver's view of the dock.
pub fn sreq_of() -> Value {
    with(|a| {
        let r = &a.sreq;
        if r.list.is_empty() { return Value::Null; }
        json!({
            "cards": r.list.iter().map(|q| json!({ "id": q.id, "name": q.name, "run": q.run, "command": q.command, "armed": r.armed.as_deref() == Some(q.id.as_str()) })).collect::<Vec<_>>(),
            "armed": r.armed, "err": r.err, "busy": r.busy, "allowOn": r.allow_on, "duration": r.duration,
            // How much is typed (never what).
            "typed": r.pin.chars().count(),
        })
    }).unwrap_or(Value::Null)
}

/// The test driver: a card by its secret's name (the first such).
pub fn sreq_by_name(name: &str) -> String {
    with(|a| a.sreq.list.iter().find(|q| q.name == name).map(|q| q.id.clone())).flatten().unwrap_or_default()
}

/// The test driver: the armed card's field takes the focus (as a click on it).
pub fn sreq_focus() {
    with(|a| a.sreq.sel_gen += 1);
    render();
}

pub fn sreq_approve_by(name: &str) { let id = sreq_by_name(name); if with(|a| a.sreq.armed.as_deref() == Some(id.as_str())).unwrap_or(false) { sreq_confirm(); } else { sreq_arm(&id); } }
pub fn sreq_reject_by(name: &str) { let id = sreq_by_name(name); if !id.is_empty() { sreq_reject(id); } }
pub fn sreq_toggle_allow() { with(|a| a.sreq.allow_on = !a.sreq.allow_on); render(); }
pub fn sreq_set_duration(d: &str) { with(|a| a.sreq.duration = d.to_string()); render(); }

// ─── Question cards (2.13, src/questions.rs, ui/chat.slint QuestionCard) ───

/// One card while it can be answered: its questions, the picks, and the
/// models the view draws (rows rewritten only where they changed).
struct QCard {
    qs: Vec<questions::Q>,
    picks: Vec<questions::Pick>,
    model: Rc<VecModel<QView>>,
    opts: Vec<Rc<VecModel<QOpt>>>,
    /// Counters: Other set again (a pick cleared it); a choice asked to take
    /// the keyboard (an arrow moved there).
    other_gen: Vec<i32>,
    other_focus: Vec<i32>,
    focus_gen: HashMap<(usize, usize), i32>,
    /// The note opened by its button (it takes the focus when it appears).
    note_focus: Option<(usize, String)>,
}

impl QCard {
    fn new(qs: Vec<questions::Q>, picks: Vec<questions::Pick>) -> QCard {
        let n = qs.len();
        let opts: Vec<Rc<VecModel<QOpt>>> = qs.iter().map(|_| Rc::new(VecModel::default())).collect();
        let mut k = QCard { qs, picks, model: Rc::new(VecModel::default()), opts, other_gen: vec![0; n], other_focus: vec![0; n], focus_gen: HashMap::new(), note_focus: None };
        k.picks.resize(n, questions::Pick::default());
        k.refresh();
        k
    }

    fn label(&self, qi: i32, oi: i32) -> String {
        self.qs.get(qi as usize).and_then(|q| q.options.get(oi as usize)).map(|o| o.label.clone()).unwrap_or_default()
    }

    /// The view from the picks: only the rows that differ are rewritten.
    fn refresh(&mut self) {
        for (qi, q) in self.qs.iter().enumerate() {
            let p = &self.picks[qi];
            let rows: Vec<QOpt> = q.options.iter().enumerate().map(|(oi, o)| QOpt {
                label: o.label.as_str().into(), desc: o.desc.as_str().into(), preview: o.preview.as_str().into(),
                chosen: p.chosen.contains(&o.label),
                note_open: !p.note_opt.is_empty() && p.note_opt == o.label,
                note: p.note.as_str().into(),
                note_focus: self.note_focus.as_ref().is_some_and(|(nq, l)| *nq == qi && *l == o.label),
                focus_gen: self.focus_gen.get(&(qi, oi)).copied().unwrap_or(0),
            }).collect();
            let m = &self.opts[qi];
            for (oi, r) in rows.iter().enumerate() {
                if oi < m.row_count() { if m.row_data(oi).as_ref() != Some(r) { m.set_row_data(oi, r.clone()); } } else { m.push(r.clone()); }
            }
            while m.row_count() > rows.len() { m.remove(m.row_count() - 1); }
            let v = QView {
                header: q.header.to_uppercase().into(), question: q.question.as_str().into(), multi: q.multi,
                options: ModelRc::from(m.clone()), other: p.other.as_str().into(), other_gen: self.other_gen[qi], other_focus: self.other_focus[qi],
            };
            if qi < self.model.row_count() { if self.model.row_data(qi).as_ref() != Some(&v) { self.model.set_row_data(qi, v); } } else { self.model.push(v); }
        }
        while self.model.row_count() > self.qs.len() { self.model.remove(self.model.row_count() - 1); }
    }

    fn can(&self) -> bool { questions::can_submit(&self.qs, &self.picks) }
}

/// A question entry's row: answered, its tags; open, its card (made once,
/// with the picks of its answer draft).
fn q_view(cards: &mut HashMap<String, QCard>, drafts: &Drafts, chat: &str, e: &Shown, v: &mut EntryView) {
    let status = s(&e.raw, "status");
    v.chat = chat.into();
    v.q_open = questions::open(&status);
    v.q_orphaned = questions::orphaned(&status);
    if !v.q_open {
        let d = questions::done(&e.raw);
        v.q_tags = ModelRc::new(VecModel::from(d.tags.iter().map(|t| SharedString::from(t.as_str())).collect::<Vec<_>>()));
        v.q_notes = ModelRc::new(VecModel::from(d.notes.iter().map(|t| SharedString::from(t.as_str())).collect::<Vec<_>>()));
        v.q_dismissed = d.dismissed;
        v.q_by_cru = d.by_cru;
        return;
    }
    let qs = questions::parse(e.raw.get("input").unwrap_or(&Value::Null));
    let key = format!("{chat}:{}", e.id);
    let fresh = || {
        let picks = drafts.answer(chat, &e.id).map(|d| questions::from_value(d, &qs)).unwrap_or_default();
        QCard::new(qs.clone(), picks)
    };
    let card = cards.entry(key).or_insert_with(fresh);
    if card.qs != qs { *card = fresh(); }
    v.questions = ModelRc::from(card.model.clone());
    v.q_can = card.can();
}

/// A change to a card's picks: drawn, its Submit re-checked, the answer
/// draft saved (or removed when nothing is picked).
fn q_act(chat: &str, entry: &str, f: impl FnOnce(&mut QCard)) {
    let key = format!("{chat}:{entry}");
    let done = with(|a| {
        drafts_for_server(a);
        let card = a.qcards.get_mut(&key)?;
        f(card);
        card.refresh();
        let can = card.can();
        let saved = (!questions::blank(&card.picks)).then(|| questions::to_value(&card.picks));
        a.saved_drafts.set_answer(chat, entry, saved, now_ms());
        a.drafts_dirty = true;
        // The row's Submit (its questions' model is the same: nothing rebuilt).
        if let Some(cs) = a.chats.get(chat) {
            if let Some(i) = (0..cs.model.row_count()).find(|i| cs.model.row_data(*i).is_some_and(|r| r.id == entry && r.kind == "question")) {
                let mut r = cs.model.row_data(i).unwrap_or_default();
                if r.q_can != can { r.q_can = can; cs.model.set_row_data(i, r); }
            }
        }
        Some(())
    }).flatten();
    if done.is_some() { write_drafts_soon(); }
}

/// Submit (or Enter in Other) and Dismiss. A live question goes to
/// `respond`; an orphaned one to `answer-closed`. The card shows its answer
/// at once (no double answers) and its draft goes. A refused `respond`
/// puts the card back with an error (the web leaves it looking answered).
fn q_answer(chat: String, entry: String, dismiss: bool) {
    let key = format!("{chat}:{entry}");
    let job = with(|a| {
        drafts_for_server(a);
        let card = a.qcards.get(&key)?;
        if !dismiss && !card.can() { return None; }
        let cs = a.chats.get_mut(&chat)?;
        let e = cs.conv.entries.iter_mut().find(|x| s(x, "id") == entry)?;
        let status = s(e, "status");
        if !questions::open(&status) { return None; }
        let input = e.get("input").cloned().unwrap_or(json!({}));
        let prev = e.clone();
        let orphaned = questions::orphaned(&status);
        let body;
        if orphaned {
            let answers = if dismiss { json!({}) } else { questions::answers(&card.qs, &card.picks) };
            let text = if dismiss { String::new() } else { questions::closed_text(&card.qs, &card.picks) };
            let mut ai = input.clone();
            ai["answers"] = answers.clone();
            e["status"] = json!("answered-late");
            e["answeredInput"] = ai;
            body = json!({ "entryId": entry, "answers": answers, "text": text });
        } else if dismiss {
            e["status"] = json!("denied");
            body = json!({ "requestId": s(e, "requestId"), "behavior": "deny", "message": "The user dismissed the question." });
        } else {
            let upd = questions::updated_input(&input, &card.qs, &card.picks);
            e["status"] = json!("allowed");
            e["answeredInput"] = upd.clone();
            body = json!({ "requestId": s(e, "requestId"), "behavior": "allow", "updatedInput": upd });
        }
        a.saved_drafts.set_answer(&chat, &entry, None, now_ms());
        a.drafts_dirty = true;
        Some((orphaned, body, prev))
    }).flatten();
    let Some((orphaned, body, prev)) = job else { return };
    show_chat(&chat);
    render();
    write_drafts_soon();
    let Some(api) = api() else { return };
    let path = format!("/api/ui-sessions/{chat}/{}", if orphaned { "answer-closed" } else { "respond" });
    bg(move || post_raw(&api, &path, &body), move |r| {
        let err = match r {
            Ok((404, _)) if orphaned => Some("Answering an old question is not supported by this server".to_string()),
            Ok((_, v)) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(v.get("error").and_then(|e| e.as_str()).unwrap_or("The answer was not taken").to_string()),
            Ok((st, _)) if st >= 400 => Some(format!("The answer was not taken (HTTP {st})")),
            Ok(_) => None,
            Err(e) => Some(e.to_string()),
        };
        let Some(err) = err else { return };
        // A refused answer: the card back as it was (its picks and draft
        // with it); an orphaned card stays answered (the server marked it).
        if !orphaned {
            with(|a| {
                if let Some(cs) = a.chats.get_mut(&chat) {
                    if let Some(e) = cs.conv.entries.iter_mut().find(|x| s(x, "id") == entry) {
                        if questions::open(s(&prev, "status").as_str()) && !questions::open(&s(e, "status")) && s(e, "status") != s(&prev, "status") { *e = prev.clone(); }
                    }
                }
                if let Some(card) = a.qcards.get(&format!("{chat}:{entry}")) {
                    let saved = (!questions::blank(&card.picks)).then(|| questions::to_value(&card.picks));
                    a.saved_drafts.set_answer(&chat, &entry, saved, now_ms());
                    a.drafts_dirty = true;
                }
            });
            show_chat(&chat);
            chat_error(&chat, err);
        } else {
            // An orphaned answer the server could not pass on (the session
            // gone): an error entry in the conversation, under the card (the
            // web's appendLocalError).
            with(|a| push_error(a, &chat, &err));
            show_chat(&chat);
            render();
        }
    });
}

/// The test driver: a chat's latest question card (its entry, status, the
/// picks as the draft keeps them, Submit, the answered tags).
pub fn q_of(key: &str) -> Value {
    let Some(chat) = chat_of(key).map(str::to_string) else { return Value::Null };
    with(|a| {
        let cs = a.chats.get(&chat)?;
        let e = cs.conv.entries.iter().rev().find(|e| s(e, "kind") == "question")?;
        let id = s(e, "id");
        let status = s(e, "status");
        let card = a.qcards.get(&format!("{chat}:{id}"));
        let d = questions::done(e);
        Some(json!({
            "entry": id, "status": status, "open": questions::open(&status), "orphaned": questions::orphaned(&status),
            "can": card.map(|k| k.can()).unwrap_or(false),
            "picks": card.map(|k| questions::to_value(&k.picks)).unwrap_or(Value::Null),
            "questions": card.map(|k| k.qs.len()).unwrap_or(0),
            "draft": a.saved_drafts.answer(&chat, &id).cloned().unwrap_or(Value::Null),
            "tags": d.tags, "notes": d.notes, "dismissed": d.dismissed,
        }))
    }).flatten().unwrap_or(Value::Null)
}

/// The test driver acts on the focused chat's latest open question card.
pub fn q_drive(what: &str, args: &[String]) {
    let chat = focused_chat();
    let entry = with(|a| a.chats.get(&chat).and_then(|cs| cs.conv.entries.iter().rev().find(|e| s(e, "kind") == "question" && questions::open(&s(e, "status"))).map(|e| s(e, "id")))).flatten().unwrap_or_default();
    if entry.is_empty() { return; }
    let n = |i: usize| args.get(i).and_then(|x| x.parse::<i32>().ok()).unwrap_or(0);
    let t = |i: usize| SharedString::from(args.get(i).cloned().unwrap_or_default());
    let Some(w) = ui() else { return };
    let qa = w.global::<QAct>();
    let (c, e) = (SharedString::from(chat.as_str()), SharedString::from(entry.as_str()));
    match what {
        "pick" => qa.invoke_pick(c, e, n(0), n(1)),
        "note" => qa.invoke_note_open(c, e, n(0), n(1)),
        "note-type" => qa.invoke_note_typed(c, e, n(0), t(1)),
        "note-remove" => qa.invoke_note_remove(c, e, n(0)),
        "other" => qa.invoke_other_typed(c, e, n(0), t(1)),
        "move" => qa.invoke_move(c, e, n(0), n(1), n(2)),
        "submit" => qa.invoke_submit(c, e),
        // The keyboard to a question's Other (as a click in it).
        "other-focus" => q_act(&chat, &entry, |k| { if let Some(f) = k.other_focus.get_mut(n(0) as usize) { *f += 1; } }),
        "dismiss" => qa.invoke_dismiss(c, e),
        _ => {}
    }
}

// ─── Tool cards (2.14, src/tools.rs, ui/chat.slint ToolRow) ───

/// A card's or a group's head clicked: open or closed (kept while the pane
/// shows the chat).
fn tool_toggle(chat: &str, id: &str) {
    with(|a| if let Some(cs) = a.chats.get_mut(chat) {
        // A thinking block's head (2.15): the reader's choice.
        if cs.conv.entries.iter().any(|e| s(e, "id") == id && s(e, "kind") == "thinking") { cs.think.toggle(id); }
        else if !cs.open.remove(id) { cs.open.insert(id.to_string()); }
    });
    show_chat(chat);
}

fn tool_json(t: &Shown, open: bool) -> Value {
    let (kind, args, lines, todos): (&str, String, Vec<Value>, Vec<Value>) = match &t.body {
        tools::Body::Bash { command, .. } => ("bash", command.clone(), vec![], vec![]),
        tools::Body::Write(c) => ("write", c.clone(), vec![], vec![]),
        tools::Body::Diff(l) => ("diff", String::new(), l.iter().map(|(x, add)| json!({ "text": x, "add": add })).collect(), vec![]),
        tools::Body::Todos(t) => ("todos", String::new(), vec![], t.iter().map(|(x, st)| json!({ "text": x, "state": st })).collect()),
        tools::Body::Json => ("json", t.args.clone(), vec![], vec![]),
    };
    json!({ "id": t.id, "name": t.title, "summary": t.detail, "status": t.status, "error": t.error, "open": open, "body": kind, "args": args, "note": t.note, "lines": lines, "todos": todos, "result": t.text })
}

/// The test driver: a chat's tool rows (cards, groups with their members,
/// agent rows), as drawn.
pub fn tools_of(key: &str) -> Value {
    let Some(chat) = chat_of(key).map(str::to_string) else { return Value::Null };
    with(|a| {
        let cs = a.chats.get(&chat)?;
        Some(Value::Array(cs.rows.iter().filter_map(|r| match r {
            Row::Tool(_, t, o) => Some(json!({ "card": tool_json(t, *o), "media": media_json(&chat, t, &a.media) })),
            Row::Tools(id, sum, ms, o) => Some(json!({ "group": id, "summary": sum, "open": o, "count": ms.len(),
                "status": if ms.iter().any(|m| m.0.status == "running") { "running" } else if ms.iter().any(|m| m.0.error) { "failed" } else { "done" },
                "members": ms.iter().map(|(t, o)| tool_json(t, *o)).collect::<Vec<_>>() })),
            Row::Prose(_, t, kind, marker, _) => Some(json!({ "prose": kind, "text": t, "marker": marker })),
            Row::Code(_, t, _) => Some(json!({ "code": t })),
            Row::Entry(e) if matches!(e.kind, "notice" | "error" | "result") => Some(json!({ "entry": e.kind, "text": e.text, "error": e.error, "detail": e.detail })),
            Row::Think(e, o) => Some(json!({ "think": e.id, "open": o, "streaming": e.status == "streaming", "detail": e.detail, "text": e.text })),
            Row::Agent(id, ag) => Some(json!({ "agent": id, "type": s(ag, "subagentType"), "description": s(ag, "description"), "status": s(ag, "status") })),
            _ => None,
        }).collect()))
    }).flatten().unwrap_or(Value::Null)
}

/// The test driver opens or closes a card or group of the focused chat.
pub fn tool_toggle_focused(id: &str) {
    let chat = focused_chat();
    tool_toggle(&chat, id);
}


// ─── The rest of the log (2.16) ───

/// A reply's link: http(s) to the system browser. (Under the test driver it
/// is only recorded.) Anything else (a path: the Files panel, phase 3)
/// does nothing yet.
fn open_link(url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) { return; }
    with(|a| a.last_opened = url.to_string());
    if std::env::var_os("CRUNDI_TEST_PORT").is_some() { return; }
    let u = url.to_string();
    std::thread::spawn(move || { let _ = open::that(&u); });
}

pub fn last_opened() -> String { with(|a| a.last_opened.clone()).unwrap_or_default() }

/// Text to the system clipboard (kept owned: on X11 the copy lives with it).
fn copy_text(text: &str) -> bool {
    let Ok(mut cb) = arboard::Clipboard::new() else { return false };
    if cb.set_text(text.to_string()).is_err() { return false; }
    keep_clipboard(cb);
    true
}

/// A plain toast (the web's "Copied" is an ordinary toast).
fn toast_ok_plain(text: &str) { toast(text, false); }

// ─── Media Claude showed (2.16m, src/media.rs, ui/chat.slint MediaBlock) ───

/// A picture (or poster) fetched and decoded off the UI thread: ready, gone
/// (404), empty (would not decode); a network failure is forgotten, so it
/// is asked for again the next time the row is drawn.
fn fetch_media(key: String) {
    let Some(api) = api() else { return };
    let Some((chat, name)) = key.split_once('|').map(|(c, n)| (c.to_string(), n.to_string())) else { return };
    let path = format!("/api/chat-media?session={chat}&name={name}");
    bg(move || {
        match api.get_bytes(&path) {
            Ok((200, _)) if path.ends_with(".embed") => ("ready", None),
            Ok((200, bytes)) => match image::load_from_memory(&bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let mut pb = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(rgba.width(), rgba.height());
                    pb.make_mut_bytes().copy_from_slice(rgba.as_raw());
                    ("ready", Some(pb))
                }
                Err(_) => ("empty", None),
            },
            Ok((404, _)) | Ok((410, _)) => ("gone", None),
            _ => ("retry", None),
        }
    }, move |(st, pb)| {
        with(|a| {
            if st == "retry" { a.media.remove(&key); } else { a.media.insert(key.clone(), (st.to_string(), pb.map(slint::Image::from_rgba8))); }
            // The rows with media are drawn again (the rest stay).
            if let Some(cs) = a.chats.get_mut(&chat) {
                for r in cs.rows.iter_mut() { if matches!(r, Row::Tool(..) | Row::Tools(..)) { *r = Row::Code(String::new(), String::new(), false); } }
            }
        });
        show_chat(&chat);
        show_viewer();
    });
}

/// The chat's pictures, in order (the viewer's gallery).
fn chat_pictures(chat: &str) -> Vec<String> {
    with(|a| a.chats.get(chat).map(|cs| cs.conv.view().iter().filter(|e| e.kind == "tool" && e.status != "running" && !e.error)
        .filter_map(|e| e.raw.as_str().map(|r| media::items(r).0)).flatten()
        .filter(|i| i.kind == "image").map(|i| format!("{chat}|{}", i.name))
        // Only the pictures that loaded (the web's galleryOf: a gone box has no picture).
        .filter(|k| a.media.get(k).is_some_and(|m| m.0 == "ready")).collect()).unwrap_or_default()).unwrap_or_default()
}

/// A picture clicked: the viewer, on it.
pub fn view_picture(key: &str) {
    let chat = key.split_once('|').map(|x| x.0.to_string()).unwrap_or_default();
    let list = chat_pictures(&chat);
    let at = list.iter().position(|k| k == key).unwrap_or(0);
    if list.is_empty() { return; }
    with(|a| a.viewer = Some((list, at)));
    show_viewer();
}

/// The viewer's step (wrapping round), or closed (None).
pub fn viewer_step(by: i32) {
    with(|a| if let Some((l, at)) = a.viewer.as_mut() { let n = l.len() as i32; *at = (((*at as i32 + by) % n + n) % n) as usize; });
    show_viewer();
}

pub fn viewer_close() { with(|a| a.viewer = None); show_viewer(); }

fn show_viewer() {
    let Some(w) = ui() else { return };
    let (open, img, at, n, want) = with(|a| match &a.viewer {
        Some((l, at)) => {
            let k = &l[*at];
            let want = (!a.media.contains_key(k)).then(|| k.clone());
            (true, a.media.get(k).and_then(|m| m.1.clone()).unwrap_or_default(), *at as i32, l.len() as i32, want)
        }
        None => (false, slint::Image::default(), 0, 0, None),
    }).unwrap_or((false, slint::Image::default(), 0, 0, None));
    if let Some(k) = want { with(|a| { a.media.insert(k.clone(), ("loading".into(), None)); }); fetch_media(k); }
    w.set_viewer_open(open);
    w.set_viewer_image(img);
    w.set_viewer_at(at);
    w.set_viewer_count(n);
}

pub fn viewer_of() -> Value {
    with(|a| a.viewer.as_ref().map(|(l, at)| json!({ "at": at, "count": l.len(), "key": l[*at], "ready": a.media.get(&l[*at]).is_some_and(|m| m.0 == "ready") }))).flatten().unwrap_or(Value::Null)
}

/// The state of a chat's media (the test driver).
pub fn media_of(chat: &str) -> Value {
    with(|a| Value::Object(a.media.iter().filter(|(k, _)| k.starts_with(&format!("{chat}|"))).map(|(k, (st, img))| {
        let size = img.as_ref().map(|i| { let s = i.size(); json!([s.width, s.height]) }).unwrap_or(Value::Null);
        (k.split_once('|').map(|x| x.1.to_string()).unwrap_or_default(), json!({ "state": st, "size": size }))
    }).collect())).unwrap_or(Value::Null)
}

/// Play a video or audio: fetched (with the sign-in, never a URL with the
/// token) to the temp folder under its stored name, then the system's
/// player opens the file. (The test driver only records the path.)
pub fn play_media(chat: &str, name: &str, filename: &str) {
    let key = format!("{chat}|{name}");
    if with(|a| a.media.get(&key).is_some_and(|m| m.0 == "fetching")).unwrap_or(true) { return; }
    let Some(api) = api() else { return };
    with(|a| { a.media.insert(key.clone(), ("fetching".into(), None)); });
    invalidate_media_rows(chat);
    let path = std::env::temp_dir().join(name);
    let url = format!("/api/chat-media?session={chat}&name={name}");
    let (chat2, label) = (chat.to_string(), if filename.is_empty() { name.to_string() } else { filename.to_string() });
    bg(move || -> Result<std::path::PathBuf, String> {
        if path.metadata().is_ok_and(|m| m.len() > 0) { return Ok(path); }
        let part = path.with_extension("part");
        let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut err = None;
        let r = api.get_stream(&url, &mut |b, _| { use std::io::Write; if let Err(e) = f.write_all(b) { err = Some(e.to_string()); false } else { true } });
        drop(f);
        match r {
            Ok((200, _)) if err.is_none() => { std::fs::rename(&part, &path).map_err(|e| e.to_string())?; Ok(path) }
            Ok((_, text)) => { let _ = std::fs::remove_file(&part); Err(err.unwrap_or_else(|| serde_json::from_str::<Value>(&text).ok().and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string)).unwrap_or_default())) }
            Err(e) => { let _ = std::fs::remove_file(&part); Err(e.to_string()) }
        }
    }, move |r| {
        with(|a| { a.media.remove(&key); });
        invalidate_media_rows(&chat2);
        match r {
            Ok(p) => {
                let ps = p.to_string_lossy().to_string();
                with(|a| a.last_opened = ps.clone());
                if std::env::var_os("CRUNDI_TEST_PORT").is_none() { std::thread::spawn(move || { let _ = open::that(&p); }); }
            }
            Err(e) => toast(&if e.is_empty() { format!("Could not play {label}") } else { e }, true),
        }
    });
}

fn invalidate_media_rows(chat: &str) {
    with(|a| if let Some(cs) = a.chats.get_mut(chat) { for r in cs.rows.iter_mut() { if matches!(r, Row::Tool(..) | Row::Tools(..)) { *r = Row::Code(String::new(), String::new(), false); } } });
    show_chat(chat);
}

/// A frame's Open (or a stored HTML frame's, when there is no live frame):
/// its page in the system browser; stored HTML through a temp file.
pub fn open_frame(chat: &str, name: &str, url: &str) {
    if !url.is_empty() { open_link(url); return; }
    let Some(api) = api() else { return };
    let path = std::env::temp_dir().join(format!("{}.html", name.trim_end_matches(".embed")));
    let q = format!("/api/chat-media?session={chat}&name={name}");
    bg(move || api.get_bytes(&q).ok().filter(|r| r.0 == 200).and_then(|r| std::fs::write(&path, r.1).ok().map(|_| path)), |p| match p {
        Some(p) => { let ps = p.to_string_lossy().to_string(); with(|a| a.last_opened = ps); if std::env::var_os("CRUNDI_TEST_PORT").is_none() { std::thread::spawn(move || { let _ = open::that(&p); }); } }
        None => toast("This embed is no longer stored.", true),
    });
}

// ─── Downloads (a show_file card's Download, the download rows) ───

pub struct Dl {
    id: u32,
    name: String,
    line: String,
    /// running | done | failed | cancelled
    state: String,
    frac: f32,
    cancel: Arc<AtomicBool>,
}

fn show_dls() {
    let Some(w) = ui() else { return };
    with(|a| {
        let rows: Vec<DlRow> = a.dls.iter().map(|d| DlRow { id: d.id as i32, name: d.name.as_str().into(), line: d.line.as_str().into(), state: d.state.as_str().into(), frac: d.frac }).collect();
        a.dl_rows.set_vec(rows);
        w.set_dl_rows(ModelRc::from(a.dl_rows.clone()));
    });
}

fn dl_update(id: u32, f: impl FnOnce(&mut Dl)) {
    with(|a| if let Some(d) = a.dls.iter_mut().find(|d| d.id == id) { f(d); });
    show_dls();
}

/// The row goes by itself after a while (4.5 s saved, 1.2 s cancelled, 7 s failed).
fn dl_linger(id: u32, ms: u64) {
    slint::Timer::single_shot(Duration::from_millis(ms), move || { with(|a| a.dls.retain(|d| d.id != id)); show_dls(); });
}

/// Download: the system's save dialog first ("Save <name>", the Downloads
/// folder; cancel does nothing), then the file streams to <path>.part,
/// renamed when complete, with its row.
pub fn download_file(chat: &str, id: &str, filename: &str) {
    let (chat, id, name) = (chat.to_string(), id.to_string(), if filename.is_empty() { "file".to_string() } else { filename.to_string() });
    std::thread::spawn(move || {
        let picked = crate::testctl::take_pick().or_else(|| {
            let mut d = rfd::FileDialog::new().set_title(format!("Save {name}")).set_file_name(&name);
            if let Some(dir) = dirs_downloads() { d = d.set_directory(dir); }
            d.save_file()
        });
        let Some(path) = picked else { return };
        let _ = slint::invoke_from_event_loop(move || start_download(chat, id, name, path));
    });
}

fn dirs_downloads() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let d = std::path::PathBuf::from(home).join("Downloads");
    d.is_dir().then_some(d)
}

fn start_download(chat: String, fid: String, name: String, path: std::path::PathBuf) {
    let Some(api) = api() else { return };
    let cancel = Arc::new(AtomicBool::new(false));
    let id = with(|a| { a.dl_seq += 1; let id = a.dl_seq; a.dls.push(Dl { id, name: name.clone(), line: "Starting…".into(), state: "running".into(), frac: 0.0, cancel: cancel.clone() }); id }).unwrap_or(0);
    show_dls();
    let url = format!("/api/chat-media/file?session={chat}&id={fid}");
    std::thread::spawn(move || {
        let part = { let mut p = path.clone().into_os_string(); p.push(".part"); std::path::PathBuf::from(p) };
        let res: Result<u64, String> = (|| {
            let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
            let (mut got, mut last) = (0u64, std::time::Instant::now());
            let mut werr = None;
            let r = api.get_stream(&url, &mut |b, total| {
                use std::io::Write;
                if cancel.load(Ordering::Relaxed) { return false; }
                if let Err(e) = f.write_all(b) { werr = Some(e.to_string()); return false; }
                got += b.len() as u64;
                if last.elapsed() >= Duration::from_millis(120) {
                    last = std::time::Instant::now();
                    let (g, t) = (got, total);
                    let _ = slint::invoke_from_event_loop(move || dl_update(id, |d| {
                        d.line = if t > 0 { format!("{} of {}", media::fmt_dl_size(g), media::fmt_dl_size(t)) } else { media::fmt_dl_size(g) };
                        d.frac = if t > 0 { g as f32 / t as f32 } else { 0.0 };
                    }));
                }
                true
            }).map_err(|e| e.to_string())?;
            drop(f);
            if let Some(e) = werr { return Err(e); }
            match r {
                (200, _) => { std::fs::rename(&part, &path).map_err(|e| e.to_string())?; Ok(got) }
                (499, _) => Err("\u{0}cancel".into()),
                (st, text) => Err(serde_json::from_str::<Value>(&text).ok().and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string)).unwrap_or_else(|| format!("Download failed ({st})"))),
            }
        })();
        if res.is_err() { let _ = std::fs::remove_file(&part); }
        let _ = slint::invoke_from_event_loop(move || match res {
            Ok(n) => { dl_update(id, |d| { d.state = "done".into(); d.frac = 1.0; d.line = format!("Saved · {}", media::fmt_dl_size(n)); }); dl_linger(id, 4500); }
            Err(e) if e == "\u{0}cancel" => { dl_update(id, |d| { d.state = "cancelled".into(); d.line = "Cancelled".into(); }); dl_linger(id, 1200); }
            Err(e) => { dl_update(id, |d| { d.state = "failed".into(); d.line = e; }); dl_linger(id, 7000); }
        });
    });
}

/// A row's ×: a running download is cancelled; a finished row goes.
pub fn dl_close(id: i32) {
    with(|a| {
        if let Some(d) = a.dls.iter().find(|d| d.id == id as u32) {
            if d.state == "running" { d.cancel.store(true, Ordering::Relaxed); return; }
        }
        a.dls.retain(|d| d.id != id as u32);
    });
    show_dls();
}

pub fn dls_of() -> Value {
    with(|a| Value::Array(a.dls.iter().map(|d| json!({ "name": d.name, "line": d.line, "state": d.state })).collect())).unwrap_or(Value::Null)
}

/// A show_* card's media as drawn (the test driver): kind, name, box, state, caption.
fn media_json(chat: &str, t: &Shown, cache: &HashMap<String, (String, Option<slint::Image>)>) -> Value {
    let Some(res) = t.raw.as_str() else { return Value::Null };
    let (items, cap) = media::items(res);
    let several = items.iter().filter(|i| matches!(i.kind.as_str(), "image" | "video" | "embed")).count() > 1;
    json!({ "caption": cap, "items": items.iter().map(|i| {
        let (w, h) = match i.kind.as_str() { "embed" => media::frame_size(i), "image" | "video" => media::box_size(i.w, i.h, several), _ => (0.0, 0.0) };
        let pic = if i.kind == "video" { &i.poster } else { &i.name };
        let (cap, link) = if i.kind == "embed" { media::frame_caption(i) } else { (media::item_caption(i), String::new()) };
        json!({ "kind": i.kind, "name": i.name, "w": w, "h": h, "state": cache.get(&format!("{chat}|{pic}")).map(|m| m.0.clone()).unwrap_or_default(), "caption": cap, "link": link, "line": format!("{}{}", media::fmt_bytes(i.size), if i.caption.is_empty() { String::new() } else { format!(" · {}", i.caption) }) })
    }).collect::<Vec<_>>() })
}

// ─── Suggestions and recaps (2.17), side questions (2.21) ───

/// The suggestion clicked: sent as it is (the box and its draft untouched);
/// it and the recap go at once.
pub fn suggestion_send(chat: &str) {
    let key = chat_key(chat);
    let text = with(|a| a.chats.get_mut(chat).map(|cs| { let t = std::mem::take(&mut cs.conv.suggestion); cs.conv.recap = None; t })).flatten().unwrap_or_default();
    if text.is_empty() { return; }
    with(|a| { a.sending.insert(chat.to_string()); if let Some(cs) = a.chats.get_mut(chat) { cs.error.clear(); } });
    post_send(&key, chat, text, false);
}

pub fn suggestion_dismiss(chat: &str) {
    with(|a| if let Some(cs) = a.chats.get_mut(chat) { cs.conv.suggestion.clear(); });
    render();
    let path = format!("/api/ui-sessions/{chat}/dismiss-suggestion");
    call(move |api| api.post(&path, &json!({})), |_| {});
}

pub fn recap_dismiss(chat: &str) {
    let had = with(|a| a.chats.get_mut(chat).and_then(|cs| cs.conv.recap.take()).is_some()).unwrap_or(false);
    render();
    if !had { return; }
    let path = format!("/api/ui-sessions/{chat}/dismiss-recap");
    call(move |api| api.post(&path, &json!({})), |_| {});
}

/// Ask on the side: the box cleared, the recap dismissed, a card in the log
/// after the last entry, then POST …/aside. A server without it (404): the
/// card goes and the text is sent as typed.
fn ask_aside(key: &str, chat: &str, text: String, question: String) {
    let n = with(|a| {
        save_draft_in(a, key, "");
        *a.draft_gen.entry(key.to_string()).or_insert(0) += 1;
        a.slash.remove(key);
        a.local_errors += 1;
        let n = a.local_errors;
        if let Some(cs) = a.chats.get_mut(chat) {
            let anchor = cs.conv.entries.last().map(|e| s(e, "id")).unwrap_or_default();
            cs.asides.push(Aside { id: n, anchor, question: question.clone(), state: "pending".into(), text: String::new() });
            cs.gen += 1;
        }
        n
    }).unwrap_or(0);
    recap_dismiss(chat);
    show_chat(chat);
    render();
    write_drafts_soon();
    let Some(api) = api() else { return };
    let stub = crate::testctl::take_stub("aside");
    let (k, c, path) = (key.to_string(), chat.to_string(), format!("/api/ui-sessions/{chat}/aside"));
    bg(move || match stub {
        Some(v) => Ok((v.get("status").and_then(|x| x.as_u64()).unwrap_or(200) as u16, v.get("body").cloned().unwrap_or(Value::Null))),
        None => post_raw(&api, &path, &json!({ "question": question })).map_err(|e| e.to_string()),
    }, move |r| {
        if matches!(r, Ok((404, _))) {
            with(|a| if let Some(cs) = a.chats.get_mut(&c) { cs.asides.retain(|x| x.id != n); });
            show_chat(&c);
            send_plain(&k, text);
            return;
        }
        let (state, out) = match r {
            Ok((_, v)) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => ("done", s(&v, "answer")),
            Ok((_, v)) => ("error", { let e = s(&v, "error"); if e.is_empty() { "No answer".to_string() } else { e } }),
            Err(e) => ("error", e),
        };
        with(|a| if let Some(cs) = a.chats.get_mut(&c) {
            if let Some(x) = cs.asides.iter_mut().find(|x| x.id == n) { x.state = state.into(); x.text = out; cs.gen += 1; }
        });
        show_chat(&c);
        render();
    });
}

/// A side question's ×: the card goes (its answer, if still coming, is dropped).
pub fn aside_close(chat: &str, id: u32) {
    with(|a| if let Some(cs) = a.chats.get_mut(chat) { cs.asides.retain(|x| x.id != id); });
    show_chat(chat);
}

/// The test driver: the dock and the side questions of a chat.
pub fn dock_of(key: &str) -> Value {
    let Some(chat) = chat_of(key).map(str::to_string) else { return Value::Null };
    with(|a| a.chats.get(&chat).map(|cs| json!({
        "suggestion": cs.conv.suggestion, "recap": cs.conv.recap.as_ref().map(|r| json!({ "text": r.0, "why": r.1 })),
        "asides": cs.asides.iter().map(|x| json!({ "id": x.id, "question": x.question, "state": x.state, "text": x.text })).collect::<Vec<_>>(),
    }))).flatten().unwrap_or(Value::Null)
}

// ─── The agent dock (2.18, src/agents.rs) ───

/// A pane's badges and its open sheet (a list, or one record's panel).
fn agent_pane(p: &mut PaneView, cs: &ChatPaneState) {
    let shown: Vec<&Value> = cs.conv.agents.iter().filter(|a| agents::shown(a)).collect();
    let (tasks, ags): (Vec<&Value>, Vec<&Value>) = shown.iter().partition(|a| agents::is_task(a));
    let run = |l: &[&Value]| l.iter().filter(|a| agents::running(a)).count();
    p.ag_n = ags.len() as i32; p.ag_run = run(&ags) > 0;
    p.cmd_n = tasks.len() as i32; p.cmd_run = run(&tasks) > 0;
    p.ag_tip = agents::tooltip(false, ags.len(), run(&ags)).into();
    p.cmd_tip = agents::tooltip(true, tasks.len(), run(&tasks)).into();
    if let Some(task) = cs.sheet_list {
        let list: Vec<Value> = shown.iter().filter(|a| agents::is_task(a) == task).map(|a| (*a).clone()).collect();
        let r = list.iter().filter(|a| agents::running(a)).count();
        p.sheet = if task { "commands".into() } else { "agents".into() };
        p.sheet_title = if task { "Commands".into() } else { "Agents".into() };
        p.sheet_sub = agents::list_sub(r, list.len() - r).into();
        p.sheet_rows = ModelRc::new(VecModel::from(agents::sorted(&list).iter().map(|a| AgentRow {
            id: s(a, "toolUseId").into(), title: agents::label(a).into(), sub: agents::row_sub(a).into(),
            state: if agents::running(a) { "running".into() } else if s(a, "status") == "failed" { "failed".into() } else { "done".into() },
        }).collect::<Vec<_>>()));
    } else if let Some((tid, back)) = &cs.sheet_panel {
        if let Some(a) = cs.conv.agents.iter().find(|a| s(a, "toolUseId") == *tid) {
            p.sheet = "panel".into();
            p.sheet_title = agents::label(a).into();
            p.sheet_sub = agents::panel_sub(a).into();
            p.sheet_back = if *back { if agents::is_task(a) { "‹ Commands".into() } else { "‹ Agents".into() } } else { SharedString::default() };
            p.panel_prompt = s(a, "prompt").into();
            p.panel_summary = md(&s(a, "summary"));
            p.panel_has_summary = !s(a, "summary").is_empty();
            p.panel_entries = ModelRc::from(cs.panel_model.clone());
            p.panel_empty = if cs.panel_model.row_count() == 0 { agents::empty_text(a).into() } else { SharedString::default() };
        }
    }
}

/// The open panel's rows: the agent's transcript (a task: its own tool card
/// from the chat, open), drawn as the log draws entries.
fn panel_rows(cs: &ChatPaneState, chat: &str, cache: &HashMap<String, (String, Option<slint::Image>)>) {
    let Some((tid, _)) = &cs.sheet_panel else { cs.panel_model.set_vec(vec![]); return };
    let a = cs.conv.agents.iter().find(|a| s(a, "toolUseId") == *tid);
    let mut c = Conversation::default();
    if a.is_some_and(agents::is_task) {
        // A command: its tool row from the chat, expanded.
        c.entries = cs.conv.entries.iter().filter(|e| s(e, "toolUseId") == *tid).cloned().collect();
    } else {
        c.entries = cs.conv.agent_msgs.get(tid).cloned().unwrap_or_default();
    }
    let open: HashSet<String> = c.entries.iter().filter(|e| a.is_some_and(agents::is_task) && s(e, "kind") == "tool").map(|e| s(e, "id")).collect();
    let rows = rows_of(&c.view(), &[], &open, &cs.think, &[]);
    let mc = MediaCtx { chat: chat.to_string(), cache, want: std::cell::RefCell::new(vec![]) };
    let views: Vec<EntryView> = rows.iter().map(|r| { let mut v = to_view(r, &mc); v.chat = chat.into(); v }).collect();
    cs.panel_model.set_vec(views);
}

fn sheet_update(chat: &str, f: impl FnOnce(&mut ChatPaneState)) {
    with(|a| {
        if let Some(cs) = a.chats.get_mut(chat) { f(cs); }
        if let Some(cs) = a.chats.get(chat) { panel_rows(cs, chat, &a.media); }
    });
    render();
}

/// A badge: its kind's list (closing any panel); again: closed.
pub fn agent_badge(chat: &str, task: bool) {
    sheet_update(chat, |cs| { cs.sheet_panel = None; cs.sheet_list = if cs.sheet_list == Some(task) { None } else { Some(task) }; });
}

/// A list row: that record's panel, with the back link; the log's agent
/// row: its panel without one (again: closed).
pub fn agent_open(chat: &str, tid: &str, from_list: bool) {
    sheet_update(chat, |cs| {
        let same = cs.sheet_panel.as_ref().is_some_and(|(t, _)| t == tid);
        cs.sheet_list = None;
        cs.sheet_panel = if same && !from_list { None } else { Some((tid.to_string(), from_list)) };
    });
}

pub fn agent_back(chat: &str) {
    sheet_update(chat, |cs| {
        let task = cs.sheet_panel.as_ref().and_then(|(t, _)| cs.conv.agents.iter().find(|a| s(a, "toolUseId") == *t)).map(agents::is_task).unwrap_or(false);
        cs.sheet_panel = None; cs.sheet_list = Some(task);
    });
}

pub fn agent_close(chat: &str) { sheet_update(chat, |cs| { cs.sheet_list = None; cs.sheet_panel = None; }); }

/// Dismiss one (a row's ✕) or every shown one of a kind (Dismiss all): at
/// once here, then POST …/dismiss-agents. The last one gone closes the list.
pub fn agent_dismiss(chat: &str, ids: Vec<String>) {
    if ids.is_empty() { return; }
    sheet_update(chat, |cs| {
        for a in cs.conv.agents.iter_mut() { if ids.contains(&s(a, "toolUseId")) { a["dismissed"] = json!(true); } }
        if let Some(task) = cs.sheet_list {
            if !cs.conv.agents.iter().any(|a| agents::shown(a) && agents::is_task(a) == task) { cs.sheet_list = None; }
        }
    });
    show_chat(chat);
    let path = format!("/api/ui-sessions/{chat}/dismiss-agents");
    let body = json!({ "toolUseIds": ids });
    let c = chat.to_string();
    bg(move || api().map(|api| post_raw(&api, &path, &body)), move |r| {
        if let Some(Ok((404, _))) = r { let _ = c; toast("Dismissing is not supported by this server", true); }
    });
}

pub fn agent_dismiss_all(chat: &str, task: bool) {
    let ids = with(|a| a.chats.get(chat).map(|cs| cs.conv.agents.iter().filter(|a| agents::shown(a) && agents::is_task(a) == task).map(|a| s(a, "toolUseId")).collect::<Vec<_>>())).flatten().unwrap_or_default();
    agent_dismiss(chat, ids);
    with(|a| if let Some(cs) = a.chats.get_mut(chat) { cs.sheet_list = None; });
    render();
}

/// The test driver: a chat's badges and sheet.
pub fn agents_of(key: &str) -> Value {
    let Some(chat) = chat_of(key).map(str::to_string) else { return Value::Null };
    let Some(w) = ui() else { return Value::Null };
    let p = w.get_panes().iter().find(|p| p.key == key);
    let Some(p) = p else { return Value::Null };
    let _ = chat;
    json!({ "agents": p.ag_n, "agentsRunning": p.ag_run, "commands": p.cmd_n, "commandsRunning": p.cmd_run, "agTip": p.ag_tip.as_str(), "cmdTip": p.cmd_tip.as_str(),
        "sheet": p.sheet.as_str(), "title": p.sheet_title.as_str(), "sub": p.sheet_sub.as_str(), "back": p.sheet_back.as_str(),
        "rows": p.sheet_rows.iter().map(|r| json!({ "id": r.id.as_str(), "title": r.title.as_str(), "sub": r.sub.as_str(), "state": r.state.as_str() })).collect::<Vec<_>>(),
        "prompt": p.panel_prompt.as_str(), "entries": p.panel_entries.row_count(), "empty": p.panel_empty.as_str() })
}

// ─── The goal and the plan (2.19) ───

fn goal_pane(p: &mut PaneView, cs: &ChatPaneState) {
    if let Some(g) = &cs.conv.goal {
        let n = g.get("verdicts").and_then(|v| v.as_u64()).unwrap_or(0);
        let looping = g.get("looping").and_then(|v| v.as_bool()).unwrap_or(false);
        p.goal_on = true;
        p.goal_cond = s(g, "condition").into();
        p.goal_looping = looping;
        p.goal_count = if n > 0 { format!("{n} check{}", if n == 1 { "" } else { "s" }) } else if looping { "running".into() } else { "set".into() }.into();
        p.goal_stopped = !looping && n > 0;
    }
    if let Some(pl) = &cs.conv.plan {
        let st = s(pl, "status");
        // (The panel shows only while there is a plan: plan_open is read with it.)
        p.plan_status = if st.is_empty() { "proposed".into() } else { st.as_str().into() };
        p.plan_open = cs.plan_open;
        p.plan_state = match st.as_str() { "executing" => "approved — being worked through", "rejected" => "sent back for revision", _ => "waiting for your approval" }.into();
        if cs.plan_open {
            p.plan_blocks = ModelRc::new(VecModel::from(split_blocks("plan", &s(pl, "text")).into_iter().filter_map(|r| match r {
                Row::Prose(_, t, kind, marker, _) => Some(AnsBlock { kind: if kind.starts_with("li") { "li".into() } else if kind.starts_with('h') { "h".into() } else { "p".into() }, marker: marker.into(), md: md(&t), text: t.into() }),
                Row::Code(_, t, _) => Some(AnsBlock { kind: "code".into(), marker: SharedString::default(), md: StyledText::default(), text: t.into() }),
                _ => None,
            }).collect::<Vec<_>>()));
        }
    }
}

/// The PLAN pill: the panel opens or closes (never without a plan).
pub fn plan_toggle(chat: &str) {
    with(|a| if let Some(cs) = a.chats.get_mut(chat) { cs.plan_open = cs.conv.plan.is_some() && !cs.plan_open; });
    render();
}

/// "clear": `/goal clear` through the send path (queued when busy), the box
/// and its draft untouched.
pub fn goal_clear(chat: &str) {
    let key = chat_key(chat);
    let text = "/goal clear".to_string();
    let state = with(|a| {
        let st = chat_state_now(a, chat);
        if st == "needs-input" { a.held.entry(chat.to_string()).or_default().push(text.clone()); sync_held(a, chat); }
        st
    }).unwrap_or_default();
    match state.as_str() {
        "needs-input" => render(),
        "working" => post_send(&key, chat, text, true),
        _ => { with(|a| a.sending.insert(chat.to_string())); post_send(&key, chat, text, false); }
    }
}

pub fn goal_of(key: &str) -> Value {
    let Some(w) = ui() else { return Value::Null };
    let Some(p) = w.get_panes().iter().find(|p| p.key == key) else { return Value::Null };
    json!({ "on": p.goal_on, "condition": p.goal_cond.as_str(), "count": p.goal_count.as_str(), "looping": p.goal_looping, "stopped": p.goal_stopped,
        "plan": p.plan_status.as_str(), "planOpen": p.plan_open, "planState": p.plan_state.as_str(), "planBlocks": p.plan_blocks.row_count() })
}
