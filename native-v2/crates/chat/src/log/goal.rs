//! 2.19: the goal bar (`.cc-goalbar`, the web's renderGoal) and the plan
//! (renderPlan, togglePlan).
//!
//! The goal (`conv.goal`: {condition, verdicts, looping}) is a bar under the
//! log, above the composer: GOAL (a spinner while looping), the condition,
//! the count, "clear" (sends `/goal clear`), and the stopped line. Each check
//! that found it not yet met is a card in the log (`goal-verdict`). The plan
//! (`conv.plan`: {text, status}) has its PLAN pill in the pane's head and a
//! panel over the log that the pill toggles (`toggle`; the head's pill is
//! the workbench's to draw: `plan_pill`); the plan going closes it.

use super::LogView;
use crate::store::Row;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

#[derive(Default)]
pub struct View { pub plan_open: bool }

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// The bar's count: "1 check" / "N checks"; else "running" (looping) or "set".
pub fn count(verdicts: u64, looping: bool) -> String {
    if verdicts > 0 { format!("{verdicts} check{}", if verdicts == 1 { "" } else { "s" }) } else if looping { "running".into() } else { "set".into() }
}

/// The plan's status ("proposed" when none is given).
pub fn status_of(plan: &Value) -> String { let st = s(plan, "status"); if st.is_empty() { "proposed".into() } else { st } }

/// The plan panel's status line.
pub fn plan_state(status: &str) -> &'static str {
    match status { "executing" => "approved — being worked through", "rejected" => "sent back for revision", _ => "waiting for your approval" }
}

/// The pill's tooltip.
pub fn pill_tip(status: &str) -> &'static str {
    match status { "executing" => "Approved plan — click to read it", "rejected" => "Plan sent back for revision — click to read it", _ => "Plan awaiting your approval — click to read it" }
}

fn parts(g: &Value) -> (u64, bool) { (g.get("verdicts").and_then(|v| v.as_u64()).unwrap_or(0), g.get("looping").and_then(|v| v.as_bool()).unwrap_or(false)) }

fn open(v: &LogView, cx: &App) -> bool { v.ui.goal.plan_open && v.store.read(cx).conv.plan.is_some() }

/// The PLAN pill (and the panel's Close): the panel opens or closes, never without a plan.
pub fn toggle(v: &mut LogView, cx: &mut Context<LogView>) {
    let has = v.store.read(cx).conv.plan.is_some();
    v.ui.goal.plan_open = has && !v.ui.goal.plan_open;
    cx.notify();
}

/// "clear": `/goal clear` through the send path (the box untouched).
pub fn clear(v: &mut LogView, cx: &mut Context<LogView>) {
    v.list.set_follow_mode(FollowMode::Tail);
    super::suggest::post_send(&v.store, "/goal clear".into(), cx);
}

pub fn state(v: &LogView, cx: &App) -> Value {
    let st = v.store.read(cx);
    let (on, cond, cnt, looping, stopped) = match &st.conv.goal {
        Some(g) => { let (n, l) = parts(g); (true, s(g, "condition"), count(n, l), l, !l && n > 0) }
        None => (false, String::new(), String::new(), false, false),
    };
    let plan = st.conv.plan.as_ref().map(status_of).unwrap_or_default();
    json!({ "on": on, "condition": cond, "count": cnt, "looping": looping, "stopped": stopped, "plan": plan, "planOpen": open(v, cx), "planState": if plan.is_empty() { "" } else { plan_state(&plan) } })
}

pub fn act(name: &str, v: &mut LogView, cx: &mut Context<LogView>) -> Option<Value> {
    match name { "plan-toggle" => toggle(v, cx), "goal-clear" => clear(v, cx), _ => return None }
    Some(json!({ "ok": true }))
}

/// GOAL / PLAN: the mono tag of the bar and the panel.
fn tag(text: &'static str, fs: f32) -> Div {
    div().flex_none().font_family(T::MONO).text_size(T::TEXT_10 * fs).font_weight(T::W_BOLD).text_color(T::accent_hover()).child(text)
}

