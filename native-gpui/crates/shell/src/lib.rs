//! The window (Lead's, AREAS.md): frameless, with the PC client's top bar
//! and its window buttons; the projects sidebar; the project tab bar; the
//! tab host, where each part draws its tabs (registry.rs); the browser drawer
//! beside the project area, below the top bar, with its divider (Desktop
//! registers the drawer's view); and sign-in while signed out.

pub mod registry;
pub mod project_icon;
pub mod session;
pub mod usage;
pub mod ptt;

use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

pub use registry::{open_bubble, register, register_action, register_settings_part, register_add_menu, register_bubble, register_drawer, register_overlay, Area, AreaEvent, Drawer, TABS};
pub use session::{api, call, project, run, signed_in, subscribe, unsubscribe, ws_send, Session, Step};

/// The window's own state: the tab, the sidebar, the drawer, sign-in's fields.
pub struct Shell {
    pub tab: String,
    pub sidebar: bool,
    pub drawer_open: bool,
    /// Each tab's body view (TabHost), cached by GPUI.
    tab_hosts: std::collections::HashMap<String, Entity<TabHost>>,
    pub drawer_w: Pixels,
    dragging: bool,
    /// Where the left button went down on a window bar: the window moves
    /// only once the mouse moves from there, so a click on a bar's button
    /// (minimise, maximise, close) is a click the first time (on X11 the
    /// move takes the pointer until release, eating the click).
    bar_press: Option<Point<Pixels>>,
    /// The sidebar folding or unfolding: its width eases (the web's 220 ms):
    /// from, and since when.
    fold_from: Option<(Pixels, std::time::Instant)>,
    /// The project tab bar's sideways scroll (its overflow cue reads it).
    tab_scroll: ScrollHandle,
    /// Ctrl+Space is held (Cru's push to talk).
    ptt_held: bool,
    /// Bumped on each hover of Cru's mark: a hover timer that outlives it does nothing.
    cru_hover: u64,
    server_input: Entity<w::InputState>,
    password_input: Entity<w::InputState>,
    code_input: Entity<w::InputState>,
    pair_input: Entity<w::InputState>,
    /// The window's own focus (the root key context's): where the keys go when
    /// nothing else has them, so the window's shortcuts (Cru's push to talk)
    /// reach it whatever was focused and has gone.
    root_focus: FocusHandle,
    _focus_lost: Subscription,
}

/// The window's Shell, for the parts and the test driver.
pub struct ShellHandle(pub WeakEntity<Shell>);
impl Global for ShellHandle {}

/// What Cru's mark in the top bar shows: Cru live (listening, thinking,
/// talking), and its unread reports. Desktop sets it (set_cru_mark).
#[derive(Clone, Default, PartialEq)]
pub struct CruMark { pub live: bool, pub unread: i64 }
impl Global for CruMark {}

/// Where Cru's mark is in the window (as last drawn): Cru's screen opens
/// under it, at its right (the web's anchor). Added by Desktop.
#[derive(Clone, Copy, Default)]
pub struct CruAnchor(pub Bounds<Pixels>);
impl Global for CruAnchor {}
pub fn cru_anchor(cx: &App) -> Option<Bounds<Pixels>> { cx.try_global::<CruAnchor>().map(|a| a.0) }

