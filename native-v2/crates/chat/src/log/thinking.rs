//! 2.15: thinking blocks (the web's thinkingNode): with no text, the flat
//! "Thought" line (a dot, "Thought for ~640 tokens"); else a bar at the left,
//! the disclosure ("Thinking", shimmering while it streams) and, open, its
//! text (muted italics, at most 260 px tall then scrolling, its caret while
//! it streams). When a block is open is the chat's (shared.rs: the web's
//! thinkUser / thinkAuto rules, crundi_core::think), the same in every log.

use super::LogView;
use crate::store::Row;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};


pub fn row(v: &mut LogView, r: &Row, fs: f32, cx: &mut Context<LogView>) -> AnyElement {
    let e = &r.shown;
    let size = T::TEXT_12 * fs;
    if e.text.is_empty() {
        return div().flex().items_center().gap(T::SP_6)
            .child(w::dot(T::text_muted()).size(T::SP_6).opacity(0.55))
            .child(div().italic().text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_muted()).child(e.detail.clone()))
            .into_any_element();
    }
    let open = v.store.read(cx).log.think.open(&e.id);
    let live = e.status == "streaming";
    let label: AnyElement = if live { w::shimmer(SharedString::from(format!("thl-{}", e.id)), "Thinking").into_any_element() } else { div().child("Thinking").into_any_element() };
    let id = e.id.clone();
    let store = v.store.clone();
    let head = w::disclosure_at(SharedString::from(format!("thh-{}", e.id)), label, open, fs).line_height(relative(T::LH_CHAT))
        .on_click(move |_, _, cx| super::shared::think_toggle(&store, &id, cx));
    let text = if live { format!("{}\u{258e}", e.text) } else { e.text.clone() };
    let body = div().id(SharedString::from(format!("thb-{}", e.id))).mt(T::SP_4).max_h(T::LOG_THINK_MAX).overflow_y_scroll()
        .italic().text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_muted())
        .child(text);
    div().flex()
        .child(div().flex_none().w(T::SP_2).bg(T::border()))
        .child(div().flex().flex_col().flex_1().min_w_0().pl(T::SP_10 - T::SP_2).child(head).when(open, |d| d.child(body)))
        .into_any_element()
}
