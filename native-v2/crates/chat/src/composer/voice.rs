//! 2.24: the mic (the web's .cc-mic and "Voice mode" in claude-chat.js;
//! native-gpui's chat-composer/voice.rs). Shown only when the server can
//! listen (the chat-voice capability and a Deepgram key). A tap is voice
//! mode (every utterance goes as "voice input: …"; a tap with words in
//! progress sends them now; a tap with none ends it), a hold (350 ms) is push
//! to talk (let go: what was heard goes as one; slid onto × and let go:
//! dropped). One recording in the whole client: a hold in another chat
//! pauses voice mode, voice mode in another chat moves it. While recording
//! the box shows the words (the draft waits under them) and Send's place the
//! strip (dot, level bars, time, status) and ×. The voice line above the box
//! says notes, errors and "Speaking…". Claude's replies (voice-say) are said
//! in voice mode or within 10 minutes of a hold, each once, queued. The
//! rules are crundi_core::voice, the audio crundi_core::voiceio.
//!
//! v2: the state is one entity (`Voice`); the audio threads' news comes down
//! an async channel the UI thread awaits (no polling), and each change emits
//! the chat it concerns, so only that chat's composer redraws (the strip at
//! most 30 times a second while recording).

use super::Composer;
use crundi_core::voice::{self as rules, Msg};
use crundi_core::voiceio::{self as io, Ctl, Dev, Ev, Out};
use crundi_core::live::Event;
use crundi_shell::Area;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use futures::channel::mpsc as amp;
use futures::StreamExt as _;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Rec {
    chat: String,
    hold: bool,
    listen: Option<io::Listen>,
    interim: String,
    heard: Vec<String>,
    started: Instant,
    level: f32,
    sending: bool,
    cancelling: bool,
    sent_now: Option<(String, Instant)>,
    last_heard: Option<Instant>,
    rgen: u64,
}

/// What the audio threads tell the UI thread.
enum In { Ev(u64, Ev), Done(u64), DevDone(u64, bool) }

/// A change for one chat's composer (None: every one).
pub struct Changed(pub Option<String>);

pub struct Voice {
    ok: Option<(bool, Instant)>,
    asking: bool,
    cap: bool,
    settings: Value,
    rec: Option<Rec>,
    away: Option<String>,
    lines: HashMap<String, (String, String, u64)>,
    line_gen: u64,
    ptt_at: HashMap<String, Instant>,
    press: Option<(String, u64)>,
    press_gen: u64,
    rgen: u64,
    seen: HashSet<String>,
    queue: VecDeque<(String, String)>,
    saying: Option<(String, u64)>,
    paused: bool,
    say_seq: u64,
    speaker: Option<mpsc::Sender<Out>>,
    device: Option<mpsc::Sender<Dev>>,
    on_device: bool,
    /// The device's voice said the last reply (None: not used yet).
    device_voiced: Option<bool>,
    tones: u32,
    said: u32,
    last_sent: String,
    tx: amp::UnboundedSender<In>,
    /// Where × is (the hold's slide), and when the strip was last redrawn.
    x_bounds: Option<Bounds<Pixels>>,
    drawn: Option<Instant>,
}
impl EventEmitter<Changed> for Voice {}

struct Handle(Entity<Voice>);
impl Global for Handle {}

fn entity(cx: &mut App) -> Entity<Voice> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let (tx, mut rx) = amp::unbounded::<In>();
    let e = cx.new(|_| Voice {
        ok: None, asking: false, cap: false, settings: Value::Null, rec: None, away: None, lines: HashMap::new(), line_gen: 0,
        ptt_at: HashMap::new(), press: None, press_gen: 0, rgen: 0, seen: HashSet::new(), queue: VecDeque::new(), saying: None,
        paused: false, say_seq: 0, speaker: None, device: None, on_device: false, device_voiced: None, tones: 0, said: 0, last_sent: String::new(), tx,
        x_bounds: None, drawn: None,
    });
    cx.set_global(Handle(e.clone()));
    // The threads' news, as it comes (nothing runs while nothing is said).
    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Some(m) = rx.next().await {
            let mut got = vec![m];
            while let Ok(m) = rx.try_recv() { got.push(m); }
            let _ = cx.update(|cx| for m in got {
                match m {
                    In::Ev(g, e) => on_ev(cx, g, e),
                    In::Done(id) => reply_done(cx, id),
                    In::DevDone(id, voiced) => { with(cx, |v| v.device_voiced = Some(voiced)); reply_done(cx, id) }
                }
            });
        }
    }).detach();
    e
}

