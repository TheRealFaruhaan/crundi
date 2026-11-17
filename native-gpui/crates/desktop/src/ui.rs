//! The drawer's view (app/vendor/crundi-browser-drawer.js, the Slint
//! client's ui/browser.slint): the tabs row (a tab per page of this
//! project, its network badge, ×; New tab, the "…" menu, Hide, Close all),
//! the address bar (back, forward, reload, the address with the history's
//! suggestions, Network, Device, Annotate, DevTools), the annotation bar
//! while annotating, then the page (or a panel: History, Clear browsing
//! data, Passwords), with a mark's note field over it.
//!
//! Every control is the widgets crate's. The state is drawer.rs's.

use crate::{drawer, passwords};
use crundi_theme::{IconName, T};
use crundi_webview::{engine as browser, PageView};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};

pub struct BrowserDrawer {
    page: Entity<PageView>,
    address: Entity<w::InputState>,
    note: Entity<w::InputState>,
    hist: Entity<w::InputState>,
    site: Entity<w::InputState>,
    /// The password bar's username, the PIN, the login picked in a fill bar.
    pw_user: Entity<w::InputState>,
    pin: Entity<w::InputState>,
    fill_sel: Option<String>,
    shown_bar: String,
    shown_url: String,
    note_open: bool,
    _subs: Vec<Subscription>,
}

