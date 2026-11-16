//! A chat's composer (Planning's, phase 2: 2.5-2.12b and voice 2.24;
//! AREAS.md, native-client/specs/composer.md): the message box, Send and
//! Stop, and the meta row (state, the permission mode, the Enter switch,
//! schedule, session id, model, cost).
//!
//! One file per feature (each owns its `State`, what it draws, its driver
//! actions and its props): mode (2.6), attach (2.7), drafts (2.8), queue
//! (2.9), sched (2.10), slash (2.11), secrets (2.12), sreq (2.12b), voice
//! (2.24). This file is the composer itself (2.5) and the glue: it places
//! each feature's part, routes the driver's actions to them and gathers their
//! props. Open composers are kept by chat id (`Composers`), so the part's
//! driver actions reach the focused chat's composer.
//!
//! 2.5's rules, as the Slint client's (deliberate differences in PLAN.md):
//! Send trims trailing whitespace, a blank box does nothing; idle, the chat
//! shows working at once (optimistic); working, the message is handed over
//! (2.9); needing input, held (2.9). A refused send puts the server's words
//! in the log as an error row, gives the text back to the box and the chat
//! back to its real state. Stop shows only while working: one click arms it
//! ("Sure?") for 3 s, a second interrupts. The Enter mode is one saved
//! setting (Saved.chat_enter) every open composer follows at once. Under
//! `composer-stack` of its own width the composer stacks (narrow).

pub mod attach;
pub mod drafts;
pub mod mode;
pub mod queue;
pub mod sched;
pub mod secrets;
pub mod slash;
pub mod sreq;
pub mod voice;

use crundi_chat_model::{ChatEvent, ChatModel};
use crundi_core::{chat as cchat, head, store};
use crundi_shell::{Area, AreaEvent};
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::input::{Enter, Escape, IndentInline, InputEvent, MoveDown, MoveUp};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::time::Duration;

/// Which key sends, in every chat (the web's crundi_chat_enter): false
/// Enter (Shift+Enter a newline), true Ctrl/Cmd+Enter (Enter a newline).
#[derive(Default)]
pub struct EnterMode { pub ctrl: bool }
impl Global for EnterMode {}

pub fn ctrl_enter(cx: &App) -> bool { cx.try_global::<EnterMode>().is_some_and(|e| e.ctrl) }

/// The Enter switch: saved, and every open composer follows it at once.
pub fn set_ctrl_enter(cx: &mut App, ctrl: bool) {
    cx.set_global(EnterMode { ctrl });
    store::update(|s| s.chat_enter = if ctrl { "newline".into() } else { String::new() });
    cx.refresh_windows();
}

pub struct ComposerView {
    pub model: Entity<ChatModel>,
    pub input: Entity<TextareaState>,
    /// Stop's first click (it disarms after 3 s; `stop_gen` tells timers apart).
    pub stop_armed: bool,
    stop_gen: u64,
    /// Sent from idle, until the server's state moves (the optimistic working).
    pub sending: bool,
    /// The composer's own width (narrow under composer-stack).
    pub width: f32,
    last_text: String,
    shown: (Option<bool>, String, bool),
    // Each feature's state (its own file).
    pub mode: mode::State,
    pub attach: attach::State,
    pub drafts: drafts::State,
    pub queue: queue::State,
    pub sched: sched::State,
    pub slash: slash::State,
    pub secrets: secrets::State,
    pub sreq: sreq::State,
    pub voice: voice::State,
    _subs: Vec<Subscription>,
}

use crundi_widgets::TextareaState;

