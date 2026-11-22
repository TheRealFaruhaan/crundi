//! Mini's body on springs (crundi-mini.js's Actor): every part a number
//! that a behaviour only ever says where to go next, walking, hopping,
//! falling (by parachute when it is a long way), blinking, the antenna
//! trailing; and the scene runner: a behaviour is an async fn polled once a
//! frame on Mini's own clock (the web's promises on `a.time`), dropped the
//! moment a newer one takes over (the web's CANCEL).

use std::{cell::{Cell, RefCell}, collections::HashMap, future::Future, pin::Pin, rc::Rc, task::{Context, Poll}};

pub const GROUND: f32 = 90.;
pub const SIT_DROP: f32 = 12.;
pub const G: f32 = 1500.;

/// The body's numbers (the web's REST keys, in its order).
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum K { turn, lean, squash, headRot, headY, eyeX, eyeY, lid, armL, armR, legL, legR, sit, swing, shake, alpha, armSwing, armsBack, grow, reach, chute }
pub const NK: usize = 21;
pub const ALL: [K; NK] = [K::turn, K::lean, K::squash, K::headRot, K::headY, K::eyeX, K::eyeY, K::lid, K::armL, K::armR, K::legL, K::legR, K::sit, K::swing, K::shake, K::alpha, K::armSwing, K::armsBack, K::grow, K::reach, K::chute];
/// [stiffness, damping]: lower damping is bouncier.
const SPRING: [(f32, f32); NK] = [(130., 21.), (150., 22.), (330., 13.), (160., 19.), (170., 20.), (230., 26.), (230., 26.), (420., 34.), (165., 16.), (165., 16.), (260., 24.), (260., 24.), (95., 17.), (50., 13.), (70., 15.), (60., 15.), (90., 18.), (110., 19.), (120., 12.), (150., 20.), (150., 13.)];
pub const REST: [f32; NK] = [0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 1., 1., 0., 1., 0., 0.];

// ─── Chance (the web's Math.random, rnd, pick, deal) ───

thread_local! {
    static SEED: Cell<u64> = Cell::new(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1);
    static PACKS: RefCell<HashMap<&'static str, (Vec<usize>, i64, usize)>> = RefCell::new(HashMap::new());
}

/// 0 ≤ r < 1.
pub fn random() -> f32 {
    SEED.with(|s| { let mut x = s.get(); x ^= x << 13; x ^= x >> 7; x ^= x << 17; s.set(x); (x >> 40) as f32 / (1u64 << 24) as f32 })
}
pub fn rnd(a: f32, b: f32) -> f32 { a + random() * (b - a) }
pub fn pick<T: Clone>(l: &[T]) -> T { l[((random() * l.len() as f32) as usize).min(l.len() - 1)].clone() }
pub fn clamp(v: f32, a: f32, b: f32) -> f32 { if v < a { a } else if v > b { b } else { v } }

/// What to do next without doing the same thing over and over: dealt from
/// a shuffled pack, each once before any comes round again, never the same
/// twice running across a reshuffle; packs last as long as the app.
pub fn deal(key: &'static str, n: usize) -> usize {
    PACKS.with(|p| {
        let mut p = p.borrow_mut();
        let e = p.entry(key).or_insert((vec![], -1, n));
        if e.0.is_empty() || e.2 != n {
            let last = if e.2 == n { e.1 } else { -1 };
            let mut q: Vec<usize> = (0..n).collect();
            for j in (1..n).rev() { let r = ((random() * (j + 1) as f32) as usize).min(j); q.swap(j, r); }
            if n > 1 && q[n - 1] as i64 == last { q.swap(n - 1, 0); }
            *e = (q, last, n);
        }
        let v = e.0.pop().unwrap_or(0);
        e.1 = v as i64;
        v
    })
}

// ─── The ground it stands on ───

/// A floor: its height and how far along it goes, in the layer's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surf { pub y: f32, pub l: f32, pub r: f32 }
pub type SurfFn = Rc<dyn Fn() -> Option<Surf>>;
/// A place in the layer (what it points at).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt { pub x: f32, pub y: f32 }
pub type PtFn = Rc<dyn Fn() -> Option<Pt>>;

// ─── Small things that appear and vanish (the web's FX) ───

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxKind { Knock, Dust, Puff, Z, Note, Bang, Ask, Dots, Ring, Sweat, Anger, Spark, Confetti, Plane }

