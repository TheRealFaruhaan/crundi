//! Mini drawn in code (crundi-mini.js's markup() and Actor.draw(), src/
//! brand.js's colours): each part of the web's SVG as a GPUI path, placed by
//! the same transforms, recomputed from the springs each frame; and the
//! effects (the web's FX and their CSS keyframes). Shapes are worked out
//! when the view renders (`figure`, `effects`) and painted as paths.

use super::actor::{clamp, Actor, Fx, FxKind, K, GROUND, SIT_DROP};
use gpui::*;

const AMBER: u32 = 0xfbbf24;
const DARK: u32 = 0x0d0d16;
const G0: u32 = 0x818cf8;
const G1: u32 = 0x4f46e5;

// ─── A 2D affine transform, as SVG's (x' = a x + c y + e, y' = b x + d y + f) ───

#[derive(Clone, Copy, Debug)]
pub struct M { a: f32, b: f32, c: f32, d: f32, e: f32, f: f32 }
impl M {
    pub const I: M = M { a: 1., b: 0., c: 0., d: 1., e: 0., f: 0. };
    pub fn tr(x: f32, y: f32) -> M { M { e: x, f: y, ..M::I } }
    pub fn sc(x: f32, y: f32) -> M { M { a: x, d: y, ..M::I } }
    pub fn rot(deg: f32) -> M { let r = deg.to_radians(); M { a: r.cos(), b: r.sin(), c: -r.sin(), d: r.cos(), e: 0., f: 0. } }
    pub fn rot_at(deg: f32, x: f32, y: f32) -> M { M::tr(x, y).then(M::rot(deg)).then(M::tr(-x, -y)) }
    pub fn sc_at(x: f32, y: f32, s: f32) -> M { if s == 1. { M::I } else { M::tr(x, y).then(M::sc(s.max(0.), s.max(0.))).then(M::tr(-x, -y)) } }
    /// self · n (n applied first, as SVG's "self n").
    pub fn then(self, n: M) -> M {
        M { a: self.a * n.a + self.c * n.b, b: self.b * n.a + self.d * n.b, c: self.a * n.c + self.c * n.d, d: self.b * n.c + self.d * n.d,
            e: self.a * n.e + self.c * n.f + self.e, f: self.b * n.e + self.d * n.f + self.f }
    }
    pub fn ap(&self, p: (f32, f32)) -> (f32, f32) { (self.a * p.0 + self.c * p.1 + self.e, self.b * p.0 + self.d * p.1 + self.f) }
    /// How much it scales (for stroke widths) and turns (for gradients).
    fn scale(&self) -> f32 { (self.a * self.d - self.b * self.c).abs().sqrt() }
    fn angle(&self) -> f32 { self.b.atan2(self.a).to_degrees() }
}

#[derive(Clone, Debug)]
pub enum Paint { Solid(Hsla), Grad { angle: f32, c0: Hsla, c1: Hsla } }

/// One shape, in the layer's pixels.
#[derive(Clone, Debug)]
pub struct Shape { pub pts: Vec<(f32, f32)>, pub paint: Paint, pub stroke: Option<f32>, pub closed: bool }

fn rgb(c: u32, a: f32) -> Hsla { let mut h: Hsla = gpui::rgb(c).into(); h.a = clamp(a, 0., 1.); h }

fn lerp(c0: u32, c1: u32, t: f32) -> u32 {
    let ch = |s: u32| { let (a, b) = (((c0 >> s) & 255) as f32, ((c1 >> s) & 255) as f32); ((a + (b - a) * t).round() as u32) << s };
    ch(16) | ch(8) | ch(0)
}

/// The brand gradient over a box whose span on it runs from t0 to t1 (the
/// web's userSpaceOnUse gradient from (14,10) to (82,84)), turned with it.
fn grad(m: &M, t0: f32, t1: f32, alpha: f32) -> Paint {
    Paint::Grad { angle: 135. + m.angle(), c0: rgb(lerp(G0, G1, t0), alpha), c1: rgb(lerp(G0, G1, t1), alpha) }
}

fn arc(out: &mut Vec<(f32, f32)>, cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32, n: usize) {
    for i in 0..=n { let t = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians(); out.push((cx + rx * t.cos(), cy + ry * t.sin())); }
}

