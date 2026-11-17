//! Cru's liquid mic (crundi-voice.js drawMic, the phone's FluidMic): a
//! sphere of three coloured layers that spreads into water while the owner
//! talks, with ripples on its surface and drops thrown up; its colours say
//! the state (MIC_COLORS, chosen by the caller). The caller keeps a
//! `Liquid` and steps it once a frame (`step`), then draws it here; the
//! box is the click target, the canvas round it larger (it spills over).
//! Also `radial_glow`, the canvas' radial gradient as stacked discs (GPUI
//! paints no radial gradients). Added by Desktop (AREAS.md, "A new control").

use crundi_theme::T;
use gpui::*;

const HIST: usize = 64;

#[derive(Clone)]
pub struct Droplet { x: f32, vx: f32, vy: f32, born: f64 }
#[derive(Clone)]
pub struct Ripple { x: f32, born: f64 }

/// The water's state (the web's S.liq).
#[derive(Clone)]
pub struct Liquid {
    pub m: f32,
    mv: f32,
    pub lv: f32,
    peak: f32,
    hist: [f32; HIST],
    drops: Vec<Droplet>,
    ripples: Vec<Ripple>,
    until: f64,
    last_drop: f64,
    last_t: f64,
    seed: u32,
}

impl Default for Liquid {
    fn default() -> Self { Liquid { m: 0., mv: 0., lv: 0., peak: 0.1, hist: [0.; HIST], drops: vec![], ripples: vec![], until: 0., last_drop: 0., last_t: 0., seed: 0x9e37_79b9 } }
}

impl Liquid {
    fn rnd(&mut self) -> f32 { self.seed ^= self.seed << 13; self.seed ^= self.seed >> 17; self.seed ^= self.seed << 5; (self.seed % 10_000) as f32 / 10_000.0 }

    /// One frame at `ms` (milliseconds): `raw` the microphone's shown level,
    /// `listening` while Cru listens (the water moves only then).
    pub fn step(&mut self, ms: f64, raw: f32, listening: bool) {
        let dt = if self.last_t > 0. { ((ms - self.last_t) / 1000.).min(0.05) as f32 } else { 0.016 };
        self.last_t = ms;
        let raw = if listening { raw } else { 0. };
        self.peak = (self.peak * 0.995).max(raw.max(0.1));
        let level = (raw / self.peak).min(1.);
        self.lv += (level - self.lv) * if level > self.lv { 0.45 } else { 0.12 };
        self.hist.copy_within(0..HIST - 1, 1);
        self.hist[0] = self.lv;
        if level > 0.22 { self.until = ms + 450.; }
        let talking = listening && ms < self.until;
        let target = if talking { 1. } else { 0. };
        self.mv += ((target - self.m) * if talking { 28. } else { 42. } - self.mv * if talking { 9. } else { 5.5 }) * dt;
        self.m += self.mv * dt;
        if talking && self.lv > 0.55 && ms - self.last_drop > 140. {
            let (a, b) = (self.rnd(), self.rnd());
            self.drops.push(Droplet { x: (a - 0.5) * 3., vx: (b - 0.5) * 60., vy: -(90. + self.lv * 120.), born: ms });
            self.last_drop = ms;
        }
        if talking && self.lv > 0.3 && self.rnd() < 0.08 { let x = (self.rnd() - 0.5) * 2.6; self.ripples.push(Ripple { x, born: ms }); }
        self.drops.retain(|d| ms - d.born < 900.);
        self.ripples.retain(|r| ms - r.born < 900.);
    }
}

/// How the mic looks now: its three layers (a colour and its dark end),
/// the glow's colour, and `dim` (waking and offline breathe, asleep is dim).
#[derive(Clone)]
pub struct MicLook { pub layers: [(Hsla, Hsla); 3], pub glow: Hsla, pub dim: f32 }

fn circle(window: &mut Window, c: Point<Pixels>, r: f32, color: Hsla) {
    if r <= 0. { return; }
    let b = Bounds { origin: point(c.x - px(r), c.y - px(r)), size: size(px(2. * r), px(2. * r)) };
    window.paint_quad(fill(b, color).corner_radii(px(r)));
}

/// A radial gradient from `color` at `alpha` in the middle to nothing at
/// `radius`, as stacked discs.
pub fn radial_glow(window: &mut Window, c: Point<Pixels>, radius: f32, color: Hsla, alpha: f32) {
    const N: usize = 16;
    let a = alpha.clamp(0., 1.);
    if a <= 0.001 { return; }
    let step = 1. - (1. - a).powf(1. / N as f32);
    for k in (1..=N).rev() { circle(window, c, radius * k as f32 / N as f32, color.opacity(step)); }
}

fn polygon(window: &mut Window, pts: &[Point<Pixels>], fill_with: Option<Background>, stroke: Option<(Pixels, Hsla)>) {
    if pts.len() < 3 { return; }
    if let Some(bg) = fill_with {
        let mut pb = PathBuilder::fill();
        pb.add_polygon(pts, true);
        if let Ok(p) = pb.build() { window.paint_path(p, bg); }
    }
    if let Some((w, c)) = stroke {
        let mut pb = PathBuilder::stroke(w);
        pb.add_polygon(pts, true);
        if let Ok(p) = pb.build() { window.paint_path(p, c); }
    }
}

