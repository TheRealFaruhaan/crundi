//! 2.11: the slash menu (the web's .cc-slash, showSlash): a box holding just
//! "/word" lists the CLI's commands starting with it (case aside; /btw last
//! when the CLI has none; at most 40), the first selected. ↑ ↓ move (kept in
//! view), Enter or Tab puts "/name " in the box and closes it (nothing is
//! sent), a press on a row does the same, Esc closes it and keeps the text
//! (the next edit opens it again). No match: no list. It stands above the
//! box (over the log's foot), drawn last. The rules are crundi_core::slash's.

use super::Composer;
use crundi_core::slash;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};

#[derive(Default)]
pub struct State {
    pub sel: usize,
    /// The text Esc closed the list on (it stays closed until the text changes).
    pub dismissed: Option<String>,
    pub scroll: ScrollHandle,
}

/// The list for the box's text now: (name, description), empty when closed.
pub fn items(c: &Composer, cx: &App) -> Vec<(String, String)> {
    let text = c.text(cx);
    if c.slash.dismissed.as_deref() == Some(text.as_str()) || c.exited { return vec![]; }
    match slash::query(&text) { Some(q) => slash::filter(&c.meta.slash, q), None => vec![] }
}

/// The box's text changed: the first item selected again.
pub fn changed(c: &mut Composer, text: &str) {
    if c.slash.dismissed.as_deref() != Some(text) { c.slash.dismissed = None; }
    c.slash.sel = 0;
    c.slash.scroll.set_offset(point(px(0.), px(0.)));
}

/// ↑ / ↓ while open (true: taken).
pub fn step(c: &mut Composer, by: i32, cx: &mut Context<Composer>) -> bool {
    let n = items(c, cx).len();
    if n == 0 { return false; }
    c.slash.sel = slash::moved(c.slash.sel, by, n);
    c.slash.scroll.scroll_to_item(c.slash.sel);
    cx.notify();
    true
}

/// Enter or Tab while open: the selected item (true: taken).
pub fn enter(c: &mut Composer, window: &mut Window, cx: &mut Context<Composer>) -> bool {
    let list = items(c, cx);
    let Some((name, _)) = list.get(c.slash.sel.min(list.len().saturating_sub(1))) else { return false };
    choose(c, &name.clone(), window, cx);
    true
}

/// Esc while open: closed, the text kept (true: taken).
pub fn escape(c: &mut Composer, cx: &mut Context<Composer>) -> bool {
    if items(c, cx).is_empty() { return false; }
    c.slash.dismissed = Some(c.text(cx));
    cx.notify();
    true
}

fn choose(c: &mut Composer, name: &str, window: &mut Window, cx: &mut Context<Composer>) {
    c.set_text(&slash::chosen(name), window, cx);
    c.focus_box(window, cx);
}

/// The list above the box (out of the flow; drawn after the log).
pub fn render(c: &mut Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let list = items(c, cx);
    if list.is_empty() { return None; }
    let fs = c.fs();
    let sel = c.slash.sel.min(list.len() - 1);
    let rows: Vec<AnyElement> = list.into_iter().enumerate().map(|(i, (name, desc))| {
        let n = name.clone();
        w::suggest_row(("slash", i), format!("/{name}"), desc, i == sel, fs)
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| { cx.stop_propagation(); choose(this, &n, window, cx); }))
            .into_any_element()
    }).collect();
    Some(deferred(div().absolute().left_0().right_0().bottom(relative(1.)).mb(T::SP_4)
        .child(div().id("slash-menu").occlude().flex().flex_col().w_full().max_h(T::COMPOSER_MAX).overflow_y_scroll().track_scroll(&c.slash.scroll)
            .bg(T::bg_secondary()).border_1().border_color(T::border()).rounded(T::R_CONTROL).shadow_md().children(rows)))
        .with_priority(1).into_any_element())
}

/// slash-pick <i>: a press on that row; slash-key up|down|enter|tab|escape.
pub fn test_ui(c: &mut Composer, name: &str, args: &[String], window: &mut Window, cx: &mut Context<Composer>) -> Option<Value> {
    let a = args.first().cloned().unwrap_or_default();
    let taken = match name {
        "slash-pick" => {
            let list = items(c, cx);
            let Some((n, _)) = a.parse::<usize>().ok().and_then(|i| list.get(i).cloned()) else { return Some(json!({ "ok": false })) };
            choose(c, &n, window, cx);
            true
        }
        "slash-key" => match a.as_str() {
            "up" => step(c, -1, cx),
            "down" => step(c, 1, cx),
            "enter" | "tab" => enter(c, window, cx),
            "escape" => escape(c, cx),
            _ => false,
        },
        _ => return None,
    };
    Some(json!({ "ok": true, "taken": taken }))
}

/// slash: null when closed, else {items: ["/name"…], sel}.
pub fn state(c: &Composer, cx: &App) -> Value {
    let list = items(c, cx);
    if list.is_empty() { return json!({ "slash": null }); }
    json!({ "slash": { "items": list.iter().map(|(n, _)| format!("/{n}")).collect::<Vec<_>>(), "sel": c.slash.sel.min(list.len() - 1) } })
}
