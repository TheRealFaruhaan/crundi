//! Cru on the PC (app/vendor/crundi-voice.js; spec
//! specs/desktop/passwords-homepool-cru.md C): the voice assistant's screen,
//! a dropdown under the top bar's Cru mark. It listens (the chat voice's
//! microphone and socket, src/voiceio.rs), asks the server's Cru brain
//! (/api/voice/turn), says the answer (/api/voice/speak, streamed), and
//! listens again; asleep after 30 s of silence. Keyboard mode types instead.
//! The chat view keeps the conversation (/api/voice/history).

use crate::{kit, voice, voiceio, DesktopCru, DesktopCruTurn};
use serde_json::{json, Value};
use slint::{ComponentHandle, VecModel};
use std::{cell::RefCell, rc::Rc, time::{Duration, Instant}};

pub const SILENCE_MS: u64 = 30_000;
pub const REPORT_LINGER_MS: u64 = 8_000;

#[derive(Default)]
struct Cru {
    open: bool,
    report_only: bool,
    /// waking, listen, think, speak, ended (asleep), offline, problem
    phase: String,
    heard: String,
    interim: String,
    reply: String,
    detail: String,
    notice: String,
    question: String,
    keyboard: bool,
    voice_off: bool,
    chat_view: bool,
    turns: Vec<Value>,
    more: bool,
    settings: Value,
    unavailable: String,
    listen: Option<voiceio::Listen>,
    speaker: Option<std::sync::mpsc::Sender<voiceio::Out>>,
    say_id: u64,
    last_voice: Option<Instant>,
    level: f32,
    gen: u64,
    unread: i64,
    menu: bool,
}

thread_local! { static C: RefCell<Cru> = RefCell::new(Cru { chat_view: true, phase: "ended".into(), ..Default::default() }); }
fn with<R>(f: impl FnOnce(&mut Cru) -> R) -> R { C.with(|c| f(&mut c.borrow_mut())) }

