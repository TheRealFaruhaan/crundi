//! A new pane's launch buttons (the web's .term-launch buttons): Empty
//! Shell, UI Mode (the hero: the indigo gradient, its block caret), Terminal
//! and the two Skip Permissions shields (icon only at rest; open, they name
//! themselves while their row-mate gives up its label, so the row's width
//! stays the same). Added by Workbench (AREAS.md, "A new control").
//!
//! UI Mode's gradient drifts and its caret blinks, as the web's (chatDrift,
//! 9 s; chatBlink, 1.15 s): the owner's call (OWNER-DECISIONS.md, 20 Nov),
//! capped at 30 frames a second (rule 6) and running only while the button
//! is drawn (an animation element asks for its next frame from its render;
//! a launcher not on screen is not rendered, so it asks for none) and
//! `live`: the launcher passes false while the window is unfocused,
//! minimised or covered, and the button rests on its first frame.

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use std::time::Duration;

/// The drift's and the blink's frame cap (rule 6).
const FX_FPS: f32 = 30.;
/// One animation for both: 207 s is a whole number of drifts (9 s) and of
/// blinks (1.15 s), so neither jumps when it repeats.
const FX_LOOP: f32 = 207.;
const DRIFT: f32 = 9.;
const BLINK: f32 = 1.15;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LaunchLook { Shell, Chat, Normal, Shield }

/// One launch button: `label` shown or folded away (a shield at rest, a
/// main button while its shield is open); `busy` spins in place of its
/// icon; `off`: another launch is under way; `live`: UI Mode's drift and
/// blink run (else it rests on their first frame and asks for no frames).
#[allow(clippy::too_many_arguments)]
pub fn launch_button(id: impl Into<ElementId>, look: LaunchLook, icon: Option<IconName>, label: &'static str, show_label: bool, busy: bool, off: bool, live: bool) -> Stateful<Div> {
    let base = div().id(id).flex().items_center().justify_center().overflow_hidden().whitespace_nowrap()
        .py(T::LAUNCH_PY).rounded(T::R_CONTROL).border_1().text_size(T::TEXT_14)
        .when(!off, |d| d.cursor_pointer())
        .when(off && !busy, |d| d.opacity(0.5));
    let b = match look {
        LaunchLook::Shell => base.min_w(T::LAUNCH_MIN_W).px(T::SP_24).border_color(T::border()).text_color(T::text_secondary())
            .when(!off, |d| d.hover(|s| s.text_color(T::text()).border_color(T::accent()))),
        LaunchLook::Chat => base.relative().flex_1().min_w_0().px(T::SP_12).border_color(T::launch_chat_edge()).text_color(T::white()).font_weight(T::W_SEMIBOLD)
            .bg(T::launch_chat_stops()[0])
            .child({
                let bed = div().absolute().inset_0().flex();
                if live { bed.with_animation("chat-drift", fx(), |d, delta| drift(d, delta * FX_LOOP)).into_any_element() }
                else { drift(bed, 0.).into_any_element() }
            })
            .when(!off, |d| d.hover(|s| s.border_color(T::accent_hover()))),
        LaunchLook::Normal => base.flex_1().min_w_0().px(T::SP_12).border_color(T::accent_edge()).bg(T::launch_normal_bg()).text_color(T::text())
            .when(!off, |d| d.hover(|s| s.border_color(T::accent()).bg(T::accent_dim()))),
        LaunchLook::Shield => base.flex_none().w(if show_label { T::LAUNCH_SHIELD_OPEN } else { T::LAUNCH_SHIELD }).border_color(T::border()).bg(T::bg_tertiary()).text_color(T::text())
            .when(!off, |d| d.hover(|s| s.border_color(T::accent()).bg(T::accent_dim()))),
    };
    let mark: Option<AnyElement> = if busy { Some(crate::spinner(T::ICON_MD).into_any_element()) }
        else if look == LaunchLook::Chat {
            let caret = div().relative().font_family(T::MONO).child("\u{258d}");
            Some(if live { caret.with_animation("chat-blink", fx(), |d, delta| d.opacity(blink_opacity(delta * FX_LOOP))).into_any_element() }
                else { caret.opacity(blink_opacity(0.)).into_any_element() })
        }
        else { icon.map(|i| div().flex_none().child(crate::icon(i, T::ICON_MD)).into_any_element()) };
    let gap = if look == LaunchLook::Chat { T::LAUNCH_CHAT_GAP } else { T::LAUNCH_GAP };
    b.children(mark).when(show_label, |d| d.child(div().relative().when(look != LaunchLook::Shell, |d| d.ml(gap)).child(label)))
}