/// Desktop's Cru tells the top bar; redraws only on a change.
pub fn set_cru_mark(cx: &mut App, live: bool, unread: i64) {
    let m = CruMark { live, unread };
    if cx.try_global::<CruMark>() == Some(&m) { return; }
    cx.set_global(m);
    cx.refresh_windows();
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let server_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("dev.example.com"));
        let password_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("Password").masked(true));
        let code_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("6-digit code"));
        let pair_input = cx.new(|cx| w::InputState::new(window, cx).placeholder("Pairing link"));
        cx.set_global(ShellHandle(cx.entity().downgrade()));
        // A focused element gone (a closed drawer's page or field, Cru's screen): the focus goes to its
        // nearest focusable ancestor still drawn, else to the window's root, never to nothing (GPUI
        // sends keys along the focused element's path only, so with none the window hears no key).
        let focus_lost = cx.on_focus_lost(window, |this, window, cx| {
            let to = window.focus_lost_restore_target(cx).unwrap_or_else(|| this.root_focus.clone());
            window.focus(&to, cx);
        });
        Shell { tab: "workbench".into(), sidebar: true, drawer_open: false, tab_hosts: Default::default(), drawer_w: T::DRAWER_W, dragging: false, bar_press: None, fold_from: None, tab_scroll: ScrollHandle::new(), ptt_held: false, cru_hover: 0, server_input, password_input, code_input, pair_input, root_focus: cx.focus_handle(), _focus_lost: focus_lost }
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
            "usage": s.usage.as_ref().and_then(|(u, _)| usage::rows(u, crundi_core::time::now_secs())).map(|r| r.iter().map(|x| json!({ "tag": x.tag, "used": x.used, "time": x.time, "left": x.left, "over": x.over })).collect::<Vec<_>>()),
            "ptt": ptt::label(&ptt::get()), "pttCapturing": ptt::capturing(cx),
            "cruMark": cx.try_global::<CruMark>().map(|m| json!({ "live": m.live, "unread": m.unread })),
            "telegram": s.tg.as_ref().map(|(c, on, n)| json!({ "configured": c, "connected": on, "name": n })),
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

    /// The top bar (the PC client's): the usage rows behind it (weekly, then
    /// 5-hour: used, the window passed, the slice running ahead of the clock,
    /// the labels); the logo and project; the "updated … ago" badge (a click
    /// opens Usage); Cru; Telegram's status; the connection badge; the
    /// window buttons.
    /// Cru's mark (the PC client's top-bar button, crundi-voice.js attach()):
    /// no outline or background; the bars breathe while Cru is live; a red
    /// dot for unread reports. A click toggles Cru; resting on it opens Cru
    /// (on a wide window, after a moment), and it closes again when the
    /// pointer leaves unless Cru is busy (Desktop's "cru-hover", "cru-leave").
    fn cru_button(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let m = cx.try_global::<CruMark>().cloned().unwrap_or_default();
        let tip: SharedString = if m.unread > 0 { format!("Talk to Cru ({} unread)", m.unread).into() } else { "Talk to Cru (voice assistant)".into() };
        let wide = window.viewport_size().width >= T::CRU_HOVER_MIN_W;
        div().id("cru").relative().flex_none().flex().items_center().p(T::SP_2).cursor_pointer()
            .child(w::cru_mark(m.live))
            // Where the mark is: Cru's screen opens under it (cru_anchor).
            .child(canvas(|b, _, cx| { if cx.try_global::<CruAnchor>().map(|a| a.0) != Some(b) { cx.set_global(CruAnchor(b)); } }, |_, _, _, _| {}).absolute().inset_0())
            .when(m.unread > 0, |d| d.child(w::dot(T::red()).absolute().top(px(1.)).right(px(1.)).border_2().border_color(T::bg_secondary())))
            .tooltip(w::tooltip(tip))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| { this.cru_hover += 1; registry::run_action(cx, "cru"); }))
            .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                this.cru_hover += 1;
                if !*on { registry::run_action(cx, "cru-leave"); return; }
                if !wide { return; }
                let n = this.cru_hover;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(350)).await;
                    let _ = this.update(cx, |this, cx| if this.cru_hover == n { registry::run_action(cx, "cru-hover"); });
                }).detach();
            }))
    }

    /// A window bar dragged: the window moves once the mouse has gone a few
    /// pixels with the left button held (see `bar_press`).
    fn bar_drag(&mut self, e: &MouseMoveEvent, window: &mut Window, _: &mut Context<Self>) {
        let Some(from) = self.bar_press else { return };
        if e.pressed_button != Some(MouseButton::Left) { self.bar_press = None; return; }
        let d = e.position - from;
        if f32::from(d.x).abs() + f32::from(d.y).abs() >= 4. {
            self.bar_press = None;
            window.start_window_move();
        }
    }

    fn top_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open_alias = session::get(cx).project.clone();
        let picon = project_icon::of_project(cx, &open_alias);
        let s = session::get(cx);
        let (name, online, server) = (session::project_name(cx), s.online, s.server.clone());
        let now = crundi_core::time::now_secs();
        let usage = s.usage.as_ref().and_then(|(u, at)| usage::rows(u, now).map(|r| (r, at.elapsed().as_secs_f64(), u.get("fetchedAt").and_then(|f| f.as_str()).and_then(crundi_core::time::parse_iso))));
        let tg = s.tg.clone();
        // The rows behind the bar (no input: the bar's own parts stay on top).
        let mut rows = div().absolute().inset_0().flex().flex_col();
        if let Some((r, _, _)) = &usage {
            for (i, row) in r.iter().enumerate() {
                let (used, time) = (row.used / 100.0, row.time.unwrap_or(0.0) / 100.0);
                let over_c = { let t = usage::over_strength(row.over); let c = T::yellow().blend(T::red().opacity(t)); c.opacity(0.18 + 0.4 * t) };
                rows = rows.child(div().relative().flex_1().overflow_hidden().when(i > 0, |d| d.border_t_1().border_color(T::border_subtle()))
                    .child(div().absolute().left(px(0.)).top(px(0.)).bottom(px(0.)).w(relative(time)).bg(T::usage_time()))
                    .child(div().absolute().left(px(0.)).top(px(0.)).bottom(px(0.)).w(relative(used)).bg(T::usage_used()))
                    .when(row.over > 0.0, |d| d.child(div().absolute().top(px(0.)).bottom(px(0.)).left(relative(time)).w(relative(row.over / 100.0)).bg(over_c)))
                    // The label and its countdown together, in the bar's margin
                    // clear of the header's contents (the Electron client's):
                    // the weekly row's on its top edge, the 5-hour row's on its
                    // bottom edge; each starts where its usage fill ends.
                    .child({
                        let lab = div().absolute().left(relative(used.min(0.85))).pl(T::SP_6).flex().items_center().gap(T::SP_8).text_size(T::TEXT_10)
                            .child(div().font_weight(T::W_BOLD).text_color(T::text()).opacity(0.9).child(format!("{} {}%", row.tag, row.used.round())))
                            .when(row.time.is_some(), |d| d.child(div().flex().items_center().gap(T::SP_2).text_color(T::text_secondary()).opacity(0.7)
                                .child(w::icon(IconName::Clock, T::TEXT_10)).child(row.left.clone())));
                        if i == 0 { lab.top(px(0.)) } else { lab.bottom(px(0.)) }
                    }));
            }
        }
        let badge = usage.as_ref().map(|(_, age, fetched)| {
            let age = fetched.map(|f| now - f).unwrap_or(*age);
            w::tip(div().id("usage-updated").flex().items_center().gap(T::SP_4).h(T::SP_24).px(T::SP_10).rounded(T::R_PILL).cursor_pointer()
                .bg(T::bg_hover()).text_size(T::TEXT_11).font_family(T::MONO).text_color(T::text_secondary()).hover(|d| d.text_color(T::text()))
                .child(w::icon(IconName::ChartBar, T::ICON_SM)).child(usage::ago(age))
                .on_click(|_, _, cx| registry::run_action(cx, "open-usage")), "Claude usage: click for its history")
        });
        let tg_el = tg.filter(|(configured, connected, _)| *configured || *connected).map(|(_, connected, n)| {
            let tip = if connected { format!("Telegram: @{}", if n.is_empty() { "bot".to_string() } else { n }) } else { "Telegram: offline".to_string() };
            w::tip(div().id("tg").relative().flex().items_center().justify_center().size(T::ICON_BTN).text_color(T::text_secondary())
                .child(w::icon(IconName::Send, T::ICON_LG))
                .child(div().absolute().right(T::SP_4).bottom(T::SP_4).size(T::SP_6 + px(1.)).rounded(T::R_PILL).border_1().border_color(T::bg_secondary())
                    .bg(if connected { T::green() } else { T::red() })), tip)
        });
        div()
            .id("topbar").relative().flex().flex_none().h(T::TOPBAR_H)
            .bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, _| this.bar_press = (e.click_count == 1).then_some(e.position)))
            .on_mouse_move(cx.listener(Self::bar_drag))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.bar_press = None))
            .on_click(|e, window, _| if e.click_count() == 2 { window.zoom_window(); })
            .child(rows)
            .child(div().relative().flex().flex_1().items_center().gap(T::SP_12).pl(T::SP_16).pr(T::SP_4)
                .child(w::brand_mark(T::BRAND_HEAD))
                .when(!name.is_empty(), |d| d.child(div().text_color(T::text_muted()).child("/")).children(picon.map(w::project_icon)).child(div().text_color(T::text_secondary()).child(name)))
                .child(div().flex_1())
                .children(badge)
                .child(self.cru_button(window, cx))
                .children(tg_el)
                .child(w::status_badge(if online { "connected" } else { "offline" }, online).id("conn").tooltip(w::tooltip(server.clone())))
                .child(w::icon_button("win-min", IconName::Minus, "Minimize").on_click(|_, window, _| window.minimize_window()))
                .child(w::icon_button("win-max", IconName::Square, if window.is_maximized() { "Restore" } else { "Maximize" }).on_click(|_, window, _| window.zoom_window()))
                .child(w::icon_button("win-close", IconName::X, "Close").on_click(|_, _, cx| cx.quit())))
    }

    // ─── The sidebar ───

    /// The projects sidebar, as the Slint client's (its geometry is what the
    /// phase tests read): the header (PROJECTS and the fold button) the top
    /// bar's height; rows 32 high, 2 apart; folded, a rail of avatars 48 apart.
    /// The sidebar's width now, and whether it is still easing.
    fn sidebar_width(&self, window: &mut Window) -> (Pixels, bool) {
        let to = if self.sidebar { T::SIDEBAR_W } else { T::SIDEBAR_FOLDED };
        let Some((from, at)) = self.fold_from else { return (to, false) };
        let t = (at.elapsed().as_secs_f32() / T::LAYOUT_EASE.as_secs_f32()).min(1.0);
        if t >= 1.0 { return (to, false); }
        let e = 1.0 - (1.0 - t).powi(3);
        let _ = window;
        (from + (to - from) * e, true)
    }

    fn sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (width, easing) = self.sidebar_width(window);
        if easing { window.request_animation_frame(); }
        let urls: Vec<String> = session::get(cx).projects.iter().map(|p| p.get("icon").and_then(|u| u.as_str()).unwrap_or("").to_string()).collect();
        let icons: Vec<_> = urls.iter().map(|u| project_icon::icon(cx, u)).collect();
        let s = session::get(cx);
        let folded = !self.sidebar;
        let mut list = div().id("projects").flex().flex_col().flex_1().pt(T::SP_4).pb(T::SP_4).px(if folded { T::SP_6 } else { T::SP_8 }).gap(T::SP_2).overflow_y_scroll();
        for (i, p) in s.projects.iter().enumerate() {
            let alias = p.get("alias").and_then(|a| a.as_str()).unwrap_or("").to_string();
            let name = p.get("name").and_then(|n| n.as_str()).filter(|n| !n.is_empty()).unwrap_or(&alias).to_string();
            let on = alias == s.project;
            let dot = crundi_core::head::project_dot(&s.terminals, &alias);
            let icon = icons.get(i).cloned().flatten();
            let row = if folded {
                div().flex().items_center().justify_center().h(T::SP_48)
                    .child(w::tip(w::rail_avatar(crundi_core::head::project_initials(&name), dot, on, icon).id(("avatar", i)).cursor_pointer(), name.clone()))
                    .into_any_element()
            } else { w::sidebar_item(name, dot, on, icon).into_any_element() };
            list = list.child(div().id(("project", i)).cursor_pointer().child(row)
                .on_click(cx.listener(move |_, _, _, cx| session::select_project(cx, &alias))));
        }
        let fold = w::icon_button("fold", if folded { IconName::ChevronsRight } else { IconName::ChevronsLeft }, if folded { "Show the projects" } else { "Fold the projects" })
            .on_click(cx.listener(|this, _, window, cx| {
                // From the width drawn now (a fold mid-way eases on from there).
                let from = this.sidebar_width(window).0;
                this.sidebar = !this.sidebar;
                this.fold_from = Some((from, std::time::Instant::now()));
                cx.notify();
            }));
        let head = div().flex().flex_none().items_center().h(T::TOPBAR_H).border_b_1().border_color(T::border_subtle());
        let head = if folded { head.justify_center().child(fold) } else {
            head.pl(T::SP_16).pr(T::SP_12)
                .child(div().flex_1().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child("PROJECTS"))
                .child(fold)
        };
        div()
            .flex().flex_col().flex_none().w(width).overflow_hidden().h_full().bg(T::bg_secondary()).border_r_1().border_color(T::border())
            .child(head)
            .child(list)
            .child(div().flex().flex_col().gap(T::SP_8).py(T::SP_10).px(if folded { T::SP_6 } else { T::SP_8 }).border_t_1().border_color(T::border_subtle())
                .child(w::dashed_button("add-project", if folded { "+" } else { "+ Add project" }).on_click(|_, _, cx| registry::add_project(cx)))
                .child(w::outline_button("inbox", IconName::Mail, if folded { "" } else { "Inbox" }).on_click(cx.listener(|this, _, _, cx| this.show_tab("inbox", cx)))))
    }

    // ─── The project bar ───

    fn project_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // The tabs scroll sideways when they don't fit (the web's, scrollbar
        // hidden); the wheel scrolls them too, and a fade at an edge shows
        // there are tabs past it (F93). The Browser button stays at the
        // right, outside the scroll, as the web's sticky toggle.
        let mut tabs = div().id("tabbar").flex().flex_1().min_w_0().h_full().items_center().overflow_x_scroll().track_scroll(&self.tab_scroll);
        for (i, (id, label, icon, _)) in TABS.iter().enumerate() {
            let id = id.to_string();
            tabs = tabs.child(w::nav_tab(icon.clone(), *label, self.tab == id).id(("tab", i)).flex_none().cursor_pointer().on_click(cx.listener(move |this, _, _, cx| this.show_tab(&id, cx))));
            if i == 0 { tabs = tabs.child(w::icon_button("add-pane", IconName::Plus, "Add to workbench").on_click(|_, _, cx| registry::add_menu(cx))); }
        }
        let sh = self.tab_scroll.clone();
        tabs = tabs.on_scroll_wheel(move |e: &ScrollWheelEvent, window, cx| {
            let d = e.delta.pixel_delta(window.line_height());
            if d.x.abs() >= d.y.abs() || sh.max_offset().x <= px(0.) { return; }
            let o = sh.offset();
            sh.set_offset(point((o.x + d.y).clamp(-sh.max_offset().x, px(0.)), o.y));
            cx.stop_propagation();
            window.refresh();
        });
        let (off, max) = (self.tab_scroll.offset().x, self.tab_scroll.max_offset().x);
        let fade = |right: bool| {
            let (a, b) = (T::bg_secondary().opacity(0.), T::bg_secondary());
            div().absolute().top(px(0.)).bottom(px(0.)).w(T::SP_32)
                .map(|d| if right { d.right(px(0.)).bg(linear_gradient(90., linear_color_stop(a, 0.), linear_color_stop(b, 1.))) } else { d.left(px(0.)).bg(linear_gradient(90., linear_color_stop(b, 0.), linear_color_stop(a, 1.))) })
        };
        let scroller = div().relative().flex().flex_1().min_w_0().h_full().child(tabs)
            .when(off < px(-1.), |d| d.child(fade(false)))
            .when(max > px(1.) && off > -max + px(1.), |d| d.child(fade(true)));
        let mut bar = div().flex().flex_none().items_center().h(T::TABBAR_H).px(T::SP_12).bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .child(scroller);
        // The Browser button: gone while the drawer is open (it has its own
        // hide and close); a dot while it is hidden with pages open.
        if !self.drawer_open {
            let pages = registry::drawer(cx).is_some_and(|d| (d.has_pages)(cx));
            bar = bar.child(w::nav_tab(IconName::Globe, "Browser", false).id("browser").flex_none().cursor_pointer()
                .when(pages, |d| d.child(w::dot(T::accent())))
.on_click(|_, _, cx| cx.defer(browser_button)));
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
                .child(w::brand_mark(T::BRAND_HEAD_LG)));
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
                .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, _| this.bar_press = Some(e.position)))
                .on_mouse_move(cx.listener(Self::bar_drag))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.bar_press = None))
                .child(w::icon_button("win-min", IconName::Minus, "Minimize").on_click(|_, window, _| window.minimize_window()))
                .child(w::icon_button("win-close", IconName::X, "Close").on_click(|_, _, cx| cx.quit())))
            .child(div().flex().flex_1().items_center().justify_center()
                .child(card.when(!err.is_empty(), |d| d.child(div().text_size(T::TEXT_13).text_color(T::red()).child(err)))))
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        draw_stats::count(&draw_stats::RENDERS);
        draw_stats::frame_start();
        self.render_inner(window, cx)
    }
}

