//! The chat's composer (2.5-2.11, 2.24; the web's claude-chat.js mount():
//! .cc-composer; native-gpui/crates/chat-composer and its specs): the
//! message box, attach, Stop, the mic, Send / Queue, and the meta row (the
//! state, the permission mode, the Enter switch, the schedule clock, the
//! session id, the model, the cost). One module per feature: enter (the
//! Enter switch), mode (2.6), attach (2.7), drafts (2.8), queue (2.9), sched
//! (2.10), slash (2.11), voice (2.24).
//!
//! A cached leaf (ARCHITECTURE.md rule 3): it observes only the chat's
//! `ChatMeta` (state, model, cost, mode, the handed-over slots, the slash
//! commands: never the streamed text), the projects store for "exited"
//! (notifying itself only when that flips), the Enter setting and the voice
//! state (only what concerns this chat), so a streaming log never redraws
//! it. Its buttons and rows paint in layers; the box only while unfocused.
//!
//! 2.5's rules: Send trims trailing whitespace and a blank box does
//! nothing; idle, the chat shows working at once (optimistic); working, the
//! message is handed over (2.9); needing input, held (2.9). A refused send
//! puts the server's words in the log as an error row, gives the text back
//! and the chat back to its real state. Stop shows only while working: one
//! click arms it ("Sure?") for 3 s, a second interrupts. Exited: the box
//! disabled and dimmed, "Session ended.". Under composer-stack of its own
//! width the composer stacks (narrow).

pub mod attach;
pub mod drafts;
pub mod enter;
pub mod mode;
pub mod queue;
pub mod sched;
pub mod secrets;
pub mod slash;
pub mod voice;

/// 2.12, for the workbench's head ("Secrets for this chat"): open the
/// dialog, the lock's count, and the entity to observe for its redraws.
pub use secrets::{open_secrets, secrets_count, secrets_entity, secrets_tip, Secrets};

use crate::store::{scale, ChatMeta, ChatStore, Meta};
use crundi_core::{chat as cchat, head};
use crundi_shell::Stores;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::input::{Enter, Escape, IndentInline, InputEvent, MoveDown, MoveUp};
use serde_json::{json, Value};
use std::time::Duration;

pub struct Composer {
    pub(crate) store: Entity<ChatStore>,
    pub(crate) id: String,
    /// The chat's meta, as last observed (what this view draws).
    pub(crate) meta: Meta,
    pub(crate) input: Entity<w::TextareaState>,
    stores: Stores,
    /// The event stream lists the chat as ended.
    pub(crate) exited: bool,
    /// The Enter setting (enter.rs), as last observed.
    ctrl: bool,
    pub(crate) stop_armed: bool,
    stop_gen: u64,
    /// Its content's size, as last laid out: the height is its cached box's
    /// (the pane's body reads it), the width decides narrow.
    pub height: Pixels,
    pub width: Pixels,
    last_text: String,
    /// What the box was last set to: submit on Enter, placeholder, disabled.
    shown: (Option<bool>, String, Option<bool>),
    pub(crate) slash: slash::State,
    pub(crate) queue: queue::State,
    pub(crate) attach: attach::State,
    /// The schedule panel while open (2.10): a view of its own, drawn by
    /// the pane's body over the chat's foot (the web's .cc-schedpanel).
    pub(crate) sched: Option<(Entity<sched::SchedPanel>, Pixels)>,
    _subs: Vec<Subscription>,
}

