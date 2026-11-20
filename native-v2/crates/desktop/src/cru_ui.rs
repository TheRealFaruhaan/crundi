//! Cru's screen (app/vendor/crundi-voice.js open, render, renderChat; the
//! web's screen part by part, on the theme's tokens): a dropdown 400 by 600
//! under the top bar's Cru mark, at the right.
//!
//! - The top row: Cru's wordmark, the admin sign (when on), the ⋯ menu
//!   (Chat view / Voice view, Mute Cru's voice, Compact the conversation),
//!   close; under it the usage and spend line in mono with coloured dots.
//! - Compact asks first, in a line under the top row.
//! - The chat view (the default): the conversation as bubbles, "Earlier
//!   messages", Cru thinking, the words being heard; "No messages yet.".
//! - The voice view: the field (cru_field.rs: the head in the contours, the
//!   usage arcs, the projects as cells), then the words: what is heard,
//!   the reply, the detail, Yes / No, and the server's message in amber.
//! - The hint, "Hold Ctrl+Space to talk · click the mic to keep this open"
//!   (the device's shortcut), then the files attached.
//! - The bottom row: the keyboard and attach at the left, the liquid mic in
//!   the middle (its colours the state), the state's word at the right; in
//!   keyboard mode attach, the keyboard, the typing box and send instead.
//!
//! The live screen (cru_screen.rs): in the voice view its panel takes the
//! field's place; in the chat view it sits above the messages; `full`, it is
//! Cru's whole view over the project area (on a window at least
//! CRU_FULL_MIN_W wide), with Normal view and close. Kid mode (cru_kid.rs):
//! the kid screen over the whole window, the sky-to-sun background, big
//! type, the big mic and mute, replies in speech bubbles, the grown-ups'
//! lock and its PIN pad, the break scene.
//!
//! Drawn over the window (the shell's overlay layer: a cached view made at
//! the area's init). It renders from its own copy (`Copy`, rule 1) of the
//! logic's state (cru.rs, cru_kid.rs, cru_screen.rs) and of the top bar's
//! usage, taken by `pump` (and the usage observer) and compared: it
//! notifies only when the copy changed. Closed, the copy is the empty one:
//! nothing else moves it. The field and the mic move a frame at a time,
//! at most T::ANIM_FPS, only while the screen is open and shows them; the
//! microphone's level is read at each such frame, never notified alone.

use crate::{cru, cru_blocks, cru_field, cru_frame::CruFrame, cru_kid, cru_screen};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::Value;
use std::time::{Duration, Instant};

/// The usage the legend and the arcs show: the week's and the five hours' share (0..1).
type UsageShare = Option<(Option<f64>, Option<f64>)>;

/// One message of the chat view: mine or Cru's, its text, its detail, its time as shown.
#[derive(Clone, PartialEq)]
struct Turn { mine: bool, text: String, show: String, when: String }

/// The kid screen's own state.
#[derive(Clone, PartialEq, Default)]
struct Kid { paused: bool, muted: bool, pin: bool, pin_len: usize, pin_err: String }

/// What the screen draws (rule 1): a copy of the logic's state, taken in
/// `pump` when it changed. Closed, it is the default (nothing drawn).
#[derive(Clone, PartialEq, Default)]
struct Copy {
    open: bool,
    kid: Option<Kid>,
    phase: String, reply: String, detail: String, notice: String, question: bool,
    keyboard: bool, voice_off: bool, chat_view: bool, more: bool, unavailable: String,
    menu: bool, confirm: bool, interim: String, heard: String, unread: bool, saying: String,
    att: Vec<cru::Att>, admin: bool,
    turns: Vec<Turn>, pending_len: usize,
    queued: String, hearing: String, can_cancel: bool, cancellable: bool,
    /// The words under the field (cru::words).
    words: String,
    legend: Vec<(&'static str, String)>, arcs: Vec<(&'static str, f32)>, cells: Vec<cru::Cell>,
    /// The live screen: what is up, a page in the frame, Cru's full view.
    screen: Option<Value>, target: bool, full: bool,
    /// The push-to-talk shortcut as the hint names it.
    ptt: String,
}

impl Copy {
    fn take(usage: UsageShare) -> Copy {
        if !cru::with(|c| c.open) { return Copy::default(); }
        let kid = cru_kid::on().then(|| cru_kid::with(|k| Kid { paused: k.paused, muted: k.muted, pin: k.pin_at.is_some(), pin_len: k.pin.len(), pin_err: k.pin_err.clone() }));
        let turns = cru::turns().into_iter().map(|(mine, text, show, at)| Turn { mine, text, show, when: cru::when(at) }).collect();
        let (screen, target) = cru_screen::with(|s| (s.screen.clone(), !s.target.is_empty()));
        let mut c = cru::with(|c| Copy {
            open: true, kid,
            phase: c.phase.clone(), reply: c.reply.clone(), detail: c.detail.clone(), notice: c.notice.clone(), question: !c.question.is_empty(),
            keyboard: c.keyboard, voice_off: c.voice_off, chat_view: c.chat_view, more: c.more, unavailable: c.unavailable.clone(),
            menu: c.menu, confirm: c.confirm, interim: c.interim.clone(), heard: c.heard.clone(), unread: c.unread > 0, saying: c.saying.clone(),
            att: c.att.clone(), admin: c.settings.get("adminMode").and_then(|a| a.as_bool()) == Some(true),
            turns, pending_len: c.pending.len(),
            queued: cru::queued(c), hearing: cru::hearing(c), can_cancel: cru::can_cancel(c), cancellable: !c.no_cancel,
            screen, target, full: false,
            ..Default::default()
        });
        c.full = cru_screen::full();
        c.words = cru::words();
        c.legend = cru::legend(usage);
        c.arcs = cru::arcs(usage);
        c.cells = cru::cells();
        c.ptt = crundi_shell::ptt::label(&crundi_shell::ptt::get());
        c
    }
}

