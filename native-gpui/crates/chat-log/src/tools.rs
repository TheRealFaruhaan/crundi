//! 2.14: tool cards (specs/tool-cards.md): one call's card (a running
//! spinner, open and closed, its body by kind, the result), a batch Claude
//! Code summed up as one group, and what a show_* call showed under it
//! (media.rs). As the Slint client's ToolCard (ui/chat.slint) and
//! tool_view / tool_json / tools_of (src/app.rs).
use crate::rows::Row;
use crate::LogView;
use crundi_core::{chat::Shown, tools};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};

#[derive(Default)]
pub struct State {}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) { Some(Value::String(x)) => x.clone(), Some(Value::Null) | None => String::new(), Some(o) => o.to_string() }
}

/// The head's summary on one line (the web's HTML collapses whitespace).
pub fn one_line(t: &str) -> String { t.split_whitespace().collect::<Vec<_>>().join(" ") }

/// A body's kind name ("bash" | "write" | "diff" | "todos" | "json").
pub fn kind_of(b: &tools::Body) -> &'static str {
    match b { tools::Body::Bash { .. } => "bash", tools::Body::Write(_) => "write", tools::Body::Diff(_) => "diff", tools::Body::Todos(_) => "todos", tools::Body::Json => "json" }
}

/// A group's status: running if any member runs, else failed if any failed.
pub fn group_status(ms: &[(Shown, bool)]) -> &'static str {
    if ms.iter().any(|m| m.0.status == "running") { "running" } else if ms.iter().any(|m| m.0.error) { "failed" } else { "done" }
}

/// "N tools" / "1 tool".
pub fn count_label(n: usize) -> String { format!("{n} tool{}", if n == 1 { "" } else { "s" }) }

/// A card as the driver sees it (the Slint client's tool_json).
pub fn card_json(t: &Shown, open: bool) -> Value {
    let (args, lines, todos): (String, Vec<Value>, Vec<Value>) = match &t.body {
        tools::Body::Bash { command, .. } => (command.clone(), vec![], vec![]),
        tools::Body::Write(c) => (c.clone(), vec![], vec![]),
        tools::Body::Diff(l) => (String::new(), l.iter().map(|(x, add)| json!({ "text": x, "add": add })).collect(), vec![]),
        tools::Body::Todos(t) => (String::new(), vec![], t.iter().map(|(x, st)| json!({ "text": x, "state": st })).collect()),
        tools::Body::Json => (t.args.clone(), vec![], vec![]),
    };
    json!({ "id": t.id, "name": t.title, "summary": t.detail, "status": t.status, "error": t.error, "open": open, "body": kind_of(&t.body), "args": args, "note": t.note, "lines": lines, "todos": todos, "result": t.text })
}

/// One row as the driver sees it (the Slint client's tools_of): cards,
/// groups, prose, code, notices / errors / results, thinking, agent rows.
/// ("media" and "shots" are the media helpers' to fill: 2.16m, 2.20.)
pub fn row_json(r: &Row) -> Option<Value> {
    Some(match r {
        Row::Tool(_, t, o) => json!({ "card": card_json(t, *o), "media": [], "shots": [] }),
        Row::Tools(id, sum, ms, o) => json!({ "group": id, "summary": sum, "open": o, "count": ms.len(), "status": group_status(ms),
            "members": ms.iter().map(|(t, o)| card_json(t, *o)).collect::<Vec<_>>() }),
        Row::Prose(_, t, kind, marker, _) => json!({ "prose": kind, "text": t, "marker": marker }),
        Row::Code(_, t, _) => json!({ "code": t }),
        Row::Entry(e) if matches!(e.kind, "notice" | "error" | "result") => json!({ "entry": e.kind, "text": e.text, "error": e.error, "detail": e.detail }),
        Row::Think(e, o) => json!({ "think": e.id, "open": o, "streaming": e.status == "streaming", "detail": e.detail, "text": e.text }),
        Row::Agent(id, ag) => json!({ "agent": id, "type": s(ag, "subagentType"), "description": s(ag, "description"), "status": s(ag, "status") }),
        _ => return None,
    })
}

/// This feature's fields in the chat's props: "tools" (every row, as the
/// Slint client's tools_of) and "entries" (how many rows the log draws).
pub fn props(v: &LogView, cx: &App, o: &mut Map<String, Value>) {
    let rows = v.rows(cx);
    o.insert("tools".into(), Value::Array(rows.iter().filter_map(|r| row_json(r).map(|mut j| { if let Row::Tool(_, t, _) = r { j["media"] = crate::media::media_json(v, t, cx); j["shots"] = crate::cards::shots_json(v, t, cx); } j })).collect()));
    o.insert("entries".into(), json!(rows.len()));
}

/// This feature's driver actions: tool-toggle <card, group or thinking id>.
pub fn act(name: &str, args: &[String], v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    match name {
        "tool-toggle" => { let id = args.first().cloned().unwrap_or_default(); v.toggle(&id, cx); Some(json!({ "ok": true })) }
        _ => None,
    }
}

/// A call's status mark: the spinner while running, else a dot (red: failed).
pub fn mark(running: bool, failed: bool) -> AnyElement {
    if running { w::spinner(T::ICON_SM).into_any_element() }
    else { div().flex_none().w(T::ICON_SM).flex().justify_center().child(w::dot(if failed { T::red() } else { T::green() }).size(T::SP_6)).into_any_element() }
}

/// The open / closed caret.
fn caret(open: bool, fs: f32) -> Div {
    div().flex_none().text_color(T::text_muted()).child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM * fs))
}

