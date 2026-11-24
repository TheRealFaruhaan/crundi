//! Cru on the PC (app/vendor/crundi-voice.js; spec
//! specs/desktop/passwords-homepool-cru.md C): the voice assistant's screen,
//! a dropdown under the top bar's Cru mark. It listens (the chat voice's
//! microphone and socket, src/voiceio.rs), asks the server's Cru brain
//! (/api/voice/turn), says the answer (/api/voice/speak, streamed), and
//! listens again; asleep after 30 s of silence. Keyboard mode types instead.
//! The chat view keeps the conversation (/api/voice/history).
//!
//! Ported from the Slint client's src/desktop/cru.rs; the screen is
//! cru_ui.rs (the web's screen, part by part), drawn from this state over
//! the window; its field and mic are cru_field.rs and w::liquid_mic.

use crate::ctx as kit;
use crundi_core::{voice, voiceio};
use crundi_webview::later;
use serde_json::{json, Value};
use std::{cell::RefCell, time::{Duration, Instant}};

pub const SILENCE_MS: u64 = 30_000;
pub const REPORT_LINGER_MS: u64 = 8_000;
/// The projects' services are read again this often while Cru is open (the field's cells).
const SERVICES_EVERY: Duration = Duration::from_secs(15);

/// A file for Cru's next turn (the web's S.att): uploading, uploaded (its
/// id and kind), or failed (why).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Att { pub name: String, pub id: String, pub kind: String, pub up: bool, pub bad: String, pub r#ref: String,
    /// A picture's thumbnail on its chip (previewOf: png, jpeg, webp, gif).
    pub thumb: Option<std::sync::Arc<gpui::Image>> }

/// A picture's own bytes as an image for its chip (previewOf's types only).
fn thumb_of(bytes: &[u8]) -> Option<std::sync::Arc<gpui::Image>> {
    use gpui::ImageFormat as F;
    let f = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { F::Png } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) { F::Jpeg }
        else if bytes.starts_with(b"GIF8") { F::Gif } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" { F::Webp } else { return None };
    Some(std::sync::Arc::new(gpui::Image::from_bytes(f, bytes.to_vec())))
}

#[derive(Default)]
pub struct Cru {
    pub open: bool,
    pub report_only: bool,
    /// waking, listen, think, speak, ended (asleep), offline, problem
    pub phase: String,
    pub heard: String,
    pub interim: String,
    pub reply: String,
    pub detail: String,
    pub notice: String,
    pub question: String,
    pub keyboard: bool,
    pub voice_off: bool,
    pub chat_view: bool,
    pub turns: Vec<Value>,
    pub more: bool,
    pub settings: Value,
    pub unavailable: String,
    pub listen: Option<voiceio::Listen>,
    pub speaker: Option<std::sync::mpsc::Sender<voiceio::Out>>,
    pub say_id: u64,
    pub last_voice: Option<Instant>,
    pub level: f32,
    /// show_in_crundi's last answer to the server (the driver reads it).
    pub last_ack: Value,
    /// The microphone's loudest moment of late, and when (recentMicPeak).
    pub peak: f32,
    pub peak_at: Option<Instant>,
    pub epoch: u64,
    pub unread: i64,
    pub menu: bool,
    /// Compact asks first (the web's .cv-confirm under the top row).
    pub confirm: bool,
    /// How the screen was opened (the web's S.how): "click", "hover", "ptt".
    pub how: String,
    /// Push to talk held.
    pub ptt: bool,
    /// A hover-opened screen closes then, unless Cru is busy (maybeLeave).
    pub leave_at: Option<Instant>,
    /// Push to talk let go: what was heard goes then (pttRelease).
    pub ptt_send_at: Option<Instant>,
    /// Voice can work here (a Deepgram key, its limit not reached).
    pub voice_usable: bool,
    /// The services (GET /api/services), for the field's cells, and when read.
    pub services: Vec<Value>,
    pub services_at: Option<Instant>,
    /// Files for the next turn.
    pub att: Vec<Att>,
    /// The reply being said now (the chat view's speaking head).
    pub saying: String,
    /// Words said or typed while Cru answers (the web's S.pending): queued,
    /// asked as soon as the answer is in; typed or not.
    pub pending: String,
    pub pending_typed: bool,
    /// Numbers each question: an answer to a cancelled one is dropped.
    pub turn: u64,
    /// The server takes words back (voice-cancel); off once it says it can't.
    pub no_cancel: bool,
    /// What the screen drew last (for the test driver): its place and size, its parts.
    pub ui: Value,
    /// The chat view's jump to the newest (T17): shown, its dot, how far from the end.
    pub jump: Value,
}

thread_local! { static C: RefCell<Cru> = RefCell::new(Cru { chat_view: true, phase: "ended".into(), voice_usable: true, ..Default::default() }); }
pub fn with<R>(f: impl FnOnce(&mut Cru) -> R) -> R { C.with(|c| f(&mut c.borrow_mut())) }

