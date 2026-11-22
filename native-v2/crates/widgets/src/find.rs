//! Words on screen, for Cru's highlight (show_in_crundi's `highlight`; the
//! web's cruFindText, src/webapp-html.js:6809-6845): the shared controls put
//! a `mark` over themselves with their label (a button's text, an icon
//! button's tip), and `find` picks the one that best shows some words.
//!
//! A mark is a zero-cost element over its parent (absolute, inset 0: no
//! space in the layout, nothing painted). As it is laid out it keeps its
//! bounds in its element state, so a cached view that is reused keeps its
//! marks with it (GPUI carries a reused view's element states into the new
//! frame), and a mark whose element left the frame is dropped with that
//! state: only what is on screen now can be found. Nothing is read while
//! rendering; `find` runs in a handler. Unused, a mark costs one weak
//! pointer per control made, kept until the control goes.

use gpui::*;
use std::{cell::{Cell, RefCell}, rc::{Rc, Weak}};

/// One control's label and where it was last laid out (window coordinates;
/// `seen` is that bounds within its clip).
pub struct Mark {
    text: SharedString,
    /// A tip or a field's placeholder (the web's title / aria-label), not text
    /// on the control.
    label: bool,
    seen: Cell<Bounds<Pixels>>,
}

thread_local! {
    static MARKS: RefCell<(Vec<Weak<Mark>>, usize)> = const { RefCell::new((Vec::new(), 0)) };
}

/// A mark with the control's own words (a button's label).
pub fn mark(text: impl Into<SharedString>) -> MarkEl { MarkEl { text: text.into(), label: false } }

/// A mark with the control's tip or placeholder (an icon button's tooltip).
pub fn label_mark(text: impl Into<SharedString>) -> MarkEl { MarkEl { text: text.into(), label: true } }

pub struct MarkEl { text: SharedString, label: bool }

