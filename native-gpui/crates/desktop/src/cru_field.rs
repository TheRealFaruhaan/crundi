//! Cru's field in the voice view (crundi-voice.js frame and drawField): the
//! Crundi head at the centre of a contour visualiser that follows the live
//! voice (twenty-two hairlines round a dark disc, turning in a sky, indigo,
//! violet, pink sweep), the usage arcs inside the disc, and the projects as
//! cells round it, coloured by their state (KIND) and joined to the centre
//! by strands with drops running along them. Drawn on a GPUI canvas, a
//! frame at a time while Cru's screen is open (cru_ui asks for the next).
//!
//! GPUI has no radial or conic gradients: radial glows are stacked discs
//! (w::radial_glow), the sweep is each line drawn in short arcs of their
//! own colour. The head is the web's LOGO (a vector drawn in one colour per
//! part: GPUI's SVG drawing is a tint), its body the accent, its antenna amber.

use crate::cru::Cell;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::*;
use std::f32::consts::{PI, TAU};

const LINES: usize = 22;
const WAVE: usize = 96;
/// Each contour's arcs (the sweep's resolution) and its points.
const ARCS: usize = 12;
const STEPS: usize = 144;

/// The head's body (the LOGO's rounded square with its eyes cut out) and its antenna, as the web draws it (viewBox 96).
const HEAD_BODY: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><mask id="eh"><rect width="96" height="96" fill="#fff"/><rect x="32" y="43" width="10.5" height="18" rx="5.2" fill="#000"/><rect x="53.5" y="43" width="10.5" height="18" rx="5.2" fill="#000"/></mask><rect x="11" y="26" width="74" height="57" rx="23" fill="#fff" mask="url(#eh)"/></svg>"##;
const HEAD_ANTENNA: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><path d="M48 27V17" fill="none" stroke-linecap="round" stroke="#fff" stroke-width="5.5"/><circle cx="48" cy="13" r="6.5" fill="#fff"/></svg>"##;

/// The level the field follows (the web's S.lvl, S.peak, S.wave).
#[derive(Clone)]
pub struct FieldState { pub lvl: f32, peak: f32, wave: [f32; WAVE], last_shift: f64, bump: f32, seed: u32 }

impl Default for FieldState {
    fn default() -> Self { FieldState { lvl: 0., peak: 0.12, wave: [0.; WAVE], last_shift: 0., bump: 0.4, seed: 0x2545_f491 } }
}

impl FieldState {
    /// One frame (frame()): `mic` the microphone's level while listening;
    /// while Cru speaks its voice is a bump on each word (no analyser here).
    pub fn step(&mut self, ms: f64, t: f32, phase: &str, mic: f32) {
        let voice = if phase == "speak" {
            self.bump *= 0.94;
            if self.bump < 0.3 { self.seed ^= self.seed << 13; self.seed ^= self.seed >> 17; self.seed ^= self.seed << 5; self.bump = 0.6 + (self.seed % 1000) as f32 / 1000. * 0.35; }
            0.3 + self.bump * 0.6 + 0.1 * (t * 9.).sin().abs()
        } else { 0. };
        let target = match phase { "listen" => mic, "speak" => voice, "think" => 0.08 + 0.05 * (t * 3.).sin(), _ => 0.03 };
        self.peak = (self.peak * 0.996).max(target.max(0.08));
        let norm = if phase == "listen" || phase == "speak" { (target / self.peak).min(1.) } else { target };
        self.lvl += (norm - self.lvl) * if norm > self.lvl { 0.45 } else { 0.15 };
        if ms - self.last_shift > 16. { self.wave.copy_within(0..WAVE - 1, 1); self.wave[0] = self.lvl; self.last_shift = ms; }
    }
}

/// What one frame draws.
#[derive(Clone)]
pub struct Frame { pub t: f32, pub phase: String, pub state: FieldState, pub cells: Vec<Cell>, pub arcs: Vec<(Hsla, f32)> }

/// PALETTE: the state's first colour (the glows, the rim when there is no sweep).
pub fn palette(phase: &str) -> Hsla {
    match phase { "think" => T::cru_violet(), "speak" => T::accent_hover(), "waking" => T::accent(), "ended" => T::text_muted(), _ => T::sky() }
}

/// KIND: a cell's colour by its project's state.
pub fn kind_color(kind: &str) -> Hsla {
    match kind { "working" => T::yellow(), "waiting" => T::sky(), "needs" => T::accent_hover(), "error" => T::red(), "running" => T::green(), _ => T::text_muted() }
}

