//! Voice input's audio (2.24, specs/voice-input.md §5 and §8): the
//! microphone (cpal, or a WAV file for tests: CRUNDI_TEST_MIC_WAV), its
//! audio sent to /api/voice/listen as 16 kHz Int16 LE frames with the
//! socket's messages coming back, and the speaker (rodio) for the tones and
//! Claude's streamed voice. Threads only: what they hear goes to the UI
//! thread through the callbacks.

use crate::voice::{self, Msg};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::{stream::MaybeTlsStream, Message};

/// What a listen session tells the app.
#[derive(Debug, Clone)]
pub enum Ev {
    Msg(Msg),
    /// The level of what the microphone hears now (0..1).
    Level(f32),
    /// The socket closed (not asked for, or after a Stop).
    Closed,
    /// The microphone could not be opened.
    NoMic,
}

/// What the app tells a listen session.
#[derive(Debug, Clone)]
pub enum Ctl {
    /// Let go: flush the last words, then close.
    Stop,
    Discard,
    Pause,
    Resume,
    Saying(String),
    Said,
}

/// One recording: the microphone and its socket, until dropped.
pub struct Listen {
    tx: mpsc::Sender<Ctl>,
    stop: Arc<AtomicBool>,
}

impl Listen {
    /// `url` is worked out on the socket's thread (a fresh token may need
    /// the network); None: the socket "closed".
    pub fn start(url: impl FnOnce() -> Option<String> + Send + 'static, on: impl Fn(Ev) + Send + Sync + 'static) -> Listen {
        let (tx, rx) = mpsc::channel::<Ctl>();
        let stop = Arc::new(AtomicBool::new(false));
        let (atx, arx) = mpsc::channel::<(u32, Vec<f32>)>();
        let on = Arc::new(on);
        let (s1, on1) = (stop.clone(), on.clone());
        thread::spawn(move || capture(atx, s1, move |e| on1(e)));
        let s2 = stop.clone();
        thread::spawn(move || match url() {
            Some(u) => socket(u, rx, arx, s2, move |e| on(e)),
            None => { if !s2.load(Ordering::Relaxed) { on(Ev::Closed); } }
        });
        Listen { tx, stop }
    }
    pub fn send(&self, c: Ctl) { let _ = self.tx.send(c); }
}

impl Drop for Listen {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); }
}

/// The microphone: blocks of mono samples at the device's rate.
fn capture(out: mpsc::Sender<(u32, Vec<f32>)>, stop: Arc<AtomicBool>, on: impl Fn(Ev)) {
    // The test hook: a WAV file, played in real time, then silence.
    if let Some(path) = std::env::var_os("CRUNDI_TEST_MIC_WAV") {
        let Some((rate, s)) = std::fs::read(&path).ok().and_then(|b| voice::wav(&b)) else { return on(Ev::NoMic) };
        let block = (rate as usize / 1000) * 85;
        let mut i = 0;
        while !stop.load(Ordering::Relaxed) {
            let chunk: Vec<f32> = if i < s.len() { s[i..(i + block).min(s.len())].to_vec() } else { vec![0.0; block] };
            i += block;
            if out.send((rate, chunk)).is_err() { return; }
            thread::sleep(Duration::from_millis(85));
        }
        return;
    }
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let Some(dev) = host.default_input_device() else { return on(Ev::NoMic) };
    let Ok(cfg) = dev.default_input_config() else { return on(Ev::NoMic) };
    let (rate, ch) = (cfg.sample_rate().0, cfg.channels() as usize);
    let err = |e| eprintln!("[native] microphone: {e}");
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => { let o = out.clone(); dev.build_input_stream(&cfg.into(), move |d: &[f32], _| { let _ = o.send((rate, voice::mono(d, ch))); }, err, None) }
        cpal::SampleFormat::I16 => { let o = out.clone(); dev.build_input_stream(&cfg.into(), move |d: &[i16], _| { let f: Vec<f32> = d.iter().map(|x| *x as f32 / 32768.0).collect(); let _ = o.send((rate, voice::mono(&f, ch))); }, err, None) }
        cpal::SampleFormat::U16 => { let o = out.clone(); dev.build_input_stream(&cfg.into(), move |d: &[u16], _| { let f: Vec<f32> = d.iter().map(|x| (*x as f32 - 32768.0) / 32768.0).collect(); let _ = o.send((rate, voice::mono(&f, ch))); }, err, None) }
        _ => return on(Ev::NoMic),
    };
    let Ok(stream) = stream else { return on(Ev::NoMic) };
    if stream.play().is_err() { return on(Ev::NoMic); }
    while !stop.load(Ordering::Relaxed) { thread::sleep(Duration::from_millis(50)); }
    drop(stream);
}

