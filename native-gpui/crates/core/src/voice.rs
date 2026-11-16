//! Voice input in a chat (2.24, specs/voice-input.md): the rules, as the
//! web's voiceLogic (app/vendor/claude-chat.js 803-924): whether the mic
//! shows, the strip's time and level bars, the listen socket's messages,
//! what is sent, when a reply is said, the tones, and the audio sent
//! (16 kHz mono Int16 LE). Pure.

use serde_json::Value;

/// A press shorter than this is a tap; longer, push to talk.
pub const HOLD_MS: u64 = 350;
/// After a push to talk, replies are said for this long.
pub const PTT_WINDOW_MS: u64 = 10 * 60 * 1000;
/// A reply waits while words were heard this recently.
pub const QUIET_MS: u64 = 1500;
/// Let go: the server's last words are awaited at most this long.
pub const SEND_WAIT_MS: u64 = 2500;
/// "Send now": a late utterance of the same words is ignored this long.
pub const ALREADY_SENT_MS: u64 = 4000;
/// The voice line's errors and notes go after this long.
pub const LINE_MS: u64 = 6000;
/// The sample rate sent to the server.
pub const RATE: u32 = 16_000;
/// Frames kept while the socket opens.
pub const BUFFER_FRAMES: usize = 40;

pub const ERR_ENDED: &str = "This chat has ended.";
pub const ERR_MIC: &str = "The microphone is not allowed. Allow it for Crundi to talk to this chat.";
pub const ERR_CLOSED: &str = "Listening stopped (the connection closed).";
pub const NOTE_MOVED: &str = "Voice mode moved to another chat.";
pub const NOTE_PAUSED: &str = "Paused: listening in another chat";

/// The mic shows: the settings answered 200, not ok:false, with a Deepgram
/// key; and (native only) the server can do chat voice.
pub fn available(status: u16, settings: &Value, chat_voice: bool) -> bool {
    status == 200 && settings.get("ok").and_then(|o| o.as_bool()) != Some(false)
        && settings.get("deepgramKeySet").and_then(|k| k.as_bool()) == Some(true) && chat_voice
}

/// "m:ss".
pub fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A block's level, 0..1: the RMS, square-rooted twice.
pub fn level(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let ms = samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32;
    ms.sqrt().sqrt().min(1.0)
}

/// The five bars' heights in px (3..16), rippling with the time `t` (s).
pub fn bars(level: f32, t: f32) -> [f32; 5] {
    let mut out = [3.0; 5];
    for (i, b) in out.iter_mut().enumerate() {
        let fi = i as f32;
        let wob = 0.55 + 0.45 * (t * (5.0 + fi * 1.7) + fi).sin().abs();
        *b = (3.0 + 13.0 * (level * 2.2).min(1.0) * wob).round().max(3.0);
    }
    out
}

/// What is sent for some words: "voice input: <words>" (whitespace
/// collapsed), or nothing for none.
pub fn voice_text(words: &str) -> Option<String> {
    let w = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if w.is_empty() { None } else { Some(format!("voice input: {w}")) }
}

/// A push to talk let go: the utterances heard, joined; or the last words
/// in progress when none came.
pub fn ptt_words(heard: &[String], interim: &str) -> String {
    let h = heard.iter().map(|x| x.trim()).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
    if h.is_empty() { interim.trim().to_string() } else { h }
}

/// What the box shows while recording: (a hold) what was heard, then the
/// words in progress.
pub fn box_text(heard: &[String], interim: &str) -> String {
    let mut parts: Vec<&str> = heard.iter().map(|x| x.trim()).filter(|x| !x.is_empty()).collect();
    if !interim.trim().is_empty() { parts.push(interim.trim()); }
    parts.join(" ")
}

/// A message from /api/voice/listen.
#[derive(Debug, PartialEq, Clone)]
pub enum Msg {
    Ready,
    Interim(String),
    Utterance(String),
    Discard,
    Error(String),
    /// Cru's, or not ours: ignored.
    Other,
}

pub fn parse(text: &str) -> Msg {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Msg::Other };
    let t = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    match t("type").as_str() {
        "ready" => Msg::Ready,
        "interim" => Msg::Interim(t("text")),
        "utterance" => Msg::Utterance(t("text")),
        "discard" => Msg::Discard,
        "error" => Msg::Error({ let e = t("error"); if e.is_empty() { ERR_CLOSED.to_string() } else { e } }),
        _ => Msg::Other,
    }
}

/// Is a reply said? In voice mode, or within the window after a hold.
pub fn should_speak(voice_mode: bool, last_ptt_ms: Option<u64>, now_ms: u64) -> bool {
    voice_mode || last_ptt_ms.is_some_and(|t| now_ms.saturating_sub(t) < PTT_WINDOW_MS)
}

