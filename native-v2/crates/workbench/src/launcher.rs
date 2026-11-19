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

/// Start a terminal in a launcher's pane (its key, mode shell | normal | skip).
pub type LaunchTerminal = Rc<dyn Fn(&str, &'static str, &mut Window, &mut App)>;
struct TermLauncher(LaunchTerminal);
impl Global for TermLauncher {}
pub fn set_terminal_launcher(cx: &mut App, f: LaunchTerminal) { cx.set_global(TermLauncher(f)); }

pub struct Launcher {
    key: String,
    /// The pressed button while its launch is under way: 1 UI Mode, 2 its
    /// shield, 3 Empty Shell, 4 Terminal, 5 its shield.
    busy: u8,
    /// A row whose shield is open (hovered, or peeking): 1 or 2.
    open: u8,
    peek: u8,
    _peek: Option<Task<()>>,
}

impl Launcher {
    pub fn new(key: &str, cx: &mut Context<Self>) -> Self {
        // The shields peek open once on first paint (the web's peekShields:
        // the first row at 300 ms for 900 ms, the second 1150 ms later).
        let peek = cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            for (row, wait) in [(1u8, 300u64), (0, 900), (2, 250), (0, 900)] {
                cx.background_executor().timer(Duration::from_millis(wait)).await;
                if this.update(cx, |l, cx| { l.peek = row; cx.notify(); }).is_err() { return; }
            }
        });
        Launcher { key: key.to_string(), busy: 0, open: 0, peek: 0, _peek: Some(peek) }
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
        let ui = w::launch_button("ui-mode", L::Chat, None, "UI Mode", !open(1), busy == 1, off)
            .on_click(cx.listener(|this, _, _, cx| this.chat(false, cx)));
        let ui_skip = w::launch_button("ui-skip", L::Shield, Some(IconName::ShieldAlert), "Skip Permissions", open(1), busy == 2, off)
            .tooltip(w::tooltip("UI Mode \u{2014} Skip Permissions"))
            .on_hover(cx.listener(|this, on: &bool, _, cx| this.hover_shield(1, *on, cx)))
            .on_click(cx.listener(|this, _, _, cx| this.chat(true, cx)));
        let term = w::launch_button("term-mode", L::Normal, Some(IconName::Terminal), "Terminal", !open(2), busy == 4, off)
            .on_click(cx.listener(|this, _, window, cx| this.terminal("normal", window, cx)));
        let term_skip = w::launch_button("term-skip", L::Shield, Some(IconName::ShieldAlert), "Skip Permissions", open(2), busy == 5, off)
            .tooltip(w::tooltip("Terminal \u{2014} Skip Permissions"))
            .on_hover(cx.listener(|this, on: &bool, _, cx| this.hover_shield(2, *on, cx)))
            .on_click(cx.listener(|this, _, window, cx| this.terminal("skip", window, cx)));
        let shell = w::launch_button("empty-shell", L::Shell, None, "Empty Shell", true, busy == 3, off)
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
