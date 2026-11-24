//! "Hey Cru": the wake word on the PC (app/voice-wake controller.js, worker.js;
//! the web's Settings › Voice assistant › On this device, vw*). Off by
//! default; it listens only while every one of these holds: the setting is
//! on, it was trained, no training is going on, the PC is on AC power, Cru's
//! screen is closed and the screen is not locked. Then a thread of its own
//! takes the microphone (16 kHz mono) and runs the spotter (engine.rs); a
//! wake whose voice is the owner's raises the window and opens Cru.
//!
//! Training: five recordings of the owner, each checked, then trained in
//! the background (enroll.rs); the profile is kept in the config folder
//! (voice-wake/profile.json, 0600) with the owner's voice prints.

pub mod engine;
pub mod enroll;

use crate::ctx as kit;
use crundi_core::capture::Clean;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::Disableable as _;
use serde_json::{json, Value};
use std::{cell::RefCell, sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc}, time::{Duration, Instant}};

const LABELS: &[(&str, &str)] = &[
    ("installing", "Downloading the wake word model…"), ("on-battery", "Paused: on battery"), ("voice-open", "Paused: voice screen open"),
    ("paused", "Paused: screen locked"), ("training", "Paused while training"), ("starting", "Starting…"), ("listening", "Listening"),
];
/// A training recording ends at 3 s, or 0.6 s after the voice stops.
const REC_MAX: usize = enroll::RATE * 3;
const REC_TAIL: usize = enroll::RATE * 6 / 10;

fn dir() -> std::path::PathBuf { crundi_core::store::config_dir().join("voice-wake") }
fn profile_file() -> std::path::PathBuf { dir().join("profile.json") }
fn setting_file() -> std::path::PathBuf { dir().join("settings.json") }

fn load_profile() -> Option<Value> {
    let v: Value = serde_json::from_slice(&std::fs::read(profile_file()).ok()?).ok()?;
    enroll::valid_profile(&v).then_some(v)
}
fn save_profile(p: &Value) -> Result<(), String> {
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let tmp = profile_file().with_extension("json.tmp");
    std::fs::write(&tmp, p.to_string()).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)); }
    std::fs::rename(&tmp, profile_file()).map_err(|e| e.to_string())
}

/// What the worker thread is told.
enum Ctl { Profile(Value), Feed(Vec<f32>), Stop }

/// The training's state (the web's vwt).
#[derive(Default)]
struct Train { active: bool, recs: Vec<Option<Vec<f32>>>, raws: Vec<Option<Vec<f32>>>, raw_rec: Vec<f32>, todo: Vec<usize>, pos: usize, rec: Option<(Vec<f32>, f64, bool, usize)>, msg: String, bad: bool, busy: bool, level: f32, stop: Option<Arc<AtomicBool>> }

#[derive(Default)]
struct St {
    enabled: bool,
    profile: Option<Value>,
    training: Train,
    installing: bool,
    install_error: String,
    on_battery: bool,
    locked: bool,
    /// The test driver's stand-ins for the power and the lock.
    test_power: Option<bool>,
    test_lock: Option<bool>,
    worker: Option<(mpsc::Sender<Ctl>, Arc<AtomicBool>)>,
    ready: bool,
    mic_error: String,
    last_heard: Option<(String, Instant)>,
    log: Vec<String>,
    checked_at: Option<Instant>,
    /// The wall clock at that look: a gap far past CHECK_EVERY means the PC slept.
    checked_wall: Option<std::time::SystemTime>,
    started: bool,
}

thread_local! { static S: RefCell<St> = RefCell::new(St::default()); }
fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { S.with(|s| f(&mut s.borrow_mut())) }

fn log(s: String) {
    eprintln!("[voice-wake] {s}");
    with(|x| { x.log.push(s); if x.log.len() > 50 { x.log.remove(0); } });
}

/// The status changed: the views take their copies again (no window refresh: rule 2).
/// A change to listening or training: the area's views, and Settings ›
/// Voice assistant › On this device (Operations' page draws this part).
fn redraw() { kit::dirty(); kit::on_app(|_, cx| crundi_operations::settings_part_changed(cx)); }

