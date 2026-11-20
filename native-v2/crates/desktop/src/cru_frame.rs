//! The live screen's frame in Cru's view: the page (webview::frames, key
//! cru_screen::FRAME) made at the box's size and kept to it, its newest
//! picture drawn, and the mouse, wheel and keys passed to it (a click gives
//! it the keyboard).
//!
//! It draws from its own copy (rule 1): the page's target, look and newest
//! picture, taken by `pump` and compared; a new frame of the page notifies
//! this view (and so Cru's screen around it), nothing else.

use crate::cru_screen::{self, FRAME};
use crundi_theme::T;
use crundi_webview::frames;
use gpui::{prelude::FluentBuilder as _, *};
use std::sync::Arc;

pub struct CruFrame {
    focus: FocusHandle,
    origin: Point<Pixels>,
    size: (f32, f32),
    target: String,
    look: Option<frames::Look>,
    image: Option<Arc<RenderImage>>,
}

impl CruFrame {
    pub fn new(cx: &mut Context<Self>) -> Self { CruFrame { focus: cx.focus_handle(), origin: Point::default(), size: (0., 0.), target: String::new(), look: None, image: None } }

    /// The page moved (painted, loaded, crashed) or another page: draw again.
    pub fn pump(&mut self, cx: &mut Context<Self>) {
        let target = cru_screen::with(|s| s.target.clone());
        let look = frames::look(FRAME);
        if target != self.target || look != self.look {
            if look.map(|l| l.frames) != self.look.map(|l| l.frames) || target != self.target { self.image = frames::image(FRAME); }
            if target != self.target { self.size = (0., 0.); }
            self.target = target;
            self.look = look;
            cx.notify();
        }
    }

    fn at(&self, p: Point<Pixels>) -> (f32, f32) { let q = p - self.origin; (f32::from(q.x), f32::from(q.y)) }
}

impl Render for CruFrame {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("cru-frame");
        let me = cx.entity();
        let target = self.target.clone();
        let look = self.look;
        let over = look.and_then(|l| if l.crashed { Some(frames::crash_overlay(FRAME)) } else { l.waiting.then(|| w_loading()) });
        div().id("cru-frame").relative().size_full().overflow_hidden().rounded(T::R_FIELD).bg(T::white())
            .track_focus(&self.focus)
            .cursor(frames::cursor_of(look.map(|l| l.cursor).unwrap_or(0)))
            .when_some(self.image.clone(), |d, i| d.child(gpui::img(ImageSource::Render(i)).absolute().inset_0().size_full()))
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
            // Until its first frame a quiet spinner; crashed: the words and Reload.
            .children(over)
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

fn w_loading() -> AnyElement { crundi_widgets::page_loading().into_any_element() }