impl IntoElement for MarkEl {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl Element for MarkEl {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut st = Style::default();
        st.position = Position::Absolute;
        st.inset.top = px(0.).into();
        st.inset.left = px(0.).into();
        st.size.width = relative(1.).into();
        st.size.height = relative(1.).into();
        (window.request_layout(st, None, cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, _: &mut App) {
        if self.text.is_empty() { return; }
        let seen = bounds.intersect(&window.visible_mask());
        let (text, label) = (self.text.clone(), self.label);
        window.with_global_id(ElementId::Name(text.clone()), |gid, window| {
            window.with_element_state::<Rc<Mark>, _>(gid, |st, _| {
                let m = match st {
                    Some(m) if m.text == text && m.label == label => m,
                    _ => {
                        let m = Rc::new(Mark { text, label, seen: Cell::new(seen) });
                        MARKS.with(|v| {
                            let mut v = v.borrow_mut();
                            v.0.push(Rc::downgrade(&m));
                            // Now and then, the marks that went are let go.
                            v.1 += 1;
                            if v.1 >= 256 { v.1 = 0; v.0.retain(|w| w.strong_count() > 0); }
                        });
                        m
                    }
                };
                m.seen.set(seen);
                ((), m)
            })
        });
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), _: &mut Window, _: &mut App) {}
}

fn register(m: &Rc<Mark>) {
    MARKS.with(|v| {
        let mut v = v.borrow_mut();
        v.0.push(Rc::downgrade(m));
        v.1 += 1;
        if v.1 >= 256 { v.1 = 0; v.0.retain(|w| w.strong_count() > 0); }
    });
}

/// Many marks over one element (a terminal's lines, F202): each words with
/// its box from the element's top left. One element and one element state
/// for all of them (a mark element each costs a layout node and a state);
/// the same list (`Rc::ptr_eq`) keeps its marks, only their bounds move.
pub fn marks(items: Rc<Vec<(SharedString, Bounds<Pixels>)>>) -> MarksEl { MarksEl { items } }

pub struct MarksEl { items: Rc<Vec<(SharedString, Bounds<Pixels>)>> }

impl IntoElement for MarksEl {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl Element for MarksEl {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut st = Style::default();
        st.position = Position::Absolute;
        st.inset.top = px(0.).into();
        st.inset.left = px(0.).into();
        st.size.width = relative(1.).into();
        st.size.height = relative(1.).into();
        (window.request_layout(st, None, cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, _: &mut App) {
        if self.items.is_empty() { return; }
        let mask = window.visible_mask();
        let items = self.items.clone();
        window.with_global_id(ElementId::Name("find-marks".into()), |gid, window| {
            type St = (Rc<Vec<(SharedString, Bounds<Pixels>)>>, Vec<Rc<Mark>>);
            window.with_element_state::<St, _>(gid, |st, _| {
                let (items, marks) = match st {
                    Some((had, marks)) if Rc::ptr_eq(&had, &items) => (had, marks),
                    _ => {
                        let marks: Vec<Rc<Mark>> = items.iter().map(|(t, _)| Rc::new(Mark { text: t.clone(), label: false, seen: Cell::new(Bounds::default()) })).collect();
                        for m in &marks { register(m); }
                        (items, marks)
                    }
                };
                for ((_, b), m) in items.iter().zip(&marks) {
                    m.seen.set(Bounds::new(bounds.origin + b.origin, b.size).intersect(&mask));
                }
                ((), (items, marks))
            })
        });
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), _: &mut Window, _: &mut App) {}
}

/// The words drawn by painted elements (a kanban card, a map's ideas):
/// each one's box from its drawing's top left, its words, and whether they
/// are a tip (`label_mark`) rather than words on it (`mark`). Laid out with
/// the drawing and kept with it.
pub type Words = Rc<Vec<(Bounds<Pixels>, SharedString, bool)>>;

/// Many drawings' words, from one element's prepaint (F243): one element
/// state for all of them under `key`. A drawing whose list is the same as
/// last frame's (`Rc::ptr_eq`: it was not laid out again) keeps its marks,
/// and only their bounds move. Per frame that is a cell set per word, not
/// an element namespace and a state lookup each, as `mark` children cost.
pub fn words_at<'a>(key: &'static str, groups: impl IntoIterator<Item = (&'a Words, Point<Pixels>)>, window: &mut Window) {
    // (What is shown: inside a wide clip, its real clip.)
    let mask = window.visible_mask();
    window.with_global_id(ElementId::Name(key.into()), |gid, window| {
        // By the list and its occurrence (one drawing shown twice, as the
        // map's ghosts share theirs, keeps a set of marks for each place).
        type St = std::collections::HashMap<(usize, u32), (Words, Vec<Rc<Mark>>)>;
        window.with_element_state::<St, _>(gid, |st, _| {
            let mut old = st.unwrap_or_default();
            let mut now: St = std::collections::HashMap::with_capacity(old.len());
            for (words, origin) in groups {
                if words.is_empty() { continue; }
                let p = Rc::as_ptr(words) as usize;
                let mut n = 0;
                while now.contains_key(&(p, n)) { n += 1; }
                let k = (p, n);
                let (w, marks) = match old.remove(&k) {
                    Some(had) => had,
                    None => {
                        let marks: Vec<Rc<Mark>> = words.iter().map(|(_, t, label)| Rc::new(Mark { text: t.clone(), label: *label, seen: Cell::new(Bounds::default()) })).collect();
                        for m in &marks { register(m); }
                        (words.clone(), marks)
                    }
                };
                for ((b, _, _), m) in w.iter().zip(&marks) {
                    m.seen.set(Bounds::new(origin + b.origin, b.size).intersect(&mask));
                }
                now.insert(k, (w, marks));
            }
            ((), now)
        })
    });
}

/// Spaces folded, trimmed, lower case (the web's norm).
pub fn norm(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase() }

/// One candidate for `best`: its words, whether they are a label, its bounds.
pub struct Found { pub text: String, pub label: bool, pub bounds: Bounds<Pixels> }

/// The candidate that best shows `want` (cruFindText's score): its words
/// exactly first, then words that start with it, then any that hold it, the
/// shortest first; nothing over 40% of the window or much longer than the
/// words; nothing smaller than 2 px.
pub fn best(want: &str, cands: impl IntoIterator<Item = Found>, viewport: Size<Pixels>) -> Option<Bounds<Pixels>> {
    let want = norm(want);
    if want.is_empty() { return None; }
    let area = f32::from(viewport.width) * f32::from(viewport.height);
    let mut top: Option<(usize, Bounds<Pixels>)> = None;
    for c in cands {
        let own = norm(&c.text);
        if !own.contains(&want) { continue; }
        let (w, h) = (f32::from(c.bounds.size.width), f32::from(c.bounds.size.height));
        if w < 2. || h < 2. || w * h > area * 0.4 { continue; }
        if !c.label && own.len() > want.len() * 6 + 200 { continue; }
        let rank = if own == want { 0 } else if own.starts_with(&want) { 1 } else { 2 };
        let score = rank * 100_000 + own.len();
        if top.as_ref().is_none_or(|t| score < t.0) { top = Some((score, c.bounds)); }
    }
    top.map(|t| t.1)
}

/// The control on screen that best shows `want`, its bounds in the window
/// (as it was last laid out). In handlers and timers only.
pub fn find(want: &str, window: &Window) -> Option<Bounds<Pixels>> {
    let live: Vec<Rc<Mark>> = MARKS.with(|v| v.borrow().0.iter().filter_map(|w| w.upgrade()).collect());
    let win = Bounds::new(Point::default(), window.viewport_size());
    best(want, live.iter().map(|m| Found { text: m.text.to_string(), label: m.label, bounds: m.seen.get().intersect(&win) }), window.viewport_size())
}

/// How many marks are on screen (the driver's count).
pub fn live() -> usize { MARKS.with(|v| v.borrow().0.iter().filter(|w| w.strong_count() > 0).count()) }

#[cfg(test)]
mod tests {
    use super::{best, Found};
    use gpui::{point, px, size, Bounds};
    fn f(t: &str, x: f32, w: f32) -> Found { Found { text: t.into(), label: false, bounds: Bounds::new(point(px(x), px(0.)), size(px(w), px(20.))) } }

    #[test]
    fn the_best_words() {
        let vp = size(px(1000.), px(800.));
        // Exact before a start, a start before a middle, the shorter first.
        let got = best("mark all READ", [f("Mark all read now", 1., 80.), f("  mark  all read ", 2., 60.), f("x mark all read", 3., 50.)], vp);
        assert_eq!(got.map(|b| f32::from(b.origin.x)), Some(2.));
        assert_eq!(best("read now", [f("x mark all read", 3., 50.), f("Mark all read now", 1., 80.)], vp).map(|b| f32::from(b.origin.x)), Some(1.));
        // Not found, too small, too big.
        assert!(best("nothing", [f("Mark all read", 1., 80.)], vp).is_none());
        assert!(best("a", [f("a", 1., 1.)], vp).is_none());
        assert!(best("a", [Found { text: "a".into(), label: false, bounds: Bounds::new(point(px(0.), px(0.)), size(px(1000.), px(800.))) }], vp).is_none());
        assert!(best("", [f("a", 1., 80.)], vp).is_none());
    }
}
