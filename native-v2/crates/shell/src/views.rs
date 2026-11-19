//! The window's views (ARCHITECTURE.md, "The view tree"). The Shell is
//! layout only: its children (TopBar, Sidebar, ProjectBar, each tab's body,
//! the drawer) are cached views side by side, each observing only the stores
//! it shows. Layout and wording are the PC client's (native-gpui's shell, the
//! web's #topbar, #sidebar, .tab-bar).

use crate::{registry, session::Step, Stores, TABS};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use std::collections::HashMap;

/// The browser drawer's view (Desktop's: `Shell::set_drawer_view`) and
/// which side it sits on.
#[derive(Clone)]
pub struct Drawer { pub view: AnyView, pub left: bool }

/// Where a window bar was pressed: the window moves only once the mouse
/// moves from there, so a click on a bar's button is a click (on X11 the
/// move takes the pointer until release, eating the click).
#[derive(Default)]
struct BarPress(Option<Point<Pixels>>);

impl BarPress {
    fn drag(&mut self, e: &MouseMoveEvent, window: &mut Window) {
        let Some(from) = self.0 else { return };
        if e.pressed_button != Some(MouseButton::Left) { self.0 = None; return; }
        let d = e.position - from;
        if f32::from(d.x).abs() + f32::from(d.y).abs() >= 4. {
            self.0 = None;
            window.start_window_move();
        }
    }
}

// ─── The Shell ───