/// The gradient's pieces at `t` s of the drift.
fn drift(d: Div, t: f32) -> Div {
    let segs = drift_segments(drift_offset(t));
    let n = segs.len();
    d.children(segs.into_iter().enumerate().map(move |(i, (x0, x1, a, b))| {
        let r = T::R_CONTROL - px(1.);
        div().h_full().w(relative(x1 - x0)).flex_none()
            .bg(linear_gradient(90., linear_color_stop(a, 0.), linear_color_stop(b, 1.)))
            .when(i == 0, |d| d.rounded_l(r)).when(i + 1 == n, |d| d.rounded_r(r))
    }))
}

fn fx() -> Animation { Animation::new(Duration::from_secs_f32(FX_LOOP)).repeat().with_max_fps(FX_FPS) }

/// chatBlink: steps(2, start) over 1.15 s: 0.95 for its first half, 0.15
/// for the second.
fn blink_opacity(t: f32) -> f32 { if (t % BLINK) / BLINK < 0.5 { 0.95 } else { 0.15 } }

/// chatDrift: the gradient is 260% of the button wide and repeats; its
/// position goes from 0% to -260% in 9 s, which (as CSS resolves a
/// percentage position, (box - image) × p) moves it right by 4.16 button
/// widths. Its offset at `t` s, within one image (0 ≤ o < 2.6).
fn drift_offset(t: f32) -> f32 { ((t % DRIFT) / DRIFT * 4.16).rem_euclid(2.6) }

/// What the button shows of the gradient at offset `o`: pieces (from, to,
/// as fractions of its width) each a straight blend between two colours,
/// cut where the web's stops are (0, 26, 50, 74, 100% of the image).
fn drift_segments(o: f32) -> Vec<(f32, f32, Hsla, Hsla)> {
    const P: f32 = 2.6;
    const STOPS: [f32; 5] = [0., 0.26, 0.5, 0.74, 1.];
    let mut xs = vec![0., 1.];
    for k in -1..=1 {
        for s in STOPS { let x = o + P * (s + k as f32); if x > 0.001 && x < 0.999 { xs.push(x); } }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    let at = |x: f32, left: bool| {
        let mut u = ((x - o) / P).rem_euclid(1.);
        if left && u < 0.0005 { u = 1.; }
        sample(u)
    };
    xs.windows(2).map(|w| (w[0], w[1], at(w[0], false), at(w[1], true))).collect()
}

/// The web's 5-stop gradient at `u` (0..1 of the image), blended in sRGB.
fn sample(u: f32) -> Hsla {
    const STOPS: [f32; 5] = [0., 0.26, 0.5, 0.74, 1.];
    let c = T::launch_chat_stops();
    let cols = [c[0], c[1], c[2], c[1], c[0]];
    let i = (1..5).find(|&i| u <= STOPS[i]).unwrap_or(4);
    let f = ((u - STOPS[i - 1]) / (STOPS[i] - STOPS[i - 1])).clamp(0., 1.);
    let (a, b) = (Rgba::from(cols[i - 1]), Rgba::from(cols[i]));
    let l = |x: f32, y: f32| x + (y - x) * f;
    Rgba { r: l(a.r, b.r), g: l(a.g, b.g), b: l(a.b, b.b), a: l(a.a, b.a) }.into()
}

#[cfg(test)]
mod tests {
    use super::{blink_opacity, drift_offset, drift_segments};
    use gpui::Rgba;
    #[test]
    fn the_drift_covers_the_button_without_gaps() {
        for i in 0..200 {
            let segs = drift_segments(drift_offset(i as f32 * 0.07));
            assert!((segs[0].0).abs() < 1e-4 && (segs.last().unwrap().1 - 1.).abs() < 1e-4);
            for w in segs.windows(2) { assert!((w[0].1 - w[1].0).abs() < 1e-4); }
        }
        // At rest (t = 0) the left edge is the image's start (#4338ca).
        let s = drift_segments(drift_offset(0.));
        let a = Rgba::from(s[0].2);
        assert!((a.r - 0x43 as f32 / 255.).abs() < 0.01);
    }
    #[test]
    fn the_caret_blinks_in_two_steps() {
        assert_eq!(blink_opacity(0.1), 0.95);
        assert_eq!(blink_opacity(0.7), 0.15);
        assert_eq!(blink_opacity(1.2), 0.95);
    }
}