/// Under the log, above the composer: the goal bar (None: no goal).
pub fn bar(v: &mut LogView, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let fs = v.fs();
    let g = v.store.read(cx).conv.goal.clone()?;
    let (n, looping) = parts(&g);
    Some(div().flex().flex_col().flex_none().px(T::SP_10).py(T::SP_6).border_t_1().border_color(T::border_subtle()).bg(T::accent_dim()).text_size(T::TEXT_12 * fs)
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().flex().items_center().gap(T::SP_4).when(looping, |d| d.child(w::spinner(T::ICON_SM * fs))).child(tag("GOAL", fs)))
            .child(div().flex_1().min_w_0().truncate().text_color(T::text()).child(s(&g, "condition")))
            .child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_10 * fs).text_color(T::text_muted()).child(count(n, looping)))
            .child(w::button("goal-clear", "clear").tooltip("Stop working towards this goal (/goal clear)").on_click(cx.listener(|v, _, _, cx| clear(v, cx)))))
        .when(!looping && n > 0, |d| d.child(div().mt(T::SP_4).text_color(T::text_muted()).child("The goal loop has stopped. Send /goal to ask for its current verdict.")))
        .into_any_element())
}

/// Over the log: the plan panel while open (None: closed or no plan).
pub fn panel(v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    if !open(v, cx) { return None; }
    let fs = v.fs();
    let p = v.store.read(cx).conv.plan.clone()?;
    let st = status_of(&p);
    let blocks = v.ui.md.blocks("plan", &s(&p, "text"));
    Some(div().id("plan").occlude().absolute().inset_0().flex().flex_col().bg(T::bg())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_6).bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .child(tag("PLAN", fs))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(plan_state(&st)))
            .child(w::button("plan-close", "Close").on_click(cx.listener(|v, _, _, cx| { v.ui.goal.plan_open = false; cx.notify(); }))))
        .child(div().id("plan-body").flex_1().min_w_0().min_h_0().overflow_y_scroll()
            .child(div().flex().flex_col().px(T::SP_16).py(T::SP_12).children(super::md::draw(&blocks, fs, T::text()))))
        .into_any_element())
}

/// The head's copy of the plan: (its status, the permission mode) while the
/// chat has a plan with text (the web's renderPlan `has`), kept in the
/// chat's meta (store.rs) so the head observes it alone.
pub fn plan_meta(conv: &crundi_core::chat::Conversation) -> Option<(String, String)> {
    let p = conv.plan.as_ref().filter(|p| !s(p, "text").is_empty())?;
    Some((status_of(p), conv.mode.clone()))
}

/// The plan for the pane's head (the Architect's hook): (status, permission
/// mode), None without a plan. Cheap: read from the chat's meta entity
/// (`ChatStore::meta`), which the head can observe.
pub fn plan_status(chat: &str, cx: &App) -> Option<(String, String)> {
    crate::find(cx, chat).and_then(|s| s.read(cx).meta.read(cx).0.plan.clone())
}

/// The head's PLAN pill clicked (the web's togglePlan): the chat's pane log
/// opens or closes its plan panel.
pub fn open_plan(chat: &str, _window: &mut Window, cx: &mut App) {
    if let Some(log) = crate::log_of(cx, chat) { log.update(cx, |v, cx| toggle(v, cx)); }
}

/// The PLAN pill for the pane's head (coloured by the plan's status; a click
/// toggles the panel). None: the chat has no plan. For the workbench's head
/// to draw once it has a place for a kind's own element there.
pub fn plan_pill(log: &Entity<LogView>, cx: &App) -> Option<AnyElement> {
    let v = log.read(cx);
    let st = status_of(v.store.read(cx).conv.plan.as_ref()?);
    let (fg, edge) = match st.as_str() { "executing" => (T::green(), T::green()), "rejected" => (T::text_muted(), T::border()), _ => (T::yellow(), T::yellow()) };
    let log = log.clone();
    Some(w::tone_pill("plan-pill", "PLAN", fg, edge)
        .tooltip(w::tooltip(pill_tip(&st)))
        .on_click(move |_, _, cx| log.update(cx, |v, cx| toggle(v, cx)))
        .into_any_element())
}

/// A check that found the goal not yet met: its card in the log.
pub fn verdict(r: &Row, fs: f32) -> AnyElement {
    let e = &r.shown;
    div().flex().flex_col().gap(T::SP_2).px(T::SP_10).py(T::SP_6).border_l_2().border_color(T::accent()).bg(T::bg_secondary()).rounded_r(T::R_CONTROL)
        .child(div().flex().items_center().gap(T::SP_6).text_size(T::TEXT_11 * fs).font_weight(T::W_SEMIBOLD).text_color(T::accent_hover())
            .child(w::dot(T::accent()).size(T::SP_6)).child(e.detail.clone()))
        .child(div().text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).child(e.text.clone()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn wording() {
        assert_eq!(count(0, true), "running");
        assert_eq!(count(1, false), "1 check");
        assert_eq!(count(3, true), "3 checks");
        assert_eq!(status_of(&json!({ "text": "x" })), "proposed");
        assert_eq!(plan_state("executing"), "approved — being worked through");
    }
}