fn with<R>(cx: &mut App, f: impl FnOnce(&mut Voice) -> R) -> R { entity(cx).update(cx, |v, _| f(v)) }
fn read<R>(cx: &App, f: impl FnOnce(&Voice) -> R) -> Option<R> { cx.try_global::<Handle>().map(|h| f(h.0.read(cx))) }

/// Tell the composer of `chat` (None: all) to redraw.
fn changed(cx: &mut App, chat: Option<&str>) {
    let c = chat.map(String::from);
    entity(cx).update(cx, |_, cx| cx.emit(Changed(c)));
}

/// A composer's subscription: it redraws on its own chat's changes only
/// (the strip at most 30 times a second); the mic's check runs when the
/// first composer is made, at most once a minute after.
pub fn watch(chat: &str, cx: &mut Context<Composer>) -> Vec<Subscription> {
    let e = entity(cx);
    check(cx);
    let chat = chat.to_string();
    vec![cx.subscribe(&e, move |_, _, ev: &Changed, cx| if ev.0.as_deref().is_none_or(|c| c == chat) { cx.notify(); })]
}

/// The chat area's part: Claude's spoken replies (voice-say) and a chat's
/// exit, from the live link's batch.
pub fn init(cx: &mut App) {
    // Esc in the recording chat (the focused pane's), wherever the focus is
    // in it, ends voice mode or drops a hold (before the bindings).
    cx.intercept_keystrokes(|e, _, cx| {
        if e.keystroke.key != "escape" || rec_chat(cx).is_none() { return; }
        let chat = crate::focused_chat(cx);
        if escape(cx, &chat) { cx.stop_propagation(); }
    }).detach();
    crundi_shell::register(cx, Area::new("chat-voice")
        .on_batch(|_, batch, cx| for e in batch {
            let Event::Chat(m) = e else { continue };
            if m.get("type").and_then(|t| t.as_str()) != Some("ui-event") { continue; }
            let chat = m.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            let Some(ev) = m.get("event") else { continue };
            match ev.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "voice-say" => say(cx, &chat, ev.get("id").and_then(|x| x.as_str()).unwrap_or(""), ev.get("text").and_then(|x| x.as_str()).unwrap_or("")),
                "exit" => chat_ended(cx, &chat),
                _ => {}
            }
        })
        .on_signed_in(|_, cx| { with(cx, |v| v.ok = None); check(cx); })
        .on_signed_out(|_, cx| { if cx.has_global::<Handle>() { rec_end(cx); } })
        .test_ui(|_, name, args, _, cx| test_ui_app(name, args, cx))
        .state(|_, _, cx| state_app(cx)));
}

/// Whether the mic shows: the server lists chat-voice and has a Deepgram
/// key (asked once a minute at most).
pub fn check(cx: &mut App) {
    if with(cx, |v| v.asking || v.ok.is_some_and(|o| o.1.elapsed() < Duration::from_secs(60))) { return; }
    let Some(api) = crundi_shell::api(cx) else { return };
    with(cx, |v| v.asking = true);
    crundi_shell::run(cx, move || {
        let caps = crundi_core::api::handshake(api.base()).ok().and_then(|h| h.pointer("/server/capabilities").cloned()).unwrap_or(Value::Null);
        let cap = caps.as_array().is_some_and(|c| c.iter().any(|x| x == "chat-voice"));
        (cap, api.get("/api/voice/settings"))
    }, |(cap, r), cx| {
        let (st, s) = match r { Ok(v) => (200, v), Err(crundi_core::api::ApiError::Status(c, _)) => (c, Value::Null), Err(_) => (0, Value::Null) };
        let was = read(cx, |v| v.ok.map(|o| o.0)).flatten();
        let ok = rules::available(st, &s, cap);
        with(cx, |v| { v.asking = false; v.cap = cap; v.ok = Some((ok, Instant::now())); if st == 200 { v.settings = s; } });
        if was != Some(ok) { changed(cx, None); }
    });
}

// ─── The voice line ───

fn line(cx: &mut App, chat: &str, kind: &str, text: &str, for_ms: Option<u64>) {
    let g = with(cx, |v| { v.line_gen += 1; v.lines.insert(chat.to_string(), (kind.to_string(), text.to_string(), v.line_gen)); v.line_gen });
    changed(cx, Some(chat));
    if let Some(ms) = for_ms {
        let k = chat.to_string();
        super::after(cx, ms, move |cx| {
            let gone = with(cx, |v| {
                let due = v.lines.get(&k).is_some_and(|l| l.2 == g) && v.rec.as_ref().is_none_or(|r| r.chat != k);
                if due { v.lines.remove(&k); }
                due
            });
            if gone { changed(cx, Some(&k)); }
        });
    }
}
fn line_clear(cx: &mut App, chat: &str) { if with(cx, |v| v.lines.remove(chat).is_some()) { changed(cx, Some(chat)); } }