fn prefs() -> Value { std::fs::read(crundi_core::store::config_dir().join("cru.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({})) }
fn save_prefs() {
    let v = with(|c| json!({ "keyboard": c.keyboard, "voiceOff": c.voice_off, "chatView": c.chat_view }));
    let _ = std::fs::write(crundi_core::store::config_dir().join("cru.json"), v.to_string());
}

/// The state as the web names its colours (micTone, then the phase):
/// problem (unavailable, or a problem), offline (the stream lost), start
/// (waking), listen, think, speak, ended.
pub fn tone(phase: &str, unavailable: bool) -> &'static str {
    if unavailable || phase == "problem" { return "problem"; }
    match phase { "offline" => "offline", "waking" => "start", "listen" => "listen", "think" => "think", "speak" => "speak", _ => "ended" }
}

/// The word at the bottom right, only when the blob does not make it plain
/// (render: Unavailable, Offline, Waking, Asleep).
pub fn status_word(phase: &str, unavailable: bool) -> &'static str {
    match tone(phase, unavailable) { "problem" => "Unavailable", "offline" => "Offline", "start" => "Waking", "ended" => "Asleep", _ => "" }
}

/// "web (<host>), <ui>" (at most 200 characters): where the owner is.
pub fn where_text(host: &str, project: &str, tab: &str) -> String {
    let ui = if project.is_empty() { format!("{tab} tab") } else { format!("project {project}, {tab} tab") };
    let s = format!("web ({host}), {ui}");
    s.chars().take(200).collect()
}

pub fn available() -> bool { kit::get().api.is_some() }

pub fn toggle() { if with(|c| c.open) { close() } else { open(false) } }

/// The top-bar mark's click: a hover-opened screen stays; else it toggles.
pub fn click() {
    if with(|c| c.open) {
        if with(|c| c.how == "hover") { with(|c| { c.how = "click".into(); c.leave_at = None; }); } else { close() }
    } else { open(false); with(|c| c.how = "click".into()); }
}

/// Resting on the mark: open (hover), unless a chat's voice holds the mic.
pub fn hover(mic_taken: bool) {
    if with(|c| c.open) { with(|c| c.leave_at = None); return; }
    if mic_taken { return; }
    open(false);
    with(|c| c.how = "hover".into());
}

/// The pointer left the mark: a hover-opened screen closes in a moment.
pub fn leave() { with(|c| if c.open && c.how == "hover" { c.leave_at = Some(Instant::now() + Duration::from_millis(700)); }); }

/// The pointer on Cru's screen (mouseenter / mouseleave): staying keeps it open.
pub fn panel_hover(on: bool) { if on { with(|c| c.leave_at = None); } else { leave(); } }

/// A press on Cru's screen: a hover-opened one stays (pointerdown).
pub fn panel_pressed() { with(|c| if c.how == "hover" { c.how = "click".into(); c.leave_at = None; }); }

/// Push to talk pressed: open listening, or stop Cru talking (barge in).
pub fn ptt_down() {
    if !with(|c| c.open) { open(false); with(|c| c.how = "ptt".into()); }
    with(|c| c.ptt = true);
    if with(|c| c.phase == "speak") { stop_saying(); with(|c| c.phase = "listen".into()); render(); }
    render();
}

/// Push to talk let go: what was heard goes without waiting for a pause.
pub fn ptt_up() {
    with(|c| if c.ptt { c.ptt = false; c.how = "click".into(); c.ptt_send_at = Some(Instant::now() + Duration::from_millis(700)); });
    render();
}

/// When `tick` has work next with nothing waking the pump (lib.rs
/// `next_due`): a hover-opened screen's leave, push to talk's send, and,
/// while the screen is open, the services read again. Each is cleared or
/// moved on when `tick` handles it, so a past time is never given twice.
pub fn next_due() -> Option<Instant> { with(|c| due(c)) }

fn due(c: &Cru) -> Option<Instant> {
    let services = c.open.then(|| c.services_at.map(|t| t + SERVICES_EVERY).unwrap_or_else(Instant::now));
    [c.leave_at, c.ptt_send_at, services].into_iter().flatten().min()
}

/// The timers above, from the main pump.
pub fn tick() {
    let now = Instant::now();
    if with(|c| c.leave_at.is_some_and(|t| now >= t)) {
        let busy = with(|c| { c.leave_at = None; c.how != "hover" || c.phase == "think" || c.phase == "speak" || !c.interim.is_empty() });
        if !busy { close(); }
    }
    if with(|c| c.ptt_send_at.is_some_and(|t| now >= t)) {
        // What is being heard (the web's S.heard: the words so far).
        let heard = with(|c| { c.ptt_send_at = None; if c.open && c.phase == "listen" { c.interim.clone() } else { String::new() } });
        if !heard.trim().is_empty() { with(|c| c.interim.clear()); utter(&heard, false); }
    }
    if with(|c| c.open && c.services_at.is_none_or(|t| t.elapsed() >= SERVICES_EVERY)) { load_services(); }
}

/// The top bar's mark: Cru live (the screen open, Cru listening, thinking
/// or talking), and the unread reports.
pub fn mark() -> (bool, i64) { with(|c| (c.open && c.phase != "ended", c.unread)) }

/// Open the screen: warm the brain, its settings, the history, the mic
/// (the web's open and start).
pub fn open(report_only: bool) {
    let p = prefs();
    with(|c| {
        c.open = true; c.report_only = report_only; c.notice.clear(); c.menu = false; c.confirm = false; c.unavailable.clear();
        c.keyboard = p.get("keyboard").and_then(|x| x.as_bool()).unwrap_or(false);
        c.voice_off = p.get("voiceOff").and_then(|x| x.as_bool()).unwrap_or(false);
        c.chat_view = p.get("chatView").and_then(|x| x.as_bool()).unwrap_or(true);
        c.phase = if c.keyboard { "listen".into() } else { "waking".into() };
        c.services_at = None;
        c.epoch += 1;
    });
    kit::call(|api| api.post("/api/voice/warm", &json!({})), |_| {});
    kit::call(|api| api.post("/api/voice/seen", &json!({})), |_| {});
    with(|c| c.unread = 0);
    kit::call(|api| api.get("/api/voice/settings"), |r| {
        if !with(|c| c.open) { return; }
        match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) != Some(false) => {
                let usable = kit::truthy(&v["deepgramKeySet"]) && !kit::truthy(&v["deepgramLimitReached"]);
                // The Claude Code brain needs no key: it runs on the subscription.
                let blocked = !kit::truthy(&v["anthropicKeySet"]) && kit::s(&v, "brain") != "claude-code";
                with(|c| { c.settings = v; c.voice_usable = usable; });
                if blocked {
                    with(|c| { c.unavailable = "There is no Claude API key on the server yet. Add one in Settings, Voice assistant.".into(); c.phase = "ended".into(); });
                } else if !usable || with(|c| c.keyboard) {
                    // Keyboard mode when voice can't work here, or when the owner chose it last time on this device.
                    with(|c| { c.keyboard = true; c.listen = None; c.phase = "listen".into(); });
                } else if !with(|c| c.report_only) { listen(); }
            }
            Ok(v) => with(|c| { c.unavailable = Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "The voice assistant is not available on this server.".into()); c.phase = "ended".into(); }),
            Err(e) => with(|c| { c.unavailable = if e.contains("Not supported") { "The voice assistant is not available on this server.".into() } else if e.is_empty() { "Could not reach the server".into() } else { e }; c.phase = "ended".into(); }),
        }
        render();
    });
    if with(|c| c.chat_view) { history(None); }
    load_services();
    crate::cru_screen::opened();
    render();
}

/// The services, for the field's cells (a project's service running or not).
fn load_services() {
    with(|c| c.services_at = Some(Instant::now()));
    kit::call(|api| api.get("/api/services"), |r| if let Ok(v) = r {
        with(|c| c.services = v.get("services").and_then(|s| s.as_array()).cloned().unwrap_or_default());
        render();
    });
}

/// The microphone and Cru's voice stop (kid mode's mute, a break).
pub fn stop() {
    with(|c| { c.listen = None; c.phase = "ended".into(); c.interim.clear(); });
    stop_saying();
    render();
}

pub fn close() {
    // The kid screen is left only with the PIN (cru_kid::exit).
    if crate::cru_kid::on() { return; }
    crate::cru_screen::closed();
    with(|c| { c.open = false; c.how.clear(); c.ptt = false; c.leave_at = None; c.ptt_send_at = None; c.listen = None; c.phase = "ended".into(); c.heard.clear(); c.interim.clear(); c.menu = false; c.confirm = false; c.saying.clear(); c.pending.clear(); c.epoch += 1; });
    stop_saying();
    render();
}

fn stop_saying() {
    let s = with(|c| c.speaker.clone());
    if let Some(s) = s { let _ = s.send(voiceio::Out::Stop); }
}

/// The microphone and its socket (the chat voice's), until asleep.
pub fn listen() {
    let c0 = kit::get();
    let Some(api) = c0.api else { return };
    let base = c0.base;
    let epoch = with(|c| { c.epoch += 1; c.phase = "waking".into(); c.last_voice = Some(Instant::now()); c.epoch });
    let l = voiceio::Listen::start(move || api.fresh_access().ok().map(|t| voice::listen_url(&base, &t)), move |ev| {
        kit::main_thread(move || on_listen(epoch, ev));
    });
    with(|c| c.listen = Some(l));
    render();
    silence_tick(epoch);
}

fn silence_tick(epoch: u64) {
    later(Duration::from_secs(2), move || {
        let (alive, asleep) = with(|c| (c.epoch == epoch && c.open && c.listen.is_some(), c.phase == "listen" && c.last_voice.is_some_and(|t| t.elapsed() > Duration::from_millis(SILENCE_MS))));
        if !alive { return; }
        if asleep { with(|c| { c.listen = None; c.phase = "ended".into(); }); render(); return; }
        silence_tick(epoch);
    });
}

