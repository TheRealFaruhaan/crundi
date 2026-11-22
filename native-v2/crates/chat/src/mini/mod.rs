//! Mini, the chat mascot (F101; app/vendor/crundi-mini.js, attached to each
//! chat as claude-chat.js does): the robot from the logo, which keeps the
//! person company when they are left waiting. Scenes: `queue` (a message
//! not picked up for a while), `sit` (an untouched suggestion), `nudge`
//! (Claude finished a while ago and nothing typed), a parked chat's own,
//! being tapped (annoyed, wary of the cursor, storming off), its eyes on the
//! cursor, the parachute; and it is the logo: the head in the top bar grows a
//! body and walks off before it turns up, and comes back to fold away into
//! the logo. One Mini at a time (HOLDER), and its own outings in the top bar
//! (TOP). Not shown at all where motion is reduced.
//!
//! How it runs here (ARCHITECTURE.md rules 3 and 6): a chat's Mini is drawn
//! by its own cached leaf, `MiniLayer`, over the chat pane's body; the top
//! bar's by `TopLayer`, a window overlay. While an actor is out, one loop
//! steps every actor at most 60 times a second and notifies only the layers
//! that show one; with no actor out there is no loop. The decisions (the
//! web's 400 ms tick) run when what they read changes (the chat's state,
//! queue, suggestion, box, the tab) and at the exact moment a delay elapses
//! (one timer for the nearest deadline): they wake the thread, never draw,
//! unless Mini comes. Where Mini goes, it finds through the spots the views
//! leave (crundi_shell::spots: the queued message, the suggestion, Send,
//! the box, the parked card's buttons; the logo, the bars, the bell, the
//! tabs, the sidebar's rows).

pub mod actor;
pub mod draw;
pub mod scenes;
pub mod view;

use actor::{clamp, rnd, Fx, Mini, Pt, PtFn, Surf, SurfFn, K};
use crundi_shell::spots::SpotsRef;
use gpui::*;
use scenes::{Aims, Env, Parked, Top, TopRead};
use serde_json::{json, Value};
use std::{cell::{Cell, RefCell}, collections::{HashMap, VecDeque}, rc::Rc, time::{Duration, Instant}};
pub use view::{MiniLayer, MiniPanes, TopLayer};

/// The web's DELAY: queue, sit, nudge (ms).
#[derive(Clone, Copy)]
pub struct Delays { pub queue: f64, pub sit: f64, pub nudge: f64 }
pub const DELAY: Delays = Delays { queue: 12000., sit: 25000., nudge: 90000. };
const NUDGE_REST: f64 = 10. * 60000.;
const UPDATE_EVERY: f64 = 4. * 3600000.;
/// The loop's cap (the web steps on every animation frame).
const FRAME: Duration = Duration::from_millis(16);

fn now_ms() -> f64 {
    thread_local! { static T0: Instant = Instant::now(); }
    T0.with(|t| t.elapsed().as_secs_f64() * 1000.) + 1e6
}
fn f(p: Pixels) -> f32 { f32::from(p) }

// ─── What Mini remembers between runs (F180) ───

/// The wall clock, ms (what is kept on disk: `now_ms` starts with the process).
fn wall_ms() -> f64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.).unwrap_or(0.) }

/// Where it is kept: this device's client folder (the web keeps it in the
/// browser's localStorage, crundi_mini_pct and crundi_mini_update_at).
fn memory_file() -> std::path::PathBuf { crundi_core::store::config_dir().join("mini.json") }

/// The limits' last percentages (so a reset that happened while the client
/// was shut is still noticed) and when the update reminder last went out
/// (`now_ms`'s clock; never: -inf, the web's 0, so the first may come at
/// once), as kept on disk.
fn load_memory() -> (HashMap<&'static str, f32>, f64) {
    let v: Value = std::fs::read_to_string(memory_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
    let mut pct = HashMap::new();
    for k in ["five", "week"] { if let Some(n) = v["pct"][k].as_f64() { pct.insert(k, n as f32); } }
    let at = v["updateAt"].as_f64().filter(|w| *w > 0.).map(|w| now_ms() - (wall_ms() - w)).unwrap_or(f64::NEG_INFINITY);
    (pct, at)
}

/// Keep them (written whole, through a temporary file; seldom: a limit's
/// percentage moved by a point, the reminder went out).
fn save_memory(pct: &HashMap<&'static str, f32>, update_at: f64) {
    let v = json!({ "pct": pct, "updateAt": if update_at.is_finite() { wall_ms() - (now_ms() - update_at) } else { 0. } });
    let path = memory_file();
    let _ = std::fs::create_dir_all(crundi_core::store::config_dir());
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, v.to_string()).is_ok() { let _ = std::fs::rename(&tmp, &path); }
}

#[derive(Clone, PartialEq, Debug)]
enum Holder { None, Chat(String), Top }

/// What happens when a behaviour ends (the web's play().then(…)).
#[derive(Clone)]
enum Then {
    /// A chat's exit played: gone, if it is still that actor and leaving.
    ChatGone { id: String, tok: u64 },
    /// Stormed off: gone, and a rest from nudging.
    Storm { id: String, tok: u64 },
    /// Watching the cursor ended (finished or interrupted).
    EyeEnd { id: String },
    /// A parked chat's goodbye played.
    ParkedGone { id: String, tok: u64 },
    /// The logo's job (leave or return) ended.
    Logo,
    /// The top bar outing ended; `cut` when a chat called it away.
    Outing { cut: Rc<Cell<bool>> },
    /// Called away from the top bar: it has walked off.
    CutDone,
    /// Resumed under its feet: it has run off the page.
    HandDone { id: String },
}

#[derive(Clone)]
enum Who { Chat(String, String), Parked(String), Home(String), HandHome(String) }
#[derive(Clone)]
enum LogoJob { Leave(Who), Return(f32, Who) }

