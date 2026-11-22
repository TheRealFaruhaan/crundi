//! A new pane's launcher (F18; the web's pending cell, src/webapp-html.js
//! buildCellEl's 'pending', "New terminal"): ">_", Empty Shell, then the
//! Claude box: UI Mode and Terminal, each with its Skip Permissions shield.
//! The workbench puts one in when it has no panes (the web's "a project
//! with no cells gets one placeholder"): the first screen of a fresh
//! project, and after the last pane closes.
//!
//! UI Mode starts a chat in this pane's place (the resume picker, when the
//! chat area registers one with `set_chooser`, asks which conversation
//! first, as the web's openSessionChooser). Empty Shell and Terminal are
//! the terminal area's (`set_terminal_launcher`; PLAN.md): until it is in,
//! they do nothing.
//!
//! The button this project launched last carries the web's "last used"
//! mark (tl-last: a dot, and "Last used for this project" in its tooltip).
//! As the web's localStorage crundi_last_mode_<project>, it is kept on this
//! computer (launch-modes.json in the config folder); Empty Shell is not
//! recorded (it is not a Claude mode).

use crate::{workbench, Workbench};
use crundi_theme::{IconName, T};
use crundi_widgets::{self as w, LaunchLook as L};
use gpui::*;
use std::{rc::Rc, time::Duration};