/// At start: the setting and the profile.
pub fn init() {
    let enabled = std::fs::read(setting_file()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v["enabled"].as_bool()).unwrap_or(false);
    with(|s| { s.enabled = enabled; s.profile = load_profile(); s.started = true; });
}

fn save_enabled(on: bool) -> bool {
    let _ = std::fs::create_dir_all(dir());
    std::fs::write(setting_file(), json!({ "enabled": on }).to_string()).is_ok()
}

// ─── The conditions ───

/// On battery? (Linux: a mains supply says offline; macOS: pmset; Windows: assumed on AC.)
pub fn read_power() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        // SAFETY: a plain call filling a sized struct.
        let mut st: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
        // ACLineStatus: 0 offline (on battery), 1 online, 255 unknown.
        return unsafe { GetSystemPowerStatus(&mut st) } != 0 && st.ACLineStatus == 0;
    }
    #[allow(unreachable_code)]
    if cfg!(target_os = "linux") {
        let Ok(rd) = std::fs::read_dir("/sys/class/power_supply") else { return false };
        let mains: Vec<bool> = rd.flatten().filter(|e| std::fs::read_to_string(e.path().join("type")).is_ok_and(|t| t.trim() == "Mains"))
            .map(|e| std::fs::read_to_string(e.path().join("online")).is_ok_and(|o| o.trim() == "0")).collect();
        !mains.is_empty() && mains.iter().all(|b| *b)
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("pmset").args(["-g", "batt"]).output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("'Battery Power'"))
    } else { false }
}

/// The screen locked? (Linux: logind's LockedHint; Windows: the input
/// desktop is Winlogon's (idle.rs); macOS: not known, so no.)
pub fn read_locked() -> bool {
    #[cfg(target_os = "windows")]
    { return crate::idle::screen_locked(); }
    #[allow(unreachable_code)]
    if !cfg!(target_os = "linux") { return false; }
    let Ok(o) = std::process::Command::new("loginctl").args(["show-session", "auto", "-p", "LockedHint", "--value"]).output() else { return false };
    String::from_utf8_lossy(&o.stdout).trim() == "yes"
}

fn voice_open() -> bool { crate::cru::with(|c| c.open) }

fn wanted() -> bool {
    let open = voice_open();
    with(|s| s.enabled && s.profile.is_some() && !s.training.active && !s.on_battery && !s.locked && !open && !s.installing && s.install_error.is_empty() && engine::installed())
}

/// The status (controller.js status()): its state, its words.
pub fn status() -> Value {
    let open = voice_open();
    let installed = engine::installed();
    with(|s| {
        let listening = s.enabled && s.profile.is_some();
        let state = if !listening { "off" } else if s.training.active { "training" } else if s.installing { "installing" } else if !s.install_error.is_empty() { "error" }
            else if s.on_battery { "on-battery" } else if s.locked { "paused" } else if open { "voice-open" } else if !s.mic_error.is_empty() { "error" }
            else if s.worker.is_some() && s.ready { "listening" } else { "starting" };
        let error = if !s.install_error.is_empty() { s.install_error.clone() } else { s.mic_error.clone() };
        let label = if state == "error" { format!("Not listening: {}", error.chars().take(80).collect::<String>()) } else { LABELS.iter().find(|(k, _)| *k == state).map(|x| x.1.to_string()).unwrap_or_default() };
        json!({
            "enabled": listening, "state": state, "label": label, "onBattery": s.on_battery, "locked": s.locked, "installed": installed, "installing": s.installing, "error": error,
            "lastHeard": s.last_heard.as_ref().map(|(k, at)| json!({ "keyword": k, "agoMs": at.elapsed().as_millis() as u64 })),
            "trained": s.profile.is_some(), "trainedAt": s.profile.as_ref().map(|p| p["trainedAt"].clone()).unwrap_or(Value::Null),
            "voiceCheck": s.profile.as_ref().map(|p| if engine::valid_voice(&p["voice"]) { "on" } else { "off" }).unwrap_or(""),
            "training": s.training.active, "trainBusy": s.training.busy, "trainPos": s.training.pos, "trainTodo": s.training.todo, "trainMsg": s.training.msg, "trainBad": s.training.bad,
            "recording": s.training.rec.is_some(), "log": s.log.iter().rev().take(5).cloned().collect::<Vec<_>>(),
        })
    })
}