/// An actor on the window's layer (the top bar's, a handed-over one).
struct TopActor { mini: Mini, thens: HashMap<u64, Then>, role: &'static str }

#[derive(Default)]
struct TopState {
    busy: bool,
    cut: Option<Rc<Cell<bool>>>,
    next: HashMap<&'static str, f64>,
    seen: HashMap<&'static str, bool>,
    prev: HashMap<&'static str, f32>,
    party_for: String,
    party_at: f64,
    bells: usize,
    bell_due: bool,
    force: Option<String>,
    any_width: bool,
    update_at: f64,
    last_tick: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode { Live, Parked }

/// One chat's watcher (the web's attach()).
pub struct ChatMini {
    store: WeakEntity<crate::store::ChatStore>,
    composer: WeakEntity<crate::composer::Composer>,
    log: WeakEntity<crate::log::LogView>,
    view: Option<WeakEntity<MiniLayer>>,
    /// The layer (the pane's body), window coordinates, as last laid out.
    pub layer: Rc<Cell<Bounds<Pixels>>>,
    mini: Option<Mini>,
    thens: HashMap<u64, Then>,
    fx: Vec<Fx>,
    mode: Mode,
    scene: Option<&'static str>,
    leaving: bool,
    coming: Option<&'static str>,
    asked: Option<&'static str>,
    since: (f64, f64, f64),
    last_state: String,
    nudge_after: f64,
    away_since: Option<f64>,
    taps: Vec<f64>,
    last_kind: i64,
    wary: f64,
    last_flee: f64,
    eyeing: Rc<Cell<bool>>,
    mouse_at: Rc<Cell<Option<(f32, f32)>>>,
    last_sig: Option<(usize, usize, usize)>,
    delay: Delays,
    // A parked chat's (attachParked).
    want_at: f64,
    handing: bool,
    last_at: Option<(f32, f32)>,
    /// Shown by the layer this frame.
    dirty: bool,
}

/// Every Mini (one System per app; its handle in a Global).
pub struct System {
    chats: HashMap<String, ChatMini>,
    top: Vec<TopActor>,
    top_fx: Vec<Fx>,
    holder: Holder,
    logo_away: bool,
    logo_q: VecDeque<LogoJob>,
    logo_run: Option<Who>,
    tops: TopState,
    clock: Rc<Cell<f32>>,
    spots: SpotsRef,
    /// The window's size (the top layer's bounds).
    pub window: Rc<Cell<(f32, f32)>>,
    /// The top layer's height while an actor is on it.
    pub clip_h: Rc<Cell<f32>>,
    tabs_moved: Rc<Cell<bool>>,
    top_view: Option<WeakEntity<TopLayer>>,
    top_dirty: bool,
    /// Mini comes by itself (off under the test driver until `mini-auto`).
    pub auto: bool,
    looping: bool,
    last_frame: Option<Instant>,
    tick_acc: f32,
    timer: Option<(f64, Task<()>)>,
    pending: Vec<Then>,
    /// A logo return queued by `gone`, to start once the App is at hand.
    logo_pending: bool,
    /// Mini is free again: every chat looks again (one may have been waiting).
    retick: bool,
}

pub type SysRef = Rc<RefCell<System>>;
struct SysHandle(SysRef);
impl Global for SysHandle {}

/// The system (made once).
pub fn sys(cx: &mut App) -> SysRef {
    if let Some(h) = cx.try_global::<SysHandle>() { return h.0.clone(); }
    let spots = crundi_shell::spots::spots(cx);
    let s = Rc::new(RefCell::new(System {
        chats: HashMap::new(), top: vec![], top_fx: vec![], holder: Holder::None, logo_away: false, logo_q: VecDeque::new(), logo_run: None,
        tops: { let (prev, update_at) = load_memory(); TopState { prev, update_at, ..TopState::default() } }, clock: Rc::new(Cell::new(0.)), spots, window: Rc::new(Cell::new((1280., 800.))), clip_h: Rc::new(Cell::new(0.)),
        tabs_moved: Rc::new(Cell::new(false)), top_view: None, top_dirty: false,
        auto: std::env::var("CRUNDI_TEST_PORT").is_err(), looping: false, last_frame: None, tick_acc: 0., timer: None, pending: vec![], logo_pending: false, retick: false,
    }));
    cx.set_global(SysHandle(s.clone()));
    s
}

/// The window's layer for the top bar's Mini (lib.rs's init registers it).
pub fn init(cx: &mut App) {
    let s = sys(cx);
    let v = cx.new(|cx| TopLayer::new(s.clone(), cx));
    s.borrow_mut().top_view = Some(v.downgrade());
    // The chat panes' layers first: the top layer draws over them.
    let panes = cx.new(|cx| MiniPanes::new(s.clone(), cx));
    crundi_shell::register_overlay(cx, panes.into());
    crundi_shell::register_overlay(cx, v.into());
    // The top bar's own outings: its look round every 3 s (the web's topTick).
    schedule(&s, cx);
}

/// A chat pane's layer (ChatPane::new).
pub fn attach(id: &str, store: &Entity<crate::store::ChatStore>, composer: &Entity<crate::composer::Composer>, log: &Entity<crate::log::LogView>, cx: &mut App) -> Entity<MiniLayer> {
    let s = sys(cx);
    let layer = Rc::new(Cell::new(Bounds::default()));
    let v = cx.new(|cx| MiniLayer::new(s.clone(), id.to_string(), layer.clone(), store, composer, log, cx));
    let now = now_ms();
    s.borrow_mut().chats.insert(id.to_string(), ChatMini {
        store: store.downgrade(), composer: composer.downgrade(), log: log.downgrade(), view: Some(v.downgrade()), layer,
        mini: None, thens: HashMap::new(), fx: vec![], mode: Mode::Live, scene: None, leaving: false, coming: None, asked: None,
        since: (0., 0., now), last_state: String::new(), nudge_after: 0., away_since: None, taps: vec![], last_kind: -1, wary: 0., last_flee: 0.,
        eyeing: Rc::new(Cell::new(false)), mouse_at: Rc::new(Cell::new(None)), last_sig: None, delay: DELAY,
        want_at: 0., handing: false, last_at: None, dirty: false,
    });
    // MiniPanes draws the new layer (it observes the shown panes).
    let panes = crundi_shell::spots::wb_panes(cx);
    cx.notify(panes.entity_id());
    v
}

/// The chat's pane is gone (the web's destroy()).
pub fn detach(id: &str, cx: &mut App) {
    let Some(h) = cx.try_global::<SysHandle>().map(|h| h.0.clone()) else { return };
    let mut s = h.borrow_mut();
    if !s.chats.contains_key(id) { return; }
    if s.mode_of(id) == Some(Mode::Parked) { s.parked_destroy(id, cx); } else { s.gone(id, false); }
    if s.holder == Holder::Chat(id.to_string()) && !s.chats.get(id).is_some_and(|c| c.handing) { s.holder = Holder::None; }
    s.chats.remove(id);
    s.flush_logo(cx);
    drop(s);
    after_change(&h, cx);
}

/// Something in the chat changed (its state, queue, suggestion, box, the tab).
pub fn changed(id: &str, cx: &mut App) {
    let Some(h) = cx.try_global::<SysHandle>().map(|h| h.0.clone()) else { return };
    let chat_now = h.borrow().chats.contains_key(id);
    if !chat_now { return; }
    h.borrow_mut().tick_chat(id, cx);
    after_change(&h, cx);
}

/// The person did something in the chat (a key, a press, the wheel).
pub fn acted(id: &str, cx: &mut App) {
    let Some(h) = cx.try_global::<SysHandle>().map(|h| h.0.clone()) else { return };
    h.borrow_mut().user_acted(id, cx);
    after_change(&h, cx);
}

/// After a change: the loop runs while an actor is out; the next deadline is set.
fn after_change(s: &SysRef, cx: &mut App) {
    let pending = std::mem::take(&mut s.borrow_mut().pending);
    if !pending.is_empty() { let mut b = s.borrow_mut(); for t in pending { b.run_then(t, cx); } }
    retick(s, cx);
    ensure_loop(s, cx);
    schedule(s, cx);
    notify_views(s, cx);
}

/// Mini came free: each chat looks whether it needs it now.
fn retick(s: &SysRef, cx: &mut App) {
    if !std::mem::take(&mut s.borrow_mut().retick) { return; }
    let ids: Vec<String> = s.borrow().chats.keys().cloned().collect();
    for id in ids { s.borrow_mut().tick_chat(&id, cx); }
    let pending = std::mem::take(&mut s.borrow_mut().pending);
    if !pending.is_empty() { let mut b = s.borrow_mut(); for t in pending { b.run_then(t, cx); } }
}

fn ensure_loop(s: &SysRef, cx: &mut App) {
    {
        let mut b = s.borrow_mut();
        if b.looping || !b.alive() { return; }
        b.looping = true;
        b.last_frame = Some(Instant::now());
    }
    let s2 = s.clone();
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(FRAME).await;
        let more = cx.update(|cx| frame(&s2, cx));
        if !more { break; }
    }).detach();
}

/// One frame: every actor stepped, the behaviours' ends seen to, the
/// chats looked at every 400 ms, the layers showing one notified.
fn frame(s: &SysRef, cx: &mut App) -> bool {
    let dt = {
        let mut b = s.borrow_mut();
        let now = Instant::now();
        let dt = b.last_frame.map_or(0.016, |l| now.duration_since(l).as_secs_f32()).min(0.05);
        b.last_frame = Some(now);
        dt
    };
    {
        let mut b = s.borrow_mut();
        b.step(dt, cx);
        b.tick_acc += dt;
        if b.tick_acc >= 0.4 {
            b.tick_acc = 0.;
            let ids: Vec<String> = b.chats.iter().filter(|(_, c)| c.mini.is_some() || c.scene.is_some()).map(|(k, _)| k.clone()).collect();
            for id in ids { b.tick_chat(&id, cx); }
        }
    }
    let pending = std::mem::take(&mut s.borrow_mut().pending);
    if !pending.is_empty() { let mut b = s.borrow_mut(); for t in pending { b.run_then(t, cx); } }
    retick(s, cx);
    if s.borrow().tabs_moved.replace(false) { if let Some(t) = crundi_shell::spots::tabs(cx) { if let Some(e) = t.bar.upgrade() { cx.notify(e.entity_id()); } } }
    notify_views(s, cx);
    let mut b = s.borrow_mut();
    let more = b.alive();
    if !more { b.looping = false; b.retick = true; drop(b); retick(s, cx); schedule(s, cx); notify_views(s, cx); }
    more
}

fn notify_views(s: &SysRef, cx: &mut App) {
    let (chats, top) = {
        let mut b = s.borrow_mut();
        let chats: Vec<WeakEntity<MiniLayer>> = b.chats.values_mut().filter_map(|c| if std::mem::take(&mut c.dirty) { c.view.clone() } else { None }).collect();
        let top = std::mem::take(&mut b.top_dirty).then(|| b.top_view.clone()).flatten();
        (chats, top)
    };
    // (By id: a layer may be the one whose observer brought us here.)
    for v in chats { cx.notify(v.entity_id()); }
    if let Some(v) = top { cx.notify(v.entity_id()); }
}

/// The next deadline: one timer, at the nearest moment a delay elapses.
fn schedule(s: &SysRef, cx: &mut App) {
    let at = s.borrow().next_deadline();
    let mut b = s.borrow_mut();
    match at {
        None => { b.timer = None; }
        Some(at) => {
            if b.timer.as_ref().is_some_and(|(t, _)| (*t - at).abs() < 1.) { return; }
            let wait = (at - now_ms()).max(0.) as u64 + 1;
            let s2 = s.clone();
            let task = cx.spawn(async move |cx: &mut AsyncApp| {
                cx.background_executor().timer(Duration::from_millis(wait)).await;
                let _ = cx.update(|cx| {
                    s2.borrow_mut().timer = None;
                    let ids: Vec<String> = s2.borrow().chats.keys().cloned().collect();
                    for id in ids { s2.borrow_mut().tick_chat(&id, cx); }
                    s2.borrow_mut().top_tick(cx);
                    after_change(&s2, cx);
                });
            });
            b.timer = Some((at, task));
        }
    }
}

// ─── The system ───

impl System {
    fn alive(&self) -> bool {
        !self.top.is_empty() || !self.top_fx.is_empty() || self.chats.values().any(|c| c.mini.is_some() || !c.fx.is_empty())
    }
    fn mode_of(&self, id: &str) -> Option<Mode> { self.chats.get(id).map(|c| c.mode) }