/// The socket: frames out (kept while it opens), messages in.
fn socket(url: String, rx: mpsc::Receiver<Ctl>, audio: mpsc::Receiver<(u32, Vec<f32>)>, stop: Arc<AtomicBool>, on: impl Fn(Ev)) {
    let mut down: Option<(u32, voice::Down)> = None;
    let mut frame: Vec<u8> = Vec::new();
    let mut kept: Vec<Vec<u8>> = Vec::new();
    let mut paused = false;
    let mut last_level = Instant::now();
    // Turn what the microphone gave into frames (about 85 ms each).
    let mut take = |frames: &mut Vec<Vec<u8>>, on: &dyn Fn(Ev)| {
        while let Ok((rate, block)) = audio.try_recv() {
            if last_level.elapsed() >= Duration::from_millis(40) { on(Ev::Level(voice::level(&block))); last_level = Instant::now(); }
            let d = match &mut down { Some((r, d)) if *r == rate => d, _ => { down = Some((rate, voice::Down::new(rate))); &mut down.as_mut().unwrap().1 } };
            d.push(&block, &mut frame);
            while frame.len() >= 2730 { frames.push(frame.drain(..2730).collect()); }
        }
    };
    // Connecting: the audio is kept (up to 40 frames).
    let (tx_conn, rx_conn) = mpsc::channel();
    let u = url.clone();
    thread::spawn(move || { let _ = tx_conn.send(tungstenite::connect(u.as_str())); });
    let mut ws = loop {
        if stop.load(Ordering::Relaxed) { return; }
        take(&mut kept, &on);
        if kept.len() > voice::BUFFER_FRAMES { kept.drain(..kept.len() - voice::BUFFER_FRAMES); }
        match rx_conn.recv_timeout(Duration::from_millis(20)) {
            Ok(Ok((ws, _))) => break ws,
            Ok(Err(e)) => { eprintln!("[native] voice: cannot connect: {e}"); if !stop.load(Ordering::Relaxed) { on(Ev::Closed); } return; }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => return,
        }
    };
    match ws.get_mut() {
        MaybeTlsStream::Plain(s) => { let _ = s.set_read_timeout(Some(Duration::from_millis(20))); }
        MaybeTlsStream::Rustls(s) => { let _ = s.sock.set_read_timeout(Some(Duration::from_millis(20))); }
        _ => {}
    }
    for f in kept.drain(..) { let _ = ws.send(Message::binary(f)); }
    let mut out: Vec<Vec<u8>> = Vec::new();
    loop {
        if stop.load(Ordering::Relaxed) { let _ = ws.close(None); let _ = ws.flush(); return; }
        while let Ok(c) = rx.try_recv() {
            let text = match c {
                Ctl::Stop => r#"{"type":"stop"}"#.to_string(),
                Ctl::Discard => r#"{"type":"discard"}"#.to_string(),
                Ctl::Pause => { paused = true; r#"{"type":"pause"}"#.to_string() }
                Ctl::Resume => { paused = false; r#"{"type":"resume"}"#.to_string() }
                Ctl::Saying(t) => serde_json::json!({ "type": "saying", "text": t }).to_string(),
                Ctl::Said => r#"{"type":"said"}"#.to_string(),
            };
            let _ = ws.send(Message::text(text));
        }
        take(&mut out, &on);
        for f in out.drain(..) { if !paused { let _ = ws.send(Message::binary(f)); } }
        match ws.read() {
            Ok(Message::Text(t)) => { let m = voice::parse(t.as_str()); if m != Msg::Other { on(Ev::Msg(m)); } }
            Ok(Message::Close(f)) => { eprintln!("[native] voice: the server closed the socket: {f:?}"); if !stop.load(Ordering::Relaxed) { on(Ev::Closed); } return; }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => { eprintln!("[native] voice: the socket failed: {e}"); if !stop.load(Ordering::Relaxed) { on(Ev::Closed); } return; }
        }
    }
}

// ─── The speaker ───

/// What the speaker does.
pub enum Out {
    /// A tone: the sent one (true) or the listen one.
    Tone(bool),
    /// A reply starts (its id); its samples follow; then it ends.
    Begin(u64, u32),
    Pcm(u64, Vec<i16>),
    End(u64),
    Pause,
    Resume,
    /// Stop the reply being said (and drop the rest).
    Stop,
}

/// The speaker's thread (rodio's output stays on it). `done(id)` is called
/// when a reply has been said (or could not be: no output device).
pub fn speaker(done: impl Fn(u64) + Send + 'static) -> mpsc::Sender<Out> {
    let (tx, rx) = mpsc::channel::<Out>();
    thread::spawn(move || {
        use rodio::Source;
        let out = rodio::OutputStream::try_default().ok();
        let tones = out.as_ref().and_then(|(_, h)| rodio::Sink::try_new(h).ok());
        let reply = out.as_ref().and_then(|(_, h)| rodio::Sink::try_new(h).ok());
        if out.is_none() { eprintln!("[native] voice: no audio output; tones and replies are not played"); }
        // The reply being said: its id, rate, whether all its samples came.
        let mut cur: Option<(u64, u32, bool)> = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Out::Tone(sent)) => { if let Some(s) = &tones { s.append(rodio::buffer::SamplesBuffer::new(1, 24_000, voice::tone(sent, 24_000)).amplify(1.0)); } }
                Ok(Out::Begin(id, rate)) => {
                    if let Some(s) = &reply { s.clear(); s.play(); }
                    cur = Some((id, rate, false));
                }
                Ok(Out::Pcm(id, samples)) => {
                    if let (Some((cid, rate, _)), Some(s)) = (cur, &reply) {
                        if cid == id { s.append(rodio::buffer::SamplesBuffer::new(1, rate, samples)); }
                    }
                }
                Ok(Out::End(id)) => { if let Some(c) = cur.as_mut() { if c.0 == id { c.2 = true; } } }
                Ok(Out::Pause) => { if let Some(s) = &reply { s.pause(); } }
                Ok(Out::Resume) => { if let Some(s) = &reply { s.play(); } }
                Ok(Out::Stop) => {
                    if let Some(s) = &reply { s.clear(); s.play(); }
                    if let Some((id, _, _)) = cur.take() { done(id); }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return,
            }
            // A reply whose samples all came and have played: done.
            if let Some((id, _, true)) = cur {
                let empty = reply.as_ref().is_none_or(|s| s.empty());
                let paused = reply.as_ref().is_some_and(|s| s.is_paused());
                if empty && !paused { cur = None; done(id); }
            }
        }
    });
    tx
}