impl Shell {
    fn render_inner(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        // Nothing focused (the window just opened): the window's root has the keys.
        if window.focused(cx).is_none() { window.focus(&self.root_focus, cx); }
        // The root key context; its focus on an empty node (a click never moves the focus to it).
        let root = div().key_context("CrundiWindow").flex().flex_col().size_full().bg(T::bg()).text_color(T::text()).text_size(T::TEXT_13).font(crundi_theme::ui_font())
            .child(div().absolute().size_0().track_focus(&self.root_focus))
            .child(canvas(|_, _, _| {}, |_, _, _, _| draw_stats::count(&draw_stats::PAINTS)).absolute().size_0());
        if !session::signed_in(cx) {
            return root.child(self.sign_in(window, cx)).into_any_element();
        }
        // The tab host: the part that registered the tab draws it.
        let tab = self.tab.clone();
        // In a cached view of its own (TabHost): the drawer's frames, the
        // top bar, Cru's mark don't lay out and paint the whole tab again.
        // The workbench tab is not cached itself: its panes are (PaneHost),
        // and a cached view inside a cached one gets nothing (a miss renders
        // the whole subtree again, gpui view.rs). As Zed: the panes are the
        // top cached views, so one pane's change leaves the others' frames.
        let body = if tab == "workbench" {
            registry::render_tab(&tab, window, cx).unwrap_or_else(|| div().into_any_element())
        } else if registry::has_tab(&tab, cx) {
            let host = self.tab_hosts.entry(tab.clone()).or_insert_with(|| { let t = tab.clone(); cx.new(|_| TabHost { tab: t }) }).clone();
            host.cached(StyleRefinement::default().size_full()).into_any_element()
        } else {
            w::empty(format!("{} is not built yet in the GPUI client.", registry::tab_label(&tab))).into_any_element()
        };
        let project = div().flex().flex_col().flex_1().min_w_0().min_h_0()
            .child(self.project_bar(cx))
            .child(div().flex().flex_col().flex_1().min_h_0().child(body));
        // The drawer: beside the whole project area, below the top bar.
        // On the left (its menu's "Open on the left"), between the projects
        // list and the project: sidebar, drawer, divider, project (the web's
        // .brd-on-left order). The owner, 18 Nov: it didn't switch sides.
        let drawer = registry::drawer(cx).filter(|_| self.drawer_open);
        let left = drawer.as_ref().is_some_and(|d| (d.left)(cx));
        let split = w::divider("drawer-split", self.dragging)
            .tooltip(w::tooltip("Drag to resize; double-click to reset"))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                if e.click_count >= 2 { this.drawer_w = T::DRAWER_W; this.dragging = false; } else { this.dragging = true; }
                cx.notify();
            }));
        // The drawer cached too: a page's frames redraw the drawer, not the project.
        let dview = drawer.map(|dr| div().flex_none().w(self.drawer_w).h_full().min_h_0().child(dr.view.cached(StyleRefinement::default().size_full())));
        let main = div().flex().flex_1().min_h_0().child(self.sidebar(window, cx));
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
                let left = registry::drawer(cx).is_some_and(|d| (d.left)(cx));
                // On the left the drawer starts where the projects list ends.
                let w = if left { e.position.x - if this.sidebar { T::SIDEBAR_W } else { T::SIDEBAR_FOLDED } } else { total - e.position.x };
                this.drawer_w = w.clamp(T::DRAWER_MIN, (total - T::DRAWER_RESERVE).max(T::DRAWER_MIN));
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| if this.dragging { this.dragging = false; cx.notify(); }))
            // Cru's push to talk (the web's default, Ctrl+Space): held, Cru
            // opens listening (or stops talking); let go, what was heard goes.
            // The shortcut is per device (ptt.rs); Settings may be waiting for
            // the next key to become it.
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if ptt::capture(cx, &e.keystroke) { cx.stop_propagation(); return; }
                if ptt::matches(&ptt::get(), &e.keystroke) {
                    cx.stop_propagation();
                    if !this.ptt_held && !e.is_held { this.ptt_held = true; registry::run_action(cx, "cru-ptt"); }
                }
            }))
            .capture_key_up(cx.listener(|this, e: &KeyUpEvent, _, cx| {
                if this.ptt_held && ptt::releases(&ptt::get(), &e.keystroke.key) { this.ptt_held = false; registry::run_action(cx, "cru-ptt-up"); }
            }))
            .child(self.top_bar(window, cx))
            .child(main)
            // The parts' layers (not modal: Cru's screen), then a view modal (w::modal), over the window.
            .children(registry::overlays(cx))
            .children(w::render_modal(window, cx))
            // The ask layer (the one confirm / prompt and context menu), once, over the window.
            .child(w::ask_layer(cx))
            // Last painted: the frame's work so far (CRUNDI_DRAW_STATS).
            .child(canvas(|_, _, _| {}, |_, _, _, _| draw_stats::frame_done()).absolute().size_0())
            .into_any_element()
    }
}

