//! Mini's two layers. `MiniLayer`: a cached leaf over a chat pane's body
//! (the web's .cm-layer): Mini and its effects, clipped to the pane; only
//! the robot takes a tap (the rest of the pane gets every event as before);
//! it also hears the pane's presses and wheel (the person getting on with
//! things) and the pointer (its eyes follow it). `TopLayer`: a window
//! overlay (the web's .cm-top) for the logo's walks and the top bar's
//! outings; it takes no pointer. Each is notified by the loop only while it
//! shows an actor, so a frame of Mini redraws Mini alone.

use super::{draw, SysRef};
use crate::{composer::Composer, log::LogView, store::ChatStore};
use gpui::*;
use std::{cell::Cell, rc::Rc};

pub struct MiniLayer {
    sys: SysRef,
    id: String,
    layer: Rc<Cell<Bounds<Pixels>>>,
    /// The pointer is over the pane (the web's pointerleave).
    inside: Rc<Cell<bool>>,
    /// The log's parked look, as last seen (a change is a new attach).
    parked: (bool, bool),
    _subs: Vec<Subscription>,
}

impl MiniLayer {
    pub fn new(sys: SysRef, id: String, layer: Rc<Cell<Bounds<Pixels>>>, store: &Entity<ChatStore>, composer: &Entity<Composer>, log: &Entity<LogView>, cx: &mut Context<Self>) -> Self {
        let meta = store.read(cx).meta.clone();
        let stores = crundi_shell::stores(cx);
        let ch = |this: &mut Self, cx: &mut Context<Self>| { let id = this.id.clone(); super::changed(&id, cx); };
        let subs = vec![
            cx.observe(store, move |this, _, cx| ch(this, cx)),
            cx.observe(&meta, move |this, _, cx| ch(this, cx)),
            cx.observe(composer, move |this, _, cx| ch(this, cx)),
            cx.observe(&stores.projects, move |this, _, cx| ch(this, cx)),
            cx.observe(&stores.nav, move |this, _, cx| ch(this, cx)),
            // The log notifies on every scroll: only its parked look matters here.
            cx.observe(log, move |this: &mut Self, l, cx| {
                let p = { let l = l.read(cx); (l.ui.parked.parked, l.ui.parked.peek) };
                if p != this.parked { this.parked = p; ch(this, cx); }
            }),
        ];
        MiniLayer { sys, id, layer, inside: Rc::new(Cell::new(false)), parked: (false, false), _subs: subs }
    }
}

impl Render for MiniLayer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("chat-mini");
        let (shapes, tap) = {
            let b = self.sys.borrow();
            let now = b.clock.get();
            let mut shapes = vec![];
            let mut tap = None;
            if let Some(c) = b.chats.get(&self.id) {
                if let Some(m) = &c.mini {
                    let a = m.a.borrow();
                    shapes = draw::figure(&a);
                    // Only the robot takes a tap, while it is there to be seen.
                    if a.v[super::K::alpha as usize] > 0.3 { tap = Some(draw::tap_box(&a)); }
                }
                for fx in &c.fx { if let Some(v) = draw::effect(fx, now) { shapes.extend(v); } }
            }
            (shapes, tap)
        };
        let (layer, sys, id, inside) = (self.layer.clone(), self.sys.clone(), self.id.clone(), self.inside.clone());
        canvas(move |bounds, window, _| {
            layer.set(bounds);
            tap.map(|t| window.insert_hitbox(Bounds::new(bounds.origin + t.origin, t.size), HitboxBehavior::BlockMouse))
        }, move |bounds, hit, window, _| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| draw::paint(&shapes, bounds.origin, window));
            if let Some(h) = &hit { window.set_cursor_style(CursorStyle::PointingHand, h); }
            // A press in the pane (not on Mini), the wheel: the person is at it.
            let (h1, id1) = (hit.clone(), id.clone());
            window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || !bounds.contains(&e.position) { return; }
                if h1.as_ref().is_some_and(|h| h.is_hovered(window)) { return; }
                super::acted(&id1, cx);
            });
            let id2 = id.clone();
            window.on_mouse_event(move |e: &ScrollEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture && bounds.contains(&e.position) { super::acted(&id2, cx); }
            });
            // A tap on the robot: it is not passed on to what is behind.
            let (h3, id3, s3) = (hit.clone(), id.clone(), sys.clone());
            window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || !h3.as_ref().is_some_and(|h| h.is_hovered(window)) { return; }
                cx.stop_propagation();
                s3.borrow_mut().tapped(&id3, cx);
                super::after_change(&s3, cx);
            });
            // Its eyes follow a cursor that is near.
            let (id4, s4) = (id.clone(), sys.clone());
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Bubble { return; }
                let inn = bounds.contains(&e.position);
                if !inn && !inside.get() { return; }
                inside.set(inn);
                let p = inn.then(|| (f32::from(e.position.x - bounds.origin.x), f32::from(e.position.y - bounds.origin.y)));
                if s4.borrow().chats.get(&id4).is_none_or(|c| c.mini.is_none()) { return; }
                s4.borrow_mut().moved(&id4, p, cx);
                super::after_change(&s4, cx);
            });
        }).size_full()
    }
}

/// The window's layer: the logo's walks, the top bar's outings, a Mini
/// handed over from a chat that woke under it.
pub struct TopLayer { sys: SysRef }

impl TopLayer {
    pub fn new(sys: SysRef, _cx: &mut Context<Self>) -> Self { TopLayer { sys } }
}

impl Render for TopLayer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("mini-top");
        let (shapes, clip_h) = {
            let b = self.sys.borrow();
            let now = b.clock.get();
            let mut shapes = vec![];
            for t in &b.top { shapes.extend(draw::figure(&t.mini.a.borrow())); }
            for fx in &b.top_fx { if let Some(v) = draw::effect(fx, now) { shapes.extend(v); } }
            (shapes, b.clip_h.get())
        };
        let win = self.sys.borrow().window.clone();
        canvas(move |bounds, _, _| { win.set((f32::from(bounds.size.width), f32::from(bounds.size.height))); }, move |bounds, _, window, _| {
            if shapes.is_empty() { return; }
            let clip = Bounds::new(bounds.origin, size(bounds.size.width, px(clip_h).min(bounds.size.height)));
            window.with_content_mask(Some(ContentMask { bounds: clip }), |window| draw::paint(&shapes, bounds.origin, window));
        }).size_full()
    }
}