impl FxKind {
    /// (w, h, duration ms)
    pub fn size(self) -> (f32, f32, f32) {
        match self {
            FxKind::Knock => (22., 22., 420.), FxKind::Dust => (12., 12., 520.), FxKind::Puff => (14., 10., 900.), FxKind::Z => (12., 12., 1700.),
            FxKind::Note => (12., 16., 1500.), FxKind::Bang => (8., 18., 900.), FxKind::Ask => (14., 20., 1500.), FxKind::Dots => (34., 22., 2300.),
            FxKind::Ring => (20., 20., 520.), FxKind::Sweat => (8., 11., 700.), FxKind::Anger => (16., 16., 900.), FxKind::Spark => (14., 14., 700.),
            FxKind::Confetti => (6., 9., 1300.), FxKind::Plane => (18., 14., 1100.),
        }
    }
}

/// One effect: its kind, its box's top left in the layer, when it began
/// (the layer's clock), its CSS variables (--dx, --dy) and colour (--cf).
#[derive(Clone, Debug)]
pub struct Fx { pub kind: FxKind, pub x: f32, pub y: f32, pub born: f32, pub dx: Option<f32>, pub dy: Option<f32>, pub cf: Option<u32> }

/// An effect's options (the web's ';--dx:…px;--dy:…;--cf:…').
#[derive(Clone, Copy, Default)]
pub struct FxOpt { pub dx: Option<f32>, pub dy: Option<f32>, pub cf: Option<u32> }
pub fn dx(v: f32) -> FxOpt { FxOpt { dx: Some(v), ..Default::default() } }

// ─── The actor ───

pub struct Actor {
    pub k: f32,
    pub v: [f32; NK], pub t: [f32; NK], pub vel: [f32; NK],
    pub x: f32, pub vx: f32, pub tx: Option<f32>, pub speed: f32,
    pub base_y: f32, pub y_off: f32, pub vy: f32, pub air: bool,
    /// The walk and the landing awaited (their ids), and the last that came.
    arrive: Option<u64>, arrived: u64, land: Option<u64>, landed: u64, seq: u64,
    pub surf: Option<SurfFn>,
    pub clamp_x: bool,
    pub phase: f32, pub walk: f32, pub time: f32, pub ant: f32, pub ant_v: f32,
    pub next_blink: f32, pub blink: f32, pub auto_blink: bool,
    pub look_x: f32, pub look_y: f32, pub look_tx: f32, pub look_ty: f32,
    pub chuting: bool,
    /// Holding on to something someone else moves (the tabs).
    pub carry: Option<Rc<dyn Fn() -> f32>>,
    pub last_speed: f32,
    pub finished: bool,
    pub home_surf: Option<SurfFn>,
    /// Effects made since the layer last took them.
    pub fx_out: Vec<Fx>,
    /// The layer's clock, for the effects' birth.
    pub clock: Rc<Cell<f32>>,
}

impl Actor {
    pub fn new(size: f32, clock: Rc<Cell<f32>>) -> Self {
        let mut v = REST;
        v[K::alpha as usize] = 0.;
        let t = v;
        Actor {
            k: size / 92., v, t, vel: [0.; NK], x: 0., vx: 0., tx: None, speed: 60., base_y: 0., y_off: 0., vy: 0., air: false,
            arrive: None, arrived: 0, land: None, landed: 0, seq: 0, surf: None, clamp_x: true,
            phase: 0., walk: 0., time: 0., ant: 0., ant_v: 0., next_blink: 1.5, blink: 0., auto_blink: true,
            look_x: 0., look_y: 0., look_tx: 0., look_ty: 0., chuting: false, carry: None, last_speed: 0., finished: false, home_surf: None,
            fx_out: vec![], clock,
        }
    }

    pub fn g(&self, k: K) -> f32 { self.v[k as usize] }
    pub fn tg(&self, k: K) -> f32 { self.t[k as usize] }
    pub fn set_t(&mut self, k: K, v: f32) { self.t[k as usize] = v; }

    /// An effect at a place on the body (in the drawing's units).
    pub fn fx(&mut self, kind: FxKind, dx: f32, dy: f32, o: FxOpt) {
        let (w, h, _) = kind.size();
        let x = self.x + dx * self.k - w / 2.;
        let y = self.base_y + self.y_off - (GROUND - dy) * self.k - h / 2.;
        self.fx_out.push(Fx { kind, x, y, born: self.clock.get(), dx: o.dx, dy: o.dy, cf: o.cf });
    }
    /// The same at a place in the layer.
    pub fn fx_at(&mut self, kind: FxKind, x: f32, y: f32) {
        let k = self.k;
        let (bx, by) = (48. + (x - self.x) / k, GROUND + (y - self.base_y - self.y_off) / k);
        self.fx(kind, bx, by, FxOpt::default());
    }