// ─── The mic ───

fn exited(cx: &App, chat: &str) -> bool {
    let s = crundi_shell::stores(cx);
    let p = s.projects.read(cx);
    p.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(chat)).is_some_and(|t| crundi_core::head::chat_state(t) == "exited")
}

/// Pressed: a tap or a hold, told apart at HOLD_MS. In voice mode any press is a tap.
pub fn press(cx: &mut App, chat: &str) {
    let (in_voice, g) = with(cx, |v| {
        let iv = v.rec.as_ref().is_some_and(|r| r.chat == chat && !r.hold);
        v.press_gen += 1;
        v.press = Some((chat.to_string(), v.press_gen));
        (iv, v.press_gen)
    });
    if in_voice { return; }
    let k = chat.to_string();
    super::after(cx, rules::HOLD_MS, move |cx| {
        let hold = with(cx, |v| { let h = v.press.as_ref().is_some_and(|p| p.0 == k && p.1 == g); if h { v.press = None; } h });
        if hold { hold_start(cx, &k); }
    });
}

pub fn release(cx: &mut App, chat: &str, over_x: bool) {
    let holding = read(cx, |v| v.rec.as_ref().is_some_and(|r| r.chat == chat && r.hold && !r.sending)).unwrap_or(false);
    if holding { return if over_x { hold_cancel(cx) } else { hold_release(cx) }; }
    if with(cx, |v| v.press.take().is_some_and(|p| p.0 == chat)) { tap(cx, chat); }
}

fn tap(cx: &mut App, chat: &str) {
    if with(cx, |v| v.saying.is_some() && !v.paused) { return reply_pause(cx); }
    match read(cx, |v| v.rec.as_ref().filter(|r| r.chat == chat && !r.hold).map(|r| r.interim.clone())).flatten() {
        Some(words) if !words.trim().is_empty() => {
            with(cx, |v| if let Some(r) = v.rec.as_mut() {
                if let Some(l) = &r.listen { l.send(Ctl::Discard); }
                r.sent_now = Some((words.trim().to_string(), Instant::now()));
                r.interim.clear();
                r.started = Instant::now();
            });
            send(cx, chat, &words);
        }
        Some(_) => stop(cx, chat),
        None => start(cx, chat),
    }
}

fn start(cx: &mut App, chat: &str) {
    if exited(cx, chat) { return line(cx, chat, "error", rules::ERR_ENDED, Some(rules::LINE_MS)); }
    let moved = with(cx, |v| {
        let mut moved = vec![];
        if let Some(r) = v.rec.take() { if r.chat != chat { moved.push(r.chat.clone()); } }
        if let Some(k) = v.away.take() { if k != chat { moved.push(k); } }
        moved
    });
    for k in moved { line(cx, &k, "note", rules::NOTE_MOVED, Some(rules::LINE_MS)); }
    rec_start(cx, chat, false);
}

fn rec_start(cx: &mut App, chat: &str, hold: bool) {
    let Some(api) = crundi_shell::api(cx) else { return };
    let (rgen, tx) = with(cx, |v| { v.rgen += 1; (v.rgen, v.tx.clone()) });
    let base = api.base().to_string();
    let listen = io::Listen::start(move || api.fresh_access().ok().map(|t| rules::listen_url(&base, &t)), move |ev| { let _ = tx.unbounded_send(In::Ev(rgen, ev)); });
    with(cx, |v| {
        v.lines.remove(chat);
        v.rec = Some(Rec { chat: chat.into(), hold, listen: Some(listen), interim: String::new(), heard: vec![], started: Instant::now(),
            level: 0.0, sending: false, cancelling: false, sent_now: None, last_heard: None, rgen });
    });
    if hold { reply_hold(cx); }
    changed(cx, Some(chat));
}

fn rec_chat(cx: &App) -> Option<String> { read(cx, |v| v.rec.as_ref().map(|r| r.chat.clone())).flatten() }