pub struct Shell {
    stores: Stores,
    top: Entity<TopBar>,
    side: Entity<Sidebar>,
    bar: Entity<ProjectBar>,
    sign_in: Entity<SignIn>,
    /// Each tab's body, made the first time it is shown.
    tabs: HashMap<String, AnyView>,
    missing: HashMap<String, Entity<NotBuilt>>,
    drawer: Option<Drawer>,
    modal: Entity<w::ModalLayer>,
    ask: AnyView,
    dragging: bool,
    root_focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl Shell {
    pub fn new(stores: Stores, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let top = cx.new(|cx| TopBar::new(stores.clone(), cx));
        let side = cx.new(|cx| Sidebar::new(stores.clone(), cx));
        let bar = cx.new(|cx| ProjectBar::new(stores.clone(), cx));
        let sign_in = cx.new(|cx| SignIn::new(stores.clone(), window, cx));
        // The Shell re-renders only when its layout changes: signed in or out
        // (the session's event, not its every notify), the tab, the fold, the drawer.
        let subs = vec![
            cx.subscribe(&stores.session, |_, _, _: &crate::SessionEvent, cx| cx.notify()),
            cx.observe(&stores.nav, |_, _, cx| cx.notify()),
        ];
        let modal = w::modal_layer(cx);
        let ask = w::ask_layer(cx);
        cx.set_global(ShellHandle(cx.entity().downgrade()));
        Shell { stores, top, side, bar, sign_in, tabs: HashMap::new(), missing: HashMap::new(), drawer: None, modal, ask, dragging: false, root_focus: cx.focus_handle(), _subs: subs }
    }

    /// The browser drawer's view (Desktop registers it).
    pub fn set_drawer_view(&mut self, d: Option<Drawer>, cx: &mut Context<Self>) { self.drawer = d; cx.notify(); }

    /// A tab's body: the registered area's view, made once.
    fn body(&mut self, tab: &str, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if !self.tabs.contains_key(tab) {
            if let Some(make) = registry::tab_maker(cx, tab) {
                let v = make(&self.stores, window, cx);
                self.tabs.insert(tab.to_string(), v);
            }
        }
        // Not cached here: a tab's view is layout, its parts are the cached
        // leaves (a cached view re-rendering ignores every cache inside it).
        if let Some(v) = self.tabs.get(tab) { return div().size_full().child(v.clone()).into_any_element(); }
        let t = tab.to_string();
        let v = self.missing.entry(t.clone()).or_insert_with(|| cx.new(|_| NotBuilt(t))).clone();
        v.cached(StyleRefinement::default().size_full()).into_any_element()
    }
}

struct ShellHandle(WeakEntity<Shell>);
impl Global for ShellHandle {}

/// The window's Shell (None before the window opens). For handlers only.
pub fn shell(cx: &App) -> Option<Entity<Shell>> { cx.try_global::<ShellHandle>().and_then(|h| h.0.upgrade()) }

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if window.focused(cx).is_none() { window.focus(&self.root_focus, cx); }
        let root = div().key_context("CrundiWindow").relative().flex().flex_col().size_full().bg(T::bg()).text_color(T::text()).text_size(T::TEXT_13).font(crundi_theme::ui_font())
            .child(div().absolute().size_0().track_focus(&self.root_focus));
        if !self.stores.session.read(cx).signed_in() {
            return root.child(self.sign_in.clone().cached(StyleRefinement::default().size_full())).into_any_element();
        }
        let nav = self.stores.nav.read(cx);
        let (tab, drawer_open, drawer_w) = (nav.tab.clone(), nav.drawer_open, nav.drawer_w);
        let (sw, easing) = nav.sidebar_width();
        // The fold's ease: frames only while it runs (rule 6).
        if easing { window.request_animation_frame(); }
        let body = self.body(&tab, window, cx);
        let project = div().flex().flex_col().flex_1().min_w_0().min_h_0()
            .child(self.bar.clone().cached(StyleRefinement::default().w_full().h(T::TABBAR_H).flex_none()))
            .child(div().flex().flex_col().flex_1().min_h_0().child(body));
        let drawer = self.drawer.clone().filter(|_| drawer_open);
        let left = drawer.as_ref().is_some_and(|d| d.left);
        let split = w::divider("drawer-split", self.dragging)
            .tooltip(w::tooltip("Drag to resize; double-click to reset"))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                if e.click_count >= 2 { this.stores.nav.update(cx, |n, cx| { n.drawer_w = T::DRAWER_W; cx.notify(); }); this.dragging = false; } else { this.dragging = true; }
                cx.notify();
            }));
        let dview = drawer.map(|d| div().flex_none().w(drawer_w).h_full().min_h_0().child(d.view.cached(StyleRefinement::default().size_full())));
        let main = div().flex().flex_1().min_h_0().child(self.side.clone().cached(StyleRefinement::default().w(sw).h_full().flex_none()));
        let main = match dview {
            Some(v) if left => main.child(v).child(split).child(project),
            Some(v) => main.child(project).child(split).child(v),
            None => main.child(project),
        };
        root
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                if !this.dragging { return; }
                if e.pressed_button != Some(MouseButton::Left) { this.dragging = false; cx.notify(); return; }
                let total = window.viewport_size().width;
                let left = this.drawer.as_ref().is_some_and(|d| d.left);
                let side = this.stores.nav.read(cx).sidebar_width().0;
                let wd = if left { e.position.x - side } else { total - e.position.x };
                let wd = wd.clamp(T::DRAWER_MIN, (total - T::DRAWER_RESERVE).max(T::DRAWER_MIN));
                this.stores.nav.update(cx, |n, cx| { n.drawer_w = wd; cx.notify(); });
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| if this.dragging { this.dragging = false; cx.notify(); }))
            .child(self.top.clone().cached(StyleRefinement::default().w_full().h(T::TOPBAR_H).flex_none()))
            .child(main)
            .child(self.modal.clone())
            .child(self.ask.clone())
            .into_any_element()
    }
}

/// A tab no area draws yet.
pub struct NotBuilt(String);
impl Render for NotBuilt {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        w::empty(format!("{} is not built yet in the v2 client.", registry::tab_label(&self.0)))
    }
}

// ─── The top bar ───

/// A usage label at `at` (0-1) of its row: just after the fill's end, or
/// past 85% just inside it; on the row's top (usage) or bottom (time) edge.
fn usage_label(at: f32, top: bool) -> Div {
    let d = div().absolute().whitespace_nowrap().text_size(T::TEXT_9);
    let d = if at > 0.85 { d.right(relative(1.0 - at)).pr(T::SP_6) } else { d.left(relative(at)).pl(T::SP_6) };
    if top { d.top(px(1.)) } else { d.bottom(px(1.)) }
}

/// The logo and project; for the owner the weekly and 5-hour usage bars and
/// their "updated … ago" badge (a click opens Usage); Cru's mark; Telegram's
/// status; the connection badge; the window buttons.
pub struct TopBar { stores: Stores, press: BarPress, _subs: Vec<Subscription> }

impl TopBar {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        let subs = vec![
            cx.observe(&stores.session, |_, _, cx| cx.notify()),
            cx.observe(&stores.projects, |_, _, cx| cx.notify()),
            cx.observe(&stores.usage, |_, _, cx| cx.notify()),
        ];
        TopBar { stores, press: BarPress::default(), _subs: subs }
    }
}