impl ComposerView {
    pub fn new(model: Entity<ChatModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 9).placeholder("Message Claude…"));
        let subs = vec![
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, secondary } = ev { if *secondary == ctrl_enter(cx) { this.send(window, cx); } }
            }),
            cx.observe(&input, |this, _, cx| this.text_changed(cx)),
            cx.observe(&model, |this, _, cx| this.model_changed(cx)),
        ];
        let id = model.read(cx).id().to_string();
        let me = cx.entity().downgrade();
        if !cx.has_global::<Composers>() { cx.set_global(Composers::default()); }
        cx.global_mut::<Composers>().0.insert(id, me);
        ComposerView {
            model, input, stop_armed: false, stop_gen: 0, sending: false, width: 0., last_text: String::new(), shown: (None, String::new(), false),
            mode: Default::default(), attach: Default::default(), drafts: Default::default(), queue: Default::default(),
            sched: Default::default(), slash: Default::default(), secrets: Default::default(), sreq: Default::default(), voice: Default::default(),
            _subs: subs,
        }
    }

    pub fn chat_id(&self, cx: &App) -> String { self.model.read(cx).id().to_string() }

    /// The pane's text scale (A- / A+), the web's --cc-fs.
    pub fn fs(&self, cx: &App) -> f32 { head::font_scale(head::FONT_BASE + self.model.read(cx).zoom) }

    pub fn narrow(&self) -> bool { self.width > 0. && self.width < f32::from(T::COMPOSER_STACK) }

    /// The chat's state as the composer reads it (idle, working, needs-input, waiting, exited).
    pub fn state_now(&self, cx: &App) -> String { chat_state(cx, &self.chat_id(cx), self.sending) }

    fn text_changed(&mut self, cx: &mut Context<Self>) {
        let t = self.text(cx);
        if t == self.last_text { return; }
        self.last_text = t.clone();
        drafts::changed(self, &t, cx);
        slash::changed(self, &t);
        cx.notify();
    }

    fn model_changed(&mut self, cx: &mut Context<Self>) {
        let st = self.model.read(cx).conv.state.clone();
        if self.sending && !st.is_empty() && st != "idle" { self.sending = false; }
        let now = self.state_now(cx);
        if self.stop_armed && now != "working" { self.stop_armed = false; }
        queue::on_state(self, &now, cx);
        cx.notify();
    }

    /// Send what is in the box (the Send button, Enter, the test driver's "send").
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text(cx).trim_end().to_string();
        if text.trim().is_empty() { return; }
        self.set_text("", window, cx);
        self.send_text(text, cx);
    }

    /// Send a message by the chat's state: idle a plain send (working at
    /// once), working handed over, needing input held (2.9).
    pub fn send_text(&mut self, text: String, cx: &mut Context<Self>) {
        let text = text.trim_end().to_string();
        if text.trim().is_empty() { return; }
        self.model.update(cx, |m, _| m.error.clear());
        match self.state_now(cx).as_str() {
            "needs-input" => queue::hold(self, text, cx),
            "working" => self.post(text, true, cx),
            _ => { self.sending = true; self.post(text, false, cx); }
        }
        cx.notify();
    }

    /// POST /send. Handed over, its slot shows at once; the answer gives it
    /// its id, or drops it (the server was idle). Refused: given back.
    pub fn post(&mut self, text: String, handed: bool, cx: &mut Context<Self>) {
        if handed { self.model.update(cx, |m, cx| { m.conv.add_slot(&text); cx.notify(); }); }
        let stub = crundi_core::driver::take_stub("send");
        let path = format!("/api/ui-sessions/{}/send", self.chat_id(cx));
        let body = text.clone();
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| match stub { Some(v) => Ok(v), None => api.post(&path, &json!({ "text": body })) }, move |r, cx| {
            let _ = me.update(cx, |this, cx| match r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) != Some(false) => {
                    let injected = v.get("injected").and_then(|x| x.as_bool()).unwrap_or(false);
                    let uuid = v.get("uuid").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    this.model.update(cx, |m, cx| {
                        if injected && !m.conv.slots.iter().any(|x| x.text == text && x.uuid.is_empty()) { m.conv.add_slot(&text); }
                        m.conv.sent(&text, injected, &uuid);
                        cx.notify();
                    });
                }
                Ok(v) => { let e = v.get("error").and_then(|e| e.as_str()).filter(|e| !e.is_empty()).unwrap_or("Failed to send").to_string(); this.refused(text, e, cx) }
                Err(e) => this.refused(text, e, cx),
            });
        });
    }

    fn refused(&mut self, text: String, error: String, cx: &mut Context<Self>) {
        self.sending = false;
        self.model.update(cx, |m, cx| { m.conv.drop_slot(&text); cx.notify(); });
        self.push_error(&error, cx);
        self.give_back(&text, cx);
    }

    /// Text back in front of what the box holds (a refused send, an exit's held lines), the caret at the end.
    pub fn give_back(&mut self, text: &str, cx: &mut Context<Self>) {
        let back = cchat::given_back(text, &self.text(cx));
        let me = cx.entity();
        cx.defer(move |cx| in_window(cx, move |window, cx| me.update(cx, |c, cx| c.set_text(&back, window, cx))));
    }

    /// An error row in the chat's log (the client's own).
    pub fn push_error(&mut self, error: &str, cx: &mut Context<Self>) {
        let e = error.to_string();
        self.model.update(cx, |m, cx| {
            let n = m.conv.entries.len();
            m.conv.entries.push(json!({ "id": format!("local-error-{n}-{}", drafts::now_ms()), "kind": "error", "text": e }));
            cx.emit(ChatEvent::NewEntry);
            cx.notify();
        });
    }

    /// Stop: the first click arms it for 3 s; a second within them interrupts.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if !self.stop_armed {
            self.stop_armed = true;
            self.stop_gen += 1;
            let turn = self.stop_gen;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = this.update(cx, |c, cx| if c.stop_gen == turn && c.stop_armed { c.stop_armed = false; cx.notify(); });
            }).detach();
            cx.notify();
            return;
        }
        self.stop_armed = false;
        cx.notify();
        let path = format!("/api/ui-sessions/{}/interrupt", self.chat_id(cx));
        crundi_shell::call(cx, move |api| api.post(&path, &json!({})), |_, _| {});
    }

    /// Put text in the box, the caret at its end (the driver, drafts, a give-back).
    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let n = text.len();
        self.input.update(cx, |st, cx| { st.set_value(text.to_string(), window, cx); st.set_selected_range(n..n, cx); });
        self.text_changed(cx);
    }

    pub fn text(&self, cx: &App) -> String { self.input.read(cx).value().to_string() }

    /// The chat's pane fields (the Slint client's per-pane names), from every feature.
    pub fn pane_props(&self, cx: &App) -> Value {
        let m = self.model.read(cx);
        let sid = m.conv.session_id.clone();
        let mut o = Map::new();
        o.insert("draft".into(), json!(self.text(cx)));
        o.insert("state".into(), json!(self.state_now(cx)));
        o.insert("session".into(), json!(sid.chars().take(8).collect::<String>()));
        o.insert("sessionFull".into(), json!(sid));
        o.insert("model".into(), json!(m.conv.model));
        o.insert("cost".into(), json!(if m.conv.cost > 0.0 { format!("${:.4}", m.conv.cost) } else { String::new() }));
        o.insert("stopArmed".into(), json!(self.stop_armed));
        o.insert("composerNarrow".into(), json!(self.narrow()));
        for v in [mode::pane_props(self, cx), attach::pane_props(self, cx), drafts::pane_props(self, cx), queue::pane_props(self, cx),
                  sched::pane_props(self, cx), slash::pane_props(self, cx), secrets::pane_props(self, cx), sreq::pane_props(self, cx), voice::pane_props(self, cx)] {
            if let Value::Object(x) = v { o.extend(x); }
        }
        Value::Object(o)
    }

    /// The window-wide props of the features, for this (the focused) chat.
    /// pointerModal is true when any feature's is.
    pub fn props(&self, cx: &App) -> Value {
        let mut m = Map::new();
        let mut modal = false;
        for v in [mode::props(self, cx), attach::props(self, cx), drafts::props(self, cx), queue::props(self, cx),
                  sched::props(self, cx), slash::props(self, cx), secrets::props(self, cx), sreq::props(self, cx), voice::props(self, cx)] {
            if let Value::Object(o) = v { modal |= o.get("pointerModal") == Some(&json!(true)); m.extend(o); }
        }
        m.insert("pointerModal".into(), json!(modal));
        Value::Object(m)
    }

    /// A driver action for this composer: the composer's own, then the first feature that knows it.
    pub fn test_ui(&mut self, name: &str, args: &[String], window: &mut Window, cx: &mut Context<Self>) -> Option<Value> {
        match name {
            "chat-enter" => { set_ctrl_enter(cx, args.first().is_some_and(|a| a == "newline")); return Some(json!({ "ok": true })); }
            "composer-focus" => { self.input.update(cx, |st, cx| st.focus(window, cx)); return Some(json!({ "ok": true })); }
            "stop" => { self.stop(cx); return Some(json!({ "ok": true })); }
            // An event played into the chat as the server would send it (2.9's both drawers).
            "chat-event" => {
                let ev: Value = serde_json::from_str(args.first().map(String::as_str).unwrap_or("")).unwrap_or(Value::Null);
                self.model.update(cx, |m, cx| m.apply(&json!({ "type": "ui-event", "event": ev }), cx));
                return Some(json!({ "ok": true }));
            }
            _ => {}
        }
        mode::test_ui(self, name, args, window, cx)
            .or_else(|| attach::test_ui(self, name, args, window, cx))
            .or_else(|| drafts::test_ui(self, name, args, window, cx))
            .or_else(|| queue::test_ui(self, name, args, window, cx))
            .or_else(|| sched::test_ui(self, name, args, window, cx))
            .or_else(|| slash::test_ui(self, name, args, window, cx))
            .or_else(|| secrets::test_ui(self, name, args, window, cx))
            .or_else(|| sreq::test_ui(self, name, args, window, cx))
            .or_else(|| voice::test_ui(self, name, args, window, cx))
    }

    /// The placeholder, the Enter key's behaviour and the disabled look follow the state.
    fn sync_box(&mut self, exited: bool, window: &mut Window, cx: &mut Context<Self>) {
        let ctrl = ctrl_enter(cx);
        let ph = if exited { "Session ended." } else if self.narrow() { "Message Claude…" }
            else if ctrl { "Message Claude…  (Ctrl+Enter to send, Enter for newline)" } else { "Message Claude…  (Enter to send, Shift+Enter for newline)" };
        if self.shown.0 != Some(ctrl) { self.shown.0 = Some(ctrl); self.input.update(cx, |st, cx| st.set_submit_on_enter(!ctrl, cx)); }
        if self.shown.1 != ph { self.shown.1 = ph.to_string(); self.input.update(cx, |st, cx| st.set_placeholder(ph, window, cx)); }
        if self.shown.2 != exited { self.shown.2 = exited; self.input.update(cx, |st, cx| st.set_disabled(exited, cx)); }
    }
}

