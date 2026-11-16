//! 2.24: voice input (specs/voice-input.md; the Slint client's src/app.rs
//! "Voice input" and ui/chat.slint's MicBtn, RecStrip, CancelX, RecBox,
//! VoiceLine). The mic beside the box: a tap is voice mode (every utterance
//! goes as "voice input: …"; a tap with words in progress sends them now; a
//! tap with none ends it), a hold (350 ms) is push to talk (let go: what was
//! heard goes as one; slid onto × and let go: dropped). One recording in the
//! whole client: a hold in another chat pauses voice mode, voice mode in
//! another chat moves it. While recording the box shows the words (the
//! draft waits under them) and Send's place the strip (dot, level bars,
//! time, status) and ×. The voice line above the box says notes, errors and
//! "Speaking…". Claude's replies (voice-say) are said in voice mode or within
//! 10 minutes of a hold, each once, queued. The rules are crundi_core::voice,
//! the audio crundi_core::voiceio (the test's mic: CRUNDI_TEST_MIC_WAV).

use crate::attach::after;
use crate::ComposerView;
use crundi_core::voice::{self as rules, Msg};
use crundi_core::voiceio::{self as io, Ctl, Dev, Ev, Out};
use crundi_shell::{Area, AreaEvent};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Per composer: nothing (voice is window-wide, in `Vo`).
#[derive(Default)]
pub struct State {}

struct Rec {
    key: String,
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

/// What the threads tell the UI thread.
enum In { Ev(u64, Ev), Done(u64), DevDone(u64, bool) }

pub struct Vo {
    cap: bool,
    ok: Option<(bool, Instant)>,
    asking: bool,
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
    device_says: u32,
    device_voiced: Option<bool>,
    tones: u32,
    said: u32,
    last_sent: String,
    tx: mpsc::Sender<In>,
    rx: mpsc::Receiver<In>,
    /// Where × is (the hold's slide) and the drawn widths (F127).
    x_bounds: Option<Bounds<Pixels>>,
    box_w: f32,
    strip_w: f32,
}
impl Global for Vo {}

fn vo(cx: &mut App) -> &mut Vo {
    if !cx.has_global::<Vo>() {
        let (tx, rx) = mpsc::channel();
        cx.set_global(Vo {
            cap: false, ok: None, asking: false, settings: Value::Null, rec: None, away: None, lines: HashMap::new(), line_gen: 0,
            ptt_at: HashMap::new(), press: None, press_gen: 0, rgen: 0, seen: HashSet::new(), queue: VecDeque::new(), saying: None,
            paused: false, say_seq: 0, speaker: None, device: None, on_device: false, device_says: 0, device_voiced: None,
            tones: 0, said: 0, last_sent: String::new(), tx, rx, x_bounds: None, box_w: 0.0, strip_w: 0.0,
        });
    }
    cx.global_mut::<Vo>()
}
fn vor(cx: &App) -> Option<&Vo> { cx.try_global::<Vo>() }

fn chat_of(key: &str) -> Option<&str> { key.strip_prefix("live:") }
fn key_of(chat: &str) -> String { format!("live:{chat}") }
fn exited(cx: &App, chat: &str) -> bool { crundi_chat_model::find(cx, chat).is_some_and(|m| m.read(cx).conv.state == "exited") }

// ─── Start: the area (its driver actions, props, the live link) and the pump ───

pub fn init(cx: &mut App) {
    let _ = vo(cx);
    // The threads' news, drained on the UI thread; the strip redrawn while recording.
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(Duration::from_millis(33)).await;
        cx.update(pump);
    }).detach();
    // Esc in the recording chat, wherever the focus is (before the bindings).
    cx.intercept_keystrokes(|e, _, cx| if e.keystroke.key == "escape" && escape(cx) { cx.stop_propagation(); }).detach();
    crundi_shell::register(cx, Area::new("voice")
        .on_signed_in(|cx| { vo(cx).ok = None; check(cx); })
        .on_event(|e, cx| if let AreaEvent::Chat(m) = e { chat_message(cx, m) })
        .test_ui(|name, args, _, cx| test_ui_app(name, args, cx))
        .props(|_, cx| props_app(cx)));
}