/// The power and the lock are read again this often while they matter.
const CHECK_EVERY: Duration = Duration::from_secs(2);

/// When the conditions must be read again (lib.rs `next_due`): only while
/// listening is on and trained (else nothing to decide); `tick` moves it on.
pub fn next_due() -> Option<Instant> { with(|s| checks(s).then(|| s.checked_at.map(|t| t + CHECK_EVERY).unwrap_or_else(Instant::now))) }

fn checks(s: &St) -> bool { s.started && s.enabled && s.profile.is_some() }

/// Each pump: the conditions read when due (while they matter), the worker started or stopped.
pub fn tick() {
    if !with(|s| s.started) { return; }
    let due = with(|s| checks(s) && s.checked_at.is_none_or(|t| t.elapsed() >= CHECK_EVERY));
    if due {
        let (b, l) = (read_power(), read_locked());
        let now = std::time::SystemTime::now();
        let slept = with(|s| s.checked_wall.is_some_and(|w| resumed(w, now)));
        with(|s| { s.checked_at = Some(Instant::now()); s.checked_wall = Some(now); s.on_battery = s.test_power.unwrap_or(b); s.locked = s.test_lock.unwrap_or(l); });
        // Back from sleep (powerMonitor 'resume'): the microphone a sleep
        // interrupted does not come back by itself, so the worker starts again.
        if slept && with(|s| s.worker.is_some()) { log("resumed from sleep: listening again".into()); stop_worker(); }
    }
    reconcile();
}

/// The PC slept between two looks 2 s apart: the wall clock moved on much further.
fn resumed(before: std::time::SystemTime, now: std::time::SystemTime) -> bool {
    now.duration_since(before).is_ok_and(|d| d > CHECK_EVERY + Duration::from_secs(30))
}

fn reconcile() {
    let listening = with(|s| s.enabled && s.profile.is_some());
    if listening && !engine::installed() && !with(|s| s.installing || !s.install_error.is_empty()) { install(); return; }
    let want = wanted();
    let running = with(|s| s.worker.is_some());
    if want && !running { start_worker(); redraw(); }
    if !want && running { stop_worker(); redraw(); }
}

fn install() {
    with(|s| { s.installing = true; s.install_error.clear(); });
    log("downloading the wake word model".into());
    std::thread::spawn(|| {
        let r = engine::install();
        kit::main_thread(move || {
            with(|s| { s.installing = false; if let Err(e) = &r { s.install_error = e.clone(); } });
            match r { Ok(()) => log("installed the wake word model".into()), Err(e) => log(format!("install failed: {e}")) }
            redraw();
        });
    });
    redraw();
}

// ─── The worker: the microphone and the spotter, on a thread of its own ───

