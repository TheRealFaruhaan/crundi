//! A chat's log (Operations', phase 2: 2.13-2.23; AREAS.md): the
//! conversation as the PC client draws it.
//!
//! The spine is here: the view (`LogView`, one per chat pane), its rows
//! (rows.rs, ported from the Slint client), and the log's part in the test
//! driver (`init`: its actions and its per-chat props). Every feature is its
//! own module, with its own state (a field of `St`), its rows' drawing, its
//! driver actions (`act`) and its props (`props`):
//!
//! | module | PLAN.md | what |
//! |---|---|---|
//! | questions | 2.13 | question cards (and permission cards) |
//! | tools | 2.14 | tool cards and groups |
//! | thinking | 2.15 | thinking blocks |
//! | prose | 2.16 | Claude's text, code blocks, the user's bubble, results, errors, notices |
//! | media | 2.16m | what a show_* call showed, under its card |
//! | frames | 2.16f | live frames (show_embed), through crundi-webview |
//! | suggest | 2.17 | suggestions and recaps |
//! | agents | 2.18 | agent rows and the agent dock |
//! | goal | 2.19 | the goal bar and the plan |
//! | cards | 2.20 | a factory's result card, a widget check's screenshots |
//! | aside | 2.21 | side questions |
//! | resume | 2.22 | the resume picker |
//! | parked | 2.23 | parked panes |
//!
//! Until the workbench's per-kind props hook exists (shared/requests.md),
//! each chat's fields are in the driver's props under `log.panes[<chat id>]`.

pub mod rows;
pub mod questions;
pub mod tools;
pub mod thinking;
pub mod prose;
pub mod media;
pub mod frames;
pub mod suggest;
pub mod agents;
pub mod goal;
pub mod cards;
pub mod aside;
pub mod resume;
pub mod parked;

use crundi_chat_model::{ChatEvent, ChatModel};
use crundi_core::chat::Shown;
use crundi_theme::T;
use gpui::*;
use rows::Row;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

/// Every feature's state for one chat (one field per module, its own type).
#[derive(Default)]
pub struct St {
    pub questions: questions::State,
    pub tools: tools::State,
    pub thinking: thinking::State,
    pub prose: prose::State,
    pub media: media::State,
    pub frames: frames::State,
    pub suggest: suggest::State,
    pub agents: agents::State,
    pub goal: goal::State,
    pub cards: cards::State,
    pub aside: aside::State,
    pub resume: resume::State,
    pub parked: parked::State,
}

pub struct LogView {
    pub model: Entity<ChatModel>,
    /// Tool cards and groups the reader opened, by row id (2.14).
    pub open: HashSet<String>,
    pub st: St,
    /// The log, virtualised: only the rows in view (and a margin) are
    /// built and laid out each frame. Scrolling a long chat rebuilt every
    /// message, its markdown included, on every frame (the owner's 18 Nov
    /// "laggy scrolling"; a profile: layout and styles of all rows).
    pub list: ListState,
    /// The rows the list was last told about (compared each render: only
    /// the range that changed is measured again).
    rows_prev: Vec<rows::Row>,
    /// The list's viewport as of this render, for what is drawn inside its
    /// rows (a browser view's visible part): reading the list from inside
    /// its own layout panicked ('RefCell already mutably borrowed').
    pub viewport: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    /// Scroll to the end after the next layouts (new entries, the first history).
    pub stick: u8,
    _sub: Subscription,
}

impl LogView {
    pub fn new(model: Entity<ChatModel>, cx: &mut Context<Self>) -> Self {
        let sub = cx.subscribe(&model, |this, _, ev: &ChatEvent, cx| {
            // Following the bottom, or the jump button's dot (2.16, prose.rs).
            prose::arrived(this, matches!(ev, ChatEvent::NewEntry));
            this.changed(cx);
            cx.notify();
        });
        let id = model.read(cx).id().to_string();
        let me = cx.entity().downgrade();
        if !cx.has_global::<Views>() { cx.set_global(Views::default()); }
        cx.global_mut::<Views>().0.insert(id, me);
        LogView { model, open: HashSet::new(), st: St::default(), list: ListState::new(0, ListAlignment::Top, T::LOG_OVERDRAW), rows_prev: Vec::new(), viewport: Default::default(), stick: 3, _sub: sub }
    }

    /// The chat's id.
    pub fn chat(&self, cx: &App) -> String { self.model.read(cx).id().to_string() }

