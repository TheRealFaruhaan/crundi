//! The window's views (ARCHITECTURE.md, "The view tree"). The Shell is
//! layout only: its children (the top bar's leaves, Sidebar, ProjectBar,
//! each tab's body, the drawer) are cached views side by side, each
//! observing only what it shows. Layout and wording are the PC client's (the
//! web's #topbar, #sidebar, .tab-bar; native-gpui's shell where it matches).
//!
//! The top bar is a body of the Shell (no view of its own), its leaves side
//! by side: UsageRows (the bars, behind the whole bar), TopBar (the mark,
//! the project, a panel's chips), UsagePill ("updated … ago"), TopRight
//! (Telegram, a collaborator's Push / Ask to merge, Cru, approvals),
//! ConnBadge, WinButtons, and ReconnLine along its bottom edge. What ticks
//! (the bars and the badge each minute) or moves (the badge's blink, the
//! line's sweep) redraws only its own leaf (rules 3 and 6).

use crate::{conn, registry, signin::{RestoreLayer, SignIn}, slots::{self, measured, Slot}, Project, Stores, TABS};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc, time::Instant};

/// The browser drawer's view (Desktop's: `Shell::set_drawer_view`) and
/// which side it sits on.
#[derive(Clone)]
pub struct Drawer { pub view: AnyView, pub left: bool }

/// Where a window bar was pressed: the window moves only once the mouse
/// moves from there, so a click on a bar's button is a click (on X11 the
/// move takes the pointer until release, eating the click).
#[derive(Default)]
pub(crate) struct BarPress(pub Option<Point<Pixels>>);