fn on_ev(cx: &mut App, rgen: u64, ev: Ev) {
    if !read(cx, |v| v.rec.as_ref().is_some_and(|r| r.rgen == rgen)).unwrap_or(false) { return; }
    let chat = rec_chat(cx).unwrap_or_default();
    match ev {
        Ev::Level(l) => {
            // The strip's bars and time: at most 30 redraws a second.
            let draw = with(cx, |v| {
                if let Some(r) = v.rec.as_mut() { r.level = l; }
                let due = v.drawn.is_none_or(|t| t.elapsed() >= Duration::from_millis(1000 / 30));
                if due { v.drawn = Some(Instant::now()); }
                due
            });
            if draw { changed(cx, Some(&chat)); }
        }
        Ev::Msg(Msg::Ready) => tone(cx, false),
        Ev::Msg(Msg::Interim(t)) => { with(cx, |v| if let Some(r) = v.rec.as_mut() { r.interim = t; r.last_heard = Some(Instant::now()); }); changed(cx, Some(&chat)); }
        Ev::Msg(Msg::Utterance(t)) => {
            let go = with(cx, |v| {
                let r = v.rec.as_mut()?;
                r.last_heard = Some(Instant::now());
                r.interim.clear();
                if r.hold { r.heard.push(t.clone()); return None; }
                if r.sent_now.as_ref().is_some_and(|(w, at)| at.elapsed() < Duration::from_millis(rules::ALREADY_SENT_MS) && w.split_whitespace().eq(t.split_whitespace())) { r.sent_now = None; return None; }
                r.started = Instant::now();
                Some(r.chat.clone())
            });
            changed(cx, Some(&chat));
            if let Some(k) = go { send(cx, &k, &t); }
        }
        Ev::Msg(Msg::Discard) => { with(cx, |v| if let Some(r) = v.rec.as_mut() { r.interim.clear(); r.heard.clear(); }); changed(cx, Some(&chat)); }
        Ev::Msg(Msg::Error(e)) => fail(cx, &e),
        Ev::Msg(Msg::Other) => {}
        Ev::Closed => {
            if read(cx, |v| v.rec.as_ref().is_some_and(|r| r.hold && r.sending)).unwrap_or(false) { hold_finish(cx, rgen); } else { fail(cx, rules::ERR_CLOSED); }
        }
        Ev::NoMic => fail(cx, rules::ERR_MIC),
    }
}

/// Words sent as "voice input: …" through the normal send; the draft stays; the sent tone.
fn send(cx: &mut App, chat: &str, words: &str) {
    let Some(text) = rules::voice_text(words) else { return };
    if exited(cx, chat) { return; }
    with(cx, |v| v.last_sent = text.clone());
    if let Some(s) = crate::find(cx, chat) { s.update(cx, |s, cx| s.send(text, cx)); }
    tone(cx, true);
}

fn fail(cx: &mut App, e: &str) {
    let Some(chat) = rec_chat(cx) else { return };
    rec_end(cx);
    line(cx, &chat, "error", e, Some(rules::LINE_MS));
}

fn rec_end(cx: &mut App) {
    let r = with(cx, |v| v.rec.take().map(|r| (r.chat, r.hold)));
    if let Some((chat, hold)) = r {
        changed(cx, Some(&chat));
        if hold { away_resume(cx); }
    }
}

fn away_resume(cx: &mut App) {
    let Some(k) = with(cx, |v| v.away.take()) else { return };
    line_clear(cx, &k);
    if !exited(cx, &k) { rec_start(cx, &k, false); }
}

fn stop(cx: &mut App, chat: &str) {
    if rec_chat(cx).as_deref() == Some(chat) { rec_end(cx); }
    reply_stop(cx);
    line_clear(cx, chat);
}

fn hold_start(cx: &mut App, chat: &str) {
    if exited(cx, chat) { return line(cx, chat, "error", rules::ERR_ENDED, Some(rules::LINE_MS)); }
    let paused = with(cx, |v| v.rec.take().filter(|r| r.chat != chat).map(|r| r.chat));
    if let Some(k) = paused { with(cx, |v| v.away = Some(k.clone())); line(cx, &k, "away", rules::NOTE_PAUSED, None); }
    rec_start(cx, chat, true);
}

fn hold_release(cx: &mut App) {
    let Some((rgen, chat)) = with(cx, |v| v.rec.as_mut().map(|r| { r.sending = true; r.cancelling = false; if let Some(l) = &r.listen { l.send(Ctl::Stop); } (r.rgen, r.chat.clone()) })) else { return };
    changed(cx, Some(&chat));
    super::after(cx, rules::SEND_WAIT_MS, move |cx| hold_finish(cx, rgen));
}

fn hold_finish(cx: &mut App, rgen: u64) {
    let Some((words, chat)) = with(cx, |v| {
        let r = v.rec.as_ref().filter(|r| r.rgen == rgen && r.hold)?;
        let out = (rules::ptt_words(&r.heard, &r.interim), r.chat.clone());
        v.ptt_at.insert(out.1.clone(), Instant::now());
        Some(out)
    }) else { return };
    rec_end(cx);
    if !words.is_empty() { send(cx, &chat, &words); }
}