fn pump(cx: &mut App) {
    let mut got = vec![];
    if let Some(v) = vor(cx) { while let Ok(m) = v.rx.try_recv() { got.push(m); } }
    let busy = !got.is_empty() || vor(cx).is_some_and(|v| v.rec.is_some() || v.saying.is_some());
    for m in got {
        match m {
            In::Ev(g, e) => on_ev(cx, g, e),
            In::Done(id) => reply_done(cx, id),
            In::DevDone(id, voiced) => { vo(cx).device_voiced = Some(voiced); reply_done(cx, id); }
        }
    }
    if busy { cx.refresh_windows(); }
}

fn chat_message(cx: &mut App, m: &Value) {
    if m.get("type").and_then(|t| t.as_str()) != Some("ui-event") { return; }
    let chat = m.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
    let Some(ev) = m.get("event") else { return };
    match ev.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "voice-say" => {
            let (id, text) = (ev.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(), ev.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string());
            say(cx, &chat, &id, &text);
        }
        "exit" => chat_gone(cx, &chat),
        _ => {}
    }
}

/// Whether the mic shows: the server lists chat-voice and has a Deepgram
/// key (asked once a minute at most).
pub fn check(cx: &mut App) {
    let v = vo(cx);
    if v.asking || v.ok.is_some_and(|o| o.1.elapsed() < Duration::from_secs(60)) { return; }
    let Some(api) = crundi_shell::api(cx) else { return };
    vo(cx).asking = true;
    crundi_shell::run(cx, move || {
        let caps = crundi_core::api::handshake(api.base()).ok().and_then(|h| h.pointer("/server/capabilities").cloned()).unwrap_or(Value::Null);
        let cap = caps.as_array().is_some_and(|c| c.iter().any(|x| x == "chat-voice"));
        let r = api.get("/api/voice/settings");
        (cap, r)
    }, |(cap, r), cx| {
        let (st, s) = match r { Ok(v) => (200, v), Err(crundi_core::api::ApiError::Status(c, _)) => (c, Value::Null), Err(_) => (0, Value::Null) };
        let v = vo(cx);
        v.asking = false;
        v.cap = cap;
        v.ok = Some((rules::available(st, &s, cap), Instant::now()));
        if st == 200 { v.settings = s; }
        cx.refresh_windows();
    });
}

// ─── The voice line ───

fn line(cx: &mut App, key: &str, kind: &str, text: &str, for_ms: Option<u64>) {
    let v = vo(cx);
    v.line_gen += 1;
    let g = v.line_gen;
    v.lines.insert(key.to_string(), (kind.to_string(), text.to_string(), g));
    cx.refresh_windows();
    if let Some(ms) = for_ms {
        let k = key.to_string();
        after(cx, ms, move |cx| {
            let v = vo(cx);
            if v.lines.get(&k).is_some_and(|l| l.2 == g) && v.rec.as_ref().is_none_or(|r| r.key != k) { v.lines.remove(&k); cx.refresh_windows(); }
        });
    }
}
fn line_clear(cx: &mut App, key: &str) { if vo(cx).lines.remove(key).is_some() { cx.refresh_windows(); } }

// ─── The mic ───

/// Pressed: a tap or a hold, told apart at HOLD_MS. In voice mode any press is a tap.
pub fn press(cx: &mut App, key: &str) {
    let v = vo(cx);
    let in_voice = v.rec.as_ref().is_some_and(|r| r.key == key && !r.hold);
    v.press_gen += 1;
    let g = v.press_gen;
    v.press = Some((key.to_string(), g));
    if in_voice { return; }
    let k = key.to_string();
    after(cx, rules::HOLD_MS, move |cx| {
        let v = vo(cx);
        if v.press.as_ref().is_some_and(|p| p.0 == k && p.1 == g) { v.press = None; hold_start(cx, &k); }
    });
}

pub fn release(cx: &mut App, key: &str, over_x: bool) {
    let v = vo(cx);
    if v.rec.as_ref().is_some_and(|r| r.key == key && r.hold && !r.sending) { return if over_x { hold_cancel(cx) } else { hold_release(cx) }; }
    if v.press.take().is_some_and(|p| p.0 == key) { tap(cx, key); }
}

