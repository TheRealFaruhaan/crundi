//! Pages Workspace shows live (the web's `<iframe>`, `<video>` and `<audio>`):
//! a PDF in a file window (feOpenViewer), and a PDF, video or audio in the
//! media lightbox (openMediaLightbox). The browser is the Desktop area's
//! (crates/desktop, crates/webview): it registers a host at its init
//! (`set_host`, the same hook as crundi_chat's live frames and
//! crundi_operations::web::set_host). The host makes the page's view the
//! first time a box is drawn (kept by `key`), and drops it when the box is
//! no longer drawn. Without a host (the browser not built in, or not
//! started), the box shows "Open" centred, which opens the address in the
//! system's browser (as the chat's frames do without one).
//!
//! A Global holds only this hook (ARCHITECTURE.md rule 1), read in render
//! of the window or lightbox that shows the page.

use crundi_theme::T;
use gpui::*;
use std::rc::Rc;

pub trait PageHost {
    /// The browser can show pages.
    fn started(&self) -> bool;
    /// An element drawing `url` live (it fills the box it is put in), its
    /// view kept by `key` across frames.
    fn frame(&self, key: &str, url: &str, window: &mut Window, cx: &mut App) -> AnyElement;
}

struct Host(Rc<dyn PageHost>);
impl Global for Host {}

/// The Desktop area's browser (at its init).
pub fn set_host(cx: &mut App, host: Rc<dyn PageHost>) { cx.set_global(Host(host)); }

/// Whether a page host is set and started (the driver's).
pub fn live(cx: &App) -> bool { cx.try_global::<Host>().is_some_and(|h| h.0.started()) }

/// The page live in a box (absolute, filling it), or "Open".
pub fn page(key: &str, url: &str, window: &mut Window, cx: &mut App) -> AnyElement {
    if let Some(h) = cx.try_global::<Host>().map(|h| h.0.clone()).filter(|h| h.started()) {
        return div().absolute().inset_0().child(h.frame(key, url, window, cx)).into_any_element();
    }
    let u = url.to_string();
    div().absolute().inset_0().flex().items_center().justify_center()
        .child(crundi_widgets::link(SharedString::from(format!("page-open-{key}")), "Open").text_size(T::TEXT_13)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| cx.open_url(&u)))
        .into_any_element()
}
