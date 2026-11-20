//! The connection badge and the reconnecting line (the web's #conn-badge,
//! #reconn-line, reconnShow / reconnHide / reconnPaint): a dropped link
//! that comes straight back shows nothing; after 1.2 s the badge says
//! "reconnecting" (yellow, its dot blinking) with the tooltip
//! "Reconnecting" and an amber line sweeps along the top bar's bottom edge;
//! after 9 s more the tooltip says it is the network or the server. Back,
//! the badge says "connected" (no tooltip) and the line is green for a
//! moment (1.6 s, fading in its second half). Before the first connection
//! it says "disconnected". (The web's "offline", this device having no
//! network, has no signal on the desktop: the native client never shows it.)
//!
//! `Conn` is the state (the shell's store, driven by the session's
//! `online`); `ConnBadge` and `ReconnLine` are cached leaves of their own
//! beside the top bar (ARCHITECTURE.md rule 3): the blink and the sweep
//! redraw only them, and only while they show (rule 6). Timers run only
//! while a drop is pending or shown.

use crate::slots::measured;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::*;
use std::time::Duration;

pub const SHOW_AFTER: Duration = Duration::from_millis(1200);
pub const SLOW_AFTER: Duration = Duration::from_millis(9000);
pub const BACK_FOR: Duration = Duration::from_millis(1700);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Badge { #[default] Disconnected, Connected, Reconnecting, Slow }

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Line { #[default] Off, On, Back }

#[derive(Default)]
pub struct Conn {
    pub badge: Badge,
    pub line: Line,
    /// The link was up once this session (the web's appReady + a socket).
    up_once: bool,
    online: bool,
    /// Bumped on every change of the link: a pending timer of an older one does nothing.
    epoch: u64,
}

impl Conn {
    fn set(&mut self, badge: Badge, line: Line, cx: &mut Context<Self>) {
        if self.badge != badge || self.line != line { self.badge = badge; self.line = line; cx.notify(); }
    }

    fn after(&mut self, d: Duration, cx: &mut Context<Self>, f: impl FnOnce(&mut Self, &mut Context<Self>) + 'static) {
        let epoch = self.epoch;
        let t = cx.background_executor().timer(d);
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            t.await;
            let _ = this.update(cx, |c, cx| if c.epoch == epoch { f(c, cx) });
        }).detach();
    }

    /// The live link went up or down (the session's `online`).
    pub fn link(&mut self, on: bool, cx: &mut Context<Self>) {
        if on == self.online { return; }
        self.online = on;
        self.epoch += 1;
        if on {
            self.up_once = true;
            let shown = matches!(self.badge, Badge::Reconnecting | Badge::Slow);
            if shown {
                self.set(Badge::Connected, Line::Back, cx);
                self.after(BACK_FOR, cx, |c, cx| { let b = c.badge; c.set(b, Line::Off, cx) });
            } else {
                let l = self.line;
                self.set(Badge::Connected, if l == Line::Back { Line::Back } else { Line::Off }, cx);
            }
        } else if self.up_once {
            self.after(SHOW_AFTER, cx, |c, cx| {
                c.set(Badge::Reconnecting, Line::On, cx);
                c.after(SLOW_AFTER, cx, |c, cx| c.set(Badge::Slow, Line::On, cx));
            });
        }
    }

    /// Signed out: as at start.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.epoch += 1;
        self.up_once = false;
        self.online = false;
        self.set(Badge::Disconnected, Line::Off, cx);
    }

    /// The badge's text, tone and tooltip.
    pub fn badge_look(&self) -> (&'static str, w::PillTone, &'static str) {
        match self.badge {
            Badge::Disconnected => ("disconnected", w::PillTone::Disconnected, ""),
            Badge::Connected => ("connected", w::PillTone::Connected, ""),
            Badge::Reconnecting => ("reconnecting", w::PillTone::Reconnecting, "Reconnecting"),
            Badge::Slow => ("reconnecting", w::PillTone::Reconnecting, "Still trying. Crundi may be restarting, or the network may be down."),
        }
    }
}

struct ConnHandle(Entity<Conn>);
impl Global for ConnHandle {}

/// The connection's state (made on first use; the shell's init drives it).
pub fn conn(cx: &mut App) -> Entity<Conn> {
    if let Some(h) = cx.try_global::<ConnHandle>() { return h.0.clone(); }
    let e = cx.new(|_| Conn::default());
    cx.set_global(ConnHandle(e.clone()));
    e
}

/// For the driver: the state, if made.
pub fn conn_read(cx: &App) -> Option<&Conn> { cx.try_global::<ConnHandle>().map(|h| h.0.read(cx)) }

