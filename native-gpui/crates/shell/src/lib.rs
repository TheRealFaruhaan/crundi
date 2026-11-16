//! The window (Lead's, AREAS.md): frameless, with the PC client's top bar
//! and its window buttons; the projects sidebar; the project tab bar; the
//! tab host, where each part draws its tabs (registry.rs); the browser drawer
//! beside the project area, below the top bar, with its divider (Desktop
//! registers the drawer's view); and sign-in while signed out.

pub mod registry;
pub mod session;

use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

pub use registry::{open_bubble, register, register_action, register_add_menu, register_bubble, register_drawer, register_overlay, Area, AreaEvent, Drawer, TABS};
pub use session::{api, call, project, run, signed_in, subscribe, unsubscribe, ws_send, Session, Step};

/// The window's own state: the tab, the sidebar, the drawer, sign-in's fields.
pub struct Shell {
    pub tab: String,
    pub sidebar: bool,
    pub drawer_open: bool,
    pub drawer_w: Pixels,
    dragging: bool,
    server_input: Entity<w::InputState>,
    password_input: Entity<w::InputState>,
    code_input: Entity<w::InputState>,
    pair_input: Entity<w::InputState>,
}

/// The window's Shell, for the parts and the test driver.
pub struct ShellHandle(pub WeakEntity<Shell>);
impl Global for ShellHandle {}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let server_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("dev.example.com"));
        let password_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("Password").masked(true));
        let code_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("6-digit code"));
        let pair_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("Pairing link"));
        cx.set_global(ShellHandle(cx.entity().downgrade()));
        Shell { tab: "workbench".into(), sidebar: true, drawer_open: false, drawer_w: T::DRAWER_W, dragging: false, server_input, password_input, code_input, pair_input }
    }

    /// Show a tab (as a click on it); the parts hear it.
    pub fn show_tab(&mut self, tab: &str, cx: &mut Context<Self>) {
        self.tab = tab.to_string();
        let t = tab.to_string();
        registry::each(cx, move |a, cx| if let Some(f) = &a.on_tab { f(&t, cx) });
        cx.notify();
    }

    pub fn set_drawer(&mut self, open: bool, cx: &mut Context<Self>) { self.drawer_open = open; cx.notify(); }

    /// What the test driver sees of the window (the Slint client's props names).
    pub fn props(&self, cx: &App) -> Value {
        let s = session::get(cx);
        json!({
            "signedIn": s.api.is_some(), "online": s.online,
            "signinError": s.signin_error, "signinBusy": s.signin_busy, "signinChecked": s.signin_checked,
            "signinStep": if s.step == Step::Server { "server" } else { "login" },
            "offerPassword": s.offer_password, "offerTelegram": s.offer_telegram, "offerGoogle": s.offer_google, "offerGithub": s.offer_github,
            "oauthCode": s.oauth_code,
            "server": s.server, "username": s.username,
            "currentProject": s.project, "projectName": session::project_name(cx),
            "currentTab": self.tab, "drawerOpen": self.drawer_open, "sidebar": self.sidebar,
            "projects": s.projects.iter().map(|p| p.get("alias").cloned().unwrap_or(Value::Null)).collect::<Vec<_>>(),
            // The sidebar's dots (the web's priority over the state event's terminals).
            "projectDots": s.projects.iter().map(|p| {
                let alias = p.get("alias").and_then(|a| a.as_str()).unwrap_or("");
                let name = p.get("name").and_then(|n| n.as_str()).filter(|n| !n.is_empty()).unwrap_or(alias);
                json!({ "alias": alias, "state": crundi_core::head::project_dot(&s.terminals, alias), "initials": crundi_core::head::project_initials(name) })
            }).collect::<Vec<_>>(),
        })
    }

    // ─── The top bar ───

    fn top_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = session::get(cx);
        let (name, online, server) = (session::project_name(cx), s.online, s.server.clone());
        div()
            .id("topbar").flex().flex_none().items_center().gap(T::SP_12).h(T::TOPBAR_H).pl(T::SP_16).pr(T::SP_4)
            .bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .on_mouse_down(MouseButton::Left, |e, window, _| if e.click_count == 1 { window.start_window_move(); })
            .on_click(|e, window, _| if e.click_count() == 2 { window.zoom_window(); })
            .child(div().text_color(T::accent()).text_size(T::TEXT_20).child(IconName::Bot))
            .child(div().font_weight(T::W_BOLD).text_size(T::TEXT_20).child("crundi"))
            .when(!name.is_empty(), |d| d.child(div().text_color(T::text_muted()).child("/")).child(div().text_color(T::text_secondary()).child(name)))
            .child(div().flex_1())
            .child(w::status_badge(if online { "connected" } else { "offline" }, online).id("conn").tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(server.clone()).build(window, cx)))
            .child(w::icon_button("win-min", IconName::Minus, "Minimize").on_click(|_, window, _| window.minimize_window()))
            .child(w::icon_button("win-max", IconName::Square, if window.is_maximized() { "Restore" } else { "Maximize" }).on_click(|_, window, _| window.zoom_window()))
            .child(w::icon_button("win-close", IconName::X, "Close").on_click(|_, _, cx| cx.quit()))
    }

    // ─── The sidebar ───

    /// The projects sidebar, as the Slint client's (its geometry is what the
    /// phase tests read): the header (PROJECTS and the fold button) the top
    /// bar's height; rows 32 high, 2 apart; folded, a rail of avatars 48 apart.
    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let s = session::get(cx);
        let folded = !self.sidebar;
        let mut list = div().id("projects").flex().flex_col().flex_1().pt(T::SP_4).pb(T::SP_4).px(if folded { T::SP_6 } else { T::SP_8 }).gap(T::SP_2).overflow_y_scroll();
        for (i, p) in s.projects.iter().enumerate() {
            let alias = p.get("alias").and_then(|a| a.as_str()).unwrap_or("").to_string();
            let name = p.get("name").and_then(|n| n.as_str()).filter(|n| !n.is_empty()).unwrap_or(&alias).to_string();
            let on = alias == s.project;
            let dot = crundi_core::head::project_dot(&s.terminals, &alias);
            let row = if folded {
                div().flex().items_center().justify_center().h(T::SP_48)
                    .child(w::tip(w::rail_avatar(crundi_core::head::project_initials(&name), dot, on).id(("avatar", i)).cursor_pointer(), name.clone()))
                    .into_any_element()
            } else { w::sidebar_item(name, dot, on).into_any_element() };
            list = list.child(div().id(("project", i)).cursor_pointer().child(row)
                .on_click(cx.listener(move |_, _, _, cx| session::select_project(cx, &alias))));
        }
        let fold = w::icon_button("fold", if folded { IconName::ChevronsRight } else { IconName::ChevronsLeft }, if folded { "Show the projects" } else { "Fold the projects" })
            .on_click(cx.listener(|this, _, _, cx| { this.sidebar = !this.sidebar; cx.notify(); }));
        let head = div().flex().flex_none().items_center().h(T::TOPBAR_H).border_b_1().border_color(T::border_subtle());
        let head = if folded { head.justify_center().child(fold) } else {
            head.pl(T::SP_16).pr(T::SP_12)
                .child(div().flex_1().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child("PROJECTS"))
                .child(fold)
        };
        div()
            .flex().flex_col().flex_none().w(if folded { T::SIDEBAR_FOLDED } else { T::SIDEBAR_W }).h_full().bg(T::bg_secondary()).border_r_1().border_color(T::border())
            .child(head)
            .child(list)
            .child(div().flex().flex_col().gap(T::SP_8).py(T::SP_10).px(if folded { T::SP_6 } else { T::SP_8 }).border_t_1().border_color(T::border_subtle())
                .child(w::dashed_button("add-project", if folded { "+" } else { "+ Add project" }).on_click(|_, _, cx| registry::add_project(cx)))
                .child(w::outline_button("inbox", IconName::Mail, if folded { "" } else { "Inbox" }).on_click(cx.listener(|this, _, _, cx| this.show_tab("inbox", cx)))))
    }

    // ─── The project bar ───

    fn project_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut bar = div().id("tabbar").flex().flex_none().items_center().h(T::TABBAR_H).px(T::SP_12).bg(T::bg_secondary()).border_b_1().border_color(T::border()).overflow_x_scroll();
        for (i, (id, label, icon, _)) in TABS.iter().enumerate() {
            let id = id.to_string();
            bar = bar.child(w::nav_tab(icon.clone(), *label, self.tab == id).id(("tab", i)).cursor_pointer().on_click(cx.listener(move |this, _, _, cx| this.show_tab(&id, cx))));
            if i == 0 { bar = bar.child(w::icon_button("add-pane", IconName::Plus, "Add to workbench").on_click(|_, _, cx| registry::add_menu(cx))); }
        }
        bar = bar.child(div().flex_1());
        // The Browser button: gone while the drawer is open (it has its own
        // hide and close); a dot while it is hidden with pages open.
        if !self.drawer_open {
            let pages = registry::drawer(cx).is_some_and(|d| (d.has_pages)(cx));
            bar = bar.child(w::nav_tab(IconName::Globe, "Browser", false).id("browser").cursor_pointer()
                .when(pages, |d| d.child(w::dot(T::accent())))
                .on_click(cx.listener(|this, _, _, cx| this.set_drawer(true, cx))));
        }
        bar
    }

    // ─── Sign-in ───

    fn sign_in(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = session::get(cx);
        let (step, err, busy, srv) = (s.step.clone(), s.signin_error.clone(), s.signin_busy, s.signin_server.clone());
        let (code, google, github, telegram, password) = (s.oauth_code.clone(), s.offer_google, s.offer_github, s.offer_telegram, s.offer_password);
        let card = w::card().w(T::DIALOG_W_LG).p(T::SP_24).gap(T::SP_12)
            .child(div().flex().items_center().justify_center().gap(T::SP_8)
                .child(div().text_color(T::accent()).text_size(T::TEXT_26).child(IconName::Bot))
                .child(div().font_weight(T::W_BOLD).text_size(T::TEXT_26).child("crundi")));
        let card = match step {
            Step::Server => card
                .child(div().text_color(T::text_secondary()).text_size(T::TEXT_13).child("Which Crundi server?"))
                .child(w::input(&self.server_input))
                .child(w::disabled(w::primary("continue", if busy { "Checking…" } else { "Continue" }), busy).on_click(cx.listener(|this, _, _, cx| {
                    let v = this.server_input.read(cx).value().to_string();
                    session::check_server(cx, &v);
                }))),
            // Waiting for the browser: the code to compare with the page's, and Cancel.
            Step::Login if !code.is_empty() => card
                .child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child("Finish signing in in your browser. Check that it shows this code:"))
                .child(div().flex().justify_center().font_family(T::MONO).text_size(T::TEXT_26).font_weight(T::W_BOLD).text_color(T::accent_hover()).child(code))
                .child(w::button("cancel-sign-in", "Cancel").on_click(|_, _, cx| session::cancel_browser(cx))),
            Step::Login => card
                .child(div().flex().items_center().gap(T::SP_8).text_size(T::TEXT_13).text_color(T::text_secondary())
                    .child(format!("Sign in to {srv}"))
                    .child(w::ghost("change-server", "Change").on_click(|_, _, cx| { session::with(cx, |s| { s.step = Step::Server; s.signin_error.clear(); }); cx.refresh_windows(); })))
                .when(google || github || telegram, |d| d
                    .when(google, |d| d.child(w::button("oauth-google", "Sign in with Google").on_click(|_, _, cx| { let srv = session::get(cx).signin_server.clone(); session::sign_in_oauth(cx, &srv, "google"); })))
                    .when(github, |d| d.child(w::button("oauth-github", "Sign in with GitHub").on_click(|_, _, cx| { let srv = session::get(cx).signin_server.clone(); session::sign_in_oauth(cx, &srv, "github"); })))
                    .when(telegram, |d| d.child(w::button("oauth-telegram", "Sign in with Telegram").on_click(|_, _, cx| { let srv = session::get(cx).signin_server.clone(); session::sign_in_oauth(cx, &srv, "telegram"); }))))
                .when(password, |d| d.child(w::input(&self.password_input)).child(w::input(&self.code_input)))
                .child(w::disabled(w::primary("sign-in", if busy { "Signing in…" } else { "Sign in" }), busy).on_click(cx.listener(|this, _, _, cx| {
                    let (p, c) = (this.password_input.read(cx).value().to_string(), this.code_input.read(cx).value().to_string());
                    let srv = session::get(cx).signin_server.clone();
                    session::sign_in_password(cx, &srv, &p, &c);
                })))
                .child(w::rule())
                .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Or pair with a link from a signed-in session"))
                .child(w::input(&self.pair_input))
                .child(w::button("pair", "Pair").on_click(cx.listener(|this, _, _, cx| {
                    let l = this.pair_input.read(cx).value().to_string();
                    session::sign_in_pair(cx, &l);
                }))),
        };
        let _ = window;
        div().flex().flex_col().size_full().bg(T::bg())
            .child(div().id("signin-bar").flex().flex_none().items_center().h(T::TOPBAR_H).px(T::SP_4).justify_end()
                .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                .child(w::icon_button("win-min", IconName::Minus, "Minimize").on_click(|_, window, _| window.minimize_window()))
                .child(w::icon_button("win-close", IconName::X, "Close").on_click(|_, _, cx| cx.quit())))
            .child(div().flex().flex_1().items_center().justify_center()
                .child(card.when(!err.is_empty(), |d| d.child(div().text_size(T::TEXT_13).text_color(T::red()).child(err)))))
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div().flex().flex_col().size_full().bg(T::bg()).text_color(T::text()).text_size(T::TEXT_13).font(crundi_theme::ui_font());
        if !session::signed_in(cx) {
            return root.child(self.sign_in(window, cx)).into_any_element();
        }
        // The tab host: the part that registered the tab draws it.
        let tab = self.tab.clone();
        let body = registry::render_tab(&tab, window, cx)
            .unwrap_or_else(|| w::empty(format!("{} is not built yet in the GPUI client.", registry::tab_label(&tab))).into_any_element());
        let project = div().flex().flex_col().flex_1().min_w_0().min_h_0()
            .child(self.project_bar(cx))
            .child(div().flex().flex_col().flex_1().min_h_0().child(body));
        // The drawer: beside the whole project area, below the top bar.
        let drawer = registry::drawer(cx).filter(|_| self.drawer_open);
        let main = div().flex().flex_1().min_h_0()
            .child(self.sidebar(cx))
            .child(project)
            .when_some(drawer, |d, dr| d
                .child(w::divider("drawer-split", self.dragging)
                    .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new("Drag to resize; double-click to reset").build(window, cx))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                        if e.click_count >= 2 { this.drawer_w = T::DRAWER_W; this.dragging = false; } else { this.dragging = true; }
                        cx.notify();
                    })))
                .child(div().flex_none().w(self.drawer_w).h_full().min_h_0().child(dr.view)));
        root
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                if !this.dragging { return; }
                if e.pressed_button != Some(MouseButton::Left) { this.dragging = false; cx.notify(); return; }
                let total = window.viewport_size().width;
                this.drawer_w = (total - e.position.x).clamp(T::DRAWER_MIN, (total - T::DRAWER_RESERVE).max(T::DRAWER_MIN));
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| if this.dragging { this.dragging = false; cx.notify(); }))
            .child(self.top_bar(window, cx))
            .child(main)
            // The parts' layers (not modal: Cru's screen), then a view modal (w::modal), over the window.
            .children(registry::overlays(cx))
            .children(w::render_modal(window, cx))
            // The ask layer (the one confirm / prompt and context menu), once, over the window.
            .child(w::ask_layer(cx))
            .into_any_element()
    }
}

/// The window's Shell (None before the window opens).
pub fn shell(cx: &App) -> Option<Entity<Shell>> {
    cx.try_global::<ShellHandle>().and_then(|h| h.0.upgrade())
}

/// Hide or show the drawer (the drawer's own hide and close buttons).
pub fn set_drawer(cx: &mut App, open: bool) {
    if let Some(s) = shell(cx) { s.update(cx, |s, cx| s.set_drawer(open, cx)); }
}

/// The tab on show.
pub fn current_tab(cx: &App) -> String { shell(cx).map(|s| s.read(cx).tab.clone()).unwrap_or_default() }

/// Start the session (the saved login, if any). Call once after the theme.
pub fn init(cx: &mut App) {
    session::init(cx);
    // The session in the driver's "state" (the Slint client's names).
    registry::register(cx, Area::new("shell").state(|_, cx| {
        let s = session::get(cx);
        let aliases: Vec<Value> = s.projects.iter().filter_map(|p| p.get("alias").cloned()).collect();
        json!({ "projects": aliases, "project": s.project, "signedIn": s.api.is_some() })
    }));
}