/// A rounded rectangle's outline (SVG rect with rx).
pub fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<(f32, f32)> {
    let r = r.min(w / 2.).min(h / 2.);
    let mut v = Vec::with_capacity(32);
    arc(&mut v, x + w - r, y + r, r, r, -90., 0., 6);
    arc(&mut v, x + w - r, y + h - r, r, r, 0., 90., 6);
    arc(&mut v, x + r, y + h - r, r, r, 90., 180., 6);
    arc(&mut v, x + r, y + r, r, r, 180., 270., 6);
    v
}
pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<(f32, f32)> { let mut v = vec![]; arc(&mut v, cx, cy, rx, ry, 0., 360., 24); v.pop(); v }

/// A quadratic curve's points (from, control, to).
fn quad(out: &mut Vec<(f32, f32)>, p0: (f32, f32), c: (f32, f32), p1: (f32, f32), n: usize) {
    for i in 1..=n { let t = i as f32 / n as f32; let u = 1. - t; out.push((u * u * p0.0 + 2. * u * t * c.0 + t * t * p1.0, u * u * p0.1 + 2. * u * t * c.1 + t * t * p1.1)); }
}
fn cubic(out: &mut Vec<(f32, f32)>, p0: (f32, f32), c1: (f32, f32), c2: (f32, f32), p1: (f32, f32), n: usize) {
    for i in 1..=n {
        let t = i as f32 / n as f32; let u = 1. - t;
        out.push((u * u * u * p0.0 + 3. * u * u * t * c1.0 + 3. * u * t * t * c2.0 + t * t * t * p1.0, u * u * u * p0.1 + 3. * u * u * t * c1.1 + 3. * u * t * t * c2.1 + t * t * t * p1.1));
    }
}

/// Clip a convex polygon by another convex one (Sutherland-Hodgman): the
/// eyes inside the head (the web's clipPath).
fn clip(subject: &[(f32, f32)], by: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let n = by.len();
    if n < 3 { return subject.to_vec(); }
    // Orientation of the clip polygon.
    let area: f32 = (0..n).map(|i| { let (a, b) = (by[i], by[(i + 1) % n]); a.0 * b.1 - b.0 * a.1 }).sum();
    let sgn = if area >= 0. { 1. } else { -1. };
    let inside = |p: (f32, f32), a: (f32, f32), b: (f32, f32)| sgn * ((b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)) >= 0.;
    let mut out = subject.to_vec();
    for i in 0..n {
        let (a, b) = (by[i], by[(i + 1) % n]);
        let inp = std::mem::take(&mut out);
        if inp.is_empty() { break; }
        for j in 0..inp.len() {
            let (p, q) = (inp[j], inp[(j + 1) % inp.len()]);
            let (pi, qi) = (inside(p, a, b), inside(q, a, b));
            let cut = || {
                let (d1, d2) = ((q.0 - p.0, q.1 - p.1), (b.0 - a.0, b.1 - a.1));
                let den = d1.0 * d2.1 - d1.1 * d2.0;
                if den.abs() < 1e-9 { return p; }
                let t = ((a.0 - p.0) * d2.1 - (a.1 - p.1) * d2.0) / den;
                (p.0 + d1.0 * t, p.1 + d1.1 * t)
            };
            if pi { out.push(p); if !qi { out.push(cut()); } } else if qi { out.push(cut()); }
        }
    }
    out
}

struct Out<'a> { v: &'a mut Vec<Shape>, alpha: f32 }
impl Out<'_> {
    fn fill(&mut self, m: &M, pts: Vec<(f32, f32)>, p: Paint) { if pts.len() >= 3 { self.v.push(Shape { pts: pts.iter().map(|q| m.ap(*q)).collect(), paint: p, stroke: None, closed: true }); } }
    fn solid(&mut self, m: &M, pts: Vec<(f32, f32)>, c: u32, a: f32) { let al = self.alpha * a; if al > 0.003 { self.fill(m, pts, Paint::Solid(rgb(c, al))); } }
    fn line(&mut self, m: &M, pts: Vec<(f32, f32)>, c: u32, a: f32, w: f32) {
        let al = self.alpha * a;
        if al > 0.003 && pts.len() >= 2 { self.v.push(Shape { pts: pts.iter().map(|q| m.ap(*q)).collect(), paint: Paint::Solid(rgb(c, al)), stroke: Some(w * m.scale()), closed: false }); }
    }
}

