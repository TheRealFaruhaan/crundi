//! The desktop area (AREAS.md): the browser drawer (beside the whole project
//! area, below the top bar, as the PC client's app/vendor/crundi-browser-drawer.js)
//! on the webview crate's pages, and later the Browsers tab, the native
//! host's full browser protocol, notifications and the tray, the Inbox, the
//! chat bubble, saved passwords, the homepool node and Cru.
//!
//! The foundation's drawer is the GPUI test's: one tab, the tabs row, the
//! address bar (back, forward, reload, the address, the Network and Device
//! menus, Annotate, DevTools), the annotation bar while annotating, the page.

use crundi_shell::{Area, Drawer};
use crundi_theme::{IconName, T};
use crundi_webview::{self as web, WebPage};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::json;
use std::rc::Rc;

const NETWORKS: &[&str] = &["This computer's network", "The server's network"];

pub struct BrowserDrawer {
    page: Entity<WebPage>,
    address: Entity<w::InputState>,
    shown_url: String,
    network: usize,
    _sub: Subscription,
}

impl BrowserDrawer {
    pub fn new(home: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let page = cx.new(|cx| WebPage::new(home, cx));
        let address = cx.new(|cx| w::InputState::new(window, cx).placeholder("Enter URL or search"));
        let p2 = page.clone();
        let sub = cx.subscribe_in(&address, window, move |_, st, ev: &w::InputEvent, _, cx| {
            if let w::InputEvent::PressEnter { .. } = ev {
                let typed = st.read(cx).value().trim().to_string();
                if typed.is_empty() { return; }
                let url = if typed.contains("://") { typed } else if typed.contains('.') && !typed.contains(' ') { format!("https://{typed}") } else { format!("https://duckduckgo.com/?q={}", typed.replace(' ', "+")) };
                let open = p2.read(cx).tab;
                if !open { p2.update(cx, |p, cx| { p.home = url; p.open_tab(cx); }); } else { web::nav("load", &url); }
            }
        });
        BrowserDrawer { page, address, shown_url: String::new(), network: 0, _sub: sub }
    }

    /// After each CEF pump: the page's new frame, and the address follows the page.
    pub fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (_, url, _, _) = web::page_state();
        if url != self.shown_url && !self.address.read(cx).focus_handle(cx).is_focused(window) {
            self.shown_url = url.clone();
            self.address.update(cx, |st, cx| st.set_value(url, window, cx));
        }
        self.page.update(cx, |p, cx| p.pump(window, cx));
    }

    pub fn has_pages(&self, cx: &App) -> bool { self.page.read(cx).tab }
}

