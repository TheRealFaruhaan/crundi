//! The chat's own controls (claude-chat.js), each once, from tokens, so
//! every log, panel and sheet that shows one draws the same (F159):
//!
//! - `log_jump`: "Go to the newest message" (.cc-jump), round, its dot
//!   while something new arrived below.
//! - `log_copy`: the copy button by a code block or a message (.cc-copy),
//!   a tick once copied.
//! - `call_mark` and `call_head`: a tool call's or an agent's head row
//!   (.cc-tool-head, .cc-tgroup, .cc-agrow): the status mark, the mono
//!   name, the one-line summary, then what goes on the right (a chevron,
//!   "N tools", "transcript ›").
//! - `agent_list_row`: a row of the dock sheet's list of agents or commands.
//! - `dock_badge`: the floating dock's badges (.cc-agbadge): the icon, the
//!   mono count, a spinner while one runs, else a green dot.
//! - `pick_card`: a choice in a picker (the resume picker's .cr-item,
//!   .cr-new accented): the title, then whatever the caller adds under it.
//! - `suggest_caption` and `suggest_bubble`: the suggested next message
//!   (.cc-suggest): its caption and its dashed bubble.
//! - `id_link`: a short id that copies (the composer's session id, .cc-sid):
//!   mono, a dashed underline.
//!
//! `fs` is the pane's text scale (A- / A+).

use crate::{dot, icon, spinner};
use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};

/// "Go to the newest message": round, over the log's foot; `new` lights its
/// edge and puts the dot on it.
pub fn log_jump(id: impl Into<ElementId>, new: bool) -> Stateful<Div> {
    div().id(id).occlude().relative().size(T::LOG_JUMP).rounded_full().flex().items_center().justify_center().cursor_pointer()
        .bg(T::bg_secondary()).border_1().border_color(if new { T::accent() } else { T::border() }).shadow_lg().text_color(T::text())
        .hover(|d| d.border_color(T::accent()).bg(T::accent_dim_on_secondary()))
        .child(icon(IconName::ArrowDown, T::LOG_JUMP_ICON))
        .when(new, |d| d.child(div().absolute().top(-T::SP_2).right(-T::SP_2).size(T::SP_10).rounded_full().bg(T::accent()).border_2().border_color(T::bg())))
}

/// The copy button: a copy icon, a green tick once `done`.
pub fn log_copy(id: impl Into<ElementId>, done: bool) -> Stateful<Div> {
    div().id(id).occlude().w(T::LOG_COPY_W).h(T::LOG_COPY_H).flex().items_center().justify_center().cursor_pointer()
        .rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg_secondary()).shadow_md()
        .text_color(if done { T::green() } else { T::text_secondary() })
        .hover(move |d| { let d = d.bg(T::bg_hover()); if done { d } else { d.text_color(T::text()) } })
        .child(icon(if done { IconName::Check } else { IconName::Copy }, T::LOG_COPY_ICON))
}

/// A call's status mark: the spinner while running, else a dot (red: failed).
pub fn call_mark(running: bool, failed: bool, fs: f32) -> AnyElement {
    if running { spinner(T::ICON_SM * fs).into_any_element() }
    else { div().flex_none().w(T::ICON_SM * fs).flex().justify_center().child(dot(if failed { T::red() } else { T::green() }).size(T::SP_6)).into_any_element() }
}

/// A call's head row: the mark, the name (mono, accent; red when it
/// failed; none for a group), the summary on one line (eliding), then the
/// caller's right-hand parts (`.child(...)`). A click is the caller's.
pub fn call_head(id: impl Into<ElementId>, mark: AnyElement, name: Option<(SharedString, bool)>, summary: impl Into<SharedString>, strong: bool, fs: f32) -> Stateful<Div> {
    div().id(id).flex().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_6).cursor_pointer()
        .child(mark)
        .when_some(name, |d, (n, failed)| d.child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_12 * fs).font_weight(T::W_SEMIBOLD)
            .text_color(if failed { T::red() } else { T::accent_hover() }).child(n)))
        .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12 * fs).text_color(if strong { T::text() } else { T::text_secondary() }).child(summary.into()))
}

/// A head's quiet words on the right ("3 tools"), or its accent link ("transcript ›").
pub fn call_aside(text: impl Into<SharedString>, accent: bool, fs: f32) -> Div {
    div().flex_none().text_size(T::TEXT_11 * fs).text_color(if accent { T::accent_hover() } else { T::text_muted() }).child(text.into())
}

/// An agent's row in the log (.cc-agrow): a call head in a card that lights on hover.
pub fn agent_row(id: impl Into<ElementId>, mark: AnyElement, kind: impl Into<SharedString>, summary: impl Into<SharedString>, fs: f32) -> Stateful<Div> {
    call_head(id, mark, Some((kind.into(), false)), summary, false, fs)
        .rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border_subtle()).hover(|d| d.border_color(T::accent()))
        .child(call_aside("transcript ›", true, fs))
}

