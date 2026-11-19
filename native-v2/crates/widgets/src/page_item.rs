//! A page in a list (the web's .nt-item, the Slint client's notes PageItem):
//! its title, a two-line preview, its meta line ("Edited 5m ago"). `cur` the
//! page open beside the list (accent); `busy` open in another pane (faded,
//! not clickable); `quiet` the tab's list (no card until hovered). A
//! Stateful<Div>: the caller adds the click and any row under it (the
//! trash's Restore / Delete for good). Planning's request (shared/requests.md).

use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};

pub fn page_item(id: impl Into<ElementId>, title: impl Into<SharedString>, preview: impl Into<SharedString>, meta: impl Into<SharedString>, cur: bool, busy: bool, quiet: bool, clickable: bool) -> Stateful<Div> {
    let preview: SharedString = preview.into();
    div()
        .id(id).flex().flex_col().flex_none().gap(T::SP_2).px(T::SP_10).py(T::SP_8).rounded(T::R_FIELD).border_1()
        .border_color(if cur { T::accent() } else { gpui::transparent_black() })
        .when(cur, |d| d.bg(T::accent_dim()))
        .when(!cur && !quiet, |d| d.bg(T::bg_card()))
        .when(!cur && clickable && !busy, |d| d.cursor_pointer().hover(move |s| if quiet { s.bg(T::bg_tertiary()) } else { s.border_color(T::border()) }))
        .when(busy, |d| d.opacity(T::NOTES_O_SOFT).cursor_not_allowed())
        .child(div().min_w_0().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(title.into()))
        .when(!preview.is_empty(), |d| d.child(div().text_size(T::TEXT_12).line_height(relative(T::LH_UI)).text_color(T::text_secondary()).min_w_0().overflow_hidden().line_clamp(2).child(preview)))
        .child(div().min_w_0().text_size(T::TEXT_11).text_color(T::text_muted()).child(meta.into()))
}