/// Mini as drawn this frame (the web's Actor.draw), in the layer's pixels.
pub fn figure(a: &Actor) -> Vec<Shape> {
    let v = |k: K| a.v[k as usize];
    let k = a.k;
    let gait = a.phase.sin();
    let w = a.walk;
    let run = clamp((a.vx.abs() - 90.) / 80., 0., 1.);
    let sit = clamp(v(K::sit), 0., 1.);
    let (sw, tm) = (v(K::swing), a.time);
    let shake = v(K::shake) * (tm * 34.).sin() * 15.;
    let amp = (30. + run * 14.) * w;
    let cosg = a.phase.cos();
    let leg_l = v(K::legL) + gait * amp + (tm * 5.2).sin() * 24. * sw;
    let leg_r = v(K::legR) - gait * amp + (tm * 5.2 + 2.5).sin() * 24. * sw;
    let way = clamp(a.vx / 18., -1., 1.);
    let (lift_l, lift_r) = ((-way * cosg).max(0.) * 3.4 * w, (way * cosg).max(0.) * 3.4 * w);
    let swing_a = 24. * w * clamp(v(K::armSwing), 0., 1.4) * (1. - clamp(v(K::shake), 0., 1.));
    let mut arm_l = v(K::armL) - gait * swing_a + shake;
    let mut arm_r = v(K::armR) + gait * swing_a - shake;
    let ch = clamp(v(K::chute), 0., 1.15);
    let c1 = ch.min(1.);
    arm_l = arm_l * (1. - c1) + 152. * c1; arm_r = arm_r * (1. - c1) + 152. * c1;
    let rise = 0.5 - 0.5 * (a.phase * 2.).cos();
    let bob = -rise * (2.3 + run * 2.) * w + if a.air { 0. } else { (tm * 2.1).sin() * 0.5 };
    let roll = gait * 1.7 * w;
    let head_lag = (a.phase * 2. - 0.9).sin() * 0.9 * w;
    let lean = v(K::lean) + clamp(a.vx / 22., -11., 11.) + roll;
    let sq = clamp(v(K::squash), 0.55, 1.4);
    let sx = 1. / sq.sqrt();
    let drop = sit * SIT_DROP;
    let alpha = clamp(v(K::alpha), 0., 1.);
    let mut shapes = vec![];
    if alpha <= 0.003 { return shapes; }
    let mut o = Out { v: &mut shapes, alpha };

    // The element: at (x - 48k, baseY + yOff - 90k), the drawing scaled by k.
    let m0 = M::tr(a.x - 48. * k, a.base_y + a.y_off - GROUND * k).then(M::sc(k, k));
    let gr = v(K::grow);
    let (mut gb, mut ga, mut gl) = (clamp(gr * 2.2, 0., 1.12), clamp(gr * 2.2 - 0.6, 0., 1.12), clamp(gr * 2.2 - 1.2, 0., 1.12));
    if gr > 0.985 && gr < 1.015 { gb = 1.; ga = 1.; gl = 1.; }

    // The shadow.
    let lift = clamp(-a.y_off / 40., 0., 1.);
    let sh_a = 0.32 * (1. - sit) * (1. - lift * 0.7) * clamp(gl, 0., 1.);
    let msh = m0.then(M::tr(0., -a.y_off / k)).then(M::sc_at(48., 90., 1. - lift * 0.4));
    if sh_a > 0.003 { let al = o.alpha; o.alpha = 1.; o.solid(&msh, ellipse(48., 90., 21., 3.2), 0x000000, sh_a * al); o.alpha = al; }

    let root = m0.then(M::tr(0., drop)).then(M::rot_at(lean, 48., GROUND)).then(M::tr(48., GROUND)).then(M::sc(sx, sq)).then(M::tr(-48., -GROUND));

    // The parachute.
    let chute_a = clamp(ch * 3., 0., 1.);
    if chute_a > 0.003 && ch > 0.01 {
        let mc = root.then(M::tr(48., 54.)).then(M::sc(ch, ch)).then(M::tr(-48., -54.));
        for (p, q) in [((12., -8.), (33., 56.)), ((36., -6.), (40., 54.)), ((60., -6.), (56., 54.)), ((84., -8.), (63., 56.))] { o.line(&mc, vec![p, q], 0xc7c9e8, chute_a, 1.4); }
        let mut canopy = vec![(6., -8.)];
        quad(&mut canopy, (6., -8.), (48., -66.), (90., -8.), 16);
        quad(&mut canopy, (90., -8.), (78., -17.), (69., -8.), 4);
        quad(&mut canopy, (69., -8.), (58., -17.), (48., -8.), 4);
        quad(&mut canopy, (48., -8.), (38., -17.), (27., -8.), 4);
        quad(&mut canopy, (27., -8.), (18., -17.), (6., -8.), 4);
        o.solid(&mc, canopy, AMBER, chute_a);
        for side in [-1f32, 1.] {
            let xo = |x: f32| 48. + side * (x - 48.);
            let mut g = vec![(xo(27.), -8.)];
            quad(&mut g, (xo(27.), -8.), (xo(30.), -44.), (48., -51.), 8);
            quad(&mut g, (48., -51.), (xo(38.), -30.), (48., -8.), 8);
            quad(&mut g, (48., -8.), (xo(38.), -17.), (xo(27.), -8.), 4);
            o.solid(&mc, g, G0, chute_a);
        }
    }

    // The legs.
    if gl > 0.01 {
        let ml = root.then(M::tr(0., -lift_l)).then(M::rot_at(leg_l, 40.5, 78.)).then(M::sc_at(40.5, 78., gl));
        o.solid(&ml, rrect(36., 77., 9., 13., 4.2), AMBER, 1.);
        let mr = root.then(M::tr(0., -lift_r)).then(M::rot_at(leg_r, 55.5, 78.)).then(M::sc_at(55.5, 78., gl));
        o.solid(&mr, rrect(51., 77., 9., 13., 4.2), AMBER, 1.);
    }

    let up = root.then(M::tr(0., bob));
    // Hands behind the back: the arms go behind the body.
    let ab = clamp(v(K::armsBack), 0., 1.);
    let behind = ab > 0.1;
    let reach = v(K::reach);
    let rc_a = clamp(reach.abs(), 0., 1.);
    let rc_x = 48. + if reach < 0. { -9. } else { 9. };
    let (rlx, rrx, rry) = (rc_a * (rc_x - 23.5), rc_a * (rc_x - 72.5), rc_a * 5.);
    let arms = |o: &mut Out| {
        if ga <= 0.01 { return; }
        let ml = up.then(M::tr(ab * 7.5 + rlx, ab * 2.5)).then(M::rot_at(arm_l - ab * 24., 23.5, 58.)).then(M::sc_at(23.5, 58., ga));
        o.solid(&ml, rrect(20., 56., 7., 16., 3.5), AMBER, 1.);
        let mr = up.then(M::tr(-ab * 7.5 + rrx, ab * 2.5 + rry)).then(M::rot_at(-(arm_r - ab * 24.), 72.5, 58.)).then(M::sc_at(72.5, 58., ga));
        o.solid(&mr, rrect(69., 56., 7., 16., 3.5), AMBER, 1.);
    };
    if behind { arms(&mut o); }

    // The body: its light, and (turned right round) its back.
    let turn = v(K::turn);
    let tc = clamp(turn, -1., 1.);
    if gb > 0.01 {
        let mt = up.then(M::sc_at(48., 51., gb));
        o.fill(&mt, rrect(32., 54., 32., 24., 9.), grad(&mt, 0.4436, 0.835, alpha));
        let light = 0.6 * (1. - clamp((turn.abs() - 1.1) / 0.4, 0., 1.));
        o.solid(&mt, ellipse(48., 66., 4.8, 4.8), DARK, light);
        let back = clamp((turn.abs() - 1.25) / 0.5, 0., 1.);
        for y in [62., 67., 72.] { o.line(&mt, vec![(41., y), (55., y)], DARK, back * 0.4, 2.2); }
    }

    // The head: antenna, tip, the head, the eyes inside it.
    let mh = up.then(M::tr(tc * 2.6, v(K::headY) + head_lag)).then(M::rot_at(v(K::headRot), 48., 50.));
    let ant = clamp(a.ant, -1.3, 1.3);
    let (ax, ay) = (48. + ant * 7., 5. + ant.abs() * 1.6);
    let mut stalk = vec![(48., 13.)];
    quad(&mut stalk, (48., 13.), (48., 9.), (ax, ay + 1.), 8);
    o.line(&mh, stalk, AMBER, 1., 4.);
    o.solid(&mh, ellipse(ax, ay, 4.5, 4.5), AMBER, 1.);
    let head = rrect(23., 12., 50., 37., 15.);
    o.fill(&mh, head.clone(), grad(&mh, 0.075, 0.683, alpha));
    let me = mh.then(M::tr(turn * 15. + v(K::eyeX) + a.look_x, v(K::eyeY) + a.look_y));
    let lid = clamp(v(K::lid) + if a.blink > 0. { 1. } else { 0. }, -0.35, 0.94);
    let open = 1. - lid;
    let near_l = 1. - clamp(-tc, 0., 1.) * 0.4;
    let near_r = 1. - clamp(tc, 0., 1.) * 0.4;
    let head_px: Vec<(f32, f32)> = head.iter().map(|q| mh.ap(*q)).collect();
    for (cx_, x, near) in [(40.25, 36.5, near_l), (55.75, 52., near_r)] {
        let m = me.then(M::tr(cx_, 30.5)).then(M::sc(near, open)).then(M::tr(-cx_, -30.5));
        let eye: Vec<(f32, f32)> = rrect(x, 24., 7.5, 13., 3.7).iter().map(|q| m.ap(*q)).collect();
        let cut = clip(&eye, &head_px);
        if cut.len() >= 3 && alpha > 0.003 { o.v.push(Shape { pts: cut, paint: Paint::Solid(rgb(DARK, alpha)), stroke: None, closed: true }); }
    }
    if !behind { arms(&mut o); }
    shapes
}

