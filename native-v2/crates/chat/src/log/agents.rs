//! 2.18: agent rows and the agent dock (the web's toolNode agent row,
//! syncBadge, openAgentList, toggleAgentPanel).
//!
//! The chat's records are `conv.agents` (with `conv.agent_msgs`, each
//! agent's own transcript); their wording is crundi_core::agents'. The
//! floating dock over the bottom right of the log holds the badges
//! (commands, then agents), then the recap and the suggestion (suggest.rs).
//! A badge opens its kind's list, a row opens one record's panel (its
//! prompt, summary and transcript; a command: its own tool row, open), both
//! as a sheet over the log. Dismissing is at once here, then POST
//! …/dismiss-agents.

use super::LogView;
use crundi_core::{agents, chat::Shown};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

#[derive(Default)]
pub struct View {
    /// The open list: Some(true) commands, Some(false) agents.
    pub list: Option<bool>,
    /// The open panel: (toolUseId, opened from the list: the back link).
    pub panel: Option<(String, bool)>,
    /// The floating dock's height as last drawn (the log's room for it).
    pub dock_h: f32,
    pub panel_scroll: ScrollHandle,
    panel_n: usize,
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// A record's state: running | failed | done.
pub fn state_of(a: &Value) -> &'static str { if agents::running(a) { "running" } else if s(a, "status") == "failed" { "failed" } else { "done" } }

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

fn list_of(list: &[Value], task: bool) -> Vec<Value> {
    let l: Vec<Value> = list.iter().filter(|a| agents::shown(a) && agents::is_task(a) == task).cloned().collect();
    agents::sorted(&l)
}

/// The log's room at its end for the floating dock (its height + 12).
pub fn room(v: &LogView) -> Pixels { if v.ui.agents.dock_h > 0. { px(v.ui.agents.dock_h) + T::SP_12 } else { px(0.) } }

pub fn badge_click(v: &mut LogView, task: bool, cx: &mut Context<LogView>) {
    v.ui.agents.panel = None;
    v.ui.agents.list = if v.ui.agents.list == Some(task) { None } else { Some(task) };
    cx.notify();
}

/// A list row: its panel with the back link; the log's agent row: its panel
/// without one (again: closed).
pub fn open(v: &mut LogView, tid: &str, from_list: bool, cx: &mut Context<LogView>) {
    let same = v.ui.agents.panel.as_ref().is_some_and(|(t, _)| t == tid);
    v.ui.agents.list = None;
    v.ui.agents.panel = if same && !from_list { None } else { Some((tid.to_string(), from_list)) };
    v.ui.agents.panel_n = 0;
    cx.notify();
}

pub fn back(v: &mut LogView, cx: &mut Context<LogView>) {
    let task = v.ui.agents.panel.as_ref().and_then(|(t, _)| v.store.read(cx).conv.agents.iter().find(|a| s(a, "toolUseId") == *t).map(agents::is_task)).unwrap_or(false);
    v.ui.agents.panel = None;
    v.ui.agents.list = Some(task);
    cx.notify();
}

pub fn close(v: &mut LogView, cx: &mut Context<LogView>) { v.ui.agents.list = None; v.ui.agents.panel = None; cx.notify(); }

/// Dismiss some (a row's ×, Dismiss all): at once here, then POST
/// …/dismiss-agents. The list closes when its last one goes.
pub fn dismiss(v: &mut LogView, ids: Vec<String>, cx: &mut Context<LogView>) {
    if ids.is_empty() { return; }
    for id in &ids { super::shared::local(&v.store, json!({ "type": "agent", "agent": { "toolUseId": id, "dismissed": true } }), cx); }
    if let Some(task) = v.ui.agents.list {
        if !v.store.read(cx).conv.agents.iter().any(|a| agents::shown(a) && agents::is_task(a) == task) { v.ui.agents.list = None; }
    }
    cx.notify();
    let path = format!("/api/ui-sessions/{}/dismiss-agents", v.chat(cx));
    let body = json!({ "toolUseIds": ids });
    crundi_shell::call(cx, move |api| api.post(&path, &body), |r, cx| {
        if let Err(e) = r { if e == "Not supported by this server" { if let Some(w) = cx.active_window() { let _ = w.update(cx, |_, window, cx| w::toast(window, cx, "Dismissing is not supported by this server", w::Toast::Error)); } } }
    });
}

pub fn dismiss_all(v: &mut LogView, task: bool, cx: &mut Context<LogView>) {
    let ids: Vec<String> = v.store.read(cx).conv.agents.iter().filter(|a| agents::shown(a) && agents::is_task(a) == task).map(|a| s(a, "toolUseId")).collect();
    dismiss(v, ids, cx);
    v.ui.agents.list = None;
    cx.notify();
}

/// The dock as the driver sees it.
pub fn state(v: &LogView, cx: &App) -> Value {
    let st = v.store.read(cx);
    let ((cn, cr, ct), (an, ar, at)) = badges(&st.conv.agents);
    let mut d = json!({ "agents": an, "agentsRunning": ar, "commands": cn, "commandsRunning": cr, "agTip": at, "cmdTip": ct, "sheet": "", "title": "", "rows": [], "dockH": v.ui.agents.dock_h });
    if let Some(task) = v.ui.agents.list {
        let l = list_of(&st.conv.agents, task);
        d["sheet"] = json!(if task { "commands" } else { "agents" });
        d["rows"] = json!(l.iter().map(|a| json!({ "id": s(a, "toolUseId"), "title": agents::label(a), "sub": agents::row_sub(a), "state": state_of(a) })).collect::<Vec<_>>());
    } else if let Some((tid, _)) = &v.ui.agents.panel {
        if let Some(a) = st.conv.agents.iter().find(|a| s(a, "toolUseId") == *tid) {
            d["sheet"] = json!("panel");
            d["title"] = json!(agents::label(a));
            d["sub"] = json!(agents::panel_sub(a));
            d["entries"] = json!(panel_entries(st, tid).len());
        }
    }
    d
}

pub fn act(name: &str, args: &[String], v: &mut LogView, cx: &mut Context<LogView>) -> Option<Value> {
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

/// A call that started a sub-agent (.cc-agentrow): the state, the type, the
/// description, "transcript ›" opening its panel.
pub fn row(v: &mut LogView, e: &Shown, fs: f32, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let a = v.store.read(cx).conv.agents.iter().find(|a| s(a, "toolUseId") == e.tool_use_id && !agents::is_task(a)).cloned()?;
    let st = state_of(&a);
    let tid = e.tool_use_id.clone();
    let t = s(&a, "subagentType");
    Some(w::agent_row(SharedString::from(format!("ag-row-{}", e.id)), super::tools::mark(st == "running", st == "failed", fs),
            if t.is_empty() { "Agent".to_string() } else { t }, super::tools::one_line(&s(&a, "description")), fs)
        .on_click(cx.listener(move |v, _, _, cx| open(v, &tid, false, cx)))
        .into_any_element())
}

/// One badge: the pill (icon, mono count, spinner or green dot).
fn badge(v: &LogView, task: bool, n: usize, run: bool, tip: String, on: bool, fs: f32, cx: &mut Context<LogView>) -> AnyElement {
    let spot = v.spot(if task { "badge-cmd" } else { "badge-ag" }, cx);
    w::dock_badge(if task { "ag-badge-cmd" } else { "ag-badge-ag" }, task, n, run, on, fs)
        .tooltip(w::tooltip(tip))
        .relative().children(spot)
        .on_click(cx.listener(move |v, _, _, cx| badge_click(v, task, cx)))
        .into_any_element()
}

/// Over the bottom right of the log: the floating dock (the badges, then
/// the recap and the suggestion). None: nothing in it.
pub fn dock(v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let fs = v.fs();
    let ((cn, cr, ct), (an, ar, at)) = badges(&v.store.read(cx).conv.agents);
    let rest = super::suggest::view(v, cx);
    let list = v.ui.agents.list;
    if cn == 0 && an == 0 && rest.is_none() {
        if v.ui.agents.dock_h != 0. { v.ui.agents.dock_h = 0.; cx.notify(); }
        return None;
    }
    // Never taller than the log: it stays inside, clipped at the top.
    let mut col = div().absolute().right(T::SP_10).bottom(T::SP_8).max_h(relative(0.9)).max_w(T::LOG_DOCK_MAX_W).overflow_hidden().flex().flex_col().justify_end().items_end().gap(T::SP_4);
    if cn + an > 0 {
        let mut b = div().flex().justify_end().gap(T::SP_6).occlude();
        if cn > 0 { b = b.child(badge(v, true, cn, cr, ct, list == Some(true), fs, cx)); }
        if an > 0 { b = b.child(badge(v, false, an, ar, at, list == Some(false), fs, cx)); }
        col = col.child(b);
    }
    // Its height, for the log's room at the end (after the frame).
    let me = cx.entity().downgrade();
    col = col.children(rest).child(canvas(move |b, _, cx| {
        let h = f32::from(b.size.height);
        let me = me.clone();
        cx.defer(move |cx| { let _ = me.update(cx, |v, cx| if (v.ui.agents.dock_h - h).abs() > 0.5 { v.ui.agents.dock_h = h; cx.notify(); }); });
    }, |_, _, _, _| {}).absolute().inset_0());
    Some(col.into_any_element())
}

/// The panel's entries: a command's own tool row from the chat, else the agent's transcript.
fn panel_entries(st: &crate::store::ChatStore, tid: &str) -> Vec<Shown> {
    let task = st.conv.agents.iter().find(|a| s(a, "toolUseId") == tid).is_some_and(agents::is_task);
    let raw: Vec<Value> = if task { st.conv.entries.iter().filter(|e| s(e, "toolUseId") == tid).cloned().collect() } else { st.conv.agent_msgs.get(tid).cloned().unwrap_or_default() };
    raw.iter().filter_map(Shown::of).collect()
}

/// The sheet's head: (back link), title, sub, (Dismiss all), ×.
fn head(back_link: Option<&'static str>, title: String, sub: String, all: Option<bool>, fs: f32, cx: &mut Context<LogView>) -> Div {
    div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_8).border_b_1().border_color(T::border_subtle())
        .when_some(back_link, |d, b| d.child(div().id("ag-back").flex_none().pr(T::SP_6).text_size(T::TEXT_12 * fs).text_color(T::text_muted()).cursor_pointer()
            .hover(|d| d.text_color(T::text())).tooltip(w::tooltip("Back to the list")).child(b).on_click(cx.listener(|v, _, _, cx| back(v, cx)))))
        .child(div().flex_none().max_w(relative(0.7)).truncate().text_size(T::TEXT_13 * fs).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(title))
        .child(div().flex_1().min_w_0().truncate().font_family(T::MONO).text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(sub))
        .when_some(all, |d, task| d.child(w::button("ag-dismiss-all", "Dismiss all").on_click(cx.listener(move |v, _, _, cx| dismiss_all(v, task, cx)))))
        .child(w::icon_button("ag-close", IconName::X, "Close").on_click(cx.listener(|v, _, _, cx| close(v, cx))))
}

/// One entry of a panel's transcript, as the log draws it.
fn entry_el(v: &mut LogView, e: &Shown, task: bool, fs: f32, cx: &mut Context<LogView>) -> Option<AnyElement> {
    Some(match e.kind {
        "user" => div().flex().justify_end().child(div().max_w(relative(0.82)).px(T::SP_12).py(T::SP_8).rounded(T::R_BUBBLE).bg(T::accent_dim()).border_1().border_color(T::accent_edge())
            .text_size(T::TEXT_13 * fs).line_height(relative(T::LH_CHAT)).child(e.text.clone())).into_any_element(),
        "assistant" => {
            let b = v.ui.md.blocks(&format!("agp-{}", e.id), &e.text);
            div().flex().flex_col().children(super::md::draw(&b, fs, T::text())).into_any_element()
        }
        "tool" => {
            // A command's own row, open; an agent's calls fold as in the log.
            let open = task || v.open.contains(&e.id);
            let k = e.id.clone();
            let mut c = w::call_card()
                .child(w::call_head(SharedString::from(format!("agt-{}", e.id)), super::tools::mark(e.status == "running", e.error, fs), Some((e.title.clone().into(), e.error)), super::tools::one_line(&e.detail), false, fs)
                    .child(w::fold_chevron(open, fs).text_color(T::text_muted()))
                    .on_click(cx.listener(move |v, _, _, cx| { if !v.open.remove(&k) { v.open.insert(k.clone()); } cx.notify(); })));
            if open { c = c.child(div().h(px(1.)).w_full().bg(T::border_subtle())).child(super::tools::body(e, fs)); }
            c.into_any_element()
        }
        "thinking" if !e.text.is_empty() => div().pl(T::SP_10).border_l_2().border_color(T::border()).italic().text_size(T::TEXT_12 * fs).text_color(T::text_muted()).child(e.text.clone()).into_any_element(),
        "error" => super::prose::error_box(fs).child(e.text.clone()).into_any_element(),
        "notice" => div().w_full().text_center().italic().text_size(T::TEXT_12 * fs).text_color(T::text_muted()).child(e.text.clone()).into_any_element(),
        "result" if e.error => super::prose::error_box(fs).child(if e.text.is_empty() { "Error".to_string() } else { e.text.clone() }).into_any_element(),
        "result" => div().w_full().border_t_1().border_color(T::border_subtle()).py(T::SP_4).text_center().text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(e.detail.clone()).into_any_element(),
        _ => return None,
    })
}

/// Over the log (inset 8): the open list or panel (None: neither).
pub fn sheet(v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let fs = v.fs();
    let frame = || div().id("ag-sheet").occlude().absolute().inset(T::SP_8).flex().flex_col().bg(T::bg()).border_1().border_color(T::border()).rounded(T::R_CARD).shadow_lg().overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    if let Some(task) = v.ui.agents.list {
        let l = list_of(&v.store.read(cx).conv.agents, task);
        let r = l.iter().filter(|a| agents::running(a)).count();
        let rows: Vec<AnyElement> = l.iter().map(|a| {
            let tid = s(a, "toolUseId");
            let (t1, t2) = (tid.clone(), tid.clone());
            let st = state_of(a);
            w::agent_list_row(SharedString::from(format!("ag-li-{tid}")), super::tools::mark(st == "running", st == "failed", fs), agents::label(a), agents::row_sub(a), st == "running", fs)
                .child(w::icon_button(SharedString::from(format!("ag-x-{tid}")), IconName::X, "Dismiss").on_click(cx.listener(move |v, _, _, cx| { cx.stop_propagation(); dismiss(v, vec![t2.clone()], cx); })))
                .on_click(cx.listener(move |v, _, _, cx| open(v, &t1, true, cx)))
                .into_any_element()
        }).collect();
        let foot = if task { "A finished command leaves this list after 5 minutes. Its output stays on its row in the chat. Dismissing one does not stop it." }
            else { "A finished agent leaves this list after 5 minutes. Its transcript stays on its Agent row in the chat." };
        return Some(frame()
            .child(head(None, (if task { "Commands" } else { "Agents" }).into(), agents::list_sub(r, l.len() - r), Some(task), fs, cx))
            .child(div().id("ag-list").flex_1().min_w_0().min_h_0().overflow_y_scroll().p(T::SP_6).child(div().flex().flex_col().children(rows)))
            .child(div().flex_none().px(T::SP_12).py(T::SP_8).border_t_1().border_color(T::border_subtle()).text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(foot))
            .into_any_element());
    }
    let (tid, from_list) = v.ui.agents.panel.clone()?;
    let a = v.store.read(cx).conv.agents.iter().find(|a| s(a, "toolUseId") == tid).cloned()?;
    let task = agents::is_task(&a);
    let entries = panel_entries(v.store.read(cx), &tid);
    // New entries: to the end when the reader was near it.
    if entries.len() != v.ui.agents.panel_n {
        let h = &v.ui.agents.panel_scroll;
        let near = f32::from(h.max_offset().y) + f32::from(h.offset().y) < f32::from(T::SP_40);
        if v.ui.agents.panel_n == 0 || near { v.ui.agents.panel_scroll.scroll_to_bottom(); }
        v.ui.agents.panel_n = entries.len();
    }
    let els: Vec<AnyElement> = entries.iter().filter_map(|e| entry_el(v, e, task, fs, cx)).collect();
    let prompt = s(&a, "prompt");
    let summary = s(&a, "summary");
    let sum: Vec<AnyElement> = if summary.is_empty() { vec![] } else { let b = v.ui.md.blocks(&format!("agsum-{tid}"), &summary); super::md::draw(&b, fs, T::text()) };
    let back = from_list.then_some(if task { "‹ Commands" } else { "‹ Agents" });
    let empty = els.is_empty().then(|| agents::empty_text(&a));
    Some(frame()
        .child(head(back, agents::label(&a), agents::panel_sub(&a), None, fs, cx))
        .child(div().id("ag-panel").flex_1().min_w_0().min_h_0().overflow_y_scroll().track_scroll(&v.ui.agents.panel_scroll)
            .child(div().flex().flex_col().gap(T::SP_10).px(T::SP_12).py(T::SP_10)
                .when(!prompt.is_empty(), |d| d.child(div().id("ag-prompt").max_h(T::COMPOSER_MAX).overflow_y_scroll().px(T::SP_8).py(T::SP_6)
                    .rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border_subtle())
                    .text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).child(prompt)))
                .children(sum)
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
        let ((cn, cr, _), (an, ar, at)) = badges(&l);
        assert_eq!((cn, cr, an, ar), (1, false, 1, true));
        assert_eq!(at, "1 agent, 1 running — click to see it");
    }
}
