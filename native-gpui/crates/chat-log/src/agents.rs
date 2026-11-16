//! 2.18: agent rows (a call that started a sub-agent) and the agent dock
//! (sub-agent badges, their list and one agent's messages; specs/agent-dock.md).
//!
//! The chat's records are `conv.agents` (with `conv.agent_msgs`, each
//! agent's own transcript; crundi_core::chat), their wording
//! crundi_core::agents'. The floating dock over the bottom right of the log
//! holds the badges (commands, then agents), then 2.17's recap and
//! suggestion. A badge opens its kind's list, a row opens one record's
//! panel (its prompt, summary and transcript drawn as the log draws
//! entries; a command: its own tool row from the chat, open), both as a
//! sheet over the log. Dismissing is at once here, then POST
//! …/dismiss-agents.
use crate::{rows::rows_of, LogView};
use crundi_core::{agents, chat::Conversation};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

#[derive(Default)]
pub struct State {
    /// The open list: Some(true) commands, Some(false) agents.
    pub list: Option<bool>,
    /// The open panel: (toolUseId, opened from the list: the back link).
    pub panel: Option<(String, bool)>,
    /// The floating dock's height as last drawn (the log's room for it).
    pub dock_h: f32,
    /// The panel's scroll, its rows last drawn, and "to the end" frames left.
    pub panel_scroll: ScrollHandle,
    pub panel_n: usize,
    pub panel_stick: u8,
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// A row's or the log row's state: running | failed | done.
pub fn state_of(a: &Value) -> &'static str {
    if agents::running(a) { "running" } else if s(a, "status") == "failed" { "failed" } else { "done" }
}

/// The badges: (count, any running, tooltip) for the commands and the agents.
pub fn badges(list: &[Value]) -> ((usize, bool, String), (usize, bool, String)) {
    let shown: Vec<&Value> = list.iter().filter(|a| agents::shown(a)).collect();
    let one = |task: bool| {
        let l: Vec<&&Value> = shown.iter().filter(|a| agents::is_task(a) == task).collect();
        let run = l.iter().filter(|a| agents::running(a)).count();
        (l.len(), run > 0, agents::tooltip(task, l.len(), run))
    };
    (one(true), one(false))
}

/// The open list's records (shown, of its kind), in the list's order.
fn list_of(conv: &Conversation, task: bool) -> Vec<Value> {
    let l: Vec<Value> = conv.agents.iter().filter(|a| agents::shown(a) && agents::is_task(a) == task).cloned().collect();
    agents::sorted(&l)
}

/// The open panel's transcript: the agent's own entries, or a command's
/// tool row from the chat (open).
fn panel_conv(conv: &Conversation, tid: &str) -> (Conversation, HashSet<String>) {
    let task = conv.agents.iter().find(|a| s(a, "toolUseId") == tid).is_some_and(agents::is_task);
    let mut c = Conversation::default();
    c.entries = if task { conv.entries.iter().filter(|e| s(e, "toolUseId") == tid).cloned().collect() } else { conv.agent_msgs.get(tid).cloned().unwrap_or_default() };
    let open = if task { c.entries.iter().filter(|e| s(e, "kind") == "tool").map(|e| s(e, "id")).collect() } else { HashSet::new() };
    (c, open)
}

fn panel_rows(v: &LogView, cx: &App) -> Vec<crate::rows::Row> {
    let Some((tid, _)) = &v.st.agents.panel else { return vec![] };
    let m = v.model.read(cx);
    let (c, mut open) = panel_conv(&m.conv, tid);
    open.extend(v.open.iter().cloned());
    rows_of(&c.view(), &[], &open, &v.st.thinking.think, &[])
}