    /// The entries as the log shows them (crundi_core::chat).
    pub fn shown(&self, cx: &App) -> Vec<Shown> { self.model.read(cx).conv.view() }

    /// The log's rows now.
    pub fn rows(&self, cx: &App) -> Vec<Row> {
        let m = self.model.read(cx);
        rows::rows_of(&m.conv.view(), &m.conv.agents, &self.open, &self.st.thinking.think, &self.st.aside.list)
    }

    /// The conversation changed (a live event, the history): each feature
    /// follows it (a thinking block's timers, a question's draft…).
    fn changed(&mut self, cx: &mut Context<Self>) {
        thinking::changed(self, cx);
        questions::changed(self, cx);
        cards::changed(self, cx);
    }

    /// Open or close a card, a group or a thinking block (its head clicked).
    pub fn toggle(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.shown(cx).iter().any(|e| e.id == id && e.kind == "thinking") { self.st.thinking.think.toggle(id); }
        else if !self.open.remove(id) { self.open.insert(id.to_string()); }
        cx.notify();
    }

    /// The list follows the rows: only the range that changed since the
    /// last render is measured again (a streamed reply: its last rows).
    fn sync_rows(&mut self, cx: &App) {
        let rows = self.rows(cx);
        let (old, new) = (self.rows_prev.len(), rows.len());
        if old == 0 && new == 0 && self.list.item_count() == 1 { return; }
        let head = self.rows_prev.iter().zip(&rows).take_while(|(a, b)| a == b).count();
        let tail = self.rows_prev[head..].iter().rev().zip(rows[head..].iter().rev()).take_while(|(a, b)| a == b).count();
        if self.list.item_count() != old + 1 {
            // First time (or out of step): every row, and the pad.
            self.list.reset(new + 1);
        } else if head != old || head != new || tail != 0 {
            self.list.splice(head..old - tail, new - head - tail);
        }
        self.rows_prev = rows;
    }

    /// List item `ix`: a row, or after the last the floating dock's room.
    fn item(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let n = self.rows_prev.len();
        let body = if ix < n {
            let r = self.rows_prev[ix].clone();
            self.row(ix, &r, window, cx)
        } else {
            Some(agents::pad(self).into_any_element())
        };
        // The old column's padding (12) and gap (10), per item.
        div().px(T::SP_12).pt(if ix == 0 { T::SP_12 } else { px(0.) }).pb(if ix + 1 >= n { T::SP_12 } else { T::SP_10 })
            .children(body).into_any_element()
    }

    fn row(&mut self, i: usize, r: &Row, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        match r {
            Row::Entry(e) => match e.kind {
                "question" | "permission" => questions::row(self, e, window, cx),
                "goal-verdict" => Some(goal::verdict(e)),
                "factory-result" => cards::row(self, e, window, cx),
                _ => prose::entry(self, i, e, cx),
            },
            Row::Prose(id, text, kind, marker, last) => Some(prose::block_in(self, id, text, kind, marker, *last, cx)),
            Row::Code(id, code, last) => Some(prose::code(self, id, code, *last, cx)),
            Row::Tool(id, t, open) => Some(tools::card(self, id, t, *open, window, cx)),
            Row::Tools(gid, summary, members, open) => Some(tools::group(self, gid, summary, members, *open, window, cx)),
            Row::Think(e, open) => Some(thinking::row(self, e, *open, cx)),
            Row::Aside(a) => Some(aside::row(self, a, cx)),
            Row::Agent(id, a) => Some(agents::row(self, id, a, cx)),
        }
    }

    /// This chat's fields for the test driver (each feature adds its own).
    pub fn props(&self, cx: &App) -> Value {
        let mut o = Map::new();
        questions::props(self, cx, &mut o);
        tools::props(self, cx, &mut o);
        thinking::props(self, cx, &mut o);
        prose::props(self, cx, &mut o);
        media::props(self, cx, &mut o);
        frames::props(self, cx, &mut o);
        suggest::props(self, cx, &mut o);
        agents::props(self, cx, &mut o);
        goal::props(self, cx, &mut o);
        cards::props(self, cx, &mut o);
        aside::props(self, cx, &mut o);
        resume::props(self, cx, &mut o);
        parked::props(self, cx, &mut o);
        Value::Object(o)
    }