/// The usage's shares as the top bar's store holds it.
fn usage_share(u: &crundi_shell::Usage) -> UsageShare {
    u.usage.as_ref().map(|(u, _)| (u.pointer("/week/utilization").and_then(|x| x.as_f64()).map(|x| x / 100.), u.pointer("/fiveHour/utilization").and_then(|x| x.as_f64()).map(|x| x / 100.)))
}

/// The mic's colours (MIC_COLORS) by the state's tone: three layers, each a
/// colour to its dark end; the glow is the first.
fn mic_look(tone: &str, phase: &str, t: f32) -> w::MicLook {
    let (a, b, c) = match tone {
        "listen" => (T::sky(), T::accent(), T::cru_violet()),
        "think" => (T::cru_violet(), T::accent_deep(), T::accent()),
        "speak" => (T::green(), T::sky(), T::green()),
        "start" => (T::accent(), T::accent_deep(), T::sky()),
        "offline" => (T::yellow(), T::cru_orange(), T::yellow()),
        "problem" => (T::red(), T::cru_red_soft(), T::red()),
        _ => (T::text_muted(), T::bg_hover(), T::text_muted()),
    };
    // Waking or the stream lost: a slow fade in and out; asleep: dim.
    let dim = if tone == "offline" || phase == "waking" { 0.72 + 0.28 * (t * 2.8).sin() } else if phase == "ended" { 0.45 } else { 1. };
    w::MicLook { layers: [(a, T::cru_deep_blue()), (b, T::cru_deep_indigo()), (c, T::cru_deep_violet())], glow: a, dim }
}

/// The kid screen's mic colour by the phase.
fn tint(p: &str) -> Hsla {
    match p { "listen" => T::sky(), "think" => T::accent_hover(), "speak" => T::green(), "waking" => T::accent(), "offline" => T::amber(), "problem" => T::red(), _ => T::text_muted() }
}

/// A legend item's or an arc's colour: wk, 5h, Claude, Deepgram.
fn usage_color(k: &str) -> Hsla {
    match k { "wk" => T::accent_hover(), "5h" => T::sky(), "claude" => T::cru_violet(), _ => T::green() }
}

/// The send button's ring in keyboard mode: the state, as a shadow.
fn ring(tone: &str) -> Option<Hsla> {
    match tone { "think" => Some(T::cru_violet()), "speak" => Some(T::green()), "start" => Some(T::accent()), "offline" => Some(T::yellow()), "problem" => Some(T::red()), "ended" => Some(T::text_muted().opacity(0.6)), _ => None }
}

pub struct CruScreen {
    /// Keyboard mode's typing box (made with the window: `attach`).
    input: Option<Entity<w::TextareaState>>,
    frame: Entity<CruFrame>,
    focus: FocusHandle,
    shown: bool,
    scroll: ScrollHandle,
    turns: usize,
    field: cru_field::FieldState,
    liquid: w::Liquid,
    start: Instant,
    /// What had the keys before Cru opened.
    prev: Option<FocusHandle>,
    /// The copy it draws, the usage it was taken with, the microphone's level (read each animated frame).
    copy: Copy,
    usage: UsageShare,
    level: f32,
    /// The top bar's mark as last painted: its right edge and bottom, and the window's width then.
    anchor: Option<(Pixels, Pixels, Pixels)>,
    /// An animated frame is asked for (at most T::ANIM_FPS).
    frame_due: bool,
    _subs: Vec<Subscription>,
}

