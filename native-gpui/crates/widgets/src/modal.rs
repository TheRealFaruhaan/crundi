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

#[derive(Default)]
struct Host(Option<Open>);
impl Global for Host {}

/// Open the modal over the window (replacing one already open).
pub fn modal(window: &mut Window, cx: &mut App, width: Pixels, body: AnyView, on_close: impl Fn(&mut Window, &mut App) + 'static) {
    let open = Open { width, body, on_close: Rc::new(on_close), focus: cx.focus_handle(), scroll: ScrollHandle::new() };
    cx.set_global(Host(Some(open)));
    window.refresh();
}

/// A view modal is open.
pub fn modal_open(cx: &App) -> bool { cx.try_global::<Host>().is_some_and(|h| h.0.is_some()) }

/// Close it (Esc, the driver, the body's own answer); `on_close` runs.
pub(crate) fn close(window: &mut Window, cx: &mut App) -> bool {
    if !cx.has_global::<Host>() { return false; }
    let Some(open) = cx.global_mut::<Host>().0.take() else { return false };
    (open.on_close)(window, cx);
    window.refresh();
    true
}

/// The open modal, for the shell to draw over the window (None: none open).
pub fn render_modal(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let h = cx.try_global::<Host>()?.0.as_ref()?;
    let m = Modal {
        id: "view-modal",
        width: h.width,
        focus: h.focus.clone(),
        scroll: h.scroll.clone(),
        // The scrim does not close it.
        on_close: Rc::new(|_, _| {}),
        on_key: Rc::new(|e: &KeyDownEvent, window, cx| if e.keystroke.key == "escape" { close(window, cx); }),
    };
    Some(overlay_modal(m, window, vec![h.body.clone().into_any_element()]).into_any_element())
}