/// The URL of the listen socket from the server's base.
pub fn listen_url(base: &str, token: &str) -> String {
    let ws = base.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
    format!("{ws}/api/voice/listen?encoding=linear16&sample_rate={RATE}&token={token}")
}

/// Mono samples at any rate down to 16 kHz Int16 LE bytes: the average of
/// each block of rate/16000 samples (the web's box filter). Keeps what is
/// left over for the next call.
#[derive(Default)]
pub struct Down {
    rate: u32,
    acc: f64,
    n: f64,
    pos: f64,
}

impl Down {
    pub fn new(rate: u32) -> Self { Down { rate, ..Default::default() } }
    pub fn push(&mut self, mono: &[f32], out: &mut Vec<u8>) {
        let step = self.rate as f64 / RATE as f64;
        for &x in mono {
            self.acc += x as f64;
            self.n += 1.0;
            self.pos += 1.0;
            if self.pos >= step {
                self.pos -= step;
                let v = (self.acc / self.n).clamp(-1.0, 1.0);
                out.extend_from_slice(&((v * 32767.0).round() as i16).to_le_bytes());
                self.acc = 0.0;
                self.n = 0.0;
            }
        }
    }
}

/// Interleaved samples to mono (the channels' mean).
pub fn mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 { return samples.to_vec(); }
    samples.chunks(channels).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect()
}

/// A PCM16 WAV file: its rate and mono samples (the test hook's audio).
pub fn wav(bytes: &[u8]) -> Option<(u32, Vec<f32>)> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" { return None; }
    let (mut i, mut rate, mut ch, mut bits) = (12usize, 0u32, 0usize, 0u16);
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        let body = bytes.get(i + 8..(i + 8 + len).min(bytes.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            ch = u16::from_le_bytes([body[2], body[3]]) as usize;
            rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            if bits != 16 || ch == 0 { return None; }
            let s: Vec<f32> = body.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
            return Some((rate, mono(&s, ch)));
        }
        i += 8 + len + (len & 1);
    }
    None
}

/// The tones (crundi-voice.js tone()): sine notes (Hz, start s, length s,
/// peak), each from silent with a 12 ms rise, then down to silent.
pub fn tone(sent: bool, rate: u32) -> Vec<f32> {
    let notes: [(f32, f32, f32, f32); 2] = if sent { [(880.0, 0.0, 0.08, 0.13), (659.25, 0.07, 0.14, 0.13)] } else { [(659.25, 0.0, 0.11, 0.16), (880.0, 0.09, 0.16, 0.16)] };
    let end = notes.iter().map(|n| n.1 + n.2).fold(0.0, f32::max);
    let mut out = vec![0.0f32; (end * rate as f32) as usize + 1];
    for (hz, start, len, peak) in notes {
        let (s0, n) = ((start * rate as f32) as usize, (len * rate as f32) as usize);
        for k in 0..n {
            let t = k as f32 / rate as f32;
            // A 12 ms rise, then an exponential fall to (nearly) silent.
            let env = if t < 0.012 { peak * (t / 0.012) } else { peak * (0.001f32 / peak).powf((t - 0.012) / (len - 0.012).max(0.001)) };
            if let Some(o) = out.get_mut(s0 + k) { *o += env * (std::f32::consts::TAU * hz * t).sin(); }
        }
    }
    out
}

/// A streamed reply's sample rate: X-Sample-Rate, or rate= in the content
/// type, else 24000.
pub fn speak_rate(x_rate: Option<&str>, content_type: &str) -> u32 {
    x_rate.and_then(|r| r.trim().parse().ok())
        .or_else(|| content_type.split(';').find_map(|p| p.trim().strip_prefix("rate=").and_then(|r| r.parse().ok())))
        .unwrap_or(24_000)
}

/// A reply in sentences (the device voice says one at a time, so a pause
/// starts again from the sentence it was in).
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        cur.push(*c);
        let end = matches!(c, '.' | '!' | '?' | '\n') && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if end { let t = cur.trim().to_string(); if !t.is_empty() { out.push(t); } cur.clear(); }
    }
    let t = cur.trim().to_string();
    if !t.is_empty() { out.push(t); }
    out
}

