//! A dropdown (the web's <select>): a quiet button showing the current
//! choice and a chevron, opening a list of the choices with the current one
//! ticked; a click picks one, Esc or a press outside closes it. Disabled:
//! dimmed and not clickable. (The secret-request dock's "For how long",
//! 2.12b.)

use crundi_theme::{IconName, T};
use gpui::*;
use gpui_component::{button::Button, menu::{DropdownMenu as _, PopupMenuItem}, Disableable as _};

pub fn dropdown(
    id: impl Into<ElementId>,
    items: &[&'static str],
    on: usize,
    disabled: bool,
    pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    dropdown_of(id, items.iter().map(|s| SharedString::from(*s)).collect(), on, disabled, false, pick)
}

/// The same dropdown over choices known only at run time (a schedule's
/// projects, a board's tasks); `full`: as wide as its parent, the label
/// left and the chevron right (a form's select). Added by Planning
/// (schedules, 4.5); `dropdown` is this with fixed items.
pub fn dropdown_of(
    id: impl Into<ElementId>,
    items: Vec<SharedString>,
    on: usize,
    disabled: bool,
    full: bool,
    pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    dropdown_off(id, items, &[], on, disabled, full, pick)
}

/// The same, with some choices shown but not pickable (`off[i]`: the web's
/// `<option disabled>`, a draft factory in the start form). Added by
/// Planning (Factories, F92).
pub fn dropdown_off(
    id: impl Into<ElementId>,
    items: Vec<SharedString>,
    off: &[bool],
    on: usize,
    disabled: bool,
    full: bool,
    pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = items.get(on).cloned().unwrap_or_default();
    choices(id, label, items, off.to_vec(), Some(on), disabled, full, pick)
}

/// The one <select> every form uses (`dropdown*` and `select`): the quiet
/// button with `label` and a chevron, the menu of `items` with `on` ticked,
/// `off[i]` shown but not pickable.
#[allow(clippy::too_many_arguments)]
pub(crate) fn choices(
    id: impl Into<ElementId>,
    label: SharedString,
    items: Vec<SharedString>,
    off: Vec<bool>,
    on: Option<usize>,
    disabled: bool,
    full: bool,
    pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let pick = std::rc::Rc::new(pick);
    let b = crate::control::sized(Button::new(id), Some(label), Some(IconName::ChevronDown), true, crate::control::Ctl::Normal).disabled(disabled);
    let b = if full { b.w_full().justify_between() } else { b };
    b.dropdown_menu(move |mut menu, _, _| {
        menu = menu.max_h(T::DIALOG_MAX_H).scrollable(true);
        for (i, l) in items.iter().enumerate() {
            let pick = pick.clone();
            menu = menu.item(PopupMenuItem::new(l.clone()).checked(Some(i) == on).disabled(off.get(i).copied().unwrap_or(false)).on_click(move |_, window, cx| pick(i, window, cx)));
        }
        menu
    })
}

/// A dropdown whose choices come in named groups (a <select> with
/// <optgroup>s: the Browsers tab's devices by maker). `on` and `pick` count
/// the choices in order across the groups; the button shows the choice.
pub fn dropdown_groups(
    id: impl Into<ElementId>,
    groups: Vec<(SharedString, Vec<SharedString>)>,
    on: usize,
    full: bool,
    pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = groups.iter().flat_map(|g| g.1.iter()).nth(on).cloned().unwrap_or_default();
    let pick = std::rc::Rc::new(pick);
    let b = crate::control::sized(Button::new(id), Some(label), Some(IconName::ChevronDown), true, crate::control::Ctl::Normal);
    let b = if full { b.w_full().justify_between() } else { b };
    b.dropdown_menu(move |mut menu, _, _| {
        menu = menu.max_h(T::DIALOG_MAX_H).scrollable(true);
        let mut i = 0;
        for (g, items) in &groups {
            menu = menu.label(g.clone());
            for l in items {
                let (pick, n) = (pick.clone(), i);
                menu = menu.item(PopupMenuItem::new(l.clone()).checked(n == on).on_click(move |_, window, cx| pick(n, window, cx)));
                i += 1;
            }
        }
        menu
    })
}
