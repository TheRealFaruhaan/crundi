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
        ChatPane { store, log, composer, mini }
    }

    /// A second pane over the same chat (F65: the bubble): its log has its
    /// own list (its own scroll and measurements) over the chat's rows.
    pub fn second(store: Entity<ChatStore>, window: &mut Window, cx: &mut App) -> Self {
        let log = cx.new(|cx| LogView::second(store.clone(), cx));
        let composer = cx.new(|cx| Composer::new(store.clone(), window, cx));
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
        // The whole chat takes a drop (the web's cc-root): files from the
        // system (the dashed outline) and a reference dragged from another
        // panel (w::ref_target); either goes in at the box's caret (2.7).
        let (c1, c2) = (self.composer.clone(), self.composer.clone());
        let chat = self.store.read(cx).id.clone();
        let body = div().id(("chat-body", self.store.entity_id().as_u64() as usize)).relative().flex().flex_col().size_full().min_h_0().bg(T::bg())
            .child(self.log.clone().cached(StyleRefinement::default().w_full().flex_1().min_h_0()))
            .when(!parked, |d| d.child(self.composer.clone().cached(StyleRefinement::default().w_full().h(h).flex_none())))
            .when_some(sched.filter(|_| !parked), |d, (v, sh)| d.child(v.cached(StyleRefinement::default().absolute().left(T::SP_8).right(T::SP_8).bottom(T::SP_8).h(sh).max_h(relative(0.82)))))
            // Mini (F101): its own leaf over the whole chat; a key in the
            // chat is the person getting on with things.
            .when_some(self.mini.clone(), |d, m| d.child(m.cached(StyleRefinement::default().absolute().inset_0())))
            .when(self.mini.is_some(), |d| d.capture_key_down(move |_, _, cx| crate::mini::acted(&chat, cx)))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_2().border_dashed().border_color(T::accent()))
            .on_drop(move |p: &ExternalPaths, _, cx| { crate::composer::os_drop(&c1, p.paths().to_vec(), cx); cx.stop_propagation(); });
        w::ref_target(body, move |text, _, cx| crate::composer::ref_drop(&c2, text, cx)).into_any_element()
    }
}
