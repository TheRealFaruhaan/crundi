//! The top bar's status pills (the web's `.topbar .status-badge` and its
//! states, src/webapp-html.js): the connection badge (connected,
//! reconnecting, offline, disconnected), the Telegram badge ("tg: @bot" /
//! "tg: offline") and the usage badge ("updated … ago", `.usage-updated`).
//! Mono 12, a pill 3 by 10; a 6 px dot before the text: green (connected),
//! yellow and blinking (reconnecting: the web's reconnBlink, 1.2 s, only
//! while it shows, capped at T::ANIM_FPS), red (offline), red at 80%
//! (disconnected); the usage badge has its icon and no dot, slate, brighter
//! on hover. (The web's connected pulse, an endless ring, is left out: an
//! idle window draws no frames, ARCHITECTURE.md rule 6.)
//! `status_badge` (lib.rs) stays as it was.

use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PillTone { Connected, Reconnecting, Offline, Disconnected, Neutral }

/// The 6 px dot.
fn pill_dot(c: Hsla) -> Div { div().flex_none().size(T::SP_6).rounded(T::R_PILL).bg(c) }

/// A status pill; `icon` before the text (the usage badge's chart).
pub fn status_pill(id: impl Into<ElementId>, icon: Option<IconName>, label: impl Into<SharedString>, tone: PillTone) -> Stateful<Div> {
    let (fg, bg) = match tone {
        PillTone::Connected => (T::green(), T::green_dim()),
        PillTone::Reconnecting => (T::yellow(), T::yellow_dim()),
        PillTone::Offline | PillTone::Disconnected => (T::red(), T::red_dim()),
        PillTone::Neutral => (T::text_secondary(), T::bg_hover()),
    };
    let d = div().id(id).flex().flex_none().items_center().gap(T::SP_6).py(T::SP_2 + px(1.)).px(T::SP_10).rounded(T::R_PILL)
        .whitespace_nowrap().bg(bg).text_color(fg).text_size(T::TEXT_12).font_family(T::MONO);
    let d = match tone {
        PillTone::Connected => d.child(pill_dot(T::green())),
        PillTone::Offline => d.child(pill_dot(T::red())),
        PillTone::Disconnected => d.child(pill_dot(T::red()).opacity(0.8)),
        PillTone::Reconnecting => d.child(pill_dot(T::yellow()).with_animation("pill-blink",
            Animation::new(Duration::from_millis(1200)).repeat().with_max_fps(T::ANIM_FPS),
            // 50%: 0.25, eased in and out.
            |d, t| { let k = (1.0 - (std::f32::consts::TAU * t).cos()) / 2.0; d.opacity(1.0 - 0.75 * k) })),
        PillTone::Neutral => d.cursor_pointer().hover(|d| d.text_color(T::text())),
    };
    d.when_some(icon, |d, i| d.child(crate::icon(i, T::ICON_SM))).child(label.into())
}