    /// A driver action for this chat; Some(answer) for a name a feature knows.
    fn act(&mut self, name: &str, args: &[String], window: &mut Window, cx: &mut Context<Self>) -> Option<Value> {
        None.or_else(|| questions::act(name, args, self, window, cx))
            .or_else(|| tools::act(name, args, self, window, cx))
            .or_else(|| thinking::act(name, args, self, window, cx))
            .or_else(|| prose::act(name, args, self, window, cx))
            .or_else(|| media::act(name, args, self, window, cx))
            .or_else(|| frames::act(name, args, self, window, cx))
            .or_else(|| suggest::act(name, args, self, window, cx))
            .or_else(|| agents::act(name, args, self, window, cx))
            .or_else(|| goal::act(name, args, self, window, cx))
            .or_else(|| cards::act(name, args, self, window, cx))
            .or_else(|| aside::act(name, args, self, window, cx))
            .or_else(|| resume::act(name, args, self, window, cx))
            .or_else(|| parked::act(name, args, self, window, cx))
    }
}

impl Render for LogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // To the end once the new content is laid out (its height is known a frame later).
        prose::keep(self);
        if self.stick > 0 {
            self.stick -= 1;
            self.list.scroll_to_end();
            let me = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| { let _ = me.update(cx, |_, cx| cx.notify()); });
        }
        let zoom = self.model.read(cx).zoom;
        self.viewport.set(self.list.viewport_bounds());
        self.sync_rows(cx);
        // The rows, then the floating dock's room at the end (2.17/2.18: the
        // newest line scrolls clear of it), as list items.
        let log = div().id("log").flex().flex_col().flex_1().min_w_0().min_h_0()
            .on_scroll_wheel(cx.listener(|this, e: &ScrollEvent, window, _| prose::wheel(this, e, window)))
            .child(list(self.list.clone(), cx.processor(|this, ix: usize, window, cx| this.item(ix, window, cx))).flex_1().min_h_0().size_full());
        // A parked chat (2.23): its card over what it was (or Peek), nothing else.
        let log = match parked::wrap(self, log.into_any_element(), window, cx) {
            Ok(p) => return with_fallbacks(div()).flex().flex_col().flex_1().min_w_0().min_h_0().text_size(T::TEXT_13 + px(zoom as f32)).child(p).into_any_element(),
            Err(l) => l,
        };
        // Over the log (its area only): the floating dock (badges, recap,
        // suggestion), the plan panel (2.19), the agent sheet (2.18); the
        // goal bar between the log and the composer (2.19).
        with_fallbacks(div()).flex().flex_col().flex_1().min_w_0().min_h_0().text_size(T::TEXT_13 + px(zoom as f32))
            // The log's box clips what floats over it (the dock, the plan, the sheet: F70).
            .child(div().relative().overflow_hidden().flex().flex_col().flex_1().min_w_0().min_h_0().child(log)
                .children(agents::dock(self, window, cx))
                .children(goal::panel(self, window, cx))
                .children(prose::jump(self, cx))
                .children(agents::sheet(self, window, cx)))
            .children(media::overlay(self, window, cx)) // media: the viewer and the download rows (window-level)
            .children(goal::view(self, window, cx))
            .into_any_element()
    }
}


/// Chat content's fallback chain: after the theme's font, Noto Emoji.
pub fn with_fallbacks<E: Styled>(mut el: E) -> E {
    el.text_style().font_fallbacks = Some(FontFallbacks::from_fonts(vec![crundi_theme::EMOJI_FONT.to_string()]));
    el
}

/// Every chat's log view, by chat id (a view registers itself).
#[derive(Default)]
pub struct Views(pub HashMap<String, WeakEntity<LogView>>);
impl Global for Views {}

/// A chat's log view, if it is open.
pub fn view_of(cx: &App, chat: &str) -> Option<Entity<LogView>> {
    cx.try_global::<Views>().and_then(|v| v.0.get(chat)).and_then(|w| w.upgrade())
}

/// The chat in the focused workbench pane (else the first chat pane).
pub fn focused_chat(cx: &mut App) -> String {
    let panes = crundi_workbench::workbench(cx).map(|w| w.update(cx, |w, cx| w.panes(cx))).unwrap_or_default();
    let chat = |p: &Value| p.get("key").and_then(|k| k.as_str()).and_then(|k| k.strip_prefix("live:")).map(String::from);
    panes.iter().find(|p| p.get("focused") == Some(&json!(true))).and_then(chat).or_else(|| panes.iter().find_map(chat)).unwrap_or_default()
}

