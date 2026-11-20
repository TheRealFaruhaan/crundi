//! What Mini does (crundi-mini.js, "Small moves", "Scenes", "Being
//! tapped", "It is the logo", "A life in the top bar", "A sleeping chat"),
//! move for move: each an async fn over the actor's springs (actor.rs).

use super::actor::{clamp, deal, dx, pick, random, rnd, FxKind as F, FxOpt, Pt, PtFn, Surf, SurfFn, K, P, S, GROUND};
use crate::p;
use crundi_shell::spots::SpotsRef;
use gpui::{px, Bounds, Pixels};
use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};

pub type Boxed = Pin<Box<dyn Future<Output = ()>>>;

// ─── Small moves, shared by the scenes ───

pub async fn knock(s: &S) {
    s.pose(p![turn: 2.1, armR: 0, armL: 0, lean: 0]);              // turn its back on us
    s.wait(620.).await;
    s.pose(p![armR: 152]);
    s.wait(300.).await;
    for _ in 0..3 {
        s.pose(p![armR: 118]); s.wait(95.).await;
        s.fx(F::Knock, 79., 20.);
        s.pose(p![armR: 156]); s.wait(150.).await;
    }
    s.pose(p![armR: 30, headRot: 13]);                               // listen
    s.wait(1000.).await;
    s.pose(p![headRot: 0, armR: 0, turn: 0]);
    s.wait(560.).await;
}
pub async fn shrug(s: &S) {
    s.pose(p![armL: 48, armR: 48, headY: 2.5, headRot: -7, eyeY: -2]);
    s.wait(620.).await;
    s.pose(p![armL: 0, armR: 0, headY: 0, headRot: 0, eyeY: 0]);
    s.wait(380.).await;
}
pub async fn watch(s: &S) {
    s.pose(p![armL: -112, headRot: -11, headY: 2, eyeX: -3.5, eyeY: 3.5, turn: -0.25]);   // wrist up, look at it
    s.wait(700.).await;
    for _ in 0..4 { s.pose(p![legR: -22]); s.wait(150.).await; s.pose(p![legR: 0]); s.wait(170.).await; }
    s.pose(p![lid: 0.45]); s.wait(260.).await;
    s.pose(p![armL: 0, headRot: 0, headY: 0, eyeX: 0, eyeY: 0, turn: 0, lid: 0]);
    s.wait(420.).await;
}
pub async fn shake_head(s: &S) {
    s.pose(p![lid: 0.55, headY: 1.5]);
    for i in 0..5 { s.pose(p![turn: if i % 2 == 1 { 0.55 } else { -0.55 }]); s.wait(190.).await; }
    s.pose(p![turn: 0, lid: 0, headY: 0]);
    s.wait(360.).await;
}
pub async fn sigh(s: &S) {
    s.pose(p![squash: 1.07, headY: -1.5, armL: 14, armR: 14]); s.wait(430.).await;
    s.pose(p![squash: 0.9, headY: 4, headRot: 6, lid: 0.6, armL: -4, armR: -4]);
    s.fx(F::Puff, 64., 38.);
    s.wait(820.).await;
    s.pose(p![squash: 1, headY: 0, headRot: 0, lid: 0, armL: 0, armR: 0]); s.wait(380.).await;
}
pub async fn tap_foot(s: &S) {
    if random() < 0.45 { s.pose(p![armsBack: 1, headRot: 8, eyeY: -2.5, eyeX: 2]); }      // hands behind its back
    else { s.pose(p![armL: -38, armR: -38, headRot: 8, eyeY: -2.5, eyeX: 2]); }          // or arms folded
    for _ in 0..7 { s.pose(p![legL: -24]); s.wait(140.).await; s.pose(p![legL: 0]); s.wait(160.).await; }
    s.pose(p![armsBack: 0, armL: 0, armR: 0, headRot: 0, eyeY: 0, eyeX: 0]); s.wait(360.).await;
}
/// Pacing outside a hospital room: slow lengths of the same stretch, head
/// down, hands clasped or wrung; at each end a look up at the door, nothing.
pub async fn pace(s: &S, turns: usize, speed: f32) {
    let Some(mut sf) = s.surf() else { return };
    if sf.r - sf.l < 58. { return; }
    let mut dir: f32 = if s.x() - sf.l > sf.r - s.x() { -1. } else { 1. };
    let mut clasp = random() < 0.65;
    let brood = |clasp: bool, d: f32| {
        if clasp { s.pose(p![armsBack: 1, armL: 0, armR: 0, shake: 0, armSwing: 0.08, headY: 2.6, headRot: d * 8., eyeY: 3.6, lid: 0.26, lean: d * 1.5, squash: 0.985]); }
        else { s.pose(p![armsBack: 0, armL: -30, armR: -30, shake: 0.1, armSwing: 0, headY: 2.6, headRot: d * 8., eyeY: 3.6, lid: 0.26, lean: d * 1.5, squash: 0.985]); }
    };
    for i in 0..turns {
        match s.surf() { Some(x) => sf = x, None => return }
        let to = if dir > 0. { sf.r - 8. - rnd(0., 14.) } else { sf.l + 8. + rnd(0., 14.) };
        let kind = if i == 0 { 0 } else { (random() * 6.) as usize };
        if i > 0 && random() < 0.3 { clasp = !clasp; }
        brood(clasp, dir);
        let mut going = s.walk_to(to, if kind == 3 { speed * 1.4 } else { speed });
        if kind == 1 {                                                         // the watch, without stopping
            s.wait(520.).await;
            s.pose(p![armsBack: 0, shake: 0, armSwing: 0.08, armL: -112, armR: 0, headRot: -9, headY: 3, eyeX: -3.2, eyeY: 3.6, lid: 0.1]);
            s.wait(1000.).await;
            if s.walking() { brood(clasp, dir); }
        } else if kind == 2 {                                                  // stop dead, a hand to the forehead
            s.wait(rnd(500., 900.)).await;
            if s.walking() {
                s.stop();
                s.pose(p![turn: 0, armsBack: 0, shake: 0, armR: 0, armL: -122, headY: 4, headRot: -5, lid: 0.9, lean: 0]);
                s.wait(1300.).await;
                s.pose(p![armL: 0, lid: 0.3, headY: 2]); s.wait(350.).await;
                brood(clasp, dir);
                going = s.walk_to(to, speed);
            }
        } else if kind == 4 {                                                  // a long breath on the way
            s.wait(rnd(400., 800.)).await;
            if s.walking() { s.stop(); s.pose(p![turn: 0, lean: 0]); s.wait(200.).await; sigh(s).await; brood(clasp, dir); going = s.walk_to(to, speed); }
        }
        going.await;
        s.pose(p![squash: 0.97, lean: 0, headRot: 0]);
        s.wait(rnd(260., 520.)).await;
        if i == turns - 1 || random() < 0.6 {                                  // anything? no.
            s.pose(p![turn: 0, headRot: -6, headY: -0.5, eyeY: -4.2, lid: -0.12, lean: 0, shake: 0, squash: 1]);
            s.wait(rnd(900., 1500.)).await;
            s.pose(p![lid: 0.55, headY: 2.5, eyeY: 2]);
            s.wait(rnd(380., 620.)).await;
        }
        if kind == 5 {                                                         // sit; cannot stay sat
            s.pose(p![armsBack: 0, shake: 0, sit: 1, swing: 0.25, armL: -118, armR: -118, headY: 5, lid: 0.9, turn: 0, headRot: 0]);
            s.wait(rnd(2200., 3200.)).await;
            s.pose(p![sit: 0, swing: 0, armL: 0, armR: 0, headY: 0, lid: 0.2]);
            s.wait(650.).await;
        }
        s.pose(p![squash: 1]);
        dir = -dir;
    }
    s.pose(p![armsBack: 0, shake: 0, armL: 0, armR: 0, armSwing: 1, headY: 0, headRot: 0, eyeY: 0, eyeX: 0, lid: 0, lean: 0, turn: 0, squash: 1]);
    s.wait(420.).await;
}
pub async fn peek_down(s: &S) {
    s.pose(p![lean: 0, headY: 6, headRot: 0, eyeY: 4.5, squash: 0.93, armL: 22, armR: 22]);     // read what is underfoot
    s.wait(900.).await;
    s.pose(p![squash: 1, headY: 0, eyeY: 0, armL: 0, armR: 0]);
    s.wait(250.).await;
    s.hop(7.).await; s.hop(7.).await;                                         // stamp on it
    s.wait(300.).await;
}
pub async fn scan(s: &S) {
    s.pose(p![armR: 148, turn: 0.85, eyeX: 2]); s.wait(820.).await;           // hand over the eyes, look far off
    s.pose(p![armR: 0, armL: 148, turn: -0.85, eyeX: -2]); s.wait(900.).await;
    s.pose(p![armL: 0, turn: 0, eyeX: 0]); s.wait(360.).await;
}
pub async fn paper_plane(s: &S) {
    s.pose(p![armR: 70, turn: 0.4, lean: -5]); s.wait(330.).await;            // wind up
    s.pose(p![armR: 160, lean: 6]);
    s.fxo(F::Plane, 76., 30., FxOpt { dx: Some(rnd(-70., -20.).round()), dy: Some(-120.), cf: None });
    s.wait(520.).await;
    s.pose(p![armR: 0, lean: 0, turn: 0, headRot: -9, eyeY: -4]);             // watch it go
    s.wait(900.).await;
    s.pose(p![headRot: 0, eyeY: 0]); s.wait(260.).await;
}
pub async fn wave(s: &S, n: usize) {
    s.pose(p![armR: 150, headRot: 6]); s.wait(260.).await;
    for _ in 0..n { s.pose(p![armR: 126]); s.wait(130.).await; s.pose(p![armR: 164]); s.wait(130.).await; }
    s.pose(p![armR: 0, headRot: 0]); s.wait(340.).await;
}
pub async fn enter(s: &S, from: f32, to: f32, speed: f32) {
    let sf = s.surf();
    s.with(|a| {
        a.clamp_x = false;
        a.x = from; a.vx = 0.; if let Some(sf) = sf { a.base_y = sf.y; } a.y_off = 0.; a.air = false;
        a.v = super::actor::REST; a.t = super::actor::REST; a.vel = [0.; super::actor::NK];
        a.v[K::alpha as usize] = 0.;
    });
    s.pose(p![alpha: 1]);
    s.walk_to(to, speed).await;
    s.with(|a| a.clamp_x = true);
}
/// Leave in a hurry: arms in the air, flapping, dust at the heels.
pub async fn run_off(s: &S, layer_w: f32) {
    let to = if s.x() < layer_w / 2. { -50. } else { layer_w + 50. };
    s.with(|a| a.clamp_x = false);
    s.pose(p![armL: 162, armR: 162, shake: 1, lid: -0.3, sit: 0, swing: 0, headY: -1, lean: 0, headRot: 0, eyeX: 0, eyeY: 0, squash: 1, legL: 0, legR: 0]);
    let done = s.walk_to(to, 215.);
    let mut i = 0;
    while i < 12 && s.walking() {
        let vx = s.with(|a| a.vx);
        s.fxo(F::Dust, if vx > 0. { 30. } else { 66. }, GROUND - 3., dx(if vx > 0. { -12. } else { 12. }));
        s.wait(105.).await;
        i += 1;
    }
    done.await;
    s.pose(p![alpha: 0]);
    s.wait(200.).await;
}

