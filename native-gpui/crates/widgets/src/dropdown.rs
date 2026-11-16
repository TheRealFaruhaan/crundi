//! A dropdown (the web's <select>): a quiet button showing the current
//! choice and a chevron, opening a list of the choices with the current one
//! ticked; a click picks one, Esc or a press outside closes it. Disabled:
//! dimmed and not clickable. (The secret-request dock's "For how long",
//! 2.12b.)

use crundi_theme::{IconName, T};
use gpui::*;
use gpui_component::{button::Button, menu::{DropdownMenu as _, PopupMenuItem}, Disableable as _, Sizable as _};

pub fn dropdown(
    id: impl Into<ElementId>,
    items: &[&'static str],
    on: usize,
    disabled: bool,
    pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let items: Vec<&'static str> = items.to_vec();
    let label = items.get(on).copied().unwrap_or("");
    let pick = std::rc::Rc::new(pick);
    Button::new(id).label(label).icon(IconName::ChevronDown).small().text_size(T::TEXT_12).disabled(disabled)
        .dropdown_menu(move |mut menu, _, _| {
            for (i, l) in items.iter().enumerate() {
                let pick = pick.clone();
                menu = menu.item(PopupMenuItem::new(*l).checked(i == on).on_click(move |_, window, cx| pick(i, window, cx)));
            }
            menu
        })
}