// ─── The device's own voice (no Deepgram voice, or over its limit) ───

/// What the device voice does.
pub enum Dev {
    /// Say a reply (its id) at a speed (1 = normal, the settings' speechRate).
    Say(u64, String, f32),
    Pause,
    Resume,
    Stop,
}

/// One engine: says a sentence, is it still saying it, stops it.
trait Engine {
    fn say(&mut self, text: &str, rate: f32) -> bool;
    fn busy(&mut self) -> bool;
    fn stop(&mut self);
}

/// Linux: speech-dispatcher's spd-say (en, a male voice), waiting for the
/// sentence to be said (-w).
#[cfg(not(any(windows, target_os = "macos")))]
struct Spd { child: Option<std::process::Child> }

#[cfg(not(any(windows, target_os = "macos")))]
impl Engine for Spd {
    fn say(&mut self, text: &str, rate: f32) -> bool {
        // speechRate 0.5..2 to spd-say's -100..100.
        let r = (((rate - 1.0) * 100.0).round() as i32).clamp(-100, 100);
        let c = std::process::Command::new("spd-say").args(["-w", "-l", "en", "-t", "male1", "-r", &r.to_string(), "--", text])
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
        match c { Ok(ch) => { self.child = Some(ch); true } Err(_) => false }
    }
    fn busy(&mut self) -> bool { self.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None))) }
    fn stop(&mut self) {
        if let Some(mut c) = self.child.take() { let _ = c.kill(); let _ = c.wait(); }
        let _ = std::process::Command::new("spd-say").arg("-S").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn engine() -> Option<Box<dyn Engine>> {
    // Only where speech-dispatcher's command is there.
    let ok = std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("spd-say").is_file()));
    ok.then(|| Box::new(Spd { child: None }) as Box<dyn Engine>)
}