/// A chat's state for its composer: exited from the event stream's list,
/// else the chat's own; working at once after a send (head::composer_state).
pub fn chat_state(cx: &App, id: &str, sending: bool) -> String {
    let s = crundi_shell::session::get(cx);
    let listed = s.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(id));
    let st = match listed {
        Some(t) if head::chat_state(t) == "exited" => "exited".to_string(),
        _ => crundi_chat_model::find(cx, id).map(|m| m.read(cx).conv.state.clone()).unwrap_or_default(),
    };
    head::composer_state(&st, sending)
}

/// A chat's state as its composer shows it (for the pane's header and dot:
/// shared/requests.md asks the chats crate to use it).
pub fn state_of(id: &str, cx: &App) -> String {
    let sending = composer(cx, id).is_some_and(|c| c.read(cx).sending);
    chat_state(cx, id, sending)
}

/// Something in the first window (a give-back, a toast) from a callback with only the App.
pub(crate) fn in_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    if let Some(h) = cx.windows().first().copied() { let _ = cx.update_window(h, |_, window, cx| f(window, cx)); }
}

/// Later, on the UI thread.
pub(crate) fn after(cx: &mut App, ms: u64, f: impl FnOnce(&mut App) + 'static) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_millis(ms)).await;
        let _ = cx.update(f);
    }).detach();
}