/// A row of the agents' or commands' list (the dock's sheet, .cc-agli): the
/// mark, the label over its mono line, a chevron; the caller adds its ×.
pub fn agent_list_row(id: impl Into<ElementId>, mark: AnyElement, label: impl Into<SharedString>, sub: impl Into<SharedString>, running: bool, fs: f32) -> Stateful<Div> {
    div().id(id).flex().items_center().gap(T::SP_8).p(T::SP_8).rounded(T::R_CONTROL).cursor_pointer().hover(|d| d.bg(T::bg_hover()))
        .child(div().flex_none().w(T::SP_12).flex().justify_center().child(mark))
        .child(div().flex_1().min_w_0().flex().flex_col().gap(T::SP_2)
            .child(div().truncate().text_size(T::TEXT_13 * fs).text_color(if running { T::text() } else { T::text_secondary() }).child(label.into()))
            .child(div().truncate().font_family(T::MONO).text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(sub.into())))
        .child(div().flex_none().text_color(T::text_muted()).child(icon(IconName::ChevronRight, T::ICON_SM)))
}

/// The card a call's head and body sit in (.cc-tool).
pub fn call_card() -> Div {
    crate::card().rounded(T::R_CONTROL).bg(T::bg_secondary()).border_color(T::border_subtle()).overflow_hidden()
}

/// A dock badge: `task` a background command's (terminal icon), else the
/// agents' (bot); `n` running or kept; `run` while one runs; `on` while its list is open.
pub fn dock_badge(id: impl Into<ElementId>, task: bool, n: usize, run: bool, on: bool, fs: f32) -> Stateful<Div> {
    div().id(id).flex().items_center().gap(T::SP_6).min_h(T::CONTROL_SM).pl(T::SP_8).pr(T::SP_10).py(T::SP_4)
        .rounded(T::R_PILL).bg(T::bg_secondary()).border_1().border_color(if on { T::accent() } else { T::border() }).shadow_lg()
        .text_size(T::TEXT_12 * fs).text_color(if on { T::text() } else { T::text_secondary() }).cursor_pointer()
        .hover(|d| d.border_color(T::accent()).text_color(T::text()))
        .child(div().flex_none().when(run, |d| d.text_color(T::accent_hover())).child(icon(if task { IconName::SquareTerminal } else { IconName::Bot }, T::ICON_MD * fs)))
        .child(div().font_family(T::MONO).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(n.to_string()))
        .child(if run { spinner(T::ICON_SM * fs).into_any_element() } else { dot(T::green()).size(T::SP_6).into_any_element() })
}

/// A picker's choice: the title (accent for the first, "New session"), and
/// what the caller adds under it; `focused` by the keys.
pub fn pick_card(id: impl Into<ElementId>, title: impl Into<SharedString>, accent: bool, focused: bool) -> Stateful<Div> {
    div().id(id).w_full().flex().flex_col().gap(T::SP_2).px(T::SP_12).py(T::SP_8).rounded(T::R_CONTROL).cursor_pointer()
        .bg(T::bg()).border_1().border_color(if accent { T::accent() } else { T::border() })
        .when(focused, |d| d.border_color(T::accent_hover()).bg(T::accent_dim()))
        .hover(|d| d.border_color(T::accent()).bg(T::accent_dim()))
        .child(div().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(if accent { T::accent_hover() } else { T::text() }).child(title.into()))
}

/// "Suggested, tap to send", over the bubble.
pub fn suggest_caption(fs: f32) -> Div {
    div().mr(T::SP_4).px(T::SP_4).rounded(T::R_CHIP).bg(T::bg()).text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child("Suggested, tap to send")
}

/// The suggestion's bubble: dashed, its corner by the box squared; the
/// caller adds the words and its ×.
pub fn suggest_bubble(id: impl Into<ElementId>, fs: f32) -> Stateful<Div> {
    div().id(id).occlude().relative().max_w_full().pl(T::SP_12).pr(T::SP_24 + T::SP_6).py(T::SP_8)
        .rounded_tl(T::R_BUBBLE).rounded_tr(T::R_BUBBLE).rounded_bl(T::R_BUBBLE).rounded_br(T::R_CHIP)
        .border_1().border_dashed().border_color(T::accent_edge()).bg(T::bg_secondary()).shadow_lg()
        .text_size(T::TEXT_13 * fs).text_color(T::text_secondary()).line_height(relative(T::LH_CHAT)).cursor_pointer()
        .hover(|d| d.border_color(T::accent()).bg(T::accent_dim_on_secondary()).text_color(T::text()))
}

/// A short id that copies on a click: mono, a dashed underline, brighter on hover.
pub fn id_link(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).font_family(T::MONO).cursor_pointer().border_b_1().border_dashed().border_color(T::border())
        .hover(|d| d.text_color(T::text_secondary())).child(text.into())
}