/// Windows and macOS: the tts crate (SAPI / WinRT, AVSpeechSynthesizer),
/// an en-GB voice (a male one where there is one) at the given speed.
#[cfg(any(windows, target_os = "macos"))]
struct Os { tts: tts::Tts }

#[cfg(any(windows, target_os = "macos"))]
impl Engine for Os {
    fn say(&mut self, text: &str, rate: f32) -> bool {
        let (n, lo, hi) = (self.tts.normal_rate(), self.tts.min_rate(), self.tts.max_rate());
        let _ = self.tts.set_rate((n * rate).clamp(lo, hi));
        self.tts.speak(text, true).is_ok()
    }
    fn busy(&mut self) -> bool { self.tts.is_speaking().unwrap_or(false) }
    fn stop(&mut self) { let _ = self.tts.stop(); }
}

#[cfg(any(windows, target_os = "macos"))]
fn engine() -> Option<Box<dyn Engine>> {
    let mut t = tts::Tts::default().ok()?;
    if let Ok(voices) = t.voices() {
        let gb: Vec<_> = voices.iter().filter(|v| v.language().to_string().to_lowercase().starts_with("en-gb")).collect();
        let pick = gb.iter().find(|v| matches!(v.gender(), Some(tts::Gender::Male))).or(gb.first());
        if let Some(v) = pick { let _ = t.set_voice(v); }
    }
    Some(Box::new(Os { tts: t }))
}

/// The device voice's thread. `done(id, voiced)` when a reply is said (or
/// could not be: `voiced` false, the system has no voice).
pub fn device_voice(done: impl Fn(u64, bool) + Send + 'static) -> mpsc::Sender<Dev> {
    let (tx, rx) = mpsc::channel::<Dev>();
    thread::spawn(move || {
        let mut eng = engine();
        if eng.is_none() { eprintln!("[native] voice: this system has no voice to say replies with"); }
        // The reply: its id, sentences, the one being said, its deadline, the speed.
        let mut cur: Option<(u64, Vec<String>, usize, Instant, f32)> = None;
        let mut paused = false;
        loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Dev::Say(id, text, rate)) => {
                    if let Some(e) = eng.as_mut() { e.stop(); }
                    if let Some((old, ..)) = cur.take() { done(old, true); }
                    paused = false;
                    let Some(e) = eng.as_mut() else { done(id, false); continue };
                    let s = voice::sentences(&text);
                    if s.is_empty() || !e.say(&s[0], rate) { done(id, false); continue; }
                    cur = Some((id, s, 0, Instant::now() + Duration::from_millis(voice::say_cap_ms(&text)), rate));
                }
                Ok(Dev::Pause) => { if cur.is_some() { paused = true; if let Some(e) = eng.as_mut() { e.stop(); } } }
                // Where it can't pause, it starts again from the sentence it was in.
                Ok(Dev::Resume) => {
                    if paused { paused = false; if let (Some((_, s, i, _, r)), Some(e)) = (cur.as_ref(), eng.as_mut()) { e.say(&s[*i], *r); } }
                }
                Ok(Dev::Stop) => { if let Some(e) = eng.as_mut() { e.stop(); } if let Some((id, ..)) = cur.take() { done(id, true); } paused = false; }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return,
            }
            if paused { continue; }
            let Some((id, s, i, deadline, rate)) = cur.as_mut() else { continue };
            let Some(e) = eng.as_mut() else { continue };
            if Instant::now() > *deadline { e.stop(); let id = *id; cur = None; done(id, true); continue; }
            if e.busy() { continue; }
            *i += 1;
            if *i < s.len() { let (t, r) = (s[*i].clone(), *rate); if !e.say(&t, r) { let id = *id; cur = None; done(id, true); } }
            else { let id = *id; cur = None; done(id, true); }
        }
    });
    tx
}