    pub fn surf_now(&self) -> Option<Surf> { self.surf.as_ref().and_then(|f| f()) }

    /// One step of `dt` seconds.
    pub fn step(&mut self, dt: f32) {
        self.time += dt;
        for i in 0..NK {
            let (k, d) = SPRING[i];
            self.vel[i] += ((self.t[i] - self.v[i]) * k - self.vel[i] * d) * dt;
            self.v[i] += self.vel[i] * dt;
        }
        // Where the ground is: rising carries it up; dropping by more than a
        // step is a fall of exactly that height.
        if let Some(sf) = self.surf_now() {
            if !self.air && sf.y - self.base_y > 10. && self.g(K::alpha) > 0.5 { self.y_off = self.base_y - sf.y; self.base_y = sf.y; self.air = true; self.vy = 0.; }
            self.base_y += (sf.y - self.base_y) * (dt * 18.).min(1.);
            if self.clamp_x { if let Some(tx) = self.tx { self.tx = Some(clamp(tx, sf.l, sf.r)); } }
        }
        // Walking: ease the speed in and out, stop on the mark.
        let mut want = 0.;
        if let Some(tx) = self.tx {
            let d = tx - self.x;
            want = clamp(d * 5., -self.speed, self.speed);
            if d.abs() < 1.2 && self.vx.abs() < 14. {
                self.x = tx; self.tx = None; want = 0.;
                if let Some(id) = self.arrive.take() { self.t[K::turn as usize] = 0.; self.arrived = id; }
            }
        }
        self.vx += (want - self.vx) * (dt * 9.).min(1.);
        self.x += self.vx * dt;
        if let Some(c) = &self.carry { let cx = c(); self.vx = 0.; self.x = cx; }
        let sp = self.vx.abs();
        self.walk += (clamp(sp / 34., 0., 1.) - self.walk) * (dt * 10.).min(1.);
        self.phase += sp * dt * 0.165 / self.k;

        // Falling and hopping; a long way down is by parachute.
        if self.air {
            self.vy += G * dt;
            if self.vy > 0. && !self.chuting && -self.y_off > 2.3 * 92. * self.k && self.vy > 190. { self.chuting = true; self.t[K::chute as usize] = 1.; self.vel[K::squash as usize] += 2.5; self.vy *= 0.35; }
            if self.chuting {
                self.vy = self.vy.min(62. + 26. * (self.time * 3.1).sin());
                self.x += (self.time * 2.3).sin() * 13. * dt;
                self.t[K::lean as usize] = (self.time * 2.3 + 1.2).sin() * 9.;
            }
            self.y_off += self.vy * dt;
            if self.y_off >= 0. && self.vy > 0. {
                let hit = self.vy;
                self.y_off = 0.; self.vy = 0.; self.air = false;
                if self.chuting { self.chuting = false; self.t[K::chute as usize] = 0.; self.t[K::lean as usize] = 0.; }
                self.vel[K::squash as usize] -= clamp(hit / 95., 1.2, 7.);
                if hit > 260. { self.fx(FxKind::Dust, 30., GROUND - 3., dx(-9.)); self.fx(FxKind::Dust, 66., GROUND - 3., dx(9.)); }
                if let Some(id) = self.land.take() { self.landed = id; }
            }
        }
        if self.auto_blink {
            self.next_blink -= dt;
            if self.next_blink <= 0. { self.blink = 0.13; self.next_blink = rnd(1.8, 4.6); }
        }
        if self.blink > 0. { self.blink -= dt; }
        self.look_x += (self.look_tx - self.look_x) * (dt * 9.).min(1.);
        self.look_y += (self.look_ty - self.look_y) * (dt * 9.).min(1.);
        let drive = -self.vx * 0.012 - self.vel[K::headRot as usize] * 0.006 - self.vel[K::turn as usize] * 0.5 + if self.air { -self.vy * 0.0016 } else { 0. };
        self.ant_v += ((drive - self.ant) * 210. - self.ant_v * 11.) * dt;
        self.ant += self.ant_v * dt;
    }

    /// A new behaviour: what was awaited before no longer comes.
    pub fn new_play(&mut self) { self.arrive = None; self.land = None; }

    fn next_id(&mut self) -> u64 { self.seq += 1; self.seq }
}

// ─── A behaviour's tools (the web's play() api) ───

pub type ActorRef = Rc<RefCell<Actor>>;
pub type Scene = Pin<Box<dyn Future<Output = ()>>>;

/// Ready once `done` says so; checked each frame (the runner polls every frame).
pub struct Until<F: Fn() -> bool>(F);
impl<F: Fn() -> bool> Future for Until<F> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> { if (self.0)() { Poll::Ready(()) } else { Poll::Pending } }
}