impl BarPress {
    pub(crate) fn drag(&mut self, e: &MouseMoveEvent, window: &mut Window) {
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

/// The top bar's leaves.
struct Top { rows: Entity<UsageRows>, bar: Entity<TopBar>, pill: Entity<UsagePill>, right: Entity<TopRight>, cru: Entity<CruButton>, appr: Entity<Approvals>, conn: Entity<conn::ConnBadge>, win: Entity<WinButtons>, line: Entity<conn::ReconnLine> }

pub struct Shell {
    stores: Stores,
    top: Top,
    press: BarPress,
    side: Entity<Sidebar>,
    bar: Entity<ProjectBar>,
    sign_in: Entity<SignIn>,
    restore: Entity<RestoreLayer>,
    widths: Entity<slots::Widths>,
    /// Each tab's body, made the first time it is shown.
    tabs: HashMap<String, AnyView>,
    missing: HashMap<String, Entity<NotBuilt>>,
    drawer: Option<Drawer>,
    modal: Entity<w::ModalLayer>,
    ask: AnyView,
    dragging: bool,
    /// The areas' layers over the window (`register_overlay`), read once here.
    overlays: Vec<AnyView>,
    root_focus: FocusHandle,
    /// Cru's push to talk is held (its shortcut, ptt.rs).
    ptt_held: bool,
    /// What the Shell lays out, copied from Session, Nav and the top bar's
    /// widths when they change (it never reads them while rendering: a
    /// notify on either that changes none of this draws no frame;
    /// ARCHITECTURE.md, rule 1).
    lay: ShellLayout,
    _subs: Vec<Subscription>,
}

/// The Shell's layout inputs: signed in, the tab, the fold, the drawer, the
/// top bar's measured leaves (usage badge, right buttons, connection, window buttons).
#[derive(Clone, PartialEq)]
struct ShellLayout { signed_in: bool, tab: String, folded: bool, fold_from: Option<(Pixels, Instant)>, drawer_open: bool, drawer_w: Pixels, tw: [Pixels; 6] }

impl ShellLayout {
    fn of(stores: &Stores, widths: &Entity<slots::Widths>, cx: &App) -> Self {
        let n = stores.nav.read(cx);
        let wd = &widths.read(cx).0;
        let g = |k| wd.get(k).copied().unwrap_or(px(0.));
        ShellLayout { signed_in: stores.session.read(cx).signed_in(), tab: n.tab.clone(), folded: n.folded, fold_from: n.fold_from, drawer_open: n.drawer_open, drawer_w: n.drawer_w,
            tw: [g("pill"), g("right"), g("cru"), g("appr"), g("conn"), g("win")] }
    }
}

impl Shell {
    pub fn new(stores: Stores, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let c = conn::conn(cx);
        let top = Top {
            rows: cx.new(|cx| UsageRows::new(stores.clone(), cx)),
            bar: cx.new(|cx| TopBar::new(stores.clone(), cx)),
            pill: cx.new(|cx| UsagePill::new(stores.clone(), cx)),
            right: cx.new(|cx| TopRight::new(stores.clone(), cx)),
            cru: cx.new(|cx| CruButton::new(stores.clone(), cx)),
            appr: cx.new(|cx| Approvals::new(stores.clone(), cx)),
            conn: cx.new(|cx| conn::ConnBadge::new(c.clone(), cx)),
            win: cx.new(|_| WinButtons),
            line: cx.new(|cx| conn::ReconnLine::new(c.clone(), cx)),
        };
        let side = cx.new(|cx| Sidebar::new(stores.clone(), cx));
        let bar = cx.new(|cx| ProjectBar::new(stores.clone(), cx));
        let sign_in = cx.new(|cx| SignIn::new(stores.clone(), window, cx));
        let restore = cx.new(RestoreLayer::new);
        let widths = slots::widths(cx);
        // The Shell re-renders only when its layout changes: signed in or out
        // (the session's event, not its every notify), the tab, the fold, the
        // drawer, a top bar leaf's width.
        let relay = |this: &mut Self, cx: &mut Context<Self>| { let l = ShellLayout::of(&this.stores, &this.widths, cx); if l != this.lay { this.lay = l; cx.notify(); } };
        let mut subs = vec![
            cx.subscribe(&stores.session, move |this, _, _: &crate::SessionEvent, cx| relay(this, cx)),
            cx.observe(&stores.nav, move |this, _, cx| relay(this, cx)),
            cx.observe(&widths, move |this, _, cx| relay(this, cx)),
        ];
        let lay = ShellLayout::of(&stores, &widths, cx);
        let modal = w::modal_layer(cx);
        let ask = w::ask_layer(cx);
        // A focused element gone (a closed drawer's field, Cru's screen): the focus goes to its
        // nearest focusable ancestor still drawn, else to the window's root, never to nothing
        // (GPUI sends keys along the focused element's path only: with none, the window's own
        // shortcuts, Cru's push to talk, would hear no key). As native-gpui's Shell.
        subs.push(cx.on_focus_lost(window, |this: &mut Self, window, cx| {
            let to = window.focus_lost_restore_target(cx).unwrap_or_else(|| this.root_focus.clone());
            window.focus(&to, cx);
        }));
        cx.set_global(ShellHandle(cx.entity().downgrade()));
        Shell { stores, top, side, bar, sign_in, restore, widths, tabs: HashMap::new(), missing: HashMap::new(), drawer: None, modal, ask, dragging: false, press: BarPress::default(), overlays: cx.try_global::<crate::Overlays>().map(|o| o.0.clone()).unwrap_or_default(), root_focus: cx.focus_handle(), ptt_held: false, lay, _subs: subs }
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

    /// The top bar: its leaves side by side (a body, no view of its own:
    /// a cached view around cached leaves would re-render them all with any
    /// one of them, rule 3).
    fn top_bar(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let s = || StyleRefinement::default();
        let [wp, wr, wm, wa, wc, ww] = self.lay.tw;
        let t = &self.top;
        div().id("topbar").relative().flex().flex_none().w_full().h(T::TOPBAR_H)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, _| this.press.0 = (e.click_count == 1).then_some(e.position)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, _| this.press.drag(e, window)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.press.0 = None))
            .on_click(|e, window, _| if e.click_count() == 2 { window.zoom_window(); })
            .child(t.rows.clone().cached(s().absolute().inset_0()))
            .child(t.bar.clone().cached(s().flex_1().min_w_0().h_full()))
            .child(t.pill.clone().cached(s().flex_none().h_full().w(wp)))
            .child(t.right.clone().cached(s().flex_none().h_full().w(wr)))
            // Cru's mark a leaf of its own beside the rest (not inside one):
            // its breathing redraws it alone (F66).
            .child(t.cru.clone().cached(s().flex_none().h_full().w(wm)))
            .child(t.appr.clone().cached(s().flex_none().h_full().w(wa)))
            .child(t.conn.clone().cached(s().flex_none().h_full().w(wc)))
            .child(t.win.clone().cached(s().flex_none().h_full().w(ww)))
            .child(t.line.clone().cached(s().absolute().left_0().right_0().bottom_0().h(T::SP_2)))
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
        let l = self.lay.clone();
        if !l.signed_in {
            return root.child(self.sign_in.clone().cached(StyleRefinement::default().size_full()))
                // The restore from a backup (Operations'), over the sign-in page while open.
                .child(self.restore.clone().cached(StyleRefinement::default().absolute().top_0().left_0().size_full()))
                .child(self.modal.clone())
                .child(self.ask.clone())
                .into_any_element();
        }
        let (tab, drawer_open, drawer_w) = (l.tab, l.drawer_open, l.drawer_w);
        let (sw, easing) = crate::nav::sidebar_width(l.folded, l.fold_from);
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
        let dview = drawer.map(|d| div().flex_none().w(drawer_w).h_full().min_h_0().child(d.view)); // layout (Desktop's): its bars and page are its cached leaves
        let main = div().flex().flex_1().min_h_0().child(self.side.clone().cached(StyleRefinement::default().w(sw).h_full().flex_none()));
        let main = match dview {
            Some(v) if left => main.child(v).child(split).child(project),
            Some(v) => main.child(project).child(split).child(v),
            None => main.child(project),
        };
        let top = self.top_bar(cx);
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
            // Cru's push to talk (the web's default, Ctrl+Space; per device, ptt.rs): held, Cru
            // opens listening (or stops talking); let go, what was heard goes (Desktop's actions).
            // Settings may be waiting for the next key to become the shortcut.
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                if crate::ptt::capture(cx, &e.keystroke) { cx.stop_propagation(); return; }
                if crate::ptt::matches(&crate::ptt::get(), &e.keystroke) {
                    cx.stop_propagation();
                    if !this.ptt_held && !e.is_held { this.ptt_held = true; registry::run_action(&this.stores.clone(), "cru-ptt", window, cx); }
                }
            }))
            .capture_key_up(cx.listener(|this, e: &KeyUpEvent, window, cx| {
                if this.ptt_held && crate::ptt::releases(&crate::ptt::get(), &e.keystroke.key) { this.ptt_held = false; registry::run_action(&this.stores.clone(), "cru-ptt-up", window, cx); }
            }))
            .child(top)
            .child(main)
            .children(self.overlays.iter().map(|v| v.clone().cached(StyleRefinement::default().absolute().top_0().left_0().size_full())))
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