fn near(cx: &mut App, key: &str, on: bool) {
    if let Some(r) = vo(cx).rec.as_mut() { if r.key == key && r.hold { r.cancelling = on; } }
    cx.refresh_windows();
}

fn tap(cx: &mut App, key: &str) {
    let v = vo(cx);
    if v.saying.is_some() && !v.paused { return reply_pause(cx); }
    match v.rec.as_ref().filter(|r| r.key == key && !r.hold).map(|r| r.interim.clone()) {
        Some(words) if !words.trim().is_empty() => {
            if let Some(r) = vo(cx).rec.as_mut() {
                if let Some(l) = &r.listen { l.send(Ctl::Discard); }
                r.sent_now = Some((words.trim().to_string(), Instant::now()));
                r.interim.clear();
                r.started = Instant::now();
            }
            send(cx, key, &words);
        }
        Some(_) => stop(cx, key),
        None => start(cx, key),
    }
}

fn start(cx: &mut App, key: &str) {
    let Some(chat) = chat_of(key).map(str::to_string) else { return };
    if exited(cx, &chat) { return line(cx, key, "error", rules::ERR_ENDED, Some(rules::LINE_MS)); }
    let v = vo(cx);
    let mut moved = vec![];
    if let Some(r) = v.rec.take() { if r.key != key { moved.push(r.key.clone()); } }
    if let Some(k) = v.away.take() { if k != key { moved.push(k); } }
    for k in moved { line(cx, &k, "note", rules::NOTE_MOVED, Some(rules::LINE_MS)); }
    rec_start(cx, key, &chat, false);
}

fn rec_start(cx: &mut App, key: &str, chat: &str, hold: bool) {
    let Some(api) = crundi_shell::api(cx) else { return };
    let v = vo(cx);
    v.rgen += 1;
    let (rgen, tx) = (v.rgen, v.tx.clone());
    let base = api.base().to_string();
    let listen = io::Listen::start(move || api.fresh_access().ok().map(|t| rules::listen_url(&base, &t)), move |ev| { let _ = tx.send(In::Ev(rgen, ev)); });
    let v = vo(cx);
    v.lines.remove(key);
    v.rec = Some(Rec { key: key.into(), chat: chat.into(), hold, listen: Some(listen), interim: String::new(), heard: vec![], started: Instant::now(),
        level: 0.0, sending: false, cancelling: false, sent_now: None, last_heard: None, rgen });
    if hold { reply_hold(cx); }
    cx.refresh_windows();
}

fn on_ev(cx: &mut App, rgen: u64, ev: Ev) {
    if !vo(cx).rec.as_ref().is_some_and(|r| r.rgen == rgen) { return; }
    match ev {
        Ev::Level(l) => { if let Some(r) = vo(cx).rec.as_mut() { r.level = l; } }
        Ev::Msg(Msg::Ready) => tone(cx, false),
        Ev::Msg(Msg::Interim(t)) => { if let Some(r) = vo(cx).rec.as_mut() { r.interim = t; r.last_heard = Some(Instant::now()); } }
        Ev::Msg(Msg::Utterance(t)) => {
            let k = (|| {
                let r = vo(cx).rec.as_mut()?;
                r.last_heard = Some(Instant::now());
                r.interim.clear();
                if r.hold { r.heard.push(t.clone()); return None; }
                if r.sent_now.as_ref().is_some_and(|(w, at)| at.elapsed() < Duration::from_millis(rules::ALREADY_SENT_MS) && w.split_whitespace().eq(t.split_whitespace())) { r.sent_now = None; return None; }
                r.started = Instant::now();
                Some(r.key.clone())
            })();
            if let Some(k) = k { send(cx, &k, &t); }
        }
        Ev::Msg(Msg::Discard) => { if let Some(r) = vo(cx).rec.as_mut() { r.interim.clear(); r.heard.clear(); } }
        Ev::Msg(Msg::Error(e)) => fail(cx, &e),
        Ev::Msg(Msg::Other) => {}
        Ev::Closed => {
            if vo(cx).rec.as_ref().is_some_and(|r| r.hold && r.sending) { hold_finish(cx, rgen); } else { fail(cx, rules::ERR_CLOSED); }
        }
        Ev::NoMic => fail(cx, rules::ERR_MIC),
    }
}

