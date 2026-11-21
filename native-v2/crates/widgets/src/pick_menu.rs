//! A button opening a menu whose items are made when it opens (their labels
//! and ids known only then): the browser drawer's Network, Device, its "…"
//! menu and the chat to send marks to (.brd-pop). Items can be ticked, carry
//! an icon, or be a group heading or a note (not clickable). Added by
//! Desktop (AREAS.md, "A new control").

use crate::icon_button;
use crundi_theme::{IconName, T};
use gpui::*;
use std::rc::Rc;

/// One row of a pick menu.
#[derive(Clone, Default, Debug)]
pub struct PickItem {
    pub id: String,
    pub label: SharedString,
    /// "item" (clickable), "group" (a heading), "note" (a line of text).
    pub kind: &'static str,
    pub on: bool,
    pub icon: Option<IconName>,
}

/// An icon button with a menu of `items()` (called each time it opens), its
/// `title` on top when not empty; `pick(id)` on a click.
pub fn pick_menu(
    id: impl Into<ElementId>,
    icon: IconName,
    tip: impl Into<SharedString>,
    title: impl Into<SharedString>,
    items: impl Fn() -> Vec<PickItem> + 'static,
    pick: impl Fn(&str, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    pick_menu_open(id, icon, tip, title, items, pick, |_, _, _| {})
}

/// `pick_menu`, told when its menu opens and closes (`open(true)` as it
/// shows): the drawer keeps which popover is open for its driver state.
pub fn pick_menu_open(
    id: impl Into<ElementId>,
    icon: IconName,
    tip: impl Into<SharedString>,
    title: impl Into<SharedString>,
    items: impl Fn() -> Vec<PickItem> + 'static,
    pick: impl Fn(&str, &mut Window, &mut App) + 'static,
    open: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    pick_menu_toned(id, icon, tip, title, None, false, items, pick, open)
}

/// `pick_menu_open` whose button can be in a tone (`.brd-netbtn.server`,
/// `.hp`, `.bad`, `.brd-devbtn.on`: the icon and an edge in that colour, the
/// icon itself unchanged) and off (no tab: `nb.disabled = db.disabled = !t`).
#[allow(clippy::too_many_arguments)]
pub fn pick_menu_toned(
    id: impl Into<ElementId>,
    icon: IconName,
    tip: impl Into<SharedString>,
    title: impl Into<SharedString>,
    tone: Option<Hsla>,
    off: bool,
    items: impl Fn() -> Vec<PickItem> + 'static,
    pick: impl Fn(&str, &mut Window, &mut App) + 'static,
    open: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
    use gpui_component::Disableable as _;
    let mut b = icon_button(id, icon, tip).disabled(off);
    if let Some(c) = tone { b = b.outline().text_color(c).border_color(c); }
    let (items, pick, title): (Rc<dyn Fn() -> Vec<PickItem>>, Rc<dyn Fn(&str, &mut Window, &mut App)>, SharedString) = (Rc::new(items), Rc::new(pick), title.into());
    b.dropdown_menu(move |mut menu, _, _| {
        menu = menu.min_w(T::MENU_MIN_W).max_h(T::DIALOG_MAX_H).scrollable(true);
        if !title.is_empty() { menu = menu.label(title.clone()); }
        for it in items() {
            match it.kind {
                "group" => { menu = menu.separator(); if !it.label.is_empty() { menu = menu.label(it.label.to_uppercase()); } }
                "note" => { menu = menu.item(PopupMenuItem::new(it.label.clone()).disabled(true)); }
                _ => {
                    let (pick, id) = (pick.clone(), it.id.clone());
                    let mut m = PopupMenuItem::new(it.label.clone()).checked(it.on).on_click(move |_, w, cx| pick(&id, w, cx));
                    if let Some(i) = it.icon { m = m.icon(i); }
                    menu = menu.item(m);
                }
            }
        }
        menu
    }).on_open_change(move |on, window, cx| open(*on, window, cx))
}