// ─── The top bar's leaves ───

/// A usage label at `at` (0-1) of its row: just after the fill's end, or
/// past 85% just inside it; on the row's top (usage) or bottom (time) edge.
fn usage_label(at: f32, top: bool) -> Div {
    let d = div().absolute().whitespace_nowrap().text_size(T::TEXT_9);
    let d = if at > 0.85 { d.right(relative(1.0 - at)).pr(T::SP_6) } else { d.left(relative(at)).pl(T::SP_6) };
    if top { d.top(px(1.)) } else { d.bottom(px(1.)) }
}

/// The bar's background and, for the owner, the weekly and 5-hour usage
/// bars behind the whole bar (the web's #usage-bg). Redrawn each minute
/// while there is usage (its countdowns), alone.
pub struct UsageRows { stores: Stores, _sub: Subscription }

impl UsageRows {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        let _sub = cx.observe(&stores.usage, |_, _, cx| cx.notify());
        UsageRows { stores, _sub }
    }
}

impl Render for UsageRows {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("usage-rows");
        crate::spots::begin(cx, "usage-rows");
        let now = crundi_core::time::now_secs();
        let r = self.stores.usage.read(cx).usage.as_ref().and_then(|(v, _)| crate::usage::rows(v, now));
        let mut rows = div().absolute().inset_0().flex().flex_col();
        if let Some(r) = r {
            for (i, row) in r.iter().enumerate() {
                // (Where the row and its fill's end are, for Mini.)
                let (sr, sf) = (crate::spots::spot(cx, "usage-rows", format!("usage:{}", row.tag)), crate::spots::spot(cx, "usage-rows", format!("usage:{}:fill", row.tag)));
                let (used, time) = (row.used / 100.0, row.time.unwrap_or(0.0) / 100.0);
                let over_c = { let t = crate::usage::over_strength(row.over); let c = T::yellow().blend(T::red().opacity(t)); c.opacity(0.18 + 0.4 * t) };
                rows = rows.child(div().relative().flex_1().overflow_hidden().when(i > 0, |d| d.border_t_1().border_color(T::border_subtle()))
                    .child(div().absolute().left(px(0.)).top(px(0.)).bottom(px(0.)).w(relative(time)).bg(T::usage_time()))
                    .child(div().absolute().left(px(0.)).top(px(0.)).bottom(px(0.)).w(relative(used)).bg(T::usage_used()).child(sf))
                    .child(sr)
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
        // One paint layer (w::layer): the bar is replayed every frame.
        w::layer(div().relative().size_full().bg(T::bg_secondary()).border_b_1().border_color(T::border()).child(rows))
    }
}

/// The mark and the project ("/ No project" when none is open), then a
/// panel's chips at the right (the "chips" top item, Operations').
pub struct TopBar { shown: (String, Option<Arc<Image>>), chips: Vec<Slot>, away: bool, _subs: Vec<Subscription> }

impl TopBar {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        let of = |p: &crate::Projects| (p.open_name(), p.icon_of(&p.open));
        // Only the open project's name and icon redraw it (not every state event).
        let mut subs = vec![cx.observe(&stores.projects, move |this: &mut Self, p, cx| {
            let n = of(p.read(cx));
            let same = n.0 == this.shown.0 && match (&n.1, &this.shown.1) { (Some(a), Some(b)) => Arc::ptr_eq(a, b), (None, None) => true, _ => false };
            if !same { this.shown = n; cx.notify(); }
        })];
        // Mini, the logo's head, out walking (the chat area's): its place is empty.
        let logo = crate::spots::logo_away(cx);
        subs.push(cx.observe(&logo, |this: &mut Self, l, cx| { let a = l.read(cx).0; if a != this.away { this.away = a; cx.notify(); } }));
        let chips: Vec<Slot> = slots::top_items(cx).into_iter().filter(|s| s.name == "chips").collect();
        subs.extend(slots::observe_slots(&chips, cx));
        let shown = of(stores.projects.read(cx));
        TopBar { shown, chips, away: false, _subs: subs }
    }
}

impl Render for TopBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("top-bar");
        crate::spots::begin(cx, "top-bar");
        let (name, picon) = self.shown.clone();
        let logo = crate::spots::spot(cx, "top-bar", "logo");
        let chips: Vec<AnyElement> = self.chips.iter().filter_map(|s| (s.draw)(window, cx)).collect();
        w::layer(div().flex().size_full().min_w_0().items_center().gap(T::SP_12).pl(T::SP_16).overflow_hidden()
            .child(div().relative().flex_none().child(w::brand_mark_with(T::BRAND_HEAD, !self.away)).child(logo))
            .child(div().text_color(T::text_muted()).child("/"))
            .children(picon.map(w::project_icon))
            .child(div().id("current-project").min_w_0().truncate().text_color(T::text_secondary()).child(if name.is_empty() { "No project".to_string() } else { name }))
            .child(div().flex_1())
            .children(chips))
    }
}

/// When the usage was fetched, in the local time (the web's toLocaleString).
fn local_time(secs_ago: f64) -> String {
    let at = chrono::Local::now() - chrono::Duration::milliseconds((secs_ago * 1000.0) as i64);
    at.format("%-m/%-d/%Y, %-I:%M:%S %p").to_string()
}

/// The usage badge (#usage-updated): "updated … ago"; a click opens Usage.
/// A leaf of its own: its text changes each minute.
pub struct UsagePill { stores: Stores, _sub: Subscription }

