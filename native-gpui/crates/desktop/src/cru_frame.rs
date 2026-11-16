//! The live screen's frame in Cru's view: the page (webview::frames, key
//! cru_screen::FRAME) made at the box's size and kept to it, its newest
//! picture drawn, and the mouse, wheel and keys passed to it (a click gives
//! it the keyboard).

use crate::cru_screen::{self, FRAME};
use crundi_theme::T;
use crundi_webview::frames;
use gpui::{prelude::FluentBuilder as _, *};

pub struct CruFrame { focus: FocusHandle, origin: Point<Pixels>, size: (f32, f32), seen: u64 }

impl CruFrame {
    pub fn new(cx: &mut Context<Self>) -> Self { CruFrame { focus: cx.focus_handle(), origin: Point::default(), size: (0., 0.), seen: 0 } }

    /// The page moved (painted, loaded): draw again.
    pub fn pump(&mut self, cx: &mut Context<Self>) {
        let g = frames::generation();
        if g != self.seen { self.seen = g; cx.notify(); }
    }

    fn at(&self, p: Point<Pixels>) -> (f32, f32) { let q = p - self.origin; (f32::from(q.x), f32::from(q.y)) }
}

impl Render for CruFrame {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity();
        let target = cru_screen::with(|s| s.target.clone());
        let img = frames::image(FRAME);
        div().id("cru-frame").relative().size_full().overflow_hidden().rounded(T::R_FIELD).bg(T::white())
            .track_focus(&self.focus)
            .cursor(frames::cursor(FRAME))
            .when_some(img, |d, i| d.child(gpui::img(ImageSource::Render(i)).absolute().inset_0().size_full()))
            .child(canvas(move |bounds, window, cx| {
                let scale = window.scale_factor();
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                me.update(cx, |this, _| {
                    this.origin = bounds.origin;
                    if target.is_empty() || w < 1. || h < 1. { return; }
                    if !frames::exists(FRAME) || (this.size.0 - w).abs() > 0.5 || (this.size.1 - h).abs() > 0.5 {
                        this.size = (w, h);
                        frames::open(FRAME, &target, w, h, scale);
                    }
                });
            }, |_, _, _, _| {}).absolute().inset_0().size_full())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| { window.focus(&this.focus, cx); let (x, y) = this.at(e.position); frames::pointer(FRAME, 1, x, y, e.button, e.click_count); }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, _, _| { let (x, y) = this.at(e.position); frames::pointer(FRAME, 2, x, y, e.button, e.click_count); }))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, _| { let (x, y) = this.at(e.position); frames::pointer(FRAME, 0, x, y, e.pressed_button.unwrap_or(MouseButton::Left), 1); }))
            .on_scroll(cx.listener(|this, e: &ScrollEvent, window, _| {
                let (x, y) = this.at(e.position);
                let d = e.delta.pixel_delta(window.line_height());
                frames::wheel(FRAME, x, y, f32::from(d.x), f32::from(d.y));
            }))
            .on_key_down(|e: &KeyDownEvent, _, _| frames::key(FRAME, &e.keystroke))
    }
}