/// This feature's fields in the chat's props: the Slint client's `agentDock`.
pub fn props(v: &LogView, cx: &App, o: &mut Map<String, Value>) {
    let m = v.model.read(cx);
    let ((cn, cr, ct), (an, ar, at)) = badges(&m.conv.agents);
    let mut d = json!({ "agents": an, "agentsRunning": ar, "commands": cn, "commandsRunning": cr, "agTip": at, "cmdTip": ct,
        "sheet": "", "title": "", "sub": "", "back": "", "rows": [], "prompt": "", "entries": 0, "empty": "" });
    if let Some(task) = v.st.agents.list {
        let l = list_of(&m.conv, task);
        let r = l.iter().filter(|a| agents::running(a)).count();
        d["sheet"] = json!(if task { "commands" } else { "agents" });
        d["title"] = json!(if task { "Commands" } else { "Agents" });
        d["sub"] = json!(agents::list_sub(r, l.len() - r));
        d["rows"] = json!(l.iter().map(|a| json!({ "id": s(a, "toolUseId"), "title": agents::label(a), "sub": agents::row_sub(a), "state": state_of(a) })).collect::<Vec<_>>());
    } else if let Some((tid, back)) = &v.st.agents.panel {
        if let Some(a) = m.conv.agents.iter().find(|a| s(a, "toolUseId") == *tid) {
            let n = panel_rows(v, cx).len();
            d["sheet"] = json!("panel");
            d["title"] = json!(agents::label(a));
            d["sub"] = json!(agents::panel_sub(a));
            d["back"] = json!(if *back { if agents::is_task(a) { "‹ Commands" } else { "‹ Agents" } } else { "" });
            d["prompt"] = json!(s(a, "prompt"));
            d["entries"] = json!(n);
            d["empty"] = json!(if n == 0 { agents::empty_text(a) } else { "" });
        }
    }
    o.insert("agentDock".into(), d);
}

/// The open panel's distance from its end (the driver's top-level
/// `panelDist`; 0 with no panel open).
pub fn panel_dist(cx: &App) -> f32 {
    let views: Vec<Entity<LogView>> = cx.try_global::<crate::Views>().map(|v| v.0.values().filter_map(|w| w.upgrade()).collect()).unwrap_or_default();
    views.iter().map(|v| v.read(cx)).find(|v| v.st.agents.panel.is_some())
        .map(|v| { let h = &v.st.agents.panel_scroll; (f32::from(h.max_offset().y) + f32::from(h.offset().y)).max(0.) }).unwrap_or(0.)
}

/// A badge: its kind's list (closing any panel); again: closed.
pub fn badge_click(v: &mut LogView, task: bool, cx: &mut Context<LogView>) {
    v.st.agents.panel = None;
    v.st.agents.list = if v.st.agents.list == Some(task) { None } else { Some(task) };
    cx.notify();
}

/// A list row: that record's panel, with the back link; the log's agent
/// row: its panel without one (again: closed).
pub fn open(v: &mut LogView, tid: &str, from_list: bool, cx: &mut Context<LogView>) {
    let same = v.st.agents.panel.as_ref().is_some_and(|(t, _)| t == tid);
    v.st.agents.list = None;
    v.st.agents.panel = if same && !from_list { None } else { Some((tid.to_string(), from_list)) };
    v.st.agents.panel_n = 0;
    v.st.agents.panel_stick = 3;
    cx.notify();
}

pub fn back(v: &mut LogView, cx: &mut Context<LogView>) {
    let task = v.st.agents.panel.as_ref().and_then(|(t, _)| v.model.read(cx).conv.agents.iter().find(|a| s(a, "toolUseId") == *t).map(agents::is_task)).unwrap_or(false);
    v.st.agents.panel = None;
    v.st.agents.list = Some(task);
    cx.notify();
}

pub fn close(v: &mut LogView, cx: &mut Context<LogView>) {
    v.st.agents.list = None;
    v.st.agents.panel = None;
    cx.notify();
}

/// Dismiss some (a row's ✕, Dismiss all): at once here, then POST
/// …/dismiss-agents. The list closes when its last one goes.
pub fn dismiss(v: &mut LogView, ids: Vec<String>, cx: &mut Context<LogView>) {
    if ids.is_empty() { return; }
    let chat = v.model.update(cx, |m, cx| {
        for a in m.conv.agents.iter_mut() { if ids.contains(&s(a, "toolUseId")) { a["dismissed"] = json!(true); } }
        cx.notify();
        m.conv.id.clone()
    });
    if let Some(task) = v.st.agents.list {
        if !v.model.read(cx).conv.agents.iter().any(|a| agents::shown(a) && agents::is_task(a) == task) { v.st.agents.list = None; }
    }
    cx.notify();
    let api = crundi_shell::api(cx);
    let path = format!("/api/ui-sessions/{chat}/dismiss-agents");
    let body = json!({ "toolUseIds": ids });
    crundi_shell::run(cx, move || api.map(|api| api.post(&path, &body)), move |r, cx| {
        if let Some(Err(crundi_core::api::ApiError::Status(404, _))) = r {
            if let Some(w) = cx.active_window() { let _ = w.update(cx, |_, window, cx| w::toast(window, cx, "Dismissing is not supported by this server", w::Toast::Error)); }
        }
    });
}