fn start_worker() {
    let Some(profile) = with(|s| s.profile.clone()) else { return };
    let (tx, rx) = mpsc::channel::<Ctl>();
    let stop = Arc::new(AtomicBool::new(false));
    with(|s| { s.worker = Some((tx.clone(), stop.clone())); s.ready = false; s.mic_error.clear(); });
    let s2 = stop.clone();
    std::thread::spawn(move || {
        let mut wake = match engine::Wake::new(&profile) { Ok(w) => w, Err(e) => { kit::main_thread(move || { with(|s| s.mic_error = e); redraw(); }); return; } };
        let owner = engine::Owner::load();
        // The microphone (or nothing, under the test driver without one: fed only).
        let (mtx, mrx) = mpsc::channel::<(Vec<f32>, Vec<f32>)>();
        let mic_ok = mic(mtx, s2.clone());
        if let Err(e) = &mic_ok { if std::env::var_os("CRUNDI_TEST_PORT").is_none() { let e = e.clone(); kit::main_thread(move || { with(|s| s.mic_error = e); redraw(); }); return; } }
        kit::main_thread(|| { with(|s| s.ready = true); log("listening".into()); redraw(); });
        let mut current = profile;
        loop {
            if s2.load(Ordering::Relaxed) { return; }
            let mut blocks: Vec<(Vec<f32>, Vec<f32>)> = vec![];
            while let Ok(c) = rx.try_recv() {
                match c {
                    Ctl::Stop => return,
                    Ctl::Profile(p) => { if let Ok(w) = engine::Wake::new(&p) { wake = w; current = p; } }
                    Ctl::Feed(s) => { for c in s.chunks(1600) { blocks.push((c.to_vec(), c.to_vec())); } blocks.push((vec![0.0; enroll::RATE], vec![0.0; enroll::RATE])); }
                }
            }
            while let Ok(b) = mrx.try_recv() { blocks.push(b); }
            if blocks.is_empty() { std::thread::sleep(Duration::from_millis(30)); continue; }
            for (b, braw) in blocks {
                let h = wake.accept_both(&b, &braw);
                // A phrase that was not quite it (T16): its match, to tell a miss from not hearing at all.
                // (its gate, and the burst kept in voice-wake/misses/ to replay a miss.)
                if let Some((score, limit, why, clip)) = wake.near_miss.take() {
                    let file = keep_miss(&format!("phrase-{why}-{score:.3}"), &clip);
                    // (and its undenoised twin, what the model and the hum guard heard)
                    if let Some(r) = wake.last_raw.take() { let _ = keep_miss(&format!("phrase-{why}-{score:.3}-raw"), &r); }
                    kit::main_thread(move || log(format!("near miss: a phrase matched {score:.3} (strict limit {:.3} of {limit:.3}), refused by the {why} check{file}", enroll::strict_limit_of(limit))));
                }
                let Some(h) = h else { continue };
                if !h.ok { let file = keep_miss(&format!("model-{:.3}", h.score), &h.clip); if !file.is_empty() { kit::main_thread(move || log(format!("the model's hit kept{file}"))); } }
                // The owner's voice, when the profile has it (ownerChecked).
                let mut verdict = if h.ok { "detected" } else { "rejected" };
                let mut voice = Value::Null;
                if h.ok && engine::valid_voice(&current["voice"]) {
                    if let Some(o) = &owner {
                        let (ok, s) = o.check(&current["voice"], &h.clip);
                        voice = json!(s);
                        if ok == Some(false) { verdict = "rejected"; wake.refused(); }
                        if ok.is_none() { let s2 = s; kit::main_thread(move || log(format!("the owner's voice check could not tell (score {s2:.3})"))); }
                    }
                }
                let (kw, score, via) = (h.keyword.clone(), h.score, h.via);
                kit::main_thread(move || heard(verdict, &kw, score, via, voice));
            }
        }
    });
}

/// A refused wake's audio (16 kHz mono WAV) in voice-wake/misses/, the
/// last 20 kept: to replay a miss on the owner's own microphone (T16).
fn keep_miss(name: &str, clip: &[f32]) -> String {
    let d = dir().join("misses");
    if std::fs::create_dir_all(&d).is_err() || clip.is_empty() { return String::new(); }
    let pcm: Vec<u8> = clip.iter().flat_map(|x| (((x.clamp(-1.0, 1.0)) * 32767.0) as i16).to_le_bytes()).collect();
    let rate = enroll::RATE as u32;
    let mut w = Vec::with_capacity(44 + pcm.len());
    w.extend_from_slice(b"RIFF"); w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes()); w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes()); w.extend_from_slice(&1u16.to_le_bytes()); w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes()); w.extend_from_slice(&(rate * 2).to_le_bytes()); w.extend_from_slice(&2u16.to_le_bytes()); w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data"); w.extend_from_slice(&(pcm.len() as u32).to_le_bytes()); w.extend_from_slice(&pcm);
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let f = d.join(format!("{ms}-{name}.wav"));
    if std::fs::write(&f, w).is_err() { return String::new(); }
    if let Ok(rd) = std::fs::read_dir(&d) {
        let mut all: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
        all.sort();
        while all.len() > 20 { let _ = std::fs::remove_file(all.remove(0)); }
    }
    format!(" ({})", f.display())
}

fn stop_worker() {
    if let Some((tx, stop)) = with(|s| { s.ready = false; s.worker.take() }) { stop.store(true, Ordering::Relaxed); let _ = tx.send(Ctl::Stop); }
}

/// A wake: the owner's, the window raised and Cru opened; else logged.
fn heard(verdict: &str, keyword: &str, score: f64, via: &str, voice: Value) {
    if verdict != "detected" { log(format!("heard {keyword} via {via}, but not the trained voice (match {score}, voice {voice})")); return; }
    log(format!("heard {keyword} (match {score}) via {via}, owner's voice {voice}"));
    with(|s| s.last_heard = Some((keyword.to_string(), Instant::now())));
    kit::raise();
    if !voice_open() { crate::cru::open(false); }
    redraw();
}