fn hold_cancel(cx: &mut App) {
    with(cx, |v| if let Some(r) = &v.rec { if let Some(l) = &r.listen { l.send(Ctl::Discard); } });
    rec_end(cx);
}

/// ×: a hold is dropped; in voice mode the words go and the reply stops.
pub fn cancel(cx: &mut App, chat: &str) {
    match read(cx, |v| v.rec.as_ref().filter(|r| r.chat == chat).map(|r| r.hold)).flatten() {
        Some(true) => hold_cancel(cx),
        Some(false) => {
            with(cx, |v| { if let Some(r) = v.rec.as_mut() { if let Some(l) = &r.listen { l.send(Ctl::Discard); } r.interim.clear(); } });
            reply_stop(cx);
            with(cx, |v| v.queue.clear());
            changed(cx, Some(chat));
        }
        None => {}
    }
}

/// Esc in the recording chat ends voice mode or drops a hold (true: taken).
fn escape(cx: &mut App, chat: &str) -> bool {
    let Some(hold) = read(cx, |v| v.rec.as_ref().filter(|r| r.chat == chat).map(|r| r.hold)).flatten() else { return false };
    if hold { hold_cancel(cx) } else { stop(cx, chat) }
    true
}

/// A chat gone or exited: its recording ends.
pub fn chat_ended(cx: &mut App, chat: &str) {
    if !cx.has_global::<Handle>() { return; }
    if rec_chat(cx).as_deref() == Some(chat) { rec_end(cx); }
    with(cx, |v| if v.away.as_deref() == Some(chat) { v.away = None; });
}

// ─── The tones and the replies ───

fn speaker(cx: &mut App) -> mpsc::Sender<Out> {
    with(cx, |v| {
        if v.speaker.is_none() { let tx = v.tx.clone(); v.speaker = Some(io::speaker(move |id| { let _ = tx.unbounded_send(In::Done(id)); })); }
        v.speaker.clone().unwrap()
    })
}
fn device(cx: &mut App) -> mpsc::Sender<Dev> {
    with(cx, |v| {
        if v.device.is_none() { let tx = v.tx.clone(); v.device = Some(io::device_voice(move |id, voiced| { let _ = tx.unbounded_send(In::DevDone(id, voiced)); })); }
        v.device.clone().unwrap()
    })
}
fn tone(cx: &mut App, sent: bool) { with(cx, |v| v.tones += 1); let _ = speaker(cx).send(Out::Tone(sent)); }

/// A reply Claude gave a voice input: said in voice mode, or within the
/// window after a hold; each id once; they queue.
pub fn say(cx: &mut App, chat: &str, id: &str, text: &str) {
    let go = with(cx, |v| {
        if !id.is_empty() && !v.seen.insert(id.to_string()) { return false; }
        let voice_mode = v.rec.as_ref().is_some_and(|r| r.chat == chat && !r.hold) || v.away.as_deref() == Some(chat);
        let since = v.ptt_at.get(chat).map(|t| t.elapsed().as_millis() as u64);
        if !rules::should_speak(voice_mode, since.map(|_| 0), since.unwrap_or(0)) { return false; }
        v.queue.push_back((chat.to_string(), text.to_string()));
        true
    });
    if go { reply_next(cx); }
}

fn talking(v: &Voice) -> bool {
    v.rec.as_ref().is_some_and(|r| r.hold || r.sending || r.last_heard.is_some_and(|t| t.elapsed() < Duration::from_millis(rules::QUIET_MS)))
}