/// Ask which conversation (project, skip, terminal?), then `go(mode, id)`.
pub type Choose = Rc<dyn Fn(&str, bool, bool, &mut App, Rc<dyn Fn(&'static str, String, &mut App)>)>;
struct Chooser(Choose);
impl Global for Chooser {}
/// The resume picker (the chat area's).
pub fn set_chooser(cx: &mut App, f: Choose) { cx.set_global(Chooser(f)); }
/// The resume picker, if the chat area registered one (the terminal area
/// asks it too before Terminal starts, as the web's openSessionChooser).
pub fn chooser(cx: &App) -> Option<Choose> { cx.try_global::<Chooser>().map(|c| c.0.clone()) }

/// Start a terminal in a launcher's pane (its key, mode shell | normal | skip).
pub type LaunchTerminal = Rc<dyn Fn(&str, &'static str, &mut Window, &mut App)>;
struct TermLauncher(LaunchTerminal);
impl Global for TermLauncher {}
pub fn set_terminal_launcher(cx: &mut App, f: LaunchTerminal) { cx.set_global(TermLauncher(f)); }

/// The launchers on show, by pane key (handles, for the terminal area to
/// show its button busy while its terminal starts).
#[derive(Default)]
struct Launchers(std::collections::HashMap<String, WeakEntity<Launcher>>);
impl Global for Launchers {}

/// The pressed terminal button of a launcher while its start is under way
/// (3 Empty Shell, 4 Terminal, 5 its Skip Permissions; 0 when done), as the
/// web shows the button busy until the server answers.
pub fn set_busy(cx: &mut App, key: &str, busy: u8) {
    let Some(l) = cx.try_global::<Launchers>().and_then(|l| l.0.get(key)).and_then(|w| w.upgrade()) else { return };
    l.update(cx, |l, cx| if l.busy != busy { l.busy = busy; cx.notify(); });
}

pub struct Launcher {
    key: String,
    /// The pressed button while its launch is under way: 1 UI Mode, 2 its
    /// shield, 3 Empty Shell, 4 Terminal, 5 its shield.
    busy: u8,
    /// A row whose shield is open (hovered, or peeking): 1 or 2.
    open: u8,
    peek: u8,
    /// The mode this project launched last ("chat", "chat-skip", "normal",
    /// "skip" or ""), read when the launcher is made.
    last: String,
    _peek: Option<Task<()>>,
    /// UI Mode's drift and blink run only while the window is focused and
    /// on screen (OWNER-DECISIONS, 20 Nov): unfocused, minimised or
    /// covered, the button rests and draws no frames.
    live: bool,
    _live: [Subscription; 2],
}

/// The modes the mark knows (the web's LAUNCH_MODES).
const LAUNCH_MODES: [&str; 4] = ["chat", "chat-skip", "normal", "skip"];

fn modes_file() -> std::path::PathBuf { crundi_core::store::config_dir().join("launch-modes.json") }

/// The mode this project launched last ("" for none).
pub fn last_mode(project: &str) -> String {
    let m = std::fs::read_to_string(modes_file()).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get(project).and_then(|m| m.as_str()).map(String::from)).unwrap_or_default();
    if LAUNCH_MODES.contains(&m.as_str()) { m } else { String::new() }
}

/// Remember a launch's mode for this project (the web's rememberLaunchMode).
pub fn remember_mode(project: &str, mode: &str) {
    if project.is_empty() || !LAUNCH_MODES.contains(&mode) { return; }
    let path = modes_file();
    let mut v = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).filter(|v| v.is_object()).unwrap_or_else(|| serde_json::json!({}));
    v[project] = serde_json::Value::String(mode.to_string());
    let _ = std::fs::create_dir_all(crundi_core::store::config_dir());
    let _ = std::fs::write(path, v.to_string());
}

impl Launcher {
    pub fn new(key: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The shields peek open once on first paint (the web's peekShields:
        // the first row at 300 ms for 900 ms, the second 1150 ms later).
        let peek = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            for (row, wait) in [(1u8, 300u64), (0, 900), (2, 250), (0, 900)] {
                cx.background_executor().timer(Duration::from_millis(wait)).await;
                if this.update(cx, |l, cx| { l.peek = row; cx.notify(); }).is_err() { return; }
            }
        });
        let me = cx.entity().downgrade();
        let launchers = cx.default_global::<Launchers>();
        launchers.0.retain(|_, w| w.upgrade().is_some());
        launchers.0.insert(key.to_string(), me);
        let last = last_mode(&crundi_shell::project(cx));
        let live = window.is_window_active() && window.is_visible();
        let relive = |l: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            let on = window.is_window_active() && window.is_visible();
            if on != l.live { l.live = on; cx.notify(); }
        };
        let _live = [
            cx.observe_window_activation(window, move |l, window, cx| relive(l, window, cx)),
            cx.observe_window_visibility(window, move |l, _, window, cx| relive(l, window, cx)),
        ];
        Launcher { key: key.to_string(), busy: 0, open: 0, peek: 0, last, _peek: Some(peek), live, _live }
    }

    fn chat(&mut self, skip: bool, cx: &mut Context<Self>) {
        if self.busy != 0 { return; }
        let project = crundi_shell::project(cx);
        if project.is_empty() { return; }
        let me = cx.entity().downgrade();
        let go: Rc<dyn Fn(&'static str, String, &mut App)> = Rc::new(move |mode, id, cx| { let _ = me.update(cx, |l, cx| l.create(skip, mode, &id, cx)); });
        match cx.try_global::<Chooser>().map(|c| c.0.clone()) {
            Some(choose) => choose(&project, skip, false, cx, go),
            None => go("new", String::new(), cx),
        }
    }

    /// POST /api/ui-sessions/create; the chat takes this pane's place.
    fn create(&mut self, skip: bool, mode: &str, resume_id: &str, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        remember_mode(&project, if skip { "chat-skip" } else { "chat" });
        self.busy = if skip { 2 } else { 1 };
        cx.notify();
        let body = crundi_core::resume::create_body(&project, skip, mode, resume_id);
        let me = cx.entity().downgrade();
        let key = self.key.clone();
        crundi_shell::call(cx, move |api| api.post("/api/ui-sessions/create", &body), move |r, cx| {
            let _ = me.update(cx, |l, cx| { l.busy = 0; cx.notify(); });
            let err = match &r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(v.get("error").and_then(|e| e.as_str()).filter(|e| !e.is_empty()).unwrap_or("Failed to launch").to_string()),
                Ok(_) => None,
                Err(e) => Some(format!("Failed to launch: {e}")),
            };
            if let Some(e) = err {
                if let Some(h) = cx.active_window() { let _ = h.update(cx, |_, window, cx| w::toast(window, cx, e, w::Toast::Error)); }
                return;
            }
            let Some(id) = r.ok().and_then(|v| v.get("id").and_then(|i| i.as_str()).map(String::from)) else { return };
            if let Some(wb) = workbench(cx) { wb.update(cx, |w: &mut Workbench, cx| w.replace(&key, &format!("live:{id}"), cx)); }
        });
    }

    fn terminal(&mut self, mode: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy != 0 { return; }
        let Some(f) = cx.try_global::<TermLauncher>().map(|t| t.0.clone()) else { log::info!("launcher: {mode}: no terminal area yet"); return };
        remember_mode(&crundi_shell::project(cx), mode);
        f(&self.key, mode, window, cx);
    }

    fn hover_shield(&mut self, row: u8, on: bool, cx: &mut Context<Self>) {
        let n = if on { row } else if self.open == row { 0 } else { self.open };
        if n != self.open { self.open = n; cx.notify(); }
    }
}

