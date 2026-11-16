//! A small time chart: one or more series, each a filled area under its
//! line, on a shared scale, filling the element's box (the web's sparkSvg on
//! Services' usage strip and chartSvg on Docker's Stats). Nothing is drawn
//! for a series under two samples (one sample is not a trend). With `slots`
//! the samples sit on the right, `slots` to the width (Docker's 90), as new
//! ones arrive from the right; with 0 they spread across. `mid`: a hairline
//! across the middle. (Made by Operations, AREAS.md "A new control".)

use crundi_theme::T;
use gpui::*;

/// One series: its values, the value at the top of the box, its colours.
#[derive(Clone)]
pub struct Series { pub values: Vec<f32>, pub max: f32, pub stroke: Hsla, pub fill: Hsla }

/// Where the samples fall in a box `w` × `h` (y down; a value of `max` one
/// pixel below the top, zero one above the bottom). Pure, for the tests.
pub fn points(values: &[f32], max: f32, slots: usize, w: f32, h: f32) -> Vec<(f32, f32)> {
    let n = values.len();
    if n < 2 { return vec![]; }
    let max = if max > 0.0 { max } else { 1.0 };
    let (step, x0) = if slots >= 2 {
        let step = w / (slots as f32 - 1.0);
        let n = n.min(slots);
        (step, w - (n as f32 - 1.0) * step)
    } else { (w / (n as f32 - 1.0), 0.0) };
    let vals = &values[n.saturating_sub(if slots >= 2 { slots } else { n })..];
    vals.iter().enumerate().map(|(i, v)| {
        let r = (v / max).clamp(0.0, 1.0);
        (x0 + i as f32 * step, (h - 1.0) - r * (h - 2.0))
    }).collect()
}

pub fn spark(series: Vec<Series>, slots: usize, mid: Option<Hsla>) -> Canvas<()> {
    canvas(|_, _, _| {}, move |b, _, window, _| {
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        let at = |x: f32, y: f32| point(b.origin.x + px(x), b.origin.y + px(y));
        if let Some(c) = mid {
            window.paint_quad(fill(Bounds::new(at(0.0, (h / 2.0).floor()), size(b.size.width, px(1.))), c));
        }
        for s in &series {
            let pts = points(&s.values, s.max, slots, w, h);
            if pts.is_empty() { continue; }
            let mut area = PathBuilder::fill();
            area.move_to(at(pts[0].0, h));
            for (x, y) in &pts { area.line_to(at(*x, *y)); }
            area.line_to(at(pts[pts.len() - 1].0, h));
            area.close();
            if let Ok(p) = area.build() { window.paint_path(p, s.fill); }
            let mut line = PathBuilder::stroke(T::CHART_STROKE);
            line.move_to(at(pts[0].0, pts[0].1));
            for (x, y) in &pts[1..] { line.line_to(at(*x, *y)); }
            if let Ok(p) = line.build() { window.paint_path(p, s.stroke); }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn placing() {
        assert!(points(&[5.0], 100.0, 0, 100.0, 20.0).is_empty());
        assert_eq!(points(&[0.0, 100.0], 100.0, 0, 100.0, 20.0), vec![(0.0, 19.0), (100.0, 1.0)]);
        // On the right, three of 91 slots.
        let p = points(&[0.0, 0.0, 0.0], 1.0, 91, 90.0, 10.0);
        assert_eq!(p.iter().map(|x| x.0).collect::<Vec<_>>(), vec![88.0, 89.0, 90.0]);
        // More samples than slots: the last ones.
        assert_eq!(points(&[1.0, 2.0, 3.0], 3.0, 2, 10.0, 10.0).len(), 2);
    }
}