/// Where Mini takes a tap (the web's .cm box: 96 × 92 at its place), in the layer.
pub fn tap_box(a: &Actor) -> Bounds<Pixels> {
    let k = a.k;
    Bounds::new(point(px(a.x - 48. * k), px(a.base_y + a.y_off - GROUND * k)), size(px(96. * k), px(92. * k)))
}

// ─── Effects ───

/// CSS ease-out (cubic-bezier(0, 0, .58, 1)).
fn ease_out(t: f32) -> f32 {
    let t = clamp(t, 0., 1.);
    // Solve x(u) = t for the curve's parameter, then y(u).
    let (x1, x2) = (0., 0.58);
    let bx = |u: f32| 3. * (1. - u) * (1. - u) * u * x1 + 3. * (1. - u) * u * u * x2 + u * u * u;
    let mut u = t;
    for _ in 0..8 {
        let x = bx(u) - t;
        let d = 3. * (1. - u) * (1. - u) * x1 + 6. * (1. - u) * u * (x2 - x1) + 3. * u * u * (1. - x2);
        if d.abs() < 1e-5 { break; }
        u = clamp(u - x / d, 0., 1.);
    }
    let (y1, y2) = (0., 1.);
    3. * (1. - u) * (1. - u) * u * y1 + 3. * (1. - u) * u * u * y2 + u * u * u
}