/// The microphone at 16 kHz mono, in blocks, until `stop` (a thread of its
/// own holds the stream; its setup's answer comes back first).
fn mic(out: mpsc::Sender<(Vec<f32>, Vec<f32>)>, stop: Arc<AtomicBool>) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    std::thread::spawn(move || {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let setup = || -> Result<(cpal::Stream, usize, mpsc::Receiver<(Vec<f32>, Vec<f32>)>), String> {
            let host = cpal::default_host();
            let dev = host.default_input_device().ok_or("no microphone")?;
            let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
            let (rate, ch) = (cfg.sample_rate().0 as usize, cfg.channels() as usize);
            // What it hears from (T16): to tell a wrong device or format from a quiet microphone.
            let what = format!("microphone: {} ({:?}, {} Hz, {} channel{}), denoised, low-passed to 16 kHz mono", dev.name().unwrap_or_else(|_| "?".into()), cfg.sample_format(), rate, ch, if ch == 1 { "" } else { "s" });
            kit::main_thread(move || log(what));
            let (tx, rx) = mpsc::channel::<(Vec<f32>, Vec<f32>)>();
            let err = |e| eprintln!("[voice-wake] microphone: {e}");
            let stream = match cfg.sample_format() {
                cpal::SampleFormat::F32 => { let o = tx.clone(); let mut c = Clean::for_wake(rate as u32); dev.build_input_stream(&cfg.into(), move |d: &[f32], _| { let b = c.push_both(&crundi_core::voice::mono(d, ch)); if !b.0.is_empty() { let _ = o.send(b); } }, err, None) }
                cpal::SampleFormat::I16 => { let o = tx.clone(); let mut c = Clean::for_wake(rate as u32); dev.build_input_stream(&cfg.into(), move |d: &[i16], _| { let f: Vec<f32> = d.iter().map(|x| *x as f32 / 32768.0).collect(); let b = c.push_both(&crundi_core::voice::mono(&f, ch)); if !b.0.is_empty() { let _ = o.send(b); } }, err, None) }
                _ => return Err("this microphone's format is not supported".into()),
            }.map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            // Cleaned (capture.rs, as Chromium cleans Electron's): out at 48 kHz.
            Ok((stream, crundi_core::capture::OUT_RATE as usize, rx))
        };
        let (stream, rate, rx) = match setup() { Ok(x) => { let _ = ready_tx.send(Ok(())); x } Err(e) => { let _ = ready_tx.send(Err(e)); return; } };
        let _keep = stream;
        let (mut down, mut down_raw) = (engine::To16::new(rate), engine::To16::new(rate));
        // The input level, logged every 30 s: its loudest 100 ms and its median (dBFS).
        let (mut lv, mut acc, mut n, mut since) = (Vec::<f64>::new(), 0f64, 0usize, Instant::now());
        while !stop.load(Ordering::Relaxed) {
            while let Ok((b, braw)) = rx.try_recv() {
                let o = down.push(&b);
                let mut r = down_raw.push(&braw);
                r.resize(o.len(), 0.0);
                for x in &o { acc += (*x as f64).powi(2); n += 1; if n == enroll::RATE / 10 { lv.push(10.0 * (acc / n as f64 + 1e-12).log10()); acc = 0.0; n = 0; } }
                if since.elapsed() >= Duration::from_secs(30) && !lv.is_empty() {
                    lv.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    let line = format!("input level: loudest {:.0} dBFS, median {:.0} dBFS", lv[lv.len() - 1], lv[lv.len() / 2]);
                    kit::main_thread(move || log(line));
                    lv.clear(); since = Instant::now();
                }
                if !o.is_empty() && out.send((o, r)).is_err() { return; }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| Err("the microphone did not start".into()))
}

// ─── The setting, training, forgetting ───

pub fn set(on: bool) {
    if on && with(|s| s.profile.is_none()) { return kit::toast_error("Train Hey Cru first"); }
    if !save_enabled(on) { return kit::toast_error("Could not save the setting"); }
    with(|s| { s.enabled = on; if on { s.install_error.clear(); } });
    log(if on { "turned on".into() } else { "turned off".into() });
    // The power and the lock read now (they are not read while it is off), then started or not.
    with(|s| s.checked_at = None);
    tick();
    redraw();
}

/// Train: five recordings, from the start.
pub fn train_start() {
    if with(|s| s.training.active) { return; }
    with(|s| s.training = Train { active: true, recs: vec![None; enroll::SAMPLES_NEEDED], raws: vec![None; enroll::SAMPLES_NEEDED], todo: (0..enroll::SAMPLES_NEEDED).collect(), msg: "Click Record, then say “Hey Cru” the way you normally would.".into(), ..Default::default() });
    log("training started".into());
    reconcile();
    if !engine::installed() && !with(|s| s.installing) { install(); }
    redraw();
}

/// Training runs while Settings shows it: leaving Settings ends it (the web's
/// Voice assistant section going away stops its recorder).
pub fn left_settings() { if with(|s| s.training.active) { train_stop(); } }

pub fn train_stop() {
    if let Some(st) = with(|s| { let st = s.training.stop.take(); s.training.active = false; s.training.rec = None; st }) { st.store(true, Ordering::Relaxed); }
    log("training ended".into());
    reconcile();
    redraw();
}

/// Record (or Stop): the microphone for one take.
pub fn record() {
    if with(|s| !s.training.active || s.training.busy || s.training.rec.is_some()) {
        if with(|s| s.training.rec.is_some()) { finish(); }
        return;
    }
    let (tx, rx) = mpsc::channel::<(Vec<f32>, Vec<f32>)>();
    let stop = Arc::new(AtomicBool::new(false));
    if let Err(e) = mic(tx, stop.clone()) {
        with(|s| { s.training.msg = format!("Could not open the microphone: {e}"); s.training.bad = true; });
        return redraw();
    }
    with(|s| { s.training.rec = Some((vec![], 1.0, false, 0)); s.training.raw_rec.clear(); s.training.stop = Some(stop.clone()); });
    // The blocks to the main thread, while it records.
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match rx.recv_timeout(Duration::from_millis(100)) { Ok((b, r)) => kit::main_thread(move || { with(|s| if s.training.rec.is_some() { s.training.raw_rec.extend_from_slice(&r) }); feed(&b) }), Err(mpsc::RecvTimeoutError::Timeout) => {}, Err(_) => return }
        }
    });
    redraw();
}