/// Words sent as "voice input: …" through the normal send; the draft stays; the sent tone.
fn send(cx: &mut App, key: &str, words: &str) {
    let Some(text) = rules::voice_text(words) else { return };
    let Some(chat) = chat_of(key).map(str::to_string) else { return };
    if exited(cx, &chat) { return; }
    vo(cx).last_sent = text.clone();
    if let Some(m) = crundi_chat_model::find(cx, &chat) { m.update(cx, |m, cx| m.send(text, cx)); }
    tone(cx, true);
}

fn fail(cx: &mut App, e: &str) {
    let Some(key) = vo(cx).rec.as_ref().map(|r| r.key.clone()) else { return };
    rec_end(cx);
    line(cx, &key, "error", e, Some(rules::LINE_MS));
}

fn rec_end(cx: &mut App) {
    let was_hold = vo(cx).rec.take().is_some_and(|r| r.hold);
    cx.refresh_windows();
    if was_hold { away_resume(cx); }
}

fn away_resume(cx: &mut App) {
    let Some(k) = vo(cx).away.take() else { return };
    line_clear(cx, &k);
    if let Some(chat) = chat_of(&k).map(str::to_string) { if !exited(cx, &chat) { rec_start(cx, &k, &chat, false); } }
}

fn stop(cx: &mut App, key: &str) {
    if vo(cx).rec.as_ref().is_some_and(|r| r.key == key) { rec_end(cx); }
    reply_stop(cx);
    line_clear(cx, key);
}

fn hold_start(cx: &mut App, key: &str) {
    let Some(chat) = chat_of(key).map(str::to_string) else { return };
    if exited(cx, &chat) { return line(cx, key, "error", rules::ERR_ENDED, Some(rules::LINE_MS)); }
    let paused = vo(cx).rec.take().filter(|r| r.key != key).map(|r| r.key);
    if let Some(k) = paused { vo(cx).away = Some(k.clone()); line(cx, &k, "away", rules::NOTE_PAUSED, None); }
    rec_start(cx, key, &chat, true);
}

fn hold_release(cx: &mut App) {
    let Some(rgen) = vo(cx).rec.as_mut().map(|r| { r.sending = true; r.cancelling = false; if let Some(l) = &r.listen { l.send(Ctl::Stop); } r.rgen }) else { return };
    after(cx, rules::SEND_WAIT_MS, move |cx| hold_finish(cx, rgen));
}

fn hold_finish(cx: &mut App, rgen: u64) {
    let v = vo(cx);
    let Some(r) = v.rec.as_ref().filter(|r| r.rgen == rgen && r.hold) else { return };
    let words = rules::ptt_words(&r.heard, &r.interim);
    let (key, chat) = (r.key.clone(), r.chat.clone());
    v.ptt_at.insert(chat, Instant::now());
    rec_end(cx);
    if !words.is_empty() { send(cx, &key, &words); }
}

fn hold_cancel(cx: &mut App) {
    if let Some(r) = &vo(cx).rec { if let Some(l) = &r.listen { l.send(Ctl::Discard); } }
    rec_end(cx);
}

/// ×: a hold is dropped; in voice mode the words go and the reply stops.
pub fn cancel(cx: &mut App, key: &str) {
    match vo(cx).rec.as_ref().filter(|r| r.key == key).map(|r| r.hold) {
        Some(true) => hold_cancel(cx),
        Some(false) => {
            if let Some(r) = vo(cx).rec.as_mut() { if let Some(l) = &r.listen { l.send(Ctl::Discard); } r.interim.clear(); }
            reply_stop(cx);
            vo(cx).queue.clear();
        }
        None => {}
    }
}

/// Esc in the recording chat ends voice mode or drops a hold.
fn escape(cx: &mut App) -> bool {
    let Some((k, hold)) = vor(cx).and_then(|v| v.rec.as_ref().map(|r| (r.key.clone(), r.hold))) else { return false };
    if key_of(&crate::focused_chat(cx)) != k { return false; }
    if hold { hold_cancel(cx) } else { stop(cx, &k) }
    true
}