// ─── A chat's scenes ───

/// The chat's own things Mini knows about: the "Working…" line and the
/// badges in the corner (it keeps off those: they are buttons).
#[derive(Clone)]
pub struct Env {
    /// (x, r, y, t) of the activity row's words.
    pub activity: Rc<dyn Fn() -> Option<(f32, f32, f32, f32)>>,
    /// (l, r, x, y) of the badges.
    pub badge: Rc<dyn Fn() -> Option<(f32, f32, f32, f32)>>,
}

/// A message is waiting to be read.
pub async fn scene_queue(s: S, layer_w: f32, env: Env) {
    let Some(sf) = s.surf() else { return };
    let side = if (env.badge)().is_some() { -1. } else if random() < 0.5 { -1. } else { 1. };
    let spot = sf.l + (sf.r - sf.l) * rnd(0.55, 0.72);
    enter(&s, if side < 0. { -40. } else { layer_w + 40. }, spot, 62.).await;
    s.wait(380.).await;
    let open = deal("queue-open", 4);
    if open == 0 { s.pose(p![eyeY: -3, headRot: -6]); s.wait(520.).await; s.pose(p![eyeY: 0, headRot: 0]); knock(&s).await; shrug(&s).await; }
    else if open == 1 { watch(&s).await; shake_head(&s).await; }
    else if open == 2 { peek_down(&s).await; knock(&s).await; }
    else { scan(&s).await; sigh(&s).await; }
    queue_loop(s, env).await;
}
/// "Working…" is right there: go over, look up at it, point, the watch.
async fn nag_working(s: &S, env: &Env) {
    let (Some(sf), Some(w)) = (s.surf(), (env.activity)()) else { return };
    s.walk_to(clamp(w.1 + 30., sf.l, sf.r - 4.), 70.).await;
    let left = w.0 < s.x();
    let arm = if left { K::armL } else { K::armR };
    let mut o = P::new().with(K::turn, if left { -0.6 } else { 0.6 }).with(K::eyeY, -4.).with(K::headRot, if left { -9. } else { 9. });
    s.pose_ref(&o); s.wait(700.).await;
    let up = if w.2 < s.base_y() - 46. * s.k() { 128. } else { 96. };
    o.set(arm, up); s.pose_ref(&o); s.wait(260.).await;
    for _ in 0..3 {
        o.set(arm, up - 14.); s.pose_ref(&o); s.wait(140.).await;
        if let Some(t) = (env.activity)() { s.fx_at(F::Ring, t.0, t.2); }
        o.set(arm, up + 6.); s.pose_ref(&o); s.wait(170.).await;
    }
    s.pose(P::new().with(K::turn, 0.).with(K::eyeY, 0.).with(K::headRot, 0.).with(arm, 0.)); s.wait(300.).await;
    watch(s).await;
}
/// A background command is running in the corner: go and see what it is.
async fn check_badge(s: &S, env: &Env) {
    let (Some(sf), Some(_)) = (s.surf(), (env.badge)()) else { return };
    s.walk_to(sf.r - 2., 60.).await;
    s.pose(p![turn: 0.75, lean: 7, headRot: 8, eyeY: 2]); s.wait(900.).await;   // lean in and read it
    s.pose(p![armR: 96]);
    for _ in 0..2 {
        s.pose(p![armR: 84]); s.wait(130.).await;
        if let Some(t) = (env.badge)() { s.fx_at(F::Ring, t.0 + 6., t.3); }
        s.pose(p![armR: 100]); s.wait(170.).await;
    }
    s.pose(p![armR: 0, lean: 0, headRot: 0, eyeY: 0, turn: 0]); s.wait(300.).await;
    shrug(s).await;
}
pub fn queue_loop(s: S, env: Env) -> Boxed {
    Box::pin(async move {
        loop {
            let mut i = deal("queue", 11);
            if i == 9 && (env.activity)().is_none() { i = deal("queue", 11); }
            if i == 10 && (env.badge)().is_none() { i = deal("queue", 11); }
            if (i == 9 && (env.activity)().is_none()) || (i == 10 && (env.badge)().is_none()) { i = deal("queue-plain", 9); }
            match i {
                0 => pace(&s, 6, 33.).await, 1 => watch(&s).await, 2 => shake_head(&s).await, 3 => tap_foot(&s).await, 4 => knock(&s).await,
                5 => peek_down(&s).await, 6 => sigh(&s).await, 7 => scan(&s).await, 8 => paper_plane(&s).await,
                9 => nag_working(&s, &env).await, _ => check_badge(&s, &env).await,
            }
            s.rest(&[]);
            s.wait(rnd(500., 1300.)).await;
        }
    })
}
/// Another message has landed on the pile under its feet.
pub async fn another(s: &S, count: usize) {
    s.stop();
    s.pose(p![lid: -0.35, headY: -2, armL: 30, armR: 30]); s.fx(F::Bang, 48., -12.);     // what was that
    s.wait(360.).await;
    s.pose(p![lid: 0, headY: 6, eyeY: 4.5, squash: 0.93, armL: 22, armR: 22]);            // look down at it
    s.wait(820.).await;
    s.pose(p![headY: 0, eyeY: 0, squash: 1, armL: 0, armR: 0]);
    s.wait(200.).await;
    if count >= 3 {                                                                     // this is getting silly
        s.pose(p![armL: 150, armR: 150, headRot: -8, eyeY: -4]); s.wait(760.).await;
        s.pose(p![armL: 0, armR: 0, headRot: 0, eyeY: 0]);
        sigh(s).await;
    } else {
        s.pose(p![armL: -118, headY: 3, lid: 0.9, headRot: -6]); s.wait(900.).await;   // hand over its face
        s.pose(p![armL: 0, headY: 0, lid: 0, headRot: 0]); s.wait(300.).await;
        shake_head(s).await;
    }
    knock(s).await;                                                                     // and knock again, harder
}

/// A suggested reply is waiting: sit on its corner.
pub async fn scene_sit(s: S, layer_w: f32) {
    let Some(sf) = s.surf() else { return };
    enter(&s, layer_w + 40., (sf.r - 8.).min(sf.l + 60.), 70.).await;
    take_seat(&s).await;
    sit_loop(s).await;
}
pub async fn take_seat(s: &S) {
    let Some(sf) = s.surf() else { return };
    s.walk_to(sf.l + 15., 46.).await;
    s.wait(200.).await;
    s.pose(p![eyeY: 4, headY: 3]); s.wait(520.).await;                        // is it safe to sit on?
    s.pose(p![eyeY: 0, headY: 0]);
    s.hop(8.).await;
    s.pose(p![sit: 1, swing: 1, armL: 10, armR: 10]);
    s.wait(1300.).await;
}
pub fn sit_loop(s: S) -> Boxed {
    Box::pin(async move {
        loop {
            let r = deal("sit", 6);
            match r {
                0 => { s.pose(p![turn: 0.7, eyeY: 3, headRot: 7]); s.wait(1200.).await; s.pose(p![turn: 0, eyeY: 0, headRot: 0]); }   // look at the suggestion
                1 => { s.pose(p![armR: 62]); for _ in 0..3 { s.pose(p![armR: 40, turn: 0.5, eyeY: 3]); s.wait(170.).await; s.fx(F::Ring, 84., 78.); s.pose(p![armR: 62]); s.wait(210.).await; } s.pose(p![armR: 10, turn: 0, eyeY: 0]); }   // pat it
                2 => { s.pose(p![armL: 34, armR: 34, headRot: -9, lean: -5, lid: 0.5]); for j in 0..3 { s.fxo(F::Note, 70. + j as f32 * 4., 6., dx(8. + j as f32 * 5.)); s.wait(760.).await; } s.pose(p![armL: 10, armR: 10, headRot: 0, lean: 0, lid: 0]); }   // hum
                3 => { wave(&s, 3).await; s.pose(p![armR: 10]); }
                4 => { s.pose(p![swing: 2.1]); s.wait(1300.).await; s.pose(p![swing: 1]); }                                     // kick
                _ => { s.pose(p![headRot: -10, eyeY: -3.5, eyeX: -2]); s.wait(1500.).await; s.pose(p![headRot: 0, eyeY: 0, eyeX: 0]); }   // the ceiling
            }
            s.wait(rnd(1400., 3000.)).await;
        }
    })
}
/// The seat has gone: hang for a moment, fall, get up, leave.
pub async fn scene_fall(s: S, layer_w: f32, ground: SurfFn) {
    s.stop();
    s.pose(p![lid: -0.35, armL: 150, armR: 150, swing: 0, headY: -2, turn: 0, headRot: 0, lean: 0]);
    s.fx(F::Bang, 48., -12.);
    s.wait(330.).await;                                                       // before gravity notices
    s.pose(p![sit: 0, shake: 0.8, legL: 24, legR: -24]);
    s.fall_to(ground).await;
    s.pose(p![shake: 0, armL: 62, armR: 62, lid: 0.9, headY: 5, legL: 0, legR: 0]);
    s.wait(520.).await;
    for i in 0..4 { s.pose(p![headRot: if i % 2 == 1 { 9 } else { -9 }]); s.wait(150.).await; }       // shake it off
    s.pose(p![headRot: 0, lid: 0, headY: 0, armL: 0, armR: 0]);
    s.wait(260.).await;
    s.pose(p![eyeY: -4.5, headRot: -8]); s.wait(620.).await;                  // where the seat was
    s.pose(p![eyeY: 0, headRot: 0, lid: -0.3]);
    s.fx(F::Sweat, 70., 14.);
    s.wait(260.).await;
    run_off(&s, layer_w).await;
}

/// Where the nudge points.
#[derive(Clone)]
pub struct Aims { pub aim: PtFn, pub send_at: Option<PtFn>, pub sug_at: PtFn }