impl BrowserDrawer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let page = cx.new(PageView::new);
        let address = cx.new(|cx| w::InputState::new(window, cx).placeholder("Enter URL or search"));
        let note = cx.new(|cx| w::InputState::new(window, cx).placeholder("Note (Enter to keep, Esc to skip)"));
        let hist = cx.new(|cx| w::InputState::new(window, cx).placeholder("Search history"));
        let site = cx.new(|cx| w::InputState::new(window, cx).placeholder("example.com"));
        let pw_user = cx.new(|cx| w::InputState::new(window, cx).placeholder("Username"));
        let pin = cx.new(|cx| w::InputState::new(window, cx).placeholder("PIN").masked(true));
        let subs = vec![
            cx.subscribe_in(&address, window, |this, st, ev: &w::InputEvent, window, cx| match ev {
                w::InputEvent::PressEnter { .. } => {
                    let typed = st.read(cx).value().trim().to_string();
                    drawer::with(|d| d.suggest.clear());
                    browser::defer(move || drawer::go(&typed));
                    let f = this.page.read(cx).focus_handle().clone();
                    window.focus(&f, cx);
                }
                w::InputEvent::Change => drawer::url_typed(&st.read(cx).value()),
                w::InputEvent::Blur => { drawer::with(|d| d.suggest.clear()); cx.notify(); }
                _ => {}
            }),
            cx.subscribe_in(&note, window, |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::PressEnter { .. } = ev {
                let t = st.read(cx).value().to_string();
                drawer::note(&t, true);
            }),
            cx.subscribe_in(&hist, window, |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev { drawer::history_query(&st.read(cx).value()); }),
            cx.subscribe_in(&pw_user, window, |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::PressEnter { .. } = ev {
                let u = st.read(cx).value().to_string();
                if let Some(t) = drawer::active_tab() { passwords::save(&t.id, &u); }
            }),
            cx.subscribe_in(&pin, window, |_, st, ev: &w::InputEvent, window, cx| if let w::InputEvent::PressEnter { .. } = ev {
                let p = st.read(cx).value().to_string();
                st.update(cx, |s, cx| s.set_value("", window, cx));
                passwords::unlock(&p);
            }),
            cx.subscribe_in(&site, window, |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev {
                let v = st.read(cx).value().to_string();
                drawer::with(|d| d.clr.site = v);
            }),
        ];
        BrowserDrawer { page, address, note, hist, site, pw_user, pin, fill_sel: None, shown_bar: String::new(), shown_url: String::new(), note_open: false, _subs: subs }
    }

    /// After each pump: the page shown and its frame, the address following
    /// the page, the note field, and drawing again when the state moved.
    pub fn pump(&mut self, dirty: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (open, annotating) = drawer::with(|d| (d.open, d.annotating));
        let act = drawer::active_tab();
        let panel = act.as_ref().and_then(|t| t.panel).filter(|_| open);
        self.page.update(cx, |p, cx| {
            if p.annotating != annotating { p.annotating = annotating; cx.notify(); }
            p.set_panel(panel, cx);
            p.pump(window, cx);
        });
        let changed = !browser::take_changed().is_empty();
        let url = act.as_ref().map(|t| if t.url == "about:blank" { String::new() } else { t.url.clone() }).unwrap_or_default();
        if url != self.shown_url && !self.address.read(cx).focus_handle(cx).is_focused(window) {
            self.shown_url = url.clone();
            self.address.update(cx, |st, cx| st.set_value(url, window, cx));
        }
        if let Some(sel) = FILL_PICK.with(|f| f.borrow_mut().take()) { self.fill_sel = Some(sel); cx.notify(); }
        let note_open = drawer::with(|d| d.note.is_some());
        if note_open != self.note_open {
            self.note_open = note_open;
            self.note.update(cx, |st, cx| st.set_value("", window, cx));
            if note_open { self.note.update(cx, |st, cx| st.focus(window, cx)); }
        }
        // A new save offer: its username in the bar's field.
        let bar = act.as_ref().and_then(|t| passwords::bar(&t.id).map(|b| (t.id.clone(), b)));
        let key = match &bar { Some((t, passwords::Bar::Save { offer, .. })) => format!("{t}:{offer}"), Some((t, _)) => format!("{t}:fill"), None => String::new() };
        if key != self.shown_bar {
            self.shown_bar = key;
            self.fill_sel = None;
            if let Some((_, passwords::Bar::Save { username, .. })) = &bar { let u = username.clone(); self.pw_user.update(cx, |st, cx| st.set_value(u, window, cx)); }
            cx.notify();
        }
        let panel_name = drawer::with(|d| d.panel.clone());
        if dirty && panel_name == "clear" {
            let site = drawer::with(|d| d.clr.site.clone());
            if self.site.read(cx).value() != site.as_str() { self.site.update(cx, |st, cx| st.set_value(site, window, cx)); }
        }
        if dirty || changed { cx.notify(); }
    }

    fn tabs_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = w::tab_row();
        for (n, t) in drawer::tab_views().into_iter().enumerate() {
            let (id, id2) = (t.id.clone(), t.id.clone());
            let mut tab = w::tab(t.title.clone(), t.active);
            if t.sleeping { tab = tab.child(div().text_size(T::TEXT_10).text_color(T::text_muted()).child(IconName::Moon)); }
            if !t.badge.is_empty() { tab = tab.child(w::tag(t.badge, if t.bad { "error" } else { "accent" })); }
            tab = tab.child(w::icon_button(("tab-x", n), IconName::X, "Close tab").on_click(move |_, _, _| { let id = id2.clone(); browser::defer(move || drawer::close_tab(&id)); }));
            let _tip = t.tip;
            row = row.child(div().id(("tab", n)).cursor_pointer().child(tab)
                .on_click(move |_, _, _| { let id = id.clone(); browser::defer(move || drawer::activate(&id)); }));
        }
        let _ = cx;
        row.child(w::icon_button("new", IconName::Plus, "New tab").on_click(|_, _, _| {
                let net = drawer::active_tab().map(|t| t.network).unwrap_or_else(|| "client".into());
                match drawer::new_tab(&crate::ctx::project(), drawer::START, &net, true, false) { Ok(_) => drawer::show_active(), Err(e) => crate::ctx::toast_error(&e) }
            }))
            .child(div().flex_1())
            .child(w::pick_menu("menu", IconName::Ellipsis, "History and browsing data", "", || pick_items("menu"), |id, _, _| pick("menu", id)))
            .child(w::icon_button("hide", IconName::ChevronRight, "Hide the browser (pages keep running)").on_click(|_, _, _| drawer::close()))
            .child(w::icon_button("closeall", IconName::X, "Close the browser: every tab in this project").on_click(|_, _, _| browser::defer(drawer::close_all)))
    }

    fn address_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let act = drawer::active_tab();
        let i = act.as_ref().and_then(|t| t.panel);
        let (back, fwd, loading) = i.map(|i| browser::with(|st| st.panels.get(i).map(|p| (p.can_back, p.can_fwd, p.loading)).unwrap_or_default())).unwrap_or_default();
        let (net_tip, tone, dev_tip, dev_on) = drawer::bar_state();
        let annotating = drawer::with(|d| d.annotating);
        let has = act.is_some();
        let logins = act.as_ref().map(|t| passwords::count_for(&t.url)).unwrap_or(0);
        let net_icon = match tone { "server" => IconName::Server, "hp" => IconName::Globe, "bad" => IconName::TriangleAlert, _ => IconName::Network };
        let hist = |h: i32| move |_: &ClickEvent, _: &mut Window, _: &mut App| { if let Some(i) = drawer::active_tab().and_then(|t| t.panel) { browser::defer(move || browser::history(i, h)); } };
        let _ = window;
        w::toolbar()
            .child(w::disabled(w::icon_button("back", IconName::ChevronLeft, "Back"), !back).on_click(hist(-1)))
            .child(w::disabled(w::icon_button("forward", IconName::ChevronRight, "Forward"), !fwd).on_click(hist(1)))
            .child(w::disabled(w::icon_button("reload", if loading { IconName::X } else { IconName::RefreshCw }, "Reload"), !has).on_click(hist(0)))
            .child(div().flex_1().min_w_0().child(w::input(&self.address)))
            .when(logins > 0, |d| d.child(w::icon_button("pw-lock", IconName::Lock, if logins == 1 { "A saved login for this site".to_string() } else { format!("{logins} saved logins for this site") })
                .on_click(|_, _, _| passwords::site_menu())))
            .child(w::pick_menu("net", net_icon, net_tip, "This tab’s network", || pick_items("net"), |id, _, _| pick("net", id)))
            .child(w::pick_menu("device", if dev_on { IconName::Smartphone } else { IconName::Monitor }, dev_tip, "Device size", || pick_items("dev"), |id, _, _| pick("dev", id)))
            .child(w::disabled(w::icon_toggle("annotate", IconName::MessageSquare, "Annotate the page", annotating), !has).on_click(cx.listener(|_, _, _, _| { let on = !drawer::with(|d| d.annotating); drawer::annotate(on); })))
            .child(w::disabled(w::icon_button("devtools", IconName::Code, "DevTools"), !has).on_click(|_, _, _| browser::defer(|| if let Some(i) = drawer::active_tab().and_then(|t| t.panel) { browser::devtools(i); })))
    }

    fn annotation_bar(&self) -> impl IntoElement {
        let tool = drawer::with(|d| d.tool.clone());
        let (count, (css_ok, css_tip), (shot_ok, shot_tip)) = drawer::ann_bar();
        let chat_tip = drawer::with(|d| d.chat.as_ref().and_then(|c| d.chats.iter().find(|x| &x.0 == c)).map(|x| format!("Chat to send to: {}", x.1)).unwrap_or_else(|| "No chat open in this project".into()));
        let t = |id: &'static str, icon: IconName, tip: &'static str| w::icon_toggle(id, icon, tip, tool == id).on_click(move |_, _, _| drawer::set_tool(id));
        w::toolbar()
            .child(t("pick", IconName::Layers, "Select an element and annotate it (its CSS selector goes with the note)"))
            .child(t("pin", IconName::Pin, "Draw: a pin with a note"))
            .child(t("box", IconName::Square, "Draw: a box"))
            .child(t("pen", IconName::Pencil, "Draw: freehand"))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).text_color(T::text_secondary()).child(count))
            .child(w::pick_menu("chat", IconName::MessageSquare, chat_tip, "Chat to send to", || { drawer::fill_chats(); pick_items("chat") }, |id, _, _| pick("chat", id)))
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Send"))
            .child(w::disabled(w::icon_button("send-css", IconName::FileText, css_tip), !css_ok).on_click(|_, _, _| drawer::send(false)))
            .child(w::disabled(w::icon_button("send-shot", IconName::Image, shot_tip), !shot_ok).on_click(|_, _, _| drawer::send(true)))
            .child(w::icon_button("clear-marks", IconName::Trash, "Remove your marks").on_click(|_, _, _| if let Some(i) = drawer::active_tab().and_then(|t| t.panel) { browser::clear_marks(i); }))
            .child(w::icon_button("done", IconName::Check, "Done annotating").on_click(|_, _, _| drawer::annotate(false)))
    }

    /// The password bar under the address bar (renderPw): save, update or fill.
    fn pw_bar(&self, tab: &str, b: passwords::Bar) -> impl IntoElement {
        let host = |o: &str| o.split("://").nth(1).unwrap_or(o).to_string();
        let row = div().flex().flex_none().items_center().gap(T::SP_6).px(T::SP_8).py(T::SP_6).bg(T::bg_tertiary()).border_b_1().border_color(T::border())
            .child(div().text_size(T::ICON_SM).text_color(T::accent()).child(IconName::Lock));
        let (t1, t2, t3) = (tab.to_string(), tab.to_string(), tab.to_string());
        match b {
            passwords::Bar::Save { origin, username, update: false, .. } => {
                let _ = username;
                let user = self.pw_user.clone();
                row.child(div().text_size(T::TEXT_12).text_color(T::text()).child(format!("Save the password for {}?", host(&origin))))
                    .child(div().w(T::DRAWER_NOTE_W).child(w::input(&self.pw_user)))
                    .child(div().flex_1())
                    .child(w::primary("pw-save", "Save").on_click(move |_, _, cx| passwords::save(&t1, &user.read(cx).value())))
                    .child(w::button("pw-never", "Never").tooltip("Never offer to save passwords for this site").on_click(move |_, _, _| passwords::never(&t2)))
                    .child(w::icon_button("pw-x", IconName::X, "Not now").on_click(move |_, _, _| passwords::dismiss(&t3)))
            }
            passwords::Bar::Save { origin, username, update: true, .. } => {
                row.child(div().flex_1().min_w_0().text_size(T::TEXT_12).text_color(T::text()).flex().gap(T::SP_4)
                        .child("Update the saved password for").child(div().font_weight(T::W_BOLD).child(if username.is_empty() { "this login".to_string() } else { username }))
                        .child(format!("on {}?", host(&origin))))
                    .child(w::primary("pw-save", "Update").on_click(move |_, _, _| passwords::save(&t1, "")))
                    .child(w::icon_button("pw-x", IconName::X, "Not now").on_click(move |_, _, _| passwords::dismiss(&t3)))
            }
            passwords::Bar::Fill { origin, logins } => {
                let sel = self.fill_sel.clone().filter(|s| logins.iter().any(|l| &l.0 == s)).unwrap_or_else(|| logins[0].0.clone());
                let name = logins.iter().find(|l| l.0 == sel).map(|l| if l.1.is_empty() { "(no username)".to_string() } else { l.1.clone() }).unwrap_or_default();
                let items: Vec<w::PickItem> = logins.iter().map(|l| w::PickItem { id: l.0.clone(), label: if l.1.is_empty() { "(no username)".into() } else { l.1.clone().into() }, kind: "item", on: l.0 == sel, icon: None }).collect();
                let several = logins.len() > 1;
                row.child(div().text_size(T::TEXT_12).text_color(T::text()).child(format!("Fill your saved login for {}?", host(&origin))))
                    .child(div().text_size(T::TEXT_12).font_weight(T::W_BOLD).text_color(T::text()).child(name))
                    .when(several, |d| d.child(w::pick_menu("pw-pick", IconName::ChevronDown, "Choose a login", "", move || items.clone(), |id, _, _| FILL_PICK.with(|f| *f.borrow_mut() = Some(id.to_string())))))
                    .child(div().flex_1())
                    .child(w::primary("pw-fill", "Fill").on_click(move |_, _, _| passwords::fill(&t1, &sel)))
                    .child(w::icon_button("pw-x", IconName::X, "Not for this site (until the app restarts)").on_click(move |_, _, _| passwords::dismiss(&t3)))
            }
        }
    }

    fn panel_view(&self, which: &str) -> AnyElement {
        let head = w::toolbar()
            .child(div().flex_1().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text())
                .child(match which { "history" => "History", "clear" => "Clear browsing data", "passwords" => "Saved passwords", _ => "Homepool for this browser" }))
            .when(which == "history", |d| d.child(w::button("to-clear", "Clear browsing data…").on_click(|_, _, _| drawer::open_panel("clear"))))
            .child(w::button("panel-close", "Close").on_click(|_, _, _| drawer::panel_close()));
        let body: AnyElement = match which {
            "history" => {
                let (rows, empty) = drawer::history_rows();
                let mut list = div().flex().flex_col().gap(T::SP_2);
                if rows.is_empty() { list = list.child(w::empty(empty)); }
                for (n, (url, title, ago)) in rows.into_iter().enumerate() {
                    let u = url.clone();
                    list = list.child(w::list_row(("hist", n), IconName::Clock, if title.is_empty() { url.clone() } else { title }, url, ago, false)
                        .on_click(move |_, _, _| { let u = u.clone(); browser::defer(move || drawer::history_open(&u)); }));
                }
                div().flex().flex_col().gap(T::SP_8).p(T::SP_12).child(w::input(&self.hist)).child(list).into_any_element()
            }
            "clear" => {
                let c = drawer::with(|d| d.clr.clone());
                let set = |f: fn(&mut drawer::Clear, bool)| move |on: &bool, _: &mut Window, _: &mut App| { drawer::with(|d| f(&mut d.clr, *on)); drawer::render(); };
                div().flex().flex_col().gap(T::SP_10).p(T::SP_12)
                    .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("For this computer’s browser: every tab in every project, on every network. Open tabs stay open. For one site, the back and forward lists are left as they are."))
                    .child(w::segmented("clr-scope", &["Every site", "Only this site"], c.site_only as usize, |i, _, _| { drawer::with(|d| d.clr.site_only = *i == 1); drawer::render(); }))
                    .when(c.site_only, |d| d.child(w::input(&self.site)))
                    .child(w::check("clr-history", "Browsing history", c.history, set(|c, on| c.history = on)))
                    .child(w::check("clr-cookies", "Cookies and site data", c.cookies, set(|c, on| c.cookies = on)))
                    .child(w::check("clr-cache", "Cached images and files", c.cache, set(|c, on| c.cache = on)))
                    .child(w::check("clr-passwords", "Saved passwords", c.passwords, set(|c, on| c.passwords = on)))
                    .child(div().pl(T::SP_24).text_size(T::TEXT_11).text_color(T::text_muted()).child("Every saved login, on every site (not limited to one site)"))
                    .child(div().flex().gap(T::SP_8).child(w::danger("clr-run", "Clear").on_click(|_, _, _| drawer::clear_run())))
                    .into_any_element()
            }
            "passwords" => {
                let (available, reason) = passwords::status();
                let (rows, never) = passwords::panel_rows();
                let pin_for = passwords::pin_for();
                let mut d = div().flex().flex_col().gap(T::SP_8).p(T::SP_12);
                if !available && !reason.is_empty() { d = d.child(div().text_size(T::TEXT_12).text_color(T::error_text()).child(reason)); }
                d = d.child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Kept on this computer only, encrypted for your account on it, as Chrome keeps them. Filling needs no PIN; showing one does. Chats can never read them."));
                if rows.is_empty() { d = d.child(div().text_size(T::TEXT_13).text_color(T::text_muted()).child("No saved passwords yet. Sign in to a site in a drawer tab and choose Save.")); }
                for (n, (id, user, host)) in rows.into_iter().enumerate() {
                    let shown = passwords::revealed(&id);
                    let (i1, i2) = (id.clone(), id.clone());
                    d = d.child(div().flex().items_center().gap(T::SP_8).py(T::SP_4).border_b_1().border_color(T::border_subtle())
                        .child(div().flex().flex_col().flex_1().min_w_0()
                            .child(div().truncate().text_size(T::TEXT_13).text_color(T::text()).child(if user.is_empty() { "(no username)".to_string() } else { user }))
                            .child(div().truncate().text_size(T::TEXT_11).text_color(T::text_muted()).child(host)))
                        .child(div().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text_secondary()).child(shown.clone().unwrap_or_else(|| "••••••••".into())))
                        .child(w::button(("pw-show", n), if shown.is_some() { "Hide" } else { "Show" }).on_click(move |_, _, _| passwords::show(&i1)))
                        .child(w::icon_button(("pw-del", n), IconName::Trash, "Delete").on_click(move |_, _, _| passwords::remove(&i2))));
                    if pin_for.as_deref() == Some(id.as_str()) {
                        let pin = self.pin.clone();
                        d = d.child(div().flex().items_center().gap(T::SP_8)
                            .child(div().flex_1().child(w::input(&self.pin)))
                            .child(w::primary("pw-unlock", "Unlock").on_click(move |_, _, cx| passwords::unlock(&pin.read(cx).value()))));
                    }
                }
                if !never.is_empty() {
                    d = d.child(div().pt(T::SP_8).text_size(T::TEXT_12).text_color(T::text_secondary()).child("Never offered for:"));
                    for (n, o) in never.into_iter().enumerate() {
                        let o2 = o.clone();
                        d = d.child(div().flex().items_center().gap(T::SP_8)
                            .child(div().flex_1().truncate().text_size(T::TEXT_13).text_color(T::text()).child(o.split("://").nth(1).unwrap_or(&o).to_string()))
                            .child(w::button(("pw-unnever", n), "Remove").on_click(move |_, _, _| passwords::unnever(&o2))));
                    }
                }
                d.into_any_element()
            }
            _ => w::empty("Homepool is not available on this server.").into_any_element(),
        };
        div().flex().flex_col().flex_1().min_h_0().bg(T::bg_secondary()).child(head)
            .child(div().id("drawer-panel").flex_1().min_h_0().overflow_y_scroll().child(body)).into_any_element()
    }
}