impl UsagePill {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        let _sub = cx.observe(&stores.usage, |_, _, cx| cx.notify());
        UsagePill { stores, _sub }
    }
}

impl Render for UsagePill {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("usage-pill");
        let now = crundi_core::time::now_secs();
        let u = self.stores.usage.read(cx);
        let age = u.usage.as_ref().filter(|(v, _)| crate::usage::rows(v, now).is_some()).map(|(v, at)| {
            v.get("fetchedAt").and_then(|f| f.as_str()).and_then(crundi_core::time::parse_iso).map(|f| now - f).unwrap_or(at.elapsed().as_secs_f64())
        });
        let stores = self.stores.clone();
        let pill = age.map(|age| div().pl(T::SP_12).child(w::tip(
            w::status_pill("usage-updated", Some(IconName::ChartBar), crate::usage::ago(age), w::PillTone::Neutral)
                .on_click(move |_, window, cx| { registry::run_action(&stores, "open-usage", window, cx); }),
            format!("Claude usage updated {} · click for history chart", local_time(age)))));
        w::layer(measured("pill", div().children(pill)))
    }
}

/// Telegram's badge and a collaborator's Push and Ask to merge. (Cru's mark
/// and the approvals button sit beside it, each a leaf of its own.)
pub struct TopRight {
    stores: Stores,
    /// The Telegram bot: configured, connected, its @name (copied from Usage when it changed).
    tg: Option<(bool, bool, String)>,
    collab: bool,
    pushing: bool,
    merging: bool,
    _subs: Vec<Subscription>,
}

impl TopRight {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        let subs = vec![
            // Usage ticks each minute: only Telegram's status redraws this.
            cx.observe(&stores.usage, |this: &mut Self, u, cx| { let t = u.read(cx).tg.clone(); if t != this.tg { this.tg = t; cx.notify(); } }),
            cx.observe(&stores.projects, |this: &mut Self, p, cx| { let c = p.read(cx).collaborator(); if c != this.collab { this.collab = c; cx.notify(); } }),
        ];
        TopRight { stores, tg: None, collab: false, pushing: false, merging: false, _subs: subs }
    }

    /// Push your branch (collabPush).
    fn push(&mut self, cx: &mut Context<Self>) {
        let project = self.stores.projects.read(cx).open.clone();
        self.pushing = true;
        cx.notify();
        let me = cx.entity().downgrade();
        crate::call(cx, move |api| api.post("/api/collab/push", &json!({ "project": project })), move |r, cx| {
            let _ = me.update(cx, |t, cx| { t.pushing = false; cx.notify(); });
            match r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => crate::toast(cx, format!("Pushed {}", v.get("branch").and_then(|b| b.as_str()).unwrap_or("")), w::Toast::Success),
                Ok(v) => crate::toast(cx, v.get("error").and_then(|e| e.as_str()).unwrap_or("Push failed").to_string(), w::Toast::Error),
                Err(e) => crate::toast(cx, if e.is_empty() { "Push failed".to_string() } else { e }, w::Toast::Error),
            }
        });
    }

    /// Ask the owner to merge your branch (collabAskMerge).
    fn ask_merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        let stores = self.stores.clone();
        w::ask(window, cx, w::Ask { title: "Anything the owner should know before merging?".into(), prompt: Some(String::new()), ..Default::default() }, move |msg, _, cx| {
            let Some(msg) = msg else { return };   // cancelled, as opposed to deliberately blank
            let project = stores.projects.read(cx).open.clone();
            let _ = me.update(cx, |t, cx| { t.merging = true; cx.notify(); });
            let me = me.clone();
            crate::call(cx, move |api| api.post("/api/collab/request", &json!({ "kind": "merge", "project": project, "message": msg })), move |r, cx| {
                let _ = me.update(cx, |t, cx| { t.merging = false; cx.notify(); });
                match r {
                    Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => crate::toast(cx, "Sent to the owner", w::Toast::Success),
                    Ok(v) => crate::toast(cx, v.get("error").and_then(|e| e.as_str()).unwrap_or("Could not send that").to_string(), w::Toast::Error),
                    Err(_) => crate::toast(cx, "Could not send that", w::Toast::Error),
                }
            });
        });
    }
}

/// Telegram's badge (updateTgBadge): shown where a bot is set up.
pub fn tg_badge(tg: &Option<(bool, bool, String)>) -> Option<(String, bool)> {
    let (configured, connected, n) = tg.clone()?;
    if !configured && !connected { return None; }
    Some((if connected { format!("tg: @{}", if n.is_empty() { "bot" } else { &n }) } else { "tg: offline".to_string() }, connected))
}

impl Render for TopRight {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("top-right");
        let tg = tg_badge(&self.tg).map(|(label, on)| w::tip(w::status_pill("tg-badge", None, label, if on { w::PillTone::Connected } else { w::PillTone::Disconnected }), "Telegram bot"));
        let collab = self.collab;
        let push = collab.then(|| w::tip(w::topbar_act("collab-push", IconName::Upload, "Push", self.pushing)
            .when(!self.pushing, |d| d.on_click(cx.listener(|this, _, _, cx| this.push(cx)))), "Push your branch so the owner can see it"));
        let merge = collab.then(|| w::tip(w::topbar_act("collab-merge", IconName::GitMerge, "Ask to merge", self.merging)
            .when(!self.merging, |d| d.on_click(cx.listener(|this, _, window, cx| this.ask_merge(window, cx)))), "Ask the owner to merge your branch"));
        let any = tg.is_some() || push.is_some();
        // One paint layer (w::layer): replayed every frame.
        w::layer(measured("right", div().flex().items_center().gap(T::SP_12).when(any, |d| d.pl(T::SP_12))
            .children(tg).children(push).children(merge)))
    }
}

