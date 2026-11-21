//! 2.16: the rest of the log (the web's paint()): the user's bubble (with
//! "Owner" / "Crundi" over it when the owner or Crundi wrote it), errors,
//! notices, a turn's end (its time and cost, or the failed turn's box); the
//! streaming caret under a reply that is still coming (`.cc-cursor`); and
//! "Go to the newest message" (`.cc-jump`: shown once the reader is a third
//! of the log up, at least 240 px; a dot when something arrived below).

use super::{text::{caret_room, Look, CARET_H, CARET_W}, LogView};
use crate::store::Row;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use std::time::Duration;

/// A red box (an error, a failed turn's end).
pub fn error_box(fs: f32) -> Div {
    div().w_full().px(T::SP_10).py(T::SP_6).rounded(T::R_CONTROL).border_1().border_color(T::red()).bg(T::red_dim())
        .text_color(T::red()).text_size(T::TEXT_12 * fs)
}

/// The user's bubble; its links keep its colour (the web's `.cc-user-body a`).
pub fn bubble(v: &LogView, ix: usize, r: &Row, fs: f32) -> AnyElement {
    let inl = r.plain.clone().unwrap_or_default();
    let by = match r.shown.title.as_str() { "owner" => Some("Owner"), "system" => Some("Crundi"), _ => None };
    let body = div().max_w(relative(0.82)).px(T::SP_12).py(T::SP_8)
        .rounded_tl(T::R_BUBBLE).rounded_tr(T::R_BUBBLE).rounded_bl(T::R_BUBBLE).rounded_br(T::R_CHIP)
        .bg(T::accent_dim()).border_1().border_color(T::accent_edge())
        .text_size(T::TEXT_13 * fs).line_height(relative(T::LH_CHAT))
        .child(v.text(ix, 0, r, &inl, Look { color: T::text(), mono: false, bold: false, link: None }));
    match by {
        Some(b) => div().flex().flex_col().items_end()
            .child(div().mr(T::SP_4).mb(T::SP_2).text_size(T::TEXT_11 * fs).font_weight(T::W_SEMIBOLD).text_color(T::sky()).child(b))
            .child(body).into_any_element(),
        None => div().flex().justify_end().child(body).into_any_element(),
    }
}

/// An error's box, a notice's line (centred, muted italics).
pub fn plain(v: &LogView, ix: usize, r: &Row, fs: f32) -> AnyElement {
    let inl = r.plain.clone().unwrap_or_default();
    let notice = r.shown.kind == "notice";
    let t = v.text(ix, 0, r, &inl, Look { color: if notice { T::text_muted() } else { T::red() }, mono: false, bold: false, link: None });
    if notice { div().w_full().flex().justify_center().text_size(T::TEXT_12 * fs).italic().child(t).into_any_element() } else { error_box(fs).child(t).into_any_element() }
}

/// A turn's end: a hairline, its time and cost centred; a failed turn: the box.
pub fn result(r: &Row, fs: f32) -> AnyElement {
    let e = &r.shown;
    if e.error { return error_box(fs).child(if e.text.is_empty() { SharedString::from("Error") } else { r.text.clone() }).into_any_element(); }
    // (.cc-result: a dashed hairline over it.)
    div().w_full().border_t_1().border_dashed().border_color(T::border_subtle())
        .py(px(3.)).text_center().text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(e.detail.clone())
        .into_any_element()
}

// ─── The streaming caret ───

/// The caret's blink: lit or not this half second, and the timer that
/// turns it (running only while a caret was drawn: nothing ticks otherwise).
#[derive(Default)]
pub struct Blink { pub on: bool, drawn: bool, task: Option<Task<()>> }