/// Nothing typed for a long time: stand by Send and make a case for the box.
pub async fn scene_nudge(s: S, layer_w: f32, mobile: bool, aims: Aims) {
    let Some(sf) = s.surf() else { return };
    let spot = if mobile { sf.l + (sf.r - sf.l) * 0.5 } else { sf.r - 70. };
    enter(&s, layer_w + 40., spot, 66.).await;
    s.wait(300.).await;
    wave(&s, 3).await;
    nudge_loop(s, 0, aims).await;
}
struct Aimed { left: bool, ang: f32, dy: f32 }
fn point_at(s: &S, t: Option<Pt>) -> Option<Aimed> {
    let t = t?;
    let (x, by, k) = (s.x(), s.base_y(), s.k());
    let (ddx, ddy) = (t.x - x, t.y - (by - 34. * k));
    Some(Aimed { left: ddx < 0., ang: clamp(ddx.abs().atan2(ddy).to_degrees(), 18., 172.), dy: ddy })
}
pub fn nudge_loop(s: S, from: usize, aims: Aims) -> Boxed {
    Box::pin(async move {
        let wide = aims.send_at.is_some();
        let turns = if from >= 99 { 0 } else { 6 + (random() * 3.) as usize };
        let key = if wide { "nudge-wide" } else { "nudge" };
        for n in 0..turns {
            let i = if n == 0 && from == 0 { 0 } else { deal(key, 11) };
            // The wide list: showAndTell, typing, orThat, ask, jacks, showAndTell, sitAwhile, watch, orThat, showAndTell, sigh.
            // The narrow: point, typing, orThat, ask, jacks, point, pace, sitAwhile, watch, orThat, sigh.
            let act = if wide { [0, 1, 2, 3, 4, 0, 6, 7, 2, 0, 8][i] } else { [9, 1, 2, 3, 4, 9, 5, 6, 7, 2, 8][i] };
            match act {
                0 => show_and_tell(&s, &aims).await,
                1 => typing(&s).await,
                2 => or_that(&s, &aims).await,
                3 => { s.pose(p![headRot: 11, armL: 44, armR: 44, eyeY: -1]); s.fx(F::Ask, 48., -16.); s.wait(1500.).await; s.pose(p![headRot: 0, armL: 0, armR: 0, eyeY: 0]); s.wait(300.).await; }
                4 => { for _ in 0..3 { s.pose(p![armL: 150, armR: 150, legL: 16, legR: -16]); s.hop(11.).await; s.pose(p![armL: 10, armR: 10, legL: 0, legR: 0]); s.wait(170.).await; } s.pose(p![armL: 0, armR: 0]); }   // jumping jacks
                5 => pace(&s, 2, 36.).await,
                6 => { s.hop(6.).await; s.pose(p![sit: 1, swing: 1, armL: 10, armR: 10]); s.wait(4200.).await; s.pose(p![sit: 0, swing: 0, armL: 0, armR: 0]); s.wait(620.).await; }   // sit a while
                7 => watch(&s).await,
                8 => sigh(&s).await,
                _ => point(&s, &aims).await,
            }
            s.rest(&[]);
            s.wait(rnd(900., 2000.) + n as f32 * 220.).await;
        }
        // It has tried everything: sit down and nod off until someone types.
        if s.t(K::sit) < 0.5 { s.hop(5.).await; }
        s.pose(p![sit: 1, swing: 0.5, armL: 8, armR: 8]);
        s.wait(1700.).await;
        s.pose(p![lid: 0.5, headY: 2]); s.wait(900.).await;
        s.pose(p![lid: 0.1, headY: 0]); s.wait(500.).await;
        s.with(|a| a.auto_blink = false);
        s.pose(p![lid: 0.94, headRot: 13, headY: 4, swing: 0]);
        // Not for ever: after a couple of minutes it wakes, stretches, goes home.
        let zs = 80 + (random() * 40.) as usize;
        for _ in 0..zs { s.fxo(F::Z, 66., 2., dx(10.)); s.wait(1500.).await; }
        s.with(|a| a.auto_blink = true);
        s.pose(p![lid: 0.5, headRot: 0, headY: 0]); s.wait(600.).await;
        s.pose(p![lid: 0.75, armL: 152, armR: 152, headY: -2, sit: 0]); s.wait(1100.).await;   // a stretch and a yawn
        s.pose(p![lid: 0, armL: 0, armR: 0, headY: 0]); s.wait(400.).await;
        s.with(|a| a.finished = true);
    })
}
async fn point(s: &S, aims: &Aims) {
    let Some(p) = point_at(s, (aims.aim)()) else { return };
    let arm = if p.left { K::armL } else { K::armR };
    let mut o = P::new().with(arm, p.ang).with(K::turn, if p.left { -0.55 } else { 0.55 }).with(K::eyeY, if p.dy < 0. { -3.5 } else { 3.5 }).with(K::headRot, if p.left { -6. } else { 6. });
    s.pose_ref(&o); s.wait(520.).await;
    for _ in 0..3 { o.set(arm, p.ang - 16.); s.pose_ref(&o); s.wait(150.).await; o.set(arm, p.ang + 4.); s.pose_ref(&o); s.wait(170.).await; }
    s.pose(p![turn: 0, eyeY: 0, headRot: 0]); s.wait(520.).await;              // look back at you, still pointing
    s.pose(P::new().with(arm, 0.)); s.wait(320.).await;
}
async fn typing(s: &S) {
    s.pose(p![armL: -52, armR: -52, headY: 2.5, eyeY: 3.5]);
    for i in 0..9 { s.pose(if i % 2 == 1 { p![armL: -66, armR: -40] } else { p![armL: -40, armR: -66] }); s.wait(110.).await; }
    s.pose(p![armL: 0, armR: 0, headY: 0, eyeY: 0]); s.wait(260.).await;
    s.fx(F::Dots, 48., -18.);
    s.wait(1500.).await;
}
/// Wide panes: walk to the start of the box and point at it, to Send and point at that.
async fn show_and_tell(s: &S, aims: &Aims) {
    let (Some(sf), Some(i0), Some(send_at)) = (s.surf(), (aims.aim)(), aims.send_at.clone()) else { return };
    let Some(s0) = send_at() else { return };
    async fn at(s: &S, sf: Surf, wh: &PtFn, x: f32) {
        s.walk_to(clamp(x, sf.l + 4., sf.r - 4.), 96.).await;
        let Some(p) = point_at(s, wh()) else { return };
        let arm = if p.left { K::armL } else { K::armR };
        let mut o = P::new().with(arm, p.ang).with(K::eyeY, 4.).with(K::headY, 2.5);
        s.pose_ref(&o); s.wait(300.).await;
        for _ in 0..2 {
            o.set(arm, p.ang + 14.); s.pose_ref(&o); s.wait(140.).await;
            if let Some(t) = wh() { s.fx_at(F::Ring, t.x, t.y); }
            o.set(arm, p.ang - 4.); s.pose_ref(&o); s.wait(190.).await;
        }
        s.pose(p![eyeY: 0, headY: 0]); s.wait(420.).await;                   // and look at you
        s.pose(P::new().with(arm, 0.)); s.wait(160.).await;
    }
    for _ in 0..2 { at(s, sf, &aims.aim, i0.x + 26.).await; at(s, sf, &send_at, s0.x - 26.).await; }
    s.pose(p![armL: 48, armR: 48, headRot: 8]); s.wait(700.).await;         // well?
    s.pose(p![armL: 0, armR: 0, headRot: 0]); s.wait(260.).await;
}
/// There is a suggested reply right there: or just tap that.
async fn or_that(s: &S, aims: &Aims) {
    if (aims.sug_at)().is_none() { return point(s, aims).await; }
    let Some(p) = point_at(s, (aims.sug_at)()) else { return };
    let arm = if p.left { K::armL } else { K::armR };
    let mut o = P::new().with(arm, p.ang).with(K::turn, if p.left { -0.55 } else { 0.55 }).with(K::eyeY, -3.5).with(K::headRot, if p.left { -7. } else { 7. });
    s.pose_ref(&o); s.wait(520.).await;
    for _ in 0..3 {
        o.set(arm, p.ang - 14.); s.pose_ref(&o); s.wait(150.).await;
        if let Some(u) = (aims.sug_at)() { s.fx_at(F::Ring, u.x, u.y); }
        o.set(arm, p.ang + 4.); s.pose_ref(&o); s.wait(180.).await;
    }
    s.pose(p![turn: 0, eyeY: 0, headRot: 0]); s.wait(600.).await;             // well, that one then
    s.pose(P::new().with(arm, 0.)); s.wait(320.).await;
}

// ─── Being tapped ───

fn steam(s: &S) { s.fxo(F::Puff, 22., 16., dx(-10.)); s.fxo(F::Puff, 74., 16., dx(10.)); }

