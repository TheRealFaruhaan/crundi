//! A block above the message box holding text that waits (the queue's two
//! drawers, `.cc-queue`): a head (a label in capitals and a note), the text
//! (at most four lines, "…" at the cut) and a hint. `held`: dashed-accent
//! look (accent-haze, accent-line border; hover accent-dim); else the
//! handed-over look (slate-haze, slate-edge). `scale`: the pane's text scale.

use crate::tip;
use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};

pub struct Drawer { pub held: bool, pub label: SharedString, pub note: SharedString, pub body: SharedString, pub hint: SharedString, pub tip: SharedString, pub clickable: bool }

pub fn queue_drawer(id: impl Into<ElementId>, d: Drawer, scale: f32) -> Stateful<Div> {
    let (fill, edge, label_c, body_c) = if d.held { (T::composer_accent_haze(), T::accent_edge(), T::accent_hover(), T::text()) } else { (T::composer_slate_haze(), T::composer_slate_edge(), T::text_muted(), T::text_secondary()) };
    let el = div().id(id).flex().flex_col().w_full().px(T::SP_12).py(T::SP_8).rounded(T::R_CARD).border_1().border_color(edge).bg(fill)
        .when(d.held, |x| x.border_dashed().hover(|x| x.bg(T::accent_dim()).border_color(T::accent())))
        .when(d.clickable, |x| x.cursor_pointer())
        .child(div().flex().items_center().gap(T::SP_6).mb(T::SP_2)
            .child(div().flex_none().text_size(T::TEXT_10 * scale).font_weight(T::W_BOLD).text_color(label_c).child(d.label.to_uppercase()))
            .child(div().flex_1().min_w_0().text_size(T::TEXT_10 * scale).font_weight(T::W_MEDIUM).text_color(label_c.opacity(0.7)).child(d.note)))
        .child(div().text_size(T::TEXT_12 * scale).text_color(body_c).line_clamp(4).text_ellipsis().child(d.body))
        .child(div().mt(T::SP_4).text_size(T::TEXT_10 * scale).text_color(T::text_muted()).child(d.hint));
    tip(el, d.tip)
}