/// The badge: a leaf of its own (its dot blinks while reconnecting).
pub struct ConnBadge { conn: Entity<Conn>, _sub: Subscription }

impl ConnBadge {
    pub fn new(conn: Entity<Conn>, cx: &mut Context<Self>) -> Self {
        let _sub = cx.observe(&conn, |_, _, cx| cx.notify());
        ConnBadge { conn, _sub }
    }
}

impl Render for ConnBadge {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("conn-badge");
        let (label, tone, tip) = self.conn.read(cx).badge_look();
        // (pl: the top bar's gap before it, part of the measured width.)
        measured("conn", div().pl(T::SP_12).child(w::tip(w::status_pill("conn", None, label, tone), tip)))
    }
}

/// The line along the top bar's bottom edge (2 px): amber, sweeping while
/// it tries; green for a moment when it is back; nothing otherwise.
pub struct ReconnLine { conn: Entity<Conn>, _sub: Subscription }

impl ReconnLine {
    pub fn new(conn: Entity<Conn>, cx: &mut Context<Self>) -> Self {
        let _sub = cx.observe(&conn, |_, _, cx| cx.notify());
        ReconnLine { conn, _sub }
    }
}

impl Render for ReconnLine {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("reconn-line");
        let line = self.conn.read(cx).line;
        let root = div().id("reconn-line").relative().size_full().overflow_hidden();
        match line {
            Line::Off => root.into_any_element(),
            // reconnSweep: a 40% wide amber glow from -40% to 100%, 1.4 s, ease-in-out.
            Line::On => root.child(div().absolute().top_0().bottom_0().w(relative(0.4)).flex()
                .child(div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(T::yellow().opacity(0.), 0.), linear_color_stop(T::yellow(), 1.))))
                .child(div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(T::yellow(), 0.), linear_color_stop(T::yellow().opacity(0.), 1.))))
                .with_animation("reconn-sweep", Animation::new(Duration::from_millis(1400)).repeat().with_max_fps(T::ANIM_FPS).with_easing(ease_in_out),
                    |d, t| d.left(relative(-0.4 + 1.4 * t)))).into_any_element(),
            // reconnBack: opaque for half of 1.6 s, then fading out.
            Line::Back => root.child(div().absolute().inset_0().bg(T::green())
                .with_animation("reconn-back", Animation::new(Duration::from_millis(1600)).with_max_fps(T::ANIM_FPS),
                    |d, t| d.opacity(if t < 0.5 { 1.0 } else { 2.0 * (1.0 - t) }))).into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Badge, Conn, Line, BACK_FOR, SHOW_AFTER, SLOW_AFTER};
    use gpui::{AppContext as _, TestAppContext};

    #[gpui::test]
    fn a_drop_shows_after_a_moment_and_says_more_later(cx: &mut TestAppContext) {
        let c = cx.new(|_| Conn::default());
        c.update(cx, |c, cx| c.link(true, cx));
        assert_eq!(c.read_with(cx, |c, _| c.badge), Badge::Connected);
        // A blink of a drop that comes straight back shows nothing.
        c.update(cx, |c, cx| { c.link(false, cx); c.link(true, cx); });
        cx.executor().advance_clock(SHOW_AFTER * 2);
        assert_eq!(c.read_with(cx, |c, _| (c.badge, c.line)), (Badge::Connected, Line::Off));
        // A real one: still "connected" for 1.2 s, then reconnecting, then slow.
        c.update(cx, |c, cx| c.link(false, cx));
        cx.executor().advance_clock(SHOW_AFTER / 2);
        assert_eq!(c.read_with(cx, |c, _| c.badge), Badge::Connected);
        cx.executor().advance_clock(SHOW_AFTER);
        assert_eq!(c.read_with(cx, |c, _| (c.badge, c.line)), (Badge::Reconnecting, Line::On));
        cx.executor().advance_clock(SLOW_AFTER);
        assert_eq!(c.read_with(cx, |c, _| c.badge), Badge::Slow);
        // Back: green for a moment, then nothing.
        c.update(cx, |c, cx| c.link(true, cx));
        assert_eq!(c.read_with(cx, |c, _| (c.badge, c.line)), (Badge::Connected, Line::Back));
        cx.executor().advance_clock(BACK_FOR * 2);
        assert_eq!(c.read_with(cx, |c, _| c.line), Line::Off);
    }

    #[gpui::test]
    fn before_the_first_connection_it_says_disconnected(cx: &mut TestAppContext) {
        let c = cx.new(|_| Conn::default());
        c.update(cx, |c, cx| c.link(false, cx));
        cx.executor().advance_clock(SHOW_AFTER * 2);
        assert_eq!(c.read_with(cx, |c, _| c.badge), Badge::Disconnected);
    }
}