/// On its feet: stamp, shoo, turn its back, a fist, step aside, leave and peek.
pub async fn annoyed(s: &S, which: usize, layer_w: f32) {
    match which {
        0 => {   // stamp
            s.pose(p![lid: 0.5, armL: -34, armR: -34, headY: 1]); s.fx(F::Anger, 76., 6.);
            s.wait(260.).await;
            s.hop(6.).await; s.hop(6.).await; s.hop(6.).await;
            steam(s); s.wait(700.).await;
        }
        1 => {   // shoo
            s.pose(p![lid: 0.45, turn: 0.35, headRot: 5]);
            for _ in 0..4 { s.pose(p![armR: 112]); s.wait(120.).await; s.pose(p![armR: 66]); s.wait(130.).await; }
            s.pose(p![armR: 0]); s.wait(360.).await;
        }
        2 => {   // turn away
            s.fx(F::Anger, 76., 6.);
            s.pose(p![turn: 2.1, armL: -36, armR: -36]); s.wait(700.).await;
            for _ in 0..4 { s.pose(p![legL: -22]); s.wait(140.).await; s.pose(p![legL: 0]); s.wait(160.).await; }
            s.pose(p![turn: 1.05, headRot: 6]); s.wait(820.).await;              // still there?
            s.pose(p![turn: 2.1, headRot: 0]); s.wait(620.).await;
            s.pose(p![turn: 0, armL: 0, armR: 0]); s.wait(460.).await;
        }
        3 => {   // a fist
            s.pose(p![lid: 0.5, lean: 5, armR: 158, shake: 0.55, headRot: 4]); s.fx(F::Anger, 20., 8.);
            s.wait(1100.).await;
            s.pose(p![shake: 0, armR: 0, lean: 0, headRot: 0]); s.wait(360.).await;
        }
        4 => {   // step aside
            let Some(sf) = s.surf() else { return };
            s.pose(p![lid: -0.35]); s.fx(F::Bang, 48., -12.); s.wait(200.).await;
            let x = s.x();
            let room: f32 = if x - sf.l > sf.r - x { -1. } else { 1. };
            let to = clamp(x + room * rnd(46., 80.), sf.l + 6., sf.r - 6.);
            s.pose(p![lid: 0]);
            s.walk_to(to, 150.).await;                                          // out of reach
            s.pose(p![turn: -room * 0.7, lid: 0.5, armL: -34, armR: -34]); s.wait(1100.).await;   // and glare back
            shake_head(s).await;
        }
        _ => {   // leave and peek
            let Some(sf) = s.surf() else { return };
            let home = s.x();
            let right = home > layer_w / 2.;
            let edge = if right { layer_w } else { 0. };
            s.pose(p![lid: 0.5]); s.fx(F::Anger, 76., 6.); s.wait(260.).await;
            s.with(|a| a.clamp_x = false);
            s.walk_to(if right { layer_w + 46. } else { -46. }, 170.).await;     // right out of the pane
            s.wait(rnd(1300., 2400.)).await;
            s.pose(p![lid: 0]);
            s.walk_to(edge + if right { 9. } else { -9. }, 26.).await;           // one eye round the corner
            s.pose(p![turn: if right { -0.6 } else { 0.6 }, headRot: if right { -8 } else { 8 }]);
            s.wait(1500.).await;
            s.pose(p![headRot: 0]);
            s.walk_to(clamp(home, sf.l + 6., sf.r - 6.), 50.).await;
            s.with(|a| a.clamp_x = true);
            s.wait(260.).await;
        }
    }
}
pub const ANNOYED_N: usize = 6;
/// On its seat: a glare and a kick, shoo, humph, hop down.
pub async fn annoyed_seated(s: &S, which: usize) {
    match which {
        0 => { s.pose(p![lid: 0.5, swing: 2.4, armL: -30, armR: -30]); s.fx(F::Anger, 76., 6.); s.wait(1500.).await; s.pose(p![lid: 0, swing: 1, armL: 10, armR: 10]); }
        1 => { s.pose(p![lid: 0.45, turn: 0.35]); for _ in 0..4 { s.pose(p![armR: 112]); s.wait(120.).await; s.pose(p![armR: 66]); s.wait(130.).await; } s.pose(p![armR: 10, lid: 0, turn: 0]); s.wait(300.).await; }
        2 => { s.pose(p![turn: 1.7, armL: -34, armR: -34, swing: 0.2]); s.wait(1500.).await; s.pose(p![turn: 1.0]); s.wait(700.).await; s.pose(p![turn: 0, armL: 10, armR: 10, swing: 1]); s.wait(400.).await; }
        _ => {
            let Some(sf) = s.surf() else { return };
            s.pose(p![sit: 0, swing: 0, lid: 0.5]); s.fx(F::Anger, 76., 6.); s.wait(380.).await;
            s.walk_to(clamp(s.x() + 56., sf.l + 6., sf.r - 6.), 110.).await;
            s.pose(p![turn: -0.7, armL: -34, armR: -34]); s.wait(1000.).await;
            s.pose(p![turn: 0, armL: 0, armR: 0, lid: 0]);
            take_seat(s).await;
        }
    }
}
pub const SEATED_N: usize = 4;
/// Prodded awake.
pub async fn grumpy_wake(s: &S) {
    s.pose(p![lid: -0.35, headRot: 0, headY: -2]); s.fx(F::Bang, 48., -12.); s.wait(420.).await;
    s.pose(p![lid: 0.5, headY: 0]); s.fx(F::Anger, 76., 6.); s.wait(500.).await;
    shake_head(s).await;
}
/// The cursor is coming and it remembers: back away; with nowhere to go, duck and cover.
async fn flee(s: &S, from_x: f32, panic: bool) {
    let Some(sf) = s.surf() else { return };
    let x = s.x();
    let dir: f32 = if x >= from_x { 1. } else { -1. };
    let room = if dir > 0. { sf.r - 6. - x } else { x - (sf.l + 6.) };
    s.stop();
    if s.t(K::sit) > 0.5 || room < 26. {
        s.pose(p![squash: 0.8, armL: 156, armR: 156, lid: 0.9, shake: 0.45, headY: 5, lean: dir * 9., swing: 0, turn: 0]);
        s.fx(F::Sweat, if dir > 0. { 24. } else { 72. }, 14.);
        s.wait(1300.).await;
        s.pose(p![squash: 1, shake: 0, lid: 0.35, headY: 0, lean: 0, armL: 40, armR: 40]);
        s.wait(520.).await;                                                     // peep out
        return;
    }
    if panic {
        s.pose(p![lid: -0.35, armL: 150, armR: 150, shake: 0.7, headY: -2]);
        s.fx(F::Sweat, if dir > 0. { 24. } else { 72. }, 14.);
        s.wait(150.).await;
        s.walk_to(s.x() + dir * room.min(rnd(80., 130.)), 190.).await;
        s.pose(p![shake: 0.25, armL: 60, armR: 60, turn: -dir * 0.75, lid: -0.2]);   // look back, still trembling
        s.wait(800.).await;
        s.pose(p![shake: 0, armL: 0, armR: 0, lid: 0]);
        return;
    }
    // Backing off, without taking its eyes off the thing.
    s.pose(p![lid: 0.35, armL: 34, armR: 34, lean: dir * 4.]);
    let going = s.walk_to(s.x() + dir * room.min(rnd(46., 74.)), 78.);
    s.with(|a| a.t[K::turn as usize] = -dir * 0.7);                             // walking backwards
    going.await;
    s.pose(p![turn: -dir * 0.7, lean: 0, armL: 0, armR: 0]);
    s.wait(260.).await;
}
/// The cursor: (its x, its distance from Mini's middle).
pub type MouseFn = Rc<dyn Fn() -> Option<(f32, f32)>>;
/// On edge, and the cursor hangs about: watch it; if it will not leave, put some distance in.
pub async fn eye_it(s: &S, mouse: MouseFn) {
    let mut t0 = s.time();
    let mut moves = 0;
    loop {
        let Some(m) = mouse().filter(|m| m.1 <= 125.) else { break };
        let dir: f32 = if m.0 > s.x() { 1. } else { -1. };
        s.pose(p![lid: 0.42, turn: dir * 0.6, headRot: dir * 5., armL: -22, armR: -22, headY: 0]);   // stare at it
        s.wait(rnd(480., 760.)).await;
        if mouse().filter(|m| m.1 <= 125.).is_none() { break; }
        s.pose(p![turn: -dir * 0.35, headRot: 0]);                             // look away; the eyes stay on it
        s.wait(rnd(300., 520.)).await;
        let Some(m) = mouse().filter(|m| m.1 <= 125.) else { break };
        if s.time() - t0 > 2.0 || m.1 < 42. { moves += 1; flee(s, m.0, moves >= 3 || m.1 < 30.).await; t0 = s.time(); }
    }
    s.pose(p![lid: 0, turn: 0, headRot: 0, armL: 0, armR: 0]);
    s.wait(360.).await;
}
/// Enough: shake a fist, stamp off.
pub async fn storm_off(s: S, layer_w: f32) {
    s.stop(); s.with(|a| a.auto_blink = true);
    s.pose(p![sit: 0, swing: 0, lid: 0.5, turn: 0, headRot: 0, headY: 0, armL: 0, armR: 158, shake: 0.6, lean: 5]);
    s.fx(F::Anger, 76., 6.); s.fx(F::Anger, 22., 10.);
    s.wait(900.).await;
    s.pose(p![shake: 0, armR: -34, armL: -34, lean: 0]);
    steam(&s);
    s.with(|a| a.clamp_x = false);
    s.walk_to(if s.x() < layer_w / 2. { -50. } else { layer_w + 50. }, 105.).await;
    s.pose(p![alpha: 0]); s.wait(200.).await;
}
/// Someone started typing: wake if asleep, salute, go.
pub async fn scene_bye(s: S, layer_w: f32) {
    s.stop(); s.with(|a| a.auto_blink = true);
    if s.t(K::lid) > 0.8 { s.pose(p![lid: -0.35, headRot: 0, headY: -2]); s.fx(F::Bang, 48., -12.); s.wait(360.).await; }
    s.pose(p![sit: 0, swing: 0, lid: 0, headRot: 0, headY: 0, eyeX: 0, eyeY: 0, turn: 0, armL: 0, armR: 0, legL: 0, legR: 0]);
    s.wait(260.).await;
    s.pose(p![armR: 148, headRot: 5]); s.wait(300.).await;                    // salute
    s.pose(p![armR: 0, headRot: 0]);
    s.hop(10.).await;
    let to = if s.x() < layer_w / 2. { -50. } else { layer_w + 50. };
    s.with(|a| a.clamp_x = false);
    s.walk_to(to, 150.).await;
    s.pose(p![alpha: 0]); s.wait(200.).await;
}

// ─── A sleeping chat ───

#[derive(Clone)]
pub struct Parked { pub resume: PtFn, pub compact: PtFn, pub compact_surf: SurfFn }