/// A chat pane's log fields (question, tools, agentDock, goal, fr, pk…), for
/// the chats crate to merge into the pane's props once the workbench has a
/// per-kind props hook (shared/requests.md). Null when the log isn't open.
pub fn pane_props(cx: &App, chat: &str) -> Value {
    view_of(cx, chat).map(|v| v.read(cx).props(cx)).unwrap_or(Value::Null)
}

/// The log's part in the window (called once at start): its driver actions
/// (to the focused chat's log) and its props (every open log's fields).
pub fn init(cx: &mut App) {
    // Emoji in chat text: the theme's Noto Emoji (registered by crundi_theme), as the log's fallback (F57).
    resume::bind_keys(cx);
    // The launcher asks which conversation first (2.22).
    crundi_workbench::set_launch_chooser(cx, std::rc::Rc::new(|project, skip, term, cx, go| {
        resume::choose(project, skip, term, cx, move |c, _, cx| go(c.mode, c.resume_id.clone(), cx))
    }));
    // `/btw <question>` from the composer's box: asked on the side here (F37).
    crundi_chat_model::register_aside(cx, |cx, chat, text| aside::ask_from_box(cx, chat, text));
    // Each chat pane's log fields (question, tools, entries, agentDock, goal,
    // fr, pk…) in the pane's own props, as the Slint client's (F30).
    crundi_workbench::register_pane_props(cx, "live:", |key, cx| pane_props(cx, key.trim_start_matches("live:")));
    prose::init(cx);
    crundi_shell::register(cx, crundi_shell::Area::new("log")
        .test_ui(|name, args, window, cx| {
            // Test stand-ins until the workbench and chats answer them (media/cards worker):
            // focus-pane <leaf id> (the workbench's), chat-event <json> (an event played into the focused chat).
            if name == "focus-pane" {
                let leaf: u32 = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
                if let Some(wb) = crundi_workbench::workbench(cx) { wb.update(cx, |w, cx| { w.focused = leaf; cx.notify(); }); }
                return Some(json!({ "ok": true }));
            }
            if name == "chat-event" {
                let ev: Value = serde_json::from_str(args.first().map(String::as_str).unwrap_or("")).unwrap_or(Value::Null);
                let id = focused_chat(cx);
                let m = view_of(cx, &id)?.read(cx).model.clone();
                m.update(cx, |m, cx| m.apply(&json!({ "type": "ui-event", "id": id, "event": ev }), cx));
                return Some(json!({ "ok": true }));
            }
            // Actions for no chat in particular first (the resume picker…).
            if let Some(r) = resume::global_act(name, args, window, cx) { return Some(r); }
            // Actions naming their pane ("peek live:<id> on", "park-resume live:<id> …")
            // go to that chat; the rest to the focused one.
            let named = matches!(name, "peek" | "park-resume").then(|| args.first().and_then(|k| k.strip_prefix("live:")).map(String::from)).flatten();
            let id = named.unwrap_or_else(|| focused_chat(cx));
            let v = view_of(cx, &id)?;
            v.update(cx, |v, cx| v.act(name, args, window, cx))
        })
        .props(|_, cx| {
            let views: Vec<(String, Entity<LogView>)> = cx.try_global::<Views>().map(|v| v.0.iter().filter_map(|(k, w)| w.upgrade().map(|e| (k.clone(), e))).collect()).unwrap_or_default();
            let mut panes = Map::new();
            for (id, v) in views { panes.insert(id, v.read(cx).props(cx)); }
            // Top level, as the Slint client's: the resume picker (2.22). The
            // modules' window-wide props may add more top-level keys here.
            let (picker, chosen) = resume::global_props(cx);
            let mut log = Map::new();
            log.insert("panes".into(), Value::Object(panes));
            log.insert("resumeChosen".into(), chosen);
            prose::watch_props(cx, &mut log);
            let mut top = Map::new();
            media::global_props(cx, &mut top); // media: media, viewer, downloads, lastOpened (2.16m)
            frames::global_props(cx, &mut top); // frames: frames, framesLive, frameOpened, frameFocused (2.16f)
            top.insert("log".into(), Value::Object(log));
            top.insert("resume".into(), picker);
            top.insert("panelDist".into(), json!(agents::panel_dist(cx)));
            Value::Object(top)
        }));
}