impl Render for TopBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("top-bar");
        let p = self.stores.projects.read(cx);
        let (name, picon) = (p.open_name(), p.icon_of(&p.open));
        let s = self.stores.session.read(cx);
        let (online, server) = (s.online, s.server.clone());
        let u = self.stores.usage.read(cx);
        let now = crundi_core::time::now_secs();
        let usage = u.usage.as_ref().and_then(|(v, at)| crate::usage::rows(v, now).map(|r| (r, at.elapsed().as_secs_f64(), v.get("fetchedAt").and_then(|f| f.as_str()).and_then(crundi_core::time::parse_iso))));
        let tg = u.tg.clone();
        let mut rows = div().absolute().inset_0().flex().flex_col();
        if let Some((r, _, _)) = &usage {
            for (i, row) in r.iter().enumerate() {
                let (used, time) = (row.used / 100.0, row.time.unwrap_or(0.0) / 100.0);
                let over_c = { let t = crate::usage::over_strength(row.over); let c = T::yellow().blend(T::red().opacity(t)); c.opacity(0.18 + 0.4 * t) };
                rows = rows.child(div().relative().flex_1().overflow_hidden().when(i > 0, |d| d.border_t_1().border_color(T::border_subtle()))
                    .child(div().absolute().left(px(0.)).top(px(0.)).bottom(px(0.)).w(relative(time)).bg(T::usage_time()))
                    .child(div().absolute().left(px(0.)).top(px(0.)).bottom(px(0.)).w(relative(used)).bg(T::usage_used()))
                    .when(row.over > 0.0, |d| d.child(div().absolute().top(px(0.)).bottom(px(0.)).left(relative(time)).w(relative(row.over / 100.0)).bg(over_c)))
                    // Each row's labels as the web's (.ublabel, placeUsageLabel):
                    // its usage on the row's top edge where the usage fill
                    // ends, its countdown on the row's bottom edge where the
                    // time fill ends; past 85% inside the end, reading leftward.
                    .child(usage_label(used, true).font_weight(T::W_BOLD).text_color(T::text()).opacity(0.9).child(format!("{} {}%", row.tag, row.used.round())))
                    .when(row.time.is_some(), |d| d.child(usage_label(time, false).flex().items_center().gap(T::SP_2).text_color(T::text_secondary()).opacity(0.7)
                        .child(w::icon(IconName::Clock, T::TEXT_9)).child(row.left.clone()))));
            }
        }
        let stores = self.stores.clone();
        let badge = usage.as_ref().map(|(_, age, fetched)| {
            let age = fetched.map(|f| now - f).unwrap_or(*age);
            w::tip(div().id("usage-updated").flex().items_center().gap(T::SP_4).h(T::SP_24).px(T::SP_10).rounded(T::R_PILL).cursor_pointer()
                .bg(T::bg_hover()).text_size(T::TEXT_11).font_family(T::MONO).text_color(T::text_secondary()).hover(|d| d.text_color(T::text()))
                .child(w::icon(IconName::ChartBar, T::ICON_SM)).child(crate::usage::ago(age))
                .on_click(move |_, window, cx| { registry::run_action(&stores, "open-usage", window, cx); }), "Claude usage: click for its history")
        });
        let tg_el = tg.filter(|(configured, connected, _)| *configured || *connected).map(|(_, connected, n)| {
            let tip = if connected { format!("Telegram: @{}", if n.is_empty() { "bot".to_string() } else { n }) } else { "Telegram: offline".to_string() };
            w::tip(div().id("tg").relative().flex().items_center().justify_center().size(T::ICON_BTN).text_color(T::text_secondary())
                .child(w::icon(IconName::Send, T::ICON_LG))
                .child(div().absolute().right(T::SP_4).bottom(T::SP_4).size(T::SP_6 + px(1.)).rounded(T::R_PILL).border_1().border_color(T::bg_secondary())
                    .bg(if connected { T::green() } else { T::red() })), tip)
        });
        let stores = self.stores.clone();
        let cru = div().id("cru").relative().flex_none().flex().items_center().p(T::SP_2).cursor_pointer()
            .child(w::cru_mark(false))
            .tooltip(w::tooltip("Talk to Cru (voice assistant)"))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| { registry::run_action(&stores, "cru", window, cx); });
        div()
            .id("topbar").relative().flex().size_full()
            .bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, _| this.press.0 = (e.click_count == 1).then_some(e.position)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, _| this.press.drag(e, window)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.press.0 = None))
            .on_click(|e, window, _| if e.click_count() == 2 { window.zoom_window(); })
            .child(rows)
            .child(div().relative().flex().flex_1().items_center().gap(T::SP_12).pl(T::SP_16).pr(T::SP_4)
                .child(w::brand_mark(T::BRAND_HEAD))
                .when(!name.is_empty(), |d| d.child(div().text_color(T::text_muted()).child("/")).children(picon.map(w::project_icon)).child(div().text_color(T::text_secondary()).child(name)))
                .child(div().flex_1())
                .children(badge)
                .child(cru)
                .children(tg_el)
                .child(w::status_badge(if online { "connected" } else { "offline" }, online).id("conn").tooltip(w::tooltip(server)))
                .child(w::icon_button("win-min", IconName::Minus, "Minimize").on_click(|_, window, _| window.minimize_window()))
                .child(w::icon_button("win-max", IconName::Square, if window.is_maximized() { "Restore" } else { "Maximize" }).on_click(|_, window, _| window.zoom_window()))
                .child(w::icon_button("win-close", IconName::X, "Close").on_click(|_, _, cx| cx.quit())))
    }
}