/// The state label: words and colour.
fn state_label(st: &str) -> (&'static str, Hsla) {
    match st {
        "working" => ("working…", T::accent_hover()),
        "needs-input" => ("needs your input", T::accent_hover()),
        "exited" => ("exited", T::text_muted()),
        _ => ("idle", T::text_muted()),
    }
}

impl Render for ComposerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drafts::restore_if_needed(self, window, cx);
        let st = self.state_now(cx);
        let exited = st == "exited";
        self.sync_box(exited, window, cx);
        let (narrow, fs) = (self.narrow(), self.fs(cx));
        // The features' parts above the box (queue, dock…), and over it (menus, dialogs).
        let parts: Vec<AnyElement> = [
            sreq::render(self, window, cx), queue::render(self, window, cx), attach::render(self, window, cx), slash::render(self, window, cx),
            sched::render(self, window, cx), mode::render(self, window, cx), secrets::render(self, window, cx), voice::render(self, window, cx),
            drafts::render(self, window, cx),
        ].into_iter().flatten().collect();
        let (sid, model, cost) = { let m = self.model.read(cx); (m.conv.session_id.clone(), m.conv.model.clone(), m.conv.cost) };
        let ctrl = ctrl_enter(cx);
        let me = cx.entity().downgrade();
        let recording = voice::recording(self, cx);

        // The box (or the recording strip's place), with its keys: the slash
        // menu's (↑ ↓ Enter Tab Esc) and ↑'s take-back come before the box's.
        let the_box = if recording { voice::rec_box(self, window, cx) } else {
            div().flex_1().min_w_0().when(exited, |d| d.opacity(0.5))
                .capture_action(cx.listener(|this, a: &Enter, window, cx| {
                    if slash::enter(this, window, cx) { cx.stop_propagation(); return; }
                    // The Enter that would send, in a blank box: nothing happens (no
                    // send, no newline), as the web's (2.5).
                    if !a.shift && a.secondary == ctrl_enter(cx) && this.text(cx).trim().is_empty() { cx.stop_propagation(); }
                }))
                .capture_action(cx.listener(|this, _: &IndentInline, window, cx| if slash::enter(this, window, cx) { cx.stop_propagation(); }))
                .capture_action(cx.listener(|this, _: &MoveUp, window, cx| if slash::step(this, -1, cx) || queue::up(this, window, cx) { cx.stop_propagation(); }))
                .capture_action(cx.listener(|this, _: &MoveDown, _, cx| if slash::step(this, 1, cx) { cx.stop_propagation(); }))
                .capture_action(cx.listener(|this, _: &Escape, _, cx| if slash::escape(this, cx) { cx.stop_propagation(); }))
                .capture_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| if attach::shift_insert(this, e, window, cx) { cx.stop_propagation(); }))
                .child(w::message_box(&self.input, fs).on_paste(attach::paste_handler(cx)))
                .into_any_element()
        };
        let stop = (st == "working").then(|| w::stop_button("stop", self.stop_armed, fs).on_click(cx.listener(|this, _, _, cx| this.stop(cx))).into_any_element());
        let busy = matches!(st.as_str(), "working" | "needs-input");
        let send = if recording {
            div().flex().items_center().gap(T::SP_8).when(narrow, |d| d.flex_1()).child(voice::strip(self, window, cx)).child(voice::cancel_x(self, window, cx)).into_any_element()
        } else {
            // One Send control in every state (the owner, 16 Nov): the shared
            // button at the control height. Wide, it sits beside the box at its
            // own width; narrow (stacked), Send / Queue fills the rest of the
            // row under the box (specs/composer.md, web-narrow).
            let b = w::send_button("send", if busy { "Queue" } else { "Send" }, fs)
                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)));
            if narrow { div().flex_1().flex().child(b.w_full()).into_any_element() } else { b.into_any_element() }
        };
        let mic = voice::mic(self, window, cx);
        let attach_btn = attach::button(self, window, cx);
        let row = if narrow {
            div().flex().flex_col().gap(T::SP_8)
                .child(div().flex().child(the_box))
                .child(div().flex().items_center().gap(T::SP_8).child(attach_btn).child(sched::clock_button(self, true, cx)).children(stop).children(mic).child(send))
        } else {
            div().flex().items_end().gap(T::SP_8).child(the_box).child(attach_btn).children(stop).children(mic).child(send)
        };

        // The meta row.
        let (label, label_c) = state_label(&st);
        let meta_text = if narrow { T::TEXT_10 } else { T::TEXT_11 } * fs;
        let me2 = me.clone();
        let switch = w::tip(div().id("enter-switch").flex().items_center().gap(T::SP_4)
            .child(w::segmented("enter", &["⏎", "⌃⏎"], if ctrl { 1 } else { 0 }, move |i, window, cx| {
                set_ctrl_enter(cx, *i == 1);
                let _ = me2.update(cx, |c, cx| c.input.update(cx, |st, cx| st.focus(window, cx)));
            }))
            .child(div().text_color(T::text_secondary()).hover(|d| d.text_color(T::text())).child("sends")),
            if ctrl { "Enter makes a newline, Ctrl+Enter sends — click to swap" } else { "Enter sends, Shift+Enter makes a newline — click to swap" });
        let sid_el = (!narrow && !sid.is_empty()).then(|| {
            let full = sid.clone();
            w::tip(div().id("sid").font_family(T::MONO).cursor_pointer().border_b_1().border_dashed().border_color(T::border())
                .hover(|d| d.text_color(T::text_secondary())).child(sid.chars().take(8).collect::<String>()),
                format!("Claude session {sid}  — click to copy (use it to resume this conversation)"))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(full.clone()));
                    attach::toast(cx, &format!("Session id copied: {full}"), false);
                }).into_any_element()
        });
        let meta = div().flex().flex_wrap().items_center().gap(if narrow { T::SP_6 } else { T::SP_8 }).text_size(meta_text).text_color(T::text_muted())
            .child(div().text_color(label_c).child(label))
            .child(mode::button(self, narrow, fs, cx))
            .child(switch)
            .when(!narrow, |d| d.child(sched::clock_button(self, false, cx)))
            .child(div().flex_1())
            .children(sid_el)
            .when(!model.is_empty(), |d| d.child(model))
            .when(cost > 0.0, |d| d.child(format!("${cost:.4}")));

        div().relative().flex().flex_col().gap(T::SP_6).p(T::SP_8).border_t_1().border_color(T::border()).bg(T::bg_secondary())
            .child(canvas(move |b, _, cx| {
                let wd = f32::from(b.size.width);
                let _ = me.update(cx, |c, cx| if (c.width - wd).abs() > 0.5 { c.width = wd; cx.notify(); });
            }, |_, _, _, _| {}).absolute().size_full())
            .children(parts)
            .child(row)
            .children(attach::strip(self, window, cx))
            .child(meta)
    }
}

