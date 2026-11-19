//! A select whose choices are known only at run time (the web's <select>
//! filled from the server: Cru's models, the skills' scopes): the
//! `dropdown`'s look (a quiet button with the current choice and a chevron,
//! a menu with the current one ticked), over owned labels. `on` out of
//! range shows `none`. (Made by operations; Lead to review.)

use crundi_theme::{IconName, T};
use gpui::*;
use gpui_component::{button::Button, menu::{DropdownMenu as _, PopupMenuItem}, Disableable as _};

pub fn select(
    id: impl Into<ElementId>,
    items: Vec<SharedString>,
    on: Option<usize>,
    none: impl Into<SharedString>,
    disabled: bool,
    pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = on.and_then(|i| items.get(i).cloned()).unwrap_or_else(|| none.into());
    let pick = std::rc::Rc::new(pick);
    crate::control::sized(Button::new(id), Some(label), Some(IconName::ChevronDown), true, crate::control::Ctl::Normal).disabled(disabled)
        .dropdown_menu(move |mut menu, _, _| {
            menu = menu.max_h(T::DIALOG_MAX_H).scrollable(true);
            for (i, l) in items.iter().enumerate() {
                let pick = pick.clone();
                menu = menu.item(PopupMenuItem::new(l.clone()).checked(Some(i) == on).on_click(move |_, window, cx| pick(i, window, cx)));
            }
            menu
        })
}
