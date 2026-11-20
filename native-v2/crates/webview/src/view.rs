//! `PageView`: one panel's page in the window (a drawer tab's). Its newest
//! frame, scaled to fit when the page has a device size (centred), the marks
//! over it (yours in the annotate colour, a chat's in the accent: boxes,
//! picked elements, pen strokes, arrows, pins, each numbered with its note),
//! the picker's hover box and the chat's cursor. The mouse, wheel and keys
//! go to the page, or to the marks while annotating (engine::pointer).
//! Until the page's first frame (the browser host may be starting) a quiet
//! spinner; a page that crashed shows so, with Reload.
//!
//! A cached leaf (ARCHITECTURE.md rule 3): it renders from its own copy of
//! what it draws (`Snap`), taken by `pump` when the panel changed, and
//! notifies itself only when that copy changed. A new frame from the
//! browser host notifies this view alone: never the drawer's bars, never
//! the window. The frame is the image the link's reader made from the
//! host's shared segment (the one copy on the app's side); GPUI uploads it
//! from there.

use crate::engine::{self, Ann};
use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};
use std::sync::Arc;

/// What the view draws of its panel (a copy, taken when the panel changed).
#[derive(Clone, PartialEq, Default)]
struct Snap {
    vw: i32, vh: i32, k: f32, ox: f32, oy: f32,
    anns: Vec<Ann>,
    hover: Option<(f32, f32, f32, f32, String)>,
    chat_at: Option<(f32, f32)>,
    cursor: i32,
    sized: bool,
    crashed: bool,
    waiting: bool,
    /// Why the page is not shown (a load that failed), over it.
    diag: String,
    /// How long the chat's cursor glides to `chat_at` (ms), and whether its button is held.
    chat_ms: f32,
    chat_down: bool,
}

impl Snap {
    fn of(i: usize) -> Snap {
        engine::with(|st| st.panels.get(i).filter(|p| !p.closed).map(|p| {
            let (vw, vh, ..) = p.view();
            let (k, ox, oy) = p.fit();
            Snap { vw, vh, k, ox, oy, anns: p.ann.clone(), hover: p.hover.clone(), chat_at: p.cursor_at.map(|c| (c.0, c.1)), cursor: p.cursor,
                sized: p.custom.is_some() || p.device != 0, crashed: p.crashed, waiting: p.browser.is_some() && p.frames == 0,
                diag: p.load_error.clone().unwrap_or_default(), chat_ms: p.cursor_ms, chat_down: p.mouse_down.is_some() }
        })).unwrap_or(Snap { k: 1., ..Snap::default() })
    }
}

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
    snap: Snap,
    /// The chat's cursor as drawn: where it glides from, where it was last
    /// (to fade out there), and a count that restarts its animation.
    cur_from: Option<(f32, f32)>,
    cur_last: Option<(f32, f32)>,
    cur_seq: u64,
    /// It was not on show before this move: it fades in.
    cur_in: bool,
}

impl PageView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        PageView { panel: None, annotating: false, image: None, shown: None, stage: (0., 0.), origin: Point::default(), focus: cx.focus_handle(), snap: Snap { k: 1., ..Snap::default() }, cur_from: None, cur_last: None, cur_seq: 0, cur_in: false }
    }

    /// Show another panel (or none).
    pub fn set_panel(&mut self, panel: Option<usize>, cx: &mut Context<Self>) {
        if self.panel == panel { return; }
        // The panel no longer shown is hidden in the host (no frames), the
        // new one shown.
        if let Some(old) = self.panel { engine::set_shown(old, false); }
        if let Some(new) = panel { engine::set_shown(new, true); }
        self.panel = panel;
        self.shown = None;
        if let Some(i) = panel { if self.stage.0 > 0. { engine::resized(i, self.stage.0, self.stage.1); } }
        cx.notify();
    }

    pub fn focus_handle(&self) -> &FocusHandle { &self.focus }

    /// Draw marks as the drawer annotates (the pointer makes marks then).
    pub fn set_annotating(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.annotating != on { self.annotating = on; cx.notify(); }
    }

    /// After a pump: the panel's new frame becomes the image, and what is
    /// drawn over it is copied; a notify only when either moved.
    pub fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.panel else {
            if let Some(old) = self.image.take() { let _ = window.drop_image(old); cx.notify(); }
            return;
        };
        let snap = Snap::of(i);
        if snap != self.snap {
            // The chat's cursor moved, came or went: it glides from where it
            // was drawn (VD placeCursor), fades in and out.
            if snap.chat_at != self.snap.chat_at {
                self.cur_in = self.snap.chat_at.is_none();
                self.cur_from = self.snap.chat_at.or(self.cur_last);
                if snap.chat_at.is_some() { self.cur_last = snap.chat_at; }
                self.cur_seq += 1;
            }
            self.snap = snap;
            cx.notify();
        }
        // The panel's slot made again under this view (closed and reused:
        // a network switch): it is shown, as it is drawn here.
        if !engine::is_shown(i) { engine::set_shown(i, true); }
        let (n, img) = engine::with(|st| st.panels.get(i).map(|p| (p.frames, p.frame.clone())).unwrap_or((0, None)));
        if self.shown == Some((i, n)) { return; }
        self.shown = Some((i, n));
        // The panel's image itself (made from the segment by the link's
        // reader): the one before it leaves GPUI's atlas.
        if let Some(old) = std::mem::replace(&mut self.image, img) { if self.image.as_ref().is_none_or(|n| n.id != old.id) { let _ = window.drop_image(old); } }
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

/// The chat's cursor's frames a second while it glides or fades (rule 6).
const CURSOR_FPS: f32 = 60.;

