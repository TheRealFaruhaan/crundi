//! The live hub (ARCHITECTURE.md, "How server data becomes view state"):
//! awaits the live link's channel on the UI thread (no polling), takes
//! everything waiting as one batch, routes it in one update, then waits a
//! frame before the next: at most one update per frame however fast the
//! server streams (as Zed's terminal batches its events).

use crate::{registry, Stores};
use crundi_core::live::Event;
use futures::{channel::mpsc::UnboundedReceiver, StreamExt as _};
use gpui::*;
use std::time::Duration;

/// One frame at 144 Hz, rounded up.
pub const FRAME: Duration = Duration::from_millis(8);

/// Run the hub until the channel closes (sign-out drops the link).
pub fn run(stores: Stores, mut rx: UnboundedReceiver<Event>, cx: &mut App) -> Task<()> {
    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Some(first) = rx.next().await {
            let mut batch = vec![first];
            while let Ok(e) = rx.try_recv() { batch.push(e); }
            let s = stores.clone();
            cx.update(|cx| route(&s, batch, cx));
            cx.background_executor().timer(FRAME).await;
        }
    })
}

fn route(stores: &Stores, batch: Vec<Event>, cx: &mut App) {
    let mut state = None;
    for e in &batch {
        match e {
            Event::SignedOut => { stores.session.update(cx, |s, cx| s.sign_out("Your sign-in ended. Sign in again.", cx)); return; }
            Event::Connected(on) => { let on = *on; stores.session.update(cx, |s, cx| s.set_online(on, cx)); }
            Event::Usage(u) => { let u = u.clone(); stores.usage.update(cx, |s, cx| s.set_usage(u, cx)); }
            // Only the last state of the batch matters (each is the whole state).
            Event::State(v) => state = Some(v),
            _ => {}
        }
    }
    if let Some(v) = state { stores.projects.update(cx, |p, cx| p.apply_state(v, cx)); }
    for a in registry::list(cx) {
        if let Some(f) = &a.on_batch { f(stores, &batch, cx); }
    }
}

#[cfg(test)]
mod tests {
    // (Not `super::*`: gpui's `test` macro would shadow the standard one.)
    use super::{run, FRAME};
    use crundi_core::live::Event;
    use crate::registry::{register, Area};
    use serde_json::json;
    use std::{cell::RefCell, rc::Rc};

    #[gpui::test]
    fn a_burst_is_one_update(cx: &mut gpui::TestAppContext) {
        let seen: Rc<RefCell<Vec<usize>>> = Rc::default();
        let s2 = seen.clone();
        let stores = cx.update(|cx| {
            let stores = crate::init(cx);
            register(cx, Area::new("count").on_batch(move |_, b, _| s2.borrow_mut().push(b.len())));
            stores
        });
        let (tx, rx) = futures::channel::mpsc::unbounded();
        // A burst: 200 chat events and a state, before the hub runs.
        for i in 0..200 { tx.unbounded_send(Event::Chat(json!({ "type": "ui-event", "id": "c", "n": i }))).unwrap(); }
        tx.unbounded_send(Event::State(json!({ "terminals": [] }))).unwrap();
        let _task = cx.update(|cx| run(stores.clone(), rx, cx));
        cx.run_until_parked();
        assert_eq!(*seen.borrow(), vec![201], "the whole burst is one batch");
        // The next events wait for the frame: nothing until the clock moves.
        tx.unbounded_send(Event::Connected(true)).unwrap();
        tx.unbounded_send(Event::Connected(false)).unwrap();
        cx.run_until_parked();
        assert_eq!(seen.borrow().len(), 1);
        cx.executor().advance_clock(FRAME);
        cx.run_until_parked();
        assert_eq!(*seen.borrow(), vec![201, 2]);
        cx.read(|cx| assert!(!stores.session.read(cx).online));
    }
}
