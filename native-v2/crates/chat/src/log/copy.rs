//! The copy button on code and links (the web's `.cc-copyhint`): hovering a
//! code block, a code span or a link in Claude's reply (or a link in the
//! user's bubble) shows one small button by it: a code block's in its top
//! right corner, a span's just after its end. Its tooltip says what it
//! copies ("Copy code", "Copy link", "Copy path"); a click copies (the toast
//! "Copied") and turns it into a tick. It goes 250 ms after the pointer
//! leaves (not while over the button), and at once when the log scrolls.
//!
//! And the link menu (webapp-html.js's openLinkMenu): a right-click on a
//! web link gives "Open in browser" and "Copy link" ("Link copied").

use super::{copy_span, LogView};
use crundi_core::webmd::Kind;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::*;
use std::time::Duration;

/// What the button is by: its place (row, piece, span start; a code block:
/// its row), what it copies, its tooltip's word and where it is drawn.
#[derive(Clone, PartialEq)]
pub struct Target { pub key: (usize, usize, usize), pub text: String, pub what: &'static str, pub at: Point<Pixels>, scroll: Pixels }

#[derive(Default)]
pub struct Hint { pub on: Option<Target>, pub done: bool, over: bool, hide: Option<Task<()>> }

/// The pointer moved over the log: the button follows what is under it.
/// True when it changed.
pub fn hover(v: &mut LogView, p: Point<Pixels>, at: Option<super::At>, cx: &mut Context<LogView>) -> bool {
    let t = target(v, p, at, cx);
    match t {
        Some(t) => {
            v.ui.copy.hide = None;
            if v.ui.copy.on.as_ref().map(|o| o.key) != Some(t.key) { v.ui.copy.done = false; }
            let changed = v.ui.copy.on.as_ref() != Some(&t);
            v.ui.copy.on = Some(t);
            changed
        }
        None => { if v.ui.copy.on.is_some() && !v.ui.copy.over { hide_soon(v, cx); } false }
    }
}

/// The pointer left the log: the button goes soon (not while over it).
pub fn left(v: &mut LogView, cx: &mut Context<LogView>) { if v.ui.copy.on.is_some() && !v.ui.copy.over { hide_soon(v, cx); } }

fn hide_soon(v: &mut LogView, cx: &mut Context<LogView>) {
    if v.ui.copy.hide.is_some() { return; }
    v.ui.copy.hide = Some(cx.spawn(async move |this: WeakEntity<LogView>, cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_millis(250)).await;
        let _ = this.update(cx, |v, cx| { v.ui.copy.hide = None; if !v.ui.copy.over && v.ui.copy.on.take().is_some() { cx.notify(); } });
    }));
}

/// What the button would be by at `p`: a code span or link, else a code block.
fn target(v: &LogView, p: Point<Pixels>, at: Option<super::At>, cx: &App) -> Option<Target> {
    let scroll = v.list.scroll_px_offset_for_scrollbar().y;
    let rows = &v.store.read(cx).rows;
    if let Some(at) = at {
        let in_code = rows.get(at.0).and_then(|r| r.block.as_ref()).is_some_and(|b| b.kind == Kind::Code);
        if !in_code {
            if let Some((rg, text, what)) = copy_span(rows, at) {
                let hits = v.hits.borrow();
                let h = hits.iter().find(|h| h.row == at.0 && h.part == at.1)?;
                let (end, lh) = h.layout.point_for(rg.end)?;
                let mut x = end.x + px(3.);
                let room = v.list.viewport_bounds().right();
                if x + T::LOG_COPY_W > room - px(4.) { x = end.x - T::LOG_COPY_W; }
                return Some(Target { key: (at.0, at.1, rg.start), text, what, at: point(x, end.y + lh / 2. - T::LOG_COPY_H / 2.), scroll });
            }
        }
    }
    // A code block under the pointer: the button in its top right corner.
    let hits = v.hits.borrow();
    for h in hits.iter() {
        let Some(r) = rows.get(h.row) else { continue };
        if r.block.as_ref().is_none_or(|b| b.kind != Kind::Code) { continue }
        // The box around the code's text: its padding and 1 px edge, the log's width less its 16 px sides.
        let tb = h.layout.bounds();
        let vb = v.list.viewport_bounds();
        let pre = Bounds::from_corners(point(vb.left() + T::SP_16, tb.top() - T::MD_CODE_PY - px(1.)), point(vb.right() - T::SP_16, tb.bottom() + T::MD_CODE_PY + px(1.)));
        if pre.contains(&p) {
            return Some(Target { key: (h.row, usize::MAX, 0), text: r.text.to_string(), what: "code", at: point(pre.right() - T::LOG_COPY_W - T::SP_6, pre.top() + T::SP_6), scroll });
        }
    }
    None
}

/// Copy, as the web's copyHint click: the text, the toast, the tick.
fn press(v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) {
    let Some(t) = v.ui.copy.on.clone() else { return };
    cx.write_to_clipboard(ClipboardItem::new_string(t.text));
    v.ui.copy.done = true;
    w::toast(window, cx, "Copied", w::Toast::Info);
    cx.notify();
}

/// The button (None: nothing hovered, or the log scrolled since).
pub fn button(v: &mut LogView, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let t = v.ui.copy.on.clone()?;
    if t.scroll != v.list.scroll_px_offset_for_scrollbar().y { v.ui.copy.on = None; v.ui.copy.over = false; return None; }
    let origin = v.list.viewport_bounds().origin;
    let done = v.ui.copy.done;
    Some(div().absolute().left(t.at.x - origin.x).top(t.at.y - origin.y)
        .child(div().id("log-copy").occlude().w(T::LOG_COPY_W).h(T::LOG_COPY_H).flex().items_center().justify_center().cursor_pointer()
            .rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg_secondary()).shadow_md()
            .text_color(if done { T::green() } else { T::text_secondary() })
            .hover(move |d| { let d = d.bg(T::bg_hover()); if done { d } else { d.text_color(T::text()) } })
            .tooltip(w::tooltip(format!("Copy {}", t.what)))
            .child(w::icon(if done { IconName::Check } else { IconName::Copy }, T::LOG_COPY_ICON))
            .on_hover(cx.listener(|v, on: &bool, _, cx| { v.ui.copy.over = *on; if *on { v.ui.copy.hide = None; } else { hide_soon(v, cx); } }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|v, _, window, cx| press(v, window, cx))))
        .into_any_element())
}

/// The driver: copy what the button is by (as a click on it).
pub fn press_now(v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> bool {
    if v.ui.copy.on.is_none() { return false; }
    press(v, window, cx);
    true
}

/// A right-click on a web link: Open in browser, Copy link.
pub fn link_menu(url: String, at: Point<Pixels>, cx: &mut App) {
    let items = vec![
        w::MenuEntry::new("open", "Open in browser").icon(IconName::ExternalLink),
        w::MenuEntry::new("copy", "Copy link").icon(IconName::Copy),
    ];
    w::menu_at(cx, at, items, move |id, window, cx| match id.as_str() {
        "open" => super::open_link(&crundi_core::webmd::Link::Url(url.clone()), cx),
        _ => { cx.write_to_clipboard(ClipboardItem::new_string(url.clone())); w::toast(window, cx, "Link copied", w::Toast::Info); }
    });
}
