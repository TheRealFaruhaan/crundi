//! A select whose choices are known only at run time (the web's <select>
//! filled from the server: Cru's models, the skills' scopes): the
//! `dropdown`'s look (a quiet button with the current choice and a chevron,
//! a menu with the current one ticked), over owned labels. `on` out of
//! range shows `none`. (Made by Operations; one control with `dropdown`: `dropdown::choices`.)

use gpui::*;

pub fn select(
    id: impl Into<ElementId>,
    items: Vec<SharedString>,
    on: Option<usize>,
    none: impl Into<SharedString>,
    disabled: bool,
    pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = on.and_then(|i| items.get(i).cloned()).unwrap_or_else(|| none.into());
    crate::dropdown::choices(id, label, items, vec![], on, disabled, false, pick)
}
