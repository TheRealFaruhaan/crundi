//! The running spinner (the web's .cc-spin): a small turning ring in the
//! accent, beside a running tool call or an asking side question. (Made by
//! operations; Lead to review.)
use crundi_theme::{IconName, T};
use gpui_component::{spinner::Spinner, Sizable as _, Size};

/// The spinner at icon size `size` (T::ICON_SM in a card's head), in the accent.
pub fn spinner(size: gpui::Pixels) -> Spinner {
    Spinner::new().icon(IconName::LoaderCircle).with_size(Size::Size(size)).color(T::accent())
}
