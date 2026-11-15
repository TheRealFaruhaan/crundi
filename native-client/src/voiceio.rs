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
            Ok(Err(_)) => { if !stop.load(Ordering::Relaxed) { on(Ev::Closed); } return; }
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
            Ok(Message::Close(_)) => { if !stop.load(Ordering::Relaxed) { on(Ev::Closed); } return; }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => { if !stop.load(Ordering::Relaxed) { on(Ev::Closed); } return; }
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