/// Cru's mark (crundi-voice.js attach()), a leaf of the top bar: the bars
/// breathe while Cru is live (24 fps, redrawing this alone); a red dot for
/// unread reports. A click toggles Cru; resting on it opens Cru (on a wide
/// window, after a moment), and it closes again when the pointer leaves
/// unless Cru is busy (Desktop's actions).
pub struct CruButton { stores: Stores, mark: Entity<crate::CruMark>, cru_hover: u64, _sub: Subscription }

impl CruButton {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        let mark = crate::cru_mark(cx);
        let _sub = cx.observe(&mark, |_, _, cx| cx.notify());
        CruButton { stores, mark, cru_hover: 0, _sub }
    }
}

impl Render for CruButton {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("cru-mark");
        let _ = window;
        let stores = self.stores.clone();
        let m = self.mark.read(cx).clone();
        let tip: SharedString = if m.unread > 0 { format!("Talk to Cru ({} unread)", m.unread).into() } else { "Talk to Cru (voice assistant)".into() };
        let cru = div().id("cru").relative().flex_none().flex().items_center().p(T::SP_2).cursor_pointer()
            .child(w::cru_mark(m.live))
            // Where the mark is: Cru's screen opens under it (cru_anchor).
            .child(canvas(|b, _, cx| crate::set_cru_anchor(cx, b), |_, _, _, _| {}).absolute().inset_0())
            .when(m.unread > 0, |d| d.child(w::layer(w::dot(T::red()).absolute().top(px(1.)).right(px(1.)).border_2().border_color(T::bg_secondary()))))
            .tooltip(w::tooltip(tip))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| { this.cru_hover += 1; registry::run_action(&stores, "cru", window, cx); }))
            .on_hover(cx.listener(|this, on: &bool, window, cx| {
                this.cru_hover += 1;
                let stores = this.stores.clone();
                if !*on { registry::run_action(&stores, "cru-leave", window, cx); return; }
                if window.viewport_size().width < T::CRU_HOVER_MIN_W { return; }
                let n = this.cru_hover;
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(350)).await;
                    let _ = this.update_in(cx, |this, window, cx| if this.cru_hover == n { registry::run_action(&stores, "cru-hover", window, cx); });
                }).detach();
            }));
        measured("cru", div().pl(T::SP_12).child(cru))
    }
}

/// The approvals button (the "approvals" top item, Operations'): the owner's.
pub struct Approvals { stores: Stores, appr: Vec<Slot>, collab: bool, _subs: Vec<Subscription> }

impl Approvals {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        let appr: Vec<Slot> = slots::top_items(cx).into_iter().filter(|s| s.name == "approvals").collect();
        let mut subs = slots::observe_slots(&appr, cx);
        subs.push(cx.observe(&stores.projects, |this: &mut Self, p, cx| { let c = p.read(cx).collaborator(); if c != this.collab { this.collab = c; cx.notify(); } }));
        Approvals { stores, appr, collab: false, _subs: subs }
    }
}

impl Render for Approvals {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = &self.stores;
        crate::spots::begin(cx, "approvals");
        let appr: Vec<AnyElement> = if self.collab { vec![] } else { self.appr.iter().filter_map(|s| (s.draw)(window, cx)).collect() };
        // (Where the bell is, for Mini.)
        let appr: Vec<AnyElement> = appr.into_iter().map(|el| div().relative().flex_none().child(el).child(crate::spots::spot(cx, "approvals", "approvals")).into_any_element()).collect();
        let any = !appr.is_empty();
        w::layer(measured("appr", div().flex().items_center().gap(T::SP_12).when(any, |d| d.pl(T::SP_12)).children(appr)))
    }
}

/// Minimise, maximise, close (the web's #win-controls).
pub struct WinButtons;
impl Render for WinButtons {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        w::layer(measured("win", div().flex().items_center().gap(T::SP_12).pl(T::SP_12).pr(T::SP_4)
            .child(w::icon_button("win-min", IconName::Minus, "Minimize").on_click(|_, window, _| window.minimize_window()))
            .child(w::icon_button("win-max", IconName::Square, "Maximize").on_click(|_, window, _| window.zoom_window()))
            .child(w::icon_button("win-close", IconName::X, "Close").on_click(|_, window, cx| crate::close_window(window, cx)))))
    }
}

// ─── The sidebar ───

/// A project being dragged to a new place (the web's projectDragHandlers).
struct Drag { alias: String, from: Pixels, active: bool, target: Option<usize> }

/// The projects sidebar: the header (PROJECTS and the fold button) the top
/// bar's height; rows 32 high, 2 apart, each with its marks (scheduled
/// tasks, a factory, the services heartbeat) and, in multi mode, its remove
/// button on hover; dragged, a row moves (a line shows where); folded, a
/// rail of avatars with their marks under them; + Add project and Inbox at the foot.
pub struct Sidebar {
    stores: Stores,
    folded: Option<bool>,
    inbox: Entity<crate::InboxBadge>,
    /// What it draws of the projects, copied when it changed (not every state event).
    seen: (Vec<Project>, String, String, usize),
    /// Where each row was drawn (the drag's targets).
    rows: Rc<RefCell<Vec<(String, Bounds<Pixels>)>>>,
    drag: Option<Drag>,
    drag_end: Option<Instant>,
    _subs: Vec<Subscription>,
}

