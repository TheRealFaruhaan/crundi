//! A pane head's tags and its state badge (the web's .term-kind-tag "chat",
//! .term-plan-tag, .term-collab-tag, .term-factory-tag and
//! .term-agent-badge), and a head button sized to its label (the secrets
//! line of a narrow head's "…" menu). Added by Workbench (AREAS.md, "A new
//! control").

use crate::{head_button, HeadContent, HeadLook};
use crundi_theme::{IconName, T};
use gpui::{prelude::FluentBuilder as _, *};

/// kind ("chat": accent text, the border colour), the plan pill's states
/// (planning sky, proposed yellow, executing green, rejected muted),
/// a collaborator's name (sky) and a factory's Manager (accent).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeadTagLook { Kind, Planning, Proposed, Executing, Rejected, Collab, Factory }

/// A tag: 0.62rem bold in a 1px-bordered pill (padding 1 / 7). The kind and
/// plan tags are upper case, as the web's CSS sets them; a collaborator's
/// name is cut at 110px. `clickable`: a readable plan (the web's
/// data-action), with the hover wash. `tip`: "" for none.
pub fn head_tag(id: impl Into<ElementId>, text: impl Into<SharedString>, look: HeadTagLook, tip: impl Into<SharedString>, clickable: bool) -> Stateful<Div> {
    let (fg, edge) = match look {
        HeadTagLook::Kind => (T::accent_hover(), T::border()),
        HeadTagLook::Planning | HeadTagLook::Collab => (T::sky(), T::sky()),
        HeadTagLook::Proposed => (T::yellow(), T::yellow()),
        HeadTagLook::Executing => (T::green(), T::green()),
        HeadTagLook::Rejected => (T::text_muted(), T::border()),
        HeadTagLook::Factory => (T::accent(), T::accent()),
    };
    let text: SharedString = text.into();
    let upper = !matches!(look, HeadTagLook::Collab | HeadTagLook::Factory);
    let text: SharedString = if upper { text.to_uppercase().into() } else { text };
    let el = div().id(id).flex_none().flex().items_center().min_w_0().px(T::SP_6 + px(1.)).py(px(1.)).rounded(T::R_PILL)
        .border_1().border_color(edge).text_color(fg).text_size(T::TEXT_10).font_weight(T::W_BOLD).whitespace_nowrap()
        .when(look == HeadTagLook::Collab, |d| d.max_w(T::SP_48 + T::SP_48 + T::SP_12 + T::SP_2).overflow_hidden())
        .when(clickable, |d| d.cursor_pointer().hover(|s| s.bg(T::bg_hover())))
        .child(div().truncate().child(text));
    crate::tip(el, tip)
}

/// A chat's state badge beside its title (WORKING, NEEDS INPUT, WAITING):
/// the tag's pill in the badge's words, 11px bold in the UI font.
pub fn state_badge(text: impl Into<SharedString>, tone: &str) -> Div {
    crate::tag(text, tone).font_family(T::UI_FONT).text_size(T::TEXT_11).font_weight(T::W_BOLD)
}

/// A head button with an icon and a label, as wide as its label (a narrow
/// head's "…" menu: the lock and "Secrets for this chat").
pub fn head_label_button(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>, look: HeadLook, tip: impl Into<SharedString>) -> Stateful<Div> {
    head_button(id, HeadContent::IconText(icon, label.into()), look, px(0.), T::SP_20 + px(1.), tip).w_auto().px(T::SP_6)
}