fn on_listen(epoch: u64, ev: voiceio::Ev) {
    if with(|c| c.epoch != epoch) { return; }
    match ev {
        voiceio::Ev::Msg(voice::Msg::Ready) => {
            with(|c| { if c.phase == "waking" { c.phase = "listen".into(); } c.last_voice = Some(Instant::now()); });
            tone_out(false);
        }
        // A video playing: what the microphone hears is the video; only "Hey
        // Cru" alone, said loudly over it, gets Cru back (the video pauses).
        voiceio::Ev::Msg(voice::Msg::Interim(_)) if crate::cru_screen::video_playing() => return,
        voiceio::Ev::Msg(voice::Msg::Utterance(t)) if crate::cru_screen::video_playing() => {
            if is_calling(&t) && with(|c| c.peak_at.is_some_and(|a| a.elapsed() < Duration::from_millis(2500)) && c.peak >= 0.55) { crate::cru_screen::talk_over_video(); }
            return;
        }
        voiceio::Ev::Msg(voice::Msg::Interim(t)) => with(|c| { c.interim = t; c.last_voice = Some(Instant::now()); }),
        voiceio::Ev::Msg(voice::Msg::Utterance(t)) => {
            with(|c| c.interim.clear());
            // Push to talk held: the words wait for the key to be let go.
            if with(|c| c.ptt) { with(|c| c.interim = t); } else if !t.trim().is_empty() { utter(&t, false); }
        }
        voiceio::Ev::Msg(voice::Msg::Discard) => with(|c| c.interim.clear()),
        voiceio::Ev::Msg(voice::Msg::Error(e)) => with(|c| { c.notice = e; c.phase = "problem".into(); }),
        voiceio::Ev::Msg(_) => {}
        // The level moves only the field and the mic, which are animating while it is heard
        // (cru_ui takes it each frame): no redraw of its own.
        voiceio::Ev::Level(l) => { with(|c| {
            c.level = l;
            if l > 0.08 { c.last_voice = Some(Instant::now()); }
            if l >= c.peak || c.peak_at.is_none_or(|a| a.elapsed() > Duration::from_millis(2500)) { c.peak = l; c.peak_at = Some(Instant::now()); }
        }); return; }
        voiceio::Ev::Closed => with(|c| { c.listen = None; if c.phase != "ended" { c.phase = "offline".into(); } }),
        voiceio::Ev::NoMic => with(|c| { c.listen = None; c.keyboard = true; c.phase = "listen".into(); c.notice = "The microphone is not allowed here, so it is keyboard mode. Allow the microphone for this site to talk.".into(); }),
    }
    render();
}

/// "Hey Cru" (or the name) alone (isCalling).
pub fn is_calling(t: &str) -> bool {
    let lower = t.to_lowercase();
    let mut w: Vec<&str> = lower.split(|ch: char| !(ch.is_alphanumeric() || ch == '\'')).filter(|x| !x.is_empty()).collect();
    while w.first().is_some_and(|x| matches!(*x, "hey" | "hi" | "hello" | "ok" | "okay" | "oi" | "yo")) { w.remove(0); }
    !w.is_empty() && w.len() <= 2 && w.iter().all(|x| matches!(*x, "cru" | "crew" | "kru" | "krew" | "crue" | "crews"))
}

/// A video started playing: the microphone's socket pauses, and what was
/// being heard goes (videoMessage 'mute').
pub fn video_mute() {
    with(|c| { if let Some(l) = &c.listen { l.send(voiceio::Ctl::Pause); } c.heard.clear(); c.interim.clear(); });
}

/// The video stopped (or "Talk to Cru"): listening again ('resume').
pub fn video_resume() {
    let had = with(|c| { let had = c.listen.is_some(); if let Some(l) = &c.listen { l.send(voiceio::Ctl::Resume); } if had { c.phase = "listen".into(); c.heard.clear(); c.last_voice = Some(Instant::now()); } had });
    if had { tone_out(false); }
}

fn speaker() -> std::sync::mpsc::Sender<voiceio::Out> {
    if let Some(s) = with(|c| c.speaker.clone()) { return s; }
    let s = voiceio::speaker(|id| kit::main_thread(move || said(id)));
    with(|c| c.speaker = Some(s.clone()));
    s
}

fn tone_out(sent: bool) { let _ = speaker().send(voiceio::Out::Tone(sent)); }

/// After a reply, or with nothing to say: listening (the mic, or the keyboard), or asleep.
fn after_phase(c: &Cru) -> String { if c.listen.is_some() || c.keyboard { "listen".into() } else { "ended".into() } }

/// Words from the owner (the web's utter): while Cru thinks they are queued
/// and asked next; typed over Cru speaking, it stops and asks; said over it,
/// they wait for it to finish; otherwise asked now.
pub fn utter(text: &str, typed: bool) {
    let t = text.trim().to_string();
    if t.is_empty() && with(|c| c.att.iter().all(|a| a.id.is_empty() && a.r#ref.is_empty())) { return; }
    let phase = with(|c| c.phase.clone());
    if phase == "speak" && typed { stop_saying(); let id = with(|c| c.say_id); said(id); }
    let phase = with(|c| c.phase.clone());
    if (phase == "think" || phase == "speak") && !t.is_empty() {
        with(|c| { c.pending_typed = (c.pending.is_empty() || c.pending_typed) && typed; c.pending = format!("{} {t}", c.pending).trim().to_string(); c.interim.clear(); });
        render();
        return;
    }
    ask(&t, typed);
}

/// What waits to be asked next, while Cru answers (queuedOf).
pub fn queued(c: &Cru) -> String { if c.phase == "think" || c.phase == "speak" { c.pending.trim().to_string() } else { String::new() } }

/// The words still coming in (hearingOf): listening, what is heard; thinking,
/// what is heard beyond what was asked and what is queued.
pub fn hearing(c: &Cru) -> String {
    let h = c.interim.trim();
    if h.is_empty() || !(c.phase == "listen" || c.phase == "think") { return String::new(); }
    if c.phase == "listen" { return h.to_string(); }
    if h == c.heard.trim() || h == c.pending.trim() { String::new() } else { h.to_string() }
}

/// The × by his words (canCancel): there are words to take back, listening or thinking.
pub fn can_cancel(c: &Cru) -> bool {
    !c.no_cancel && match c.phase.as_str() { "listen" => !c.interim.trim().is_empty(), "think" => !c.heard.trim().is_empty() || !c.pending.trim().is_empty(), _ => false }
}

/// What a cancelled turn says (cancelledNotice): what it had already done.
pub fn cancelled_notice(acted: &Value) -> String {
    let a: Vec<String> = acted.as_array().map(|l| l.iter().map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| kit::s(x, "what"))).filter(|x| !x.is_empty()).collect()).unwrap_or_default();
    if a.is_empty() { "Cancelled.".into() } else { format!("Cancelled. Already done: {}.", a.join("; ")) }
}

/// Take back what he said (takeBack): listening, the words go (the stream
/// is told); thinking, the turn is cancelled on the server with what was
/// queued, and its answer is neither said nor shown.
pub fn cancel_words() {
    if !with(|c| can_cancel(c)) { return; }
    with(|c| if let Some(l) = &c.listen { l.send(voiceio::Ctl::Discard); });
    if with(|c| c.phase == "listen") { with(|c| c.interim.clear()); render(); return; }
    with(|c| {
        c.turn += 1; c.phase = after_phase(c); c.heard.clear(); c.interim.clear(); c.pending.clear(); c.pending_typed = false; c.notice = "Cancelled.".into();
        if c.turns.last().is_some_and(|t| !kit::s(t, "said").is_empty() && kit::s(t, "speak").is_empty() && kit::s(t, "show").is_empty()) { c.turns.pop(); }
    });
    render();
    let turn = with(|c| c.turn);
    kit::call(|api| api.post("/api/voice/cancel", &json!({})), move |r| {
        match r {
            Err(e) if e.contains("404") || e.contains("Not supported") => with(|c| c.no_cancel = true),
            Ok(v) if v.get("acted").and_then(|a| a.as_array()).is_some_and(|a| !a.is_empty()) => with(|c| if c.turn == turn && c.phase == "listen" && c.interim.is_empty() { c.notice = cancelled_notice(&v["acted"]); }),
            _ => {}
        }
        render();
    });
}