    fn step(&mut self, dt: f32, cx: &mut App) {
        let clock = self.clock.get() + dt;
        self.clock.set(clock);
        let (ww, wh) = self.window.get();
        for c in self.chats.values_mut() {
            // Only while its pane is in front of the person (rule 6).
            if c.away_since.is_some() { continue; }
            if let Some(m) = c.mini.as_mut() {
                m.step(dt);
                c.fx.append(&mut m.a.borrow_mut().fx_out);
                for t in std::mem::take(&mut m.ended) { if let Some(th) = c.thens.remove(&t) { self.pending.push(th); } }
                c.dirty = true;
            }
            if !c.fx.is_empty() { c.fx.retain(|x| clock - x.born < x.kind.size().2 / 1000. + 0.06); c.dirty = true; }
        }
        let _ = (ww, wh, cx);
        for t in self.top.iter_mut() {
            t.mini.step(dt);
            self.top_fx.append(&mut t.mini.a.borrow_mut().fx_out);
            for tok in std::mem::take(&mut t.mini.ended) { if let Some(th) = t.thens.remove(&tok) { self.pending.push(th); } }
            self.top_dirty = true;
        }
        if !self.top_fx.is_empty() { self.top_fx.retain(|x| clock - x.born < x.kind.size().2 / 1000. + 0.06); self.top_dirty = true; }
    }

    /// When the next decision could change by time alone.
    fn next_deadline(&self) -> Option<f64> {
        let mut at: Option<f64> = None;
        let mut put = |t: f64| at = Some(at.map_or(t, |a: f64| a.min(t)));
        let now = now_ms();
        for c in self.chats.values() {
            // (A moment already past is waiting on something else: a change brings it back.)
            if c.mode == Mode::Parked {
                if c.scene.is_none() && self.auto && c.want_at > now { put(c.want_at); }
                continue;
            }
            if let Some(a) = c.away_since { put(a + 5000.); }
            if c.mini.is_some() || c.scene.is_some() { continue; }
            if !self.auto && c.asked.is_none() { continue; }
            if c.asked.is_some() { put(now + 400.); }
            for d in [c.since.0 + c.delay.queue, c.since.1 + c.delay.sit, (c.since.2 + c.delay.nudge).max(c.nudge_after)] {
                if d > now && d < now + 1e9 { put(d); }
            }
        }
        if self.auto && self.spots.borrow().get("logo").is_some() { put(self.tops.last_tick + 3000.); }
        at.map(|a| a.max(now))
    }

    fn run_then(&mut self, t: Then, cx: &mut App) {
        match t {
            Then::ChatGone { id, tok } => { if self.chats.get(&id).is_some_and(|c| c.leaving && c.mini.as_ref().is_some_and(|m| m.tok == tok)) { self.gone(&id, false); } }
            Then::Storm { id, tok } => {
                if self.chats.get(&id).is_some_and(|c| c.leaving && c.mini.as_ref().is_some_and(|m| m.tok == tok)) {
                    self.gone(&id, false);
                    if let Some(c) = self.chats.get_mut(&id) { c.since.0 = 0.; c.since.1 = 0.; c.since.2 = now_ms(); c.nudge_after = now_ms() + NUDGE_REST; }
                }
            }
            Then::EyeEnd { id } => { if let Some(c) = self.chats.get_mut(&id) { c.eyeing.set(false); c.last_flee = now_ms(); } }
            Then::ParkedGone { id, tok } => { if self.chats.get(&id).is_some_and(|c| c.mini.as_ref().is_some_and(|m| m.tok == tok)) { self.gone(&id, false); } }
            Then::Logo => {
                // The job's actor goes, the logo comes back where it returned.
                let who = self.logo_run.take();
                self.top.retain(|t| t.role != "logo");
                self.top_dirty = true;
                if let Some(Who::Home(_) | Who::HandHome(_)) = &who { self.set_away(false, cx); }
                if let Some(w) = who { self.arrived(w, cx); }
                self.pump_logo(cx);
            }
            Then::Outing { cut } => {
                if cut.get() { return; }
                self.set_away(false, cx);
                self.free_top();
            }
            Then::CutDone => self.free_top(),
            Then::HandDone { id } => {
                self.top.retain(|t| t.role != "hand");
                self.top_dirty = true;
                if let Some(c) = self.chats.get_mut(&id) { c.handing = false; }
                self.logo_return(215., Who::HandHome(id), cx);
            }
        }
        self.flush_logo(cx);
    }

    // ─── It is the logo ───

    fn set_away(&mut self, away: bool, cx: &mut App) { self.logo_away = away; crundi_shell::spots::set_logo_away(cx, away); }

    /// The logo's head as the web's .cb-headmark: (left, top, width, height).
    fn logo_mark(&self) -> Option<(f32, f32, f32, f32)> {
        let b = self.spots.borrow().get("logo")?;
        // The mark's head is its first T::BRAND_HEAD square, centred.
        let size = f32::from(crundi_theme::T::BRAND_HEAD);
        let top = f(b.top()) + (f(b.size.height) - size) / 2.;
        (size >= 8.).then_some((f(b.left()), top, size, size))
    }
    /// A stage on the window's layer for the logo's actor (the web's logoStage).
    fn logo_stage(&mut self, role: &'static str) -> Option<(Mini, (f32, f32), f32)> {
        let (l, t, w, h) = self.logo_mark()?;
        let k = (w * 74. / 96.) / 50.;
        let tb = self.spots.borrow().get("tabbar").map_or(0., |b| f(b.bottom()));
        self.clip_h.set((t + h + 30.).max(tb + 26.).ceil());
        let mut m = Mini::new(92. * k, self.clock.clone());
        let home = (l + w * 11. / 96. + 25. * k, t + h * 26. / 96. + 78. * k);
        let ww = self.window.get().0;
        let edge = if home.0 < ww / 2. { -46. } else { ww + 46. };
        {
            let mut a = m.a.borrow_mut();
            a.clamp_x = false;
            let hy = home.1;
            a.surf = Some(Rc::new(move || Some(Surf { y: hy, l: -200., r: ww + 200. })));
            a.base_y = home.1;
        }
        let _ = (&mut m, role);
        Some((m, home, edge))
    }
    fn logo_leave(&mut self, who: Who, cx: &mut App) { self.logo_q.push_back(LogoJob::Leave(who)); self.pump_logo(cx); }
    fn logo_return(&mut self, sp: f32, who: Who, cx: &mut App) { self.logo_q.push_back(LogoJob::Return(sp, who)); self.pump_logo(cx); }
    fn pump_logo(&mut self, cx: &mut App) {
        while self.logo_run.is_none() {
            let Some(job) = self.logo_q.pop_front() else { return };
            match job {
                LogoJob::Leave(who) => {
                    if self.logo_mark().is_none() || self.logo_away { self.arrived(who, cx); continue; }
                    let Some((mut m, home, edge)) = self.logo_stage("logo") else { self.arrived(who, cx); continue };
                    { let mut a = m.a.borrow_mut(); a.x = home.0; a.v[K::grow as usize] = 0.; a.t[K::grow as usize] = 0.; a.v[K::alpha as usize] = 1.; a.t[K::alpha as usize] = 1.; }
                    self.set_away(true, cx);
                    let tok = m.play(move |s| async move { scenes::logo_leave(s, edge).await });
                    self.push_top(m, tok, Then::Logo, "logo");
                    self.logo_run = Some(who);
                }
                LogoJob::Return(sp, who) => {
                    if !self.logo_away { self.arrived(who, cx); continue; }
                    let Some((mut m, home, edge)) = self.logo_stage("logo") else { self.set_away(false, cx); self.arrived(who, cx); continue };
                    let sp = clamp(if sp > 0. { sp } else { 62. }, 40., 230.);
                    { let mut a = m.a.borrow_mut(); a.x = edge; a.v[K::alpha as usize] = 1.; a.t[K::alpha as usize] = 1.; }
                    let tok = m.play(move |s| async move { scenes::logo_return(s, home.0, sp).await });
                    self.push_top(m, tok, Then::Logo, "logo");
                    self.logo_run = Some(who);
                }
            }
        }
    }
    fn push_top(&mut self, mini: Mini, tok: u64, then: Then, role: &'static str) {
        let mut thens = HashMap::new();
        thens.insert(tok, then);
        self.top.push(TopActor { mini, thens, role });
        self.top_dirty = true;
    }
    /// The logo's job is done: what was waiting on it goes on.
    fn arrived(&mut self, who: Who, cx: &mut App) {
        match who {
            Who::Chat(id, name) => {
                let ok = self.chats.get(&id).is_some_and(|c| c.coming.is_some_and(|n| n == name));
                if !ok { return; }
                if let Some(c) = self.chats.get_mut(&id) { c.coming = None; c.scene = None; }
                let name: &'static str = match name.as_str() { "queue" => "queue", "sit" => "sit", _ => "nudge" };
                if !self.wanted(&id, name, cx) { self.gone(&id, false); return; }
                self.start2(&id, name, cx);
            }
            Who::Parked(id) => self.parked_arrived(&id, cx),
            Who::Home(id) => { if self.holder == Holder::Chat(id.clone()) && self.chats.get(&id).is_none_or(|c| c.scene.is_none()) { self.holder = Holder::None; self.retick = true; } }
            Who::HandHome(id) => { if self.holder == Holder::Chat(id) { self.holder = Holder::None; self.retick = true; } }
        }
    }

    // ─── A chat ───