/// A chat gone or exited: its recording ends.
fn chat_gone(cx: &mut App, chat: &str) {
    if vo(cx).rec.as_ref().is_some_and(|r| r.chat == chat) { rec_end(cx); }
    let v = vo(cx);
    if v.away.as_deref().and_then(chat_of) == Some(chat) { v.away = None; }
}

// ─── The tones and the replies ───

fn speaker(cx: &mut App) -> mpsc::Sender<Out> {
    let v = vo(cx);
    if v.speaker.is_none() { let tx = v.tx.clone(); v.speaker = Some(io::speaker(move |id| { let _ = tx.send(In::Done(id)); })); }
    v.speaker.clone().unwrap()
}
fn device(cx: &mut App) -> mpsc::Sender<Dev> {
    let v = vo(cx);
    if v.device.is_none() { let tx = v.tx.clone(); v.device = Some(io::device_voice(move |id, voiced| { let _ = tx.send(In::DevDone(id, voiced)); })); }
    v.device.clone().unwrap()
}
fn tone(cx: &mut App, sent: bool) { vo(cx).tones += 1; let _ = speaker(cx).send(Out::Tone(sent)); }

/// A reply Claude gave a voice input: said in voice mode, or within the
/// window after a hold; each id once; they queue.
pub fn say(cx: &mut App, chat: &str, id: &str, text: &str) {
    let v = vo(cx);
    if !id.is_empty() && !v.seen.insert(id.to_string()) { return; }
    let voice_mode = v.rec.as_ref().is_some_and(|r| r.chat == chat && !r.hold) || v.away.as_deref().and_then(chat_of) == Some(chat);
    let since = v.ptt_at.get(chat).map(|t| t.elapsed().as_millis() as u64);
    if !rules::should_speak(voice_mode, since.map(|_| 0), since.unwrap_or(0)) { return; }
    v.queue.push_back((chat.to_string(), text.to_string()));
    reply_next(cx);
}

fn talking(v: &Vo) -> bool {
    v.rec.as_ref().is_some_and(|r| r.hold || r.sending || r.last_heard.is_some_and(|t| t.elapsed() < Duration::from_millis(rules::QUIET_MS)))
}

fn reply_next(cx: &mut App) {
    let v = vo(cx);
    if v.saying.is_some() { return; }
    if talking(v) { if !v.queue.is_empty() { after(cx, 250, reply_next); } return; }
    let Some((chat, text)) = v.queue.pop_front() else { return };
    v.say_seq += 1;
    let id = v.say_seq;
    v.saying = Some((chat.clone(), id));
    v.paused = false;
    let voice = v.settings.get("ttsVoice").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let limit = v.settings.get("deepgramLimitReached").and_then(|x| x.as_bool()).unwrap_or(false);
    let rate = v.settings.get("speechRate").and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
    if let Some(r) = &v.rec { if let Some(l) = &r.listen { l.send(Ctl::Saying(text.clone())); l.send(Ctl::Pause); } }
    if !v.rec.as_ref().is_some_and(|r| r.chat == chat) { line(cx, &key_of(&chat), "speaking", "Speaking…", None); }
    // Without a Deepgram voice (or over its limit): the device's own voice.
    if voice.is_empty() || limit {
        let v = vo(cx);
        v.on_device = true;
        v.device_says += 1;
        let _ = device(cx).send(Dev::Say(id, text, rate));
        return;
    }
    vo(cx).on_device = false;
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
    let v = vo(cx);
    if v.saying.as_ref().is_none_or(|s| s.1 != id) { return; }
    let (chat, _) = v.saying.take().unwrap();
    v.paused = false;
    v.said += 1;
    let k = key_of(&chat);
    if v.lines.get(&k).is_some_and(|l| l.0 == "speaking" || l.0 == "paused") { line_clear(cx, &k); }
    after(cx, 450, |cx| { if let Some(r) = &vo(cx).rec { if let Some(l) = &r.listen { l.send(Ctl::Resume); l.send(Ctl::Said); } } });
    reply_next(cx);
}

fn reply_pause(cx: &mut App) {
    vo(cx).paused = true;
    if vo(cx).on_device { let _ = device(cx).send(Dev::Pause); } else { let _ = speaker(cx).send(Out::Pause); }
    let k = vo(cx).saying.as_ref().map(|s| key_of(&s.0));
    if let Some(k) = k { if vo(cx).lines.get(&k).is_some_and(|l| l.0 == "speaking") { line(cx, &k, "paused", "Paused", None); } }
    resume_later(cx);
}