fn reply_next(cx: &mut App) {
    if with(cx, |v| v.saying.is_some()) { return; }
    if with(cx, |v| talking(v)) { if with(cx, |v| !v.queue.is_empty()) { super::after(cx, 250, reply_next); } return; }
    let Some((chat, text, id, voice, limit, rate, here)) = with(cx, |v| {
        let (chat, text) = v.queue.pop_front()?;
        v.say_seq += 1;
        v.saying = Some((chat.clone(), v.say_seq));
        v.paused = false;
        if let Some(r) = &v.rec { if let Some(l) = &r.listen { l.send(Ctl::Saying(text.clone())); l.send(Ctl::Pause); } }
        let here = v.rec.as_ref().is_some_and(|r| r.chat == chat);
        Some((chat, text, v.say_seq, v.settings.get("ttsVoice").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            v.settings.get("deepgramLimitReached").and_then(|x| x.as_bool()).unwrap_or(false),
            v.settings.get("speechRate").and_then(|x| x.as_f64()).unwrap_or(1.0) as f32, here))
    }) else { return };
    if !here { line(cx, &chat, "speaking", "Speaking…", None); } else { changed(cx, Some(&chat)); }
    // Without a Deepgram voice (or over its limit): the device's own voice.
    if voice.is_empty() || limit {
        with(cx, |v| v.on_device = true);
        let _ = device(cx).send(Dev::Say(id, text, rate));
        return;
    }
    with(cx, |v| v.on_device = false);
    let out = speaker(cx);
    let Some(api) = crundi_shell::api(cx) else { return reply_done(cx, id) };
    let _ = out.send(Out::Begin(id, 24_000));
    crundi_shell::run(cx, move || {
        let mut carry: Vec<u8> = vec![];
        let o2 = out.clone();
        let r = api.post_stream("/api/voice/speak?stream=1", &json!({ "text": text }), "audio/L16", &mut |ct, xr| {
            let _ = o2.send(Out::Begin(id, rules::speak_rate(xr, ct)));
        }, &mut |b| {
            carry.extend_from_slice(b);
            let n = carry.len() / 2 * 2;
            let s: Vec<i16> = carry[..n].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            carry.drain(..n);
            out.send(Out::Pcm(id, s)).is_ok()
        });
        let _ = out.send(Out::End(id));
        matches!(r, Ok((200, _)))
    }, move |ok, cx| if !ok { reply_done(cx, id) });
}

fn reply_done(cx: &mut App, id: u64) {
    let Some(chat) = with(cx, |v| {
        if v.saying.as_ref().is_none_or(|s| s.1 != id) { return None; }
        let (chat, _) = v.saying.take().unwrap();
        v.paused = false;
        v.said += 1;
        Some(chat)
    }) else { return };
    if read(cx, |v| v.lines.get(&chat).is_some_and(|l| l.0 == "speaking" || l.0 == "paused")).unwrap_or(false) { line_clear(cx, &chat); } else { changed(cx, Some(&chat)); }
    super::after(cx, 450, |cx| with(cx, |v| if let Some(r) = &v.rec { if let Some(l) = &r.listen { l.send(Ctl::Resume); l.send(Ctl::Said); } }));
    reply_next(cx);
}

fn reply_pause(cx: &mut App) {
    let on_device = with(cx, |v| { v.paused = true; v.on_device });
    if on_device { let _ = device(cx).send(Dev::Pause); } else { let _ = speaker(cx).send(Out::Pause); }
    let k = read(cx, |v| v.saying.as_ref().map(|s| s.0.clone())).flatten();
    if let Some(k) = k { if read(cx, |v| v.lines.get(&k).is_some_and(|l| l.0 == "speaking")).unwrap_or(false) { line(cx, &k, "paused", "Paused", None); } else { changed(cx, Some(&k)); } }
    resume_later(cx);
}

fn resume_later(cx: &mut App) {
    super::after(cx, rules::QUIET_MS, |cx| {
        let (still, quiet, on_device) = with(cx, |v| (v.paused && v.saying.is_some(), !talking(v), v.on_device));
        if still && quiet {
            with(cx, |v| v.paused = false);
            if on_device { let _ = device(cx).send(Dev::Resume); } else { let _ = speaker(cx).send(Out::Resume); }
            let k = read(cx, |v| v.saying.as_ref().map(|s| s.0.clone())).flatten();
            if let Some(k) = k { if read(cx, |v| v.lines.get(&k).is_some_and(|l| l.0 == "paused")).unwrap_or(false) { line(cx, &k, "speaking", "Speaking…", None); } }
        } else if still { resume_later(cx); }
    });
}

fn reply_hold(cx: &mut App) { if with(cx, |v| v.saying.is_some() && !v.paused) { reply_pause(cx); } }

fn reply_stop(cx: &mut App) {
    let Some(on_device) = with(cx, |v| v.saying.as_ref().map(|_| v.on_device)) else { return };
    if on_device { let _ = device(cx).send(Dev::Stop); } else { let _ = speaker(cx).send(Out::Stop); }
}

fn vline_click(cx: &mut App, chat: &str) {
    if read(cx, |v| v.saying.as_ref().is_some_and(|s| s.0 == chat)).unwrap_or(false) { reply_stop(cx); }
}

// ─── What it draws (the composer's parts) ───

/// This chat is recording (voice mode or a hold here, not paused).
pub fn recording(c: &Composer, cx: &App) -> bool {
    read(cx, |v| v.rec.as_ref().is_some_and(|r| r.chat == c.id) && v.away.as_deref() != Some(c.id.as_str())).unwrap_or(false)
}

