//! The app's side of the browser host (host/, proc.rs): starting it when a
//! view is first needed, the link's two threads, the views it holds (each
//! an engine panel or a frame), and stopping it.
//!
//! - Lazy: no host at launch. `open_view` starts it; commands sent before
//!   it is up wait in the link (it reads them once CEF has started).
//! - Idle: with no view for `idle_grace()` (CRUNDI_BROWSER_IDLE_SECS,
//!   default 60) and nothing keeping it (the drawer open: `set_keep_alive`),
//!   it is asked to stop, and killed if it hasn't within 3 s.
//! - Crashed: when the link closes and nobody asked, every view is told
//!   (engine `host_gone`, frames `host_gone`: "This page crashed" with
//!   Reload), and the next view starts a new host.
//! - Its PID is in the config folder while it runs; a crashed run's host is
//!   reaped at the next start (proc.rs `reap_stale`).
//!
//! Threads: the reader maps each view's frame segment, copies a painted
//! frame straight out of it into a new GPUI image (the one copy on the
//! app's side: the image owns its pixels, GPUI uploads them from there),
//! answers `Ack` at once, and queues the image and every other event; the
//! app's pump (main thread) hands them to their views.

use crate::ipc::{self, Cmd, Ev, Fd};
use crate::shm::Segment;
use gpui::RenderImage;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    path::PathBuf,
    process::Child,
    sync::{atomic::{AtomicU64, Ordering}, mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

/// Who a view belongs to.
#[derive(Clone, Debug)]
pub enum Owner { Panel(usize), Frame(String) }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State { Off, Starting, Up, Stopping }

/// What the reader hands the main thread, with the link's generation.
enum In { Ev(u32, Ev), Frame(u32, u64, Arc<RenderImage>), Gone(u32) }

static INBOX: Mutex<Vec<In>> = Mutex::new(Vec::new());
/// Frames the app took from the host (every view, every host this run).
static FRAMES: AtomicU64 = AtomicU64::new(0);
/// GPU textures the host handed over and the app opened (gpu.rs `Mode::Texture`), and the last error.
static TEXTURES: AtomicU64 = AtomicU64::new(0);
static TEXTURE_ERROR: Mutex<String> = Mutex::new(String::new());

/// A frame of `w` x `h` BGRA from a segment: the image that owns it.
pub(crate) fn image_from(bytes: &[u8], w: u32, h: u32) -> Option<Arc<RenderImage>> {
    let n = (w as usize) * (h as usize) * 4;
    if bytes.len() < n || n == 0 { return None; }
    let buf = image::RgbaImage::from_raw(w, h, bytes[..n].to_vec())?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buf)])))
}

struct Client {
    state: State,
    epoch: u32,
    child: Option<Child>,
    pid: u32,
    out: Option<mpsc::Sender<Vec<u8>>>,
    owners: HashMap<u64, Owner>,
    next: u64,
    idle_since: Option<Instant>,
    stop_at: Option<Instant>,
    starts: u32,
    crashes: u32,
    /// Starts in a row that died before CEF was up: three and the browser
    /// is off for this run (as if CEF had not started).
    failures: u32,
    available: bool,
    config_dir: PathBuf,
    reaped: Option<u32>,
    last_error: String,
    /// The host's last counters (`Ev::Stats`).
    stats: Value,
}

thread_local! {
    static C: RefCell<Client> = RefCell::new(Client {
        state: State::Off, epoch: 0, child: None, pid: 0, out: None, owners: HashMap::new(), next: 1,
        idle_since: None, stop_at: None, starts: 0, crashes: 0, failures: 0, available: true,
        config_dir: PathBuf::new(), reaped: None, last_error: String::new(), stats: Value::Null,
    });
    static KEEP_ALIVE: Cell<fn() -> bool> = const { Cell::new(|| false) };
}

fn with<R>(f: impl FnOnce(&mut Client) -> R) -> R { C.with(|c| f(&mut c.borrow_mut())) }

/// At the app's start: what a crashed run left is cleaned up. The host
/// itself is not started.
pub fn start(config_dir: PathBuf) -> bool {
    let reaped = crate::proc::reap_stale(&config_dir, &crate::common::cef_root());
    let off = std::env::var_os("CRUNDI_NO_BROWSER").is_some();
    with(|c| { c.config_dir = config_dir; c.reaped = reaped; c.available = !off; });
    !off
}

/// The browser can be used (the host may not be running yet).
pub fn available() -> bool { with(|c| c.available) }

pub fn state() -> State { with(|c| c.state) }

/// While this says so (the drawer is open), the host is kept with no views.
pub fn set_keep_alive(f: fn() -> bool) { KEEP_ALIVE.with(|k| k.set(f)); }