fn seen_of(p: &crate::Projects) -> (Vec<Project>, String, String, usize) { (p.list.clone(), p.open.clone(), p.mode.clone(), p.icons_ready()) }

impl Sidebar {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        let inbox = crate::inbox_badge(cx);
        let subs = vec![
            cx.observe(&stores.projects, |this: &mut Self, p, cx| { let s = seen_of(p.read(cx)); if s != this.seen { this.seen = s; cx.notify(); } }),
            // Only the fold changes the sidebar's own drawing (not the tab).
            cx.observe(&stores.nav, |this: &mut Self, nav, cx| { let f = nav.read(cx).folded; if this.folded != Some(f) { this.folded = Some(f); cx.notify(); } }),
            // The Inbox button's count (Desktop's).
            cx.observe(&inbox, |_, _, cx| cx.notify()),
        ];
        let seen = seen_of(stores.projects.read(cx));
        let folded = Some(stores.nav.read(cx).folded);
        Sidebar { stores, folded, inbox, seen, rows: Rc::default(), drag: None, drag_end: None, _subs: subs }
    }

    /// Where the dragged project would land among the others (projectReorderTarget).
    fn target_at(&self, y: Pixels, alias: &str) -> usize {
        let rows = self.rows.borrow();
        let others: Vec<&Bounds<Pixels>> = rows.iter().filter(|(a, _)| a != alias).map(|(_, b)| b).collect();
        others.iter().position(|b| y < b.origin.y + b.size.height / 2.).unwrap_or(others.len())
    }

    fn drag_move(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(d) = self.drag.as_ref() else { return };
        if e.pressed_button != Some(MouseButton::Left) { self.drag = None; cx.notify(); return; }
        let active = d.active || f32::from((e.position.y - d.from).abs()) >= 4.;
        let target = if active { Some(self.target_at(e.position.y, &d.alias)) } else { None };
        let d = self.drag.as_mut().unwrap();
        if d.active != active || d.target != target { d.active = active; d.target = target; cx.notify(); }
    }

    /// The drag ends: the new order, at once and on the server.
    fn drag_end(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.drag.take() else { return };
        if !d.active { return; }
        self.drag_end = Some(Instant::now());
        cx.notify();
        let Some(t) = d.target else { return };
        let mut order: Vec<String> = self.seen.0.iter().map(|p| p.alias.clone()).filter(|a| *a != d.alias).collect();
        order.insert(t.min(order.len()), d.alias.clone());
        self.stores.projects.update(cx, |p, cx| p.reorder(&order, cx));
        crate::call(cx, move |api| api.post("/api/projects/reorder", &json!({ "order": order })), |_, _| {});
    }
}

