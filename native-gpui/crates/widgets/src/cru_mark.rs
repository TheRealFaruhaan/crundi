//! Cru's mark (app/vendor/crundi-voice.js cruIcon, the app's CruIcon.kt):
//! three bars centred like a sound wave, the tall one running into a joined
//! "cru", one stroke on a dark layer with a light edge, in the theme's
//! gradient (T::CRU_MARK). Drawn as a full-colour SVG (gpui's svg() is one
//! tint), so it is an image. While Cru is live the two short bars breathe
//! (the web's cru-p: scaleY 1 → .45 → 1 over .9 s, the second .15 s later):
//! FRAMES pictures of one cycle, made once.

use crundi_theme::T;
use gpui::*;
use std::{cell::RefCell, sync::Arc, time::Duration};

const WORD: &str = "M76.6 24C70.2 12 54.2 14 51 38C48.6 60 57.4 74 68.6 72C76.6 71 81.4 64 84.6 58C87.8 51 90.2 46 92.6 42L92.6 72L92.6 58C95.8 48 100.6 42 107 42C105.4 56 105.4 71 114.2 72C123 73 127.8 60 129.4 42C127.8 58 127.8 72 135.8 72C140.6 72 143.8 67 146.2 62";
const FRAMES: usize = 12;
const CYCLE: Duration = Duration::from_millis(900);
/// Drawn at this many pixels per unit of the mark's height, then scaled down: crisp on HiDPI.
const OVERSAMPLE: f32 = 3.0;

/// One bar centred at y 45, half-height `h`, scaled by `s`.
fn bar(x: f32, h: f32, s: f32) -> String { format!("M{x} {:.2}V{:.2}", 45.0 - h * s, 45.0 + h * s) }

fn stops(cs: &[(f32, u32)]) -> String {
    cs.iter().map(|(o, c)| format!("<stop offset=\"{o}\" stop-color=\"#{c:06x}\"/>")).collect()
}

/// The mark as SVG, the short bars scaled by s1 and s2.
pub fn svg_text(s1: f32, s2: f32) -> String {
    let hpx = f32::from(T::CRU_MARK_H) * OVERSAMPLE;
    let wpx = (hpx * 150.0 / 90.0).round();
    let lines = format!("<path d=\"{}\"/><path d=\"{}\"/><path d=\"M41 10V80\"/><path d=\"{WORD}\"/>", bar(15.0, 10.0, s1), bar(28.0, 20.0, s2));
    format!(concat!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"6 0 150 90\" width=\"{w}\" height=\"{h}\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\">",
        "<defs><linearGradient id=\"g\" gradientUnits=\"userSpaceOnUse\" x1=\"8\" y1=\"90\" x2=\"155\" y2=\"0\">{g}</linearGradient>",
        "<linearGradient id=\"d\" gradientUnits=\"userSpaceOnUse\" x1=\"8\" y1=\"90\" x2=\"155\" y2=\"0\">{d}</linearGradient></defs>",
        "<g stroke=\"url(#d)\" stroke-width=\"9\" transform=\"translate(1.6 2.4)\">{l}</g>",
        "<g stroke=\"url(#g)\" stroke-width=\"9\">{l}</g>",
        "<g stroke=\"#fff\" stroke-opacity=\".28\" stroke-width=\"2.4\" transform=\"translate(-1.2 -1.4)\">{l}</g></svg>"),
        w = wpx, h = hpx, g = stops(&T::CRU_MARK), d = stops(&T::CRU_MARK_DARK), l = lines)
}

/// cru-p's ease: 1 at the ends of the cycle, .45 halfway.
fn breathe(p: f32) -> f32 { let p = p.rem_euclid(1.0); 1.0 - 0.55 * (1.0 - (std::f32::consts::TAU * p).cos()) / 2.0 }

thread_local! { static PICS: RefCell<Vec<Arc<Image>>> = const { RefCell::new(Vec::new()) }; }

/// Frame 0 is the mark at rest; 0..FRAMES one live cycle.
fn frame(i: usize) -> Arc<Image> {
    PICS.with(|p| {
        let mut p = p.borrow_mut();
        if p.is_empty() {
            let lag = 0.15 / CYCLE.as_secs_f32();
            *p = (0..FRAMES).map(|k| { let t = k as f32 / FRAMES as f32; Arc::new(Image::from_bytes(ImageFormat::Svg, svg_text(breathe(t), breathe(t - lag)).into_bytes())) }).collect();
        }
        p[i % FRAMES].clone()
    })
}

fn pic(i: usize) -> Img { img(frame(i)).h(T::CRU_MARK_H).w(T::CRU_MARK_H * (150.0 / 90.0)) }

/// The mark, still, or breathing while `live`.
pub fn cru_mark(live: bool) -> AnyElement {
    if !live { return div().flex_none().child(pic(0)).into_any_element(); }
    // Each step of the animation draws the box afresh with that step's picture.
    div().flex_none()
        .with_animation("cru-live", Animation::new(CYCLE).repeat(), |_, t| div().flex_none().child(pic((t * FRAMES as f32) as usize)))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{breathe, svg_text};
    #[test]
    fn breathes_and_parses() {
        assert!((breathe(0.0) - 1.0).abs() < 1e-4 && (breathe(0.5) - 0.45).abs() < 1e-4);
        let s = svg_text(1.0, 0.5);
        assert!(s.contains("#38bdf8") && s.contains("#e879f9") && s.contains("M15 35.00V55.00") && s.contains("M28 35.00V55.00"));
    }
}