/// Dismiss all: every shown record of this kind (running ones too); the list closes.
pub fn dismiss_all(v: &mut LogView, task: bool, cx: &mut Context<LogView>) {
    let ids: Vec<String> = v.model.read(cx).conv.agents.iter().filter(|a| agents::shown(a) && agents::is_task(a) == task).map(|a| s(a, "toolUseId")).collect();
    dismiss(v, ids, cx);
    v.st.agents.list = None;
    cx.notify();
}

/// This feature's driver actions (Some(answer) for a name it knows).
pub fn act(name: &str, args: &[String], v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "agent-badge" => badge_click(v, a(0) == "commands", cx),
        "agent-open" => open(v, &a(0), a(1) == "list", cx),
        "agent-back" => back(v, cx),
        "agent-close" => close(v, cx),
        "agent-dismiss" => dismiss(v, vec![a(0)], cx),
        "agent-dismiss-all" => dismiss_all(v, a(0) == "commands", cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// A call that started a sub-agent: its row in the log (the status, the
/// type and description, "transcript ›" opening its panel).
pub fn row(v: &mut LogView, id: &str, a: &Value, cx: &mut Context<LogView>) -> AnyElement {
    let z = px(v.model.read(cx).zoom as f32);
    let st = state_of(a);
    let tid = s(a, "toolUseId");
    let t = s(a, "subagentType");
    div().id(SharedString::from(format!("ag-row-{id}"))).flex().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_6)
        .rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border_subtle()).hover(|d| d.border_color(T::accent())).cursor_pointer()
        .child(crate::tools::mark(st == "running", st == "failed"))
        .child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_12 + z).font_weight(T::W_SEMIBOLD).text_color(T::accent_hover()).child(if t.is_empty() { "Agent".to_string() } else { t }))
        .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12 + z).text_color(T::text_secondary()).child(crate::tools::one_line(&s(a, "description"))))
        .child(div().flex_none().text_size(T::TEXT_11 + z).text_color(T::accent_hover()).child("transcript ›"))
        .on_click(cx.listener(move |v, _, _, cx| open(v, &tid, false, cx)))
        .into_any_element()
}

/// The log's room at its end for the floating dock (its height + SP_12).
pub fn pad(v: &LogView) -> AnyElement {
    let h = v.st.agents.dock_h;
    div().flex_none().h(if h > 0. { px(h) + T::SP_12 } else { px(0.) }).into_any_element()
}

/// One badge: the pill (icon, mono count, spinner or green dot).
fn badge(task: bool, n: usize, run: bool, tip: String, on: bool, z: Pixels, cx: &mut Context<LogView>) -> AnyElement {
    div().id(if task { "ag-badge-cmd" } else { "ag-badge-ag" }).flex().items_center().gap(T::SP_6).min_h(T::CONTROL_SM).pl(T::SP_8).pr(T::SP_10).py(T::SP_4)
        .rounded(T::R_PILL).bg(T::bg_secondary()).border_1().border_color(if on { T::accent() } else { T::border() }).shadow_lg()
        .text_size(T::TEXT_12 + z).text_color(if on { T::text() } else { T::text_secondary() }).cursor_pointer()
        .hover(|d| d.border_color(T::accent()).text_color(T::text()))
        .tooltip(w::tip(tip))
        .child(div().flex_none().text_size(T::ICON_MD + z).when(run, |d| d.text_color(T::accent_hover())).child(if task { IconName::SquareTerminal } else { IconName::Bot }))
        .child(div().font_family(T::MONO).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(n.to_string()))
        .child(if run { w::spinner(T::ICON_SM).into_any_element() } else { w::dot(T::green()).size(T::SP_6).into_any_element() })
        .on_click(cx.listener(move |v, _, _, cx| badge_click(v, task, cx)))
        .into_any_element()
}

/// Over the bottom right of the log: the floating dock (the badges, then
/// 2.17's recap and suggestion). None: nothing in it.
pub fn dock(v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let m = v.model.read(cx);
    let z = px(m.zoom as f32);
    let ((cn, cr, ct), (an, ar, at)) = badges(&m.conv.agents);
    let rest = crate::suggest::view(v, window, cx);
    let list = v.st.agents.list;
    if cn == 0 && an == 0 && rest.is_none() {
        if v.st.agents.dock_h != 0. { v.st.agents.dock_h = 0.; cx.notify(); }
        return None;
    }
    let mut col = div().absolute().right(T::SP_10).bottom(T::SP_8).max_w(T::DRAWER_MIN).flex().flex_col().items_end().gap(T::SP_4);
    if cn + an > 0 {
        let mut b = div().flex().justify_end().gap(T::SP_6);
        if cn > 0 { b = b.child(badge(true, cn, cr, ct, list == Some(true), z, cx)); }
        if an > 0 { b = b.child(badge(false, an, ar, at, list == Some(false), z, cx)); }
        col = col.child(b);
    }
    // Its height, for the log's room at the end (a frame later).
    let me = cx.entity().downgrade();
    col = col.children(rest).child(canvas(move |b, _, cx| {
        let h = f32::from(b.size.height);
        let _ = me.update(cx, |v, cx| if (v.st.agents.dock_h - h).abs() > 0.5 { v.st.agents.dock_h = h; cx.notify(); });
    }, |_, _, _, _| {}).absolute().inset_0());
    Some(col.into_any_element())
}

