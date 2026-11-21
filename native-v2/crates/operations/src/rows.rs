//! A virtual list's rows as cached views (F62/F130): a row that only moves
//! under the scroll is replayed at an offset by GPUI patch (b)
//! (vendor/gpui/PATCH.md) instead of being built again on every wheel frame.
//!
//! A cached view is laid out from its style alone, so a row's height is
//! measured on the frame it is built uncached and then given to its cache.
//! The owner calls `Rows::changed` when what the rows show changes: every
//! row is drawn uncached once more and measured again (what a change cost
//! before). A row whose height changes on its own (a field growing) asks
//! its owner to draw again.

use gpui::*;
use std::{cell::Cell, rc::Rc};

type Draw = Rc<dyn Fn(usize, &mut Window, &mut App) -> AnyElement>;

pub struct Row { i: usize, h: Rc<Cell<Option<Pixels>>>, draw: Draw, owner: EntityId, grew: Rc<Cell<bool>> }

impl Render for Row {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let el = (self.draw)(self.i, window, cx);
        let (h, owner, grew) = (self.h.clone(), self.owner, self.grew.clone());
        div().w_full().relative().child(el).child(
            canvas(move |b, _, cx| {
                let was = h.replace(Some(b.size.height));
                if was.is_some_and(|w| w != b.size.height) { grew.set(true); cx.defer(move |cx| cx.notify(owner)); }
            }, |_, _, _, _| {}).absolute().top_0().left_0().size_full())
    }
}

pub struct Rows { rows: Vec<(Entity<Row>, Rc<Cell<Option<Pixels>>>)>, grew: Rc<Cell<bool>> }

impl Rows {
    pub fn new<V: 'static>(n: usize, draw: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static, cx: &mut Context<V>) -> Self {
        let draw: Draw = Rc::new(draw);
        let owner = cx.entity_id();
        let grew = Rc::new(Cell::new(false));
        let rows = (0..n).map(|i| {
            let h = Rc::new(Cell::new(None));
            (cx.new(|_| Row { i, h: h.clone(), draw: draw.clone(), owner, grew: grew.clone() }), h)
        }).collect();
        Rows { rows, grew }
    }
    /// What the rows show changed: build each again (uncached) and measure it.
    pub fn changed(&self, cx: &mut App) {
        for (e, h) in &self.rows { h.set(None); cx.notify(e.entity_id()); }
    }
    pub fn len(&self) -> usize { self.rows.len() }
    /// A row's height changed on its own since the last call: remeasure the list.
    pub fn take_grew(&self) -> bool { self.grew.replace(false) }
    /// Row `i` for the list: cached at its measured height once it has one.
    pub fn el(&self, i: usize) -> AnyElement {
        let Some((e, h)) = self.rows.get(i) else { return div().into_any_element() };
        let v: AnyView = e.clone().into();
        match h.get() {
            Some(h) => v.cached(StyleRefinement::default().w_full().h(h)).into_any_element(),
            None => v.into_any_element(),
        }
    }
}