impl Composer {
    pub fn new(store: Entity<ChatStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let id = store.read(cx).id.clone();
        let stores = crundi_shell::stores(cx);
        let em = enter::mode(cx);
        let ctrl = em.read(cx).ctrl;
        let input = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(1, 9).placeholder(enter::placeholder(ctrl, false, false)));
        let meta_e = store.read(cx).meta.clone();
        let mut subs = vec![
            cx.observe(&meta_e, |this: &mut Self, m: Entity<ChatMeta>, cx| { let n = m.read(cx).0.clone(); this.meta_changed(n, cx); }),
            cx.observe_in(&stores.projects, window, |this: &mut Self, _, window, cx| {
                let ex = exited_in(&this.stores, &this.id, cx);
                if ex != this.exited { this.exited = ex; this.state_moved(cx); this.sync_box(window, cx); cx.notify(); }
            }),
            cx.observe_in(&em, window, |this: &mut Self, e, window, cx| {
                let c = e.read(cx).ctrl;
                if c != this.ctrl { this.ctrl = c; this.sync_box(window, cx); cx.notify(); }
            }),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { shift: false, secondary } => if *secondary == this.ctrl { this.send(window, cx); },
                InputEvent::Change => this.text_changed(cx),
                _ => {}
            }),
        ];
        subs.extend(w::watch_focus(&input.focus_handle(cx), window, cx));
        subs.extend(voice::watch(&id, cx));
        // The pane gone: its draft's claim on the project's copy goes (2.8).
        let chat = id.clone();
        cx.on_release(move |_, cx| drafts::release(cx, &chat)).detach();
        let exited = exited_in(&stores, &id, cx);
        let meta = meta_e.read(cx).0.clone();
        let mut c = Composer {
            store, id, meta, input, stores, exited, ctrl, stop_armed: false, stop_gen: 0, height: px(96.), width: px(0.), last_text: String::new(),
            shown: (None, String::new(), None), slash: Default::default(), queue: Default::default(), attach: Default::default(),
            sched: None, _subs: subs,
        };
        drafts::restore(&mut c, window, cx);
        c.sync_box(window, cx);
        c
    }

    pub fn chat_id(&self) -> &str { &self.id }

    /// The pane's text scale (A- / A+).
    pub fn fs(&self) -> f32 { scale(self.meta.zoom) }

    /// Narrower than composer-stack (its own width, as the web's).
    pub fn narrow(&self) -> bool { self.width > px(0.) && self.width < T::COMPOSER_STACK }

    /// The chat's state as the composer reads it: exited (the event
    /// stream's list), else the chat's own, working at once after a send.
    pub fn state_now(&self) -> String {
        if self.exited { "exited".into() } else { head::composer_state(&self.meta.state, self.meta.sending) }
    }

    fn meta_changed(&mut self, n: Meta, cx: &mut Context<Self>) {
        // (The title is the head's: a new one draws nothing here.)
        let same = Meta { title: n.title.clone(), ..self.meta.clone() } == n;
        self.meta = n;
        if same { return; }
        self.state_moved(cx);
        cx.notify();
    }

    /// The state may have moved: Stop disarms when it leaves working; held
    /// lines go out (2.9).
    fn state_moved(&mut self, cx: &mut Context<Self>) {
        let now = self.state_now();
        if self.stop_armed && now != "working" { self.stop_armed = false; }
        queue::on_state(self, &now, cx);
        if now == "exited" { voice::chat_ended(cx, &self.id); }
    }

    fn text_changed(&mut self, cx: &mut Context<Self>) {
        let t = self.text(cx);
        if t == self.last_text { return; }
        self.last_text = t.clone();
        drafts::changed(self, &t, cx);
        slash::changed(self, &t);
        cx.notify();
    }

    /// The box was changed in code (an insertion): the draft and the menu follow.
    pub(crate) fn text_edited(&mut self, cx: &mut Context<Self>) { self.text_changed(cx); }

    /// The text the box was given without an edit (a draft taken back).
    pub(crate) fn last_text_set(&mut self, t: String) { self.last_text = t; }

    pub fn text(&self, cx: &App) -> String { self.input.read(cx).value().to_string() }

    /// Put text in the box, the caret at its end (the driver, drafts, a give-back).
    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let n = text.len();
        self.input.update(cx, |st, cx| { st.set_value(text.to_string(), window, cx); st.set_selected_range(n..n, cx); });
        self.text_changed(cx);
    }

    pub fn focus_box(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |st, cx| st.focus(window, cx));
    }

    /// Send what is in the box (Send, Enter, the driver's "send").
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text(cx).trim_end().to_string();
        if text.trim().is_empty() { return; }
        // `/btw <question>` is asked on the side (2.21: the log's aside).
        if crate::log::aside::ask_from_box(cx, &self.id.clone(), &text) { self.set_text("", window, cx); return; }
        self.set_text("", window, cx);
        // The log goes to the end and follows it again.
        self.store.read(cx).list.set_follow_mode(FollowMode::Tail);
        self.send_text(text, cx);
    }

    /// A message by the chat's state: idle a plain send (working at once),
    /// working handed over, needing input held (2.9).
    pub fn send_text(&mut self, text: String, cx: &mut Context<Self>) {
        let text = text.trim_end().to_string();
        if text.trim().is_empty() { return; }
        match self.state_now().as_str() {
            "needs-input" => queue::hold(self, text, cx),
            "working" => self.post(text, true, cx),
            _ => {
                let mut m = self.meta.clone();
                m.sending = true;
                m.error.clear();
                self.store.update(cx, |s, cx| s.set_meta(m, cx));
                self.post(text, false, cx);
            }
        }
        cx.notify();
    }

    /// POST /send. Handed over, its slot shows at once; the answer gives it
    /// its id, or drops it (the server was idle). Refused: given back.
    pub fn post(&mut self, text: String, handed: bool, cx: &mut Context<Self>) {
        if handed { let t = text.clone(); self.store.update(cx, |s, cx| s.edit_conv(|c| c.add_slot(&t), cx)); }
        let stub = crundi_core::driver::take_stub("send");
        let path = format!("/api/ui-sessions/{}/send", self.id);
        let body = text.clone();
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| match stub { Some(v) => Ok(v), None => api.post(&path, &json!({ "text": body })) }, move |r, cx| {
            let _ = me.update(cx, |this, cx| match r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) != Some(false) => {
                    let injected = v.get("injected").and_then(|x| x.as_bool()).unwrap_or(false);
                    let uuid = v.get("uuid").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    this.store.update(cx, |s, cx| s.edit_conv(|c| {
                        if injected && !c.slots.iter().any(|x| x.text == text && x.uuid.is_empty()) { c.add_slot(&text); }
                        c.sent(&text, injected, &uuid);
                    }, cx));
                }
                Ok(v) => { let e = v.get("error").and_then(|e| e.as_str()).filter(|e| !e.is_empty()).unwrap_or("Failed to send").to_string(); this.refused(text, e, cx) }
                Err(e) => this.refused(text, e, cx),
            });
        });
    }

    fn refused(&mut self, text: String, error: String, cx: &mut Context<Self>) {
        let mut m = self.store.read(cx).meta(cx);
        m.sending = false;
        self.store.update(cx, |s, cx| { s.set_meta(m, cx); s.edit_conv(|c| c.drop_slot(&text), cx); });
        self.push_error(&error, cx);
        self.give_back(&text, cx);
    }

    /// Text back in front of what the box holds (a refused send, an exit's
    /// held lines), the caret at the end.
    pub fn give_back(&mut self, text: &str, cx: &mut Context<Self>) {
        let back = cchat::given_back(text, &self.text(cx));
        let me = cx.entity();
        cx.defer(move |cx| in_window(cx, move |window, cx| me.update(cx, |c, cx| c.set_text(&back, window, cx))));
    }

    /// An error row in the chat's log (the client's own, as the web's).
    pub fn push_error(&mut self, error: &str, cx: &mut Context<Self>) {
        let e = error.to_string();
        let id = format!("local-error-{}", drafts::now_ms());
        self.store.update(cx, |s, cx| s.edit_conv(|c| { c.apply(&json!({ "type": "entry", "entry": { "id": id, "kind": "error", "text": e } })); }, cx));
    }

    /// Stop: the first click arms it for 3 s; a second within them interrupts.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if !self.stop_armed {
            self.stop_armed = true;
            self.stop_gen += 1;
            let turn = self.stop_gen;
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = this.update(cx, |c, cx| if c.stop_gen == turn && c.stop_armed { c.stop_armed = false; cx.notify(); });
            }).detach();
            cx.notify();
            return;
        }
        self.stop_armed = false;
        cx.notify();
        let path = format!("/api/ui-sessions/{}/interrupt", self.id);
        crundi_shell::call(cx, move |api| api.post(&path, &json!({})), |_, _| {});
    }

    /// The placeholder, the Enter key's behaviour and the disabled look
    /// follow the state, the width and the setting.
    fn sync_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (ctrl, exited) = (self.ctrl, self.exited);
        let ph = enter::placeholder(ctrl, self.narrow(), exited);
        if self.shown.0 != Some(ctrl) { self.shown.0 = Some(ctrl); self.input.update(cx, |st, cx| st.set_submit_on_enter(!ctrl, cx)); }
        if self.shown.1 != ph { self.shown.1 = ph.to_string(); self.input.update(cx, |st, cx| st.set_placeholder(ph, window, cx)); }
        if self.shown.2 != Some(exited) { self.shown.2 = Some(exited); self.input.update(cx, |st, cx| st.set_disabled(exited, cx)); }
    }

    /// Its size as laid out (after the frame): a new height notifies (the
    /// pane's body places it at that height), a new width may make it narrow.
    fn measured(&mut self, size: Size<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        if (self.height - size.height).abs() > px(0.5) { self.height = size.height; changed = true; }
        if (self.width - size.width).abs() > px(0.5) {
            let was = self.narrow();
            self.width = size.width;
            if was != self.narrow() { self.sync_box(window, cx); changed = true; }
        }
        if changed { cx.notify(); }
    }

    /// The session id: copied, and a toast says so.
    fn copy_sid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let full = self.meta.session_id.clone();
        cx.write_to_clipboard(ClipboardItem::new_string(full.clone()));
        w::toast(window, cx, format!("Session id copied: {full}"), w::Toast::Info);
    }

    // ─── The test driver ───

    /// A driver action on this composer: its own, then each feature's.
    pub fn test_ui(&mut self, name: &str, args: &[String], window: &mut Window, cx: &mut Context<Self>) -> Option<Value> {
        let a = |i: usize| args.get(i).cloned().unwrap_or_default();
        match name {
            // Text into the box as typed (each change as an edit).
            "composer-type" => { let t = a(0); self.focus_box(window, cx); self.input.update(cx, |st, cx| { let n = st.value().len(); st.set_selected_range(n..n, cx); st.replace(t, window, cx); }); }
            "composer-text" => { let t = a(0); self.set_text(&t, window, cx); }
            "composer-focus" => self.focus_box(window, cx),
            "composer-blur" => window.blur(cx),
            "composer-send" => self.send(window, cx),
            "composer-stop" => self.stop(cx),
            "composer-sid" => self.copy_sid(window, cx),
            "chat-enter" => { enter::set(cx, a(0) == "newline"); }
            // A reference dragged from another panel, dropped on this chat.
            "composer-ref-drop" => { let (me, t) = (cx.entity(), a(0)); cx.defer(move |cx| ref_drop(&me, &t, cx)); }
            // An event played into the chat as the server would send it.
            "composer-event" => {
                let ev: Value = serde_json::from_str(&a(0)).unwrap_or(Value::Null);
                let id = self.id.clone();
                self.store.update(cx, |s, cx| s.apply_batch(&[json!({ "type": "ui-event", "id": id, "event": ev })], cx));
            }
            _ => return mode::test_ui(self, name, args, window, cx)
                .or_else(|| slash::test_ui(self, name, args, window, cx))
                .or_else(|| queue::test_ui(self, name, args, window, cx))
                .or_else(|| attach::test_ui(self, name, args, window, cx))
                .or_else(|| sched::test_ui(self, name, args, window, cx))
                .or_else(|| voice::test_ui(self, name, args, window, cx)),
        }
        Some(json!({ "ok": true }))
    }

    /// What the driver's state shows of this composer.
    pub fn state(&self, cx: &App) -> Value {
        let m = &self.meta;
        let sid = m.session_id.clone();
        let mut o = json!({
            "text": self.text(cx), "state": self.state_now(), "placeholder": self.shown.1, "disabled": self.exited,
            "send": if matches!(self.state_now().as_str(), "working" | "needs-input") { "Queue" } else { "Send" },
            "stop": self.state_now() == "working", "stopArmed": self.stop_armed,
            "session": if self.narrow() { String::new() } else { sid.chars().take(8).collect::<String>() }, "sessionFull": sid,
            "model": m.model, "cost": if m.cost > 0.0 { format!("${:.4}", m.cost) } else { String::new() },
            "narrow": self.narrow(), "width": f32::from(self.width), "height": f32::from(self.height),
            "enter": if self.ctrl { "newline" } else { "enter" },
        });
        for v in [mode::state(self, cx), slash::state(self, cx), queue::state(self, cx), attach::state(self, cx), sched::state(self, cx), voice::state(self, cx), drafts::state(self, cx)] {
            if let (Value::Object(o), Value::Object(x)) = (&mut o, v) { o.extend(x); }
        }
        o
    }
}