/// A property's value at progress p over keyframes (offset, value), each
/// span eased out (CSS applies the timing function per keyframe span).
fn kf(p: f32, keys: &[(f32, f32)]) -> f32 {
    if p <= keys[0].0 { return keys[0].1; }
    for w in keys.windows(2) {
        let ((a, va), (b, vb)) = (w[0], w[1]);
        if p <= b { let t = if b > a { (p - a) / (b - a) } else { 1. }; return va + (vb - va) * ease_out(t); }
    }
    keys[keys.len() - 1].1
}

/// An effect at `now` (the layer's clock): None once it is over.
pub fn effect(f: &Fx, now: f32) -> Option<Vec<Shape>> {
    let (w, h, d) = f.kind.size();
    let p = (now - f.born) * 1000. / d;
    if !(0. ..=1.).contains(&p) { return if p < 0. { Some(vec![]) } else { None }; }
    // (opacity, translate x, y, scale, rotate)
    let (op, tx, ty, s, r) = match f.kind {
        FxKind::Knock => (kf(p, &[(0., 0.), (0.25, 1.), (1., 0.)]), 0., 0., kf(p, &[(0., 0.3), (1., 1.5)]), 0.),
        FxKind::Dust => (kf(p, &[(0., 0.55), (1., 0.)]), kf(p, &[(0., 0.), (1., f.dx.unwrap_or(0.))]), kf(p, &[(0., 0.), (1., -9.)]), kf(p, &[(0., 0.4), (1., 1.5)]), 0.),
        FxKind::Puff | FxKind::Z | FxKind::Note => (kf(p, &[(0., 0.), (0.2, 1.), (1., 0.)]), kf(p, &[(0., 0.), (1., f.dx.unwrap_or(6.))]), kf(p, &[(0., 4.), (1., -26.)]), kf(p, &[(0., 0.6), (1., 1.1)]), 0.),
        FxKind::Bang | FxKind::Ask | FxKind::Dots | FxKind::Anger | FxKind::Spark => (kf(p, &[(0., 0.), (0.18, 1.), (0.3, 1.), (0.78, 1.), (1., 0.)]), 0., 0., kf(p, &[(0., 0.2), (0.18, 1.15), (0.3, 1.), (0.78, 1.), (1., 0.8)]), 0.),
        FxKind::Ring => (kf(p, &[(0., 0.9), (1., 0.)]), 0., 0., kf(p, &[(0., 0.3), (1., 1.6)]), 0.),
        FxKind::Sweat => (kf(p, &[(0., 0.), (0.2, 1.), (1., 0.)]), 0., kf(p, &[(0., -3.), (1., 14.)]), 1., 0.),
        FxKind::Confetti => (kf(p, &[(0., 0.), (0.15, 1.), (1., 0.)]), kf(p, &[(0., 0.), (1., f.dx.unwrap_or(6.))]), kf(p, &[(0., -6.), (1., 26.)]), 1., kf(p, &[(0., 0.), (1., 260.)])),
        FxKind::Plane => (kf(p, &[(0., 0.), (0.12, 1.), (1., 0.)]), kf(p, &[(0., 0.), (1., f.dx.unwrap_or(-60.))]), kf(p, &[(0., 0.), (1., f.dy.unwrap_or(-110.))]), kf(p, &[(0., 0.6), (1., 1.)]), kf(p, &[(0., -20.), (1., -34.)])),
    };
    let mut v = vec![];
    if op <= 0.003 { return Some(v); }
    // transform-origin: the box's centre.
    let m = M::tr(f.x + tx + w / 2., f.y + ty + h / 2.).then(M::rot(r)).then(M::sc(s, s)).then(M::tr(-w / 2., -h / 2.));
    let mut o = Out { v: &mut v, alpha: op };
    match f.kind {
        FxKind::Knock => { for l in [vec![(11., 2.), (11., 7.)], vec![(3., 6.), (7., 9.)], vec![(19., 6.), (15., 9.)]] { o.line(&m, l, AMBER, 1., 2.4); } }
        FxKind::Dust => o.solid(&m, ellipse(6., 6., 5., 5.), 0x8888a8, 1.),
        FxKind::Puff => o.solid(&m, ellipse(7., 5., 6., 4.), 0x8888a8, 0.7),
        FxKind::Z => o.line(&m, vec![(2., 2.), (10., 2.), (2., 10.), (10., 10.)], 0xa5b4fc, 1., 2.2),
        FxKind::Note => {
            o.line(&m, vec![(4., 12.), (4., 3.), (10., 1.5), (10., 10.)], AMBER, 1., 2.);
            o.solid(&m, ellipse(2.6, 12., 2.4, 2.4), AMBER, 1.);
            o.solid(&m, ellipse(8.6, 10., 2.4, 2.4), AMBER, 1.);
        }
        FxKind::Bang => { o.line(&m, vec![(4., 2.), (4., 10.)], AMBER, 1., 3.4); o.solid(&m, ellipse(4., 15.5, 2., 2.), AMBER, 1.); }
        FxKind::Ask => {
            let mut q = vec![(3., 6.)];
            cubic(&mut q, (3., 6.), (3., 1.), (11., 1.), (11., 6.), 8);
            cubic(&mut q, (11., 6.), (11., 9.), (7., 9.), (7., 13.), 8);
            o.line(&m, q, AMBER, 1., 3.);
            o.solid(&m, ellipse(7., 17.6, 1.9, 1.9), AMBER, 1.);
        }
        FxKind::Dots => {
            let mut b = rrect(1., 1., 32., 15., 3.);
            // the tail under its left: (14,16) (9,21) (9,16), on the bottom edge
            let at = b.iter().position(|q| q.1 >= 15.99 && q.0 < 14.).unwrap_or(b.len());
            b.splice(at..at, [(14., 16.), (9., 21.), (9., 16.)]);
            o.solid(&m, b.clone(), 0x1a1a28, 1.);
            let mut edge = b; edge.push(edge[0]);
            o.line(&m, edge, 0x3a3a55, 1., 1.2);
            for x in [10., 17., 24.] { o.solid(&m, ellipse(x, 8.5, 2.1, 2.1), 0xa5b4fc, 1.); }
        }
        FxKind::Ring => { let mut c = ellipse(10., 10., 8., 8.); c.push(c[0]); o.line(&m, c, G0, 1., 2.); }
        FxKind::Sweat => {
            let mut q = vec![(4., 1.)];
            cubic(&mut q, (4., 1.), (6., 4.), (7., 5.5), (7., 7.5), 6);
            arc(&mut q, 4., 7.5, 3., 3., 0., 180., 10);
            cubic(&mut q, (1., 7.5), (1., 5.5), (2., 4.), (4., 1.), 6);
            o.solid(&m, q, 0x7dd3fc, 1.);
        }
        FxKind::Anger => { for l in [vec![(6., 2.), (6., 6.), (2., 6.)], vec![(10., 2.), (10., 6.), (14., 6.)], vec![(6., 14.), (6., 10.), (2., 10.)], vec![(10., 14.), (10., 10.), (14., 10.)]] { o.line(&m, l, 0xfb7185, 1., 2.4); } }
        FxKind::Spark => o.solid(&m, vec![(7., 0.), (8.6, 5.4), (14., 7.), (8.6, 8.6), (7., 14.), (5.4, 8.6), (0., 7.), (5.4, 5.4)], AMBER, 1.),
        FxKind::Confetti => o.solid(&m, rrect(0., 0., 6., 9., 1.5), f.cf.unwrap_or(G0), 1.),
        FxKind::Plane => { o.solid(&m, vec![(1., 6.), (17., 1.), (11., 13.), (8., 8.)], 0xe8e8f0, 1.); o.line(&m, vec![(8., 8.), (17., 1.)], 0x8888a8, 1., 1.); }
    }
    Some(v)
}