    /// The layer-relative box of a spot (None: not drawn).
    fn bx(spots: &SpotsRef, layer: &Rc<Cell<Bounds<Pixels>>>, name: &str) -> Option<(f32, f32, f32, f32)> {
        let b = spots.borrow().get(name)?;
        let o = layer.get().origin;
        if f(b.size.width) == 0. && f(b.size.height) == 0. { return None; }
        Some((f(b.left() - o.x), f(b.right() - o.x), f(b.top() - o.y), f(b.bottom() - o.y)))
    }
    fn width(&self, id: &str) -> f32 { self.chats.get(id).map_or(300., |c| { let w = f(c.layer.get().size.width); if w > 0. { w } else { 300. } }) }
    fn mobile(&self, id: &str, cx: &App) -> bool {
        let narrow = self.chats.get(id).and_then(|c| c.composer.upgrade()).is_some_and(|c| c.read(cx).narrow());
        narrow || self.width(id) < 520.
    }
    fn badge_fn(&self, id: &str) -> Rc<dyn Fn() -> Option<(f32, f32, f32, f32)>> {
        let (sp, ly, id) = (self.spots.clone(), self.chats[id].layer.clone(), id.to_string());
        Rc::new(move || {
            let mut out: Option<(f32, f32, f32, f32)> = None;
            for n in ["badge-cmd", "badge-ag"] {
                if let Some(b) = Self::bx(&sp, &ly, &format!("{n}:{id}")) {
                    out = Some(match out { None => (b.0, b.1, (b.2 + b.3) / 2., 0.), Some(o) => (o.0.min(b.0), o.1.max(b.1), (b.2 + b.3) / 2., 0.) });
                }
            }
            out.map(|o| (o.0, o.1, (o.0 + o.1) / 2., o.2))
        })
    }
    /// The activity row's words (log/activity.rs): the row's left + 8, the
    /// words' right, the row's middle and top (the web's env.activity).
    fn activity_fn(&self, id: &str) -> Rc<dyn Fn() -> Option<(f32, f32, f32, f32)>> {
        let (sp, ly, id) = (self.spots.clone(), self.chats[id].layer.clone(), id.to_string());
        Rc::new(move || {
            let r = Self::bx(&sp, &ly, &format!("activity:{id}"))?;
            let words = Self::bx(&sp, &ly, &format!("activity-w:{id}")).map_or(r.0 + 90., |w| w.1);
            Some((r.0 + 8., words, (r.2 + r.3) / 2., r.2))
        })
    }
    fn env(&self, id: &str) -> Env { Env { activity: self.activity_fn(id), badge: self.badge_fn(id) } }
    fn top_of(&self, id: &str, name: String, pl: f32, pr: f32) -> SurfFn {
        let (sp, ly) = (self.spots.clone(), self.chats[id].layer.clone());
        Rc::new(move || Self::bx(&sp, &ly, &name).map(|b| Surf { y: b.2, l: b.0 + pl, r: b.1 - pr }))
    }
    fn clear_of_badges(&self, id: &str, f0: SurfFn) -> SurfFn {
        let (badge, act) = (self.badge_fn(id), self.activity_fn(id));
        Rc::new(move || {
            let mut sf = f0()?;
            if let Some(b) = badge() { if b.0 - 12. > sf.l + 40. { sf.r = sf.r.min(b.0 - 12.); } }
            // Not on top of the words "Working…" either, when they are down
            // at its level and there is room beside them.
            if let Some(w) = act() { if w.3 > sf.y - 60. && sf.r - (w.1 + 26.) > 70. { sf.l = sf.l.max(w.1 + 26.); } }
            Some(sf)
        })
    }
    fn nudge_surf(&self, id: &str, mobile: bool) -> SurfFn {
        if mobile { self.top_of(id, format!("send:{id}"), 14., 14.) } else { self.top_of(id, format!("composer:{id}"), 16., 16.) }
    }
    fn pt(&self, id: &str, name: String, how: fn((f32, f32, f32, f32)) -> Pt) -> PtFn {
        let (sp, ly) = (self.spots.clone(), self.chats[id].layer.clone());
        Rc::new(move || Self::bx(&sp, &ly, &name).map(how))
    }
    fn aims(&self, id: &str, mobile: bool) -> Aims {
        Aims {
            aim: self.pt(id, format!("input:{id}"), |b| Pt { x: b.0 + 90f32.min((b.1 - b.0) * 0.3), y: (b.2 + b.3) / 2. }),
            send_at: (!mobile).then(|| self.pt(id, format!("send:{id}"), |b| Pt { x: (b.0 + b.1) / 2., y: (b.2 + b.3) / 2. })),
            sug_at: self.pt(id, format!("sug:{id}"), |b| Pt { x: b.0 + 40f32.min((b.1 - b.0) / 2.), y: (b.2 + b.3) / 2. }),
        }
    }
    fn has(&self, name: &str) -> bool { self.spots.borrow().get(name).is_some() }

    /// What the chat reads now: its state, a queue, a suggestion, typed text, the queue's signature.
    fn read(&self, id: &str, cx: &App) -> Option<(String, bool, bool, bool, (usize, usize, usize))> {
        let c = self.chats.get(id)?;
        let comp = c.composer.upgrade()?;
        let store = c.store.upgrade()?;
        let comp = comp.read(cx);
        let st = comp.state_now();
        let sig = crate::composer::queue::sig(comp);
        let has_q = sig.0 > 0;
        let has_s = crate::log::suggest::shown(store.read(cx));
        let typed = !comp.text(cx).trim().is_empty();
        Some((st, has_q, has_s, typed, sig))
    }
    /// The pane is in front of the person: the workbench shown, the pane laid out, mostly in the window.
    fn seen(&self, id: &str, cx: &App) -> bool {
        let Some(c) = self.chats.get(id) else { return false };
        if crundi_shell::stores(cx).nav.read(cx).tab != "workbench" { return false; }
        in_view(c.layer.get(), self.window.get())
    }
    fn parked_now(&self, id: &str, cx: &App) -> bool {
        self.chats.get(id).and_then(|c| c.log.upgrade()).is_some_and(|l| { let l = l.read(cx); l.ui.parked.parked && !l.ui.parked.peek })
    }

    fn wanted(&self, id: &str, name: &str, cx: &App) -> bool {
        let Some((st, has_q, has_s, typed, _)) = self.read(id, cx) else { return false };
        match name { "queue" => has_q, "sit" => has_s, _ => st == "idle" && !typed }
    }

    /// Gone from the chat: home to the top bar at the pace it left with.
    fn gone(&mut self, id: &str, quiet: bool) {
        let Some(c) = self.chats.get_mut(id) else { return };
        let sp = c.mini.as_ref().map_or(0., |m| m.a.borrow().last_speed);
        // (Its layer redraws only if it showed something.)
        if c.mini.take().is_some() { c.dirty = true; }
        c.thens.clear(); c.scene = None; c.leaving = false; c.coming = None; c.eyeing.set(false);
        if self.holder != Holder::Chat(id.to_string()) { return; }
        if quiet { self.holder = Holder::None; return; }
        // (The logo's return needs the App: queued, run at once by the caller's pump.)
        self.logo_q.push_back(LogoJob::Return(sp, Who::Home(id.to_string())));
        self.logo_pending = true;
    }

    fn start(&mut self, id: &str, name: &'static str, cx: &mut App) {
        if cx.reduce_motion() { return; }
        if self.holder == Holder::Top { self.top_cut(); return; }
        if !matches!(&self.holder, Holder::None) && self.holder != Holder::Chat(id.to_string()) { return; }
        self.holder = Holder::Chat(id.to_string());
        let Some(c) = self.chats.get_mut(id) else { return };
        if c.mini.is_none() && c.coming != Some(name) {
            c.coming = Some(name); c.scene = Some("coming");
            self.logo_leave(Who::Chat(id.to_string(), name.to_string()), cx);
            return;
        }
        self.start2(id, name, cx);
    }

    fn ensure(&mut self, id: &str, cx: &App) {
        let mobile = self.mobile(id, cx);
        let clock = self.clock.clone();
        let c = self.chats.get_mut(id).unwrap();
        if c.mini.is_none() { c.mini = Some(Mini::new(if mobile { 50. } else { 54. }, clock)); c.thens.clear(); }
    }