// ─── The sidebar ───

/// The projects sidebar: the header (PROJECTS and the fold button) the top
/// bar's height; rows 32 high, 2 apart; folded, a rail of avatars 48 apart;
/// + Add project and Inbox at the foot.
pub struct Sidebar { stores: Stores, folded: Option<bool>, _subs: Vec<Subscription> }

impl Sidebar {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        let subs = vec![
            cx.observe(&stores.projects, |_, _, cx| cx.notify()),
            // Only the fold changes the sidebar's own drawing (not the tab).
            cx.observe(&stores.nav, |this: &mut Self, nav, cx| { let f = nav.read(cx).folded; if this.folded != Some(f) { this.folded = Some(f); cx.notify(); } }),
        ];
        Sidebar { stores, _subs: subs, folded: None }.with_fold(cx)
    }
    fn with_fold(mut self, cx: &App) -> Self { self.folded = Some(self.stores.nav.read(cx).folded); self }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("sidebar");
        let folded = self.folded.unwrap_or(false);
        let p = self.stores.projects.read(cx);
        let mut list = div().id("projects").flex().flex_col().flex_1().pt(T::SP_4).pb(T::SP_4).px(if folded { T::SP_6 } else { T::SP_8 }).gap(T::SP_2).overflow_y_scroll();
        for (i, pr) in p.list.iter().enumerate() {
            let on = pr.alias == p.open;
            let icon = p.icon_of(&pr.alias);
            let row = if folded {
                div().flex().items_center().justify_center().h(T::SP_48)
                    .child(w::tip(w::rail_avatar(crundi_core::head::project_initials(&pr.name), pr.dot, on, icon).id(("avatar", i)).cursor_pointer(), pr.name.clone()))
                    .into_any_element()
            } else { w::sidebar_item(pr.name.clone(), pr.dot, on, icon).into_any_element() };
            let alias = pr.alias.clone();
            let projects = self.stores.projects.clone();
            list = list.child(div().id(("project", i)).cursor_pointer().child(row)
                .on_click(move |_, _, cx| projects.update(cx, |p, cx| p.pick(&alias, cx))));
        }
        let nav = self.stores.nav.clone();
        let fold = w::icon_button("fold", if folded { IconName::ChevronsRight } else { IconName::ChevronsLeft }, if folded { "Show the projects" } else { "Fold the projects" })
            .on_click(move |_, _, cx| nav.update(cx, |n, cx| n.toggle_fold(cx)));
        let head = div().flex().flex_none().items_center().h(T::TOPBAR_H).border_b_1().border_color(T::border_subtle());
        let head = if folded { head.justify_center().child(fold) } else {
            head.pl(T::SP_16).pr(T::SP_12)
                .child(div().flex_1().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child("PROJECTS"))
                .child(fold)
        };
        let (s1, s2) = (self.stores.clone(), self.stores.clone());
        div()
            .flex().flex_col().size_full().overflow_hidden().bg(T::bg_secondary()).border_r_1().border_color(T::border())
            .child(head)
            .child(list)
            .child(div().flex().flex_col().gap(T::SP_8).py(T::SP_10).px(if folded { T::SP_6 } else { T::SP_8 }).border_t_1().border_color(T::border_subtle())
                .child(w::dashed_button("add-project", if folded { "+" } else { "+ Add project" }).on_click(move |_, window, cx| { registry::run_action(&s1, "add-project", window, cx); }))
                .child(w::outline_button("inbox", IconName::Mail, if folded { "" } else { "Inbox" }).on_click(move |_, _, cx| s2.nav.update(cx, |n, cx| n.show_tab("inbox", cx)))))
    }
}

