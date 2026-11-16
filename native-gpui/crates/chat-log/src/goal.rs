//! 2.19: the goal bar (cc-goalbar) and the plan (specs/goal-plan.md).
//!
//! The goal (`conv.goal`: {condition, verdicts, looping}) is a bar between
//! the log and the composer: GOAL (a spinner while looping), the condition,
//! the count, "clear" (sends `/goal clear`), and the stopped line. Each check
//! that found it not yet met is a verdict card in the log (`goal-verdict`).
//! The plan (`conv.plan`: {text, status}) has its PLAN pill (the pane
//! header's, `plan_pill`) and a panel over the log that the pill toggles;
//! the plan going closes it.
use crate::{rows::{split_blocks, Row}, suggest, LogView};
use crundi_core::chat::Shown;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};

#[derive(Default)]
pub struct State {
    /// The plan panel is open (read only while there is a plan).
    pub plan_open: bool,
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// The bar's count: "1 check" / "N checks"; else "running" (looping) or "set".
pub fn count(verdicts: u64, looping: bool) -> String {
    if verdicts > 0 { format!("{verdicts} check{}", if verdicts == 1 { "" } else { "s" }) } else if looping { "running".into() } else { "set".into() }
}

/// The plan's status as the pill names it ("proposed" when none is given).
pub fn plan_status(plan: &Value) -> String { let st = s(plan, "status"); if st.is_empty() { "proposed".into() } else { st } }

/// The plan panel's status line.
pub fn plan_state(status: &str) -> &'static str {
    match status { "executing" => "approved — being worked through", "rejected" => "sent back for revision", _ => "waiting for your approval" }
}

/// The pill's tooltip.
pub fn pill_tip(status: &str) -> &'static str {
    match status { "executing" => "Approved plan — click to read it", "rejected" => "Plan sent back for revision — click to read it", _ => "Plan awaiting your approval — click to read it" }
}

fn goal_parts(g: &Value) -> (u64, bool) {
    (g.get("verdicts").and_then(|v| v.as_u64()).unwrap_or(0), g.get("looping").and_then(|v| v.as_bool()).unwrap_or(false))
}

fn open(v: &LogView, cx: &App) -> bool { v.st.goal.plan_open && v.model.read(cx).conv.plan.is_some() }

/// This feature's fields in the chat's props: the Slint client's `goal`.
pub fn props(v: &LogView, cx: &App, o: &mut Map<String, Value>) {
    let m = v.model.read(cx);
    let (on, cond, cnt, looping, stopped) = match &m.conv.goal {
        Some(g) => { let (n, l) = goal_parts(g); (true, s(g, "condition"), count(n, l), l, !l && n > 0) }
        None => (false, String::new(), String::new(), false, false),
    };
    let op = open(v, cx);
    let (plan, state, blocks) = match &m.conv.plan {
        Some(p) => { let st = plan_status(p); (st.clone(), plan_state(&st), if op { split_blocks("plan", &s(p, "text")).len() } else { 0 }) }
        None => (String::new(), "", 0),
    };
    o.insert("goal".into(), json!({ "on": on, "condition": cond, "count": cnt, "looping": looping, "stopped": stopped,
        "plan": plan, "planOpen": op, "planState": state, "planBlocks": blocks }));
}

/// The PLAN pill (and the panel's Close): the panel opens or closes, never without a plan.
pub fn toggle(v: &mut LogView, cx: &mut Context<LogView>) {
    let has = v.model.read(cx).conv.plan.is_some();
    v.st.goal.plan_open = has && !v.st.goal.plan_open;
    cx.notify();
}

/// "clear": `/goal clear` through the send path (the box untouched).
pub fn clear(v: &mut LogView, cx: &mut Context<LogView>) {
    v.stick = 3;
    suggest::post_send(&v.model, "/goal clear".into(), cx);
}

