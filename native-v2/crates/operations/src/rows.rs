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

use gpui::{prelude::FluentBuilder as _, *};
use std::{cell::Cell, rc::Rc};

type Draw = Rc<dyn Fn(usize, &mut Window, &mut App) -> AnyElement>;

pub struct Row { i: usize, h: Rc<Cell<Option<Pixels>>>, draw: Draw, owner: EntityId, grew: Rc<Cell<bool>> }

impl Render for Row {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("ops-row");
        let el = (self.draw)(self.i, window, cx);
        let (h, owner, grew) = (self.h.clone(), self.owner, self.grew.clone());
        div().w_full().relative().child(el).child(
            canvas(move |b, _, cx| {
                let was = h.replace(Some(b.size.height));
                if was.is_some_and(|w| w != b.size.height) { grew.set(true); cx.defer(move |cx| cx.notify(owner)); }
            }, |_, _, _, _| {}).absolute().top_0().left_0().size_full())
    }
}

pub struct Rows { rows: Vec<(Entity<Row>, Rc<Cell<Option<Pixels>>>)>, grew: Rc<Cell<bool>>, draw: Draw, owner: EntityId }

impl Rows {
    pub fn new<V: 'static>(n: usize, draw: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static, cx: &mut Context<V>) -> Self {
        let mut r = Rows { rows: vec![], grew: Rc::new(Cell::new(false)), draw: Rc::new(draw), owner: cx.entity_id() };
        r.resize(n, cx);
        r
    }
    /// `n` rows: the first ones kept as they are (cached), rows added or
    /// dropped at the end (a log's line appended).
    pub fn resize(&mut self, n: usize, cx: &mut App) {
        self.rows.truncate(n);
        while self.rows.len() < n {
            let (i, h) = (self.rows.len(), Rc::new(Cell::new(None)));
            let (draw, owner, grew) = (self.draw.clone(), self.owner, self.grew.clone());
            self.rows.push((cx.new(|_| Row { i, h: h.clone(), draw, owner, grew }), h));
        }
    }
    /// What the rows show changed: build each again (uncached) and measure it.
    pub fn changed(&self, cx: &mut App) {
        for (e, h) in &self.rows { h.set(None); cx.notify(e.entity_id()); }
    }
    /// What row `i` shows changed: build it again (uncached) and measure it.
    pub fn changed_one(&self, i: usize, cx: &mut App) {
        if let Some((e, h)) = self.rows.get(i) { h.set(None); cx.notify(e.entity_id()); }
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

/// A section's body as parts (F62): the children of its column, each drawn
/// as a row of its own in the page's list, so a wheel frame builds again only
/// the parts the list's edges cut, never a whole section taller than the
/// page. As an element it is the same column (`gap` between the parts).
///
/// A row needs only its own part: a body built `wanting(Some(j))` builds
/// only part `j` of the parts added with `part(|| …)` and counts the rest
/// (`Some(usize::MAX)` counts them all, building none); `child` always builds.
pub struct Parts { gap: Pixels, want: Want, items: Vec<(Option<AnyElement>, Pixels)> }

/// Which part a body is built for: every part (None), or one (Some(j)).
pub type Want = Option<usize>;

/// The part a nested column (added with `then` after `offset` parts) is
/// wanted for, in its own numbering (none of it when the want is before it).
pub fn shift(want: Want, offset: usize) -> Want { want.map(|w| w.checked_sub(offset).unwrap_or(usize::MAX)) }

/// A column of parts, `gap` apart.
pub fn parts(gap: Pixels) -> Parts { Parts { gap, want: None, items: vec![] } }

impl Parts {
    /// A body of one part.
    pub fn one(el: impl IntoElement) -> Parts { parts(px(0.)).child(el) }
    /// Built for part `want` only (see `Parts`).
    pub fn wanting(mut self, want: Want) -> Self { self.want = want; self }
    /// A part built only when it is wanted (counted either way).
    pub fn part<E: IntoElement>(mut self, f: impl FnOnce() -> E) -> Self {
        let j = self.items.len();
        let el = self.want.is_none_or(|w| w == j).then(|| f().into_any_element());
        self.items.push((el, self.gap));
        self
    }
    pub fn len(&self) -> usize { self.items.len() }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
    /// The space above part `j` (none above the first).
    pub fn gap_before(&self, j: usize) -> Pixels { if j == 0 { px(0.) } else { self.items.get(j).map(|x| x.1).unwrap_or_default() } }
    /// Part `j` (if it was built) and the space above it.
    pub fn take(mut self, j: usize) -> Option<(AnyElement, Pixels)> {
        let g = self.gap_before(j);
        if j >= self.items.len() { return None; }
        self.items.swap_remove(j).0.map(|e| (e, g))
    }
    /// Another column's parts after these, `gap` above its first (its own
    /// gaps between its parts): a nested column flattened into parts. Build
    /// it `wanting(shift(want, self.len()))`.
    pub fn then(mut self, other: Parts, gap: Pixels) -> Self {
        for (k, (e, g)) in other.items.into_iter().enumerate() { self.items.push((e, if k == 0 { gap } else { g })); }
        self
    }
    /// A framed list (one border, rounded, its rows' hairlines their own) as
    /// parts: each row draws its slice of the frame (`slice`), so the frame
    /// looks as one box while its rows are drawn apart.
    pub fn framed(self, rows: Vec<AnyElement>, radius: Pixels, border: Hsla, bg: Option<Hsla>) -> Self {
        let n = rows.len();
        let mut rows = rows.into_iter();
        self.framed_with(n, radius, border, bg, move |_| rows.next().unwrap_or_else(|| div().into_any_element()))
    }
    /// `framed` with its `n` rows built by `row(k)`, only the wanted ones.
    pub fn framed_with(mut self, n: usize, radius: Pixels, border: Hsla, bg: Option<Hsla>, mut row: impl FnMut(usize) -> AnyElement) -> Self {
        for k in 0..n {
            let j = self.items.len();
            let el = self.want.is_none_or(|w| w == j).then(|| slice(k, n, radius, border, bg).overflow_hidden().child(row(k)).into_any_element());
            self.items.push((el, if k == 0 { self.gap } else { px(0.) }));
        }
        self
    }
}

/// Slice `j` of `n` of a box drawn in slices (a card, a framed list): the
/// sides on every slice, the top edge and corners on the first, the bottom
/// ones on the last.
pub fn slice(j: usize, n: usize, radius: Pixels, border: Hsla, bg: Option<Hsla>) -> Div {
    div().flex().flex_col().border_l_1().border_r_1().border_color(border)
        .when_some(bg, |d, c| d.bg(c))
        .when(j == 0, |d| d.border_t_1().rounded_t(radius))
        .when(j + 1 == n, |d| d.border_b_1().rounded_b(radius))
}

impl ParentElement for Parts {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        let g = self.gap;
        self.items.extend(elements.into_iter().map(|e| (Some(e), g)));
    }
}

impl IntoElement for Parts {
    type Element = Div;
    fn into_element(self) -> Div {
        div().flex().flex_col().children(self.items.into_iter().enumerate().filter_map(|(j, (e, g))| e.map(|e| div().when(j > 0, |d| d.pt(g)).child(e))))
    }
}