impl CruScreen {
    /// Made at the area's init (no window yet): the overlay is registered before the Shell is made.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let frame = cx.new(CruFrame::new);
        let stores = crundi_shell::stores(cx);
        let usage = usage_share(stores.usage.read(cx));
        // The top bar's usage: the legend and the arcs (only while open).
        let sub = cx.observe(&stores.usage, |this: &mut Self, u, cx| {
            let share = usage_share(u.read(cx));
            if share == this.usage { return; }
            this.usage = share;
            if this.copy.open { this.retake(cx); }
        });
        CruScreen { input: None, frame, focus: cx.focus_handle(), shown: false, scroll: ScrollHandle::new(), turns: 0, field: Default::default(), liquid: Default::default(), start: Instant::now(), prev: None,
            copy: Copy::default(), usage, level: 0., anchor: None, frame_due: false, _subs: vec![sub] }
    }

    /// The window opened: the typing box (Enter sends; Shift+Enter is a new line).
    pub fn attach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_some() { return; }
        let input = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(1, 6).placeholder("Type to Cru…"));
        self._subs.push(cx.subscribe_in(&input, window, |_, st, ev: &w::InputEvent, window, cx| if let w::InputEvent::PressEnter { shift: false, .. } = ev {
            let t = st.read(cx).value().to_string();
            st.update(cx, |s, cx| s.set_value("", window, cx));
            cru::utter(&t, true);
        }));
        // The box out of its paint layer while it has the caret.
        let fh = input.read(cx).focus_handle(cx);
        self._subs.extend(w::watch_focus(&fh, window, cx));
        self.input = Some(input);
    }

    fn retake(&mut self, cx: &mut Context<Self>) {
        let c = Copy::take(self.usage);
        if c != self.copy { self.copy = c; cx.notify(); }
    }

    /// After each pump: the copy taken again when the logic said something changed; opened, it
    /// takes the keys (the box in keyboard mode); the frame draws again when its page moved.
    pub fn pump(&mut self, dirty: bool, window: &mut Window, cx: &mut Context<Self>) {
        if dirty { self.retake(cx); }
        let (open, kb) = (self.copy.open, self.copy.keyboard);
        if open {
            self.level = cru::with(|c| c.level);
            // Under the top bar's mark (where it was last painted).
            let a = crundi_shell::cru_anchor(cx).map(|b| (b.right(), b.bottom(), window.viewport_size().width));
            if a.is_some() && a != self.anchor { self.anchor = a; cx.notify(); }
        }
        if open != self.shown {
            self.shown = open;
            if open {
                self.prev = window.focused(cx).filter(|f| f != &self.focus);
                self.turns = usize::MAX;
                if kb && self.copy.kid.is_none() { self.focus_box(window, cx); } else { window.focus(&self.focus, cx); }
            } else if let Some(f) = self.prev.take() {
                // Back to what had the keys (a chat's composer, as the web). Gone meanwhile, the
                // shell's focus-lost rule gives them to the window's root.
                window.focus(&f, cx);
            }
            cx.notify();
        }
        if open { self.frame.update(cx, |f, cx| f.pump(cx)); }
    }

    /// The next animated frame, at most T::ANIM_FPS (rule 6): a timer that notifies this view
    /// alone, asked only by a render that animates (so closed, or still, none is asked).
    fn next_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.frame_due { return; }
        self.frame_due = true;
        let every = Duration::from_secs_f32(1. / T::ANIM_FPS);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(every).await;
            let _ = this.update(cx, |this, cx| {
                this.frame_due = false;
                if this.copy.open { this.level = cru::with(|c| c.level); cx.notify(); }
            });
        }).detach();
    }

    /// Keyboard mode's box takes the keys (the web focuses it as it opens).
    fn focus_box(&self, window: &mut Window, cx: &mut Context<Self>) { if let Some(i) = &self.input { i.update(cx, |s, cx| s.focus(window, cx)); } }

    /// The live screen's panel: its head (title; Full view, or Normal view; ×) and its body (blocks, then the frame).
    fn screen_panel(&self, full: bool, kid: bool) -> Option<Div> {
        let scr = self.copy.screen.as_ref()?;
        let title = scr.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
        let video = scr.get("video").is_some();
        let target = self.copy.target;
        let blocks = scr.get("blocks").and_then(|b| b.as_array()).cloned().unwrap_or_default();
        let head = div().flex().flex_none().items_center().gap(T::SP_6)
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(title))
            .when(full, |d| d.child(w::button("cru-normal", "Normal view").icon(IconName::Minimize2).on_click(|_, _, _| cru_screen::normal_view())))
            .when(!full && !video, |d| d.child(w::icon_button("cru-full", IconName::Maximize2, "Full view").on_click(|_, _, _| cru_screen::full_view())))
            .child(w::icon_button("cru-scr-x", IconName::X, "Close what Cru shows").on_click(|_, _, _| cru_screen::close_screen()));
        let grow = full || kid;
        let mut body = div().flex().flex_col().gap(T::SP_8).when(grow, |d| d.flex_1().min_h_0());
        if !blocks.is_empty() { body = body.child(div().id("cru-blocks").max_h(T::CRU_SCREEN_H).overflow_y_scroll().child(cru_blocks::blocks(&blocks, kid))); }
        if target { body = body.child(div().when(grow, |d| d.flex_1().min_h_0()).when(!grow, |d| d.h(T::CRU_SCREEN_H)).child(self.frame.clone())); }
        Some(div().flex().flex_col().gap(T::SP_6).when(grow, |d| d.flex_1().min_h_0()).when(!kid, |d| d.child(head)).child(body))
    }

    fn kid_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.copy;
        let (phase, reply, heard) = (c.phase.clone(), c.reply.clone(), if c.interim.is_empty() { c.heard.clone() } else { c.interim.clone() });
        let Kid { paused, muted, pin, pin_len, pin_err } = c.kid.clone().unwrap_or_default();
        let screen = self.screen_panel(false, true);
        let has_screen = screen.is_some();
        let top = div().flex().items_center().px(T::SP_24).pt(T::SP_16)
            .child(div().text_color(T::kid_button()).text_size(T::KID_HEADING).child(IconName::AudioLines))
            .child(div().flex_1())
            .child(w::icon_button("kid-lock", IconName::Lock, "Grown-ups").on_click(|_, _, _| cru_kid::pin_open()));
        let mut words = div().flex().flex_col().items_center().gap(T::SP_8).px(T::SP_24).w_full();
        if !heard.is_empty() {
            words = words.child(div().flex().w_full().max_w(T::KID_BUBBLE_W).justify_end()
                .child(div().px(T::SP_20).py(T::SP_10).rounded(T::R_PILL).bg(T::kid_heard_bg()).text_color(T::kid_heard()).text_size(T::KID_HEARD).font_weight(T::W_BOLD).child(heard)));
        }
        if !reply.is_empty() || phase == "think" {
            words = words.child(div().flex().flex_col().max_w(T::KID_BUBBLE_W).px(T::SP_24).py(T::SP_16).rounded(T::KID_REPLY_R).bg(T::white())
                .text_color(T::kid_text()).text_size(T::KID_REPLY).font_weight(T::W_BOLD).shadow_md()
                .when(phase == "think" && !reply.is_empty(), |d| d.child(div().text_size(T::TEXT_16).text_color(T::kid_wait()).child("Wait for me!")))
                .child(if phase == "think" && reply.is_empty() { "…".to_string() } else { reply }));
        }
        let mic = div().id("kid-mic").size(T::KID_MIC).rounded_full().bg(tint(&phase)).flex().items_center().justify_center()
            .text_color(T::white()).text_size(T::KID_MIC_ICON).cursor_pointer().shadow_lg().when(muted, |d| d.opacity(T::INBOX_TINT))
            .child(if phase == "speak" { IconName::Square } else { IconName::Mic }).on_click(|_, _, _| cru::mic());
        let mute = div().id("kid-mute").size(T::KID_BTN).rounded_full().bg(if muted { T::red_dim() } else { T::white() }).flex().items_center().justify_center()
            .text_color(if muted { T::red() } else { T::kid_button() }).text_size(T::ICON_XL).cursor_pointer().shadow_md()
            .child(if muted { IconName::MicOff } else { IconName::Volume2 }).on_click(|_, _, _| cru_kid::mute());
        let bottom = div().relative().flex().items_center().justify_center().py(T::SP_20).child(mic)
            .child(div().absolute().right(T::SP_24).child(mute));
        let mut root = div().id("kid-screen").absolute().inset_0().flex().flex_col()
            .bg(linear_gradient(175., linear_color_stop(T::kid_sky(), 0.), linear_color_stop(T::kid_sun(), 1.)))
            .text_color(T::kid_text())
            .track_focus(&self.focus)
            .on_key_down(|e: &KeyDownEvent, _, _| if cru_kid::pin_shown() {
                match e.keystroke.key.as_str() { "enter" => cru_kid::pin_key("OK"), "backspace" => cru_kid::pin_key("Back"), "escape" => {}, k => cru_kid::pin_key(k) }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(top)
            .when_some(screen, |d, s| d.child(div().flex().flex_col().flex_1().min_h_0().px(T::SP_24).child(s)))
            .when(!has_screen, |d| d.child(div().flex_1()))
            .child(words)
            .child(bottom);
        if paused {
            root = root.child(div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap(T::SP_12).bg(T::kid_break())
                .text_color(T::kid_break_text()).text_size(T::KID_TEXT).font_weight(T::W_BOLD)
                .child(div().text_size(T::KID_MIC_ICON).child(IconName::Sun)).child("Break time! See you soon."));
        }
        if pin {
            let mut keys = div().flex().flex_wrap().justify_center().gap(T::SP_8);
            for key in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "Back", "0", "OK"] {
                keys = keys.child(div().id(("pin-key", key.as_bytes()[0] as usize + key.len())).size(T::KID_KEY).rounded_full().bg(if key == "OK" { T::kid_button() } else { T::bg_tertiary() })
                    .flex().items_center().justify_center().text_color(T::white()).text_size(T::TEXT_20).font_weight(T::W_BOLD).cursor_pointer()
                    .child(if key == "Back" { "←".to_string() } else { key.to_string() })
                    .on_click(move |_, _, _| cru_kid::pin_key(key)));
            }
            root = root.child(div().id("kid-pin").absolute().inset_0().flex().items_center().justify_center().bg(T::scrim())
                .on_click(|_, _, _| cru_kid::pin_close())
                .child(div().id("kid-pin-box").w(T::KID_PAD_W).p(T::SP_20).rounded(T::R_CARD).bg(T::bg_secondary()).text_color(T::text()).flex().flex_col().gap(T::SP_12).items_center()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(div().text_size(T::TEXT_14).text_center().child("Grown-ups: enter the PIN to leave kid mode"))
                    .child(div().h(T::CONTROL).text_size(T::TEXT_20).font_family(T::MONO).child("•".repeat(pin_len)))
                    .child(keys)
                    .when(!pin_err.is_empty(), |d| d.child(div().text_size(T::TEXT_13).text_color(T::red()).child(pin_err)))));
        }
        let _ = cx;
        root.into_any_element()
    }
}