/// A pose: some of the numbers' targets.
#[derive(Clone, Default)]
pub struct P(pub Vec<(K, f32)>);
impl P {
    pub fn new() -> Self { P(vec![]) }
    pub fn set(&mut self, k: K, v: f32) -> &mut Self { if let Some(e) = self.0.iter_mut().find(|e| e.0 == k) { e.1 = v; } else { self.0.push((k, v)); } self }
    pub fn with(mut self, k: K, v: f32) -> Self { self.set(k, v); self }
}

/// `p![armR: 152, headRot: 13]`
#[macro_export]
macro_rules! p {
    ($($k:ident : $v:expr),* $(,)?) => { $crate::mini::actor::P(vec![$(($crate::mini::actor::K::$k, ($v) as f32)),*]) };
}

#[derive(Clone)]
pub struct S { pub a: ActorRef }

impl S {
    pub fn wait(&self, ms: f32) -> impl Future<Output = ()> + 'static + use<> {
        let a = self.a.clone();
        let at = a.borrow().time + ms / 1000.;
        Until(move || a.borrow().time >= at)
    }
    pub fn pose(&self, o: P) { let mut a = self.a.borrow_mut(); for (k, v) in o.0 { a.t[k as usize] = v; } }
    pub fn pose_ref(&self, o: &P) { let mut a = self.a.borrow_mut(); for (k, v) in &o.0 { a.t[*k as usize] = *v; } }
    /// Back to rest, but for alpha, sit, grow, chute and what `keep` names.
    pub fn rest(&self, keep: &[K]) {
        let mut a = self.a.borrow_mut();
        for k in ALL { if !matches!(k, K::alpha | K::sit | K::grow | K::chute) && !keep.contains(&k) { a.t[k as usize] = REST[k as usize]; } }
    }
    pub fn walk_to(&self, x: f32, speed: f32) -> impl Future<Output = ()> + 'static + use<> {
        let a2 = self.a.clone();
        let id = {
            let mut a = self.a.borrow_mut();
            a.t[K::sit as usize] = 0.; a.t[K::swing as usize] = 0.;
            a.speed = if speed > 0. { speed } else { 55. }; a.tx = Some(x); a.last_speed = a.speed;
            a.t[K::turn as usize] = if x > a.x { 0.8 } else { -0.8 };
            let id = a.next_id(); a.arrive = Some(id); id
        };
        Until(move || a2.borrow().arrived == id)
    }
    fn landing(&self, id: u64) -> impl Future<Output = ()> + 'static + use<> {
        let a2 = self.a.clone();
        Until(move || a2.borrow().landed == id)
    }
    pub fn hop(&self, h: f32) -> impl Future<Output = ()> + 'static + use<> {
        let id = {
            let mut a = self.a.borrow_mut();
            a.air = true; a.vy = -(2. * G * if h > 0. { h } else { 12. }).sqrt();
            a.vel[K::squash as usize] += 2.2;
            let id = a.next_id(); a.land = Some(id); id
        };
        self.landing(id)
    }
    /// The ground has moved lower: keep the body where it is and let it drop.
    pub fn fall_to(&self, surf: SurfFn) -> impl Future<Output = ()> + 'static + use<> {
        let id = {
            let mut a = self.a.borrow_mut();
            let was = a.base_y + a.y_off;
            let sf = surf();
            a.surf = Some(surf);
            a.base_y = sf.map_or(was, |s| s.y); a.y_off = (was - a.base_y).min(0.); a.air = true; a.vy = a.vy.max(0.);
            let id = a.next_id(); a.land = Some(id); id
        };
        self.landing(id)
    }
    /// The ground has moved higher: jump up onto it.
    pub fn jump_to(&self, surf: SurfFn) -> impl Future<Output = ()> + 'static + use<> {
        let id = {
            let mut a = self.a.borrow_mut();
            let was = a.base_y + a.y_off;
            let sf = surf();
            a.surf = Some(surf);
            a.base_y = sf.map_or(was, |s| s.y); a.y_off = (was - a.base_y).max(0.); a.air = true;
            a.vy = -(2. * G * (a.y_off + 9.)).sqrt(); a.vel[K::squash as usize] += 2.4;
            let id = a.next_id(); a.land = Some(id); id
        };
        self.landing(id)
    }
    /// Until it lands from what it is in the air for (TOP.cut).
    pub fn landed(&self) -> impl Future<Output = ()> + 'static + use<> { let id = { let mut a = self.a.borrow_mut(); let id = a.next_id(); a.land = Some(id); id }; self.landing(id) }
    pub fn fx(&self, kind: FxKind, dx_: f32, dy_: f32) { self.a.borrow_mut().fx(kind, dx_, dy_, FxOpt::default()); }
    pub fn fxo(&self, kind: FxKind, dx_: f32, dy_: f32, o: FxOpt) { self.a.borrow_mut().fx(kind, dx_, dy_, o); }
    pub fn fx_at(&self, kind: FxKind, x: f32, y: f32) { self.a.borrow_mut().fx_at(kind, x, y); }
    pub fn surf(&self) -> Option<Surf> { let f = self.a.borrow().surf.clone(); f.and_then(|f| f()) }
    // Quick reads.
    pub fn x(&self) -> f32 { self.a.borrow().x }
    pub fn k(&self) -> f32 { self.a.borrow().k }
    pub fn base_y(&self) -> f32 { self.a.borrow().base_y }
    pub fn time(&self) -> f32 { self.a.borrow().time }
    pub fn walking(&self) -> bool { self.a.borrow().tx.is_some() }
    pub fn stop(&self) { self.a.borrow_mut().tx = None; }
    pub fn t(&self, k: K) -> f32 { self.a.borrow().t[k as usize] }
    pub fn with<R>(&self, f: impl FnOnce(&mut Actor) -> R) -> R { f(&mut self.a.borrow_mut()) }
}

