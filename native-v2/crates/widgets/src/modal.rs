//! A modal dialog whose body is a part's own view (Operations' resume
//! picker, 2.22), drawn with the one modal look (`overlay_modal`, the web's
//! `.cc-modal-back` + `.cc-modal`; Planning's and Operations' modals unified
//! by the Lead). `modal` opens it; the shell draws it (`render_modal`, over
//! the window); `close_dialog` closes it. The scrim does not close it (as
//! the web's); Esc closes it and calls `on_close`. The body keeps its own
//! state, focus and keys (it is a view).

use crate::overlay::{overlay_modal, Modal};
use gpui::*;
use std::rc::Rc;

struct Open {
    width: Pixels,
    body: AnyView,
    on_close: Rc<dyn Fn(&mut Window, &mut App)>,
    focus: FocusHandle,
    scroll: ScrollHandle,
}

/// The modal layer (v2): one view, over the window, that draws the open
/// modal. Opening or closing a modal notifies this view alone (never a
/// window refresh, ARCHITECTURE.md rule 2).
#[derive(Default)]
pub struct ModalLayer(Option<Open>);

struct Host(Entity<ModalLayer>);
impl Global for Host {}

/// The window's modal layer (made the first time; the shell draws it).
pub fn modal_layer(cx: &mut App) -> Entity<ModalLayer> {
    if let Some(h) = cx.try_global::<Host>() { return h.0.clone(); }
    let e = cx.new(|_| ModalLayer::default());
    cx.set_global(Host(e.clone()));
    e
}

/// Open the modal over the window (replacing one already open).
pub fn modal(_window: &mut Window, cx: &mut App, width: Pixels, body: AnyView, on_close: impl Fn(&mut Window, &mut App) + 'static) {
    let open = Open { width, body, on_close: Rc::new(on_close), focus: cx.focus_handle(), scroll: ScrollHandle::new() };
    modal_layer(cx).update(cx, |l, cx| { l.0 = Some(open); cx.notify(); });
}

/// A view modal is open.
pub fn modal_open(cx: &App) -> bool { cx.try_global::<Host>().is_some_and(|h| h.0.read(cx).0.is_some()) }

/// Close it (Esc, the driver, the body's own answer); `on_close` runs.
pub(crate) fn close(window: &mut Window, cx: &mut App) -> bool {
    let Some(h) = cx.try_global::<Host>().map(|h| h.0.clone()) else { return false };
    let Some(open) = h.update(cx, |l, cx| { let o = l.0.take(); if o.is_some() { cx.notify(); } o }) else { return false };
    (open.on_close)(window, cx);
    true
}

impl Render for ModalLayer {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(h) = self.0.as_ref() else { return div().into_any_element() };
        let m = Modal {
            id: "view-modal",
            width: h.width,
            focus: h.focus.clone(),
            scroll: h.scroll.clone(),
            // The scrim does not close it.
            on_close: Rc::new(|_, _| {}),
            on_key: Rc::new(|e: &KeyDownEvent, window, cx| if e.keystroke.key == "escape" { close(window, cx); }),
        };
        overlay_modal(m, window, vec![h.body.clone().into_any_element()]).into_any_element()
    }
}
