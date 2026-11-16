//! A modal dialog whose body is a part's own view (made by Operations for
//! the resume picker, 2.22; AREAS.md "A new control"): on the window's one
//! dialog layer, centred over the scrim, no title bar and no ×; the scrim
//! does not close it (as the web's); Esc closes it and calls `on_close`.
//! The body redraws itself (it is a view): keep its state in that view.

use gpui::*;
use gpui_component::WindowExt as _;
use std::rc::Rc;

pub fn modal(window: &mut Window, cx: &mut App, width: Pixels, body: AnyView, on_close: impl Fn(&mut Window, &mut App) + 'static) {
    let on_close = Rc::new(on_close);
    window.open_dialog(cx, move |d, _, _| {
        let oc = on_close.clone();
        d.w(width).close_button(false).overlay_closable(false).keyboard(true).centered()
            .child(body.clone())
            .on_close(move |_, w, cx| oc(w, cx))
    });
}