fn paint(bounds: Bounds<Pixels>, l: &Liquid, look: &MicLook, t: f32, ms: f64, window: &mut Window) {
    let c = bounds.center();
    let (cx, cy) = (f32::from(c.x), f32::from(c.y));
    let r = f32::from(T::CRU_MIC_R);
    let m = l.m;
    radial_glow(window, c, r * 3., look.layers[0].0, 0.18 + 0.2 * l.lv);
    for rp in &l.ripples {
        let a = ((ms - rp.born) / 900.) as f32;
        let (rx, ry) = (6. + 40. * a, (6. + 40. * a) * 0.22);
        let (ex, ey) = (cx + rp.x * r, cy - r * (1. - 0.62 * m) * 0.35);
        let pts: Vec<_> = (0..32).map(|i| { let q = i as f32 / 32. * std::f32::consts::TAU; point(px(ex + q.cos() * rx), px(ey + q.sin() * ry)) }).collect();
        polygon(window, &pts, None, Some((px(1.2), T::cru_drop().opacity(0.5 * (1. - a) * m))));
    }
    let shape = |tt: f32, layer: f32| -> Vec<Point<Pixels>> {
        let half_w = r * (1. + 1.9 * m);
        let band = r * (1. - 0.62 * m);
        (0..=120).map(|i| {
            let a = i as f32 / 120. * std::f32::consts::TAU;
            let wob = r * (0.03 * (a * 3. + tt * 2. + layer).sin() + 0.02 * (a * 5. - tt * 3. + layer * 2.).sin()) * (1. - m) + r * 0.06 * (1. - m) * (a * 2. + tt * 1.3).sin();
            let (sx, sy) = (a.cos() * (r + wob), a.sin() * (r + wob));
            let uu = a.cos();
            let k = ((uu * 0.5 + 0.5) * 63.).floor() as i32;
            let wave = l.hist[((k - 32).unsigned_abs() as usize).min(HIST - 1)] * r * 0.9;
            let surf = -band * 0.35 - wave * (0.6 + 0.4 * (uu * 9. + tt * 8. + layer).sin()) - r * 0.05 * (uu * 14. - tt * 10. + layer).sin();
            let bx = uu * half_w;
            let by = if a.sin() < 0. { surf * (1. - uu.abs().powi(6)) } else { band * 0.55 * (1. - uu.abs().powi(4)) };
            point(px(cx + sx + (bx - sx) * m), px(cy + sy + (by - sy) * m))
        }).collect()
    };
    for (i, (c0, c1)) in look.layers.iter().enumerate() {
        let pts = shape(t + i as f32 * 0.7, i as f32 * 1.3);
        let a = 0.6 * look.dim;
        let bg = linear_gradient(116., linear_color_stop(c0.opacity(a), 0.), linear_color_stop(c1.opacity(a), 1.));
        polygon(window, &pts, Some(bg), None);
    }
    polygon(window, &shape(t, 0.), None, Some((px(1.), T::white().opacity(0.22))));
    // The light on its top.
    let (hx, hy) = (cx - r * 0.25 * (1. - m) - r * 0.6 * m, cy - r * 0.55 * (1. - m) - r * 0.38 * m);
    let (hrx, hry, rot) = (r * (0.34 + 0.5 * m), r * (0.13 - 0.06 * m).max(0.02), -0.2 * (1. - m));
    let hl: Vec<_> = (0..32).map(|i| {
        let q = i as f32 / 32. * std::f32::consts::TAU;
        let (x, y) = (q.cos() * hrx, q.sin() * hry);
        point(px(hx + x * rot.cos() - y * rot.sin()), px(hy + x * rot.sin() + y * rot.cos()))
    }).collect();
    polygon(window, &hl, Some(T::white().opacity(0.22 * (1. - 0.5 * m)).into()), None);
    for d in &l.drops {
        let a = ((ms - d.born) / 1000.) as f32;
        circle(window, point(px(cx + d.x * r + d.vx * a), px(cy - r * 0.4 + d.vy * a + 260. * a * a)), 2.4 * (1. - a * 0.5), T::cru_drop().opacity(0.8 * (1. - a)));
    }
}

/// The mic: a `box_size` square to click, the water drawn round it.
pub fn liquid_mic(id: impl Into<ElementId>, liquid: &Liquid, look: MicLook, t: f32, ms: f64, box_size: Pixels, tip: impl Into<SharedString>) -> Stateful<Div> {
    let (l, cw, ch) = (liquid.clone(), T::CRU_MIC_CANVAS_W, T::CRU_MIC_CANVAS_H);
    div().id(id).relative().flex_none().size(box_size).cursor_pointer()
        .child(canvas(|_, _, _| {}, move |b, _, window, _| paint(b, &l, &look, t, ms, window))
            .absolute().left((box_size - cw) / 2.).top((box_size - ch) / 2.).w(cw).h(ch))
        .tooltip(crate::tooltip(tip.into()))
}