/// The chat is listed as ended (or parked) by the event stream.
fn exited_in(stores: &Stores, id: &str, cx: &App) -> bool {
    stores.projects.read(cx).terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(id)).is_some_and(|t| head::chat_state(t) == "exited")
}

/// Something in the first window (a give-back, a toast) from a callback with only the App.
pub(crate) fn in_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    if let Some(h) = cx.windows().first().copied() { let _ = cx.update_window(h, |_, window, cx| f(window, cx)); }
}

/// The same, with what `f` answers (None: no window).
pub(crate) fn in_window_ret<R>(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) -> R) -> Option<R> {
    let h = cx.windows().first().copied()?;
    cx.update_window(h, |_, window, cx| f(window, cx)).ok()
}

/// The composer's parts that live beside the panes (lib.rs's init): voice
/// (Claude's spoken replies, Esc), the chat secrets (2.12).
pub fn init(cx: &mut App) {
    voice::init(cx);
    secrets::init(cx);
}

/// Later, on the UI thread.
pub(crate) fn after(cx: &mut App, ms: u64, f: impl FnOnce(&mut App) + 'static) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_millis(ms)).await;
        let _ = cx.update(f);
    }).detach();
}

/// A toast from a callback with only the App.
pub(crate) fn toast(cx: &mut App, text: &str, error: bool) {
    let t = text.to_string();
    in_window(cx, move |window, cx| w::toast(window, cx, t, if error { w::Toast::Error } else { w::Toast::Info }));
}

