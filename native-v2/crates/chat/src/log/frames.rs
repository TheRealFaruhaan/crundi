//! 2.16f: live frames (a show_embed item: a web page, or stored HTML, live
//! in its box). The browser is the Desktop area's (crates/desktop,
//! crates/webview): it registers a frame host at its init (`set_host`, the
//! same kind of hook as crundi_operations::web::set_host). The log asks the
//! host for an element drawing the page at the box's size (the host makes
//! the view the first time a box is drawn, hides it when the box is not
//! drawn, frees it after the unload grace, caps how many live, forwards the
//! pointer and the keys: native-gpui's frames.rs rules, all the browser's).
//! A stored page's HTML is fetched once (/api/chat-media) and given as a
//! data: address. Without a host (the browser not built in, or not started),
//! the box shows "Open" centred, as native-gpui did without CEF.

use super::LogView;
use crundi_core::media;
use crundi_theme::T;
use gpui::*;
use std::collections::HashMap;
use std::rc::Rc;

/// The Desktop area's browser for chat frames.
pub trait FrameHost {
    /// The browser can show pages.
    fn started(&self) -> bool;
    /// An element drawing `url` live in a box (it fills the box it is put
    /// in), the view kept by `key` (the chat and the item) across frames.
    fn frame(&self, key: &str, url: &str, window: &mut Window, cx: &mut App) -> AnyElement;
}

struct Host(Rc<dyn FrameHost>);
impl Global for Host {}

/// The Desktop area's frame host (at its init).
pub fn set_host(cx: &mut App, host: Rc<dyn FrameHost>) { cx.set_global(Host(host)); }

/// Stored pages' HTML by (chat, name): None while it is being fetched.
#[derive(Default)]
struct Pages(HashMap<(String, String), Option<String>>);
impl Global for Pages {}

/// The page to open for a target ("stored:<name>" or the address): None
/// until a stored page has come.
fn url_of(chat: &str, target: &str, cx: &mut App) -> Option<String> {
    let Some(name) = target.strip_prefix("stored:") else { return Some(target.to_string()) };
    let k = (chat.to_string(), name.to_string());
    match cx.default_global::<Pages>().0.get(&k) {
        Some(Some(u)) => return Some(u.clone()),
        Some(None) => return None,
        None => {}
    }
    let api = crundi_shell::api(cx)?;
    cx.default_global::<Pages>().0.insert(k.clone(), None);
    let path = format!("/api/chat-media?session={}&name={}", k.0, k.1);
    crundi_shell::run(cx, move || api.get_bytes(&path).ok().filter(|r| r.0 == 200).map(|r| { use base64::Engine; format!("data:text/html;charset=utf-8;base64,{}", base64::engine::general_purpose::STANDARD.encode(r.1)) }), move |u, cx| {
        match u { Some(u) => { cx.default_global::<Pages>().0.insert(k.clone(), Some(u)); } None => { cx.default_global::<Pages>().0.remove(&k); } }
        if let Some(s) = crate::find(cx, &k.0) { s.update(cx, |_, cx| cx.notify()); }
    });
    None
}

/// Inside a frame's box: the live page (the host's), or "Open".
pub fn view(_v: &mut LogView, key: &str, chat: &str, target: &str, item: &media::Item, window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    let host = cx.try_global::<Host>().map(|h| h.0.clone()).filter(|h| h.started());
    if let Some(h) = host {
        if let Some(url) = url_of(chat, target, cx) {
            return div().absolute().inset_0().child(h.frame(key, &url, window, cx)).into_any_element();
        }
        return div().into_any_element();
    }
    let (c, n, u) = (chat.to_string(), item.name.clone(), if item.url.is_empty() { item.src.clone() } else { item.url.clone() });
    div().absolute().inset_0().flex().items_center().justify_center()
        .child(div().id(SharedString::from(format!("fr-open-{key}"))).text_size(T::TEXT_13).text_color(T::accent_hover()).underline().cursor_pointer().child("Open")
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| super::media::open_frame(&c, &n, &u, cx)))
        .into_any_element()
}

/// Whether a frame host is set and started (the driver's).
pub fn live(cx: &App) -> bool { cx.try_global::<Host>().is_some_and(|h| h.0.started()) }
