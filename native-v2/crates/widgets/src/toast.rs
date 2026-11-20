//! The toast (the web's `toast(msg, type)` and its `.toast` CSS): one line
//! or a few of plain text, no icon, in a card just under the top bar on the
//! right (top: the bar + 10 px, right: 16 px, at most 420 px wide), padded
//! 12 × 20, a hairline border and a 3 px edge on the left in the accent (an
//! error's red, a success's green; the border takes the same colour), the
//! web's shadow. It slides down 10 px and fades in over 0.2 s, shows for
//! 3.5 s, and fades out. One at a time: a new one replaces the one on show.
//! Drawn once, over the window, by the ask layer (the shell draws that).

use crundi_theme::T;
use gpui::*;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Toast { Info, Error, Success }

const SHOW: Duration = Duration::from_millis(3500);
const FADE: Duration = Duration::from_millis(200);

struct On { text: SharedString, kind: Toast, n: u64, leaving: bool }

/// The toast's layer: the one on show.
#[derive(Default)]
pub struct ToastLayer { on: Option<On>, n: u64 }

struct Host(Entity<ToastLayer>);
impl Global for Host {}

/// The window's toast layer (made the first time).
pub fn toast_layer(cx: &mut App) -> Entity<ToastLayer> {
    if let Some(h) = cx.try_global::<Host>() { return h.0.clone(); }
    let e = cx.new(|_| ToastLayer::default());
    cx.set_global(Host(e.clone()));
    e
}

/// Show `text` (replacing the toast on show).
pub fn show(cx: &mut App, text: SharedString, kind: Toast) {
    let l = toast_layer(cx);
    l.update(cx, |l, cx| {
        l.n += 1;
        let n = l.n;
        l.on = Some(On { text, kind, n, leaving: false });
        cx.notify();
        cx.spawn(async move |me, cx| {
            cx.background_executor().timer(SHOW).await;
            let _ = me.update(cx, |l, cx| if let Some(o) = l.on.as_mut().filter(|o| o.n == n) { o.leaving = true; cx.notify(); });
            cx.background_executor().timer(FADE).await;
            let _ = me.update(cx, |l, cx| if l.on.as_ref().is_some_and(|o| o.n == n) { l.on = None; cx.notify(); });
        }).detach();
    });
}

/// Take it down at once.
pub fn clear(cx: &mut App) {
    if let Some(h) = cx.try_global::<Host>().map(|h| h.0.clone()) { h.update(cx, |l, cx| if l.on.take().is_some() { cx.notify(); }); }
}

impl Render for ToastLayer {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(o) = self.on.as_ref() else { return div().into_any_element() };
        let vp = window.viewport_size();
        let (edge, border) = match o.kind { Toast::Info => (T::accent(), T::border()), Toast::Error => (T::red(), T::red()), Toast::Success => (T::green(), T::green()) };
        let max_w = T::TOAST_MAX_W.min(vp.width - T::SP_16 * 2.);
        let card = div().id(("toast", o.n as usize)).occlude().max_w(max_w)
            .bg(T::bg_secondary()).border_1().border_l(px(3.)).border_color(border).rounded(T::R_CONTROL)
            .shadow(vec![BoxShadow { color: hsla(0., 0., 0., 0.5), offset: point(px(0.), px(6.)), blur_radius: px(20.), spread_radius: px(0.), inset: false }])
            .px(T::SP_20).py(T::SP_12).text_size(T::TOAST_TEXT).line_height(relative(T::LH_NORMAL)).text_color(T::text())
            .child(o.text.clone());
        // The left edge in its own colour: drawn as a strip over the border.
        let card = div().relative().child(card).child(div().absolute().top_0().bottom_0().left_0().w(px(3.)).rounded_l(T::R_CONTROL).bg(edge));
        let leaving = o.leaving;
        let anim = Animation::new(FADE).with_max_fps(T::ANIM_FPS);
        let key = ("toast-in", (o.n * 2 + leaving as u64) as usize);
        let moving = card.with_animation(key, anim, move |d, t| {
            let k = if leaving { 1. - t } else { t };
            d.opacity(k).mt(px(-10.) * (1. - k))
        });
        deferred(anchored().position(point(vp.width - T::SP_16, T::TOPBAR_H + T::SP_10)).anchor(Anchor::TopRight).child(moving))
            .with_priority(3).into_any_element()
    }
}