/// The sheet's head: (back link), title, sub, (Dismiss all), ✕.
fn head(back_link: Option<String>, title: String, sub: String, all: Option<bool>, z: Pixels, cx: &mut Context<LogView>) -> Div {
    div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_8).border_b_1().border_color(T::border_subtle())
        .when_some(back_link, |d, b| d.child(div().id("ag-back").flex_none().pr(T::SP_6).text_size(T::TEXT_12 + z).text_color(T::text_muted()).cursor_pointer()
            .hover(|d| d.text_color(T::text())).tooltip(w::tip("Back to the list")).child(b).on_click(cx.listener(|v, _, _, cx| back(v, cx)))))
        .child(div().flex_none().max_w(relative(0.7)).truncate().text_size(T::TEXT_13 + z).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(title))
        .child(div().flex_1().min_w_0().truncate().font_family(T::MONO).text_size(T::TEXT_11 + z).text_color(T::text_muted()).child(sub))
        .when_some(all, |d, task| d.child(w::button("ag-dismiss-all", "Dismiss all").on_click(cx.listener(move |v, _, _, cx| dismiss_all(v, task, cx)))))
        .child(w::icon_button("ag-close", IconName::X, "Close").on_click(cx.listener(|v, _, _, cx| close(v, cx))))
}