/// An actor and the behaviour it is playing.
pub struct Mini {
    pub a: ActorRef,
    scene: Option<Scene>,
    /// Bumped by each new behaviour.
    pub tok: u64,
    /// The behaviour that ended (or was dropped) since last asked, by token.
    pub ended: Vec<u64>,
}

impl Mini {
    pub fn new(size: f32, clock: Rc<Cell<f32>>) -> Self { Mini { a: Rc::new(RefCell::new(Actor::new(size, clock))), scene: None, tok: 0, ended: vec![] } }
    pub fn s(&self) -> S { S { a: self.a.clone() } }
    /// Play a behaviour; the one before is dropped (and counts as ended).
    pub fn play<F, Fut>(&mut self, f: F) -> u64 where F: FnOnce(S) -> Fut, Fut: Future<Output = ()> + 'static {
        if self.scene.take().is_some() { self.ended.push(self.tok); }
        self.tok += 1;
        self.a.borrow_mut().new_play();
        self.scene = Some(Box::pin(f(self.s())));
        self.tok
    }
    pub fn playing(&self) -> bool { self.scene.is_some() }
    /// One frame: the body, then the behaviour from where it waits.
    pub fn step(&mut self, dt: f32) {
        self.a.borrow_mut().step(dt);
        if let Some(mut sc) = self.scene.take() {
            let mut cx = Context::from_waker(std::task::Waker::noop());
            match sc.as_mut().poll(&mut cx) {
                Poll::Ready(()) => self.ended.push(self.tok),
                Poll::Pending => self.scene = Some(sc),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn deals_every_one_before_repeating() {
        let mut seen: Vec<usize> = (0..5).map(|_| deal("t", 5)).collect();
        seen.sort();
        assert_eq!(seen, vec![0, 1, 2, 3, 4]);
    }
    #[::core::prelude::v1::test]
    fn walks_waits_and_hops_on_its_own_clock() {
        let mut m = Mini::new(54., Rc::new(Cell::new(0.)));
        m.a.borrow_mut().surf = Some(Rc::new(|| Some(Surf { y: 100., l: 0., r: 400. })));
        m.a.borrow_mut().base_y = 100.;
        let done = Rc::new(Cell::new(0));
        let d2 = done.clone();
        m.play(move |s| async move {
            s.walk_to(200., 80.).await;
            d2.set(1);
            s.wait(300.).await;
            s.hop(10.).await;
            d2.set(2);
        });
        for _ in 0..600 { m.step(1. / 60.); }
        assert_eq!(done.get(), 2);
        assert!((m.a.borrow().x - 200.).abs() < 3.);
        assert!(!m.playing() && m.ended == vec![1]);
    }
    #[::core::prelude::v1::test]
    fn a_long_fall_opens_the_parachute() {
        let mut m = Mini::new(54., Rc::new(Cell::new(0.)));
        m.a.borrow_mut().base_y = 0.;
        m.play(|s| async move { s.fall_to(Rc::new(|| Some(Surf { y: 400., l: 0., r: 400. }))).await; });
        let mut chute = false;
        for _ in 0..600 { m.step(1. / 60.); chute |= m.a.borrow().chuting; }
        assert!(chute && !m.playing());
    }
}