fn resume_later(cx: &mut App) {
    after(cx, rules::QUIET_MS, |cx| {
        let v = vo(cx);
        let still = v.paused && v.saying.is_some();
        if still && !talking(v) {
            v.paused = false;
            if v.on_device { let _ = device(cx).send(Dev::Resume); } else { let _ = speaker(cx).send(Out::Resume); }
            let k = vo(cx).saying.as_ref().map(|s| key_of(&s.0));
            if let Some(k) = k { if vo(cx).lines.get(&k).is_some_and(|l| l.0 == "paused") { line(cx, &k, "speaking", "Speaking…", None); } }
        } else if still { resume_later(cx); }
    });
}

fn reply_hold(cx: &mut App) { let v = vo(cx); if v.saying.is_some() && !v.paused { reply_pause(cx); } }

fn reply_stop(cx: &mut App) {
    if vo(cx).saying.is_none() { return; }
    if vo(cx).on_device { let _ = device(cx).send(Dev::Stop); } else { let _ = speaker(cx).send(Out::Stop); }
}

fn vline_click(cx: &mut App, key: &str) {
    if vo(cx).saying.as_ref().is_some_and(|s| key_of(&s.0) == key) { reply_stop(cx); }
}

// ─── What it draws ───

fn my_key(c: &ComposerView, cx: &App) -> String { key_of(&c.chat_id(cx)) }

/// This chat is recording (voice mode or a hold here, not paused).
pub fn recording(c: &ComposerView, cx: &App) -> bool {
    let k = my_key(c, cx);
    vor(cx).is_some_and(|v| v.rec.as_ref().is_some_and(|r| r.key == k) && v.away.as_deref() != Some(k.as_str()))
}

fn mic_tip(v: &Vo, k: &str) -> &'static str {
    let rec = v.rec.as_ref().filter(|r| r.key == k);
    if v.saying.is_some() && rec.is_some() { "Pause what Claude is saying" }
    else if let Some(r) = rec {
        if r.hold && r.cancelling { "Let go to cancel" } else if r.hold { "Let go to send (slide to cancel to drop it)" }
        else if !r.interim.is_empty() { "Send what was heard now" } else { "Voice mode is on: tap to turn it off" }
    } else { "Voice input (tap: voice mode, hold: push to talk)" }
}

/// The voice line above the box, and (out of the flow) the hold's slide onto ×.
pub fn render(c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> Option<AnyElement> {
    check(cx);
    let k = my_key(c, cx);
    let hooks = canvas(|_, _, _| {}, |_, _, window, _| {
        window.on_mouse_event(|e: &MouseMoveEvent, phase, _, cx| {
            if phase != DispatchPhase::Capture { return; }
            let v = vo(cx);
            let Some(b) = v.x_bounds else { return };
            let Some(r) = v.rec.as_mut().filter(|r| r.hold && !r.sending) else { return };
            let n = b.dilate(T::SP_8).contains(&e.position);
            if n != r.cancelling { r.cancelling = n; cx.refresh_windows(); }
        });
    }).absolute().size_0();
    let line = vor(cx).and_then(|v| v.lines.get(&k).cloned());
    let Some((kind, text, _)) = line else { return Some(hooks.into_any_element()) };
    let soft = kind == "speaking" || kind == "paused";
    let key = k.clone();
    Some(div().id("vline").relative().flex().items_center().gap(T::SP_6).px(T::SP_2).cursor_pointer()
        .on_click(move |_, _, cx| vline_click(cx, &key))
        .when(kind != "error", |d| d.child(w::dot(if soft { T::sky() } else { T::yellow() })))
        .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().text_size(T::TEXT_12)
            .when(soft, |d| d.italic())
            .text_color(if kind == "error" { T::red() } else if soft { T::text_muted() } else { T::text_secondary() })
            .child(text))
        .child(hooks)
        .into_any_element())
}