/// A piece from the microphone: kept, shown on the meter; the take ends at 3 s or after a short quiet once something was said.
fn feed(c: &[f32]) {
    let done = with(|s| {
        let t = &mut s.training;
        let Some((all, floor, spoke, quiet)) = t.rec.as_mut() else { return false };
        all.extend_from_slice(c);
        let rms = (c.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / c.len().max(1) as f64).sqrt();
        t.level = ((rms.sqrt() * 250.0).min(100.0) / 100.0) as f32;
        if rms < *floor { *floor = rms; }
        if rms > 0.02f64.max(*floor * 4.0) { *spoke = true; *quiet = 0; } else if *spoke { *quiet += c.len(); }
        (*spoke && *quiet >= REC_TAIL) || all.len() >= REC_MAX
    });
    if done { finish(); } else { redraw(); }
}

fn finish() {
    let Some((all, ..)) = with(|s| { if let Some(st) = s.training.stop.take() { st.store(true, Ordering::Relaxed); } s.training.rec.take() }) else { return };
    let raw = with(|s| std::mem::take(&mut s.training.raw_rec));
    take_both(all, Some(raw));
}

/// A take in: checked, kept; after the last, trained.
pub fn take(all: Vec<f32>) { take_both(all, None) }

/// A take and the same take not denoised (the keyword model's, V2).
fn take_both(all: Vec<f32>, raw: Option<Vec<f32>>) {
    let Some(i) = with(|s| s.training.todo.get(s.training.pos).copied()) else { return };
    match enroll::check_sample(&all) {
        Err(why) => with(|s| { s.training.msg = format!("{why} Record it again."); s.training.bad = true; }),
        Ok(_) => {
            let raw = raw.filter(|r| r.len() == all.len());
            let more = with(|s| { s.training.raws[i] = raw; s.training.recs[i] = Some(all); s.training.pos += 1; s.training.bad = false; let more = s.training.pos < s.training.todo.len(); if more { s.training.msg = "Got it.".into(); } more });
            if !more { return train(); }
        }
    }
    redraw();
}