impl CruScreen {
    /// The top row and the usage line under it (with the ⋯ menu over the screen).
    fn top(&self, live: bool, unread: bool, admin: bool, legend: Vec<(&'static str, String)>) -> Div {
        let row = div().flex().w_full().items_center().gap(T::SP_8)
            .child(div().id("cru-wordmark").child(w::cru_mark(live)))
            .child(div().flex_1())
            .when(admin, |d| d.child(w::tone_button("cru-admin", IconName::ShieldAlert, None, T::red(), T::error_edge(), true,
                "Admin mode is on: Cru can run any command on the server. Turn it off in Settings, or say \"admin mode off\".", 1.)))
            .child(w::round_button("cru-more", IconName::Ellipsis, "More", false, None, T::CRU_BTN, T::CRU_BTN_ICON)
                .when(unread, |d| d.child(w::dot(T::red()).absolute().top(T::SP_2).right(T::SP_2)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, _| cru::menu_toggle()))
            .child(w::round_button("cru-x", IconName::X, "Close (Esc)", false, None, T::CRU_BTN, T::CRU_BTN_ICON).on_click(|_, _, _| cru::close()));
        let mut leg = div().flex().flex_wrap().w_full().items_center().gap_x(T::SP_12).gap_y(T::SP_2).px(T::SP_2)
            .font_family(T::MONO).text_size(T::TEXT_10).line_height(T::TEXT_10 * T::LH_CHAT);
        for (k, s) in legend {
            let c = usage_color(k);
            leg = leg.child(div().flex().items_center().gap(T::SP_4).whitespace_nowrap().text_color(c).child(w::dot(c)).child(s));
        }
        div().flex().flex_col().w_full().gap(T::SP_6).px(T::SP_12).pt(T::SP_12).child(row).child(leg)
    }

    /// The ⋯ menu (.cv-menu): the view, Cru's voice, compact.
    fn menu(chat_view: bool, voice_off: bool, unread: bool) -> Stateful<Div> {
        w::menu_panel("cru-menu").absolute().top(T::SP_12 + T::CRU_BTN + T::SP_6).right(T::SP_12 + T::CRU_BTN + T::SP_8).min_w(T::CRU_MENU_W)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(w::menu_item("cru-mi-chat", IconName::MessageCircle, if chat_view { "Voice view" } else { "Chat view" }, false, false)
                .when(unread && !chat_view, |d| d.text_color(T::red()))
                .tooltip(w::tooltip(if chat_view { "Back to the voice view" } else { "Chat view: the whole conversation as messages" }))
                .on_click(|_, _, _| cru::menu_pick("chat-view")))
            .child(w::menu_item("cru-mi-quiet", if voice_off { IconName::VolumeX } else { IconName::Volume2 }, "Mute Cru's voice", false, voice_off)
                .when(voice_off, |d| d.text_color(T::yellow()))
                .tooltip(w::tooltip("Replies as text only, until turned back on"))
                .on_click(|_, _, _| cru::menu_pick("voice-off")))
            .child(w::menu_item("cru-mi-compact", IconName::Archive, "Compact the conversation", false, false)
                .tooltip(w::tooltip("Compact the conversation (Cru keeps a summary and its notebook)"))
                .on_click(|_, _, _| cru::menu_pick("compact")))
    }

    /// The chat view's conversation (renderChat).
    fn chat(&mut self, phase: &str, interim: &str, more: bool, saying: &str) -> Stateful<Div> {
        let turns = self.copy.turns.clone();
        let count = turns.len() + usize::from(phase == "think") + usize::from(!interim.is_empty()) + self.copy.pending_len;
        if count != self.turns { self.turns = count; self.scroll.scroll_to_bottom(); }
        let mut list = div().flex().flex_col().justify_end().gap(T::SP_8).min_h_full().w_full();
        if more {
            list = list.child(div().flex().justify_center().child(div().id("cru-older").px(T::SP_10).py(T::SP_4).rounded(T::R_PILL).bg(T::bg_tertiary())
                .text_size(T::TEXT_12).text_color(T::text_secondary()).cursor_pointer().child("Earlier messages").on_click(|_, _, _| cru::older())));
        }
        let me = |id: (&'static str, usize), text: String, time: Option<String>, live: bool| {
            div().flex().justify_end().child(div().id(id).max_w(relative(T::CRU_BUBBLE_SHARE)).px(T::SP_12).py(T::SP_8)
                .rounded(T::CRU_BUBBLE_R).rounded_br(T::CRU_BUBBLE_TAIL).bg(T::cru_me()).text_size(T::TEXT_14).line_height(T::TEXT_14 * T::LH_COMPOSER)
                .when(live, |d| d.opacity(0.6).italic())
                .child(text)
                .when_some(time, |d, t| d.child(div().mt(T::SP_2).text_size(T::TEXT_10).text_color(T::text_muted()).child(t))))
        };
        let mut any = false;
        for (n, Turn { mine, text, show, when }) in turns.into_iter().enumerate() {
            any = true;
            if mine { list = list.child(me(("cru-me", n), text, Some(when), false).w_full()); continue; }
            let on = phase == "speak" && !text.is_empty() && text.trim() == saying.trim();
            let t2 = text.clone();
            let bubble = div().id(("cru-reply", n)).max_w(relative(T::CRU_BUBBLE_SHARE)).px(T::SP_12).py(T::SP_8)
                .rounded(T::CRU_BUBBLE_R).rounded_bl(T::CRU_BUBBLE_TAIL).bg(T::bg_secondary()).border_1().border_color(T::border())
                .text_size(T::TEXT_14).line_height(T::TEXT_14 * T::LH_COMPOSER)
                .when(!text.is_empty(), |d| d.child(text.clone()))
                .when(!show.is_empty(), |d| d.child(div().mt(T::SP_6).pt(T::SP_6).border_t_1().border_color(T::border()).text_size(T::TEXT_13).text_color(T::text_secondary()).child(show)))
                .child(div().mt(T::SP_2).text_size(T::TEXT_10).text_color(T::text_muted()).child(when));
            list = list.child(div().flex().w_full().items_end().gap(T::SP_6).child(bubble)
                .when(!t2.is_empty(), |d| d.child(w::icon_toggle(("cru-say", n), IconName::Speech, if on { "Cru is saying this: tap to stop" } else { "Say it aloud" }, on)
                    .on_click(move |_, _, _| if cru::with(|c| c.phase == "speak" && c.saying.trim() == t2.trim()) { cru::mic(); } else if cru::with(|c| c.phase == "listen" && c.interim.is_empty()) { cru::say(&t2, false); }))));
        }
        if phase == "think" {
            any = true;
            let dot = |i: usize| div().size(T::SP_8).rounded_full().bg(T::text_secondary())
                .with_animation(("cru-dot", i), Animation::new(std::time::Duration::from_millis(1200)).repeat().with_max_fps(T::ANIM_FPS), move |d, p| { let q = (p - i as f32 * 0.17).rem_euclid(1.); let k = if q < 0.3 { q / 0.3 } else if q < 0.6 { 1. - (q - 0.3) / 0.3 } else { 0. }; d.opacity(0.35 + 0.65 * k) });
            list = list.child(div().flex().w_full().child(div().id("cru-typing").flex().gap(T::SP_6).items_center().px(T::SP_16).py(T::SP_12)
                .rounded(T::CRU_BUBBLE_R).rounded_bl(T::CRU_BUBBLE_TAIL).bg(T::bg_secondary()).border_1().border_color(T::border())
                .child(dot(0)).child(dot(1)).child(dot(2))));
        }
        // What is queued for next, then the words being heard: faded, each with its × (dropLive).
        let (queued, hearing, cancellable) = (self.copy.queued.clone(), self.copy.hearing.clone(), self.copy.cancellable);
        let drop_x = |which: &'static str| w::round_button(("cru-drop", which.len()), IconName::X, "Cancel this: it is not sent", false, None, T::CRU_DROP_BTN, T::ICON_SM)
            .on_click(move |_, _, _| cru::drop_live(which));
        if !queued.is_empty() {
            any = true;
            list = list.child(div().id("cru-queued").flex().w_full().justify_end().items_center().gap(T::SP_6).child(drop_x("queued"))
                .child(me(("cru-queued-m", 0), queued, Some("Queued: Cru answers it next".into()), true).flex_1().min_w_0()));
        }
        if !hearing.is_empty() {
            any = true;
            list = list.child(div().id("cru-hearing").flex().w_full().justify_end().items_center().gap(T::SP_6).when(cancellable, |d| d.child(drop_x("hearing")))
                .child(me(("cru-hearing-m", 0), hearing, None, true).flex_1().min_w_0()));
        }
        if !any { list = list.child(div().id("cru-empty").w_full().text_center().text_size(T::TEXT_13).text_color(T::text_muted()).child("No messages yet.")); }
        div().id("cru-chat").flex_1().min_h_0().w_full().overflow_y_scroll().track_scroll(&self.scroll).px(T::SP_12).py(T::SP_10).child(list)
    }

    /// The voice view's words (.cv-words): heard, the reply, the detail, Yes / No, the note.
    fn words(c: &Copy, note: String) -> Stateful<Div> {
        let (phase, reply, detail, question) = (c.phase.as_str(), c.reply.clone(), c.detail.clone(), c.question);
        let heard = c.words.clone();
        let (can_cancel, qd) = (c.can_cancel, queued_line(c));
        div().id("cru-words").flex().flex_col().items_center().flex_1().min_h_0().w_full().overflow_y_scroll().px(T::SP_24).py(T::SP_10).text_center()
            .child(div().flex().items_center().justify_center().gap(T::SP_6).min_h(T::TEXT_14 * T::LH_CHAT).text_size(T::TEXT_14).text_color(T::text_secondary()).child(heard)
                .when(can_cancel, |d| d.child(w::round_button("cru-cx", IconName::X, "Cancel what I said", false, None, T::CRU_CX, T::ICON_SM).on_click(|_, _, _| cru::cancel_words()))))
            .when(!qd.is_empty(), |d| d.child(div().id("cru-q").mt(T::SP_6).text_size(T::TEXT_13).text_color(T::accent_hover()).child(format!("Queued, answered next: “{qd}”"))))
            .when(!reply.is_empty(), |d| d.child(div().mt(T::SP_12).text_size(T::TEXT_16).font_weight(T::W_MEDIUM).line_height(T::TEXT_16 * T::LH_UI).child(reply)))
            .when(!detail.is_empty(), |d| d.child(div().w_full().mt(T::SP_12).px(T::SP_12).py(T::SP_12).rounded(T::R_CARD).bg(T::bg_secondary()).border_1().border_color(T::border())
                .text_left().text_size(T::TEXT_14).child(detail)))
            .when(question && phase != "think", |d| d.child(div().mt(T::SP_8).flex().items_center().justify_center().gap(T::SP_8).text_size(T::TEXT_13).text_color(T::accent_hover())
                .child("Say it, or tap:")
                .child(w::button("cru-yes", "Yes").on_click(|_, _, _| cru::utter("Yes, go ahead.", true)))
                .child(w::button("cru-no", "No").on_click(|_, _, _| cru::utter("No, don't.", true)))))
            .when(!note.is_empty(), |d| d.child(div().id("cru-note").mt(T::SP_10).text_size(T::TEXT_13).text_color(T::yellow()).child(note)))
    }
}

/// The queued words and the words being heard, as the voice view's "Queued, answered next" line.
fn queued_line(c: &Copy) -> String { [c.queued.clone(), c.hearing.clone()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ") }

impl Render for CruScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("cru");
        let c = self.copy.clone();
        if !c.open { return div().into_any_element(); }
        if c.kid.is_some() { return self.kid_view(cx); }
        // Cru's full view: the project area (on a wide enough window).
        if c.full && window.viewport_size().width >= T::CRU_FULL_MIN_W {
            if let Some(p) = self.screen_panel(true, false) {
                return div().absolute().top(T::TOPBAR_H).left_0().right_0().bottom_0().flex().flex_col().p(T::SP_16).bg(T::bg())
                    .track_focus(&self.focus)
                    .on_key_down(|e: &KeyDownEvent, _, _| if e.keystroke.key == "escape" { cru::close(); })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(p).into_any_element();
            }
        }
        let (phase, keyboard, chat_view) = (c.phase.clone(), c.keyboard, c.chat_view);
        let un = !c.unavailable.is_empty();
        let tone = cru::tone(&phase, un);
        let word = cru::status_word(&phase, un);
        // A blocked screen's reason is its notice (the web's block()).
        let note = if un { c.unavailable.clone() } else { c.notice.clone() };
        // A frame of the field and the mic.
        let t = self.start.elapsed().as_secs_f32();
        let ms = t as f64 * 1000.;
        let mic_level = if phase == "listen" { self.level } else { 0. };
        let fphase = if un { "ended".to_string() } else { phase.clone() };
        let panel = self.screen_panel(false, false);
        let has_scr = panel.is_some();
        let mut animate = !keyboard;
        let field_shown = !chat_view && !has_scr;
        if field_shown { animate = true; self.field.step(ms, t, &fphase, mic_level); }
        if !keyboard { self.liquid.step(ms, mic_level, phase == "listen"); }

        // Under the top bar's mark, at its right (the web's anchor), else at the window's right.
        let vp = window.viewport_size();
        // The mark sits at a fixed distance from the window's right edge: moved with a resize.
        let anchor = self.anchor.map(|(r, b, w0)| (r + (vp.width - w0), b));
        let top = anchor.map(|(_, b)| b + T::SP_8).unwrap_or(T::TOPBAR_H + T::SP_4);
        let right = anchor.map(|(r, _)| (vp.width - r - T::SP_8).max(T::SP_8)).unwrap_or(T::SP_12);
        let (pw, ph) = if has_scr { (T::CRU_W_SCR.min(vp.width - T::SP_40), T::CRU_H_SCR) } else { (T::CRU_W, T::CRU_H) };
        let ph = ph.min(vp.height - T::CRU_ROOM);

        // Each part in a paint layer (the screen is replayed while other views draw); the typing
        // box with its caret, and the ⋯ menu over the rest, are drawn outside / over them.
        let mut col = div().flex().flex_col().items_center().size_full()
            .child(w::layer(self.top(phase != "ended", c.unread && !chat_view, c.admin, c.legend.clone())));
        if c.confirm {
            col = col.child(w::layer(div().id("cru-confirm").flex().flex_wrap().items_center().gap(T::SP_8).mx(T::SP_12).mt(T::SP_8).px(T::SP_12).py(T::SP_10)
                .rounded(T::R_BUBBLE).bg(T::bg_secondary()).border_1().border_color(T::border()).text_size(T::TEXT_13)
                .child(div().flex_1().min_w(T::CRU_MENU_W).child("Compact the conversation? Cru carries on from a summary; its notebook and memory search keep everything."))
                .child(w::primary("cru-compact-yes", "Compact").on_click(|_, _, _| cru::menu_pick("compact-yes")))
                .child(w::button("cru-compact-no", "Cancel").on_click(|_, _, _| cru::menu_pick("compact-no")))));
        }
        let mut panel = panel;
        if chat_view {
            // The chat view: the live screen above the messages.
            if let Some(p) = panel.take() { col = col.child(div().flex_none().w_full().px(T::SP_12).pt(T::SP_8).child(p)); }
            let chat = self.chat(&phase, &c.interim, c.more, &c.saying);
            col = col.child(w::layer(chat));
        } else {
            // The voice view: the live screen takes the field's place.
            if let Some(p) = panel.take() {
                col = col.child(div().flex().flex_col().flex_1().flex_grow(T::CRU_FIELD_GROW).min_h_0().w_full().px(T::SP_12).pt(T::SP_8).child(p));
            } else {
                let arcs = c.arcs.iter().map(|(k, v)| (usage_color(k), *v)).collect();
                let frame = cru_field::Frame { t, phase: fphase.clone(), state: self.field.clone(), cells: c.cells.clone(), arcs };
                col = col.child(div().id("cru-field").relative().flex_1().flex_grow(T::CRU_FIELD_GROW).min_h(T::CRU_FIELD_MIN).w_full().child(cru_field::field(frame)));
            }
            col = col.child(w::layer(Self::words(&c, note.clone())));
        }
        let hint = format!("Hold {} to talk · click the mic to keep this open", c.ptt);
        if !keyboard {
            col = col.child(w::layer(div().id("cru-hint").flex_none().pb(T::SP_6).text_size(T::TEXT_11).text_color(T::text_muted()).text_center().child(hint.clone())));
        }
        if !c.att.is_empty() {
            let mut chips = div().flex().flex_wrap().justify_center().gap(T::SP_6).px(T::SP_16).pt(T::SP_4);
            for (i, a) in c.att.iter().enumerate() {
                let label = if a.bad.is_empty() { a.name.clone() } else { format!("{}: {}", a.name, a.bad) };
                chips = chips.child(div().id(("cru-att", i)).cursor_pointer().when(a.up, |d| d.opacity(0.6))
                    .child(w::chip(IconName::Paperclip, label, !a.bad.is_empty()).child(w::icon(IconName::X, T::ICON_SM)))
                    .tooltip(w::tooltip("Remove")).on_click(move |_, _, _| cru::unattach(i)));
            }
            col = col.child(w::layer(chips));
        }
        // The bottom row.
        let word_el = div().absolute().right(T::SP_20).bottom(if keyboard { T::SP_2 } else { T::SP_10 }).text_size(T::TEXT_12).whitespace_nowrap()
            .text_color(match tone { "offline" => T::yellow(), "problem" => T::red(), _ => T::text_muted() }).child(word);
        let kb_btn = w::round_button("cru-kb", IconName::Keyboard, if keyboard { "Back to voice" } else { "Keyboard mode: type to Cru, no microphone" }, keyboard, None, T::CRU_BTN, T::CRU_BTN_ICON)
            .on_click(cx.listener(|this, _, window, cx| { let k = cru::with(|c| c.keyboard); cru::set_keyboard(!k); if !k { this.focus_box(window, cx); } else { window.focus(&this.focus, cx); } }));
        let att_btn = w::round_button("cru-attach", IconName::Paperclip, "Attach a file, photo or video (or drop it here)", false, None, T::CRU_BTN, T::CRU_BTN_ICON)
            .on_click(|_, _, _| cru::attach());
        let bottom = if keyboard {
            let send = w::round_button("cru-send", IconName::Send, "Send", true, ring(tone), T::CRU_BTN, T::CRU_BTN_ICON)
                .on_click(cx.listener(|this, _, window, cx| {
                    let Some(input) = this.input.clone() else { return };
                    let t = input.read(cx).value().to_string();
                    input.update(cx, |s, cx| s.set_value("", window, cx));
                    cru::utter(&t, true);
                    this.focus_box(window, cx);
                }));
            let boxed = self.input.as_ref().map(|i| { let fh = i.read(cx).focus_handle(cx); w::text_box(&fh, w::message_box(i, 1.), window, cx) });
            div().relative().flex().w_full().items_center().gap(T::SP_12).px(T::SP_12).pt(T::SP_4).pb(T::SP_12)
                .child(w::layer(att_btn)).child(w::layer(kb_btn)).child(div().flex_1().min_w_0().children(boxed)).child(w::layer(send)).child(w::layer(word_el))
                .into_any_element()
        } else {
            let mic_box = if chat_view { T::CRU_MIC_CHAT } else { T::CRU_MIC };
            let mic = w::liquid_mic("cru-mic", &self.liquid, mic_look(tone, &fphase, t), t, ms, mic_box, if phase == "speak" { "Stop talking" } else { "Talk" })
                .on_click(|_, _, _| cru::mic());
            div().relative().flex().w_full().items_center().justify_center().px(T::SP_12).pt(T::SP_4).pb(T::SP_12)
                .child(div().absolute().left(T::SP_24).top_0().bottom(T::SP_8).flex().items_center().child(kb_btn))
                .child(div().absolute().left(T::SP_24 + T::CRU_BTN + T::SP_12).top_0().bottom(T::SP_8).flex().items_center().child(att_btn))
                .child(mic).child(word_el)
                .into_any_element()
        };
        col = col.child(bottom);
        // The menu over the screen: a layer of its own, drawn over the others.
        if c.menu { col = col.child(w::layer(Self::menu(chat_view, c.voice_off, c.unread))); }
        if animate { self.next_frame(window, cx); }
        let n_bubbles = if chat_view { c.turns.len() } else { 0 };
        let ui = serde_json::json!({
            "top": f32::from(top), "right": f32::from(right), "w": f32::from(pw), "h": f32::from(ph), "viewportW": f32::from(vp.width),
            "anchor": anchor.map(|(r, b)| serde_json::json!({ "right": f32::from(r), "bottom": f32::from(b) })),
            "view": if chat_view { "chat" } else { "voice" }, "wordmark": true, "more": true, "close": true,
            "menuItems": if c.menu { serde_json::json!([if chat_view { "Voice view" } else { "Chat view" }, "Mute Cru's voice", "Compact the conversation"]) } else { serde_json::json!([]) },
            "bubbles": n_bubbles, "empty": chat_view && n_bubbles == 0 && phase != "think" && c.queued.is_empty() && c.hearing.is_empty(),
            "field": field_shown, "note": if chat_view { String::new() } else { note.clone() }, "noteColor": "amber",
            "hint": if keyboard { String::new() } else { hint },
            "cancelX": !chat_view && c.can_cancel, "queuedLine": if chat_view { String::new() } else { queued_line(&c) },
            "queuedBubble": chat_view && !c.queued.is_empty(), "hearingBubble": chat_view && !c.hearing.is_empty(),
            "typingBox": keyboard, "mic": !keyboard, "micBox": f32::from(if chat_view { T::CRU_MIC_CHAT } else { T::CRU_MIC }), "word": word, "tone": tone, "animating": animate,
        });
        cru::with(|c| c.ui = ui);
        div().id("cru-panel").absolute().top(top).right(right).w(pw).h(ph)
            .track_focus(&self.focus)
            .on_key_down(|e: &KeyDownEvent, _, _| if e.keystroke.key == "escape" { cru::close(); })
            .on_mouse_down(MouseButton::Left, |_, _, cx| { cru::panel_pressed(); if cru::with(|c| c.menu) { cru::with(|c| c.menu = false); cru::render(); } cx.stop_propagation(); })
            .on_hover(|on, _, _| cru::panel_hover(*on))
            .bg(T::bg()).text_color(T::text()).rounded(T::CRU_BUBBLE_R).border_1().border_color(T::border()).shadow_lg().overflow_hidden()
            .child(col)
            .into_any_element()
    }
}