/// In a parked chat, when it feels like it: stand on Resume and show it;
/// a long conversation worries it, and it makes a case for "Compact and resume".
pub async fn scene_parked(s: S, layer_w: f32, get: Parked) {
    let Some(sf) = s.surf() else { return };
    enter(&s, if random() < 0.5 { -40. } else { layer_w + 40. }, (sf.l + sf.r) / 2., 62.).await;
    s.wait(300.).await;
    s.pose(p![eyeX: -3, eyeY: -2]); s.wait(600.).await; s.pose(p![eyeX: 3]); s.wait(600.).await;   // dark in here
    s.pose(p![eyeX: 0, eyeY: 0]);
    if random() < 0.5 { s.pose(p![lid: 0.7, armL: 150, armR: 150, headY: -2]); s.wait(900.).await; s.pose(p![lid: 0, armL: 0, armR: 0, headY: 0]); }   // a yawn
    async fn press(s: &S, wh: &PtFn) {
        if wh().is_none() { return; }
        s.pose(p![headY: 4, eyeY: 4.5, armL: 26, armR: 26]); s.wait(500.).await;
        for _ in 0..2 { s.hop(6.).await; if let Some(u) = wh() { s.fx_at(F::Ring, u.x, u.y); } }
        s.pose(p![headY: 0, eyeY: 0, armL: 0, armR: 0]); s.wait(300.).await;
        let arm = if random() < 0.5 { K::armL } else { K::armR };
        s.pose(P::new().with(arm, 24.)); s.wait(250.).await;
        for _ in 0..3 { s.pose(P::new().with(arm, 14.)); s.wait(140.).await; s.pose(P::new().with(arm, 30.)); s.wait(160.).await; }
        s.pose(P::new().with(arm, 0.)); s.wait(500.).await;
    }
    press(&s, &get.resume).await;
    if (get.compact)().is_some() {
        s.pose(p![headY: 6, eyeY: 4.5, squash: 0.93]); s.wait(1100.).await;      // read it
        s.pose(p![headY: -2, eyeY: 0, squash: 1, lid: -0.35]); s.fx(F::Bang, 48., -12.); s.wait(500.).await;
        s.fx(F::Sweat, 72., 14.);
        s.pose(p![armL: -118, armR: -118, lid: 0.3, headY: 3]); s.wait(900.).await;   // hands to its face
        s.pose(p![armL: 0, armR: 0, headY: 0, lid: 0]);
        pace(&s, 2, 40.).await;
        if let Some(cs) = (get.compact_surf)() {
            s.pose(p![headY: 5, eyeY: 4.5]); s.wait(500.).await;
            s.pose(p![headY: 0, eyeY: 0, armL: 120, armR: 120]); s.wait(200.).await;
            let held = Surf { y: cs.y, l: -100., r: layer_w + 100. };
            let cf = get.compact_surf.clone();
            s.fall_to(Rc::new(move || Some(cf().map_or(held, |f| Surf { y: f.y, l: -100., r: layer_w + 100. })))).await;
            s.with(|a| a.surf = Some(get.compact_surf.clone()));
            s.pose(p![armL: 0, armR: 0]); s.wait(300.).await;
            if let Some(c2) = (get.compact_surf)() { s.walk_to((c2.l + c2.r) / 2., 50.).await; }
            loop {
                press(&s, &get.compact).await;
                s.pose(p![armL: -60, armR: -60, headRot: 7, eyeY: -2]); s.wait(1300.).await;     // please
                s.pose(p![armL: 0, armR: 0, headRot: 0, eyeY: 0]);
                s.wait(rnd(2500., 5000.)).await;
                if random() < 0.4 { watch(&s).await; }
            }
        }
    }
    // Nothing to worry about: keep it company, then doze off on the button.
    for n in 0..3 {
        s.wait(rnd(2200., 4200.)).await;
        if n == 1 { wave(&s, 3).await; } else { press(&s, &get.resume).await; }
    }
    s.hop(5.).await;
    s.pose(p![sit: 1, swing: 0.6, armL: 8, armR: 8]); s.wait(2600.).await;
    s.pose(p![lid: 0.5, headY: 2]); s.wait(900.).await;
    s.with(|a| a.auto_blink = false);
    s.pose(p![lid: 0.94, headRot: 13, headY: 4, swing: 0]);
    let zs = 60 + (random() * 40.) as usize;
    for _ in 0..zs { s.fxo(F::Z, 66., 2., dx(10.)); s.wait(1500.).await; }
    s.with(|a| a.auto_blink = true);
    s.pose(p![lid: 0.5, headRot: 0, headY: 0, sit: 0]); s.wait(700.).await;   // enough of that: home
    s.pose(p![lid: 0]); s.wait(300.).await;
    s.with(|a| a.finished = true);
}
/// Resumed: what it stood on is gone. Glad, down it comes (a parachute from
/// up there), and off it runs.
pub async fn handed_over(s: S, ground: SurfFn, w: f32) {
    s.rest(&[]); s.pose(p![sit: 0, swing: 0, lid: -0.35, headY: -2, armL: 150, armR: 150]); s.fx(F::Bang, 48., -12.);
    s.wait(380.).await;
    s.pose(p![legL: 22, legR: -22]);
    s.fall_to(ground).await;
    s.pose(p![legL: 0, legR: 0, lid: 0.5, armL: 160, armR: 160, headY: 0]);
    s.wait(260.).await;
    s.hop(11.).await; s.fx(F::Spark, 20., 2.); s.hop(11.).await; s.fx(F::Spark, 76., 2.);
    s.pose(p![armL: 0, armR: 0, lid: 0]); s.wait(200.).await;
    run_off(&s, w).await;
}

// ─── It is the logo ───

/// Out of the logo: the head grows a body, arms and legs, and walks off.
pub async fn logo_leave(s: S, edge: f32) {
    s.wait(260.).await;
    s.pose(p![eyeX: -3]); s.wait(300.).await; s.pose(p![eyeX: 3]); s.wait(300.).await; s.pose(p![eyeX: 0]);   // is anyone looking
    s.wait(160.).await;
    s.pose(p![grow: 1]);                                                       // body, arms, legs
    s.wait(950.).await;
    s.pose(p![armL: 150, armR: 150, headY: -1.5]); s.wait(380.).await;         // a stretch
    s.pose(p![armL: 0, armR: 0, headY: 0]); s.wait(260.).await;
    s.walk_to(edge, 62.).await;
}
/// Back along the top bar at the pace it left with, and folded away.
pub async fn logo_return(s: S, home_x: f32, sp: f32) {
    if sp > 170. { s.pose(p![armL: 162, armR: 162, shake: 1, lid: -0.3]); }    // it left running
    else if sp > 95. && sp < 125. { s.pose(p![lid: 0.5, armL: -34, armR: -34]); }   // it stormed off: still cross
    s.walk_to(home_x, sp).await;
    s.pose(p![shake: 0, armL: 0, armR: 0, lid: 0, turn: 0]);
    s.wait(if sp > 170. { 420. } else { 240. }).await;
    if sp > 170. { sigh(&s).await; }                                           // out of breath
    s.pose(p![eyeX: -3]); s.wait(240.).await; s.pose(p![eyeX: 3]); s.wait(240.).await; s.pose(p![eyeX: 0]);
    s.pose(p![grow: 0, squash: 1, headY: 0, headRot: 0, lean: 0]);             // fold away
    s.wait(820.).await;
}

// ─── A life in the top bar ───

/// What the top bar shows (the web's topRead), window coordinates.
#[derive(Clone, Default)]
pub struct TopRead {
    /// (end x, pct, top, bottom, left, right) of the 5-hour and weekly rows.
    pub five: Option<(f32, f32, f32, f32, f32, f32)>,
    pub week: Option<(f32, f32, f32, f32, f32, f32)>,
    pub update: bool,
    pub tabs: bool,
    pub settings: bool,
    pub bell: bool,
    pub project: Option<String>,
    pub waiting: usize,
}

/// The top bar's stage: where home is, the window, what it can reach.
#[derive(Clone)]
pub struct Top {
    pub spots: SpotsRef,
    pub w: f32,
    pub h: f32,
    pub home: (f32, f32),
    pub edge: f32,
    pub force: Option<String>,
    pub party_for: String,
    /// The tabs' scroll (the update scene hauls them), and set when it moved them.
    pub tabs: Option<gpui::ScrollHandle>,
    pub tabs_moved: Rc<Cell<bool>>,
    /// The layer's height (the project scene takes the whole window).
    pub clip_h: Rc<Cell<f32>>,
}
impl Top {
    fn spot(&self, n: &str) -> Option<Bounds<Pixels>> { self.spots.borrow().get(n) }
    fn rows(&self) -> Vec<(String, Bounds<Pixels>)> { self.spots.borrow().all("sidebar:") }
}
fn f(p: Pixels) -> f32 { f32::from(p) }

/// Straining at something that will not give.
async fn strain(s: &S, lean: f32, ms: f32, dust_dx: f32) {
    let end = s.time() + ms / 1000.;
    let mut i = 0;
    while s.time() < end {
        let odd = i % 2 == 1;
        s.pose(p![legL: if odd { 26 } else { -22 }, legR: if odd { -22 } else { 26 }, lean: lean * if odd { 1. } else { 0.72 }, squash: if odd { 0.93 } else { 0.97 }, lid: 0.86, headY: 2]);
        if i % 3 == 0 { s.fxo(F::Dust, 48. + dust_dx, GROUND - 3., dx(if dust_dx > 0. { 9. } else { -9. })); }
        if i % 4 == 1 { s.fx(F::Sweat, 48. + dust_dx * 0.5, 10.); }
        s.wait(150.).await; i += 1;
    }
    s.pose(p![legL: 0, legR: 0]);
}
async fn worn(s: &S) {
    s.pose(p![reach: 0, armL: 0, armR: 0, lean: 0, squash: 0.9, headY: 5, lid: 0.8, turn: 0, headRot: 0]);
    s.fx(F::Puff, 64., 34.); s.wait(700.).await;
    s.fx(F::Puff, 30., 34.); s.wait(600.).await;
    if random() < 0.6 { s.pose(p![sit: 1, swing: 0, armL: 20, armR: 20, squash: 1]); s.wait(rnd(1500., 2400.)).await; s.pose(p![sit: 0]); s.wait(600.).await; }
    s.pose(p![squash: 1, headY: 0, lid: 0.2]);
    s.pose(p![armR: -118]); s.wait(700.).await; s.pose(p![armR: 0]); s.wait(250.).await;   // wipe its brow
}
#[derive(Clone, Copy, PartialEq)]
pub enum Mood { Calm, Tired }