/// The × on a faded bubble (dropLive): only that phrase goes. "queued": it
/// is never asked; "hearing": the stream drops it. The answer is untouched.
pub fn drop_live(which: &str) {
    let stream = with(|c| {
        if which == "queued" {
            if queued(c).is_empty() { return None; }
            c.pending.clear(); c.pending_typed = false;
            return Some(false);
        }
        if hearing(c).is_empty() { return None; }
        c.interim.clear();
        Some(true)
    });
    if stream == Some(true) { with(|c| if let Some(l) = &c.listen { l.send(voiceio::Ctl::Discard); }); }
    if stream.is_some() { render(); }
}

/// Cru is free again: what was queued meanwhile is asked now (afterSpeech).
fn ask_pending() {
    let p = with(|c| if c.phase == "listen" && !c.pending.trim().is_empty() { let p = (std::mem::take(&mut c.pending), c.pending_typed); c.pending_typed = false; Some(p) } else { None });
    if let Some((p, typed)) = p { ask(&p, typed); }
}

/// A turn: what was said (or typed) to Cru's brain, with the files attached.
pub fn ask(text: &str, typed: bool) {
    let text = text.trim().to_string();
    // The uploaded files and the dropped references (takeAtt); the tray keeps what is still uploading.
    let ready: Vec<Att> = with(|c| c.att.iter().filter(|a| !a.id.is_empty() || !a.r#ref.is_empty()).cloned().collect());
    if text.is_empty() && ready.is_empty() { return; }
    with(|c| c.att.retain(|a| a.up));
    let label = ready.iter().map(|a| if a.r#ref.is_empty() { format!("[The owner attached: \"{}\" ({}), id {}.]", a.name, if a.kind.is_empty() { "file" } else { &a.kind }, a.id) } else { format!("[The owner dropped in: {}]", a.r#ref) }).collect::<Vec<_>>().join("\n");
    let asked = if label.is_empty() { text.clone() } else if text.is_empty() { label } else { format!("{label}\n{text}") };
    let host = gethostname::gethostname().to_string_lossy().to_string();
    let wh = where_text(&host, &kit::project(), &kit::get().tab);
    let rid = format!("w{}", crundi_webview::engine::now_ms());
    let turn = with(|c| { c.turn += 1; c.turn });
    with(|c| { c.heard = asked.clone(); c.interim.clear(); c.phase = "think".into(); c.reply.clear(); c.detail.clear(); c.question.clear(); c.notice.clear(); c.turns.push(json!({ "said": asked, "at": crundi_webview::engine::now_ms() })); });
    if !typed { tone_out(true); }
    render();
    let ids: Vec<String> = ready.iter().filter(|a| !a.id.is_empty()).map(|a| a.id.clone()).collect();
    let refs: Vec<String> = ready.iter().filter(|a| !a.r#ref.is_empty()).map(|a| a.r#ref.clone()).collect();
    let body = json!({ "text": text, "where": wh, "page": "", "locked": false, "typed": typed, "attachments": ids, "refs": refs, "requestId": rid, "sayNow": true });
    let mut body = body;
    body["page"] = json!(page_id());
    kit::call(move |api| api.post("/api/voice/turn", &body), move |r| {
        // Taken back meanwhile (or a newer question): not this answer's place.
        if with(|c| c.turn != turn || c.phase != "think") { return; }
        match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()).unwrap_or(true) && v.get("error").is_none() => answered(&v, typed),
            Ok(v) => { with(|c| { c.notice = Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Something went wrong".into()); c.phase = after_phase(c); }); render(); ask_pending(); }
            Err(e) => { with(|c| { c.notice = if e.is_empty() { "Something went wrong".into() } else { e }; c.phase = after_phase(c); }); render(); ask_pending(); }
        }
    });
}

fn answered(v: &Value, typed: bool) {
    let speak = kit::s(v, "speak");
    with(|c| {
        c.reply = speak.clone();
        c.detail = kit::s(v, "show");
        c.question = v.get("pending").and_then(|p| p.as_array()).and_then(|p| p.first()).map(|q| kit::s(q, "question")).unwrap_or_default();
        if v.pointer("/speaker/unrecognised").and_then(|x| x.as_bool()).unwrap_or(false) { c.notice = "Cru could not recognise who was speaking, so nothing was changed.".into(); }
        if let Some(t) = c.turns.last_mut() { t["speak"] = json!(speak); t["show"] = v["show"].clone(); t["media"] = Value::Array(media_of(v.get("media"))); }
    });
    let close_after = kit::truthy(v.get("close").unwrap_or(&Value::Null));
    // Typed turns are read, not said; "voice off" is never said.
    if typed || with(|c| c.voice_off) || speak.is_empty() {
        with(|c| c.phase = after_phase(c));
        render();
        if close_after { close(); return; }
        ask_pending();
        return;
    }
    say(&speak, close_after);
}

/// Say a reply: the server's voice, streamed (the mic paused meanwhile).
pub fn say(text: &str, close_after: bool) {
    let id = with(|c| { c.say_id += 1; c.phase = "speak".into(); c.saying = text.to_string(); if let Some(l) = &c.listen { l.send(voiceio::Ctl::Saying(text.to_string())); l.send(voiceio::Ctl::Pause); } c.say_id });
    if close_after { with(|c| c.notice = "closing".into()); }
    render();
    if kit::get().api.is_none() { return said(id); }
    let out = speaker();
    let text = text.to_string();
    let _ = out.send(voiceio::Out::Begin(id, 24_000));
    kit::bg(move |api| {
        let mut carry: Vec<u8> = vec![];
        let r = api.post_stream("/api/voice/speak?stream=1", &json!({ "text": text }), "audio/L16", &mut |ct, xr| {
            let _ = out.send(voiceio::Out::Begin(id, voice::speak_rate(xr, ct)));
        }, &mut |b| {
            carry.extend_from_slice(b);
            let n = carry.len() / 2 * 2;
            let s: Vec<i16> = carry[..n].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            carry.drain(..n);
            out.send(voiceio::Out::Pcm(id, s)).is_ok()
        });
        let _ = out.send(voiceio::Out::End(id));
        r.is_ok()
    }, move |ok| {
        if !ok { said(id); }
        if close_after { later(Duration::from_millis(600), move || if with(|c| c.say_id == id) { close() }); }
    });
}

fn said(id: u64) {
    let report = with(|c| {
        if c.say_id != id || c.phase != "speak" { return None; }
        if let Some(l) = &c.listen { l.send(voiceio::Ctl::Said); l.send(voiceio::Ctl::Resume); }
        c.phase = after_phase(c);
        c.saying.clear();
        c.last_voice = Some(Instant::now());
        Some(c.report_only)
    });
    render();
    if report.is_some() { ask_pending(); }
    // A report closes again after a moment, unless the owner talks.
    if report == Some(true) {
        let epoch = with(|c| c.epoch);
        later(Duration::from_millis(REPORT_LINGER_MS), move || if with(|c| c.epoch == epoch && c.report_only) { close() });
    }
}

/// The conversation so far (newest last), or the page before [before].
pub fn history(before: Option<u64>) {
    let path = format!("/api/voice/history?limit=40{}", before.map(|b| format!("&before={b}")).unwrap_or_default());
    kit::call(move |api| api.get(&path), move |r| {
        if let Ok(v) = r {
            let turns = expand_turns(v.get("turns").and_then(|t| t.as_array()).cloned().unwrap_or_default());
            with(|c| { c.more = kit::truthy(&v["more"]); if before.is_some() { let mut t = turns; t.extend(std::mem::take(&mut c.turns)); c.turns = t; } else { c.turns = turns; } });
            render();
        }
    });
}

/// A turn with lines said during it (say_now): his words, each line, then
/// the answer, each its own message (the web's expandTurns).
pub fn expand_turns(list: Vec<Value>) -> Vec<Value> {
    let mut out = vec![];
    for t in list {
        let ups: Vec<String> = t.get("updates").and_then(|u| u.as_array()).map(|u| u.iter().map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| kit::s(x, "speak"))).filter(|s| !s.trim().is_empty()).collect()).unwrap_or_default();
        if ups.is_empty() { out.push(t); continue; }
        let at = t.get("at").cloned().unwrap_or(Value::Null);
        if !kit::s(&t, "said").is_empty() { out.push(json!({ "at": at, "said": kit::s(&t, "said") })); }
        for u in ups { out.push(json!({ "at": at, "speak": u.trim() })); }
        out.push(json!({ "at": at, "speak": kit::s(&t, "speak"), "show": kit::s(&t, "show") }));
    }
    out
}

/// The mic (tapMic): stop Cru speaking (barge in), or wake it.
pub fn mic() {
    with(|c| c.report_only = false);
    if with(|c| !c.unavailable.is_empty()) { return; }
    let phase = with(|c| c.phase.clone());
    match phase.as_str() {
        "speak" => { stop_saying(); let id = with(|c| c.say_id); said(id); }
        "ended" | "offline" | "problem" => { with(|c| { c.keyboard = false; c.notice.clear(); }); save_prefs(); listen(); }
        _ => {}
    }
}

/// A voice report (/ws voice-report): the screen opens and says it.
pub fn report(v: &Value) {
    let speak = kit::s(v, "speak");
    let media = media_of(v.get("media"));
    if speak.is_empty() && media.is_empty() { return; }
    if !with(|c| c.open) { open(true); }
    with(|c| { c.reply = speak.clone(); c.detail = kit::s(v, "show"); c.turns.push(json!({ "speak": speak, "show": kit::s(v, "show"), "media": media, "at": crundi_webview::engine::now_ms() })); });
    if !speak.is_empty() { say(&speak, false); } else { render(); }
}

/// A reply's media as it came (a turn, a report, the history): only objects
/// with a kind, 24 at most (mediaOf).
pub fn media_of(list: Option<&Value>) -> Vec<Value> {
    list.and_then(|l| l.as_array()).map(|l| l.iter().filter(|x| x.get("kind").is_some_and(|k| k.is_string())).take(24).cloned().collect()).unwrap_or_default()
}

/// The ⋯ menu (its open state), and its items: the chat or voice view,
/// Cru's voice muted, compact (asks first, under the top row).
pub fn menu_toggle() { with(|c| c.menu = !c.menu); render(); }

pub fn menu_pick(id: &str) {
    with(|c| c.menu = false);
    match id {
        "chat-view" => { with(|c| c.chat_view = !c.chat_view); save_prefs(); if with(|c| c.chat_view) { history(None); with(|c| c.unread = 0); } }
        "voice-view" => { with(|c| c.chat_view = false); save_prefs(); }
        "voice-off" => { with(|c| c.voice_off = !c.voice_off); save_prefs(); if with(|c| c.voice_off && c.phase == "speak") { stop_saying(); let id = with(|c| c.say_id); said(id); } }
        "compact" => with(|c| c.confirm = !c.confirm),
        "compact-no" => with(|c| c.confirm = false),
        "compact-yes" => {
            with(|c| { c.confirm = false; c.notice = "Compacting the conversation…".into(); });
            kit::call(|api| api.post("/api/voice/compact", &json!({})), |r| {
                with(|c| c.notice = match r {
                    Ok(v) if kit::truthy(v.get("ok").unwrap_or(&Value::Null)) => "Compacted: Cru carries on from a summary, its notebook kept.".into(),
                    Ok(v) => { let why = Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| kit::s(&v, "skipped")); if why.is_empty() { "Not compacted".into() } else { format!("Not compacted: {why}") } }
                    Err(_) => "Not compacted".into(),
                });
                render();
            });
        }
        _ => {}
    }
    render();
}

/// Keyboard mode on or off (the web's keyboard(on, chosen)): off needs a
/// usable voice; on lets the microphone go and listens to the keys.
/// The kid screen's keyboard button: typing to Cru, no microphone (and the
/// mute let go quietly), or back to the voice; not remembered for the
/// grown-ups' screen (kidMute(false, true), keyboard(!S.kb, false)).
pub fn kid_keyboard(on: bool) {
    // Back to the voice needs a usable voice, as for the grown-ups (keyboard(false)).
    if !on && !with(|c| c.voice_usable) { voice_refused(); return; }
    crate::cru_kid::with(|k| k.muted = false);
    with(|c| { c.keyboard = on; c.listen = None; if c.unavailable.is_empty() { c.phase = "listen".into(); } c.interim.clear(); });
    if !on && with(|c| c.open && c.unavailable.is_empty()) { listen(); }
    render();
}

/// Why the voice can't be used, in the notice; keyboard mode stays.
fn voice_refused() {
    with(|c| c.notice = if kit::truthy(&c.settings["deepgramLimitReached"]) { "Voice is off: this month's Deepgram limit is reached.".into() } else { "Voice needs a Deepgram key: add one in Settings, Voice assistant.".into() });
    render();
}

/// Woken by "Hey Cru" (the owner, V7): always voice mode, whatever was
/// used last. Opening, keyboard mode is let go first (and remembered, as a
/// choice of voice); already open in keyboard mode, it goes to the voice.
pub fn open_by_voice() {
    if with(|c| c.open) {
        if with(|c| c.keyboard) { set_keyboard(false); }
        return;
    }
    with(|c| c.keyboard = false);
    save_prefs();
    open(false);
}

pub fn set_keyboard(on: bool) {
    if !on && !with(|c| c.voice_usable) { voice_refused(); return; }
    with(|c| { c.keyboard = on; c.listen = None; if on { if c.unavailable.is_empty() { c.phase = "listen".into(); } c.interim.clear(); } });
    save_prefs();
    if !on && with(|c| c.open && c.unavailable.is_empty()) { listen(); }
    render();
}

/// The attach button: the system's file picker (the driver may answer it),
/// each file uploaded for Cru (/api/voice/attach), shown as a chip until the
/// next turn takes it.
pub fn attach() {
    kit::bg(|_| crundi_core::driver::take_pick().map(|p| vec![p]).or_else(|| rfd::FileDialog::new().pick_files()).unwrap_or_default(), attach_paths);
}

/// Files for Cru from a drop anywhere on its screen (and the attach button):
/// ten at most, none in kid mode (attachFiles).
pub fn attach_paths(paths: Vec<std::path::PathBuf>) {
    if crate::cru_kid::on() { return; }
    for p in paths.into_iter().take(10) {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
        upload(name, Box::new(move || std::fs::read(&p).map_err(|e| e.to_string())));
    }
}

/// A picture pasted into the typing box (the paste's files).
pub fn attach_bytes(name: String, bytes: Vec<u8>) {
    if crate::cru_kid::on() { return; }
    upload(name, Box::new(move || Ok(bytes)));
}

/// A kanban card, a file row, a mindmap idea… dragged from Crundi onto Cru:
/// its reference ("[Kanban Task ID: …]"), sent with the next turn (addRef).
pub fn add_ref(t: &str) {
    if crate::cru_kid::on() { return; }
    let t: String = t.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(400).collect();
    if t.is_empty() { return; }
    with(|c| c.att.push(Att { name: t.clone(), r#ref: t, ..Default::default() }));
    render();
}

type Read = Box<dyn FnOnce() -> Result<Vec<u8>, String> + Send>;

fn upload(name: String, read: Read) {
    use base64::Engine;
    let i = with(|c| { c.att.push(Att { name: name.clone(), up: true, ..Default::default() }); c.att.len() - 1 });
    render();
    let n2 = name.clone();
    kit::bg(move |api| {
        let bytes = read().map_err(|e| (e, None))?;
        if bytes.len() > 50 * 1024 * 1024 { return Err(("Too large (50 MB at most)".to_string(), None)); }
        let thumb = thumb_of(&bytes);
        let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        api.post("/api/voice/attach", &json!({ "name": n2, "type": "", "data": data, "from": "web" })).map(|v| (v, thumb.clone())).map_err(|e| (e.to_string(), thumb))
    }, move |r| {
        let (r, thumb) = match r { Ok((v, t)) => (Ok(v), t), Err((e, t)) => (Err(e), t) };
        with(|c| if let Some(a) = c.att.get_mut(i).filter(|a| a.name == name) {
            a.up = false;
            a.thumb = thumb;
            match r {
                Ok(v) if kit::truthy(v.get("ok").unwrap_or(&Value::Null)) => { a.id = kit::s(&v, "id"); a.kind = kit::s(&v, "kind"); let n = kit::s(&v, "name"); if !n.is_empty() { a.name = n; } }
                Ok(v) => a.bad = Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Could not attach".into()),
                Err(e) => a.bad = if e.contains("404") || e.contains("Not supported") { "This server does not take files for Cru yet: update it".into() } else { e },
            }
        });
        render();
    });
}

pub fn unattach(i: usize) { with(|c| if i < c.att.len() { c.att.remove(i); }); render(); }

/// The unread dot's count.
pub fn load_unread() {
    kit::call(|api| api.get("/api/voice/unread"), |r| if let Ok(v) = r { with(|c| c.unread = kit::n(&v, "count") as i64); render(); });
}

/// The screen draws again (cru_ui.rs reads the state).
pub fn render() { kit::dirty(); }

/// The words under the field (render): what is being heard (listening),
/// what was asked; the invitation when listening with no reply yet.
pub fn words() -> String {
    with(|c| {
        let h = if c.phase == "listen" { c.interim.clone() } else { c.heard.clone() };
        if !h.is_empty() { format!("“{h}”") } else if c.phase == "listen" && c.reply.is_empty() { "Ask me anything, Sir.".into() } else { String::new() }
    })
}

/// Usage and spend under the top row (refreshData): wk %, 5h %, Claude
/// $spent / $limit (the API brain only), Deepgram $spent / $limit, each
/// with its colour's name ("wk", "5h", "claude", "deepgram").
pub fn legend(usage: Option<(Option<f64>, Option<f64>)>) -> Vec<(&'static str, String)> {
    let mut items = vec![];
    if let Some((week, five)) = usage {
        if let Some(w) = week { items.push(("wk", format!("wk {}%", (w * 100.).round()))); }
        if let Some(f) = five { items.push(("5h", format!("5h {}%", (f * 100.).round()))); }
    }
    with(|c| {
        let s = &c.settings;
        let Some(sp) = s.get("spend").filter(|x| x.is_object()) else { return };
        // The Claude API's spend means nothing while Cru answers through Claude Code.
        let brain = Some(kit::s(s, "brain")).filter(|b| !b.is_empty()).unwrap_or_else(|| "api".into());
        if brain == "api" { items.push(("claude", format!("Claude ${:.2} / ${}", kit::n(sp, "claudeUsd"), js_num(s.get("monthlyLimitUsd"))))); }
        let dg = s.get("deepgramLimitUsd").filter(|x| x.as_f64().is_some_and(|v| v != 0.0)).cloned().unwrap_or(json!(200));
        items.push(("deepgram", format!("Deepgram ${:.2} / ${}", kit::n(sp, "deepgramUsd"), js_num(Some(&dg)))));
    });
    items
}

/// A number as JavaScript prints it (50, 12.5).
fn js_num(v: Option<&Value>) -> String {
    match v.and_then(|x| x.as_f64()) { Some(n) if n.fract() == 0.0 => format!("{}", n as i64), Some(n) => format!("{n}"), None => "undefined".into() }
}

/// The usage arcs in the disc, 0..1 each with its colour's name (drawField).
pub fn arcs(usage: Option<(Option<f64>, Option<f64>)>) -> Vec<(&'static str, f32)> {
    let mut a = vec![];
    if let Some((week, five)) = usage {
        if let Some(w) = week { a.push(("wk", w as f32)); }
        if let Some(f) = five { a.push(("5h", f as f32)); }
    }
    with(|c| {
        let s = &c.settings;
        let Some(sp) = s.get("spend").filter(|x| x.is_object()) else { return };
        let lim = kit::n(s, "monthlyLimitUsd");
        a.push(("claude", if lim > 0. { (kit::n(sp, "claudeUsd") / lim) as f32 } else { 1. }));
        let dg = Some(kit::n(s, "deepgramLimitUsd")).filter(|x| *x != 0.).unwrap_or(200.);
        a.push(("deepgram", if dg > 0. { (kit::n(sp, "deepgramUsd") / dg) as f32 } else { 1. }));
    });
    a
}

/// One project as a cell (the web's voiceOpts cells): its name, its kind
/// (needs, working, waiting, running, idle) and its service (running or
/// not; None without one). At most twelve.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell { pub name: String, pub kind: &'static str, pub service: Option<bool> }

pub fn cells() -> Vec<Cell> {
    let k = kit::get();
    let services = with(|c| c.services.clone());
    k.projects.iter().take(12).map(|p| {
        let alias = kit::s(p, "alias");
        let name = Some(kit::s(p, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| alias.clone());
        let svc: Vec<&Value> = services.iter().filter(|s| kit::s(s, "alias") == alias).collect();
        let svc_up = svc.iter().any(|s| kit::s(s, "status") == "running");
        let kind = match crundi_core::head::project_dot(&k.terminals, &alias) {
            "needs-input" => "needs", "working" => "working", "waiting" => "waiting", "running" => "running",
            _ if svc_up => "running", _ => "idle",
        };
        Cell { name, kind, service: if svc.is_empty() { None } else { Some(svc_up) } }
    }).collect()
}

/// The chat view's messages, oldest first: (mine, text, show, at ms, media).
/// A reply with only media is a message too (its media under no bubble).
pub fn turns() -> Vec<(bool, String, String, f64, Vec<Value>)> {
    with(|c| c.turns.iter().flat_map(|t| {
        let mut v = vec![];
        let at = kit::n(t, "at");
        if !kit::s(t, "said").is_empty() { v.push((true, kit::s(t, "said"), String::new(), at, vec![])); }
        let (sp, sh, mm) = (kit::s(t, "speak"), kit::s(t, "show"), media_of(t.get("media")));
        if !sp.is_empty() || !sh.is_empty() || !mm.is_empty() { v.push((false, sp, sh, at, mm)); }
        v
    }).collect())
}

/// A message's time as the web shows it (toLocaleString: "Nov 17, 07:45 AM").
pub fn when(at_ms: f64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(at_ms as i64).single().map(|d| d.format("%b %-d, %I:%M %p").to_string()).unwrap_or_default()
}

/// Earlier messages (the chat view's top).
pub fn older() { let at = with(|c| c.turns.first().map(|t| kit::n(t, "at") as u64)); history(at); }

/// The test driver's stand-in for the microphone: words being heard.
pub fn test_state(phase: &str) {
    with(|c| { c.unavailable.clear(); c.notice.clear(); c.phase = phase.to_string(); });
    render();
}

/// For the test driver: the conversation as the history would give it.
pub fn test_turns(list: &str) {
    let list = serde_json::from_str::<Vec<Value>>(list).unwrap_or_default();
    with(|c| c.turns = expand_turns(list));
    render();
}

pub fn test_hear(text: &str) {
    with(|c| { c.phase = "listen".into(); c.interim = text.to_string(); c.last_voice = Some(Instant::now()); });
    render();
}

pub fn props() -> Value {
    let un = with(|c| !c.unavailable.is_empty());
    let leg: Vec<String> = legend(None).into_iter().map(|(_, s)| s).collect();
    with(|c| json!({
        "open": c.open, "phase": c.phase, "heard": c.heard, "interim": c.interim, "reply": c.reply, "notice": c.notice, "keyboard": c.keyboard, "turns": c.turns.len(),
        "unavailable": c.unavailable, "listening": c.listen.is_some(), "chatView": c.chat_view, "menu": c.menu, "confirm": c.confirm, "voiceOff": c.voice_off,
        "how": c.how, "ptt": c.ptt, "tone": tone(&c.phase, un), "word": status_word(&c.phase, un), "legend": leg, "attachments": c.att.len(), "refs": c.att.iter().filter(|a| !a.r#ref.is_empty()).map(|a| a.r#ref.clone()).collect::<Vec<_>>(), "page": page_id(), "ui": c.ui, "lastAck": c.last_ack, "pending": c.pending, "queued": queued(c), "hearing": hearing(c), "canCancel": can_cancel(c),
    }))
}


// ─── Cru shows the owner things in Crundi (show_in_crundi, voice/ui-nav.js) ───

/// This window's own id, sent with each turn: Cru moves only the page the
/// owner talked from (crundiPageId).
pub fn page_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let r = std::collections::hash_map::RandomState::new();
        let n = { use std::hash::{BuildHasher, Hasher}; let mut h = r.build_hasher(); h.write_u128(t); h.finish() };
        format!("pg{}{}", radix36(t as u64), &radix36(n)[..6.min(radix36(n).len())])
    })
}

fn radix36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 { return "0".into(); }
    let mut v = vec![];
    while n > 0 { v.push(D[(n % 36) as usize]); n /= 36; }
    v.reverse();
    String::from_utf8(v).unwrap_or_default()
}

/// The tabs that need no project (GLOBAL_TABS).
const GLOBAL_TABS: [&str; 8] = ["media", "mindmap", "widgets", "docker", "homepool", "info", "secrets", "settings"];

/// What the window shows, for Cru's "looking at" (cruUiWhere).
pub fn ui_where() -> String {
    let c = kit::get();
    let name = c.projects.iter().find(|p| kit::s(p, "alias") == c.project).map(|p| { let n = kit::s(p, "name"); if n.is_empty() { kit::s(p, "alias") } else { n } });
    let mut bits = vec![name.map(|n| format!("project {n}")).unwrap_or_else(|| "no project open".into()), format!("{} tab", c.tab)];
    if c.tab == "workbench" && !c.focused.is_empty() {
        if let Some(t) = c.terminals.iter().find(|t| kit::s(t, "id") == c.focused && kit::s(t, "kind") == "ui") {
            bits.push(format!("chat \"{}\"", kit::s(t, "title").chars().take(60).collect::<String>()));
        }
    }
    bits.join(", ")
}

/// One move from Cru (the `cru-ui` event, for this window only): the
/// project, the tab, the chat (placed, focused and flashed), then a card
/// (flashed, its Edit dialog when asked to open it), an idea (flashed, its
/// details when asked), a file (its window) or a panel (selected in Panels);
/// then what to ring (cruUiGo, src/webapp-html.js:6876-6933): the words of
/// a highlight found on screen (`w::find`), else the target, with Cru's note
/// beside the ring; the note is a toast only when nothing is targeted or a
/// dialog was opened. Answers the server (/api/voice/ui/ack).
pub fn ui_go(d: &Value) {
    if kit::s(d, "page") != page_id() { return; }
    let id = kit::s(d, "id");
    let ack = move |mut r: Value| {
        r["id"] = json!(id.clone());
        with(|c| c.last_ack = r.clone());
        // Where the window is once the move has landed (the next pump).
        kit::on_app(move |_, _| { r["where"] = json!(ui_where()); let body = r.clone(); kit::call(move |api| api.post("/api/voice/ui/ack", &body), |_| {}); });
    };
    let c = kit::get();
    let project = kit::s(d, "project");
    if !project.is_empty() && project != c.project {
        if !c.projects.iter().any(|p| kit::s(p, "alias") == project) { return ack(json!({ "ok": false, "error": "That project is not on this page" })); }
        kit::select_project(&project);
    }
    let has_project = !project.is_empty() || !c.project.is_empty();
    let tab = kit::s(d, "tab");
    if !tab.is_empty() {
        if !has_project && !GLOBAL_TABS.contains(&tab.as_str()) && tab != "inbox" { return ack(json!({ "ok": false, "error": "No project is open on the page: say which project" })); }
        if tab != c.tab { kit::show_tab(&tab); }
    }
    let note = kit::s(d, "note");
    let open = kit::truthy(d.get("open").unwrap_or(&Value::Null));
    let highlight = kit::s(d, "highlight");
    let ack: Box<dyn FnOnce(Value)> = Box::new(ack);
    let chat = kit::s(d, "chat");
    let mut target: Option<Target> = None;
    if !chat.is_empty() {
        let key = format!("live:{chat}");
        let k2 = key.clone();
        kit::on_app(move |_, cx| if let Some(wb) = crundi_workbench::workbench(cx) { wb.update(cx, |w, cx| { w.place(&k2, cx); w.focus_key(&k2, cx); }); });
        // Flashed once it is laid out (flashEl), unless words are to be rung.
        if highlight.is_empty() {
            let t = Target::Chat(key.clone());
            poll(2500, move |w, cx| crate::cru_spot::flash(t.clone(), w, cx).then_some(()), |_| {});
        }
        target = Some(Target::Chat(key));
    }
    let (card, node, file, widget) = (kit::s(d, "card"), kit::s(d, "node"), kit::s(d, "file"), kit::s(d, "widget"));
    if !widget.is_empty() { kit::on_app(move |_, cx| crundi_operations::panels::show(&widget, cx)); }
    if !file.is_empty() {
        let project = if project.is_empty() { c.project.clone() } else { project.clone() };
        if project.is_empty() { return ack(json!({ "ok": false, "error": "No project is open on the page" })); }
        kit::on_app(move |_, cx| crundi_workspace::wins::open_file(&project, &file, cx));
    }
    if !card.is_empty() {
        if c.tab != "kanban" && tab != "kanban" { kit::show_tab("kanban"); }
        // The board loaded again first (loadKanban), then the card looked for.
        let after = std::rc::Rc::new(std::cell::Cell::new(None::<u64>));
        let t = Target::Card(card.clone());
        return poll(2500, move |w, cx| {
            let want = match after.get() { Some(g) => g, None => { let g = crundi_planning::kanban::load_now(cx); after.set(Some(g)); g } };
            crundi_planning::kanban::show_card(&card, open, want, w, cx)
        }, move |r| match r {
            Some(true) => finish(ack, Some(t), note, highlight, open),
            _ => ack(json!({ "ok": false, "error": "No such card on this project's board" })),
        });
    }
    if !node.is_empty() {
        if c.tab != "mindmap" && tab != "mindmap" { kit::show_tab("mindmap"); }
        return poll(2500, move |_, cx| crundi_planning::mindmap::has_node(&node, cx).map(|has| (has, node.clone())), move |r| match r {
            Some((true, node)) => {
                let n2 = node.clone();
                kit::on_app(move |w, cx| crundi_planning::mindmap::flash_node(&n2, open, w, cx));
                finish(ack, Some(Target::Idea(node)), note, highlight, open)
            }
            _ => ack(json!({ "ok": false, "error": "No such idea on this project's mindmap" })),
        });
    }
    finish(ack, target, note, highlight, open)
}

use crate::cru_spot::Target;

/// The end of a move (cruUiGo's last part): the highlight's words looked
/// for (up to 2.5 s, as the tab's content comes) and rung, with the note;
/// not found, the target rung instead and found: false. With no highlight,
/// the note in a ring around the target, or a toast when nothing is
/// targeted or a dialog was opened over it (`open`).
fn finish(ack: Box<dyn FnOnce(Value)>, target: Option<Target>, note: String, highlight: String, open: bool) {
    if !highlight.is_empty() {
        let words = Target::Words(highlight);
        let n2 = note.clone();
        return poll(2500, move |w, cx| crate::cru_spot::spotlight(words.clone(), &n2, w, cx).then_some(()), move |r| {
            if r.is_some() { return ack(json!({ "ok": true, "found": true })); }
            if let Some(t) = target { ring(t, note); }
            ack(json!({ "ok": true, "found": false }))
        });
    }
    match target {
        Some(t) if !note.is_empty() && !open => ring(t, note),
        _ => if !note.is_empty() { kit::toast(&format!("Cru: {note}")); },
    }
    ack(json!({ "ok": true }))
}

/// The ring around `t` once it is on screen (up to 1.5 s: a card or an idea
/// is laid out after the switch); never on screen, the note as a toast.
fn ring(t: Target, note: String) {
    let n2 = note.clone();
    poll(1500, move |w, cx| crate::cru_spot::spotlight(t.clone(), &n2, w, cx).then_some(()), move |r| {
        if r.is_none() && !note.is_empty() { kit::toast(&format!("Cru: {note}")); }
    });
}

/// `f` with the window until it gives something, every 150 ms for up to
/// `ms` (a tab's content loads after the switch: cruWaitFor); then `done`.
fn poll<R: 'static>(ms: u64, f: impl Fn(&mut gpui::Window, &mut gpui::App) -> Option<R> + 'static, done: impl FnOnce(Option<R>) + 'static) {
    let end = Instant::now() + Duration::from_millis(ms);
    fn step<R: 'static>(end: Instant, f: std::rc::Rc<dyn Fn(&mut gpui::Window, &mut gpui::App) -> Option<R>>, done: Box<dyn FnOnce(Option<R>)>) {
        kit::on_app(move |w, cx| {
            if let Some(r) = f(w, cx) { return done(Some(r)); }
            if Instant::now() >= end { return done(None); }
            later(Duration::from_millis(150), move || step(end, f, done));
        });
    }
    step(end, std::rc::Rc::new(f), Box::new(done));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calling_over_a_video() {
        assert!(is_calling("Hey Cru") && is_calling("cru crew") && is_calling("OK, Krew!"));
        assert!(!is_calling("hey") && !is_calling("hey cru what is that") && !is_calling("crew cru cru"));
    }

    #[test]
    fn words() {
        assert_eq!(status_word("offline", false), "Offline");
        assert_eq!(status_word("listen", true), "Unavailable");
        assert_eq!(status_word("listen", false), "");
        assert_eq!(status_word("waking", false), "Waking");
        assert_eq!(status_word("ended", false), "Asleep");
        assert_eq!(status_word("think", false), "");
        assert_eq!(tone("waking", false), "start");
        assert_eq!(tone("speak", true), "problem");
        assert_eq!(where_text("pc1", "demo", "kanban"), "web (pc1), project demo, kanban tab");
        assert_eq!(where_text("pc1", "", "inbox"), "web (pc1), inbox tab");
        assert!(where_text(&"x".repeat(300), "", "a").chars().count() <= 200);
        assert_eq!(js_num(Some(&json!(50))), "50");
        assert_eq!(js_num(Some(&json!(12.5))), "12.5");
    }

    /// The pump's deadlines: none while closed with nothing pending; a leave or a push-to-talk
    /// send when set; open, the services' next read. A deadline handled is never given again.
    #[test]
    fn deadlines() {
        let now = Instant::now();
        let mut c = Cru::default();
        assert_eq!(due(&c), None);
        c.open = true;
        c.services_at = Some(now);
        assert_eq!(due(&c), Some(now + SERVICES_EVERY));
        c.leave_at = Some(now + Duration::from_millis(700));
        assert_eq!(due(&c), Some(now + Duration::from_millis(700)));
        c.ptt_send_at = Some(now + Duration::from_millis(300));
        assert_eq!(due(&c), Some(now + Duration::from_millis(300)));
        // Opened and never read: due now (the pump reads them at once).
        let c2 = Cru { open: true, ..Default::default() };
        assert!(due(&c2).is_some_and(|t| t <= Instant::now()));

        // A hover-opened screen whose leave fell due while Cru thinks: it stays open, and the
        // deadline is gone (no spin on a past time).
        with(|c| { *c = Cru { open: true, how: "hover".into(), phase: "think".into(), services_at: Some(Instant::now()), leave_at: Some(Instant::now() - Duration::from_millis(5)), ..Default::default() }; });
        assert!(next_due().is_some_and(|t| t <= Instant::now()));
        tick();
        assert!(with(|c| c.open && c.leave_at.is_none()));
        assert!(next_due().is_some_and(|t| t > Instant::now() + Duration::from_secs(10)));
        // Push to talk let go with nothing heard: handled once, then gone.
        with(|c| c.ptt_send_at = Some(Instant::now() - Duration::from_millis(1)));
        tick();
        assert!(with(|c| c.ptt_send_at.is_none()));
        assert!(next_due().is_some_and(|t| t > Instant::now()));
    }

    /// Hover, leave, click and push to talk as the web's attach() and pttPress / pttRelease.
    #[test]
    fn hover_leave_ptt() {
        with(|c| { *c = Cru { open: true, how: "hover".into(), phase: "listen".into(), services_at: Some(Instant::now()), ..Default::default() }; });
        leave();
        assert!(with(|c| c.leave_at.is_some()));
        panel_hover(true);
        assert!(with(|c| c.leave_at.is_none()));
        leave();
        panel_pressed();
        assert!(with(|c| c.how == "click" && c.leave_at.is_none()));
        leave();
        assert!(with(|c| c.leave_at.is_none()), "a clicked-open screen does not close on leave");
        ptt_down();
        assert!(with(|c| c.ptt));
        ptt_up();
        assert!(with(|c| !c.ptt && c.ptt_send_at.is_some()));
        assert_eq!(mark(), (true, 0));
    }

    #[test]
    fn turns_with_updates() {
        let t = expand_turns(vec![json!({ "at": 5, "said": "hi", "speak": "done", "updates": ["one moment", ""] }), json!({ "at": 6, "said": "x", "speak": "y" })]);
        assert_eq!(t.len(), 4);
        assert_eq!(t[1]["speak"], "one moment");
        assert_eq!(t[2]["speak"], "done");
    }
}