fn mic_tip(v: &Voice, chat: &str) -> &'static str {
    let rec = v.rec.as_ref().filter(|r| r.chat == chat);
    if v.saying.is_some() && rec.is_some() { "Pause what Claude is saying" }
    else if let Some(r) = rec {
        if r.hold && r.cancelling { "Let go to cancel" } else if r.hold { "Let go to send (slide to cancel to drop it)" }
        else if !r.interim.is_empty() { "Send what was heard now" } else { "Voice mode is on: tap to turn it off" }
    } else { "Voice input (tap: voice mode, hold: push to talk)" }
}

/// The voice line above the box (notes, errors, "Speaking…"); a click stops
/// what is being said. While a hold runs, the pointer's slide onto × is
/// followed window-wide.
pub fn vline(c: &Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let chat = c.id.clone();
    let (l, holding) = read(cx, |v| (v.lines.get(&chat).cloned(), v.rec.as_ref().is_some_and(|r| r.chat == chat && r.hold && !r.sending)))?;
    let hooks = holding.then(|| canvas(|_, _, _| {}, |_, _, window, _| {
        window.on_mouse_event(|e: &MouseMoveEvent, phase, _, cx| {
            if phase != DispatchPhase::Capture { return; }
            let hit = with(cx, |v| {
                let b = v.x_bounds?;
                let r = v.rec.as_mut().filter(|r| r.hold && !r.sending)?;
                let n = b.dilate(T::SP_8).contains(&e.position);
                (n != r.cancelling).then(|| { r.cancelling = n; r.chat.clone() })
            });
            if let Some(k) = hit { changed(cx, Some(&k)); }
        });
    }).absolute().size_0());
    let Some((kind, text, _)) = l else { return hooks.map(|h| h.into_any_element()) };
    let soft = kind == "speaking" || kind == "paused";
    let tone = if kind == "error" { w::LineTone::Error } else if soft { w::LineTone::Soft } else { w::LineTone::Note };
    Some(w::status_line("vline", text, tone)
        .on_click(move |_, _, cx| vline_click(cx, &chat))
        .children(hooks)
        .into_any_element())
}