/// The body's text: mono 12, keeps its lines, wraps.
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

/// One call's card.
pub fn card(v: &mut LogView, key: &str, e: &Shown, open: bool, window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    let fs = crate::prose::fs(v, cx);
    let k = key.to_string();
    let head = div().id(SharedString::from(format!("th-{key}")))
        .flex().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_6).cursor_pointer()
        .child(mark(e.status == "running", e.error))
        .child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_12 * fs).font_weight(T::W_SEMIBOLD)
            .text_color(if e.error { T::red() } else { T::accent_hover() }).child(e.title.clone()))
        .child(div().flex_1().min_w_0().text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).truncate().child(one_line(&e.detail)))
        .child(caret(open, fs))
        .on_click(cx.listener(move |this, _, _, cx| this.toggle(&k, cx)));
    let mut card = w::card().rounded(T::R_CONTROL).bg(T::bg_secondary()).border_color(T::border_subtle()).overflow_hidden().child(head);
    if open {
        let mut body = div().id(SharedString::from(format!("tb-{key}"))).flex().flex_col().px(T::SP_10).py(T::SP_8)
            .max_h(T::COMPOSER_MAX * 2.).overflow_y_scroll();
        body = match &e.body {
            tools::Body::Bash { command, description } => body
                .child(body_text(&tools::expand_tabs(command), T::text_secondary(), fs))
                .when(!description.is_empty(), |d| d.child(body_text(description, T::text_muted(), fs).pt(T::SP_6))),
            tools::Body::Write(c) => body.child(band(&tools::expand_tabs(c), true, fs)),
            tools::Body::Diff(lines) => body.children(lines.iter().map(|(l, add)| band(&tools::expand_tabs(l), *add, fs))),
            tools::Body::Todos(items) => body.child(div().flex().flex_col().gap(T::SP_4).children(items.iter().map(|(t, st)| todo_row(t, st, fs)))),
            tools::Body::Json => body.child(body_text(&e.args, T::text_secondary(), fs)),
        };
        // The result, under a hairline: muted, red when it failed.
        if !e.text.is_empty() {
            body = body.child(div().mt(T::SP_8).mb(T::SP_8).h(px(1.)).w_full().bg(T::border_subtle()))
                .child(body_text(&e.text, if e.error { T::red() } else { T::text_muted() }, fs));
        }
        card = card.child(div().h(px(1.)).w_full().bg(T::border_subtle())).child(body);
    }
    let under = crate::media::under(v, e, window, cx);
    div().flex().flex_col().gap(T::SP_6).child(card).children(under).into_any_element()
}

/// A batch Claude Code summed up in one line (.cc-tgroup): its head (the
/// members' status, the summary, "N tools", the caret), and its members'
/// cards when open (each with its own open state).
pub fn group(v: &mut LogView, gid: &str, summary: &str, members: &[(Shown, bool)], open: bool, window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    let fs = crate::prose::fs(v, cx);
    let k = gid.to_string();
    let st = group_status(members);
    let head = div().id(SharedString::from(format!("tg-{gid}"))).flex().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_6).cursor_pointer()
        .child(mark(st == "running", members.iter().any(|m| m.0.error)))
        .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12 * fs).text_color(T::text()).child(one_line(summary)))
        .child(div().flex_none().text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(count_label(members.len())))
        .child(caret(open, fs))
        .tooltip(w::tip(if open { "Fold these tool calls" } else { "Show each tool call" }))
        .on_click(cx.listener(move |this, _, _, cx| this.toggle(&k, cx)));
    let mut g = div().flex().flex_col()
        .child(w::card().rounded(T::R_CONTROL).bg(T::bg_secondary()).border_color(T::border_subtle()).child(head));
    if open {
        let cards: Vec<AnyElement> = members.iter().map(|(m, o)| card(v, &m.id, m, *o, window, cx)).collect();
        g = g.child(div().flex().flex_col().gap(T::SP_10).pt(T::SP_4).children(cards));
    }
    g.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn driver_rows() {
        let t = Shown { id: "t1".into(), kind: "tool", title: "Bash".into(), detail: "ls\n -la".into(), status: "done".into(),
            body: tools::Body::Bash { command: "ls -la".into(), description: "List".into() }, note: "List".into(), text: "app.js".into(), ..Default::default() };
        let c = card_json(&t, true);
        assert_eq!((c["body"].as_str(), c["args"].as_str(), c["open"].as_bool()), (Some("bash"), Some("ls -la"), Some(true)));
        assert_eq!(one_line("ls\n -la"), "ls -la");
        let bad = Shown { error: true, ..t.clone() };
        let run = Shown { status: "running".into(), ..t.clone() };
        assert_eq!(group_status(&[(t.clone(), false), (bad.clone(), false)]), "failed");
        assert_eq!(group_status(&[(bad, false), (run, false)]), "running");
        assert_eq!(count_label(1), "1 tool");
        assert_eq!(count_label(3), "3 tools");
        let g = row_json(&Row::Tools("g:x".into(), "Read 2 files".into(), vec![(t.clone(), true)], false)).unwrap();
        assert_eq!((g["count"].as_u64(), g["members"][0]["open"].as_bool()), (Some(1), Some(true)));
        let d = Shown { body: tools::Body::Diff(vec![("- a".into(), false), ("+ b".into(), true)]), ..t };
        assert_eq!(card_json(&d, false)["lines"][1]["add"].as_bool(), Some(true));
    }
}