    fn play_chat<Fut: std::future::Future<Output = ()> + 'static>(&mut self, id: &str, then: Option<fn(String, u64) -> Then>, f: impl FnOnce(actor::S) -> Fut) -> u64 {
        let c = self.chats.get_mut(id).unwrap();
        let m = c.mini.as_mut().unwrap();
        let tok = m.play(f);
        if let Some(t) = then { c.thens.insert(tok, t(id.to_string(), tok)); }
        c.dirty = true;
        tok
    }

    fn start2(&mut self, id: &str, name: &'static str, cx: &mut App) {
        self.ensure(id, cx);
        let w = self.width(id);
        let mobile = self.mobile(id, cx);
        let surf = match name {
            "queue" => self.clear_of_badges(id, self.top_of(id, format!("queue:{id}"), 12., 12.)),
            "sit" => self.top_of(id, format!("sug:{id}"), 0., 0.),
            _ => self.clear_of_badges(id, self.nudge_surf(id, mobile)),
        };
        let (env, aims) = (self.env(id), self.aims(id, mobile));
        {
            let c = self.chats.get_mut(id).unwrap();
            c.asked = None; c.scene = Some(name); c.leaving = false;
            let m = c.mini.as_mut().unwrap();
            let mut a = m.a.borrow_mut();
            a.auto_blink = true; a.surf = Some(surf);
        }
        match name {
            "queue" => { self.play_chat(id, None, move |s| scenes::scene_queue(s, w, env)); }
            "sit" => { self.play_chat(id, None, move |s| scenes::scene_sit(s, w)); }
            _ => { self.play_chat(id, None, move |s| scenes::scene_nudge(s, w, mobile, aims)); }
        }
    }

    /// From the seat on the suggestion down to the box, without leaving.
    fn climb_down(&mut self, id: &str, cx: &mut App) {
        let (w, mob) = (self.width(id), self.mobile(id, cx));
        let surf = self.clear_of_badges(id, self.nudge_surf(id, mob));
        let aims = self.aims(id, mob);
        let c = self.chats.get_mut(id).unwrap();
        c.scene = Some("nudge");
        let m = c.mini.as_mut().unwrap();
        let held = { let mut a = m.a.borrow_mut(); let h = Surf { y: a.base_y, l: -100., r: w + 100. }; a.surf = Some(Rc::new(move || Some(h))); a.tx = None; a.auto_blink = true; h };
        self.play_chat(id, None, move |s| async move {
            s.rest(&[]);
            s.pose(crate::p![sit: 0, swing: 0]); s.wait(650.).await;
            s.pose(crate::p![headY: 5, eyeY: 4.5]); s.wait(600.).await;             // a long way down
            s.pose(crate::p![headY: 0, eyeY: 0, armL: 120, armR: 120]);
            s.wait(220.).await;
            let sf2 = surf.clone();
            s.fall_to(Rc::new(move || Some(sf2().map_or(held, |f| Surf { y: f.y, l: -100., r: w + 100. })))).await;
            s.pose(crate::p![armL: 0, armR: 0]); s.wait(360.).await;
            s.with(|a| a.surf = Some(surf.clone()));
            if let Some(sf) = surf() { s.walk_to(if mob { sf.l + (sf.r - sf.l) * 0.5 } else { sf.r - 70. }, 60.).await; }
            scenes::wave(&s, 2).await;
            scenes::nudge_loop(s, 0, aims).await;
        });
    }

    fn leave(&mut self, id: &str, fall: bool, _cx: &mut App) {
        let w = self.width(id);
        let Some(c) = self.chats.get_mut(id) else { return };
        if c.mini.is_none() || c.leaving { return; }
        c.leaving = true;
        let was = c.scene;
        let m = c.mini.as_ref().unwrap();
        let held = { let a = m.a.borrow(); Surf { y: a.base_y, l: -100., r: w + 100. } };
        if fall {
            let ground = if was == Some("sit") { self.top_of(id, format!("composer:{id}"), 10., 10.) } else {
                let (sp, ly, id2) = (self.spots.clone(), self.chats[id].layer.clone(), id.to_string());
                Rc::new(move || Self::bx(&sp, &ly, &format!("inrow:{id2}")).or_else(|| Self::bx(&sp, &ly, &format!("composer:{id2}"))).map(|b| Surf { y: b.2, l: b.0 + 10., r: b.1 - 10. })) as SurfFn
            };
            self.chats.get_mut(id).unwrap().mini.as_ref().unwrap().a.borrow_mut().surf = Some(Rc::new(move || Some(held)));
            let g: SurfFn = Rc::new(move || Some(ground().unwrap_or(held)));
            self.play_chat(id, Some(|id, tok| Then::ChatGone { id, tok }), move |s| scenes::scene_fall(s, w, g));
        } else {
            let cur = self.chats.get_mut(id).unwrap().mini.as_ref().unwrap().a.borrow().surf.clone();
            let f2: SurfFn = Rc::new(move || cur.as_ref().and_then(|f| f()).or(Some(held)));
            self.chats.get_mut(id).unwrap().mini.as_ref().unwrap().a.borrow_mut().surf = Some(f2);
            self.play_chat(id, Some(|id, tok| Then::ChatGone { id, tok }), move |s| scenes::scene_bye(s, w));
        }
    }

    /// Back to what it was doing (the web's resume()).
    fn resume_fn(&self, id: &str, was: &'static str, cx: &App) -> impl FnOnce(actor::S) -> scenes::Boxed + use<> {
        let env = self.env(id);
        let mob = self.mobile(id, cx);
        let aims = self.aims(id, mob);
        move |s: actor::S| -> scenes::Boxed {
            Box::pin(async move {
                if was != "sit" && s.t(K::sit) > 0.5 { s.pose(crate::p![sit: 0, swing: 0, armL: 0, armR: 0]); s.wait(650.).await; }
                match was {
                    "queue" => scenes::queue_loop(s, env).await,
                    "sit" => {
                        if s.t(K::sit) < 0.5 { scenes::take_seat(&s).await; }
                        s.pose(crate::p![sit: 1, swing: 1, armL: 10, armR: 10]);
                        scenes::sit_loop(s).await
                    }
                    _ => scenes::nudge_loop(s, 1 + (actor::random() * 7.) as usize, aims).await,
                }
            })
        }
    }

    /// The pointer moved over the chat (the web's moved()).
    pub fn moved(&mut self, id: &str, p: Option<(f32, f32)>, cx: &mut App) {
        let Some(was) = self.chats.get(id).filter(|c| c.mini.is_some()).and_then(|c| c.scene) else { return };
        let mouse = self.mouse_fn(id);
        let resume = self.resume_fn(id, if was == "coming" { "nudge" } else { was }, cx);
        let Some(c) = self.chats.get_mut(id) else { return };
        let Some(m) = c.mini.as_ref() else { return };
        let Some((mx, my)) = p else { c.mouse_at.set(None); let mut a = m.a.borrow_mut(); a.look_tx = 0.; a.look_ty = 0.; return };
        let (d, air, lid_t) = {
            let mut a = m.a.borrow_mut();
            let (cx_, cy_) = (a.x, a.base_y + a.y_off - 30. * a.k);
            let (ddx, ddy) = (mx - cx_, my - cy_);
            let d = (ddx * ddx + ddy * ddy).sqrt();
            let near = clamp(1. - (d - 60.) / 220., 0., 1.);
            let n = if d == 0. { 1. } else { d };
            a.look_tx = ddx / n * 3.2 * near; a.look_ty = ddy / n * 3. * near;
            (d, a.air, a.t[K::lid as usize])
        };
        c.mouse_at.set(Some((mx, my)));
        if c.leaving || air || c.eyeing.get() { return; }
        let t = now_ms();
        if c.wary > 0. && t - c.wary > 60000. { c.wary = 0.; }
        if c.wary == 0. || d > 105. || t - c.last_flee < 900. || lid_t > 0.8 { return; }
        c.eyeing.set(true);
        m.a.borrow_mut().auto_blink = true;
        let eyeing = c.eyeing.clone();
        self.play_chat(id, Some(|id, _| Then::EyeEnd { id }), move |s| async move {
            s.rest(&[K::sit]);
            scenes::eye_it(&s, mouse).await;
            eyeing.set(false);
            s.rest(&[]);
            s.wait(400.).await;
            resume(s).await;
        });
        let _ = cx;
    }
    fn mouse_fn(&self, id: &str) -> scenes::MouseFn {
        let c = &self.chats[id];
        let (at, a) = (c.mouse_at.clone(), c.mini.as_ref().map(|m| m.a.clone()));
        Rc::new(move || {
            let (mx, my) = at.get()?;
            let a = a.as_ref()?.borrow();
            let (ddx, ddy) = (mx - a.x, my - (a.base_y + a.y_off - 30. * a.k));
            Some((mx, (ddx * ddx + ddy * ddy).sqrt()))
        })
    }

    /// A tap on the robot (the web's tapped()).
    pub fn tapped(&mut self, id: &str, cx: &mut App) {
        let Some(c) = self.chats.get_mut(id) else { return };
        if c.mini.is_none() || c.leaving || c.scene.is_none() || c.scene == Some("coming") { return; }
        if c.mode == Mode::Parked { return self.parked_tapped(id, cx); }
        let w = self.width(id);
        let c = self.chats.get_mut(id).unwrap();
        let was = c.scene.unwrap();
        let t = now_ms();
        c.taps.retain(|x| t - x < 16000.); c.taps.push(t);
        c.wary = t; c.last_flee = t;
        let m = c.mini.as_ref().unwrap();
        if c.taps.len() >= 4 {                                                 // it has had enough of you
            c.taps.clear(); c.leaving = true;
            let held = { let a = m.a.borrow(); Surf { y: a.base_y, l: -100., r: w + 100. } };
            let cur = m.a.borrow().surf.clone();
            m.a.borrow_mut().surf = Some(Rc::new(move || cur.as_ref().and_then(|f| f()).or(Some(held))));
            self.play_chat(id, Some(|id, tok| Then::Storm { id, tok }), move |s| scenes::storm_off(s, w));
            return;
        }
        let (asleep, seated) = { let a = m.a.borrow(); (a.t[K::lid as usize] > 0.8, a.t[K::sit as usize] > 0.5) };
        let n = if seated { scenes::SEATED_N } else { scenes::ANNOYED_N };
        let mut k = (actor::random() * n as f32) as i64;
        if k == c.last_kind { k = (k + 1) % n as i64; }
        c.last_kind = k;
        { let mut a = m.a.borrow_mut(); a.tx = None; a.auto_blink = true; }
        let resume = self.resume_fn(id, was, cx);
        self.play_chat(id, None, move |s| async move {
            s.rest(&[K::lid]);
            if asleep { scenes::grumpy_wake(&s).await; }
            else if seated { scenes::annoyed_seated(&s, k as usize).await; }
            else { scenes::annoyed(&s, k as usize, w).await; }
            // However it took it, it gets to its feet afterwards.
            if s.t(K::sit) > 0.5 {
                s.pose(crate::p![sit: 0, swing: 0, lid: 0.4, armL: -34, armR: -34, turn: 0, headRot: 0]);
                s.wait(700.).await;
                scenes::tap_foot(&s).await;
            }
            s.rest(&[]);
            s.wait(500.).await;
            resume(s).await;
        });
    }

    /// The person did something (not to Mini): the nudge's clock starts again.
    fn user_acted(&mut self, id: &str, cx: &mut App) {
        let Some(c) = self.chats.get_mut(id) else { return };
        if c.mode == Mode::Parked {
            c.want_at = now_ms() + rnd(60000., 180000.) as f64;
            if c.scene == Some("parked") { self.parked_leave(id); }
            return;
        }
        c.since.2 = now_ms();
        if c.scene == Some("nudge") { self.leave(id, false, cx); }
    }

    /// The web's tick(), for one chat.
    fn tick_chat(&mut self, id: &str, cx: &mut App) {
        // Parked or live: a change of mode is a new attach.
        let parked = self.parked_now(id, cx);
        let mode = self.mode_of(id);
        if mode == Some(Mode::Live) && parked {
            self.gone(id, false);
            if self.holder == Holder::Chat(id.to_string()) && self.chats[id].scene.is_none() && !self.logo_pending { self.holder = Holder::None; }
            let c = self.chats.get_mut(id).unwrap();
            c.mode = Mode::Parked; c.want_at = now_ms() + rnd(35000., 110000.) as f64;
        } else if mode == Some(Mode::Parked) && !parked {
            self.parked_destroy(id, cx);
            let c = self.chats.get_mut(id).unwrap();
            c.mode = Mode::Live; c.since = (0., 0., now_ms()); c.last_state.clear();
        }
        self.flush_logo(cx);
        if self.mode_of(id) == Some(Mode::Parked) { return self.parked_tick(id, cx); }
        let Some((st, has_q, has_s, typed, sig)) = self.read(id, cx) else { return };
        let t = now_ms();
        let seen = self.seen(id, cx);
        let auto = self.auto;
        let mob = self.mobile(id, cx);
        let nudge_ok = self.nudge_surf(id, mob)().is_some();
        let c = self.chats.get_mut(id).unwrap();
        if st != c.last_state {
            if st == "working" { c.nudge_after = 0.; }
            if st == "idle" { c.since.2 = t; }
            c.last_state = st.clone();
        }
        // Only in a pane in front of the person: away for 5 s, it goes home.
        if c.scene.is_some() && c.scene != Some("coming") && !c.leaving {
            if seen { c.away_since = None; } else if c.away_since.is_none() { c.away_since = Some(t); }
            if c.away_since.is_some_and(|a| t - a > 5000.) { c.away_since = None; self.gone(id, false); self.flush_logo(cx); return; }
        } else { c.away_since = None; }
        if c.scene == Some("nudge") && !c.leaving && c.mini.as_ref().is_some_and(|m| m.a.borrow().finished) { self.leave(id, false, cx); return; }
        c.since.0 = if has_q { if c.since.0 > 0. { c.since.0 } else { t } } else { 0. };
        c.since.1 = if has_s { if c.since.1 > 0. { c.since.1 } else { t } } else { 0. };
        let air = c.mini.as_ref().is_some_and(|m| m.a.borrow().air);
        let may_nudge = t >= c.nudge_after;
        if c.scene == Some("queue") && !has_q { self.leave(id, true, cx); }
        else if c.scene == Some("sit") && !has_s {
            if !c.leaving && st == "idle" { c.since.2 = c.since.2.min(t - c.delay.nudge + c.delay.nudge.min(30000.)); }
            self.leave(id, true, cx);
        }
        else if c.scene == Some("nudge") && (st != "idle" || typed || has_q) { self.leave(id, false, cx); }
        else if c.scene == Some("sit") && !c.leaving && c.mini.is_some() && !air && st == "idle" && may_nudge && !typed && c.since.2 > 0. && t - c.since.2 >= c.delay.nudge && nudge_ok {
            c.nudge_after = t + NUDGE_REST;
            self.climb_down(id, cx);
        }
        let c = self.chats.get_mut(id).unwrap();
        // More was queued while it stood there: it notices.
        if c.scene == Some("queue") && !c.leaving && c.mini.is_some() && !air && sig.0 > 0 && c.last_sig.is_some_and(|l| sig.0 > l.0 || sig.1 > l.1 + 2) {
            let cnt = sig.2;
            c.mini.as_ref().unwrap().a.borrow_mut().auto_blink = true;
            let env = self.env(id);
            self.play_chat(id, None, move |s| async move { s.rest(&[]); scenes::another(&s, cnt).await; s.rest(&[]); s.wait(500.).await; scenes::queue_loop(s, env).await; });
        }
        let c = self.chats.get_mut(id).unwrap();
        c.last_sig = Some(sig);
        if c.scene.is_some() { return; }
        if !matches!(self.holder, Holder::None | Holder::Top) && self.holder != Holder::Chat(id.to_string()) { return; }
        if let Some(a) = c.asked { self.start(id, a, cx); self.flush_logo(cx); return; }
        if !seen || !auto { return; }
        let d = c.delay;
        if has_q && t - c.since.0 >= d.queue { self.start(id, "queue", cx); }
        else if has_s && st == "idle" && t - c.since.1 >= d.sit { self.start(id, "sit", cx); }
        else if !has_q && st == "idle" && may_nudge && !typed && c.since.2 > 0. && t - c.since.2 >= d.nudge && nudge_ok {
            c.nudge_after = t + NUDGE_REST;
            self.start(id, "nudge", cx);
        }
        self.flush_logo(cx);
    }

    /// The test driver's trigger (the web's trigger(name)).
    fn trigger(&mut self, id: &str, name: &'static str, cx: &mut App) {
        if self.mode_of(id) == Some(Mode::Parked) { if let Some(c) = self.chats.get_mut(id) { c.want_at = 0.; } self.parked_tick_forced(id, cx); return; }
        if self.chats.get(id).is_some_and(|c| c.scene.is_some()) { self.gone(id, true); }
        let sig = self.read(id, cx).map(|r| r.4);
        let Some(c) = self.chats.get_mut(id) else { return };
        c.last_sig = sig; c.asked = Some(name);
        self.start(id, name, cx);
        self.flush_logo(cx);
    }

    // ─── A sleeping chat (attachParked) ───

    fn parked_get(&self, id: &str) -> Parked {
        Parked {
            resume: self.pt(id, format!("pk-resume:{id}"), |b| Pt { x: (b.0 + b.1) / 2., y: (b.2 + b.3) / 2. }),
            compact: self.pt(id, format!("pk-compact:{id}"), |b| Pt { x: (b.0 + b.1) / 2., y: (b.2 + b.3) / 2. }),
            compact_surf: self.top_of(id, format!("pk-compact:{id}"), 6., 6.),
        }
    }
    fn resume_ok(&self, id: &str, cx: &App) -> bool {
        let busy = self.chats.get(id).and_then(|c| c.store.upgrade()).is_some_and(|s| !s.read(cx).log.park_busy.is_empty());
        self.has(&format!("pk-resume:{id}")) && !busy
    }
    fn parked_tick_forced(&mut self, id: &str, cx: &mut App) { let a = self.auto; self.auto = true; self.parked_tick(id, cx); self.auto = a; }
    fn parked_tick(&mut self, id: &str, cx: &mut App) {
        let ok = self.resume_ok(id, cx);
        let veil = self.spots.borrow().get(&format!("pk-veil:{id}"));
        let seen = crundi_shell::stores(cx).nav.read(cx).tab == "workbench" && veil.is_some_and(|v| in_view(v, self.window.get()));
        let auto = self.auto;
        let held = self.holder != Holder::None;
        let c = self.chats.get_mut(id).unwrap();
        if let Some(m) = &c.mini { let a = m.a.borrow(); let o = c.layer.get().origin; c.last_at = Some((f(o.x) + a.x, f(o.y) + a.base_y + a.y_off)); }
        if c.scene == Some("parked") && !c.leaving && (!ok || veil.is_none()) { self.hand_over(id, cx); return; }
        if c.scene == Some("parked") && !c.leaving && c.mini.as_ref().is_some_and(|m| m.a.borrow().finished) { self.parked_leave(id); return; }
        let t = now_ms();
        if c.scene == Some("parked") && !c.leaving {
            if seen { c.away_since = None; } else if c.away_since.is_none() { c.away_since = Some(t); }
            if c.away_since.is_some_and(|a| t - a > 5000.) { c.away_since = None; self.gone(id, false); self.flush_logo(cx); return; }
        }
        if c.scene.is_some() || !ok || !seen || held || t < c.want_at || !auto || cx.reduce_motion() { return; }
        self.holder = Holder::Chat(id.to_string());
        c.scene = Some("coming");
        self.logo_leave(Who::Parked(id.to_string()), cx);
    }
    fn parked_arrived(&mut self, id: &str, cx: &mut App) {
        if !self.chats.contains_key(id) || self.mode_of(id) != Some(Mode::Parked) { return; }
        if !self.has(&format!("pk-resume:{id}")) { if let Some(c) = self.chats.get_mut(id) { c.scene = None; } self.gone(id, false); self.flush_logo(cx); return; }
        let clock = self.clock.clone();
        let (w, get, surf) = (self.width(id), self.parked_get(id), self.top_of(id, format!("pk-resume:{id}"), 6., 6.));
        let c = self.chats.get_mut(id).unwrap();
        let m = Mini::new(50., clock);
        m.a.borrow_mut().surf = Some(surf);
        c.mini = Some(m); c.thens.clear();
        c.scene = Some("parked"); c.leaving = false;
        self.play_chat(id, None, move |s| scenes::scene_parked(s, w, get));
        self.chats.get_mut(id).unwrap().want_at = now_ms() + 15. * 60000.;
    }
    fn parked_leave(&mut self, id: &str) {
        let w = self.width(id);
        let Some(c) = self.chats.get_mut(id) else { return };
        let Some(m) = c.mini.as_ref() else { return };
        if c.leaving { return; }
        c.leaving = true;
        let held = { let a = m.a.borrow(); Surf { y: a.base_y, l: -100., r: w + 100. } };
        m.a.borrow_mut().surf = Some(Rc::new(move || Some(held)));
        self.play_chat(id, Some(|id, tok| Then::ParkedGone { id, tok }), move |s| scenes::scene_bye(s, w));
    }
    fn parked_tapped(&mut self, id: &str, _cx: &mut App) {
        let w = self.width(id);
        let c = self.chats.get_mut(id).unwrap();
        if c.leaving || c.scene != Some("parked") { return; }
        let which = actor::pick(&[0usize, 1, 3]);
        { let m = c.mini.as_ref().unwrap(); let mut a = m.a.borrow_mut(); a.tx = None; a.auto_blink = true; }
        let id2 = id.to_string();
        let tok = self.play_chat(id, Some(|id, tok| Then::ParkedGone { id, tok }), move |s| async move {
            s.rest(&[]); s.pose(crate::p![sit: 0, swing: 0]); s.wait(300.).await;
            scenes::annoyed(&s, which, w).await;
            s.rest(&[]); s.wait(600.).await;
            scenes::scene_bye(s, w).await;
        });
        let _ = (tok, id2);
        // (The web sets leaving once the annoyance is over; the goodbye is in the same play.)
        self.chats.get_mut(id).unwrap().leaving = true;
    }
    /// Resumed (or the card went) while it stood on it: picked up where it
    /// was and put on the window's layer, to be glad and run off.
    fn hand_over(&mut self, id: &str, cx: &mut App) {
        let (spots, ww, wh) = (self.spots.clone(), self.window.get().0, self.window.get().1);
        let Some(c) = self.chats.get_mut(id) else { return };
        let (Some(old), Some(at)) = (c.mini.take(), c.last_at) else { self.gone(id, false); self.flush_logo(cx); return };
        if c.handing { return; }
        c.handing = true; c.scene = None; c.leaving = false; c.thens.clear(); c.dirty = true;
        let layer = c.layer.get();
        let mut m = Mini::new(50., self.clock.clone());
        {
            let o = old.a.borrow();
            let mut a = m.a.borrow_mut();
            a.v = o.v; a.t = o.t;
            a.x = at.0; a.base_y = at.1; a.clamp_x = false;
            let floor = Surf { y: at.1, l: -200., r: ww + 200. };
            a.surf = Some(Rc::new(move || Some(floor)));
            a.v[K::alpha as usize] = 1.; a.t[K::alpha as usize] = 1.; a.auto_blink = true;
        }
        let id2 = id.to_string();
        let floor_y = at.1;
        let ground: SurfFn = Rc::new(move || {
            let y = spots.borrow().get(&format!("composer:{id2}")).map(|b| f(b.top()))
                .unwrap_or_else(|| if f(layer.size.height) > 0. { f(layer.bottom()) - 4. } else { wh - 6. });
            Some(Surf { y: y.max(floor_y), l: -200., r: ww + 200. })
        });
        self.clip_h.set(wh);
        let tok = m.play(move |s| scenes::handed_over(s, ground, ww));
        self.push_top(m, tok, Then::HandDone { id: id.to_string() }, "hand");
    }
    fn parked_destroy(&mut self, id: &str, cx: &mut App) {
        let standing = self.chats.get(id).is_some_and(|c| c.mini.is_some() && c.scene == Some("parked") && !c.leaving);
        if standing { self.hand_over(id, cx); }
        else {
            self.gone(id, false);
            if self.holder == Holder::Chat(id.to_string()) && !self.chats.get(id).is_some_and(|c| c.handing) && !self.logo_pending { self.holder = Holder::None; }
        }
        self.flush_logo(cx);
    }

    // ─── A life in the top bar ───

    fn top_read(&self, cx: &App) -> TopRead {
        let sp = self.spots.borrow();
        let row = |tag: &str| -> Option<(f32, f32, f32, f32, f32, f32)> {
            let pr = sp.get(&format!("usage:{tag}"))?;
            let fill = sp.get(&format!("usage:{tag}:fill"))?;
            if f(pr.size.width) < 40. || f(pr.size.height) < 4. { return None; }
            Some((f(fill.right()), f(fill.size.width) / f(pr.size.width) * 100., f(pr.top()), f(pr.bottom()), f(pr.left()), f(pr.right())))
        };
        let (ww, wh) = self.window.get();
        let waiting: Vec<(String, Bounds<Pixels>)> = sp.all("sidebar:input:");
        let project = if ww < 900. && !self.tops.any_width { None } else {
            waiting.iter().find(|(_, b)| f(b.size.width) > 20. && f(b.top()) > 40. && f(b.bottom()) < wh - 8.).map(|(n, _)| n.clone())
        };
        let _ = cx;
        TopRead {
            five: row("5h"), week: row("wk"),
            update: sp.get("tab-badge:settings").is_some(),
            tabs: sp.get("tabbar").is_some(), settings: sp.get("tab:settings").is_some(),
            bell: sp.get("approvals").is_some(),
            project, waiting: waiting.len(),
        }
    }
    fn top_env(&self, cx: &App) -> Option<Top> {
        let (l, t, w, h) = self.logo_mark()?;
        let k = (w * 74. / 96.) / 50.;
        let home = (l + w * 11. / 96. + 25. * k, t + h * 26. / 96. + 78. * k);
        let (ww, wh) = self.window.get();
        Some(Top {
            spots: self.spots.clone(), w: ww, h: wh, home, edge: if home.0 < ww / 2. { -46. } else { ww + 46. },
            force: self.tops.force.clone(), party_for: self.tops.party_for.clone(),
            tabs: crundi_shell::spots::tabs(cx).map(|t| t.scroll), tabs_moved: self.tabs_moved.clone(), clip_h: self.clip_h.clone(),
        })
    }
    fn top_outing(&mut self, kind: &str, r: TopRead, cx: &mut App) -> bool {
        if self.holder != Holder::None || self.logo_away || self.tops.busy || cx.reduce_motion() { return false; }
        let Some(st) = self.top_env(cx) else { return false };
        let Some((mut m, home, _)) = self.logo_stage("top") else { return false };
        self.holder = Holder::Top; self.tops.busy = true;
        self.set_away(true, cx);
        {
            let mut a = m.a.borrow_mut();
            a.home_surf = a.surf.clone();
            a.x = home.0; a.v[K::grow as usize] = 0.; a.t[K::grow as usize] = 0.; a.v[K::alpha as usize] = 1.; a.t[K::alpha as usize] = 1.;
        }
        let cut = Rc::new(Cell::new(false));
        self.tops.cut = Some(cut.clone());
        let kind = kind.to_string();
        let tok = m.play(move |s| scenes::top_outing(s, kind, st, r));
        self.push_top(m, tok, Then::Outing { cut }, "top");
        true
    }
    /// A chat wants it while it is out in the top bar: called away.
    fn top_cut(&mut self) {
        let Some(cut) = self.tops.cut.clone() else { return };
        if cut.get() { return; }
        cut.set(true);
        let w = self.window.get().0;
        if let Some(t) = self.top.iter_mut().find(|t| t.role == "top") {
            let tok = t.mini.play(move |s| scenes::top_cut(s, w));
            t.thens.insert(tok, Then::CutDone);
        }
    }
    fn free_top(&mut self) {
        self.top.retain(|t| t.role != "top");
        self.top_dirty = true;
        self.tops.busy = false; self.tops.cut = None;
        if self.holder == Holder::Top { self.holder = Holder::None; self.retick = true; }
    }
    /// The web's topTick (every 3 s while nothing else is going on).
    fn top_tick(&mut self, cx: &mut App) {
        let t = now_ms();
        if t - self.tops.last_tick < 2900. { return; }
        self.tops.last_tick = t;
        if !self.auto || self.holder != Holder::None || self.logo_away || self.tops.busy || cx.reduce_motion() || self.logo_mark().is_none() { return; }
        if !crundi_shell::stores(cx).session.read(cx).signed_in() { return; }
        let r = self.top_read(cx);
        let mut ready: Vec<(&'static str, (f64, f64))> = vec![];
        // A limit that was well up and is suddenly near nothing has reset.
        for (kk, row) in [("five", r.five), ("week", r.week)] {
            let Some(row) = row else { continue };
            let now = row.1;
            if let Some(p) = self.tops.prev.get(kk).copied() { if p >= 35. && p - now >= 30. { self.tops.party_for = kk.to_string(); self.tops.party_at = t + rnd(3000., 9000.) as f64; } }
            if self.tops.prev.get(kk).is_none_or(|p| (p - now).abs() >= 1.) { self.tops.prev.insert(kk, now); save_memory(&self.tops.prev, self.tops.update_at); }
        }
        if self.tops.party_at > 0. && t >= self.tops.party_at { if self.top_outing("party", r, cx) { self.tops.party_at = 0.; } return; }
        let mut due = |s: &mut TopState, kind: &'static str, on: bool, first: (f64, f64), again: (f64, f64)| {
            if !on { s.seen.insert(kind, false); return; }
            if !s.seen.get(kind).copied().unwrap_or(false) {
                s.seen.insert(kind, true);
                if s.next.get(kind).copied().unwrap_or(0.) < t + first.0 { s.next.insert(kind, t + first.0 + actor::random() as f64 * (first.1 - first.0)); }
            }
            if t >= s.next.get(kind).copied().unwrap_or(0.) { ready.push((kind, again)); }
        };
        due(&mut self.tops, "five", r.five.is_some_and(|x| x.1 >= 90.), (6000., 20000.), (7. * 60000., 16. * 60000.));
        due(&mut self.tops, "week", r.week.is_some_and(|x| x.1 >= 90.), (15000., 45000.), (12. * 60000., 25. * 60000.));
        if r.update && r.tabs && r.settings && t - self.tops.update_at >= UPDATE_EVERY { due(&mut self.tops, "update", true, (20000., 45000.), (UPDATE_EVERY, UPDATE_EVERY * 1.5)); }
        else { self.tops.seen.insert("update", false); }
        // A notification: only sometimes, and only when one has just arrived.
        let nb = (if r.bell { 1 } else { 0 }) + if r.project.is_some() { r.waiting } else { 0 };
        if nb > self.tops.bells && t >= self.tops.next.get("notify").copied().unwrap_or(0.) {
            if actor::random() < 0.55 { self.tops.seen.insert("notify", true); self.tops.next.insert("notify", t + rnd(8000., 30000.) as f64); self.tops.bell_due = true; }
            else { self.tops.next.insert("notify", t + rnd(10. * 60000., 20. * 60000.) as f64); }
        }
        self.tops.bells = nb;
        if self.tops.bell_due && !r.bell && r.project.is_none() { self.tops.bell_due = false; }
        if self.tops.bell_due && t >= self.tops.next.get("notify").copied().unwrap_or(0.) {
            ready.push((if r.project.is_some() && (!r.bell || actor::random() < 0.6) { "project" } else { "notify" }, (20. * 60000., 40. * 60000.)));
        }
        if ready.is_empty() { return; }
        let go = actor::pick(&ready);
        if self.top_outing(go.0, r, cx) {
            let nx = t + go.1.0 + actor::random() as f64 * (go.1.1 - go.1.0);
            self.tops.next.insert(go.0, nx);
            if go.0 == "notify" || go.0 == "project" { self.tops.bell_due = false; self.tops.next.insert("notify", nx); }
            if go.0 == "update" { self.tops.update_at = t; save_memory(&self.tops.prev, t); }
        }
    }

    /// Run the logo's returns queued by `gone` (they need the App).
    fn flush_logo(&mut self, cx: &mut App) { if std::mem::take(&mut self.logo_pending) { self.pump_logo(cx); } }
}