/// The caret under this row (the reply's last block while it streams): its
/// gap under the text and whether it is lit.
pub fn caret_of(v: &mut LogView, r: &Row, _cx: &App) -> Option<(Pixels, bool)> {
    let b = r.block.as_ref()?;
    if !b.last || r.shown.status != "streaming" { return None; }
    v.ui.blink.drawn = true;
    let fs = v.fs();
    Some((px(caret_room(&b.kind, fs) - 13. * fs * T::LH_CHAT), v.ui.blink.on))
}

/// The caret as a line of its own (under a code block or a table).
pub fn caret_line(c: (Pixels, bool), fs: f32) -> AnyElement {
    div().pt(c.0).h(c.0 + px(13. * fs * T::LH_CHAT)).flex().items_center()
        .child(div().w(px(CARET_W)).h(px(CARET_H)).when(c.1, |d| d.bg(T::accent())))
        .into_any_element()
}

/// After a render: while a caret is drawn, it turns every half second (the
/// web's 1 s, two steps); with none drawn, the timer stops.
pub fn blink_after(v: &mut LogView, cx: &mut Context<LogView>) {
    let drawn = std::mem::take(&mut v.ui.blink.drawn);
    if !drawn { v.ui.blink.task = None; v.ui.blink.on = true; return; }
    if v.ui.blink.task.is_some() { return; }
    v.ui.blink.task = Some(cx.spawn(async move |this: WeakEntity<LogView>, cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_millis(500)).await;
        let _ = this.update(cx, |v, cx| {
            v.ui.blink.task = None;
            v.ui.blink.on = !v.ui.blink.on;
            // (The caret is painted, not laid out: its rows keep their sizes;
            // under the streaming tail it is that view's alone: tail.rs.)
            if !super::tail::redraw(v, cx) { cx.notify(); }
        });
    }));
}

// ─── Following the end, and "Go to the newest message" ───

/// The jump button's state: the entries seen, and whether something
/// arrived below while the reader was up the log (its dot).
#[derive(Default)]
pub struct Jump { pub seen: usize, pub new: bool }

/// How far the log is from its end (px).
pub fn dist(v: &LogView) -> f32 {
    let max = f32::from(v.list.max_offset_for_scrollbar().y);
    let off = -f32::from(v.list.scroll_px_offset_for_scrollbar().y);
    (max - off).max(0.)
}

/// The button shows beyond max(240, a third of the log's height).
pub fn shown(dist: f32, view_h: f32) -> bool { dist > (view_h / 3.).max(240.) }

/// The store changed: a new entry while the reader is up the log (more
/// than 60 px from the end) lights the dot (the web's noteBelow).
pub fn arrived(v: &mut LogView, cx: &App) {
    let n = v.store.read(cx).conv.entries.len();
    if n > v.ui.jump.seen && !v.list.is_following_tail() && dist(v) > 60. { v.ui.jump.new = true; }
    v.ui.jump.seen = n;
}

/// Back to the end, following it again.
pub fn go_newest(v: &mut LogView, cx: &mut Context<LogView>) {
    v.ui.jump.new = false;
    v.list.set_follow_mode(FollowMode::Tail);
    cx.notify();
}

/// The button, over the log's bottom centre (None: near the end).
pub fn jump(v: &mut LogView, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let d = dist(v);
    if d < 60. { v.ui.jump.new = false; }
    if !shown(d, f32::from(v.list.viewport_bounds().size.height)) { return None; }
    let new = v.ui.jump.new;
    Some(div().absolute().bottom(T::SP_10).left_0().right_0().flex().justify_center()
        .child(w::log_jump("log-jump", new)
            .tooltip(w::tooltip("Go to the newest message"))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|v, _, _, cx| go_newest(v, cx))))
        .into_any_element())
}

#[cfg(test)]
mod tests {
    #[::core::prelude::v1::test]
    fn the_jump_button_shows_past_a_third() {
        assert!(!super::shown(240., 600.));
        assert!(super::shown(241., 600.));
        assert!(!super::shown(300., 1200.));
        assert!(super::shown(401., 1200.));
    }
}
