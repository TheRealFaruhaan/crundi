//! 2.14: tool cards (the web's toolNode / toolDetailHtml): the head (a
//! spinner while running, else a dot; the name; the summary; the
//! disclosure's chevron) and, open, the body by kind (Bash, Write, Edit's
//! diff, TodoWrite, else the JSON) with the result under a hairline. A
//! batch Claude Code summed up in one line folds behind that line (the
//! web's toolGroupHead): its head ("Listed current directory · 1 tool ▸")
//! on the call whose id is the group's, every call's card only while the
//! group is open. A call that started a sub-agent is its agent's row
//! (agents.rs), never folded. What a show_* call showed comes under its
//! card (media.rs), a widget check's screenshots after (cards.rs).
//!
//! Each call is its own row (the store's): folding a group measures its
//! calls' rows again in this log only (`LogView::toggle`).

use super::LogView;
use crate::store::Row;
use crundi_core::{chat::Shown, tools};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

/// The head's summary on one line (the web's HTML collapses whitespace).
pub fn one_line(t: &str) -> String { t.split_whitespace().collect::<Vec<_>>().join(" ") }

/// "N tools" / "1 tool".
pub fn count_label(n: usize) -> String { format!("{n} tool{}", if n == 1 { "" } else { "s" }) }

/// A call's status mark: the spinner while running, else a dot (red: failed).
pub fn mark(running: bool, failed: bool, fs: f32) -> AnyElement { w::call_mark(running, failed, fs) }

/// A call's row: its agent's row, its group's head (and card while open),
/// its card, or nothing (a folded group's call).
pub fn row(v: &mut LogView, ix: usize, r: &Row, fs: f32, pad: (Pixels, Pixels), window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    let e = r.shown.clone();
    let wrap = |el: AnyElement| div().w_full().px(T::SP_16).pt(pad.0).pb(pad.1).child(el).into_any_element();
    if v.store.read(cx).log.agent_rows.contains(&e.id) {
        if let Some(el) = super::agents::row(v, &e, fs, cx) { return wrap(el); }
    }
    if !e.group.is_empty() && !e.group_summary.is_empty() {
        let open = v.open.contains(&format!("g:{}", e.group));
        let head = (e.group == e.id).then(|| group_head(v, &e, open, fs, cx));
        let body = open.then(|| card(v, &e, fs, window, cx));
        if head.is_none() && body.is_none() {
            // Folded away (the web's display:none): no room, no gap.
            let _ = ix;
            return div().into_any_element();
        }
        return wrap(div().flex().flex_col().gap(T::SP_4).children(head).children(body).into_any_element());
    }
    wrap(card(v, &e, fs, window, cx))
}

/// The group's head (.cc-tool.cc-tgroup): the calls' state (running if any
/// runs, else red if any failed), the summary, "N tools", the chevron.
fn group_head(v: &mut LogView, e: &Shown, open: bool, fs: f32, cx: &mut Context<LogView>) -> AnyElement {
    let s = v.store.read(cx);
    let ms: Vec<&Value> = s.log.groups.get(&e.group).map(|m| m.iter().filter_map(|id| s.conv.position(id).map(|p| &s.conv.entries[p])).collect()).unwrap_or_default();
    let n = ms.len().max(1);
    let running = ms.iter().any(|m| m.get("status").and_then(|x| x.as_str()) == Some("running")) || (ms.is_empty() && e.status == "running");
    let failed = ms.iter().any(|m| m.get("isError").and_then(|x| x.as_bool()).unwrap_or(false));
    let key = format!("g:{}", e.group);
    w::call_card()
        .child(w::call_head(SharedString::from(format!("tg-{}", e.group)), mark(running, failed, fs), None, one_line(&e.group_summary), true, fs)
            .child(w::call_aside(count_label(n), false, fs))
            .child(w::fold_chevron(open, fs).text_color(T::text_muted()))
            .tooltip(w::tooltip(if open { "Fold these tool calls" } else { "Show each tool call" }))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle(&key, cx))))
        .into_any_element()
}

/// The body's text: mono, keeps its lines, wraps.
fn body_text(t: &str, color: Hsla, fs: f32) -> Div {
    div().w_full().font_family(T::MONO).text_size(T::TEXT_12 * fs).line_height(relative(T::LH_CHAT)).text_color(color).child(t.to_string())
}