/// The five-hour limit is over 90%: panic, drag it back, shove it, plead.
pub async fn scene_five(s: &S, st: &Top, r: &TopRead) -> Mood {
    let Some(five) = r.five else { return Mood::Calm };
    let (k, end, w) = (s.k(), five.0, st.w);
    s.pose(p![turn: if end > s.x() { 0.8 } else { -0.8 }, eyeX: 0]); s.wait(500.).await;
    s.pose(p![lid: -0.35, headY: -2, armL: 150, armR: 150, shake: 0.9]); s.fx(F::Bang, 48., -14.);
    s.wait(620.).await;
    let room_right = w.min(five.5) - end;
    let far = room_right > 44. * k + 8.;
    let ways: &[&str] = if far { &["pull", "push", "back", "charge", "plead"] } else { &["pull", "plead"] };
    let rounds = 1 + (random() * 2.) as usize;
    for n in 0..rounds {
        let way: String = st.force.clone().filter(|f| ways.contains(&f.as_str())).unwrap_or_else(|| ways[deal(if far { "five-far" } else { "five-near" }, ways.len())].to_string());
        let heaves = 1 + (random() * 3.) as usize;
        match way.as_str() {
            "pull" => {
                s.walk_to(clamp(end - 5. * k, 12., w - 12.), 150.).await;
                s.pose(p![shake: 0, lid: 0, turn: 0.8, reach: 1, armR: 100, armL: -92, headY: 0]); s.wait(360.).await;   // take hold
                for _ in 0..heaves {
                    s.pose(p![lean: -34, squash: 0.94, armR: 124, armL: -114, headRot: -8]);
                    strain(s, -34., rnd(800., 1700.), 14.).await;
                    s.pose(p![lean: -12, squash: 1, lid: 0.3, armR: 104, armL: -96, headRot: 0]); s.wait(rnd(200., 420.)).await;
                }
            }
            "push" => {
                s.walk_to(clamp(end + 33. * k, 12., w - 12.), 150.).await;
                s.pose(p![shake: 0, lid: 0, turn: -0.8, reach: -1, armL: 90, armR: -80, headY: 0]); s.wait(360.).await;
                for _ in 0..heaves { strain(s, -19., rnd(700., 1600.), 14.).await; s.pose(p![lean: -5, squash: 1, lid: 0.3]); s.wait(rnd(200., 420.)).await; }
            }
            "back" => {
                s.walk_to(clamp(end + 23. * k, 12., w - 12.), 150.).await;
                s.pose(p![shake: 0, lid: 0, turn: 0.8, armL: 26, armR: 26, headRot: -9, headY: 0]); s.wait(360.).await;
                for _ in 0..heaves { strain(s, -17., rnd(800., 1700.), 14.).await; s.pose(p![lean: -6, squash: 1, lid: 0.3]); s.wait(rnd(200., 420.)).await; }
            }
            "charge" => {
                s.walk_to(clamp(end + 30. * k + rnd(46., 70.), 12., w - 12.), 120.).await;
                s.pose(p![shake: 0, lid: 0.5, turn: -0.8, lean: -6, headY: 1]);
                for _ in 0..3 { s.pose(p![legR: 34]); s.wait(150.).await; s.fxo(F::Dust, 66., GROUND - 3., dx(10.)); s.pose(p![legR: 0]); s.wait(170.).await; }   // paw the ground
                s.pose(p![lean: -12, armL: 40, armR: 40]);
                s.walk_to(clamp(end + 20. * k, 12., w - 12.), 230.).await;
                s.fx_at(F::Ring, end, s.base_y() - 12. * k); s.fx(F::Bang, 48., -14.);
                s.with(|a| { a.vx = 150.; a.vel[K::squash as usize] -= 3.; });     // and straight back off it
                s.pose(p![lid: 0.94, lean: 14, armL: 150, armR: 150, turn: 0]);
                s.hop(7.).await;
                s.pose(p![sit: 1, swing: 0, lean: 0, armL: 30, armR: 30, headY: 4]);
                for d2 in 0..5 { s.pose(p![headRot: if d2 % 2 == 1 { 10 } else { -10 }]); s.wait(170.).await; }   // stars
                s.pose(p![headRot: 0, lid: 0.3, sit: 0, headY: 0, armL: 0, armR: 0]); s.wait(650.).await;
            }
            _ => {   // ask it nicely
                s.walk_to(clamp(end + (if far { 30. } else { -30. }) * k, 12., w - 12.), 110.).await;
                let fd: f32 = if far { -1. } else { 1. };
                s.pose(p![shake: 0, lid: 0, turn: fd * 0.8, squash: 0.86, headY: 3, reach: fd, armL: if fd < 0. { 62 } else { -52 }, armR: if fd < 0. { -52 } else { 62 }]); s.wait(1300.).await;   // on its knees
                s.pose(p![turn: 0, eyeY: -3, headRot: 6]); s.wait(900.).await;   // then to you
                s.pose(p![turn: fd * 0.8, eyeY: 0, headRot: 0]); s.wait(900.).await;
                s.pose(p![squash: 1, headY: 0, reach: 0, armL: 0, armR: 0]); s.wait(350.).await;
            }
        }
        if way != "plead" || random() < 0.4 { worn(s).await; } else { s.pose(p![turn: 0]); sigh(s).await; }
        if n < rounds - 1 { s.pose(p![turn: if end > s.x() { 0.8 } else { -0.8 }, lid: 0.45]); s.wait(700.).await; }
    }
    let after = deal("five-after", 3) as f32 / 3.;
    if after < 0.34 {                                                          // kick it, regret it
        s.pose(p![turn: if end > s.x() { 0.7 } else { -0.7 }, lid: 0.5]); s.wait(300.).await;
        s.pose(p![legR: if end > s.x() { -70 } else { 70 }]); s.wait(170.).await; s.fx_at(F::Ring, end, s.base_y() - 8. * k);
        s.pose(p![legR: 0, lid: -0.35]); s.fx(F::Bang, 48., -14.);
        for _ in 0..4 { s.hop(5.).await; }
    } else if after < 0.67 { s.pose(p![turn: 0]); shrug(s).await; }
    else { s.pose(p![turn: if end > s.x() { 0.7 } else { -0.7 }]); shake_head(s).await; }
    Mood::Tired
}
/// The weekly limit is over 90%: that bar is out of reach. It jumps. And cannot.
pub async fn scene_week(s: &S, st: &Top, r: &TopRead) -> Mood {
    let Some(week) = r.week else { return Mood::Calm };
    let (k, end, w) = (s.k(), week.0, st.w);
    s.pose(p![eyeY: -4.5, headRot: -7]); s.wait(700.).await;                   // up there
    s.pose(p![lid: -0.3]); s.fx(F::Bang, 48., -14.); s.wait(420.).await;
    s.pose(p![lid: 0, eyeY: 0, headRot: 0]);
    s.walk_to(clamp(end - 6. * k, 12., w - 12.), 120.).await;
    let tries = 3 + (random() * 3.) as usize;
    for i in 0..tries {
        s.pose(p![squash: 0.86, armL: 30, armR: 30, eyeY: -4.5]); s.wait(220.).await;     // crouch
        s.pose(p![squash: 1, armL: 170, armR: 170]);
        s.hop(7. + i as f32 * 1.2).await;                                      // never high enough
        s.pose(p![armL: 0, armR: 0]); s.wait(rnd(160., 320.)).await;
    }
    let after = deal("week-after", 3) as f32 * 0.34;
    if after < 0.4 {                                                           // on tiptoe, trembling
        s.pose(p![squash: 1.09, armR: 172, headY: -3, shake: 0.3, eyeY: -4.5]); s.wait(1500.).await;
        s.pose(p![squash: 1, armR: 0, headY: 0, shake: 0, eyeY: 0]);
    } else if after < 0.7 {                                                    // a fist at it
        s.pose(p![armR: 158, shake: 0.55, lid: 0.5, eyeY: -4]); s.fx(F::Anger, 76., 6.); s.wait(1100.).await;
        s.pose(p![armR: 0, shake: 0, lid: 0, eyeY: 0]);
    }
    worn(s).await;
    Mood::Tired
}
/// There is an update: down onto the tabs, along to Settings (hauling the
/// tabs along when it is off the side), point at it, back upstairs.
pub async fn scene_update(s: &S, st: &Top) -> Mood {
    let (k, w) = (s.k(), st.w);
    if st.spot("tabbar").is_none() || st.spot("tab:settings").is_none() { return Mood::Calm; }
    let sp = st.spots.clone();
    let floor: SurfFn = Rc::new(move || sp.borrow().get("tabbar").map(|b| Surf { y: f(b.bottom()) - 1., l: -200., r: w + 200. }));
    let seen = || match (st.spot("tab:settings"), st.spot("tabbar")) { (Some(b), Some(t)) => f(b.left()) >= f(t.left()) - 2. && f(b.right()) <= f(t.right()).min(w) + 2., _ => false };
    let scroll_left = || st.tabs.as_ref().map_or(0., |h| -f(h.offset().x));
    let set_scroll = |x: f32| -> f32 {
        let Some(h) = &st.tabs else { return 0. };
        let max = f(h.max_offset().x);
        let o = h.offset();
        h.set_offset(gpui::point(px(-clamp(x, 0., max)), o.y));
        st.tabs_moved.set(true);
        clamp(x, 0., max)
    };
    s.pose(p![eyeY: 4, headY: 3]); s.wait(600.).await;                         // what is that down there
    s.pose(p![eyeY: 0, headY: 0, armL: 120, armR: 120]); s.wait(200.).await;
    s.fall_to(floor.clone()).await;
    s.pose(p![armL: 0, armR: 0]); s.wait(380.).await;
    let mut n = 0;
    while n < 9 && !seen() {
        n += 1;
        let Some(t) = st.spot("tabbar") else { break };
        let from = clamp(f(t.right()).min(w) - 26., 30., w - 14.);
        s.walk_to(from, 70.).await;
        s.pose(p![turn: 0.75, reach: 1, armR: 90, armL: -80]); s.wait(300.).await;   // get a grip on it
        let (x0, base) = (s.x(), scroll_left());
        let going = s.walk_to(from - rnd(42., 58.), 13.);
        s.with(|a| a.t[K::turn as usize] = 0.75);                               // backwards, leaning into it
        let mut i = 0;
        let mut set = scroll_left();
        let mut fate: Option<&str> = None;
        while s.walking() {
            // Somebody else has hold of the tabs: the person is dragging them.
            if (scroll_left() - set).abs() > 2. { fate = Some(carried(s, st, set).await); break; }
            set = set_scroll(base + (x0 - s.x()));
            s.pose(p![lean: if i % 2 == 1 { -19 } else { -13 }, squash: if i % 2 == 1 { 0.93 } else { 0.97 }, lid: 0.85, headY: 2]);
            if i % 5 == 0 { s.fx(F::Sweat, 40., 10.); }
            if i % 4 == 0 { s.fxo(F::Dust, 62., GROUND - 3., dx(9.)); }
            s.wait(120.).await; i += 1;
        }
        if fate == Some("flung") { return stomp_back(s, st).await; }
        if fate.is_none() { going.await; }
        s.pose(p![reach: 0, lean: 0, squash: 1, lid: 0.3, armL: 0, armR: 0, headY: 0, turn: 0, shake: 0, legL: 0, legR: 0]); s.wait(260.).await;
        if fate == Some("dropped") { shake_head(s).await; continue; }
        if !seen() { s.fx(F::Puff, 64., 34.); s.pose(p![squash: 0.92, headY: 4, lid: 0.7]); s.wait(rnd(600., 1000.)).await; s.pose(p![squash: 1, headY: 0, lid: 0]); }
    }
    let Some(b) = st.spot("tab:settings") else { return Mood::Calm };
    let (bx, by) = ((f(b.left()) + f(b.right())) / 2., (f(b.top()) + f(b.bottom())) / 2.);
    let side: f32 = if bx - 22. * k > 12. { -1. } else { 1. };
    s.walk_to(clamp(bx + side * 22. * k, 10., w - 10.), 66.).await;
    let arm = if side < 0. { K::armR } else { K::armL };
    let mut o = P::new().with(K::turn, if side < 0. { 0.6 } else { -0.6 }).with(K::eyeY, -1.);
    for rep in 0..2 {
        o.set(arm, 104.); s.pose_ref(&o); s.wait(320.).await;
        for _ in 0..3 { o.set(arm, 92.); s.pose_ref(&o); s.wait(130.).await; s.fx_at(F::Ring, bx, by); o.set(arm, 110.); s.pose_ref(&o); s.wait(180.).await; }
        s.pose(p![turn: 0, eyeY: 0]); s.wait(700.).await;                      // this. here.
        if rep == 0 { s.hop(6.).await; s.hop(6.).await; }
    }
    s.pose(P::new().with(arm, 0.)); s.wait(300.).await;
    // and back upstairs
    let home = s.with(|a| a.home_surf.clone());
    if let Some(h) = home { s.jump_to(h).await; }
    s.wait(260.).await;
    Mood::Calm
}
/// It was holding the tabs and the person dragged them: it hangs on; dragged
/// far enough, or flicked, it leaves the screen with them.
async fn carried(s: &S, st: &Top, grip_scroll: f32) -> &'static str {
    let scroll_left = || st.tabs.as_ref().map_or(0., |h| -f(h.offset().x));
    let (grip_x, w) = (s.x(), st.w);
    let mut prev = scroll_left();
    let (mut still, mut v) = (0, 0f32);
    s.stop();
    let tabs = st.tabs.clone();
    s.with(|a| a.carry = Some(Rc::new(move || grip_x - (tabs.as_ref().map_or(0., |h| -f(h.offset().x)) - grip_scroll))));
    s.pose(p![lid: -0.35, reach: 1, armR: 90, armL: -80, turn: 0.75, shake: 0.35, squash: 1, headY: -1]); s.fx(F::Bang, 48., -14.);
    let mut out = "dropped";
    while still < 6 {
        s.wait(70.).await;
        let cur = scroll_left();
        let d = cur - prev;
        if d != 0. { v = -d / 0.07; still = 0; s.pose(p![lean: clamp(d * 2.2, -34., 34.), legL: clamp(d * 3., -50., 50.), legR: clamp(d * 3. + 14., -50., 50.)]); } else { still += 1; }
        prev = cur;
        if s.x() < -30. || s.x() > w + 30. { out = "flung"; break; }
        if d == 0. && v.abs() > 260. {
            s.with(|a| a.carry = None);
            s.pose(p![armL: 160, armR: 160, shake: 1, lean: if v > 0. { 30 } else { -30 }]);
            s.walk_to(if v > 0. { w + 60. } else { -60. }, clamp(v.abs(), 300., 700.)).await;
            out = "flung"; break;
        }
        if still > 2 { v = 0.; }
    }
    s.with(|a| a.carry = None);
    out
}
/// Thrown off the screen: back along the top bar, cross or dizzy (and
/// half of those times its legs give out on the way), and home.
async fn stomp_back(s: &S, st: &Top) -> Mood {
    let w = st.w;
    let from = if s.x() < w / 2. { -46. } else { w + 46. };
    s.rest(&[]); s.pose(p![alpha: 0]); s.wait(rnd(1100., 1900.)).await;
    s.with(|a| { a.surf = a.home_surf.clone(); a.base_y = st.home.1; a.y_off = 0.; a.air = false; a.x = from; a.vx = 0.; });
    let how = st.force.clone().filter(|f| f == "cross" || f == "dizzy" || f == "faint");
    if how.as_deref().map_or(random() < 0.5, |h| h == "cross") {
        s.pose(p![alpha: 1, lid: 0.5, armL: -34, armR: -34]);
        let going2 = s.walk_to(st.home.0, 46.);
        let mut n2 = 0;
        while s.walking() { if n2 % 6 == 0 { s.fx(F::Anger, 76., 4.); } n2 += 1; s.wait(160.).await; }
        going2.await;
        s.pose(p![turn: 0]); s.wait(500.).await;                               // at you
        match deal("flung-cross", 3) {
            0 => { s.pose(p![armL: 0, armR: 158, shake: 0.55, lean: 4]); s.wait(1000.).await; s.pose(p![shake: 0, armR: 0, lean: 0]); }
            1 => shake_head(s).await,
            _ => { steam(s); s.hop(5.).await; s.hop(5.).await; }
        }
        s.pose(p![armL: 0, armR: 0, lid: 0]); s.wait(300.).await;
        return Mood::Calm;
    }
    let faint = how.as_deref().map_or(random() < 0.5, |h| h == "faint");
    let stop_at = if faint { from + (st.home.0 - from) * rnd(0.35, 0.7) } else { st.home.0 };
    s.pose(p![alpha: 1, lid: 0.62]);
    // stagger: not in a straight line
    {
        let mut g = s.walk_to(stop_at, 24.);
        let mut i = 0;
        while s.walking() {
            let odd = i % 2 == 1;
            s.pose(p![lean: if odd { 13 } else { -13 }, headRot: if odd { -11 } else { 11 }, lid: 0.62, armL: if odd { 40 } else { 8 }, armR: if odd { 8 } else { 40 }, eyeX: if odd { 2.5 } else { -2.5 }]);
            if i % 2 == 0 { s.fx(F::Spark, if i % 4 != 0 { 70. } else { 26. }, -8.); }
            if i % 5 == 4 && s.walking() {
                let keep = s.with(|a| a.tx).unwrap_or(stop_at);
                s.stop(); s.wait(rnd(350., 650.)).await;                        // which way was it
                if !s.walking() { g = s.walk_to(keep, 24.); }
            }
            s.wait(260.).await; i += 1;
        }
        g.await;
        s.pose(p![lean: 0, headRot: 0, armL: 0, armR: 0, eyeX: 0]);
    }
    if faint {
        s.pose(p![turn: 0, lid: 0.3, headRot: 9]); s.wait(500.).await;         // oh
        s.pose(p![lid: 0.94, lean: 22, armL: 30, armR: 30]); s.wait(260.).await;
        s.pose(p![lean: 88, squash: 0.96]);                                    // and over it goes
        s.with(|a| a.vel[K::squash as usize] -= 1.5); s.fxo(F::Dust, 78., GROUND - 3., dx(9.));
        s.wait(700.).await;
        let (out, t0) = (rnd(3200., 5600.), s.time());
        while (s.time() - t0) * 1000. < out { s.fx(F::Spark, rnd(60., 86.), rnd(50., 70.)); s.wait(700.).await; }
        s.pose(p![lid: 0.6]); s.wait(350.).await; s.pose(p![lid: 0.94]); s.wait(300.).await;     // coming round
        s.pose(p![lid: 0.4, lean: 30]); s.wait(500.).await;
        s.pose(p![lean: 0, squash: 1, sit: 1, armL: 20, armR: 20]); s.wait(700.).await;        // sitting up
        shake_head(s).await;
        s.pose(p![sit: 0, lid: 0.2]); s.wait(600.).await;
        s.pose(p![armR: -118]); s.wait(700.).await; s.pose(p![armR: 0]); s.wait(250.).await;    // a hand to its head
        s.walk_to(st.home.0, 34.).await;
    } else {
        shake_head(s).await;
    }
    s.pose(p![turn: 0, lid: 0, armL: 0, armR: 0]); s.wait(300.).await;
    Mood::Tired
}
/// The bell is lit: some of the time, walk over and point at it.
pub async fn scene_notify(s: &S, st: &Top) -> Mood {
    let (k, w) = (s.k(), st.w);
    let at = || st.spot("approvals").map(|b| (f(b.left()), f(b.right()), (f(b.left()) + f(b.right())) / 2., (f(b.top()) + f(b.bottom())) / 2.));
    let Some(t) = at() else { return Mood::Calm };
    s.pose(p![turn: if t.2 > s.x() { 0.8 } else { -0.8 }, lid: -0.2]); s.wait(650.).await;   // oh, that is new
    let side: f32 = if t.0 - 24. * k > 12. { -1. } else { 1. };
    s.walk_to(clamp((if side < 0. { t.0 } else { t.1 }) + side * 20. * k, 10., w - 10.), 96.).await;
    let arm = if side < 0. { K::armR } else { K::armL };
    let mut o = P::new().with(K::turn, if side < 0. { 0.6 } else { -0.6 });
    let how = random();
    for n in 0..2 {
        o.set(arm, 100.); s.pose_ref(&o); s.wait(300.).await;
        for _ in 0..3 { o.set(arm, 88.); s.pose_ref(&o); s.wait(130.).await; if let Some(u) = at() { s.fx_at(F::Ring, u.2, u.3); } o.set(arm, 106.); s.pose_ref(&o); s.wait(170.).await; }
        s.pose(p![turn: 0]); s.wait(650.).await;                               // you. look.
        if n == 0 {
            if how < 0.35 { s.hop(6.).await; s.hop(6.).await; }
            else if how < 0.7 { s.pose(p![armL: -118, armR: -118, squash: 1.06, headY: -2]); s.wait(700.).await; s.pose(p![armL: 0, armR: 0, squash: 1, headY: 0]); }   // oi
            else { wave(s, 3).await; }
            o.set(K::turn, if side < 0. { 0.6 } else { -0.6 });
        }
    }
    s.pose(P::new().with(arm, 0.)); s.wait(300.).await;
    Mood::Calm
}
/// Hand over hand to a ledge lower or higher, its back to you, not enjoying it.
async fn climb_to(s: &S, surf: SurfFn, speed: f32) {
    let Some(sf) = surf() else { return };
    s.with(|a| { let was = a.base_y + a.y_off; a.air = false; a.vy = 0.; a.surf = Some(surf.clone()); a.base_y = sf.y; a.y_off = was - sf.y; });
    let yo = || s.with(|a| a.y_off);
    let down = yo() < 0.;
    let pause_at = yo().abs() * rnd(0.35, 0.6);
    let mut looked = yo().abs() < 60.;
    s.pose(p![turn: 2.1, shake: 0.22, lid: 0.3]);
    s.wait(500.).await;
    let mut i = 0;
    while yo().abs() > 1.5 {
        let step = yo().abs().min(speed * 0.06);
        s.with(|a| a.y_off += if down { step } else { -step });
        s.pose(if i % 2 == 1 { p![armL: 158, armR: 112, legL: -14, legR: 12] } else { p![armL: 112, armR: 158, legL: 12, legR: -14] });
        if !looked && yo().abs() < pause_at {                                 // it looked down
            looked = true;
            s.pose(p![turn: 1.15, headY: 3, eyeY: 4, shake: 0.5, lid: -0.3]); s.fx(F::Sweat, 72., 14.);
            s.wait(rnd(800., 1300.)).await;
            s.pose(p![turn: 2.1, headY: 0, eyeY: 0, shake: 0.22, lid: 0.3]);
            s.wait(300.).await;
        }
        s.wait(if i % 2 == 1 { 130. } else { 60. }).await; i += 1;
    }
    s.with(|a| a.y_off = 0.);
    s.pose(p![turn: 0, shake: 0, lid: 0, armL: 0, armR: 0, legL: 0, legR: 0]);
    s.wait(300.).await;
    s.fx(F::Puff, 64., 34.); s.pose(p![squash: 0.93, headY: 3]); s.wait(500.).await; s.pose(p![squash: 1, headY: 0]);   // made it
}
/// A project in the sidebar needs an answer: off the screen, in along the
/// first project, down the list a project at a time, point at it, and off.
pub async fn scene_project(s: &S, st: &Top, r: &TopRead) -> Mood {
    let (k, w) = (s.k(), st.w);
    let Some(target) = r.project.clone() else { return Mood::Calm };
    let rows: Vec<(String, Bounds<Pixels>)> = st.rows().into_iter().filter(|(_, b)| f(b.top()) > 40. && f(b.bottom()) < st.h - 8.).collect();
    let Some(ti) = rows.iter().position(|(n, _)| *n == target) else { return Mood::Calm };
    let names: Vec<String> = rows.iter().map(|(n, _)| n.clone()).collect();
    let b0 = rows[ti].1;
    st.clip_h.set(st.h);
    let narrow = f(b0.size.width) < 90.;
    let sp = st.spots.clone();
    let box_of = move |n: &str| sp.borrow().get(n).filter(|b| f(b.size.width) > 0.);
    let bo = box_of.clone();
    let spot_x = move |n: &str| { let b = bo(n).unwrap_or(b0); if narrow { f(b.right()) + 12. * k } else { clamp(f(b.right()) - 20. * k, f(b.left()) + 40., w - 12.) } };
    let ledge = |n: &str| -> SurfFn { let bo = box_of.clone(); let n = n.to_string(); Rc::new(move || { let b = bo(&n).unwrap_or(b0); Some(Surf { y: f(b.bottom()) - 1., l: -300., r: w + 300. }) }) };
    let side_x = f(b0.left()).min(0.) - 46.;
    s.pose(p![eyeY: 4, headY: 3, turn: -0.4]); s.wait(700.).await;            // someone down there
    s.pose(p![eyeY: 0, headY: 0, turn: 0]);
    s.walk_to(st.edge, 70.).await;                                             // off the top bar
    s.wait(rnd(500., 1000.)).await;
    let l0 = ledge(&names[0]);
    let f0 = l0();
    s.with(|a| { a.surf = Some(l0.clone()); a.base_y = f0.map_or(a.base_y, |x| x.y); a.y_off = 0.; a.air = false; a.x = side_x; a.vx = 0.; });
    s.walk_to(spot_x(&names[0]), 62.).await;
    s.wait(300.).await;
    let mut careful = 0;
    let mut n = 1;
    while n <= ti {
        if ti - n >= 2 && random() < 0.25 { n += 1; }                           // two at a time
        s.pose(p![headY: 4, eyeY: 4.5]); s.wait(rnd(260., 520.)).await;        // the next one down
        s.pose(p![headY: 0, eyeY: 0]);
        if random() < 0.5 { careful += 1; climb_to(s, ledge(&names[n]), 38.).await; }
        else {
            s.pose(p![squash: 0.9, armL: 30, armR: 30]); s.wait(200.).await;
            s.pose(p![squash: 1, armL: 140, armR: 140]);
            s.fall_to(ledge(&names[n])).await;
            s.pose(p![armL: 0, armR: 0]); s.wait(rnd(200., 380.)).await;
        }
        let want = spot_x(&names[n]);
        if (want - s.x()).abs() > 4. { s.walk_to(want, 50.).await; }
        n += 1;
    }
    // The row's dot (at its left, where the web's .dot is).
    let t = target.clone();
    let tgt = || box_of(&t).map(|b| (f(b.left()) + (20f32).min(f(b.size.width) / 2.), (f(b.top()) + f(b.bottom())) / 2.));
    for m in 0..2 {
        s.pose(p![turn: -0.6, armL: 98]); s.wait(320.).await;
        for _ in 0..3 { s.pose(p![armL: 86]); s.wait(130.).await; if let Some(t) = tgt() { s.fx_at(F::Ring, t.0, t.1); } s.pose(p![armL: 104]); s.wait(170.).await; }
        s.pose(p![turn: 0]); s.wait(700.).await;                               // this one. it is waiting for you.
        if m == 0 { if random() < 0.5 { s.hop(6.).await; s.hop(6.).await; } else { s.pose(p![armL: 0]); wave(s, 3).await; } }
    }
    s.pose(p![armL: 0]); s.wait(300.).await;
    s.walk_to(side_x, 70.).await;                                              // off the side
    s.wait(rnd(400., 800.)).await;
    s.with(|a| { a.surf = a.home_surf.clone(); a.base_y = st.home.1; a.y_off = 0.; a.air = false; a.x = st.edge; a.vx = 0.; });
    if careful > 1 { Mood::Tired } else { Mood::Calm }
}
/// A limit has reset: jumps, a lap of honour, a dance, a spin.
pub async fn scene_party(s: &S, st: &Top) -> Mood {
    let w = st.w;
    let big = st.party_for == "week";
    const COL: [u32; 5] = [0x818cf8, 0xfbbf24, 0x34d399, 0xfb7185, 0x38bdf8];
    let throw_up = || for _ in 0..4 { s.fxo(F::Confetti, rnd(10., 86.), rnd(-26., -4.), FxOpt { dx: Some(rnd(-12., 12.).round()), dy: None, cf: Some(pick(&COL)) }); };
    s.pose(p![eyeY: -3, lid: -0.3]); s.wait(600.).await;                      // it looks. it looks again.
    s.pose(p![eyeY: 0, lid: 0]); s.wait(200.).await;
    s.pose(p![eyeY: -3, lid: -0.35]); s.fx(F::Bang, 48., -14.); s.wait(500.).await;
    s.pose(p![eyeY: 0]);
    let count = if big { 3 } else { 1 + (random() * 2.) as usize };
    for _ in 0..count {
        match deal("party", 4) {
            0 => {   // up and down on the spot
                let n = 4 + (random() * 3.) as usize;
                for i in 0..n { s.pose(p![armL: 162, armR: 162, lid: 0.5]); s.fx(F::Spark, if i % 2 == 1 { 78. } else { 18. }, 4.); if i % 2 == 0 { throw_up(); } s.hop(rnd(8., 13.)).await; s.pose(p![armL: 120, armR: 120]); s.wait(90.).await; }
                s.pose(p![armL: 0, armR: 0, lid: 0]);
            }
            1 => {   // a lap of honour
                let far = clamp(s.x() + rnd(0.3, 0.55) * w, 20., w - 20.);
                s.pose(p![armL: 162, armR: 162, shake: 0.9, lid: 0.5]);
                for i in 0..2 {
                    let g = s.walk_to(if i % 2 == 1 { clamp(st.home.0 + 30., 20., w - 20.) } else { far }, 185.);
                    while s.walking() { s.fx(F::Spark, 48., -6.); s.wait(170.).await; }
                    g.await; throw_up();
                }
                s.pose(p![shake: 0, armL: 0, armR: 0, lid: 0, turn: 0]);
            }
            2 => {   // a little dance
                for i in 0..8 {
                    s.pose(if i % 2 == 1 { p![armL: 150, armR: 24, lean: 7, headRot: 8, legL: 0, legR: -18, lid: 0.5] } else { p![armL: 24, armR: 150, lean: -7, headRot: -8, legL: 18, legR: 0, lid: 0.5] });
                    if i % 2 == 1 { s.fxo(F::Note, 74., 2., dx(10.)); }
                    s.wait(230.).await;
                }
                s.pose(p![armL: 0, armR: 0, lean: 0, headRot: 0, legL: 0, legR: 0, lid: 0]);
            }
            _ => {   // round and round, then a pump of the fist
                for _ in 0..2 { s.pose(p![turn: 2.1, armL: 60, armR: 60]); s.wait(260.).await; s.pose(p![turn: 0]); s.wait(260.).await; }
                s.pose(p![squash: 0.85, armR: 30]); s.wait(200.).await;
                s.pose(p![squash: 1, armR: 168, armL: 0]); throw_up(); s.hop(12.).await;
                s.pose(p![armR: 0]);
            }
        }
        s.wait(rnd(250., 500.)).await;
    }
    s.pose(p![turn: 0]); wave(s, 3).await;                                    // did you see that
    Mood::Calm
}
/// An outing: grow, the scene, home (slowly when worn out), fold away.
pub async fn top_outing(s: S, kind: String, st: Top, r: TopRead) {
    s.wait(260.).await;
    s.pose(p![grow: 1]); s.wait(950.).await;
    let mood = match kind.as_str() {
        "five" => scene_five(&s, &st, &r).await,
        "week" => scene_week(&s, &st, &r).await,
        "update" => scene_update(&s, &st).await,
        "notify" => scene_notify(&s, &st).await,
        "project" => scene_project(&s, &st, &r).await,
        _ => scene_party(&s, &st).await,
    };
    s.rest(&[]);
    if mood == Mood::Tired { s.pose(p![headY: 2.6, eyeY: 3.4, lid: 0.3, armSwing: 0.3]); }
    s.walk_to(st.home.0, if mood == Mood::Tired { 30. } else { 62. }).await;
    s.pose(p![headY: 0, eyeY: 0, lid: 0, armSwing: 1, turn: 0]);
    s.wait(300.).await;
    if mood == Mood::Tired { sigh(&s).await; }
    s.pose(p![grow: 0, squash: 1, headRot: 0, lean: 0]);
    s.wait(820.).await;
}
/// A chat wants it: leave from wherever it is, by the nearest edge.
pub async fn top_cut(s: S, w: f32) {
    s.rest(&[]); s.pose(p![sit: 0, swing: 0, grow: 1]);
    if s.with(|a| a.air) { s.landed().await; }
    s.pose(p![lid: -0.3]); s.fx(F::Bang, 48., -14.); s.wait(300.).await;        // needed elsewhere
    s.pose(p![lid: 0]);
    s.walk_to(if s.x() < w / 2. { -46. } else { w + 46. }, 120.).await;
}