/// The mic (.cc-mic), if the server can listen: idle, on (recording), away
/// (paused for a hold elsewhere). A press and its release go to `press` and
/// `release` (over × when the hold slid onto it).
pub fn mic(c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> Option<AnyElement> {
    let k = my_key(c, cx);
    let v = vor(cx)?;
    if !v.ok.is_some_and(|o| o.0) { return None; }
    let on = v.rec.as_ref().is_some_and(|r| r.key == k) && v.away.as_deref() != Some(k.as_str());
    let away = v.away.as_deref() == Some(k.as_str());
    let tip = mic_tip(v, &k);
    let (k1, k2, k3) = (k.clone(), k.clone(), k.clone());
    let up = move |k: &str, cx: &mut App| { let over = vo(cx).rec.as_ref().is_some_and(|r| r.cancelling); release(cx, k, over); };
    let up2 = up;
    Some(w::square_button("mic", if on { IconName::Send } else { IconName::Mic }, tip,
            if on { w::SquareLook::On } else if away { w::SquareLook::Outline } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
        .on_mouse_down(MouseButton::Left, move |_, _, cx| press(cx, &k1))
        .on_mouse_up(MouseButton::Left, move |_, _, cx| up(&k2, cx))
        .on_mouse_up_out(MouseButton::Left, move |_, _, cx| if vo(cx).rec.as_ref().is_some_and(|r| r.key == k3 && r.hold) { up2(&k3, cx) })
        .into_any_element())
}

/// The box's place while recording: the words heard (or "Listening…").
pub fn rec_box(_c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> AnyElement {
    let text = vor(cx).and_then(|v| v.rec.as_ref().map(|r| if r.hold { rules::box_text(&r.heard, &r.interim) } else { r.interim.clone() })).unwrap_or_default();
    div().relative().flex_1().min_h(T::CONTROL).max_h(T::COMPOSER_MAX).px(T::SP_10).py(T::SP_8)
        .rounded(T::R_CONTROL).border_1().border_color(T::accent()).bg(T::accent_dim()).text_size(T::TEXT_13)
        .child(if text.is_empty() { div().italic().text_color(T::text_secondary()).child("Listening…") } else { div().text_color(T::text()).child(text) })
        .child(canvas(|b, _, cx| vo(cx).box_w = f32::from(b.size.width), |_, _, _, _| {}).absolute().size_full())
        .into_any_element()
}

/// The recording strip (.cc-rec) in Send's place: a blinking dot, five
/// level bars, the time and the status; red while a hold is over ×.
pub fn strip(_c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> AnyElement {
    let (secs, level, status, red) = vor(cx).and_then(|v| v.rec.as_ref().map(|r| {
        let speaking = v.saying.is_some();
        let st = if r.sending { "Sending" } else if speaking && v.paused { "Paused" } else if speaking && !r.hold { "Speaking" } else { "Listening" };
        (r.started.elapsed().as_secs_f64(), r.level, st, r.cancelling)
    })).unwrap_or((0.0, 0.0, "Listening", false));
    let ink = if red { T::red() } else { T::white() };
    let lit = (secs * 2.0) as u64 % 2 == 0;
    let mut bars = div().flex().items_center().gap(T::SP_2);
    for b in rules::bars(level, secs as f32) { bars = bars.child(div().w(T::SP_2).h(px(b)).rounded(T::R_CHIP).bg(ink)); }
    div().relative().flex().flex_none().items_center().gap(T::SP_6).h(T::CONTROL).px(T::SP_12).rounded(T::R_CONTROL)
        .map(|d| if red { d.bg(T::red_dim()).border_1().border_color(T::red()) } else { d.bg(T::accent()) })
        .text_color(ink).text_size(T::TEXT_11)
        .child(div().size(T::SP_6).rounded_full().bg(ink).when(!lit, |d| d.opacity(T::MEDIA_O_DIM)))
        .child(bars)
        .child(div().font_weight(T::W_SEMIBOLD).child(rules::fmt_time(secs)))
        .child(div().font_weight(T::W_MEDIUM).whitespace_nowrap().child(status))
        .child(canvas(|b, _, cx| vo(cx).strip_w = f32::from(b.size.width), |_, _, _, _| {}).absolute().size_full())
        .into_any_element()
}

/// The cancel × (.cc-rec-x): solid red while a hold is over it.
pub fn cancel_x(c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> AnyElement {
    let hot = vor(cx).is_some_and(|v| v.rec.as_ref().is_some_and(|r| r.cancelling));
    let k = my_key(c, cx);
    w::square_button("rec-x", IconName::X, "Cancel: drop what was heard (when holding, slide here and let go)", if hot { w::SquareLook::Hot } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
        .relative()
        .on_click(move |_, _, cx| cancel(cx, &k))
        .child(canvas(|b, _, cx| vo(cx).x_bounds = Some(b), |_, _, _, _| {}).absolute().size_full())
        .into_any_element()
}

// ─── The test driver ───

/// The composer's routing: nothing here (the voice area answers by key).
pub fn test_ui(_c: &mut ComposerView, _name: &str, _args: &[String], _window: &mut Window, _cx: &mut Context<ComposerView>) -> Option<Value> { None }

fn test_ui_app(name: &str, args: &[String], cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "voice-recheck" => { vo(cx).ok = None; check(cx); }
        "mic-press" => press(cx, &a(0)),
        "mic-release" => release(cx, &a(0), a(1) == "over"),
        "mic-tap" => { let k = a(0); press(cx, &k); release(cx, &k, false); }
        "mic-near" => near(cx, &a(0), a(1) != "off"),
        "voice-cancel" => cancel(cx, &a(0)),
        "vline-click" => vline_click(cx, &a(0)),
        // A voice-say played into the focused chat (other events: the chats part's).
        "chat-event" => {
            let ev: Value = serde_json::from_str(&a(0)).unwrap_or(Value::Null);
            if ev.get("type").and_then(|t| t.as_str()) != Some("voice-say") { return None; }
            let chat = crate::focused_chat(cx);
            chat_message(cx, &json!({ "type": "ui-event", "id": chat, "event": ev }));
        }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

pub fn pane_props(c: &ComposerView, cx: &App) -> Value {
    let k = my_key(c, cx);
    let v = vor(cx);
    let l = v.and_then(|v| v.lines.get(&k));
    json!({ "mic": v.is_some_and(|v| v.ok.is_some_and(|o| o.0)), "vline": l.map(|l| l.1.clone()).unwrap_or_default(), "vlineKind": l.map(|l| l.0.clone()).unwrap_or_default() })
}

pub fn props(_c: &ComposerView, _cx: &App) -> Value { json!({}) }

/// props.voice and props.voiceUi (the Slint client's voice_of and VoiceUi).
fn props_app(cx: &mut App) -> Value {
    let Some(v) = vor(cx) else { return json!({}) };
    let lines: Map<String, Value> = v.lines.iter().map(|(k, l)| (k.clone(), json!({ "kind": l.0, "text": l.1 }))).collect();
    let r = v.rec.as_ref();
    let speaking = v.saying.is_some();
    json!({
        "voice": {
            "ok": v.ok.map(|o| o.0), "cap": v.cap,
            "rec": r.map(|r| json!({ "key": r.key, "hold": r.hold, "interim": r.interim, "heard": r.heard, "sending": r.sending, "cancelling": r.cancelling })),
            "away": v.away, "lines": lines, "tones": v.tones, "said": v.said, "saying": v.saying.as_ref().map(|s| s.0.clone()), "paused": v.paused,
            "queue": v.queue.len(), "lastSent": v.last_sent, "deviceSays": v.device_says, "deviceVoiced": v.device_voiced,
        },
        "voiceUi": {
            "key": r.map(|r| r.key.clone()).unwrap_or_default(),
            "text": r.map(|r| if r.hold { rules::box_text(&r.heard, &r.interim) } else { r.interim.clone() }).unwrap_or_default(),
            "time": r.map(|r| rules::fmt_time(r.started.elapsed().as_secs_f64())).unwrap_or_default(),
            "status": r.map(|r| if r.sending { "Sending" } else if speaking && v.paused { "Paused" } else if speaking && !r.hold { "Speaking" } else { "Listening" }).unwrap_or(""),
            "away": v.away.clone().unwrap_or_default(), "cancelling": r.is_some_and(|r| r.cancelling), "speaking": speaking && r.is_some_and(|r| !r.hold),
            "boxW": if r.is_some() { v.box_w } else { 0.0 }, "stripW": if r.is_some() { v.strip_w } else { 0.0 },
        },
    })
}