/// Paint shapes made in a layer whose top left is `origin`.
pub fn paint(shapes: &[Shape], origin: Point<Pixels>, window: &mut Window) {
    let at = |p: (f32, f32)| point(origin.x + px(p.0), origin.y + px(p.1));
    for s in shapes {
        if s.pts.len() < 2 { continue; }
        let mut b = match s.stroke { Some(w) => PathBuilder::stroke(px(w.max(0.5))), None => PathBuilder::fill() };
        b.move_to(at(s.pts[0]));
        for q in &s.pts[1..] { b.line_to(at(*q)); }
        if s.closed { b.close(); }
        let Ok(path) = b.build() else { continue };
        match &s.paint {
            Paint::Solid(c) => window.paint_path(path, *c),
            Paint::Grad { angle, c0, c1 } => window.paint_path(path, linear_gradient(*angle, linear_color_stop(*c0, 0.), linear_color_stop(*c1, 1.))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn transforms_compose_as_svg() {
        let m = M::tr(10., 0.).then(M::sc(2., 2.));
        assert_eq!(m.ap((1., 1.)), (12., 2.));
        let r = M::rot_at(90., 1., 0.).ap((2., 0.));
        assert!((r.0 - 1.).abs() < 1e-5 && (r.1 - 1.).abs() < 1e-5);
    }
    #[::core::prelude::v1::test]
    fn eyes_clip_inside_the_head() {
        let head = rrect(0., 0., 10., 10., 2.);
        let eye = rrect(8., 2., 6., 4., 1.);
        let c = clip(&eye, &head);
        assert!(!c.is_empty() && c.iter().all(|p| p.0 <= 10.01));
    }
    #[::core::prelude::v1::test]
    fn a_whole_mini_is_drawn() {
        let mut a = Actor::new(54., std::rc::Rc::new(std::cell::Cell::new(0.)));
        a.v[K::alpha as usize] = 1.;
        a.x = 100.; a.base_y = 100.;
        let f = figure(&a);
        // shadow, 2 legs, torso, light, antenna, tip, head, 2 eyes, 2 arms
        assert!(f.len() >= 12, "{}", f.len());
        assert!((ease_out(1.) - 1.).abs() < 1e-4 && ease_out(0.) == 0.);
    }
}