impl Render for Launcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("launcher");
        let (busy, off) = (self.busy, self.busy != 0);
        let open = |row: u8| self.open == row || (self.open == 0 && self.peek == row);
        let row = |main: Stateful<Div>, shield: Stateful<Div>| div().flex().gap(T::LAUNCH_GAP).w(T::LAUNCH_ROW_W).max_w_full().child(main).child(shield);
        // The last-used mark and its words (the web's lastCls / peekShields).
        let last = self.last.clone();
        let mark = |b: Stateful<Div>, mode: &str, tip: &'static str| -> Stateful<Div> {
            if last != mode { return if tip.is_empty() { b } else { b.tooltip(w::tooltip(tip)) }; }
            let words = if tip.is_empty() { "Last used for this project".to_string() } else { format!("{tip} \u{2014} Last used for this project") };
            b.child(w::launch_mark()).tooltip(w::tooltip(words))
        };
        let ui = mark(w::launch_button("ui-mode", L::Chat, None, "UI Mode", !open(1), busy == 1, off, self.live), "chat", "")
            .on_click(cx.listener(|this, _, _, cx| this.chat(false, cx)));
        let ui_skip = mark(w::launch_button("ui-skip", L::Shield, Some(IconName::ShieldAlert), "Skip Permissions", open(1), busy == 2, off, false), "chat-skip", "UI Mode \u{2014} Skip Permissions")
            .on_hover(cx.listener(|this, on: &bool, _, cx| this.hover_shield(1, *on, cx)))
            .on_click(cx.listener(|this, _, _, cx| this.chat(true, cx)));
        let term = mark(w::launch_button("term-mode", L::Normal, Some(IconName::Terminal), "Terminal", !open(2), busy == 4, off, false), "normal", "")
            .on_click(cx.listener(|this, _, window, cx| this.terminal("normal", window, cx)));
        let term_skip = mark(w::launch_button("term-skip", L::Shield, Some(IconName::ShieldAlert), "Skip Permissions", open(2), busy == 5, off, false), "skip", "Terminal \u{2014} Skip Permissions")
            .on_hover(cx.listener(|this, on: &bool, _, cx| this.hover_shield(2, *on, cx)))
            .on_click(cx.listener(|this, _, window, cx| this.terminal("skip", window, cx)));
        let shell = w::launch_button("empty-shell", L::Shell, None, "Empty Shell", true, busy == 3, off, false)
            .on_click(cx.listener(|this, _, window, cx| this.terminal("shell", window, cx)));
        let claude = div().flex().flex_col().items_center().gap(T::SP_10).mt(T::SP_4).py(T::LAUNCH_BOX_PY).px(T::LAUNCH_BOX_PX)
            .rounded(T::R_CARD).border_1().border_color(T::border())
            .child(div().text_size(T::LAUNCH_LABEL).font_weight(T::W_BOLD).text_color(T::text_muted()).child("CLAUDE"))
            .child(row(ui, ui_skip))
            .child(row(term, term_skip));
        div().size_full().flex().flex_col().items_center().justify_center().gap(T::SP_10).text_color(T::text_muted())
            .child(div().text_size(T::LAUNCH_MARK).opacity(0.3).child(">_"))
            .child(shell)
            .child(claude)
    }
}