/// The longest a device voice may take for a reply (a lost "end" never
/// sticks the queue): 8 s + 120 ms a character.
pub fn say_cap_ms(text: &str) -> u64 { 8000 + 120 * text.chars().count() as u64 }

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn showing_the_mic() {
        assert!(available(200, &json!({ "ok": true, "deepgramKeySet": true }), true));
        assert!(!available(200, &json!({ "ok": true, "deepgramKeySet": true }), false));
        assert!(!available(200, &json!({ "deepgramKeySet": false }), true));
        assert!(!available(404, &json!({}), true));
        assert!(!available(200, &json!({ "ok": false, "deepgramKeySet": true }), true));
    }

    #[test]
    fn strip_and_words() {
        assert_eq!(fmt_time(4.7), "0:04");
        assert_eq!(fmt_time(75.0), "1:15");
        assert_eq!(level(&[]), 0.0);
        assert!((level(&[0.5, -0.5]) - 0.5f32.sqrt()).abs() < 1e-5);
        let b = bars(0.0, 1.0);
        assert!(b.iter().all(|x| *x == 3.0));
        assert!(bars(1.0, 0.3).iter().all(|x| *x >= 3.0 && *x <= 16.0));
        assert_eq!(voice_text("  What  day is it? "), Some("voice input: What day is it?".into()));
        assert_eq!(voice_text("What\n day"), Some("voice input: What day".into()));
        assert_eq!(voice_text("  "), None);
        assert_eq!(ptt_words(&["Make the".into(), "table bold".into()], "sort"), "Make the table bold");
        assert_eq!(ptt_words(&[], " sort it "), "sort it");
        assert_eq!(box_text(&["Make the".into()], "table"), "Make the table");
    }

    #[test]
    fn messages() {
        assert_eq!(parse(r#"{"type":"ready","echoFilter":true}"#), Msg::Ready);
        assert_eq!(parse(r#"{"type":"interim","text":"Could you"}"#), Msg::Interim("Could you".into()));
        assert_eq!(parse(r#"{"type":"utterance","text":"Hi","owner":true}"#), Msg::Utterance("Hi".into()));
        assert_eq!(parse(r#"{"type":"discard","text":"scratch that"}"#), Msg::Discard);
        assert_eq!(parse(r#"{"type":"error","error":"Deepgram refused the key."}"#), Msg::Error("Deepgram refused the key.".into()));
        assert_eq!(parse(r#"{"type":"settings"}"#), Msg::Other);
        assert_eq!(parse("nope"), Msg::Other);
    }

    #[test]
    fn device_voice() {
        assert_eq!(sentences("It is Sunday. Rest well!\nBye"), vec!["It is Sunday.", "Rest well!", "Bye"]);
        assert_eq!(sentences("Version 1.5 is out."), vec!["Version 1.5 is out."]);
        assert!(sentences("  ").is_empty());
        assert_eq!(say_cap_ms("abc"), 8360);
    }

    #[test]
    fn replies_and_urls() {
        assert!(should_speak(true, None, 0));
        assert!(should_speak(false, Some(1000), 1000 + PTT_WINDOW_MS - 1));
        assert!(!should_speak(false, Some(1000), 1000 + PTT_WINDOW_MS));
        assert!(!should_speak(false, None, 5));
        assert_eq!(listen_url("https://h:1", "T"), "wss://h:1/api/voice/listen?encoding=linear16&sample_rate=16000&token=T");
        assert_eq!(speak_rate(Some("16000"), "audio/L16"), 16000);
        assert_eq!(speak_rate(None, "audio/L16; rate=22050"), 22050);
        assert_eq!(speak_rate(None, "audio/L16"), 24000);
    }

    #[test]
    fn audio() {
        // 48 kHz to 16 kHz: every 3 samples averaged into one Int16 LE.
        let mut d = Down::new(48_000);
        let mut out = vec![];
        d.push(&[0.5, 0.5, 0.5, -1.0, -1.0, -1.0, 0.0], &mut out);
        assert_eq!(out.len(), 4);
        assert_eq!(i16::from_le_bytes([out[0], out[1]]), 16384);
        assert_eq!(i16::from_le_bytes([out[2], out[3]]), -32767);
        assert_eq!(mono(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        // A WAV: 16 kHz mono, two samples.
        let mut w = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&[1, 0, 1, 0]);
        w.extend_from_slice(&16000u32.to_le_bytes());
        w.extend_from_slice(&32000u32.to_le_bytes());
        w.extend_from_slice(&[2, 0, 16, 0]);
        w.extend_from_slice(b"data");
        w.extend_from_slice(&4u32.to_le_bytes());
        w.extend_from_slice(&16384i16.to_le_bytes());
        w.extend_from_slice(&(-16384i16).to_le_bytes());
        let (rate, s) = wav(&w).unwrap();
        assert_eq!(rate, 16000);
        assert_eq!(s, vec![0.5, -0.5]);
        let t = tone(false, 16_000);
        assert_eq!(t.len(), (0.25 * 16000.0) as usize + 1);
        assert!(t.iter().all(|x| x.abs() <= 0.33));
    }
}
