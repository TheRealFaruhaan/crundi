//! `PageView`: one panel's page in the window (a drawer tab's). Its newest
//! frame, scaled to fit when the page has a device size (centred), the marks
//! over it (yours in the annotate colour, a chat's in the accent: boxes,
//! picked elements, pen strokes, arrows, pins, each numbered with its note),
//! the picker's hover box and the chat's cursor. The mouse, wheel and keys
//! go to the page, or to the marks while annotating (engine::pointer).
//! Until the page's first frame (the browser host may be starting) a quiet
//! spinner; a page that crashed shows so, with Reload.

use crate::engine::{self, Ann};
use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};
use std::sync::Arc;

pub struct PageView {
    /// The panel shown (engine's index); None: nothing yet.
    pub panel: Option<usize>,
    /// The drawer is annotating: the pointer makes marks.
    pub annotating: bool,
    image: Option<Arc<RenderImage>>,
    shown: Option<(usize, u64)>,
    stage: (f32, f32),
    origin: Point<Pixels>,
    focus: FocusHandle,
}

impl PageView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        PageView { panel: None, annotating: false, image: None, shown: None, stage: (0., 0.), origin: Point::default(), focus: cx.focus_handle() }
    }

    /// Show another panel (or none).
    pub fn set_panel(&mut self, panel: Option<usize>, cx: &mut Context<Self>) {
        if self.panel == panel { return; }
        self.panel = panel;
        self.shown = None;
        if let Some(i) = panel { if self.stage.0 > 0. { engine::resized(i, self.stage.0, self.stage.1); } }
        cx.notify();
    }

    pub fn focus_handle(&self) -> &FocusHandle { &self.focus }

    /// After a pump: the panel's new frame becomes the image.
    pub fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.panel else {
            if let Some(old) = self.image.take() { let _ = window.drop_image(old); cx.notify(); }
            return;
        };
        let (n, frame) = engine::with(|st| st.panels.get(i).map(|p| (p.frames, p.frame.clone())).unwrap_or((0, None)));
        if self.shown == Some((i, n)) { return; }
        self.shown = Some((i, n));
        let img = frame.and_then(|f| image::RgbaImage::from_raw(f.1, f.2, f.0.clone())).map(|b| Arc::new(RenderImage::new([image::Frame::new(b)])));
        if let Some(old) = std::mem::replace(&mut self.image, img) { let _ = window.drop_image(old); }
        cx.notify();
    }

    fn at(&self, p: Point<Pixels>) -> (f32, f32) {
        let q = p - self.origin;
        (f32::from(q.x), f32::from(q.y))
    }

    fn pointer(&mut self, kind: i32, p: Point<Pixels>, b: MouseButton) {
        let Some(i) = self.panel else { return };
        let (x, y) = self.at(p);
        let b = match b { MouseButton::Middle => 1, MouseButton::Right => 2, _ => 0 };
        let ann = self.annotating;
        engine::defer(move || engine::pointer(i, kind, x, y, b, ann));
    }

    fn key(&mut self, ks: &Keystroke, down: bool) {
        let Some(i) = self.panel else { return };
        let named = match ks.key.as_str() {
            "enter" => Some('\r'), "backspace" => Some('\u{8}'), "tab" => Some('\t'), "escape" => Some('\u{1b}'), "delete" => Some('\u{7f}'),
            "up" => Some('\u{F700}'), "down" => Some('\u{F701}'), "left" => Some('\u{F702}'), "right" => Some('\u{F703}'),
            "home" => Some('\u{F729}'), "end" => Some('\u{F72B}'), "pageup" => Some('\u{F72C}'), "pagedown" => Some('\u{F72D}'),
            _ => None,
        };
        let text = match named {
            Some(c) => c.to_string(),
            None => match &ks.key_char { Some(c) => c.clone(), None if ks.key.chars().count() == 1 => ks.key.clone(), None => return },
        };
        let (ctrl, shift) = (ks.modifiers.control || ks.modifiers.platform, ks.modifiers.shift);
        engine::defer(move || engine::key(i, &text, down, ctrl, shift));
    }
}

/// A mark's colour: a chat's in the accent, yours in the annotate colour.
fn mark_color(a: &Ann) -> Hsla { if a.by == "claude" { T::accent() } else { T::annotate() } }