/// The window's Shell (None before the window opens).
pub fn shell(cx: &App) -> Option<Entity<Shell>> {
    cx.try_global::<ShellHandle>().and_then(|h| h.0.upgrade())
}

/// Hide or show the drawer (the drawer's own hide and close buttons).
/// The project bar's Browser button: the drawer's own open, which loads the
/// active tab's page and starts the browser host. (Showing the drawer's view
/// alone left the page blank and the host never started.) Without a drawer
/// part, the view is just shown.
pub fn browser_button(cx: &mut App) {
    if registry::has_action(cx, "drawer-open") { registry::run_action(cx, "drawer-open"); } else { set_drawer(cx, true); }
}

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

/// CRUNDI_DRAW_STATS=1: once a second, on stderr, the frames the window
/// painted, the slowest gap between two of them, how often the shell was
/// rendered, and the slowest frame (the shell's render to the last element
/// painted: every view's render, the layout, the paint). An idle window shows nothing;
/// a smooth scroll shows ~60 frames with gaps near 16 ms.
mod draw_stats {
    use std::cell::Cell;
    use std::time::{Duration, Instant};
    #[derive(Clone, Copy)]
    pub enum K { Render, Paint }
    pub use K::{Paint as PAINTS, Render as RENDERS};
    thread_local! {
        static ON: bool = std::env::var_os("CRUNDI_DRAW_STATS").is_some();
        static S: Cell<(Option<Instant>, u32, u32, Duration, Option<Instant>, Duration)> = const { Cell::new((None, 0, 0, Duration::ZERO, None, Duration::ZERO)) };
    }
    /// Count one; [Render] also returns a guard timing the render.
    pub fn count(k: &K) {
        if !ON.with(|o| *o) { return; }
        let now = Instant::now();
        S.with(|s| {
            let (mut start, mut paints, mut renders, mut gap, mut last, slow) = s.get();
            let start_at = *start.get_or_insert(now);
            match k {
                K::Paint => { paints += 1; if let Some(l) = last { gap = gap.max(now - l); } last = Some(now); }
                K::Render => renders += 1,
            }
            if now - start_at >= Duration::from_secs(1) {
                eprintln!("[draw] {paints} frames/s, slowest gap {} ms, {renders} shell renders, slowest frame {} ms (layout and paint of the whole window)", gap.as_millis(), slow.as_millis());
                start = Some(now); paints = 0; renders = 0; gap = Duration::ZERO;
                s.set((start, paints, renders, gap, last, Duration::ZERO));
            } else { s.set((start, paints, renders, gap, last, slow)); }
        });
    }
    thread_local! { static RENDER_AT: Cell<Option<Instant>> = const { Cell::new(None) }; }
    /// The shell's render starts a frame.
    pub fn frame_start() { if ON.with(|o| *o) { RENDER_AT.with(|r| r.set(Some(Instant::now()))); } }
    /// The last element painted: the frame's CPU time, render to paint.
    pub fn frame_done() {
        if !ON.with(|o| *o) { return; }
        let Some(t0) = RENDER_AT.with(|r| r.take()) else { return };
        let d = t0.elapsed();
        S.with(|s| { let mut v = s.get(); v.5 = v.5.max(d); s.set(v); });
    }
    pub fn on() -> bool { ON.with(|o| *o) }
}

/// A project-bar tab's body (the part's render) in a view of its own, so
/// GPUI can cache it (Shell::render).
pub struct TabHost { tab: String }

impl Render for TabHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        registry::render_tab(&self.tab, window, cx).unwrap_or_else(|| div().into_any_element())
    }
}