/// This feature's driver actions (Some(answer) for a name it knows).
pub fn act(name: &str, _args: &[String], v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    match name {
        "plan-toggle" => toggle(v, cx),
        "goal-clear" => clear(v, cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// GOAL / PLAN: the mono tag of the bar and the panel.
fn tag(text: &'static str, z: Pixels) -> Div {
    div().flex_none().font_family(T::MONO).text_size(T::TEXT_10 + z).font_weight(T::W_BOLD).text_color(T::accent_hover()).child(text)
}

/// Between the log and the composer: the goal bar (None: no goal).
pub fn view(v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let m = v.model.read(cx);
    let g = m.conv.goal.clone()?;
    let z = px(m.zoom as f32);
    let (n, looping) = goal_parts(&g);
    Some(div().flex().flex_col().flex_none().px(T::SP_10).py(T::SP_6).border_t_1().border_color(T::border_subtle()).bg(T::accent_dim()).text_size(T::TEXT_12 + z)
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().flex().items_center().gap(T::SP_4).when(looping, |d| d.child(w::spinner(T::ICON_SM))).child(tag("GOAL", z)))
            .child(div().flex_1().min_w_0().truncate().text_color(T::text()).child(s(&g, "condition")))
            .child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_10 + z).text_color(T::text_muted()).child(count(n, looping)))
            .child(w::button("goal-clear", "clear").tooltip("Stop working towards this goal (/goal clear)").on_click(cx.listener(|v, _, _, cx| clear(v, cx)))))
        .when(!looping && n > 0, |d| d.child(div().mt(T::SP_4).text_color(T::text_muted()).child("The goal loop has stopped. Send /goal to ask for its current verdict.")))
        .into_any_element())
}

/// Over the log: the plan panel while open (None: closed or no plan).
pub fn panel(v: &mut LogView, _window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    if !open(v, cx) { return None; }
    let m = v.model.read(cx);
    let p = m.conv.plan.clone()?;
    let z = px(m.zoom as f32);
    let st = plan_status(&p);
    let blocks: Vec<AnyElement> = split_blocks("plan", &s(&p, "text")).into_iter().filter_map(|r| match r {
        Row::Prose(id, t, kind, marker, last) => Some(crate::prose::block(&id, &t, kind, &marker, last, cx)),
        Row::Code(id, c, last) => Some(crate::prose::code(v, &id, &c, last, cx)),
        _ => None,
    }).collect();
    Some(div().absolute().inset_0().flex().flex_col().bg(T::bg()).occlude()
        .child(div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_6).bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .child(tag("PLAN", z))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_11 + z).text_color(T::text_muted()).child(plan_state(&st)))
            .child(w::button("plan-close", "Close").on_click(cx.listener(|v, _, _, cx| { v.st.goal.plan_open = false; cx.notify(); }))))
        .child(div().id("plan-body").flex_1().min_h_0().overflow_y_scroll()
            .child(div().flex().flex_col().gap(T::SP_6).px(T::SP_16).py(T::SP_12).children(blocks)))
        .into_any_element())
}

/// The PLAN pill for the pane header (2.19, the header is Workspace's:
/// shared/requests.md): coloured by the plan's status, a click toggles the
/// panel. None: the chat has no plan (or its log isn't open).
pub fn plan_pill(cx: &mut App, chat: &str) -> Option<AnyElement> {
    let v = crate::view_of(cx, chat)?;
    let st = plan_status(v.read(cx).model.read(cx).conv.plan.as_ref()?);
    let (fg, edge) = match st.as_str() { "executing" => (T::green(), T::green()), "rejected" => (T::text_muted(), T::border()), _ => (T::yellow(), T::yellow()) };
    Some(div().id(SharedString::from(format!("plan-pill-{chat}"))).flex_none().px(T::SP_6).rounded(T::R_PILL).border_1().border_color(edge)
        .text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(fg).cursor_pointer().hover(|d| d.bg(T::bg_hover()))
        .tooltip(w::tooltip(pill_tip(&st))).child("PLAN")
        .on_click(move |_, _, cx| { v.update(cx, |v, cx| toggle(v, cx)); })
        .into_any_element())
}

/// A check that found the goal not yet met: its card in the log.
pub fn verdict(e: &Shown) -> AnyElement {
    div().flex().flex_col().gap(T::SP_2).px(T::SP_10).py(T::SP_6).border_l_2().border_color(T::accent()).bg(T::bg_secondary()).rounded_r(T::R_CONTROL)
        .child(div().flex().items_center().gap(T::SP_6).text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::accent_hover())
            .child(w::dot(T::accent()).size(T::SP_6)).child(e.detail.clone()))
        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(e.text.clone()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn wording() {
        assert_eq!(count(0, true), "running");
        assert_eq!(count(0, false), "set");
        assert_eq!(count(1, false), "1 check");
        assert_eq!(count(3, true), "3 checks");
        assert_eq!(plan_status(&json!({ "text": "x" })), "proposed");
        assert_eq!(plan_state("proposed"), "waiting for your approval");
        assert_eq!(plan_state("executing"), "approved — being worked through");
        assert_eq!(plan_state("rejected"), "sent back for revision");
        assert_eq!(pill_tip("rejected"), "Plan sent back for revision — click to read it");
    }
}