/// How long the host stays with no view.
pub fn idle_grace() -> Duration {
    let s = std::env::var("CRUNDI_BROWSER_IDLE_SECS").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(60.0);
    Duration::from_secs_f64(s.max(0.0))
}

/// How long a web view that is not visible keeps its page before it is
/// freed (CRUNDI_BROWSER_UNLOAD_SECS, default 30): the one rule for every
/// view (drawer tabs, chat embeds, widget panels). Hidden at once (no
/// frames), freed after this, loaded again (with the loading state) when
/// visible again.
pub fn unload_grace() -> Duration {
    let s = std::env::var("CRUNDI_BROWSER_UNLOAD_SECS").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(30.0);
    Duration::from_secs_f64(s.max(0.0))
}

/// A new view for `owner`: the host started if it isn't, `open(id)` sent.
/// None: no browser this run.
pub fn open_view(owner: Owner, open: impl FnOnce(u64) -> Cmd) -> Option<u64> {
    if !ensure() { return None; }
    let id = with(|c| { let id = c.next; c.next += 1; c.owners.insert(id, owner); c.idle_since = None; id });
    send(open(id));
    Some(id)
}

/// A command for a view the host holds (dropped for one it doesn't: closed,
/// or lost in a crash).
pub fn send_to(id: u64, cmd: Cmd) {
    if with(|c| c.owners.contains_key(&id)) { send(cmd); }
}

/// A command for the host as a whole, when it runs.
pub fn send_running(cmd: Cmd) {
    if matches!(state(), State::Starting | State::Up) { send(cmd); }
}

/// Close a view (its segment goes on both sides).
pub fn close_view(id: u64) {
    if with(|c| c.owners.remove(&id).is_some()) { send(Cmd::Close { id }); }
}


fn send(cmd: Cmd) {
    let b = ipc::encode(&cmd);
    with(|c| if let Some(o) = &c.out { let _ = o.send(b); });
}

/// The host running (or on its way): started if it isn't. False: no browser.
fn ensure() -> bool {
    let (state, available) = with(|c| (c.state, c.available));
    if !available { return false; }
    if matches!(state, State::Starting | State::Up) { return true; }
    if state == State::Stopping {
        // Stopping with a new view wanted: let it go now and start afresh.
        kill_child();
        reap_child();
        with(|c| { c.state = State::Off; c.out = None; c.epoch += 1; });
        drop_pid();
    }
    spawn()
}

fn spawn() -> bool {
    crate::proc::clear_stale_locks(&crate::common::cef_root());
    let sp = match crate::proc::spawn_host() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[browser] the browser host did not start: {e}");
            with(|c| { c.available = false; c.last_error = e.to_string(); });
            return false;
        }
    };
    let pid = sp.child.id();
    let epoch = with(|c| { c.epoch += 1; c.epoch });
    let (otx, orx) = mpsc::channel::<Vec<u8>>();
    let mut sender = sp.send;
    std::thread::Builder::new().name("browser-host-out".into()).spawn(move || {
        for b in orx { if sender(&b, None).is_err() { break; } }
    }).ok();
    let ack = otx.clone();
    let mut recv = sp.recv;
    std::thread::Builder::new().name("browser-host-in".into()).spawn(move || reader(epoch, &mut recv, ack)).ok();
    let dir = with(|c| {
        c.state = State::Starting; c.child = Some(sp.child); c.pid = pid; c.out = Some(otx);
        c.starts += 1; c.idle_since = None; c.stop_at = None; c.stats = Value::Null;
        c.config_dir.clone()
    });
    if !dir.as_os_str().is_empty() { crate::proc::write_pid(&dir, pid); }
    send(Cmd::Hello { textures: crate::gpu::mode() == crate::gpu::Mode::Texture });
    eprintln!("[browser] browser host started (pid {pid})");
    true
}

