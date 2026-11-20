//! Where things are on screen, for Mini (the chat area's mascot,
//! app/vendor/crundi-mini.js), which walks over the window's parts: the
//! logo's head in the top bar, the usage bars, the approvals bell, the
//! project bar's tabs and Settings, the sidebar's projects, a chat's queued
//! message, its suggestion, its Send button, its parked card. The web reads
//! those with getBoundingClientRect(); here each view that draws one puts a
//! `spot` (a canvas, no layout of its own) over it, which notes its bounds
//! as it is laid out: nothing is drawn again for it, and nothing is noted
//! while a view is reused from its cache (its bounds are the same then).
//!
//! A view that draws spots calls `begin(group)` at the top of its render:
//! a spot of the group not drawn again in that render is gone (an element
//! that went). Read in handlers and timers only (`get`, `all`), never in
//! render; window coordinates.
//!
//! Also the logo's place while Mini is out (`logo_away`): the TopBar
//! observes it and leaves the head's place empty.

use gpui::*;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Default)]
pub struct Spots {
    /// Each group's render count.
    gens: HashMap<String, u64>,
    /// Each spot: its bounds, its group, the group's render it was drawn in.
    at: HashMap<String, (Bounds<Pixels>, String, u64)>,
}

impl Spots {
    /// A spot's bounds, if it was drawn in its group's last render.
    pub fn get(&self, name: &str) -> Option<Bounds<Pixels>> {
        let (b, g, n) = self.at.get(name)?;
        (self.gens.get(g) == Some(n)).then_some(*b)
    }
    /// Every live spot whose name starts with `prefix`.
    pub fn all(&self, prefix: &str) -> Vec<(String, Bounds<Pixels>)> {
        let mut v: Vec<(String, Bounds<Pixels>)> = self.at.iter().filter(|(k, (_, g, n))| k.starts_with(prefix) && self.gens.get(g) == Some(n)).map(|(k, (b, _, _))| (k.clone(), *b)).collect();
        v.sort_by(|a, b| a.1.origin.y.partial_cmp(&b.1.origin.y).unwrap_or(std::cmp::Ordering::Equal));
        v
    }
}

/// The spots' handle (set once; a handle, not data: rule 1).
pub type SpotsRef = Rc<RefCell<Spots>>;
struct SpotsHandle(SpotsRef);
impl Global for SpotsHandle {}

pub fn spots(cx: &mut App) -> SpotsRef {
    if let Some(h) = cx.try_global::<SpotsHandle>() { return h.0.clone(); }
    let s = SpotsRef::default();
    cx.set_global(SpotsHandle(s.clone()));
    s
}

/// At the top of a render: the group's spots not drawn again are gone.
pub fn begin(cx: &mut App, group: &str) {
    let s = spots(cx);
    let mut s = s.borrow_mut();
    match s.gens.get_mut(group) { Some(n) => *n += 1, None => { s.gens.insert(group.to_string(), 1); } }
}

/// A spot over its parent (absolute, inset 0): notes the parent's bounds.
pub fn spot(cx: &mut App, group: &str, name: impl Into<String>) -> Canvas<()> {
    let s = spots(cx);
    let (group, name) = (group.to_string(), name.into());
    canvas(move |b, _, _| {
        let mut s = s.borrow_mut();
        let n = s.gens.get(&group).copied().unwrap_or(0);
        s.at.insert(name, (b, group, n));
    }, |_, _, _, _| {}).absolute().inset_0()
}

/// The logo's head is out walking (Mini): its place in the top bar is empty.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct LogoAway(pub bool);
struct LogoHandle(Entity<LogoAway>);
impl Global for LogoHandle {}
pub fn logo_away(cx: &mut App) -> Entity<LogoAway> {
    if let Some(h) = cx.try_global::<LogoHandle>() { return h.0.clone(); }
    let e = cx.new(|_| LogoAway::default());
    cx.set_global(LogoHandle(e.clone()));
    e
}
pub fn set_logo_away(cx: &mut App, away: bool) {
    let e = logo_away(cx);
    if e.read(cx).0 != away { e.update(cx, |x, cx| { x.0 = away; cx.notify(); }); }
}

/// The project bar's tabs, to haul them sideways (Mini's update scene):
/// their scroll and the bar to redraw.
#[derive(Clone)]
pub struct Tabs { pub scroll: ScrollHandle, pub bar: AnyWeakEntity }
struct TabsHandle(Tabs);
impl Global for TabsHandle {}
pub(crate) fn set_tabs(cx: &mut App, t: Tabs) { cx.set_global(TabsHandle(t)); }
pub fn tabs(cx: &App) -> Option<Tabs> { cx.try_global::<TabsHandle>().map(|t| t.0.clone()) }
/// Scroll the tabs to `x` (0 to -max) and redraw the bar alone.
pub fn scroll_tabs(cx: &mut App, x: Pixels) {
    let Some(t) = tabs(cx) else { return };
    let o = t.scroll.offset();
    t.scroll.set_offset(point(x.clamp(-t.scroll.max_offset().x, px(0.)), o.y));
    if let Some(e) = t.bar.upgrade() { cx.notify(e.entity_id()); }
}