/// A band (a Write's content, a diff line): green or red on its fill.
fn band(t: &str, add: bool, fs: f32) -> Div {
    div().w_full().bg(if add { T::green_dim() } else { T::red_dim() }).child(body_text(t, if add { T::green() } else { T::red() }, fs))
}

/// A to-do: a box (ticked when done; a play mark while in progress), its text.
fn todo_row(text: &str, state: &str, fs: f32) -> Div {
    let c = match state { "completed" => T::text_muted(), "in_progress" => T::accent_hover(), _ => T::text_secondary() };
    let icon = match state { "completed" => IconName::SquareCheck, "in_progress" => IconName::Play, _ => IconName::Square };
    div().flex().gap(T::SP_8).items_start()
        .child(div().flex_none().pt(T::SP_2).text_color(c).child(w::icon(icon, T::ICON_SM * fs)))
        .child(body_text(text, c, fs).flex_1().min_w_0().when(state == "completed", |d| d.line_through()))
}

/// One call's card, and what it showed under it.
pub fn card(v: &mut LogView, e: &Shown, fs: f32, window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    let open = v.open.contains(&e.id);
    let k = e.id.clone();
    let head = w::call_head(SharedString::from(format!("th-{}", e.id)), mark(e.status == "running", e.error, fs), Some((e.title.clone().into(), e.error)), one_line(&e.detail), false, fs)
        .child(w::fold_chevron(open, fs).text_color(T::text_muted()))
        .on_click(cx.listener(move |this, _, _, cx| this.toggle(&k, cx)));
    let mut card = w::call_card().child(head);
    if open { card = card.child(div().h(px(1.)).w_full().bg(T::border_subtle())).child(body(e, fs)); }
    let under = super::media::under(v, e, fs, window, cx);
    let shots = super::cards::shots_row(v, e, cx);
    if under.is_none() && shots.is_none() { return card.into_any_element(); }
    div().flex().flex_col().gap(T::SP_6).child(card).children(under).children(shots).into_any_element()
}

/// The open card's body: the arguments by kind, the result under a hairline
/// (muted; red when the call failed).
pub fn body(e: &Shown, fs: f32) -> Stateful<Div> {
    let mut b = div().id(SharedString::from(format!("tb-{}", e.id))).flex().flex_col().px(T::SP_10).py(T::SP_8)
        .max_h(T::COMPOSER_MAX * 2.).overflow_y_scroll();
    b = match &e.body {
        tools::Body::Bash { command, description } => b
            .child(body_text(&tools::expand_tabs(command), T::text_secondary(), fs))
            .when(!description.is_empty(), |d| d.child(body_text(description, T::text_muted(), fs).pt(T::SP_6))),
        tools::Body::Write(c) => b.child(band(&tools::expand_tabs(c), true, fs)),
        tools::Body::Diff(lines) => b.children(lines.iter().map(|(l, add)| band(&tools::expand_tabs(l), *add, fs))),
        tools::Body::Todos(items) => b.child(div().flex().flex_col().gap(T::SP_4).children(items.iter().map(|(t, st)| todo_row(t, st, fs)))),
        tools::Body::Json => b.child(body_text(&e.args, T::text_secondary(), fs)),
    };
    if !e.text.is_empty() {
        b = b.child(div().mt(T::SP_8).mb(T::SP_8).h(px(1.)).w_full().bg(T::border_subtle()))
            .child(body_text(&e.text, if e.error { T::red() } else { T::text_muted() }, fs));
    }
    b
}

/// A body's kind name ("bash" | "write" | "diff" | "todos" | "json").
pub fn kind_of(b: &tools::Body) -> &'static str {
    match b { tools::Body::Bash { .. } => "bash", tools::Body::Write(_) => "write", tools::Body::Diff(_) => "diff", tools::Body::Todos(_) => "todos", tools::Body::Json => "json" }
}

/// A call as the driver sees it.
pub fn card_json(e: &Shown, open: bool) -> Value {
    json!({ "id": e.id, "name": e.title, "summary": e.detail, "status": e.status, "error": e.error, "open": open, "body": kind_of(&e.body), "result": e.text,
        "group": e.group, "groupSummary": e.group_summary })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn wording() {
        assert_eq!(count_label(1), "1 tool");
        assert_eq!(count_label(3), "3 tools");
        assert_eq!(one_line("ls\n -la"), "ls -la");
    }
}