/// "a/b" → "a%2Fb": a project's alias in a path.
fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// Remove a project (removeProject): asked first; the files stay.
pub fn remove_project(stores: &Stores, alias: String, name: String, window: &mut Window, cx: &mut App) {
    let s = stores.clone();
    w::ask_confirm(window, cx, w::Ask {
        title: format!("Remove \"{name}\" from Crundi?").into(),
        message: "This only removes the project reference and its services — your files on disk are kept.".into(),
        danger: true, ..Default::default()
    }, move |ok, _, cx| {
        if !ok { return; }
        let (s, alias, name) = (s.clone(), alias.clone(), name.clone());
        let path = format!("/api/projects/{}", enc(&alias));
        crate::call(cx, move |api| api.delete(&path), move |r, cx| match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) != Some(false) => {
                s.projects.update(cx, |p, cx| p.removed(&alias, cx));
                crate::load_projects(&s, cx);
                let n = v.get("servicesRemoved").and_then(|n| n.as_u64()).unwrap_or(0);
                crate::toast(cx, format!("Removed \"{name}\"{}", if n > 0 { format!(" and {n} service(s)") } else { String::new() }), w::Toast::Success);
            }
            Ok(v) => crate::toast(cx, format!("Failed to remove project: {}", v.get("error").and_then(|e| e.as_str()).unwrap_or("Unknown error")), w::Toast::Error),
            Err(e) => crate::toast(cx, format!("Failed to remove project: {e}"), w::Toast::Error),
        });
    });
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("sidebar");
        crate::spots::begin(cx, "sidebar");
        let folded = self.folded.unwrap_or(false);
        let (list_, open, mode, _) = self.seen.clone();
        let can_remove = mode != "single";
        let dragging = self.drag.as_ref().filter(|d| d.active).map(|d| (d.alias.clone(), d.target));
        self.rows.borrow_mut().clear();
        let mut list = div().id("projects").flex().flex_col().flex_1().pt(T::SP_4).pb(T::SP_4).px(if folded { T::SP_6 } else { T::SP_8 }).gap(T::SP_2).overflow_y_scroll();
        let mut k = 0usize; // the row's place among those not dragged
        for (i, pr) in list_.iter().enumerate() {
            let on = pr.alias == open;
            let icon = self.stores.projects.read(cx).icon_of(&pr.alias);
            let is_dragged = dragging.as_ref().is_some_and(|(a, _)| *a == pr.alias);
            if !is_dragged {
                if dragging.as_ref().is_some_and(|(_, t)| *t == Some(k)) { list = list.child(w::drop_line()); }
                k += 1;
            }
            // Its marks (renderProjects): scheduled tasks, a factory, the services' heartbeat.
            let sched = pr.sched.then(|| w::tip(div().id(("sched", i)).child(w::sched_mark(on && !folded)), "Has scheduled tasks"));
            let fx = pr.factory.as_ref().map(|f| w::tip(div().id(("fx", i)).child(w::factory_mark(f.1)), pr.factory_tip()));
            let heart = pr.heart.map(|(r, _)| w::tip(div().id(("heart", i)).flex().items_center().child(w::ecg_mark(r > 0)), pr.heart_tip()));
            let row = if folded {
                div().flex().flex_col().items_center().justify_center().gap(T::SP_4 + px(1.)).py(T::SP_8)
                    .child(w::rail_avatar(crundi_core::head::project_initials(&pr.name), pr.dot, on, icon).id(("avatar", i)))
                    .children(sched).children(fx).children(heart)
                    .into_any_element()
            } else {
                let group: SharedString = format!("prow{i}").into();
                let (alias, name, stores) = (pr.alias.clone(), pr.name.clone(), self.stores.clone());
                let rm = can_remove.then(|| w::tip(w::remove_mark(("remove", i)).opacity(0.).group_hover(group.clone(), |s| s.opacity(1.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, window, cx| { cx.stop_propagation(); remove_project(&stores, alias.clone(), name.clone(), window, cx); }),
                    "Remove project (keeps files)"));
                w::sidebar_item(pr.name.clone(), pr.dot, on, icon).group(group)
                    .children(sched).children(fx).children(heart).children(rm)
                    .into_any_element()
            };
            let alias = pr.alias.clone();
            let projects = self.stores.projects.clone();
            let rows = self.rows.clone();
            let a2 = pr.alias.clone();
            let spot = crate::spots::spot(cx, "sidebar", format!("sidebar:{}:{}", if pr.dot == "needs-input" { "input" } else { "-" }, pr.alias));
            list = list.child(div().id(("project", i)).relative().cursor_pointer().when(is_dragged, |d| d.opacity(0.4)).child(row).child(spot)
                // Where it was drawn: the drag's targets.
                .child(canvas(move |b, _, _| rows.borrow_mut().push((a2.clone(), b)), |_, _, _, _| {}).absolute().inset_0())
                .tooltip(w::tooltip(pr.name.clone()))
                .on_mouse_down(MouseButton::Left, cx.listener({ let alias = pr.alias.clone(); move |this, e: &MouseDownEvent, _, _| {
                    this.drag = Some(Drag { alias: alias.clone(), from: e.position.y, active: false, target: None });
                } }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    // The click that trails a drag is not a pick.
                    if this.drag_end.is_some_and(|t| t.elapsed().as_millis() < 300) { return; }
                    projects.update(cx, |p, cx| p.pick(&alias, cx));
                })));
        }
        if dragging.as_ref().is_some_and(|(_, t)| *t == Some(k)) { list = list.child(w::drop_line()); }
        let nav = self.stores.nav.clone();
        let fold = w::icon_button("fold", if folded { IconName::ChevronsRight } else { IconName::ChevronsLeft }, if folded { "Expand panel" } else { "Collapse panel" })
            .on_click(move |_, _, cx| nav.update(cx, |n, cx| n.toggle_fold(cx)));
        let head = div().flex().flex_none().items_center().h(T::TOPBAR_H).border_b_1().border_color(T::border_subtle());
        let head = if folded { head.justify_center().child(fold) } else {
            head.pl(T::SP_16).pr(T::SP_12)
                .child(div().flex_1().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child("PROJECTS"))
                .child(fold)
        };
        let (s1, s2) = (self.stores.clone(), self.stores.clone());
        let ib = self.inbox.read(cx).clone();
        // One paint layer (w::layer): the sidebar is replayed every frame.
        w::layer(div().id("sidebar")
            .flex().flex_col().size_full().overflow_hidden().bg(T::bg_secondary()).border_r_1().border_color(T::border())
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| this.drag_move(e, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.drag_end(cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| this.drag_end(cx)))
            .child(head)
            .child(list)
            .child(div().flex().flex_col().gap(T::SP_8).py(T::SP_10).px(if folded { T::SP_6 } else { T::SP_8 }).border_t_1().border_color(T::border_subtle())
                .child(w::dashed_button("add-project", if folded { "+" } else { "+ Add project" }).tooltip(w::tooltip("Add project")).on_click(move |_, window, cx| { registry::run_action(&s1, "add-project", window, cx); }))
                .when(ib.supported, |d| d.child(w::outline_button("inbox", IconName::Mail, if folded { "" } else { "Inbox" })
                    // The web's .has-unread: brighter, an accent edge, the count's badge; its title.
                    .when(ib.unread > 0, |b| b.border_color(T::accent()).text_color(T::text()).child(w::count_badge(if ib.unread > 99 { "99+".to_string() } else { ib.unread.to_string() })))
                    .tooltip(w::tooltip(if ib.unread > 0 { format!("Inbox: {} unread", ib.unread) } else { "Inbox".to_string() }))
                    .on_click(move |_, _, cx| s2.nav.update(cx, |n, cx| n.show_tab("inbox", cx)))))))
    }
}

// ─── The project bar ───

/// The tabs a collaborator has no route to (the web's COLLAB_HIDDEN_TABS;
/// its "widgets" is the Panels tab).
pub const COLLAB_HIDDEN_TABS: &[&str] = &["git", "secrets", "schedule", "docker", "homepool", "info", "settings", "panels", "factories"];