/// The sweep (the conic gradient's stops) at `f` of the turn.
fn sweep(f: f32) -> Hsla {
    let stops = [(0., T::sky()), (0.18, T::accent()), (0.36, T::cru_violet()), (0.5, T::cru_fuchsia()), (0.64, T::cru_violet()), (0.82, T::accent()), (1., T::sky())];
    let f = f.rem_euclid(1.);
    for w2 in stops.windows(2) {
        let ((a, ca), (b, cb)) = (w2[0], w2[1]);
        if f <= b {
            let k = if b > a { (f - a) / (b - a) } else { 0. };
            let (x, y) = (ca.to_rgb(), cb.to_rgb());
            return Rgba { r: x.r + (y.r - x.r) * k, g: x.g + (y.g - x.g) * k, b: x.b + (y.b - x.b) * k, a: 1. }.into();
        }
    }
    T::sky()
}

fn pt(x: f32, y: f32) -> Point<Pixels> { point(px(x), px(y)) }

fn circle(window: &mut Window, x: f32, y: f32, r: f32, c: Hsla) {
    if r <= 0. { return; }
    window.paint_quad(fill(Bounds { origin: pt(x - r, y - r), size: size(px(2. * r), px(2. * r)) }, c).corner_radii(px(r)));
}

fn stroke(window: &mut Window, pts: &[Point<Pixels>], closed: bool, width: f32, c: Hsla) {
    if pts.len() < 2 { return; }
    let mut pb = PathBuilder::stroke(px(width));
    pb.add_polygon(pts, closed);
    if let Ok(p) = pb.build() { window.paint_path(p, c); }
}

/// A cell's wobbling outline (blob): 26 points joined by quadratic curves.
fn blob(x: f32, y: f32, r: f32, t: f32, s: f32, wob: f32) -> PathBuilder {
    let n = 26;
    let p: Vec<(f32, f32)> = (0..n).map(|k| {
        let a = k as f32 / n as f32 * TAU;
        let rr = r * (1. + wob * (0.6 * (3. * a + t * 1.3 + s).sin() + 0.4 * (5. * a - t * 0.9 + s * 2.).sin()));
        (x + a.cos() * rr, y + a.sin() * rr)
    }).collect();
    let mut pb = PathBuilder::fill();
    let z = p[n - 1];
    pb.move_to(pt((z.0 + p[0].0) / 2., (z.1 + p[0].1) / 2.));
    for k in 0..n {
        let (a1, b1) = (p[k], p[(k + 1) % n]);
        pb.curve_to(pt((a1.0 + b1.0) / 2., (a1.1 + b1.1) / 2.), pt(a1.0, a1.1));
    }
    pb.close();
    pb
}

fn bez(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), u: f32) -> (f32, f32) {
    let v = 1. - u;
    (v * v * v * p0.0 + 3. * v * v * u * p1.0 + 3. * v * u * u * p2.0 + u * u * u * p3.0, v * v * v * p0.1 + 3. * v * v * u * p1.1 + 3. * v * u * u * p2.1 + u * u * u * p3.1)
}