/// The link's reading half (its own thread): events in order, frames
/// copied out of their segments and answered.
fn reader(epoch: u32, recv: &mut ipc::RecvFn, out: mpsc::Sender<Vec<u8>>) {
    let mut d = ipc::Decoder::default();
    let mut buf = vec![0u8; 64 * 1024];
    let mut fds: VecDeque<Fd> = VecDeque::new();
    let mut maps: HashMap<u64, Segment> = HashMap::new();
    #[cfg(windows)]
    let mut textures = crate::gpu::AppTextures::default();
    let push = |m: In| INBOX.lock().unwrap().push(m);
    'read: loop {
        let n = match recv(&mut buf, &mut fds) { Ok(0) | Err(_) => break, Ok(n) => n };
        d.push(&buf[..n]);
        while let Some(m) = d.next::<Ev>() {
            let ev = match m { Ok(e) => e, Err(e) => { eprintln!("[browser] a bad message from the host: {e}"); break 'read; } };
            match ev {
                Ev::Shm { id, cap, name } => {
                    #[cfg(target_os = "linux")]
                    let fd = fds.pop_front();
                    #[cfg(not(target_os = "linux"))]
                    let fd: Option<Fd> = None;
                    match Segment::open(cap, fd, &name) {
                        Ok(s) => { maps.insert(id, s); }
                        Err(e) => { eprintln!("[browser] view {id}: its frame segment did not open: {e}"); maps.remove(&id); }
                    }
                }
                Ev::Paint { id, w, h } => {
                    let img = maps.get(&id).and_then(|s| image_from(s.bytes(), w, h));
                    let _ = out.send(ipc::encode(&Cmd::Ack { id }));
                    if let Some(img) = img { FRAMES.fetch_add(1, Ordering::Relaxed); push(In::Frame(epoch, id, img)); }
                }
                #[allow(unused_variables)]
                Ev::Texture { id, handle, w, h } => {
                    #[cfg(windows)]
                    {
                        let r = unsafe { textures.open(handle) };
                        match r {
                            Ok(_) => { TEXTURES.fetch_add(1, Ordering::Relaxed); }
                            Err(e) => *TEXTURE_ERROR.lock().unwrap() = e,
                        }
                    }
                }
                Ev::Closed { id } => { maps.remove(&id); push(In::Ev(epoch, Ev::Closed { id })); }
                e => push(In::Ev(epoch, e)),
            }
        }
    }
    drop(maps);
    push(In::Gone(epoch));
}

fn owner(id: u64) -> Option<Owner> { with(|c| c.owners.get(&id).cloned()) }

/// The main thread's part (lib.rs `pump`, every few ms): the host's events
/// to their views, a host that went, the idle stop.
pub fn pump() {
    let all = std::mem::take(&mut *INBOX.lock().unwrap());
    for m in all {
        let cur = with(|c| c.epoch);
        match m {
            In::Ev(g, _) | In::Frame(g, ..) | In::Gone(g) if g != cur => {}
            In::Frame(_, id, img) => match owner(id) {
                Some(Owner::Panel(i)) => crate::engine::on_frame(i, id, img),
                Some(Owner::Frame(k)) => crate::frames::on_frame(&k, id, img),
                None => {}
            },
            In::Ev(_, Ev::Stats { sent, unchanged, hidden, copied, gpu, mode, error }) => {
                if !error.is_empty() && with(|c| c.stats.get("error").and_then(|e| e.as_str()) != Some(error.as_str())) { eprintln!("[browser] frames: {error}"); }
                with(|c| c.stats = json!({ "sent": sent, "unchanged": unchanged, "hidden": hidden, "copied": copied, "gpu": gpu, "mode": mode, "error": error }));
            }
            In::Ev(_, Ev::Ready { pid }) => { with(|c| { if c.state == State::Starting { c.state = State::Up; } c.failures = 0; }); eprintln!("[browser] browser host {pid} is up"); }
            In::Ev(_, Ev::Failed { why }) => { eprintln!("[browser] {why}"); with(|c| { c.last_error = why; c.available = false; }); }
            In::Ev(_, ev) => {
                let Some(id) = ev_id(&ev) else { continue };
                match owner(id) {
                    Some(Owner::Panel(i)) => crate::engine::on_event(i, id, ev),
                    Some(Owner::Frame(k)) => crate::frames::on_event(&k, id, ev),
                    None => {}
                }
            }
            In::Gone(_) => gone(),
        }
    }
    idle();
    let late = with(|c| c.state == State::Stopping && c.stop_at.is_some_and(|t| t.elapsed() > Duration::from_secs(3)));
    if late { kill_child(); }
}

fn ev_id(e: &Ev) -> Option<u64> {
    Some(match e {
        Ev::Shm { id, .. } | Ev::Paint { id, .. } | Ev::Loading { id, .. } | Ev::LoadError { id, .. } | Ev::LoadEnd { id, .. }
        | Ev::Title { id, .. } | Ev::Address { id, .. } | Ev::Cursor { id, .. } | Ev::Console { id, .. } | Ev::Popup { id, .. }
        | Ev::Scrolled { id } | Ev::Net { id, .. } | Ev::CdpResult { id, .. } | Ev::Gone { id, .. } | Ev::Closed { id } | Ev::Texture { id, .. } => *id,
        Ev::Ready { .. } | Ev::Failed { .. } | Ev::Stats { .. } => return None,
    })
}

/// The link closed: asked to (a stop), or a crash. Either way the process
/// is reaped and its PID file goes; on a crash every view hears.
fn gone() {
    let (was, owners) = with(|c| {
        let was = c.state;
        c.state = State::Off;
        c.out = None;
        c.idle_since = None;
        c.stop_at = None;
        let owners: Vec<Owner> = if was == State::Stopping { vec![] } else { c.owners.drain().map(|(_, o)| o).collect() };
        if was != State::Stopping {
            c.crashes += 1;
            if was == State::Starting { c.failures += 1; if c.failures >= 3 { c.available = false; } }
        }
        c.owners.clear();
        (was, owners)
    });
    reap_child();
    drop_pid();
    if was == State::Stopping { eprintln!("[browser] browser host stopped"); return; }
    eprintln!("[browser] the browser host went away: {} view(s) show a crash", owners.len());
    for o in owners {
        match o {
            Owner::Panel(i) => crate::engine::host_gone(i),
            Owner::Frame(k) => crate::frames::host_gone(&k),
        }
    }
}

fn idle() {
    let (state, views) = with(|c| (c.state, c.owners.len()));
    if !matches!(state, State::Starting | State::Up) { return; }
    if views > 0 || KEEP_ALIVE.with(|k| k.get())() { with(|c| c.idle_since = None); return; }
    let since = with(|c| *c.idle_since.get_or_insert_with(Instant::now));
    if since.elapsed() >= idle_grace() { stop(); }
}

/// Ask the host to stop (its views close, CEF shuts down, it leaves).
pub fn stop() {
    if !matches!(state(), State::Starting | State::Up) { return; }
    send(Cmd::Quit);
    with(|c| { c.state = State::Stopping; c.stop_at = Some(Instant::now()); c.owners.clear(); });
    eprintln!("[browser] no web views for a while: stopping the browser host");
}

fn kill_child() {
    if let Some(ch) = with(|c| c.child.as_mut().map(|ch| ch.id())) { crate::proc::kill_tree(ch); }
}

/// Wait for the process off the main thread (it is leaving already).
fn reap_child() {
    if let Some(mut ch) = with(|c| c.child.take()) {
        std::thread::spawn(move || {
            for _ in 0..60 {
                if let Ok(Some(_)) = ch.try_wait() { return; }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = ch.kill();
            let _ = ch.wait();
        });
    }
}

fn drop_pid() {
    let dir = with(|c| c.config_dir.clone());
    if !dir.as_os_str().is_empty() { crate::proc::remove_pid(&dir); }
}

/// The app is closing: the host is asked to stop and given up to 2 s.
pub fn shutdown() {
    if !matches!(state(), State::Starting | State::Up | State::Stopping) { return; }
    send(Cmd::Quit);
    let end = Instant::now() + Duration::from_secs(2);
    let mut child = with(|c| { c.out = None; c.state = State::Off; c.child.take() });
    if let Some(ch) = child.as_mut() {
        while Instant::now() < end { if let Ok(Some(_)) = ch.try_wait() { break; } std::thread::sleep(Duration::from_millis(20)); }
        if let Ok(None) = ch.try_wait() { crate::proc::kill_tree(ch.id()); let _ = ch.wait(); }
    }
    drop_pid();
}

/// The driver's view (props "browserHost").
pub fn props() -> Value {
    with(|c| json!({
        "state": match c.state { State::Off => "off", State::Starting => "starting", State::Up => "up", State::Stopping => "stopping" },
        "pid": if c.state == State::Off { Value::Null } else { json!(c.pid) },
        "views": c.owners.len(), "starts": c.starts, "crashes": c.crashes, "available": c.available,
        "reaped": c.reaped, "idleSecs": idle_grace().as_secs_f64(), "unloadSecs": unload_grace().as_secs_f64(), "error": c.last_error,
        // Frames: taken by the app (all hosts this run), and the running
        // host's counters (sent, not sent as unchanged or hidden, bytes
        // copied into segments, GPU frames); the GPU path's mode.
        "frames": FRAMES.load(Ordering::Relaxed),
        "host": c.stats,
        "gpu": { "asked": crate::gpu::name(crate::gpu::asked()), "mode": crate::gpu::name(crate::gpu::mode()), "textures": TEXTURES.load(Ordering::Relaxed), "textureError": TEXTURE_ERROR.lock().unwrap().clone() },
        "libcef": libcef_mapped(),
    }))
}

/// Is libcef mapped into this (the app's) process? It never should be
/// (only the host links it). Linux reads /proc/self/maps; elsewhere null.
pub fn libcef_mapped() -> Value {
    #[cfg(target_os = "linux")]
    { json!(std::fs::read_to_string("/proc/self/maps").map(|m| m.lines().any(|l| l.contains("libcef"))).unwrap_or(false)) }
    #[cfg(not(target_os = "linux"))]
    { Value::Null }
}