/// The mic, if the server can listen: idle, on (recording), away (paused
/// for a hold elsewhere). A press and its release tell a tap from a hold.
pub fn mic(c: &Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let chat = c.id.clone();
    let (ok, on, away, tip) = read(cx, |v| (v.ok.is_some_and(|o| o.0), v.rec.as_ref().is_some_and(|r| r.chat == chat) && v.away.as_deref() != Some(chat.as_str()),
        v.away.as_deref() == Some(chat.as_str()), mic_tip(v, &chat)))?;
    if !ok { return None; }
    let (k1, k2, k3) = (chat.clone(), chat.clone(), chat);
    let up = |k: &str, cx: &mut App| { let over = read(cx, |v| v.rec.as_ref().is_some_and(|r| r.cancelling)).unwrap_or(false); release(cx, k, over); };
    Some(w::square_button("mic", if on { IconName::Send } else { IconName::Mic }, tip,
            if on { w::SquareLook::On } else if away { w::SquareLook::Outline } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
        .on_mouse_down(MouseButton::Left, move |_, _, cx| press(cx, &k1))
        .on_mouse_up(MouseButton::Left, move |_, _, cx| up(&k2, cx))
        .on_mouse_up_out(MouseButton::Left, move |_, _, cx| if read(cx, |v| v.rec.as_ref().is_some_and(|r| r.chat == k3 && r.hold)).unwrap_or(false) { up(&k3, cx) })
        .into_any_element())
}

/// The box's place while recording: the words heard (or "Listening…").
pub fn rec_box(c: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let text = read(cx, |v| v.rec.as_ref().map(|r| if r.hold { rules::box_text(&r.heard, &r.interim) } else { r.interim.clone() })).flatten().unwrap_or_default();
    div().relative().flex_1().min_w_0().min_h(T::CONTROL).max_h(T::COMPOSER_MAX).px(T::SP_10).py(T::SP_8)
        .rounded(T::R_CONTROL).border_1().border_color(T::accent()).bg(T::accent_dim()).text_size(T::TEXT_13 * c.fs())
        .child(if text.is_empty() { div().italic().text_color(T::text_secondary()).child("Listening…") } else { div().text_color(T::text()).child(text) })
        .into_any_element()
}

/// The recording strip in Send's place: a blinking dot, five level bars,
/// the time and the status; red while a hold is over ×.
pub fn strip(_c: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let (secs, level, status, red) = read(cx, |v| v.rec.as_ref().map(|r| {
        let speaking = v.saying.is_some();
        let st = if r.sending { "Sending" } else if speaking && v.paused { "Paused" } else if speaking && !r.hold { "Speaking" } else { "Listening" };
        (r.started.elapsed().as_secs_f64(), r.level, st, r.cancelling)
    })).flatten().unwrap_or((0.0, 0.0, "Listening", false));
    let ink = if red { T::red() } else { T::white() };
    let lit = (secs * 2.0) as u64 % 2 == 0;
    let mut bars = div().flex().items_center().gap(T::SP_2);
    for b in rules::bars(level, secs as f32) { bars = bars.child(div().w(T::SP_2).h(px(b)).rounded(T::R_CHIP).bg(ink)); }
    div().flex().flex_none().items_center().gap(T::SP_6).h(T::CONTROL).px(T::SP_12).rounded(T::R_CONTROL)
        .map(|d| if red { d.bg(T::red_dim()).border_1().border_color(T::red()) } else { d.bg(T::accent()) })
        .text_color(ink).text_size(T::TEXT_11)
        .child(div().size(T::SP_6).rounded_full().bg(ink).when(!lit, |d| d.opacity(T::MEDIA_O_DIM)))
        .child(bars)
        .child(div().font_weight(T::W_SEMIBOLD).child(rules::fmt_time(secs)))
        .child(div().font_weight(T::W_MEDIUM).whitespace_nowrap().child(status))
        .into_any_element()
}

/// The cancel ×: solid red while a hold is over it.
pub fn cancel_x(c: &Composer, cx: &mut Context<Composer>) -> AnyElement {
    let hot = read(cx, |v| v.rec.as_ref().is_some_and(|r| r.cancelling)).unwrap_or(false);
    let chat = c.id.clone();
    w::square_button("rec-x", IconName::X, "Cancel: drop what was heard (when holding, slide here and let go)", if hot { w::SquareLook::Hot } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
        .relative()
        .on_click(move |_, _, cx| cancel(cx, &chat))
        .child(canvas(|b, _, cx| { with(cx, |v| v.x_bounds = Some(b)); }, |_, _, _, _| {}).absolute().size_full())
        .into_any_element()
}

// ─── The test driver ───

/// The composer's: mic-tap, mic-press, mic-release [over], voice-cancel,
/// vline-click, voice-say <id> <text> (as the server's), voice-escape.
pub fn test_ui(c: &mut Composer, name: &str, args: &[String], _window: &mut Window, cx: &mut Context<Composer>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let chat = c.id.clone();
    match name {
        "mic-press" => press(cx, &chat),
        "mic-release" => release(cx, &chat, a(0) == "over"),
        "mic-tap" => { press(cx, &chat); release(cx, &chat, false); }
        "voice-cancel" => cancel(cx, &chat),
        "voice-escape" => { escape(cx, &chat); }
        "vline-click" => vline_click(cx, &chat),
        "voice-say" => say(cx, &chat, &a(0), &a(1)),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

fn test_ui_app(name: &str, _args: &[String], cx: &mut App) -> Option<Value> {
    match name {
        "voice-recheck" => { with(cx, |v| v.ok = None); check(cx); Some(json!({ "ok": true })) }
        _ => None,
    }
}

pub fn state(c: &Composer, cx: &App) -> Value {
    let chat = &c.id;
    let (mic, l) = read(cx, |v| (v.ok.is_some_and(|o| o.0), v.lines.get(chat).cloned())).unwrap_or((false, None));
    json!({ "mic": mic, "recording": recording(c, cx), "vline": l.as_ref().map(|l| l.1.clone()).unwrap_or_default(), "vlineKind": l.map(|l| l.0).unwrap_or_default() })
}

/// The driver's `voice`: the client-wide state.
fn state_app(cx: &mut App) -> Value {
    let Some(v) = read(cx, |v| {
        let r = v.rec.as_ref();
        json!({ "ok": v.ok.map(|o| o.0), "cap": v.cap, "away": v.away, "tones": v.tones, "said": v.said, "queue": v.queue.len(), "lastSent": v.last_sent,
            "saying": v.saying.as_ref().map(|s| s.0.clone()), "paused": v.paused, "deviceVoiced": v.device_voiced,
            "rec": r.map(|r| json!({ "chat": r.chat, "hold": r.hold, "interim": r.interim, "heard": r.heard, "sending": r.sending, "cancelling": r.cancelling })) })
    }) else { return json!({}) };
    json!({ "voice": v })
}