pub(crate) fn cursor_style(kind: i32) -> CursorStyle {
    match kind {
        1 => CursorStyle::PointingHand,
        2 => CursorStyle::IBeam,
        3 => CursorStyle::Crosshair,
        6 => CursorStyle::OperationNotAllowed,
        7 => CursorStyle::ResizeLeftRight,
        8 => CursorStyle::ResizeUpDown,
        9 => CursorStyle::ResizeUpRightDownLeft,
        10 => CursorStyle::ResizeUpLeftDownRight,
        11 => CursorStyle::OpenHand,
        12 => CursorStyle::ClosedHand,
        _ => CursorStyle::Arrow,
    }
}

impl Render for PageView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity();
        // What is drawn, from the panel: the page's place, the marks, the hover, the chat's cursor.
        let snap = self.panel.and_then(|i| engine::with(|st| st.panels.get(i).map(|p| {
            let (vw, vh, ..) = p.view();
            let (k, ox, oy) = p.fit();
            (vw, vh, k, ox, oy, p.ann.clone(), p.hover.clone(), p.cursor_at.map(|c| (c.0, c.1)), p.cursor, p.custom.is_some() || p.device != 0)
        })));
        let (vw, vh, k, ox, oy, anns, hover, chat_at, cursor, sized) = snap.unwrap_or((0, 0, 1., 0., 0., vec![], None, None, 0, false));
        let at = move |x: f32, y: f32| point(px(x * k + ox), px(y * k + oy));
        let mut d = div()
            .id("page").relative().size_full().overflow_hidden()
            .bg(if sized { T::bg() } else { T::white() })
            .track_focus(&self.focus)
            .cursor(if self.annotating { CursorStyle::Crosshair } else { cursor_style(cursor) })
            .when_some(self.image.clone(), |d, img| d.child(gpui::img(ImageSource::Render(img)).absolute()
                .left(px(ox)).top(px(oy)).w(px(vw as f32 * k)).h(px(vh as f32 * k))));
        // The marks' shapes, the hover, and the stage's size (for the page).
        let (a2, h2) = (anns.clone(), hover.clone());
        d = d.child(canvas(
            move |bounds, _, cx| {
                me.update(cx, |this, _| {
                    this.origin = bounds.origin;
                    let s = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    if (s.0 - this.stage.0).abs() > 0.5 || (s.1 - this.stage.1).abs() > 0.5 {
                        this.stage = s;
                        if let Some(i) = this.panel { engine::defer(move || engine::resized(i, s.0, s.1)); }
                    }
                });
                bounds
            },
            move |bounds, _, window, _| {
                let o = bounds.origin;
                let pt = |x: f32, y: f32| o + at(x, y);
                if let Some((x, y, w, h, _)) = &h2 {
                    let b = Bounds { origin: pt(*x, *y), size: size(px(w * k), px(h * k)) };
                    window.paint_quad(quad(b, px(0.), T::accent_dim(), px(2.), T::accent(), BorderStyle::Solid));
                }
                for a in &a2 {
                    let c = mark_color(a);
                    match a.kind.as_str() {
                        "box" | "element" => {
                            let b = Bounds { origin: pt(a.x, a.y), size: size(px(a.w * k), px(a.h * k)) };
                            let style = if a.kind == "element" { BorderStyle::Dashed } else { BorderStyle::Solid };
                            window.paint_quad(quad(b, px(0.), transparent_black(), T::SP_2, c, style));
                        }
                        "pen" if a.points.len() > 1 => {
                            let mut pb = PathBuilder::stroke(T::ANNOTATE_STROKE);
                            pb.move_to(pt(a.points[0].0, a.points[0].1));
                            for p in &a.points[1..] { pb.line_to(pt(p.0, p.1)); }
                            if let Ok(path) = pb.build() { window.paint_path(path, c); }
                        }
                        "arrow" => {
                            let (from, to) = (pt(a.x, a.y), pt(a.to.0, a.to.1));
                            let mut pb = PathBuilder::stroke(T::ANNOTATE_STROKE);
                            pb.move_to(from);
                            pb.line_to(to);
                            // The head: two strokes back from the point.
                            let ang = (f32::from(to.y - from.y)).atan2(f32::from(to.x - from.x));
                            let hl = f32::from(T::SP_12);
                            for dlt in [-T::ARROW_SPREAD, T::ARROW_SPREAD] {
                                pb.move_to(to);
                                pb.line_to(to - point(px(hl * (ang + dlt).cos()), px(hl * (ang + dlt).sin())));
                            }
                            if let Ok(path) = pb.build() { window.paint_path(path, c); }
                        }
                        _ => {}
                    }
                }
            },
        ).absolute().inset_0().size_full());
        // The numbers and notes (text, so elements): a disc on a pin, else at the mark's corner.
        for (n, a) in anns.iter().enumerate() {
            let c = mark_color(a);
            let r = T::MARK_R;
            let (cx0, cy0) = match a.kind.as_str() {
                "pin" => (a.x, a.y),
                "arrow" => (a.x, a.y),
                "pen" => a.points.first().copied().unwrap_or((a.x, a.y)),
                _ => (a.x, a.y),
            };
            let p = at(cx0, cy0);
            let (left, top) = if a.kind == "pin" { (p.x - r, p.y - r) } else { (p.x, p.y) };
            let label = if a.by == "claude" { format!("{} Claude", n + 1) } else { format!("{}", n + 1) };
            d = d.child(div().absolute().left(left).top(top).flex().items_center().gap(T::SP_4)
                .child(div().h(T::MARK_D).min_w(T::MARK_D).px(T::SP_4).rounded(T::R_PILL).bg(c).border_1().border_color(T::white())
                    .flex().items_center().justify_center().text_color(T::white()).text_size(T::TEXT_11).font_weight(T::W_BOLD).child(label))
                .when(!a.text.is_empty(), |d| d.child(div().max_w(T::DRAWER_NOTE_W).px(T::SP_6).py(T::SP_2).rounded(T::R_CHIP).bg(c)
                    .text_color(T::white()).text_size(T::TEXT_11).truncate().child(a.text.clone()))));
        }
        // The chat's cursor (browser_mouse), with its tag.
        if let Some((x, y)) = chat_at {
            let p = at(x, y);
            d = d.child(div().absolute().left(p.x).top(p.y).flex().flex_col().gap(T::SP_2)
                .child(svg().path(crundi_theme::IconName::MousePointer2.path()).size(T::PAGE_CURSOR).text_color(T::accent()))
                .child(div().px(T::SP_4).rounded(T::R_CHIP).bg(T::accent()).text_color(T::white()).text_size(T::TEXT_10).font_weight(T::W_BOLD).child("Claude")));
        }
        // The browser host starting, or the page not drawn yet: a quiet
        // spinner; its page process or the whole host gone: the crash, Reload.
        if let Some(i) = self.panel {
            let (crashed, waiting) = engine::with(|st| st.panels.get(i).filter(|p| !p.closed).map(|p| (p.crashed, p.browser.is_some() && p.frames == 0)).unwrap_or_default());
            if crashed {
                d = d.child(crundi_widgets::page_crashed("page-reload", move |_, _, _| engine::defer(move || engine::reload(i))));
            } else if waiting {
                d = d.child(crundi_widgets::page_loading());
            }
        }
        d.on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| { window.focus(&this.focus, cx); this.pointer(1, e.position, e.button) }))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, e: &MouseDownEvent, _, _| this.pointer(1, e.position, e.button)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, _, _| this.pointer(2, e.position, e.button)))
            .on_mouse_up(MouseButton::Right, cx.listener(|this, e: &MouseUpEvent, _, _| this.pointer(2, e.position, e.button)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, _| this.pointer(0, e.position, e.pressed_button.unwrap_or(MouseButton::Left))))
            .on_scroll(cx.listener(|this, e: &ScrollEvent, window, _| {
                let Some(i) = this.panel else { return };
                let (x, y) = this.at(e.position);
                let dl = e.delta.pixel_delta(window.line_height());
                let (dx, dy) = (f32::from(dl.x), f32::from(dl.y));
                engine::defer(move || engine::wheel(i, x, y, dx, dy));
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, _| this.key(&e.keystroke, true)))
            .on_key_up(cx.listener(|this, e: &KeyUpEvent, _, _| this.key(&e.keystroke, false)))
    }
}