fn paint(b: Bounds<Pixels>, f: &Frame, window: &mut Window, cx: &mut App) {
    let (w0, h0) = (f32::from(b.size.width), f32::from(b.size.height));
    let (cx0, cy0) = (f32::from(b.center().x), f32::from(b.center().y));
    let (t, lvl, a) = (f.t, f.state.lvl, palette(&f.phase));
    let l = f32::from(T::CRU_HEAD_L).min(w0.min(h0 * 0.8) / 390. * 40.);
    w::radial_glow(window, b.center(), w0.min(h0) * 0.5, a, 0.16 + 0.12 * lvl);
    // The cells, round the field.
    let n = f.cells.len();
    let (rx, ry) = (w0 / 2. - f32::from(T::CRU_CELL_INSET_X), h0 / 2. - f32::from(T::CRU_CELL_INSET_Y));
    let placed: Vec<(usize, f32, f32, f32)> = (0..n).map(|i| {
        let an = -PI / 2. + i as f32 / n.max(1) as f32 * TAU + t * 0.025 + 0.06 * (t * 0.4 + i as f32).sin();
        let c = &f.cells[i];
        let r = f32::from(if c.kind == "idle" { T::CRU_CELL_R_IDLE } else { T::CRU_CELL_R }) + if c.service.is_some() { 1.5 } else { 0. };
        (i, cx0 + an.cos() * (rx + 6. * (t * 0.5 + i as f32 * 1.7).sin()), cy0 + an.sin() * (ry + 6. * (t * 0.45 + i as f32).cos()), r)
    }).collect();
    let disc = l * 1.38;
    let gap = 3.2;
    let r0 = disc + 5. + LINES as f32 * gap - 4.;
    // The strands, the centre to each cell, with drops running along the busy ones.
    for &(i, x, y, r) in &placed {
        let d = ((x - cx0).powi(2) + (y - cy0).powi(2)).sqrt();
        if d < 1. || d - r <= r0 { continue; }
        let (dx, dy, wv) = ((x - cx0) / d, (y - cy0) / d, 14. * (t * 0.8 + i as f32).sin());
        let c = &f.cells[i];
        let (col, idle) = (kind_color(c.kind), c.kind == "idle");
        let p0 = (cx0 + dx * r0, cy0 + dy * r0);
        let p3 = (cx0 + dx * (d - r), cy0 + dy * (d - r));
        let p1 = (p0.0 + dx * 28. - dy * wv, p0.1 + dy * 28. + dx * wv);
        let p2 = (p3.0 - dx * 28. - dy * wv, p3.1 - dy * 28. + dx * wv);
        for (wd, al) in [(f32::from(T::CRU_STRAND), if idle { 0.12 } else { 0.22 }), (1.2, if idle { 0.25 } else { 0.55 })] {
            let mut pb = PathBuilder::stroke(px(wd));
            pb.move_to(pt(p0.0, p0.1));
            pb.cubic_bezier_to(pt(p3.0, p3.1), pt(p1.0, p1.1), pt(p2.0, p2.1));
            if let Ok(p) = pb.build() { window.paint_path(p, col.opacity(al)); }
        }
        if !idle && c.kind != "running" && c.kind != "error" {
            for j in 0..3 {
                let u = (t * 0.45 + j as f32 / 3. + i as f32 * 0.13).rem_euclid(1.);
                let (bx, by) = bez(p3, p2, p1, p0, u);
                circle(window, bx, by, f32::from(T::CRU_DROP), col.opacity(0.9 * (1. - u * 0.6)));
            }
        }
    }
    for &(i, x, y, r0c) in &placed {
        let c = &f.cells[i];
        let (col, idle) = (kind_color(c.kind), c.kind == "idle");
        let r = r0c * match c.kind { "working" => 1. + 0.08 * (t * 4. + i as f32).sin(), "error" => 1. + 0.05 * (t * 8.).sin(), _ => 1. };
        if !idle { circle(window, x, y, r * 1.9, col.opacity(0.1)); }
        let s = i as f32 * 2.1;
        let wob = if idle { 0.05 } else { 0.09 };
        if let Ok(p) = blob(x, y, r, t, s, wob).build() {
            window.paint_path(p, linear_gradient(135., linear_color_stop(col.opacity(if idle { 0.22 } else { 0.45 }), 0.), linear_color_stop(col.opacity(0.05), 1.)));
        }
        let mut edge = blob(x, y, r, t, s, wob);
        edge.style = PathStyle::Stroke(StrokeOptions::default().with_line_width(1.4));
        if let Ok(p) = edge.build() { window.paint_path(p, col.opacity(if idle { 0.45 } else { 0.9 })); }
        circle(window, x + (t * 0.7 + i as f32).sin() * 2., y + (t * 0.6 + i as f32).cos() * 2., r * 0.28, col.opacity(0.95));
        if let Some(up) = c.service {
            let sa = t * 1.2 + i as f32;
            circle(window, x + sa.cos() * r * 0.62, y + sa.sin() * r * 0.62, f32::from(T::CRU_DROP), if up { T::green() } else { T::text_muted() });
        }
        let name: String = if c.name.chars().count() > 14 { c.name.chars().take(13).collect::<String>() + "…" } else { c.name.clone() };
        let run = TextRun { len: name.len(), font: Font { weight: T::W_MEDIUM, ..font(T::UI_FONT) }, color: if idle { T::text_muted() } else { T::cru_label() }, background_color: None, underline: None, strikethrough: None };
        let line = window.text_system().shape_line(name.into(), T::CRU_CELL_NAME, &[run], None);
        let lw = f32::from(line.width);
        let _ = line.paint(pt(x - lw / 2., y + r + 4.), T::CRU_CELL_NAME * T::LH_UI, TextAlign::Left, None, window, cx);
    }
    // The contours: hairlines round the disc, swelling with the voice.
    let spin = (if f.phase == "think" { t * 60. } else { t * 6. }).to_radians();
    let swell = 8. + 38. * lvl;
    for li in 0..LINES {
        let fr = (li + 1) as f32 / LINES as f32;
        let base = disc + 5. + li as f32 * gap;
        let amp = swell * fr * fr + 2.5 * fr;
        let alpha = ((1. - fr * 0.7) * (0.7 + 0.3 * lvl)).max(0.12);
        let pts: Vec<Point<Pixels>> = (0..=STEPS).map(|s2| {
            let an2 = s2 as f32 / STEPS as f32 * TAU;
            let fx = (s2 as f32 / STEPS as f32 * 2. - 1.).abs();
            let wv = f.state.wave[(((1. - fx) * (WAVE - 25) as f32).floor() as usize + li / 3).min(WAVE - 1)];
            let sh = 0.55 * (an2 * 3. + t * 0.6 + fr * 1.2).sin() + 0.3 * (an2 * 5. - t * 0.45 + fr * 2.1).sin() + 0.15 * (an2 * 2. + t * 0.8).sin();
            let rr = base + amp * (0.35 + 0.65 * wv) * (0.6 + 0.4 * sh) + amp * 0.5 * sh;
            pt(cx0 + (an2 - PI / 2.).cos() * rr, cy0 + (an2 - PI / 2.).sin() * rr)
        }).collect();
        let per = STEPS / ARCS;
        for k in 0..ARCS {
            let mid = (k as f32 + 0.5) / ARCS as f32 * TAU - PI / 2.;
            let col = sweep((mid - spin) / TAU);
            stroke(window, &pts[k * per..=((k + 1) * per).min(STEPS)], false, 1., col.opacity(alpha));
        }
    }
    // The disc, its rim, the usage arcs, the head.
    w::radial_glow(window, b.center(), disc * 1.6, a, 0.35 + 0.4 * lvl);
    circle(window, cx0, cy0, disc, T::cru_disc());
    let rim: Vec<Point<Pixels>> = (0..=STEPS).map(|s| { let q = s as f32 / STEPS as f32 * TAU - PI / 2.; pt(cx0 + q.cos() * disc, cy0 + q.sin() * disc) }).collect();
    let per = STEPS / ARCS;
    for k in 0..ARCS {
        let mid = (k as f32 + 0.5) / ARCS as f32 * TAU - PI / 2.;
        stroke(window, &rim[k * per..=((k + 1) * per).min(STEPS)], false, 2. + 1.5 * lvl, sweep((mid - spin) / TAU));
    }
    for (k, (col, v)) in f.arcs.iter().enumerate() {
        let rad = disc - (6. + k as f32 * 4.5);
        if rad <= 0. { continue; }
        let ring: Vec<_> = (0..=72).map(|s| { let q = s as f32 / 72. * TAU; pt(cx0 + q.cos() * rad, cy0 + q.sin() * rad) }).collect();
        stroke(window, &ring, true, 2.2, T::border().opacity(0.6));
        let sweep_to = v.clamp(0.05, 1.) * TAU;
        let steps = ((sweep_to / TAU) * 72.).ceil().max(2.) as usize;
        let arc: Vec<_> = (0..=steps).map(|s| { let q = -PI / 2. + s as f32 / steps as f32 * sweep_to; pt(cx0 + q.cos() * rad, cy0 + q.sin() * rad) }).collect();
        stroke(window, &arc, false, 2.6, *col);
    }
    let sz = (l * 1.45 * (1. + 0.06 * lvl)).round();
    let hb = Bounds { origin: pt(cx0 - sz / 2., cy0 - sz / 2. - 2.), size: size(px(sz), px(sz)) };
    let _ = window.paint_svg(hb, "cru-head-body".into(), Some(HEAD_BODY.as_bytes()), TransformationMatrix::unit(), T::accent(), cx);
    let _ = window.paint_svg(hb, "cru-head-antenna".into(), Some(HEAD_ANTENNA.as_bytes()), TransformationMatrix::unit(), T::amber(), cx);
}

/// The field, filling its box.
pub fn field(frame: Frame) -> impl IntoElement {
    canvas(|_, _, _| {}, move |b, _, window, cx| paint(b, &frame, window, cx)).size_full()
}

#[cfg(test)]
mod tests {
    use super::{sweep, FieldState};
    #[test]
    fn sweeps_and_follows() {
        let s0 = sweep(0.).to_rgb();
        let s1 = sweep(1.).to_rgb();
        assert!((s0.r - s1.r).abs() < 1e-3 && (s0.b - s1.b).abs() < 1e-3);
        let mut f = FieldState::default();
        for i in 0..30 { f.step(i as f64 * 20., i as f32 * 0.02, "listen", 0.5); }
        assert!(f.lvl > 0.5);
        for i in 30..200 { f.step(i as f64 * 20., i as f32 * 0.02, "ended", 0.); }
        assert!(f.lvl < 0.1);
    }
}
