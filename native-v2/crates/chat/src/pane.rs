//! A chat pane (ARCHITECTURE.md, "The view tree"): ChatPane is layout only
//! (log over composer), and so not a view: its `body` is built inside the
//! Workbench's render, the LogView and the Composer placed there as cached
//! leaves. A log's notify then re-renders the Workbench (an ancestor) and
//! that log, never another pane's parts nor its own composer.
//! The LogView observes the chat's store (its rows); the Composer observes
//! only the chat's meta (state, model, cost, mode…), so a streamed delta
//! never redraws the box.

pub use crate::composer::Composer;
pub use crate::log::LogView;
use crate::store::ChatStore;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};

#[derive(Clone)]
pub struct ChatPane {
    pub store: Entity<ChatStore>,
    pub log: Entity<LogView>,
    pub composer: Entity<Composer>,
    /// Mini's layer over the pane (F101; the main pane's only).
    pub mini: Option<Entity<crate::mini::MiniLayer>>,
}

impl ChatPane {
    pub fn new(store: Entity<ChatStore>, window: &mut Window, cx: &mut App) -> Self {
        let log = cx.new(|cx| LogView::new(store.clone(), cx));
        let composer = cx.new(|cx| Composer::new(store.clone(), window, cx));
        // Where Mini goes (the box, Send, the queue, the suggestion, the
        // parked card): this pane's views note it (crundi_shell::spots).
        log.update(cx, |l, _| l.spots = true);
        composer.update(cx, |c, _| c.spots = true);
        let id = store.read(cx).id.clone();
        let mini = Some(crate::mini::attach(&id, &store, &composer, &log, cx));
        // The streaming reply's last block draws itself (log/tail.rs).
        crate::log::tail::watch(&log, window, cx);
        ChatPane { store, log, composer, mini }
    }

    /// A chat's log and message box without Mini (F169: the factory
    /// panel's transcript over its graph), on a store of the caller's: the
    /// same log (its streaming tail and activity row included) and box, no
    /// spots noted, nothing attached to the chat's id, so a workbench pane
    /// of the same chat keeps its own Mini. Placed with `body`.
    pub fn plain(store: Entity<ChatStore>, window: &mut Window, cx: &mut App) -> Self {
        let log = cx.new(|cx| LogView::new(store.clone(), cx));
        let composer = cx.new(|cx| Composer::new(store.clone(), window, cx));
        crate::log::tail::watch(&log, window, cx);
        ChatPane { store, log, composer, mini: None }
    }

    /// The pane's body (the workbench's PaneParts::body).
    pub fn body(&self, cx: &App) -> AnyElement {
        // The composer is cached at the height it measured itself (it grows
        // with its text: a change of height notifies it, and so the
        // Workbench, its ancestor).
        // The schedule panel (2.10), while open, is a view of its own over
        // the chat's foot (the web's .cc-schedpanel), at its measured height.
        let c = self.composer.read(cx);
        let (h, sched) = (c.height, c.sched.clone());
        // A parked chat shows no composer unless peeked at (2.23).
        let parked = self.log.read(cx).hides_composer();
        // The streaming tail at the log's foot (log/tail.rs), over the list's
        // room for it: its own cached view, so a delta redraws it alone.
        let tail = self.log.read(cx).tail_place(cx).map(|(v, th, gap)| {
            let foot = if parked { px(0.) } else { h };
            v.cached(StyleRefinement::default().absolute().left_0().right_0().bottom(foot + gap).h(th))
        });
        // The activity row while a turn runs (log/activity.rs), likewise
        // its own view at the log's foot, over the dock's room.
        let act = self.log.read(cx).activity_place(cx).map(|(v, ah, dock)| {
            let foot = if parked { px(0.) } else { h };
            v.cached(StyleRefinement::default().absolute().left_0().right_0().bottom(foot + dock).h(ah))
        });
        // The whole chat takes a drop (the web's cc-root): files from the
        // system (the dashed outline) and a reference dragged from another
        // panel (w::ref_target); either goes in at the box's caret (2.7).
        let (c1, c2) = (self.composer.clone(), self.composer.clone());
        let chat = self.store.read(cx).id.clone();
        let body = div().id(("chat-body", self.store.entity_id().as_u64() as usize)).relative().flex().flex_col().size_full().min_h_0().bg(T::bg())
            .child(self.log.clone().cached(StyleRefinement::default().w_full().flex_1().min_h_0()))
            .children(tail)
            .children(act)
            .when(!parked, |d| d.child(self.composer.clone().cached(StyleRefinement::default().w_full().h(h).flex_none())))
            .when_some(sched.filter(|_| !parked), |d, (v, sh)| d.child(v.cached(StyleRefinement::default().absolute().left(T::SP_8).right(T::SP_8).bottom(T::SP_8).h(sh).max_h(relative(0.82)))))
            // Mini (F101): drawn at window level by MiniPanes over the bounds
            // noted here (F179), so its frames never render the Workbench.
            // The recorder runs whenever the Workbench lays the pane out; a
            // move redraws the layer once, after the frame. A key in the chat
            // is the person getting on with things.
            .when_some(self.mini.clone(), |d, m| {
                let (layer, clip) = m.read(cx).anchor();
                let id = m.entity_id();
                d.child(canvas(move |b, window, cx| {
                    let c = window.content_mask().bounds.intersect(&b);
                    if layer.get() != b || clip.get() != c { layer.set(b); clip.set(c); cx.defer(move |cx| cx.notify(id)); }
                }, |_, _, _, _| {}).absolute().inset_0())
            })
            .when(self.mini.is_some(), |d| d.capture_key_down(move |_, _, cx| crate::mini::acted(&chat, cx)))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_2().border_dashed().border_color(T::accent()))
            // (Deferred, as the reference below: the composer's box is
            // reached through the window, which a drop's handler is inside.)
            .on_drop(move |p: &ExternalPaths, _, cx| { let (c1, p) = (c1.clone(), p.paths().to_vec()); cx.defer(move |cx| crate::composer::os_drop(&c1, p, cx)); cx.stop_propagation(); });
        w::ref_target(body, move |text, _, cx| { let (c2, t) = (c2.clone(), text.to_string()); cx.defer(move |cx| crate::composer::ref_drop(&c2, &t, cx)) }).into_any_element()
    }
}