fn train() {
    let recs: Vec<Vec<f32>> = with(|s| { s.training.busy = true; s.training.msg.clear(); s.training.recs.iter().map(|r| r.clone().unwrap_or_default()).collect() });
    // The undenoised takes, when every one has its own (else the model hears the cleaned ones).
    let raws: Option<Vec<Vec<f32>>> = with(|s| s.training.raws.iter().cloned().collect::<Option<Vec<_>>>());
    redraw();
    std::thread::spawn(move || {
        // The model first (downloaded once), then a few seconds.
        let ready = if engine::installed() { Ok(()) } else { engine::install() };
        let r = ready.map_err(|e| (e, vec![])).and_then(|_| engine::train_with_voice_raw(&recs, raws.as_deref()));
        kit::main_thread(move || trained(r));
    });
}

fn trained(r: Result<(Value, String), (String, Vec<usize>)>) {
    with(|s| s.training.busy = false);
    match r {
        Ok((p, note)) => {
            if let Err(e) = save_profile(&p) { with(|s| { s.training.msg = format!("Could not save the profile: {e}"); s.training.bad = true; }); return redraw(); }
            let voice = engine::valid_voice(&p["voice"]);
            log(format!("trained (threshold {}, limit {}, owner's voice check {})", p["keywordsThreshold"], p["limit"], if voice { "on" } else { "off" }));
            if let Some((tx, _)) = with(|s| s.worker.clone()) { let _ = tx.send(Ctl::Profile(p.clone())); }
            with(|s| {
                s.profile = Some(p);
                s.training.msg = if voice { "Done. Only your voice wakes Cru. With listening on, say “Hey Cru” while the computer is plugged in.".into() }
                    else if !note.is_empty() { format!("Done. {note}") } else { "Done. With listening on, say “Hey Cru” while the computer is plugged in.".into() };
                s.training.bad = !note.is_empty() && !voice;
                s.training.active = false;
            });
            log("training ended".into());
            reconcile();
        }
        Err((why, redo)) => with(|s| {
            let redo: Vec<usize> = redo.into_iter().filter(|n| *n < enroll::SAMPLES_NEEDED).collect();
            s.training.todo = if redo.is_empty() { (0..enroll::SAMPLES_NEEDED).collect() } else { redo };
            s.training.pos = 0;
            s.training.msg = if why.is_empty() { "That did not work. Record them again.".into() } else { why };
            s.training.bad = true;
        }),
    }
    redraw();
}

pub fn forget() {
    kit::confirm("Forget your “Hey Cru” training? Listening turns off until you train it again.", "Forget", true, |ok| {
        if !ok { return; }
        let _ = std::fs::remove_file(profile_file());
        with(|s| { s.profile = None; s.training.msg.clear(); });
        if with(|s| s.enabled) && save_enabled(false) { with(|s| s.enabled = false); }
        log("profile forgotten".into());
        reconcile();
        redraw();
    });
}

/// The test driver: a recording heard by the running spotter.
pub fn hear(samples: Vec<f32>) -> bool {
    match with(|s| s.worker.clone()) { Some((tx, _)) => tx.send(Ctl::Feed(samples)).is_ok(), None => false }
}

/// The test driver's stand-ins for the power and the lock (None: the system's).
pub fn test_conditions(battery: Option<bool>, locked: Option<bool>) {
    with(|s| { s.test_power = battery; s.test_lock = locked; s.checked_at = None; });
    tick();
}

// ─── Settings › Voice assistant › On this device ───