/// The state label: words and colour (a waiting chat reads idle).
fn state_label(st: &str) -> (&'static str, Hsla) {
    match st {
        "working" => ("working…", T::accent_hover()),
        "needs-input" => ("needs your input", T::accent_hover()),
        "exited" => ("exited", T::text_muted()),
        _ => ("idle", T::text_muted()),
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("composer");
        let st = self.state_now();
        let exited = st == "exited";
        let (narrow, fs) = (self.narrow(), self.fs());
        let recording = voice::recording(self, cx);
        let busy = matches!(st.as_str(), "working" | "needs-input");

        // The box (or, recording, the words heard in its place), with its
        // keys: the slash menu's (↑ ↓ Enter Tab Esc) and ↑'s take-back come
        // before the box's own.
        let focus = self.input.focus_handle(cx);
        let the_box = if recording { voice::rec_box(self, cx) } else {
            let mbox = w::message_box(&self.input, fs).on_paste(attach::paste_handler(cx));
            div().flex_1().min_w_0().when(exited, |d| d.opacity(T::MEDIA_O_SOFT))
                .capture_action(cx.listener(|this, a: &Enter, window, cx| {
                    if slash::enter(this, window, cx) { cx.stop_propagation(); return; }
                    // The Enter that would send, in a blank box: nothing (no send, no newline).
                    if !a.shift && a.secondary == this.ctrl && this.text(cx).trim().is_empty() { cx.stop_propagation(); }
                }))
                .capture_action(cx.listener(|this, _: &IndentInline, window, cx| if slash::enter(this, window, cx) { cx.stop_propagation(); }))
                .capture_action(cx.listener(|this, _: &MoveUp, window, cx| if slash::step(this, -1, cx) || queue::up(this, window, cx) { cx.stop_propagation(); }))
                .capture_action(cx.listener(|this, _: &MoveDown, _, cx| if slash::step(this, 1, cx) { cx.stop_propagation(); }))
                .capture_action(cx.listener(|this, _: &Escape, _, cx| if slash::escape(this, cx) { cx.stop_propagation(); }))
                .capture_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| if attach::shift_insert(this, e, window, cx) { cx.stop_propagation(); }))
                .child(w::text_box(&focus, mbox, window, cx))
                .into_any_element()
        };
        let stop = (st == "working").then(|| w::layer(w::stop_button("stop", self.stop_armed, fs).on_click(cx.listener(|this, _, _, cx| this.stop(cx)))).into_any_element());
        let send = if recording {
            div().flex().items_center().gap(T::SP_8).when(narrow, |d| d.flex_1()).child(voice::strip(self, cx)).child(voice::cancel_x(self, cx)).into_any_element()
        } else {
            // Wide, Send sits beside the box at its own width; narrow, it
            // fills the rest of the row under the box.
            let b = w::send_button("send", if busy { "Queue" } else { "Send" }, fs).on_click(cx.listener(|this, _, window, cx| this.send(window, cx)));
            w::layer(if narrow { div().flex_1().flex().child(b.w_full()).into_any_element() } else { b.into_any_element() }).into_any_element()
        };
        let mic = voice::mic(self, cx).map(|m| w::layer(m).into_any_element());
        let attach_btn = w::layer(attach::button(self, cx));
        let row = if narrow {
            div().flex().flex_col().gap(T::SP_8)
                .child(div().flex().child(the_box))
                .child(div().flex().items_center().gap(T::SP_8).child(attach_btn).child(w::layer(sched::clock_button(self, true, cx))).children(stop).children(mic).child(send))
        } else {
            div().flex().items_end().gap(T::SP_8).child(the_box).child(attach_btn).children(stop).children(mic).child(send)
        };

        // The meta row: one row; a narrow pane drops the session id and the
        // mode's name, and the clock moves to the buttons' row.
        let (label, label_c) = state_label(&st);
        let meta_text = if narrow { T::TEXT_10 } else { T::TEXT_11 } * fs;
        let m = &self.meta;
        let input = self.input.clone();
        let switch = enter::switch(self.ctrl, move |window, cx| input.update(cx, |st, cx| st.focus(window, cx)));
        let sid = (!narrow && !m.session_id.is_empty()).then(|| {
            w::tip(div().id("sid").font_family(T::MONO).cursor_pointer().border_b_1().border_dashed().border_color(T::border())
                .hover(|d| d.text_color(T::text_secondary())).child(m.session_id.chars().take(8).collect::<String>()),
                format!("Claude session {}  — click to copy (use it to resume this conversation)", m.session_id))
                .on_click(cx.listener(|this, _, window, cx| this.copy_sid(window, cx)))
        });
        let meta = div().flex().flex_wrap().items_center().min_w_0().gap(if narrow { T::SP_6 } else { T::SP_8 }).text_size(meta_text).text_color(T::text_muted())
            .child(div().text_color(label_c).child(label))
            .child(mode::button(self, narrow, fs, cx))
            .child(switch)
            .when(!narrow, |d| d.child(sched::clock_button(self, false, cx)))
            .child(div().flex_1())
            .children(sid)
            .when(!m.model.is_empty(), |d| d.child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(m.model.clone())))
            .when(m.cost > 0.0, |d| d.child(format!("${:.4}", m.cost)));

        // Its size, after layout (a notify while drawing would not draw again).
        let me = cx.entity().downgrade();
        let (h0, w0) = (self.height, self.width);
        let measure = canvas(move |b, window, cx| {
            let s = b.size;
            if (h0 - s.height).abs() > px(0.5) || (w0 - s.width).abs() > px(0.5) {
                let me = me.clone();
                window.defer(cx, move |window, cx| { let _ = me.update(cx, |c, cx| c.measured(s, window, cx)); });
            }
        }, |_, _, _, _| {}).absolute().size_full();

        div().relative().flex().flex_col().w_full().gap(T::SP_6).p(T::SP_8).border_t_1().border_color(T::border()).bg(T::bg_secondary())
            .child(div().relative().flex().flex_col().gap(T::SP_6)
                .children(voice::vline(self, cx))
                .children(queue::render(self, cx))
                .child(row)
                .children(attach::strip(self))
                .children(slash::render(self, cx)))
            .child(w::layer(meta))
            .child(measure)
    }
}

// ─── The chat area's hooks ───

/// The driver's composer actions, for the focused chat's composer.
pub fn test_ui_for(c: &Entity<Composer>, name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    c.update(cx, |c, cx| c.test_ui(name, args, window, cx))
}

/// Files dropped from the system on a chat pane (the pane's body).
pub fn os_drop(c: &Entity<Composer>, paths: Vec<std::path::PathBuf>, cx: &mut App) {
    attach::drop_files(c, paths, false, cx);
}

/// A reference dragged from another panel ("[File Path: …]") dropped on a
/// chat pane: in at the box's caret, spaced; the pane takes the focus and a
/// toast says "Added to chat" (the web's insertRefToTarget → insertText).
pub fn ref_drop(c: &Entity<Composer>, text: &str, cx: &mut App) {
    if text.is_empty() { return; }
    attach::insert_text(c, text, cx);
    let key = format!("live:{}", c.read(cx).id);
    if let Some(wb) = crundi_workbench::workbench(cx) { wb.update(cx, |w, cx| w.focus_key(&key, cx)); }
    toast(cx, "Added to chat", false);
}
