//! A tag with an icon before its text (a factory's skill mentions: a check
//! when the machine has the skill, an alert when not). The same look as `tag`.

use crundi_theme::{IconName, T};
use gpui::*;
use gpui_component::Icon;

pub fn icon_tag(icon: IconName, text: impl Into<SharedString>, tone: &str) -> Div {
    let (fg, bg) = match tone {
        "running" | "ok" => (T::green(), T::green_dim()),
        "error" => (T::red(), T::red_dim()),
        "warn" => (T::yellow(), T::yellow_dim()),
        "accent" => (T::accent_hover(), T::accent_dim()),
        "sky" => (T::sky(), T::sky_dim()),
        _ => (T::text_muted(), T::bg_tertiary()),
    };
    div().flex_none().flex().items_center().gap(T::SP_2).px(T::SP_8).rounded(T::R_PILL).bg(bg).text_color(fg).text_size(T::TEXT_10).font_family(T::MONO)
        .child(Icon::new(icon).size(T::ICON_SM).text_color(fg)).child(text.into())
}