/// A tab's tooltip (its title in the web's .tab-bar).
pub fn tab_title(id: &str, label: &'static str) -> &'static str {
    match id {
        "panels" => "Panels built by Claude",
        "factories" => "Factories: teams of Claude employees",
        "homepool" => "Homepool: where traffic leaves from",
        _ => label,
    }
}

/// The tabs (scrolling sideways when they don't fit, cut at the edge as the
/// web's .tab-bar), each with its tooltip and its badge (the areas'), the
/// workbench's "+" after the first, and the Browser button at the right
/// while the drawer is closed.
pub struct ProjectBar { stores: Stores, scroll: ScrollHandle, scope: (bool, bool), badges: Vec<Slot>, _subs: Vec<Subscription> }

impl ProjectBar {
    fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        // (The nav store also notifies when a project opens or none is: sync_scope.)
        let mut subs = vec![
            cx.observe(&stores.nav, |_, _, cx| cx.notify()),
            // A collaborator sees fewer tabs.
            cx.observe(&stores.projects, |this: &mut Self, p, cx| { let p = p.read(cx); let s = (!p.open.is_empty(), p.collaborator()); if s != this.scope { this.scope = s; cx.notify(); } }),
        ];
        let badges = slots::tab_badges(cx);
        subs.extend(slots::observe_slots(&badges, cx));
        let p = stores.projects.read(cx);
        let scope = (!p.open.is_empty(), p.collaborator());
        let scroll = ScrollHandle::new();
        // The tabs' scroll, for Mini (it hauls them along to Settings).
        let bar: AnyWeakEntity = cx.entity().downgrade().into();
        crate::spots::set_tabs(cx, crate::spots::Tabs { scroll: scroll.clone(), bar });
        ProjectBar { stores, scroll, scope, badges, _subs: subs }
    }
}

impl Render for ProjectBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("project-bar");
        crate::spots::begin(cx, "project-bar");
        let nav = self.stores.nav.read(cx);
        let (on_tab, drawer_open, docker) = (nav.tab.clone(), nav.drawer_open, nav.docker);
        let (project_open, collab) = self.scope;
        let mut tabs = div().id("tabbar").flex().flex_1().min_w_0().h_full().items_center().overflow_x_scroll().track_scroll(&self.scroll);
        for (i, (id, label, icon, _)) in TABS.iter().enumerate() {
            // With no project, only the global tabs (and no "+"); Docker only where installed.
            if !crate::nav::tab_shown(id, project_open, docker) { continue; }
            if collab && COLLAB_HIDDEN_TABS.contains(id) { continue; }
            let badge: Vec<AnyElement> = self.badges.iter().filter(|s| s.name == *id).filter_map(|s| (s.draw)(window, cx)).collect();
            // (Where each tab and its badge are, for Mini.)
            let badge: Vec<AnyElement> = badge.into_iter().map(|b| div().relative().child(b).child(crate::spots::spot(cx, "project-bar", format!("tab-badge:{id}"))).into_any_element()).collect();
            let tspot = crate::spots::spot(cx, "project-bar", format!("tab:{id}"));
            let (tid, nav) = (id.to_string(), self.stores.nav.clone());
            tabs = tabs.child(w::nav_tab(icon.clone(), *label, on_tab == tid).relative().children(badge).child(tspot).id(("tab", i)).flex_none().cursor_pointer()
                .tooltip(w::tooltip(tab_title(id, label)))
                .on_click(move |_, _, cx| nav.update(cx, |n, cx| n.show_tab(&tid, cx))));
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
        let scroller = div().relative().flex().flex_1().min_w_0().h_full().child(tabs).child(crate::spots::spot(cx, "project-bar", "tabbar"));
        let mut bar = div().flex().size_full().items_center().px(T::SP_12).bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .child(scroller);
        if !drawer_open {
            let s = self.stores.clone();
            bar = bar.child(w::nav_tab(IconName::Globe, "Browser", false).id("browser").flex_none().cursor_pointer()
                .tooltip(w::tooltip("Browser (desktop app)"))
                .on_click(move |_, window, cx| { if !registry::run_action(&s, "browser", window, cx) { s.nav.update(cx, |n, cx| n.set_drawer(true, cx)); } }));
        }
        // One paint layer (w::layer): the bar is replayed every frame.
        w::layer(bar)
    }
}

/// The driver's look at the shell's own parts (merged into "state").
pub fn driver_state(stores: &Stores, cx: &App) -> Value {
    let c = conn::conn_read(cx);
    let p = stores.projects.read(cx);
    let s = stores.session.read(cx);
    let u = stores.usage.read(cx);
    json!({
        "conn": c.map(|c| { let (label, _, tip) = c.badge_look(); json!({ "badge": label, "tip": tip, "line": format!("{:?}", c.line) }) }),
        "tgBadge": tg_badge(&u.tg).map(|t| t.0),
        "projectName": if p.open.is_empty() { "No project".to_string() } else { p.open_name() },
        "role": p.role, "mode": p.mode,
        "rows": p.list.iter().map(|x| json!({ "alias": x.alias, "heart": x.heart.map(|h| json!([h.0, h.1])), "heartTip": x.heart_tip(), "sched": x.sched, "factory": x.factory_tip() })).collect::<Vec<_>>(),
        "signin": { "setupRequired": s.setup_required, "enrol": s.enrol.is_some(), "enrolSecret": s.enrol.as_ref().map(|e| e.0.clone()), "collab": s.collab, "bootSlow": s.boot_slow,
            "prompt": crate::signin::prompt(s.setup_required, s.offer_telegram, s.offer_google, s.offer_github, s.offer_password) },
    })
}