pub fn settings_part(_window: &mut Window, _cx: &mut App) -> AnyElement {
    crundi_core::driver::rendered("voice-device");
    let st = status();
    let (trained, training, busy, recording, msg, bad, level) = with(|s| (s.profile.is_some(), s.training.active, s.training.busy, s.training.rec.is_some(), s.training.msg.clone(), s.training.bad, s.training.level));
    let enabled = st["enabled"].as_bool().unwrap_or(false);
    let line = if training { String::new() } else if enabled { st["label"].as_str().unwrap_or("").to_string() } else { st["error"].as_str().unwrap_or("").to_string() };
    let say = |t: String, bad: bool| div().text_size(T::TEXT_12).text_color(if bad { T::red() } else { T::text_muted() }).child(t);
    let mut d = div().flex().flex_col().gap(T::SP_8).pt(T::SP_8)
        .child(div().flex().items_center().gap(T::SP_8).flex_wrap()
            .child(w::check("voice-wake-on", "Listen for “Hey Cru” while plugged in", enabled, |on, _, _| set(*on)).disabled(!trained || training))
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child(line)));
    if !trained { d = d.child(say("Train Hey Cru first: say it five times so it knows your voice.".into(), false)); }
    if !training {
        if trained {
            let when = st["trainedAt"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| format!(" on {}", t.with_timezone(&chrono::Local).format("%-d %b %Y"))).unwrap_or_default();
            d = d.child(div().flex().items_center().gap(T::SP_8).flex_wrap()
                .child(div().flex().items_center().gap(T::SP_6).text_size(T::TEXT_12).text_color(T::text_secondary()).child(IconName::Check).child(format!("Trained{when}")))
                .when(st["voiceCheck"] == "on", |d| d.child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Only your voice wakes Cru")))
                .child(w::button("voice-wake-train", "Train again").on_click(|_, _, _| train_start()))
                .child(w::button("voice-wake-forget", "Forget").on_click(|_, _, _| forget())));
            if st["voiceCheck"] == "off" && msg.is_empty() { d = d.child(say("Train again so that only your voice wakes Cru: the voice check is not set up for this training.".into(), false)); }
        } else {
            d = d.child(div().child(w::primary("voice-wake-train", "Train Hey Cru").on_click(|_, _, _| train_start())));
        }
        if !msg.is_empty() { d = d.child(say(msg, bad)); }
        return d.into_any_element();
    }
    if busy {
        return d.child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child("Setting up…"))
            .child(say(if st["installing"] == true || !engine::installed() { "Downloading the wake word model first, then a few seconds more.".into() } else { "This takes a few seconds.".into() }, false))
            .into_any_element();
    }
    let i = with(|s| s.training.todo.get(s.training.pos).copied().unwrap_or(0));
    d.child(div().text_size(T::TEXT_14).text_color(T::text()).child(format!("Say “Hey Cru” — {} of {}", i + 1, enroll::SAMPLES_NEEDED)))
        .child(div().flex().items_center().gap(T::SP_8).flex_wrap()
            .child(if recording { w::primary("voice-wake-rec", "Stop").icon(IconName::Square) } else { w::primary("voice-wake-rec", "Record") }.on_click(|_, _, _| record()))
            .child(div().w(T::WAKE_METER_W).child(w::meter(if recording { level } else { 0.0 }, T::accent(), T::SP_6)))
            .child(w::button("voice-wake-cancel", "Cancel").on_click(|_, _, _| train_stop())))
        .child(say(if recording { "Listening…".into() } else { msg }, !recording && bad))
        .into_any_element()
}

#[cfg(test)]
mod tests {

    #[test]
    fn sleep_is_a_long_gap() {
        let t = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(!resumed(t, t + Duration::from_secs(2)));
        assert!(!resumed(t, t + Duration::from_secs(20)));
        assert!(resumed(t, t + Duration::from_secs(600)));
        assert!(!resumed(t + Duration::from_secs(5), t));
    }

    use super::{next_due, resumed, with, St, CHECK_EVERY};
    use std::time::Duration;
    use serde_json::json;
    use std::time::Instant;

    /// The conditions are read only while listening is on and trained, every CHECK_EVERY.
    #[test]
    fn deadlines() {
        with(|s| *s = St { started: true, ..Default::default() });
        assert_eq!(next_due(), None);
        let at = Instant::now();
        with(|s| { s.enabled = true; s.profile = Some(json!({})); s.checked_at = Some(at); });
        assert_eq!(next_due(), Some(at + CHECK_EVERY));
        with(|s| s.checked_at = None);
        assert!(next_due().is_some_and(|t| t <= Instant::now()));
        with(|s| s.enabled = false);
        assert_eq!(next_due(), None);
    }
}
