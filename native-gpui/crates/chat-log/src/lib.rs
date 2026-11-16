//! A chat's log (Operations', phase 2: 2.13-2.23; AREAS.md): the
//! conversation as the PC client draws it. The user's messages as bubbles on
//! the right, Claude's replies as markdown, thinking, each tool call as a
//! card that opens to its arguments and result, results, errors and notices.
//! The rows come from crundi_core::chat (`Conversation::view`, the Slint
//! client's rules).
//!
//! The foundation's version: the rows above, following the end while the
//! reader is at it. Questions, the full tool cards, thinking's shimmer,
//! media, suggestions, the agent dock, goal and plan, cards, side questions,
//! resume and parked panes are 2.13-2.23.

use crundi_chat_model::{ChatEvent, ChatModel};
use crundi_core::{chat::Shown, tools};
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::text::TextView;
use std::collections::HashSet;

pub struct LogView {
    model: Entity<ChatModel>,
    /// Tool cards the person opened, by entry id.
    open: HashSet<String>,
    scroll: ScrollHandle,
    /// Scroll to the end after the next layouts (new entries, the first history).
    stick: u8,
    _sub: Subscription,
}

impl LogView {
    pub fn new(model: Entity<ChatModel>, cx: &mut Context<Self>) -> Self {
        let sub = cx.subscribe(&model, |this, _, ev: &ChatEvent, cx| { if let ChatEvent::NewEntry = ev { this.stick = 3; } cx.notify(); });
        LogView { model, open: HashSet::new(), scroll: ScrollHandle::new(), stick: 3, _sub: sub }
    }

    fn row(&self, i: usize, e: &Shown, cx: &mut Context<Self>) -> Option<AnyElement> {
        let key = if e.id.is_empty() { format!("e{i}") } else { e.id.clone() };
        Some(match e.kind {
            "user" => div().flex().justify_end()
                .child(div().max_w(relative(0.82)).px(T::SP_12 + T::SP_2).py(T::SP_8).rounded(T::R_BUBBLE)
                    .bg(T::bg_tertiary()).border_1().border_color(T::accent_edge()).text_color(T::text()).child(e.text.clone()))
                .into_any_element(),
            "assistant" => div().text_color(T::text())
                .child(TextView::markdown(SharedString::from(format!("md-{key}")), e.text.clone()).selectable(true))
                .into_any_element(),
            "thinking" => div().text_color(T::text_muted()).italic().text_size(T::TEXT_12)
                .child(format!("Thinking  {}", e.text.chars().take(tools::SUMMARY_MAX).collect::<String>())).into_any_element(),
            "tool" => self.tool_card(&key, e, cx),
            "result" => { if e.text.is_empty() { return None; } div().w_full().text_center().text_size(T::TEXT_11).text_color(T::text_muted()).child(e.text.clone()).into_any_element() }
            "error" => w::card().px(T::SP_12).py(T::SP_6).border_color(T::red()).bg(T::red_dim()).text_color(T::error_text()).text_size(T::TEXT_12).child(e.text.clone()).into_any_element(),
            "notice" => div().w_full().text_center().text_size(T::TEXT_12).italic().text_color(T::text_muted()).child(e.text.clone()).into_any_element(),
            "permission" | "question" => w::card().px(T::SP_12).py(T::SP_8).gap(T::SP_4).text_size(T::TEXT_12)
                .child(div().font_weight(T::W_SEMIBOLD).child(if e.title.is_empty() { e.text.clone() } else { e.title.clone() }))
                .when(!e.detail.is_empty(), |d| d.child(div().font_family(T::MONO).text_color(T::text_secondary()).child(e.detail.clone())))
                .child(w::tag(e.status.clone(), if e.status == "pending" { "warn" } else { "" }))
                .into_any_element(),
            _ => return None,
        })
    }

    fn tool_card(&self, key: &str, e: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let dot = if e.error { T::red() } else if e.status == "running" { T::accent() } else { T::green() };
        let open = self.open.contains(key);
        let k = key.to_string();
        let head = div().id(SharedString::from(format!("th-{key}")))
            .flex().items_center().gap(T::SP_8).px(T::SP_12).py(T::SP_6).cursor_pointer()
            .child(w::dot(dot))
            .child(div().text_color(T::accent_hover()).font_weight(T::W_SEMIBOLD).child(e.title.clone()))
            .child(div().flex_1().text_color(T::text_muted()).truncate().child(e.detail.clone()))
            .child(div().text_color(T::text_muted()).child(if open { IconName::ChevronDown } else { IconName::ChevronRight }))
            .on_click(cx.listener(move |this, _, _, cx| { if !this.open.remove(&k) { this.open.insert(k.clone()); } cx.notify(); }));
        let mut card = w::card().text_size(T::TEXT_12).child(head);
        if open {
            let args = match &e.body { tools::Body::Bash { command, .. } => command.clone(), _ => e.args.clone() };
            card = card.child(div().flex().flex_col().gap(T::SP_8).px(T::SP_12).pb(T::SP_8).border_t_1().border_color(T::border())
                .child(div().pt(T::SP_8).font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text()).child(tools::truncate(&args, tools::ARGS_MAX)))
                .when(!e.text.is_empty(), |d| d.child(div().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(tools::truncate(&e.text, tools::RESULT_MAX)))));
        }
        card.into_any_element()
    }
}

use crundi_theme::IconName;

impl Render for LogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // To the end once the new content is laid out (its height is known a frame later).
        if self.stick > 0 {
            self.stick -= 1;
            self.scroll.scroll_to_bottom();
            let me = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| { let _ = me.update(cx, |_, cx| cx.notify()); });
        }
        let (rows, zoom): (Vec<Shown>, i32) = { let m = self.model.read(cx); (m.conv.view(), m.zoom) };
        let els: Vec<AnyElement> = rows.iter().enumerate().filter_map(|(i, e)| self.row(i, e, cx)).collect();
        div().id("log").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).text_size(T::TEXT_13 + px(zoom as f32))
            .child(div().flex().flex_col().gap(T::SP_10).p(T::SP_12).children(els))
    }
}
