//! A flat Lucide icon at a given size, in the current text colour (a card's
//! caret, a to-do's box). (Made by operations; Lead to review.)
use crundi_theme::IconName;
use gpui::{Pixels, Styled as _};
use gpui_component::Icon;

pub fn icon(name: IconName, size: Pixels) -> Icon { Icon::new(name).size(size) }