thread_local! {
    /// The login picked in a fill bar's menu (taken by the next pump).
    static FILL_PICK: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// A popover's rows as the widgets crate's pick menu takes them.
fn pick_items(which: &str) -> Vec<w::PickItem> {
    drawer::pop_items(which).1.into_iter().map(|p| w::PickItem { id: p.id, label: p.label.into(), kind: p.kind, on: p.on, icon: p.icon }).collect()
}

/// A row picked in a popover (the drawer's pop_pick, as its popovers do).
fn pick(which: &str, id: &str) {
    drawer::with(|d| d.pop = which.to_string());
    let id = id.to_string();
    browser::defer(move || drawer::pop_pick(&id));
}

impl Render for BrowserDrawer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let act = drawer::active_tab();
        let (annotating, panel, suggest) = drawer::with(|d| (d.annotating, d.panel.clone(), d.suggest.clone()));
        let diag = act.as_ref().map(|t| t.diag.clone()).unwrap_or_default();
        let focused = self.address.read(cx).focus_handle(cx).is_focused(window);
        let body: AnyElement = if !panel.is_empty() {
            self.panel_view(&panel)
        } else if act.is_some() {
            let note = drawer::note_at();
            div().relative().flex_1().min_h_0().child(self.page.clone())
                .when_some(note, |d, (x, y)| d.child(div().absolute().left(px(x)).top(px(y) + T::SP_4).w(T::DRAWER_NOTE_W)
                    .p(T::SP_6).rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border())
                    .on_key_down(|e: &KeyDownEvent, _, _| if e.keystroke.key == "escape" { drawer::note("", false); })
                    .child(w::input(&self.note))))
                .into_any_element()
        } else {
            w::empty("No browser tabs in this project yet.")
                .child(w::button("open", "Open a tab").on_click(|_, _, _| {
                    match drawer::new_tab(&crate::ctx::project(), drawer::START, "client", true, true) { Ok(_) => drawer::show_active(), Err(e) => crate::ctx::toast_error(&e) }
                }))
                .into_any_element()
        };
        let mut root = div().relative().flex().flex_col().size_full().min_h_0().min_w_0().bg(T::bg_secondary()).overflow_hidden()
            .child(self.tabs_row(cx))
            .child(self.address_bar(window, cx));
        if !diag.is_empty() {
            root = root.child(div().px(T::SP_10).py(T::SP_4).text_size(T::TEXT_12).text_color(T::error_text()).bg(T::red_dim()).child(diag));
        }
        if let Some(t) = &act { if let Some(b) = passwords::bar(&t.id) { root = root.child(self.pw_bar(&t.id, b)); } }
        if annotating && act.is_some() { root = root.child(self.annotation_bar()); }
        root = root.child(body);
        // The address field's suggestions, under it.
        if focused && !suggest.is_empty() {
            let mut list = w::card().absolute().top(T::DRAWER_BARS_H).left(T::DRAWER_NAV_W).right(T::DRAWER_TOOLS_W).p(T::SP_4).bg(T::bg_secondary());
            for (n, h) in suggest.into_iter().enumerate() {
                let u = h.url.clone();
                list = list.child(w::list_row(("sug", n), IconName::Clock, if h.title.is_empty() { h.url.clone() } else { h.title.clone() }, h.url, "", false)
                    .on_mouse_down(MouseButton::Left, move |_, _, _| { let u = u.clone(); drawer::with(|d| d.suggest.clear()); browser::defer(move || drawer::go(&u)); }));
            }
            root = root.child(list);
        }
        root
    }
}