/// Over the log (inset SP_8): the open list or panel (None: neither).
pub fn sheet(v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let z = px(v.model.read(cx).zoom as f32);
    let frame = || div().absolute().inset(T::SP_8).flex().flex_col().bg(T::bg()).border_1().border_color(T::border()).rounded(T::R_CARD).shadow_lg().overflow_hidden().occlude();
    if let Some(task) = v.st.agents.list {
        let l = list_of(&v.model.read(cx).conv, task);
        let r = l.iter().filter(|a| agents::running(a)).count();
        let rows: Vec<AnyElement> = l.iter().map(|a| {
            let tid = s(a, "toolUseId");
            let (t1, t2) = (tid.clone(), tid.clone());
            let st = state_of(a);
            div().id(SharedString::from(format!("ag-li-{tid}"))).flex().items_center().gap(T::SP_8).p(T::SP_8).rounded(T::R_CONTROL).cursor_pointer().hover(|d| d.bg(T::bg_hover()))
                .child(div().flex_none().w(T::SP_12).flex().justify_center().child(crate::tools::mark(st == "running", st == "failed")))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(T::SP_2)
                    .child(div().truncate().text_size(T::TEXT_13 + z).text_color(if st == "running" { T::text() } else { T::text_secondary() }).child(agents::label(a)))
                    .child(div().truncate().font_family(T::MONO).text_size(T::TEXT_11 + z).text_color(T::text_muted()).child(agents::row_sub(a))))
                .child(div().flex_none().text_color(T::text_muted()).child(w::icon(IconName::ChevronRight, T::ICON_SM)))
                .child(w::icon_button(SharedString::from(format!("ag-x-{tid}")), IconName::X, "Dismiss").on_click(cx.listener(move |v, _, _, cx| { cx.stop_propagation(); dismiss(v, vec![t2.clone()], cx); })))
                .on_click(cx.listener(move |v, _, _, cx| open(v, &t1, true, cx)))
                .into_any_element()
        }).collect();
        let foot = if task { "A finished command leaves this list after 5 minutes. Its output stays on its row in the chat. Dismissing one does not stop it." }
            else { "A finished agent leaves this list after 5 minutes. Its transcript stays on its Agent row in the chat." };
        return Some(frame()
            .child(head(None, (if task { "Commands" } else { "Agents" }).into(), agents::list_sub(r, l.len() - r), Some(task), z, cx))
            .child(div().id("ag-list").flex_1().min_h_0().overflow_y_scroll().p(T::SP_6).child(div().flex().flex_col().children(rows)))
            .child(div().flex_none().px(T::SP_12).py(T::SP_8).border_t_1().border_color(T::border_subtle()).text_size(T::TEXT_11 + z).text_color(T::text_muted()).child(foot))
            .into_any_element());
    }
    let (tid, from_list) = v.st.agents.panel.clone()?;
    let a = v.model.read(cx).conv.agents.iter().find(|a| s(a, "toolUseId") == tid).cloned()?;
    let rows = panel_rows(v, cx);
    // New entries: to the end when the reader was near it (within SP_40).
    if rows.len() != v.st.agents.panel_n {
        let h = &v.st.agents.panel_scroll;
        let dist = f32::from(h.max_offset().y) + f32::from(h.offset().y);
        if v.st.agents.panel_n == 0 || dist < f32::from(T::SP_40) { v.st.agents.panel_stick = 3; }
        v.st.agents.panel_n = rows.len();
    }
    if v.st.agents.panel_stick > 0 {
        v.st.agents.panel_stick -= 1;
        v.st.agents.panel_scroll.scroll_to_bottom();
        let me = cx.entity().downgrade();
        window.on_next_frame(move |_, cx| { let _ = me.update(cx, |_, cx| cx.notify()); });
    }
    let els: Vec<AnyElement> = rows.iter().enumerate().filter_map(|(i, r)| v.row(i, r, window, cx)).collect();
    let prompt = s(&a, "prompt");
    let summary = s(&a, "summary");
    let summary_blocks: Vec<AnyElement> = if summary.is_empty() { vec![] } else {
        crate::rows::split_blocks(&format!("agsum-{tid}"), &summary).into_iter().filter_map(|r| match r {
            crate::rows::Row::Prose(id, t, kind, marker, last) => Some(crate::prose::block(&id, &t, kind, &marker, last, cx)),
            crate::rows::Row::Code(id, c, last) => Some(crate::prose::code(v, &id, &c, last, cx)),
            _ => None,
        }).collect()
    };
    let back = from_list.then(|| (if agents::is_task(&a) { "‹ Commands" } else { "‹ Agents" }).to_string());
    let empty = els.is_empty().then(|| agents::empty_text(&a));
    Some(frame()
        .child(head(back, agents::label(&a), agents::panel_sub(&a), None, z, cx))
        .child(div().id("ag-panel").flex_1().min_h_0().overflow_y_scroll().track_scroll(&v.st.agents.panel_scroll)
            .child(div().flex().flex_col().gap(T::SP_10).px(T::SP_12).py(T::SP_10)
                .when(!prompt.is_empty(), |d| d.child(div().id("ag-prompt").max_h(T::COMPOSER_MAX).overflow_y_scroll().px(T::SP_8).py(T::SP_6)
                    .rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border_subtle())
                    .text_size(T::TEXT_12 + z).text_color(T::text_secondary()).child(prompt)))
                .children(summary_blocks)
                .children(els)
                .when_some(empty, |d, t| d.child(div().italic().text_color(T::text_muted()).child(t)))))
        .into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn badges_count_shown_records_by_kind() {
        let l = vec![
            json!({ "toolUseId": "a", "kind": "agent", "status": "running" }),
            json!({ "toolUseId": "t", "kind": "task", "status": "completed" }),
            json!({ "toolUseId": "x", "kind": "agent", "status": "completed", "dismissed": true }),
        ];
        let ((cn, cr, ct), (an, ar, at)) = badges(&l);
        assert_eq!((cn, cr, an, ar), (1, false, 1, true));
        assert_eq!(at, "1 agent, 1 running — click to see it");
        assert_eq!(ct, "1 background command, all finished — click to see it");
        assert_eq!(state_of(&json!({ "status": "failed" })), "failed");
        assert_eq!(state_of(&json!({})), "running");
    }

    #[::core::prelude::v1::test]
    fn a_commands_panel_is_its_tool_row_open() {
        let mut c = Conversation::default();
        c.agents = vec![json!({ "toolUseId": "t1", "kind": "task" }), json!({ "toolUseId": "a1", "kind": "agent" })];
        c.entries = vec![json!({ "id": "e1", "kind": "tool", "toolUseId": "t1", "name": "Bash" }), json!({ "id": "e2", "kind": "tool", "toolUseId": "zz" })];
        c.agent_msgs.insert("a1".into(), vec![json!({ "id": "m1", "kind": "assistant-text", "text": "hi" })]);
        let (p, open) = panel_conv(&c, "t1");
        assert_eq!(p.entries.len(), 1);
        assert!(open.contains("e1"));
        let (p, open) = panel_conv(&c, "a1");
        assert_eq!(p.entries.len(), 1);
        assert!(open.is_empty());
    }
}