// ─── The project bar ───

/// The tabs (scrolling sideways when they don't fit, cut at the edge as the
/// web's .tab-bar), the workbench's "+" after the first, and the Browser
/// button at the right while the drawer is closed.
pub struct ProjectBar { stores: Stores, scroll: ScrollHandle, _subs: Vec<Subscription> }

impl ProjectBar {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        // (The nav store also notifies when a project opens or none is: sync_scope.)
        let subs = vec![cx.observe(&stores.nav, |_, _, cx| cx.notify())];
        ProjectBar { stores, scroll: ScrollHandle::new(), _subs: subs }
    }
}

impl Render for ProjectBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("project-bar");
        let nav = self.stores.nav.read(cx);
        let (on_tab, drawer_open, docker) = (nav.tab.clone(), nav.drawer_open, nav.docker);
        let project_open = !self.stores.projects.read(cx).open.is_empty();
        let mut tabs = div().id("tabbar").flex().flex_1().min_w_0().h_full().items_center().overflow_x_scroll().track_scroll(&self.scroll);
        for (i, (id, label, icon, _)) in TABS.iter().enumerate() {
            // With no project, only the global tabs (and no "+"); Docker only where installed.
            if !crate::nav::tab_shown(id, project_open, docker) { continue; }
            let (id, nav) = (id.to_string(), self.stores.nav.clone());
            tabs = tabs.child(w::nav_tab(icon.clone(), *label, on_tab == id).id(("tab", i)).flex_none().cursor_pointer().on_click(move |_, _, cx| nav.update(cx, |n, cx| n.show_tab(&id, cx))));
            if i == 0 {
                let s = self.stores.clone();
                tabs = tabs.child(w::icon_button("add-pane", IconName::Plus, "Add to workbench").on_click(move |_, window, cx| { registry::run_action(&s, "add-menu", window, cx); }));
            }
        }
        // The wheel scrolls the tabs sideways; this bar alone redraws.
        tabs = tabs.on_scroll(cx.listener(|this, e: &ScrollEvent, window, cx| {
            let sh = &this.scroll;
            let d = e.delta.pixel_delta(window.line_height());
            if d.x.abs() >= d.y.abs() || sh.max_offset().x <= px(0.) { return; }
            let o = sh.offset();
            sh.set_offset(point((o.x + d.y).clamp(-sh.max_offset().x, px(0.)), o.y));
            cx.stop_propagation();
            cx.notify();
        }));
        // The web's strip: tabs past the edge are cut there (no fade) and
        // scroll in sideways; the Browser button stays at the right over it.
        let scroller = div().relative().flex().flex_1().min_w_0().h_full().child(tabs);
        let mut bar = div().flex().size_full().items_center().px(T::SP_12).bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .child(scroller);
        if !drawer_open {
            let s = self.stores.clone();
            bar = bar.child(w::nav_tab(IconName::Globe, "Browser", false).id("browser").flex_none().cursor_pointer()
                .on_click(move |_, window, cx| { if !registry::run_action(&s, "browser", window, cx) { s.nav.update(cx, |n, cx| n.set_drawer(true, cx)); } }));
        }
        bar
    }
}

// ─── Sign-in ───

/// Sign-in while signed out: the server step, then the login step (Google /
/// GitHub / Telegram, password and code, pairing), the browser's code step.
pub struct SignIn {
    stores: Stores,
    server_input: Entity<w::InputState>,
    password_input: Entity<w::InputState>,
    code_input: Entity<w::InputState>,
    pair_input: Entity<w::InputState>,
    press: BarPress,
    _subs: Vec<Subscription>,
}

impl SignIn {
    fn new(stores: Stores, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let server_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("dev.example.com"));
        let password_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("Password").masked(true));
        let code_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("6-digit code"));
        let pair_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("Pairing link"));
        let subs = vec![cx.observe(&stores.session, |_, _, cx| cx.notify())];
        SignIn { stores, server_input, password_input, code_input, pair_input, press: BarPress::default(), _subs: subs }
    }
}