/// Mostly inside the window (the web's inView).
fn in_view(b: Bounds<Pixels>, (ww, wh): (f32, f32)) -> bool {
    let (l, t, w, h) = (f(b.left()), f(b.top()), f(b.size.width), f(b.size.height));
    if w < 40. || h < 40. { return false; }
    let vw = (l + w).min(ww) - l.max(0.);
    let vh = (t + h).min(wh) - t.max(0.);
    vw > w * 0.55 && vh > (h * 0.5).min(160.)
}

// ─── The test driver ───

/// `mini-trigger <queue|sit|nudge|parked>`, `mini-poke`, `mini-scare [x]`,
/// `mini-top <five|week|update|notify|project|party> [force]`,
/// `mini-auto <on|off>`, `mini-delays <queue> <sit> <nudge>`, `mini-act`,
/// all on the focused chat.
pub fn test_ui(chat: &str, name: &str, args: &[String], cx: &mut App) -> Option<Value> {
    let s = sys(cx);
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let has = s.borrow().chats.contains_key(chat);
    match name {
        "mini-auto" => { s.borrow_mut().auto = a(0) != "off"; }
        // `mini-memory`: what Mini remembers, and what is on disk;
        // `mini-memory reload`: read back from disk (as a new run starts);
        // `mini-memory <five> <week> <update-ago-ms>`: kept as top_tick keeps them.
        "mini-memory" => {
            let mut b = s.borrow_mut();
            if a(0) == "reload" { let (prev, at) = load_memory(); b.tops.prev = prev; b.tops.update_at = at; }
            else if !a(0).is_empty() {
                let n = |i: usize| args.get(i).and_then(|v| v.parse::<f32>().ok());
                if let Some(v) = n(0) { b.tops.prev.insert("five", v); }
                if let Some(v) = n(1) { b.tops.prev.insert("week", v); }
                if let Some(v) = args.get(2).and_then(|v| v.parse::<f64>().ok()) { b.tops.update_at = now_ms() - v; }
                save_memory(&b.tops.prev, b.tops.update_at);
            }
            let disk: Value = std::fs::read_to_string(memory_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
            let ago = b.tops.update_at.is_finite().then(|| now_ms() - b.tops.update_at);
            return Some(json!({ "ok": true, "pct": b.tops.prev, "updateAgo": ago, "disk": disk }));
        }
        "mini-top" => {
            let kind = a(0);
            let ok = {
                let mut b = s.borrow_mut();
                let f = a(1);
                b.tops.force = (!f.is_empty()).then_some(f.clone());
                b.tops.any_width = true;
                if kind == "party" { b.tops.party_for = if f.is_empty() { "five".into() } else { f }; }
                let r = b.top_read(cx);
                let ok = b.top_outing(&kind, r, cx);
                if ok { let k: &'static str = match kind.as_str() { "five" => "five", "week" => "week", "update" => "update", "notify" => "notify", "project" => "project", _ => "party" }; b.tops.seen.insert(k, true); b.tops.next.insert(k, now_ms() + 10. * 60000.); }
                ok
            };
            after_change(&s, cx);
            return Some(json!({ "ok": ok }));
        }
        _ if !has => return Some(json!({ "ok": false, "error": "no chat" })),
        "mini-trigger" => {
            let n: &'static str = match a(0).as_str() { "queue" => "queue", "sit" => "sit", "parked" => "parked", _ => "nudge" };
            s.borrow_mut().trigger(chat, n, cx);
        }
        "mini-poke" => { s.borrow_mut().tapped(chat, cx); }
        "mini-scare" => {
            let mut b = s.borrow_mut();
            let Some(c) = b.chats.get_mut(chat) else { return None };
            c.wary = now_ms(); c.last_flee = 0.;
            let at = c.mini.as_ref().map(|m| { let a = m.a.borrow(); (a.x, a.base_y) });
            if let Some((x, y)) = at {
                let mx = a(0).parse::<f32>().unwrap_or(x + 20.);
                b.moved(chat, Some((mx, y - 20.)), cx);
            }
        }
        "mini-act" => { s.borrow_mut().user_acted(chat, cx); }
        "mini-delays" => {
            let n = |i: usize, d: f64| args.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
            if let Some(c) = s.borrow_mut().chats.get_mut(chat) { c.delay = Delays { queue: n(0, DELAY.queue), sit: n(1, DELAY.sit), nudge: n(2, DELAY.nudge) }; }
        }
        _ => return None,
    }
    after_change(&s, cx);
    Some(json!({ "ok": true }))
}

/// The driver's look: the focused chat's scene, the holder, the logo, the actors.
pub fn state(chat: &str, cx: &App) -> Value {
    let Some(s) = cx.try_global::<SysHandle>().map(|h| h.0.clone()) else { return Value::Null };
    let b = s.borrow();
    let c = b.chats.get(chat);
    let actor = c.and_then(|c| c.mini.as_ref()).map(|m| {
        let a = m.a.borrow();
        json!({ "x": a.x, "y": a.base_y + a.y_off, "alpha": a.v[K::alpha as usize], "sit": a.t[K::sit as usize], "lid": a.t[K::lid as usize], "air": a.air, "chute": a.chuting, "finished": a.finished, "lookX": a.look_tx })
    });
    let scene = c.and_then(|c| if c.leaving { Some("leaving") } else { c.scene });
    json!({
        "scene": scene, "actor": actor, "mode": c.map(|c| if c.mode == Mode::Parked { "parked" } else { "live" }),
        "holder": match &b.holder { Holder::None => Value::Null, Holder::Top => json!("top"), Holder::Chat(i) => json!(i) },
        "logoAway": b.logo_away, "topBusy": b.tops.busy, "topActors": b.top.len(), "topRoles": b.top.iter().map(|t| t.role).collect::<Vec<_>>(),
        "looping": b.looping, "auto": b.auto, "fx": c.map_or(0, |c| c.fx.len()) + b.top_fx.len(),
        "layer": c.map(|c| { let l = c.layer.get(); json!({ "x": f(l.left()), "y": f(l.top()), "w": f(l.size.width), "h": f(l.size.height) }) }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn in_view_is_mostly_inside() {
        let b = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(300.)));
        assert!(in_view(b, (1000., 800.)));
        assert!(!in_view(Bounds::new(point(px(900.), px(0.)), size(px(400.), px(300.))), (1000., 800.)));
        assert!(!in_view(Bounds::new(point(px(0.), px(0.)), size(px(30.), px(300.))), (1000., 800.)));
    }
}