fn prefs() -> Value { std::fs::read(crate::store::config_dir().join("cru.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({})) }
fn save_prefs() {
    let v = with(|c| json!({ "keyboard": c.keyboard, "voiceOff": c.voice_off, "chatView": c.chat_view }));
    let _ = std::fs::write(crate::store::config_dir().join("cru.json"), v.to_string());
}

/// The state pill's word.
pub fn pill(phase: &str) -> &'static str {
    match phase { "waking" => "Waking", "listen" => "Listening", "think" => "Thinking", "speak" => "Speaking", _ => "Asleep" }
}

/// The status word under the mic, only when the colour is not enough.
pub fn status_word(phase: &str, unavailable: bool) -> &'static str {
    if unavailable { return "Unavailable"; }
    match phase { "offline" => "Offline", "waking" => "Waking", "ended" => "Asleep", _ => "" }
}

/// "web (<host>), <ui>" (at most 200 characters): where the owner is.
pub fn where_text(host: &str, project: &str, tab: &str) -> String {
    let ui = if project.is_empty() { format!("{tab} tab") } else { format!("project {project}, {tab} tab") };
    let s = format!("web ({host}), {ui}");
    s.chars().take(200).collect()
}

pub fn available() -> bool { kit::api().is_some() }

pub fn toggle() { if with(|c| c.open) { close() } else { open(false) } }

/// Open the screen: warm the brain, its settings, the history, the mic.
pub fn open(report_only: bool) {
    let p = prefs();
    with(|c| {
        c.open = true; c.report_only = report_only; c.notice.clear(); c.menu = false;
        c.keyboard = p.get("keyboard").and_then(|x| x.as_bool()).unwrap_or(false);
        c.voice_off = p.get("voiceOff").and_then(|x| x.as_bool()).unwrap_or(false);
        c.chat_view = p.get("chatView").and_then(|x| x.as_bool()).unwrap_or(true);
        c.gen += 1;
    });
    if let Some(w) = kit::ui() { w.global::<crate::Shell>().set_modal_desktop(false); }
    kit::call(|api| api.post("/api/voice/warm", &json!({})), |_| {});
    kit::call(|api| api.post("/api/voice/seen", &json!({})), |_| {});
    with(|c| c.unread = 0);
    kit::call(|api| api.get("/api/voice/settings"), |r| {
        match r {
            Ok(v) => {
                let blocked = if !kit::truthy(&v["anthropicKeySet"]) && kit::s(&v, "brain") != "claude-code" { "There is no Claude API key on the server yet. Add one in Settings, Voice assistant.".to_string() } else { String::new() };
                let no_voice = if !kit::truthy(&v["deepgramKeySet"]) { "Voice needs a Deepgram key: add one in Settings, Voice assistant." } else if kit::truthy(&v["deepgramLimitReached"]) { "Voice is off: this month's Deepgram limit is reached." } else { "" };
                with(|c| { c.settings = v; c.unavailable = blocked; if !no_voice.is_empty() { c.keyboard = true; c.notice = no_voice.into(); } });
            }
            Err(e) => with(|c| c.unavailable = if e.contains("Not supported") { "The voice assistant is not available on this server.".into() } else { e }),
        }
        if with(|c| !c.keyboard && !c.report_only && c.unavailable.is_empty()) { listen(); }
        render();
    });
    if with(|c| c.chat_view) { history(None); }
    render();
}

pub fn close() {
    with(|c| { c.open = false; c.listen = None; c.phase = "ended".into(); c.heard.clear(); c.interim.clear(); c.menu = false; c.gen += 1; });
    stop_saying();
    render();
}

fn stop_saying() {
    let s = with(|c| c.speaker.clone());
    if let Some(s) = s { let _ = s.send(voiceio::Out::Stop); }
}

/// The microphone and its socket (the chat voice's), until asleep.
pub fn listen() {
    let Some(api) = kit::api() else { return };
    let base = kit::base();
    let gen = with(|c| { c.gen += 1; c.phase = "waking".into(); c.last_voice = Some(Instant::now()); c.gen });
    let l = voiceio::Listen::start(move || api.fresh_access().ok().map(|t| voice::listen_url(&base, &t)), move |ev| {
        let _ = slint::invoke_from_event_loop(move || on_listen(gen, ev));
    });
    with(|c| c.listen = Some(l));
    render();
    silence_tick(gen);
}

fn silence_tick(gen: u64) {
    slint::Timer::single_shot(Duration::from_secs(2), move || {
        let (alive, asleep) = with(|c| (c.gen == gen && c.open && c.listen.is_some(), c.phase == "listen" && c.last_voice.is_some_and(|t| t.elapsed() > Duration::from_millis(SILENCE_MS))));
        if !alive { return; }
        if asleep { with(|c| { c.listen = None; c.phase = "ended".into(); }); render(); return; }
        silence_tick(gen);
    });
}

fn on_listen(gen: u64, ev: voiceio::Ev) {
    if with(|c| c.gen != gen) { return; }
    match ev {
        voiceio::Ev::Msg(voice::Msg::Ready) => {
            with(|c| { if c.phase == "waking" { c.phase = "listen".into(); } c.last_voice = Some(Instant::now()); });
            tone(false);
        }
        voiceio::Ev::Msg(voice::Msg::Interim(t)) => with(|c| { c.interim = t; c.last_voice = Some(Instant::now()); }),
        voiceio::Ev::Msg(voice::Msg::Utterance(t)) => { with(|c| c.interim.clear()); if !t.trim().is_empty() { ask(&t, false); } }
        voiceio::Ev::Msg(voice::Msg::Discard) => with(|c| c.interim.clear()),
        voiceio::Ev::Msg(voice::Msg::Error(e)) => with(|c| { c.notice = e; c.phase = "problem".into(); }),
        voiceio::Ev::Msg(_) => {}
        voiceio::Ev::Level(l) => { with(|c| { c.level = l; if l > 0.08 { c.last_voice = Some(Instant::now()); } }); }
        voiceio::Ev::Closed => with(|c| { c.listen = None; if c.phase != "ended" { c.phase = "offline".into(); } }),
        voiceio::Ev::NoMic => with(|c| { c.listen = None; c.keyboard = true; c.phase = "ended".into(); c.notice = "The microphone is not allowed here, so it is keyboard mode. Allow the microphone for this site to talk.".into(); }),
    }
    render();
}

fn speaker() -> std::sync::mpsc::Sender<voiceio::Out> {
    if let Some(s) = with(|c| c.speaker.clone()) { return s; }
    let s = voiceio::speaker(|id| { let _ = slint::invoke_from_event_loop(move || said(id)); });
    with(|c| c.speaker = Some(s.clone()));
    s
}

fn tone(sent: bool) { let _ = speaker().send(voiceio::Out::Tone(sent)); }

/// A turn: what was said (or typed) to Cru's brain.
pub fn ask(text: &str, typed: bool) {
    let text = text.trim().to_string();
    if text.is_empty() { return; }
    let host = gethostname::gethostname().to_string_lossy().to_string();
    let wh = where_text(&host, &kit::project(), &kit::ui().map(|w| w.get_current_tab().to_string()).unwrap_or_default());
    let rid = format!("w{}", crate::browser::now_ms());
    with(|c| { c.heard = text.clone(); c.phase = "think".into(); c.reply.clear(); c.detail.clear(); c.question.clear(); c.notice.clear(); c.turns.push(json!({ "said": text, "at": crate::browser::now_ms() })); });
    if !typed { tone(true); }
    render();
    let body = json!({ "text": text, "where": wh, "page": "", "locked": false, "typed": typed, "attachments": [], "refs": [], "requestId": rid, "sayNow": true });
    kit::call(move |api| api.post("/api/voice/turn", &body), move |r| {
        match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()).unwrap_or(true) && v.get("error").is_none() => answered(&v, typed),
            Ok(v) => { with(|c| { c.notice = Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Something went wrong".into()); c.phase = "listen".into(); }); render(); }
            Err(e) => { with(|c| { c.notice = if e.is_empty() { "Something went wrong".into() } else { e }; c.phase = "listen".into(); }); render(); }
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
        if let Some(t) = c.turns.last_mut() { t["speak"] = json!(speak); t["show"] = v["show"].clone(); }
    });
    let close_after = kit::truthy(v.get("close").unwrap_or(&Value::Null));
    // Typed turns are read, not said; "voice off" is never said.
    if typed || with(|c| c.voice_off) || speak.is_empty() {
        with(|c| c.phase = if c.listen.is_some() { "listen".into() } else { "ended".into() });
        render();
        if close_after { close(); }
        return;
    }
    say(&speak, close_after);
}

/// Say a reply: the server's voice, streamed (the mic paused meanwhile).
pub fn say(text: &str, close_after: bool) {
    let id = with(|c| { c.say_id += 1; c.phase = "speak".into(); if let Some(l) = &c.listen { l.send(voiceio::Ctl::Saying(text.to_string())); l.send(voiceio::Ctl::Pause); } c.say_id });
    if close_after { with(|c| c.notice = "closing".into()); }
    render();
    let (Some(api), out) = (kit::api(), speaker()) else { return said(id) };
    let text = text.to_string();
    let _ = out.send(voiceio::Out::Begin(id, 24_000));
    kit::bg(move || {
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
        if close_after { slint::Timer::single_shot(Duration::from_millis(600), move || if with(|c| c.say_id == id) { close() }); }
    });
}

fn said(id: u64) {
    let report = with(|c| {
        if c.say_id != id || c.phase != "speak" { return None; }
        if let Some(l) = &c.listen { l.send(voiceio::Ctl::Said); l.send(voiceio::Ctl::Resume); }
        c.phase = if c.listen.is_some() { "listen".into() } else { "ended".into() };
        c.last_voice = Some(Instant::now());
        Some(c.report_only)
    });
    render();
    // A report closes again after a moment, unless the owner talks.
    if report == Some(true) {
        let gen = with(|c| c.gen);
        slint::Timer::single_shot(Duration::from_millis(REPORT_LINGER_MS), move || if with(|c| c.gen == gen && c.report_only) { close() });
    }
}

/// The conversation so far (newest last), or the page before [before].
pub fn history(before: Option<u64>) {
    let path = format!("/api/voice/history?limit=40{}", before.map(|b| format!("&before={b}")).unwrap_or_default());
    kit::call(move |api| api.get(&path), move |r| {
        if let Ok(v) = r {
            let turns = v.get("turns").and_then(|t| t.as_array()).cloned().unwrap_or_default();
            with(|c| { c.more = kit::truthy(&v["more"]); if before.is_some() { let mut t = turns; t.extend(std::mem::take(&mut c.turns)); c.turns = t; } else { c.turns = turns; } });
            render();
        }
    });
}

/// The mic: wake (asleep), or stop Cru speaking (barge in).
pub fn mic() {
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
    if speak.is_empty() { return; }
    if !with(|c| c.open) { open(true); }
    with(|c| { c.reply = speak.clone(); c.detail = kit::s(v, "show"); c.turns.push(json!({ "speak": speak, "at": crate::browser::now_ms() })); });
    say(&speak, false);
}

pub fn menu_pick(id: &str) {
    with(|c| c.menu = false);
    match id {
        "chat-view" => { with(|c| c.chat_view = !c.chat_view); save_prefs(); if with(|c| c.chat_view) { history(None); } }
        "voice-off" => { with(|c| c.voice_off = !c.voice_off); save_prefs(); }
        "compact" => kit::confirm(kit::Confirm { title: "Compact the conversation?", message: "Cru carries on from a summary; its notebook and memory search keep everything.", ok: "Compact", danger: false }, |ok| {
            if !ok { return; }
            with(|c| c.notice = "Compacting the conversation…".into());
            render();
            kit::call(|api| api.post("/api/voice/compact", &json!({})), |r| {
                with(|c| c.notice = match r { Ok(v) if v.get("ok").and_then(|o| o.as_bool()).unwrap_or(true) => "Compacted: Cru carries on from a summary, its notebook kept.".into(), Ok(v) => format!("Not compacted: {}", kit::s(&v, "error")), Err(e) => format!("Not compacted: {e}") });
                render();
            });
        }),
        _ => {}
    }
    render();
}

pub fn set_keyboard(on: bool) {
    with(|c| { c.keyboard = on; if on { c.listen = None; c.phase = "ended".into(); } });
    save_prefs();
    if !on { listen(); }
    render();
}

/// The unread dot's count.
pub fn load_unread() {
    kit::call(|api| api.get("/api/voice/unread"), |r| if let Ok(v) = r { with(|c| c.unread = kit::n(&v, "count") as i64); render(); });
}

pub fn render() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<DesktopCru>();
    let (open, phase, heard, interim, reply, detail, notice, question, keyboard, voice_off, chat_view, turns, more, unavailable, menu, unread, settings) = with(|c| (c.open, c.phase.clone(), c.heard.clone(), c.interim.clone(), c.reply.clone(), c.detail.clone(), c.notice.clone(), c.question.clone(), c.keyboard, c.voice_off, c.chat_view, c.turns.clone(), c.more, c.unavailable.clone(), c.menu, c.unread, c.settings.clone()));
    g.set_open(open);
    g.set_phase(phase.clone().into());
    g.set_pill(pill(&phase).into());
    g.set_status(status_word(&phase, !unavailable.is_empty()).into());
    g.set_words(if !interim.is_empty() { format!("“{interim}”") } else if !heard.is_empty() { format!("“{heard}”") } else { "Ask me anything, Sir.".into() }.into());
    g.set_reply(reply.into());
    g.set_detail(detail.into());
    g.set_notice(if !unavailable.is_empty() { unavailable } else { notice }.into());
    g.set_question(question.into());
    g.set_keyboard(keyboard);
    g.set_voice_off(voice_off);
    g.set_chat_view(chat_view);
    g.set_more(more);
    g.set_menu(menu);
    g.set_unread(unread as i32);
    let spend = settings.get("spend").cloned().unwrap_or(Value::Null);
    g.set_legend(if spend.is_null() { String::new() } else { format!("Deepgram ${:.2} / ${:.0}", kit::n(&spend, "deepgramUsd"), kit::n(&settings, "deepgramLimitUsd")) }.into());
    let rows: Vec<DesktopCruTurn> = turns.iter().flat_map(|t| {
        let mut v = vec![];
        if !kit::s(t, "said").is_empty() { v.push(DesktopCruTurn { mine: true, text: kit::s(t, "said").into() }); }
        let s = Some(kit::s(t, "speak")).filter(|x| !x.is_empty()).unwrap_or_else(|| kit::s(t, "show"));
        if !s.is_empty() { v.push(DesktopCruTurn { mine: false, text: s.into() }); }
        v
    }).collect();
    g.set_turns(Rc::new(VecModel::from(rows)).into());
    g.set_hint("Hold Ctrl+Space to talk · click the mic to keep this open".into());
}

pub fn init(w: &crate::MainWindow) {
    let g = w.global::<DesktopCru>();
    g.on_close(close);
    g.on_mic(mic);
    g.on_send(|t| ask(&t, true));
    g.on_keyboard_toggle(|| { let k = with(|c| c.keyboard); set_keyboard(!k); });
    g.on_menu_open(|| { with(|c| c.menu = !c.menu); render(); });
    g.on_menu_pick(|id| menu_pick(&id));
    g.on_older(|| { let at = with(|c| c.turns.first().map(|t| kit::n(t, "at") as u64)); history(at); });
    g.on_answer(|yes| ask(if yes { "Yes" } else { "No" }, true));
    g.on_say_again(|t| say(&t, false));
    w.global::<crate::Shell>().on_cru(toggle);
    render();
}

pub fn props() -> Value {
    with(|c| json!({ "open": c.open, "phase": c.phase, "heard": c.heard, "reply": c.reply, "notice": c.notice, "keyboard": c.keyboard, "turns": c.turns.len(), "unavailable": c.unavailable, "listening": c.listen.is_some() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        assert_eq!(pill("listen"), "Listening");
        assert_eq!(pill("ended"), "Asleep");
        assert_eq!(status_word("offline", false), "Offline");
        assert_eq!(status_word("listen", true), "Unavailable");
        assert_eq!(status_word("listen", false), "");
        assert_eq!(where_text("pc1", "demo", "kanban"), "web (pc1), project demo, kanban tab");
        assert_eq!(where_text("pc1", "", "inbox"), "web (pc1), inbox tab");
        assert!(where_text(&"x".repeat(300), "", "a").chars().count() <= 200);
    }
}