impl Render for SignIn {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.stores.session.read(cx);
        let (step, err, busy, srv) = (s.step.clone(), s.signin_error.clone(), s.signin_busy, s.signin_server.clone());
        let (code, google, github, telegram, password) = (s.oauth_code.clone(), s.offer_google, s.offer_github, s.offer_telegram, s.offer_password);
        let session = self.stores.session.clone();
        let card = w::card().w(T::DIALOG_W_LG).p(T::SP_24).gap(T::SP_12)
            .child(div().flex().items_center().justify_center().gap(T::SP_8).child(w::brand_mark(T::BRAND_HEAD_LG)));
        let oauth = |id: &'static str, label: &'static str, provider: &'static str| {
            let session = session.clone();
            w::button(id, label).on_click(move |_, _, cx| session.update(cx, |s, cx| s.sign_in_oauth(provider, cx)))
        };
        let card = match step {
            Step::Server => card
                .child(div().text_color(T::text_secondary()).text_size(T::TEXT_13).child("Which Crundi server?"))
                .child(w::input(&self.server_input))
                .child(w::disabled(w::primary("continue", if busy { "Checking…" } else { "Continue" }), busy).on_click(cx.listener(|this, _, _, cx| {
                    let v = this.server_input.read(cx).value().to_string();
                    this.stores.session.update(cx, |s, cx| s.check_server(&v, cx));
                }))),
            Step::Login if !code.is_empty() => card
                .child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child("Finish signing in in your browser. Check that it shows this code:"))
                .child(div().flex().justify_center().font_family(T::MONO).text_size(T::TEXT_26).font_weight(T::W_BOLD).text_color(T::accent_hover()).child(code))
                .child(w::button("cancel-sign-in", "Cancel").on_click({ let session = session.clone(); move |_, _, cx| session.update(cx, |s, cx| s.cancel_browser(cx)) })),
            Step::Login => card
                .child(div().flex().items_center().gap(T::SP_8).text_size(T::TEXT_13).text_color(T::text_secondary())
                    .child(format!("Sign in to {srv}"))
                    .child(w::ghost("change-server", "Change").on_click({ let session = session.clone(); move |_, _, cx| session.update(cx, |s, cx| s.change_server(cx)) })))
                .when(google, |d| d.child(oauth("oauth-google", "Sign in with Google", "google")))
                .when(github, |d| d.child(oauth("oauth-github", "Sign in with GitHub", "github")))
                .when(telegram, |d| d.child(oauth("oauth-telegram", "Sign in with Telegram", "telegram")))
                .when(password, |d| d.child(w::input(&self.password_input)).child(w::input(&self.code_input)))
                .child(w::disabled(w::primary("sign-in", if busy { "Signing in…" } else { "Sign in" }), busy).on_click(cx.listener(|this, _, _, cx| {
                    let (p, c) = (this.password_input.read(cx).value().to_string(), this.code_input.read(cx).value().to_string());
                    this.stores.session.update(cx, |s, cx| { let srv = s.signin_server.clone(); s.sign_in_password(&srv, &p, &c, cx) });
                })))
                .child(w::rule())
                .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Or pair with a link from a signed-in session"))
                .child(w::input(&self.pair_input))
                .child(w::button("pair", "Pair").on_click(cx.listener(|this, _, _, cx| {
                    let l = this.pair_input.read(cx).value().to_string();
                    this.stores.session.update(cx, |s, cx| s.sign_in_pair(&l, cx));
                }))),
        };
        div().flex().flex_col().size_full().bg(T::bg())
            .child(div().id("signin-bar").flex().flex_none().items_center().h(T::TOPBAR_H).px(T::SP_4).justify_end()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, _| this.press.0 = Some(e.position)))
                .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, _| this.press.drag(e, window)))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.press.0 = None))
                .child(w::icon_button("win-min", IconName::Minus, "Minimize").on_click(|_, window, _| window.minimize_window()))
                .child(w::icon_button("win-close", IconName::X, "Close").on_click(|_, _, cx| cx.quit())))
            .child(div().flex().flex_1().items_center().justify_center()
                .child(card.when(!err.is_empty(), |d| d.child(div().text_size(T::TEXT_13).text_color(T::red()).child(err)))))
    }
}