/// Every open composer, by chat id.
#[derive(Default)]
pub struct Composers(pub HashMap<String, WeakEntity<ComposerView>>);
impl Global for Composers {}

/// A chat's composer, if its pane has been drawn.
pub fn composer(cx: &App, chat: &str) -> Option<Entity<ComposerView>> {
    cx.try_global::<Composers>().and_then(|c| c.0.get(chat)).and_then(|w| w.upgrade())
}

/// A chat's pane fields (for the workbench's pane list; see shared/requests.md).
thread_local! {
    /// Set while the composer lists the workbench's panes for itself: the
    /// workbench then asks every pane-props hook, and ours must not read a
    /// composer that may be in the middle of an update (GPUI panics).
    static QUIET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The workbench's panes (keys, focus, sizes) without the composer's own
/// pane fields: what the composer's code uses inside its updates.
pub(crate) fn wb_panes(cx: &mut App) -> Vec<Value> {
    QUIET.with(|q| q.set(true));
    let r = crundi_workbench::workbench(cx).map(|w| w.update(cx, |w, cx| w.panes(cx))).unwrap_or_default();
    QUIET.with(|q| q.set(false));
    r
}

pub fn pane_props(chat: &str, cx: &App) -> Value {
    if QUIET.with(|q| q.get()) { return json!({}); }
    if let Some(c) = composer(cx, chat) { return c.read(cx).pane_props(cx); }
    // Its pane isn't drawn yet: the same fields, from the chat's model.
    let (sid, model, cost) = crundi_chat_model::find(cx, chat).map(|m| { let m = m.read(cx); (m.conv.session_id.clone(), m.conv.model.clone(), m.conv.cost) }).unwrap_or_default();
    json!({
        "draft": "", "state": chat_state(cx, chat, false),
        "session": sid.chars().take(8).collect::<String>(), "sessionFull": sid, "model": model,
        "cost": if cost > 0.0 { format!("${cost:.4}") } else { String::new() },
        "stopArmed": false,
    })
}

/// The chat in the focused pane (else the first chat pane).
pub fn focused_chat(cx: &mut App) -> String {
    let panes = crate::wb_panes(cx);
    let chat = |p: &Value| p.get("key").and_then(|k| k.as_str()).and_then(|k| k.strip_prefix("live:")).map(String::from);
    panes.iter().find(|p| p.get("focused") == Some(&json!(true))).and_then(chat).or_else(|| panes.iter().find_map(chat)).unwrap_or_default()
}

/// Register the composer's part: its driver actions go to the focused chat's
/// composer; its props are the features' window-wide ones plus every open
/// composer's pane fields (props.composer.panes, until the workbench merges them).
pub fn init(cx: &mut App) {
    cx.set_global(EnterMode { ctrl: store::load().chat_enter == "newline" });
    attach::init(cx); voice::init(cx); // media: attach's window state; voice's area (mic-*, voice-say) and pump
    secrets::init(cx); sreq::init(cx); // secrets: their own areas (chat-secrets, sreq): live events, sec-*/sreq-* actions, props
    mode::init(cx);
    // The chat panes' fields in the driver's pane list, and the header's lock
    // opening this chat's secrets dialog (the workbench's hooks).
    crundi_workbench::register_pane_props(cx, "live:", |key, cx| pane_props(key.trim_start_matches("live:"), cx));
    crundi_workbench::set_on_secrets(cx, |chat, window, cx| secrets::open(chat, window, cx));
    cx.on_app_quit(|cx| { drafts::flush(cx); async {} }).detach();
    crundi_shell::register(cx, Area::new("composer")
        .on_event(|e, cx| if let AreaEvent::State(_) = e { drafts::sync(cx); })
        .on_project(|_, cx| drafts::sync(cx))
        .test_ui(|name, args, window, cx| {
            // The test's stand-in for the workbench's focus-pane (2.1-2.4
            // build it): the pane with that leaf id takes the focus.
            if name == "focus-leaf" {
                let leaf: u32 = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
                if let Some(wb) = crundi_workbench::workbench(cx) { wb.update(cx, |w, cx| { w.focused = leaf; cx.notify(); }); }
                return Some(json!({ "ok": true }));
            }
            let id = focused_chat(cx);
            let c = composer(cx, &id)?;
            c.update(cx, |c, cx| c.test_ui(name, args, window, cx))
        })
        .props(|_, cx| {
            let id = focused_chat(cx);
            let mut out = composer(cx, &id).map(|c| c.read(cx).props(cx)).unwrap_or_else(|| json!({ "modePicker": null, "pointerModal": false }));
            out["chatEnter"] = json!(if ctrl_enter(cx) { "newline" } else { "enter" });
            let panes: Map<String, Value> = cx.try_global::<Composers>().map(|c| c.0.iter().filter_map(|(k, w)| w.upgrade().map(|e| (k.clone(), e.read(cx).pane_props(cx)))).collect()).unwrap_or_default();
            out["composer"] = json!({ "panes": panes });
            out
        }));
}