/// The glide's ease (the web's cubic-bezier(.3,.7,.3,1): fast, then settling).
fn ease_glide(t: f32) -> f32 { 1. - (1. - t).powi(3) }

/// The chat's cursor: the pointer, its "Claude" tag, a ring while pressed.
fn cursor_el(down: bool) -> Div {
    div().absolute().size(T::PAGE_CURSOR)
        .when(down, |d| d.child(div().absolute().left(-T::MARK_R).top(-T::MARK_R).size(T::MARK_D).rounded(T::R_PILL).border_2().border_color(T::accent()).bg(T::accent().opacity(0.28))))
        .child(svg().absolute().left(px(-2.)).top(px(-2.)).path(crundi_theme::IconName::MousePointer2.path()).size(T::PAGE_CURSOR).text_color(T::accent()))
        .child(div().absolute().left(px(15.)).top(px(21.)).px(T::SP_6).rounded(T::R_PILL).bg(T::accent()).text_color(T::white()).text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).whitespace_nowrap().child("Claude"))
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
        crundi_core::driver::rendered("page");
        let me = cx.entity();
        // What is drawn (the copy `pump` took): the page's place, the marks, the hover, the chat's cursor.
        let Snap { vw, vh, k, ox, oy, anns, hover, chat_at, cursor, sized, crashed, waiting, diag, chat_ms, chat_down } = if self.panel.is_some() { self.snap.clone() } else { Snap { k: 1., ..Snap::default() } };
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
                    // (.brd-hov: the annotate colour, a 12% fill.)
                    let b = Bounds { origin: pt(*x, *y), size: size(px((w * k).max(2.)), px((h * k).max(2.))) };
                    window.paint_quad(quad(b, px(2.), T::annotate().opacity(0.12), px(2.), T::annotate(), BorderStyle::Solid));
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
        // The hovered element's selector, on a tag above its box (.brd-hov span).
        if let Some((x, y, _, _, sel)) = hover.as_ref().filter(|h| !h.4.is_empty()) {
            let p = at(*x, *y);
            d = d.child(div().absolute().left(p.x - px(2.)).top(p.y - px(22.)).max_w(px(320.)).px(T::SP_6).py(px(1.)).rounded(T::R_CHIP)
                .bg(T::annotate()).text_color(T::white()).text_size(T::TEXT_11).font_family(T::MONO).truncate().child(sel.clone()));
        }
        // Each mark's label (.brd-lbl): its number (and "Claude"), its note,
        // a picked element's selector under it, at the mark's corner.
        for (n, a) in anns.iter().enumerate() {
            let c = mark_color(a);
            let (cx0, cy0) = match a.kind.as_str() {
                "pen" => a.points.first().copied().unwrap_or((a.x, a.y)),
                _ => (a.x, a.y),
            };
            let p = at(cx0, cy0);
            let who = if a.by == "claude" { format!("{} Claude", n + 1) } else { format!("{}", n + 1) };
            d = d.child(div().absolute().left(p.x).top(p.y).max_w(T::DRAWER_NOTE_W).px(T::SP_6).py(px(3.)).rounded(T::R_CONTROL).bg(c)
                .text_color(T::white()).text_size(T::TEXT_12)
                .child(div().flex().gap(T::SP_4).child(div().flex_none().font_weight(T::W_BOLD).opacity(0.85).child(who))
                    .when(!a.text.is_empty(), |d| d.child(div().min_w_0().child(a.text.clone()))))
                .when(a.kind == "element" && !a.selector.is_empty(), |d| d.child(div().font_family(T::MONO).text_size(T::TEXT_10).opacity(0.85).child(a.selector.clone()))));
        }
        // The chat's cursor (browser_mouse) with its tag: it glides to where
        // it points (as long as the move takes), a ring while its button is
        // held, and fades in and out (VD placeCursor).
        let seq = self.cur_seq;
        if let Some((x, y)) = chat_at {
            let from = self.cur_from.unwrap_or((x, y));
            let fade_in = self.cur_in;
            let (a0, a1) = (at(from.0, from.1), at(x, y));
            let ms = chat_ms.max(200.);
            // (The glide takes chat_ms of it; a jump is there at once, the fade 0.2 s.)
            let glide = chat_ms;
            d = d.child(cursor_el(chat_down).with_animation(("chat-cur", seq as usize), Animation::new(std::time::Duration::from_millis(ms as u64)).with_max_fps(CURSOR_FPS),
                move |el, t| {
                    let g = if glide <= 0. { 1. } else { ease_glide((t * ms / glide).min(1.)) };
                    el.left(a0.x + (a1.x - a0.x) * g).top(a0.y + (a1.y - a0.y) * g).opacity(if fade_in { (t * ms / 200.).min(1.) } else { 1. })
                }));
        } else if let Some((x, y)) = self.cur_last.filter(|_| seq > 0) {
            let p = at(x, y);
            d = d.child(cursor_el(false).left(p.x).top(p.y).with_animation(("chat-cur", seq as usize), Animation::new(std::time::Duration::from_millis(200)).with_max_fps(CURSOR_FPS),
                move |el, t| el.opacity(1. - t)));
        }
        // The browser host starting, or the page not drawn yet: a quiet
        // spinner; its page process or the whole host gone, or a load failed: the line saying so.
        if self.panel.is_some() {
            // (.brd-diag: one centred muted line over the page; Reload is the bar's.)
            if crashed {
                d = d.child(crundi_widgets::page_message("The page crashed. Reload to try again."));
            } else if !diag.is_empty() {
                d = d.child(crundi_widgets::page_message(diag));
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
