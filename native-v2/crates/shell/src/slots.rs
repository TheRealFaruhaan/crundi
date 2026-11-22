//! Places in the shell's views that an area fills (AREAS.md, "How an area
//! plugs in"), registered once at init, before the window opens; the shell
//! never depends on the area's crate. Each slot names an entity the area
//! notifies when what it draws there changed: the shell's view that shows
//! the slot observes it and redraws only then (ARCHITECTURE.md rules 1-2).
//!
//! - `register_top_item(cx, name, &watch, draw)`: an element in the top
//!   bar. Names and places, as the web's #topbar: "chips" (a panel's
//!   chips, .wg-chips, after the spacer), "approvals" (#approvals-btn,
//!   beside the connection badge; not for a collaborator). `draw` returns
//!   None to show nothing.
//! - `register_tab_badge(cx, tab, &watch, draw)`: a tab's badge in the
//!   project bar (the Secrets and Factories counts, Settings' update dot).
//! - `register_login_restore(cx, &watch, open, draw)`: the sign-in page's
//!   "Restore from a backup instead" (first run only): `open(cx, server)`
//!   opens it, `draw` is its layer over the sign-in page while open.
//!
//! Also the top bar's measured widths (`Widths`): the top bar is a row of
//! cached leaves (rule 3), and a cached view is laid out from its style
//! alone, so each leaf whose width follows its content (the usage badge,
//! the right-hand buttons, the connection badge, the window buttons)
//! reports the width it drew (`measured`); the Shell lays the row out with
//! them and redraws only when one changed.

use gpui::*;
use std::{collections::HashMap, rc::Rc};

pub type Draw = Rc<dyn Fn(&mut Window, &mut App) -> Option<AnyElement>>;
/// Observe the slot's entity, calling back on a change.
pub(crate) type Watch = Rc<dyn Fn(&mut App, Box<dyn Fn(&mut App)>) -> Subscription>;

fn watch_of<E: 'static>(e: &Entity<E>) -> Watch {
    let e = e.clone();
    Rc::new(move |cx: &mut App, f: Box<dyn Fn(&mut App)>| cx.observe(&e, move |_, cx| f(cx)))
}

#[derive(Clone)]
pub(crate) struct Slot { pub name: &'static str, pub watch: Watch, pub draw: Draw }

#[derive(Default)]
pub(crate) struct Slots {
    pub top: Vec<Slot>,
    pub tabs: Vec<Slot>,
    pub restore: Option<(Watch, Rc<dyn Fn(&mut App, &str)>, Draw)>,
}
impl Global for Slots {}

pub fn register_top_item<E: 'static>(cx: &mut App, name: &'static str, watch: &Entity<E>, draw: impl Fn(&mut Window, &mut App) -> Option<AnyElement> + 'static) {
    let s = Slot { name, watch: watch_of(watch), draw: Rc::new(draw) };
    cx.default_global::<Slots>().top.push(s);
}

pub fn register_tab_badge<E: 'static>(cx: &mut App, tab: &'static str, watch: &Entity<E>, draw: impl Fn(&mut Window, &mut App) -> Option<AnyElement> + 'static) {
    let s = Slot { name: tab, watch: watch_of(watch), draw: Rc::new(draw) };
    cx.default_global::<Slots>().tabs.push(s);
}

pub fn register_login_restore<E: 'static>(cx: &mut App, watch: &Entity<E>, open: impl Fn(&mut App, &str) + 'static, draw: impl Fn(&mut Window, &mut App) -> Option<AnyElement> + 'static) {
    cx.default_global::<Slots>().restore = Some((watch_of(watch), Rc::new(open), Rc::new(draw)));
}

pub(crate) fn top_items(cx: &App) -> Vec<Slot> { cx.try_global::<Slots>().map(|s| s.top.clone()).unwrap_or_default() }
pub(crate) fn tab_badges(cx: &App) -> Vec<Slot> { cx.try_global::<Slots>().map(|s| s.tabs.clone()).unwrap_or_default() }
pub(crate) fn restore(cx: &App) -> Option<(Watch, Rc<dyn Fn(&mut App, &str)>, Draw)> { cx.try_global::<Slots>().and_then(|s| s.restore.clone()) }

/// A view's subscriptions to the slots it shows: it redraws when one changed.
pub(crate) fn observe_slots<V: 'static>(slots: &[Slot], cx: &mut Context<V>) -> Vec<Subscription> {
    let me = cx.entity().downgrade();
    slots.iter().map(|s| {
        let me = me.clone();
        (s.watch)(cx, Box::new(move |cx| { let _ = me.update(cx, |_, cx| cx.notify()); }))
    }).collect()
}

// ─── The top bar's measured widths ───

#[derive(Default)]
pub struct Widths(pub HashMap<&'static str, Pixels>);

struct WidthsHandle(Entity<Widths>);
impl Global for WidthsHandle {}

pub(crate) fn widths(cx: &mut App) -> Entity<Widths> {
    if let Some(h) = cx.try_global::<WidthsHandle>() { return h.0.clone(); }
    let e = cx.new(|_| Widths::default());
    cx.set_global(WidthsHandle(e.clone()));
    e
}

/// `content` at the right edge of its leaf, at its own width, which it
/// reports once drawn (a change: the Shell lays the row out again).
pub(crate) fn measured(key: &'static str, content: impl IntoElement) -> Div {
    div().size_full().relative().child(
        div().absolute().top_0().bottom_0().right_0().flex().items_center().child(content)
            .child(canvas(move |b, _, cx| {
                let w = b.size.width;
                let e = widths(cx);
                if e.read(cx).0.get(key).copied() != Some(w) {
                    cx.defer(move |cx| e.update(cx, |x, cx| { if x.0.get(key).copied() != Some(w) { x.0.insert(key, w); cx.notify(); } }));
                }
            }, |_, _, _, _| {}).absolute().inset_0()))
}