impl Render for BrowserDrawer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, url, back, fwd) = web::page_state();
        let (tab_open, annotate, marks, device) = { let p = self.page.read(cx); (p.tab, p.annotate, p.strokes.len(), p.device) };
        let tab_title = if !title.is_empty() { title } else if !url.is_empty() { url } else { "New tab".into() };
        let page = self.page.clone();
        let tabs = w::tab_row()
            .when(tab_open, |d| d.child(w::tab(tab_title, true)
                .child(w::icon_button("tab-x", IconName::X, "Close tab").on_click({ let p = page.clone(); move |_, _, cx| p.update(cx, |p, cx| p.close_tab(cx)) }))))
            .child(w::icon_button("new", IconName::Plus, "New tab").on_click({ let p = page.clone(); move |_, _, cx| if !p.read(cx).tab { p.update(cx, |p, cx| p.open_tab(cx)); } }))
            .child(div().flex_1())
            .child(w::icon_button("menu", IconName::Ellipsis, "History and browsing data"))
            .child(w::icon_button("hide", IconName::ChevronRight, "Hide the browser (pages keep running)").on_click(|_, _, cx| crundi_shell::set_drawer(cx, false)))
            .child(w::icon_button("closeall", IconName::X, "Close the browser: every tab in this project").on_click({ let p = page.clone(); move |_, _, cx| { p.update(cx, |p, cx| p.close_tab(cx)); crundi_shell::set_drawer(cx, false); } }));
        let devices: Vec<&'static str> = web::DEVICES.iter().map(|d| d.0).collect();
        let bar = w::toolbar()
            .child(w::disabled(w::icon_button("back", IconName::ChevronLeft, "Back"), !back).on_click(|_, _, _| web::nav("back", "")))
            .child(w::disabled(w::icon_button("forward", IconName::ChevronRight, "Forward"), !fwd).on_click(|_, _, _| web::nav("forward", "")))
            .child(w::icon_button("reload", IconName::RefreshCw, "Reload").on_click(|_, _, _| web::nav("reload", "")))
            .child(div().flex_1().min_w_0().child(w::input(&self.address)))
            .child(w::menu_button("net", IconName::Network, "Network", NETWORKS, self.network, cx.entity().downgrade(), |this, i, cx| { this.network = i; cx.notify(); }))
            .child(w::menu_button("device", IconName::Smartphone, "Device size", &devices, device, cx.entity().downgrade(), |this, i, cx| { this.page.update(cx, |p, cx| p.set_device(i, cx)); cx.notify(); }))
            .child(w::icon_toggle("annotate", IconName::MessageSquare, "Annotate the page", annotate).on_click({ let p = page.clone(); move |_, _, cx| p.update(cx, |p, cx| { p.annotate = !p.annotate; cx.notify(); }) }))
            .child(w::icon_button("devtools", IconName::Terminal, "DevTools"));
        let body = if tab_open {
            div().flex().flex_col().flex_1().min_h_0()
                .when(annotate, |d| d.child(w::toolbar()
                    .child(w::icon_toggle("pen", IconName::Pencil, "Draw: freehand", true))
                    .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(match marks { 0 => "Draw on the page".to_string(), 1 => "1 mark".into(), n => format!("{n} marks") }))
                    .child(div().flex_1())
                    .child(w::icon_button("clear", IconName::Trash, "Remove your marks").on_click({ let p = page.clone(); move |_, _, cx| p.update(cx, |p, cx| { p.strokes.clear(); cx.notify(); }) }))))
                .child(self.page.clone())
                .into_any_element()
        } else {
            w::empty("No browser tabs in this project yet.")
                .child(w::button("open", "Open a tab").on_click({ let p = page.clone(); move |_, _, cx| p.update(cx, |p, cx| p.open_tab(cx)) }))
                .into_any_element()
        };
        div().flex().flex_col().size_full().min_h_0().min_w_0().bg(T::bg_secondary()).overflow_hidden().child(tabs).child(bar).child(body)
    }
}

/// The drawer, for the app's pump.
pub struct DrawerHandle(pub Entity<BrowserDrawer>);
impl Global for DrawerHandle {}

/// Make the drawer and register it, and the Browsers tab (once, after the window opens).
pub fn init(window: &mut Window, cx: &mut App) {
    let home = std::env::var("CRUNDI_URL").unwrap_or_else(|_| "about:blank".into());
    let drawer = cx.new(|cx| BrowserDrawer::new(home, window, cx));
    cx.set_global(DrawerHandle(drawer.clone()));
    let d2 = drawer.clone();
    crundi_shell::register_drawer(cx, Drawer { view: drawer.into(), has_pages: Rc::new(move |cx| d2.read(cx).has_pages(cx)) });
    crundi_shell::register(cx, Area::new("desktop")
        .tabs(&["browsers"], |_, _, _| w::empty("The Browsers tab comes with phase 3 of the GPUI client.").into_any_element())
        .test_ui(|name, args, _, cx| match name {
            "ping" => Some(json!({ "ok": true, "area": "desktop" })),
            "drawer" => { crundi_shell::set_drawer(cx, args.first().is_some_and(|a| a == "true")); Some(json!({ "ok": true })) }
            _ => None,
        }));
}

/// After each CEF pump (the app's): the drawer's page and address.
pub fn pump(window: &mut Window, cx: &mut App) {
    if let Some(d) = cx.try_global::<DrawerHandle>().map(|d| d.0.clone()) { d.update(cx, |d, cx| d.pump(window, cx)); }
}
