//! The drawer's view (app/vendor/crundi-browser-drawer.js, the Slint
//! client's ui/browser.slint): the tabs row (a tab per page of this
//! project, its network badge, ×; New tab, the "…" menu, Hide, Close all),
//! the address bar (back, forward, reload, the address with the history's
//! suggestions, Network, Device, Annotate, DevTools), the password bar, the
//! annotation bar while annotating, then the page (or a panel: History,
//! Clear browsing data, Passwords), with a mark's note field over it.
//!
//! **How it is built (ARCHITECTURE.md rules 1 to 3).** `BrowserDrawer` is a
//! layout view (the shell places it uncached): no text, only boxes and its
//! leaves, each a cached view of its own: the tabs row, the address bar
//! (with the page's load error under it), the password bar, the annotation
//! bar, the body (a panel, or the empty state), the page (`PageView`), the
//! note field and the address's suggestions. Every leaf renders from its own
//! copy (`#[derive(Clone, PartialEq)]`), which `BrowserDrawer::pump` takes
//! from the logic (drawer.rs, passwords.rs, the engine) only when it said
//! something changed, and a leaf notifies itself only when its copy moved.
//! So a page's new frame re-renders the page alone (and this layout view,
//! its ancestor, which is a few boxes); the bars are replayed from their
//! caches. The bars paint in one layer each (`w::layer`), a text box only
//! while it is not focused (`w::text_box`, `w::watch_focus`).
//!
//! Every control is the widgets crate's.

use crate::{drawer, passwords, rules};
use crundi_core::driver::rendered;
use crundi_theme::{IconName, T};
use crundi_webview::{engine as browser, PageView};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::button::{Button, ButtonVariants as _};

fn sr() -> StyleRefinement { StyleRefinement::default() }

/// A leaf whose height is its content's: it measures itself after layout
/// and the drawer places it at that height (a cached view is laid out from
/// its style alone).
trait Measured: 'static + Sized {
    fn height(&self) -> Pixels;
    fn set_height(&mut self, h: Pixels);
}

fn measure<V: Measured>(me: Entity<V>) -> impl IntoElement {
    canvas(move |b, _, cx| {
        let h = b.size.height;
        // (After the frame: a notify while drawing would not draw again.)
        if (me.read(cx).height() - h).abs() > px(0.5) { let me = me.clone(); cx.defer(move |cx| me.update(cx, |v, cx| { v.set_height(h); cx.notify(); })); }
    }, |_, _, _, _| {}).absolute().size_full()
}

/// Set a leaf's copy; a notify only when it changed.
fn set_copy<C: PartialEq, V: 'static>(slot: &mut C, new: C, cx: &mut Context<V>) {
    if *slot != new { *slot = new; cx.notify(); }
}

// ─── The drawer (layout) ───

/// What the layout shows (which leaves, where).
#[derive(Clone, PartialEq, Default)]
struct Lay { has_tab: bool, panel: bool, pw: bool, ann: bool, note_at: Option<(f32, f32)>, suggest: bool, left: bool }

pub struct BrowserDrawer {
    page: Entity<PageView>,
    tabs: Entity<TabsBar>,
    addr: Entity<AddressBar>,
    pw: Entity<PwBar>,
    ann: Entity<AnnBar>,
    body: Entity<DrawerBody>,
    note: Entity<NoteBox>,
    suggest: Entity<SuggestList>,
    addr_focus: FocusHandle,
    lay: Lay,
    /// The shell (to move the drawer to the other side).
    shell: Option<WeakEntity<crundi_shell::Shell>>,
}

impl BrowserDrawer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let page = cx.new(PageView::new);
        let page_focus = page.read(cx).focus_handle().clone();
        let addr = cx.new(|cx| AddressBar::new(page_focus, window, cx));
        let addr_focus = addr.read(cx).address.read(cx).focus_handle(cx);
        BrowserDrawer {
            page,
            tabs: cx.new(|_| TabsBar { c: TabsCopy::default() }),
            addr,
            pw: cx.new(|cx| PwBar::new(window, cx)),
            ann: cx.new(|_| AnnBar { c: AnnCopy::default(), height: T::DRAWER_BARS_H - T::TABBAR_H }),
            body: cx.new(|cx| DrawerBody::new(window, cx)),
            note: cx.new(|cx| NoteBox::new(window, cx)),
            suggest: cx.new(|_| SuggestList { rows: vec![], height: T::CONTROL }),
            addr_focus,
            lay: Lay { left: drawer::with(|d| d.side == "left"), ..Lay::default() },
            shell: None,
        }
    }

    /// The shell, to move the drawer when its side changes (lib.rs, once).
    pub fn set_shell(&mut self, shell: WeakEntity<crundi_shell::Shell>) { self.shell = Some(shell); }

    /// The address field takes the keys (the test driver: a drawer holding the focus as it closes).
    pub fn focus_address(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.addr.update(cx, |a, cx| a.address.update(cx, |s, cx| s.focus(window, cx)));
        self.addr_focus.is_focused(window)
    }

    /// Each pump: the page shown and its frame (the page's own notify), the
    /// sleep rule's turn; when the logic changed (`dirty`) or a page's state
    /// did, every leaf's copy (each notifies only if its copy moved).
    pub fn pump(&mut self, dirty: bool, window: &mut Window, cx: &mut Context<Self>) {
        drawer::rest_check(dirty);
        let (open, annotating) = drawer::with(|d| (d.open, d.annotating));
        let act = drawer::active_tab();
        let panel = act.as_ref().and_then(|t| t.panel).filter(|_| open);
        self.page.update(cx, |p, cx| {
            p.set_annotating(annotating, cx);
            p.set_panel(panel, cx);
            p.pump(window, cx);
        });
        let changed = !browser::take_changed().is_empty();
        if !(dirty || changed) { return; }

        let has = act.is_some();
        // The tabs row.
        let tabs = TabsCopy { tabs: drawer::tab_views(), left: drawer::with(|d| d.side == "left") };
        self.tabs.update(cx, |v, cx| set_copy(&mut v.c, tabs, cx));
        // The address bar; the address follows the page while not typed in.
        let (back, fwd, loading) = act.as_ref().and_then(|t| t.panel).map(|i| browser::with(|st| st.panels.get(i).map(|p| (p.can_back, p.can_fwd, p.loading)).unwrap_or_default())).unwrap_or_default();
        let (net_tip, tone, dev_tip, dev_on) = drawer::bar_state();
        let a = AddrCopy {
            has, back, fwd, loading, net_tip, tone, dev_tip, dev_on, annotating,
            logins: act.as_ref().map(|t| passwords::count_for(&t.url)).unwrap_or(0),
        };
        let url = act.as_ref().map(|t| if t.url == "about:blank" { String::new() } else { t.url.clone() }).unwrap_or_default();
        self.addr.update(cx, |v, cx| { v.follow(url, window, cx); set_copy(&mut v.c, a, cx); });
        // The annotation bar.
        let ann = (annotating && has).then(|| {
            let (count, (css_ok, css_tip), (shot_ok, shot_tip)) = drawer::ann_bar();
            let (tool, chats, chat_on) = drawer::with(|d| (d.tool.clone(), d.chats.iter().map(|x| x.1.clone()).collect::<Vec<_>>(), d.chat.as_ref().and_then(|c| d.chats.iter().position(|x| &x.0 == c))));
            AnnCopy { tool, count, css_ok, css_tip, shot_ok, shot_tip, chats, chat_on }
        });
        if let Some(c) = ann.clone() { self.ann.update(cx, |v, cx| set_copy(&mut v.c, c, cx)); }
        // The password bar.
        let bar = act.as_ref().and_then(|t| passwords::bar(&t.id).map(|b| (t.id.clone(), b)));
        let has_bar = bar.is_some();
        self.pw.update(cx, |v, cx| v.set(bar, window, cx));
        // The body: a panel, or the empty state.
        let panel_name = drawer::with(|d| d.panel.clone());
        let body = BodyCopy::take(&panel_name, has);
        self.body.update(cx, |v, cx| v.set(body, window, cx));
        // The note field, the suggestions.
        let note_at = if panel_name.is_empty() && has { drawer::note_at() } else { None };
        let note_open = drawer::with(|d| d.note.is_some());
        self.note.update(cx, |v, cx| v.set_open(note_open, window, cx));
        let suggest = drawer::with(|d| d.suggest.clone());
        let any = !suggest.is_empty();
        self.suggest.update(cx, |v, cx| set_copy(&mut v.rows, suggest, cx));

        let lay = Lay { has_tab: has, panel: !panel_name.is_empty(), pw: has_bar, ann: ann.is_some(), note_at, suggest: any, left: drawer::with(|d| d.side == "left") };
        if lay.left != self.lay.left {
            if let Some(s) = self.shell.as_ref().and_then(|s| s.upgrade()) {
                let view: AnyView = cx.entity().into();
                let left = lay.left;
                s.update(cx, |s, cx| s.set_drawer_view(Some(crundi_shell::Drawer { view, left }), cx));
            }
        }
        set_copy(&mut self.lay, lay, cx);
    }
}

impl Render for BrowserDrawer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("drawer");
        let l = self.lay.clone();
        let (addr_h, pw_h, ann_h, note_h, sug_h) = (self.addr.read(cx).height, self.pw.read(cx).height, self.ann.read(cx).height, self.note.read(cx).height, self.suggest.read(cx).height);
        let body: AnyElement = if l.panel || !l.has_tab {
            self.body.clone().cached(sr().w_full().flex_1().min_h_0()).into_any_element()
        } else {
            div().relative().w_full().flex_1().min_h_0()
                .child(self.page.clone().cached(sr().size_full()))
                .when_some(l.note_at, |d, (x, y)| d.child(self.note.clone().cached(sr().absolute().left(px(x)).top(px(y) + T::SP_4).w(T::DRAWER_NOTE_W).h(note_h))))
                .into_any_element()
        };
        let focused = self.addr_focus.contains_focused(window, cx);
        div().relative().flex().flex_col().size_full().min_h_0().min_w_0().bg(T::bg_secondary()).overflow_hidden()
            .child(canvas(|b, _, _| drawer::set_root(b), |_, _, _, _| {}).absolute().top_0().left_0().size_full())
            .child(self.tabs.clone().cached(sr().w_full().h(T::TABBAR_H).flex_none()))
            .child(self.addr.clone().cached(sr().w_full().h(addr_h).flex_none()))
            .when(l.pw && l.has_tab, |d| d.child(self.pw.clone().cached(sr().w_full().h(pw_h).flex_none())))
            .when(l.ann, |d| d.child(self.ann.clone().cached(sr().w_full().h(ann_h).flex_none())))
            .child(body)
            // The address field's suggestions, under it.
            .when(focused && l.suggest, |d| d.child(self.suggest.clone().cached(sr().absolute().top(T::TABBAR_H + addr_h).left(T::DRAWER_NAV_W).right(T::DRAWER_TOOLS_W).h(sug_h))))
    }
}

// ─── The tabs row ───

#[derive(Clone, PartialEq, Default)]
struct TabsCopy { tabs: Vec<drawer::TabView>, left: bool }

struct TabsBar { c: TabsCopy }

impl Render for TabsBar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        rendered("drawer-tabs");
        // (w_full: the row spans the drawer, so New tab sits after the tabs and the rest at the right.)
        let mut row = w::tab_row().w_full();
        for (n, t) in self.c.tabs.iter().enumerate() {
            let (id, id2, id3) = (t.id.clone(), t.id.clone(), t.id.clone());
            // The network's badge before the title (.brd-net-dot), the title dimmed while asleep.
            let lead = (!t.badge.is_empty()).then(|| w::tag(t.badge, if t.bad { "error" } else if t.badge == "HP" { "ok" } else { "accent" }).into_any_element());
            let tab = w::tab_with(lead, t.title.clone(), t.active, t.sleeping)
                .child(w::icon_button(("tab-x", n), IconName::X, "Close tab").on_click(move |_, _, _| { let id = id2.clone(); browser::defer(move || drawer::close_tab(&id)); }));
            row = row.child(div().id(("tab", n)).min_w_0().cursor_pointer().rounded_t(T::R_CONTROL)
                .when(!t.active, |d| d.hover(|s| s.bg(T::bg_tertiary())))
                .child(tab).tooltip(w::tooltip(t.tip.clone()))
                .on_click(move |_, _, _| { let id = id.clone(); browser::defer(move || drawer::activate(&id)); })
                // A middle click closes it (the tabs' auxclick).
                .on_mouse_up(MouseButton::Middle, move |_, _, _| { let id = id3.clone(); browser::defer(move || drawer::close_tab(&id)); }));
        }
        let hide_icon = if self.c.left { IconName::ChevronLeft } else { IconName::ChevronRight };
        w::layer(row.child(w::icon_button("new", IconName::Plus, "New tab").on_click(|_, _, _| {
                let net = drawer::active_tab().map(|t| t.network).unwrap_or_else(|| "client".into());
                match drawer::new_tab(&crate::ctx::project(), drawer::START, &net, true, false) { Ok(_) => drawer::show_active(), Err(e) => crate::ctx::toast_error(&e) }
            }))
            .child(div().flex_1())
            .child(spot("menu", w::pick_menu_open("menu", IconName::Ellipsis, "History and browsing data", "", || pick_items("menu"), |id, _, _| pick("menu", id), |on, _, _| popped("menu", on))))
            .child(w::icon_button("hide", hide_icon, "Hide the browser (pages keep running)").on_click(|_, _, _| drawer::close()))
            .child(w::icon_button("closeall", IconName::X, "Close the browser: every tab in this project").on_click(|_, _, _| browser::defer(drawer::close_all))))
    }
}

// ─── The address bar (and the page's load error under it) ───

#[derive(Clone, PartialEq, Default)]
struct AddrCopy {
    has: bool, back: bool, fwd: bool, loading: bool,
    net_tip: String, tone: &'static str, dev_tip: String, dev_on: bool,
    annotating: bool, logins: usize,
}

struct AddressBar {
    c: AddrCopy,
    address: Entity<w::InputState>,
    page_focus: FocusHandle,
    shown_url: String,
    height: Pixels,
    _subs: Vec<Subscription>,
}

impl Measured for AddressBar { fn height(&self) -> Pixels { self.height } fn set_height(&mut self, h: Pixels) { self.height = h; } }

impl AddressBar {
    fn new(page_focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let address = cx.new(|cx| w::InputState::new(window, cx).placeholder("Enter URL or search"));
        let pf = page_focus.clone();
        let mut subs = vec![cx.subscribe_in(&address, window, move |_, st, ev: &w::InputEvent, window, cx| match ev {
            w::InputEvent::PressEnter { .. } => {
                let typed = st.read(cx).value().trim().to_string();
                drawer::with(|d| d.suggest.clear());
                browser::defer(move || drawer::go(&typed));
                window.focus(&page_focus, cx);
            }
            w::InputEvent::Change => drawer::url_typed(&st.read(cx).value()),
            // (The address follows the page again; the suggestions go.)
            w::InputEvent::Blur => { drawer::with(|d| d.suggest.clear()); drawer::render(); }
            _ => {}
        })];
        let f = address.read(cx).focus_handle(cx);
        subs.extend(w::watch_focus(&f, window, cx));
        AddressBar { c: AddrCopy::default(), address, page_focus: pf, shown_url: String::new(), height: T::DRAWER_BARS_H - T::TABBAR_H, _subs: subs }
    }

    /// The address follows the page, unless the person is typing in it.
    fn follow(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if url == self.shown_url || self.address.read(cx).focus_handle(cx).is_focused(window) { return; }
        self.shown_url = url.clone();
        self.address.update(cx, |st, cx| st.set_value(url, window, cx));
    }

    /// Esc in the address: what was typed goes, the page's address comes
    /// back, and the field lets go (VD urlEl keydown Escape: blur, renderBar).
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = drawer::active_tab().map(|t| if t.url == "about:blank" { String::new() } else { t.url }).unwrap_or_default();
        self.shown_url = url.clone();
        self.address.update(cx, |st, cx| st.set_value(url, window, cx));
        drawer::with(|d| d.suggest.clear());
        window.focus(&self.page_focus, cx);
        drawer::render();
    }
}

impl Render for AddressBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("drawer-address");
        let c = &self.c;
        // The Network button keeps its icon and takes the tab's tone; Device
        // is lit while a device is set (renderBar: .brd-netbtn.server/.hp/.bad,
        // .brd-devbtn.on); both are off with no tab.
        let net_tone = match c.tone { "server" => Some(T::accent()), "hp" => Some(T::green()), "bad" => Some(T::red()), _ => None };
        let hist = |h: i32| move |_: &ClickEvent, _: &mut Window, _: &mut App| { if let Some(i) = drawer::active_tab().and_then(|t| t.panel) { browser::defer(move || browser::history(i, h)); } };
        let logins = c.logins;
        // Reload: its arrow turns while the page loads (.brd-loading), still Reload.
        let reload = if c.loading {
            w::sized(Button::new("reload").ghost(), None, None, false, w::Ctl::Normal).tooltip("Reload")
                .child(w::spinner(T::ICON_BTN_ICON).icon(IconName::RefreshCw).color(T::text_secondary()))
        } else { w::icon_button("reload", IconName::RefreshCw, "Reload") };
        let bar = w::toolbar()
            .child(w::disabled(w::icon_button("back", IconName::ChevronLeft, "Back"), !c.back).on_click(hist(-1)))
            .child(w::disabled(w::icon_button("forward", IconName::ChevronRight, "Forward"), !c.fwd).on_click(hist(1)))
            .child(w::disabled(reload, !c.has).on_click(hist(0)))
            .child(div().flex_1().min_w_0().child(w::input(&self.address)))
            .when(logins > 0, |d| d.child(w::icon_button("pw-lock", IconName::Lock, if logins == 1 { "A saved login for this site".to_string() } else { format!("{logins} saved logins for this site") })
                .on_click(|_, _, _| passwords::site_menu())))
            .child(spot("net", w::pick_menu_toned("net", IconName::Network, c.net_tip.clone(), "This tab’s network", net_tone, !c.has, || pick_items("net"), |id, _, _| pick("net", id), |on, _, _| popped("net", on))))
            .child(spot("device", w::pick_menu_toned("device", IconName::Smartphone, c.dev_tip.clone(), "Device size", c.dev_on.then(T::accent), !c.has, || pick_items("dev"), |id, _, _| pick("dev", id), |on, _, _| popped("device", on))))
            .child(w::disabled(w::icon_toggle("annotate", IconName::MessageSquare, "Annotate the page", c.annotating), !c.has).on_click(|_, _, _| { let on = !drawer::with(|d| d.annotating); drawer::annotate(on); }))
            .child(w::disabled(w::icon_button("devtools", IconName::Terminal, "DevTools"), !c.has).on_click(|_, _, _| browser::defer(|| if let Some(i) = drawer::active_tab().and_then(|t| t.panel) { browser::devtools(i); })));
        let root = div().relative().w_full().flex().flex_col().child(bar)
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                if e.keystroke.key == "escape" && this.address.read(cx).focus_handle(cx).is_focused(window) { this.escape(window, cx); cx.stop_propagation(); }
            }))
            .child(measure(cx.entity()));
        let f = self.address.read(cx).focus_handle(cx);
        w::text_box(&f, root, window, cx)
    }
}

// ─── The annotation bar ───

#[derive(Clone, PartialEq, Default)]
struct AnnCopy { tool: String, count: String, css_ok: bool, css_tip: String, shot_ok: bool, shot_tip: String, chats: Vec<String>, chat_on: Option<usize> }

struct AnnBar { c: AnnCopy, height: Pixels }

impl Measured for AnnBar { fn height(&self) -> Pixels { self.height } fn set_height(&mut self, h: Pixels) { self.height = h; } }

impl Render for AnnBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("drawer-ann");
        let c = &self.c;
        let tool = c.tool.clone();
        let t = |id: &'static str, icon: IconName, tip: &'static str| w::icon_toggle(id, icon, tip, tool == id).on_click(move |_, _, _| drawer::set_tool(id));
        w::layer(div().relative().w_full().child(w::toolbar()
            .child(t("pick", IconName::Layers, "Select an element and annotate it (its CSS selector goes with the note)"))
            .child(t("pin", IconName::Pin, "Draw: a pin with a note"))
            .child(t("box", IconName::Square, "Draw: a box"))
            .child(t("pen", IconName::Pencil, "Draw: freehand"))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).text_color(T::text_secondary()).child(c.count.clone()))
            // The chat to send to: a select showing the chat's title (.brd-chat,
            // max 110px), "No chat open in this project" when there is none.
            .child(w::tip(div().id("chat-sel").flex_none().max_w(px(110.)).overflow_hidden().child(w::select("chat", c.chats.iter().map(|t| SharedString::from(t.clone())).collect(), c.chat_on, "No chat open in this project", c.chats.is_empty(),
                |i, _, _| { let id = drawer::with(|d| d.chats.get(i).map(|x| x.0.clone())); if let Some(id) = id { pick("chat", &id); } })), "Chat to send to"))
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Send"))
            .child(w::disabled(w::icon_button("send-css", IconName::FileText, c.css_tip.clone()), !c.css_ok).on_click(|_, _, _| drawer::send(false)))
            .child(w::disabled(w::icon_button("send-shot", IconName::Image, c.shot_tip.clone()), !c.shot_ok).on_click(|_, _, _| drawer::send(true)))
            .child(w::icon_button("clear-marks", IconName::Trash, "Remove your marks").on_click(|_, _, _| if let Some(i) = drawer::active_tab().and_then(|t| t.panel) { browser::clear_marks(i); }))
            .child(w::icon_button("done", IconName::Check, "Done annotating").on_click(|_, _, _| drawer::annotate(false))))
            .child(measure(cx.entity())))
    }
}

// ─── The password bar (renderPw): save, update or fill ───

struct PwBar {
    c: Option<(String, passwords::Bar)>,
    pw_user: Entity<w::InputState>,
    /// The login picked in a fill bar's menu.
    fill_sel: Option<String>,
    shown_bar: String,
    height: Pixels,
    _subs: Vec<Subscription>,
}

impl Measured for PwBar { fn height(&self) -> Pixels { self.height } fn set_height(&mut self, h: Pixels) { self.height = h; } }

impl PwBar {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pw_user = cx.new(|cx| w::InputState::new(window, cx).placeholder("Username"));
        let mut subs = vec![cx.subscribe_in(&pw_user, window, |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::PressEnter { .. } = ev {
            let u = st.read(cx).value().to_string();
            if let Some(t) = drawer::active_tab() { passwords::save(&t.id, &u); }
        })];
        let f = pw_user.read(cx).focus_handle(cx);
        subs.extend(w::watch_focus(&f, window, cx));
        PwBar { c: None, pw_user, fill_sel: None, shown_bar: String::new(), height: T::DRAWER_BARS_H - T::TABBAR_H, _subs: subs }
    }

    /// A new save offer puts its username in the bar's field.
    fn set(&mut self, bar: Option<(String, passwords::Bar)>, window: &mut Window, cx: &mut Context<Self>) {
        let key = match &bar { Some((t, passwords::Bar::Save { offer, .. })) => format!("{t}:{offer}"), Some((t, _)) => format!("{t}:fill"), None => String::new() };
        if key != self.shown_bar {
            self.shown_bar = key;
            self.fill_sel = None;
            if let Some((_, passwords::Bar::Save { username, .. })) = &bar { let u = username.clone(); self.pw_user.update(cx, |st, cx| st.set_value(u, window, cx)); }
            cx.notify();
        }
        set_copy(&mut self.c, bar, cx);
    }
}

impl Render for PwBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("drawer-pw");
        let host = |o: &str| o.split("://").nth(1).unwrap_or(o).to_string();
        let row = div().flex().flex_none().items_center().gap(T::SP_6).px(T::SP_8).py(T::SP_6).bg(T::bg_tertiary()).border_b_1().border_color(T::border())
            .child(div().text_size(T::ICON_SM).text_color(T::accent()).child(IconName::Lock));
        let Some((tab, b)) = self.c.clone() else { return div().into_any_element() };
        let (t1, t2, t3) = (tab.clone(), tab.clone(), tab);
        let row = match b {
            passwords::Bar::Save { origin, update: false, .. } => {
                let user = self.pw_user.clone();
                // (.brd-pwmsg grows from 160 px; the username field up to 180 px, never cut.)
                row.child(div().flex_1().min_w(px(160.)).text_size(T::TEXT_12).text_color(T::text()).child(format!("Save the password for {}?", host(&origin))))
                    .child(div().flex_none().w(px(180.)).child(w::input(&self.pw_user)))
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
                let labels: Vec<SharedString> = logins.iter().map(|l| if l.1.is_empty() { "(no username)".into() } else { l.1.clone().into() }).collect();
                let on = logins.iter().position(|l| l.0 == sel).unwrap_or(0);
                let ids: Vec<String> = logins.iter().map(|l| l.0.clone()).collect();
                let several = logins.len() > 1;
                let me = cx.entity().downgrade();
                // Several logins: a select of them (.brd-pwpick); one: its name in bold.
                row.child(div().flex_1().min_w(px(160.)).text_size(T::TEXT_12).text_color(T::text()).child(format!("Fill your saved login for {}?", host(&origin))))
                    .when(several, |d| d.child(div().flex_none().max_w(px(180.)).child(w::dropdown_of("pw-pick", labels.clone(), on, false, false,
                        move |i, _, cx| { let id = ids.get(i).cloned(); let _ = me.update(cx, |b, cx| { b.fill_sel = id; cx.notify(); }); }))))
                    .when(!several, |d| d.child(div().flex_none().text_size(T::TEXT_12).font_weight(T::W_BOLD).text_color(T::text()).child(name)))
                    .child(w::primary("pw-fill", "Fill").on_click(move |_, _, _| passwords::fill(&t1, &sel)))
                    .child(w::icon_button("pw-x", IconName::X, "Not for this site (until the app restarts)").on_click(move |_, _, _| passwords::dismiss(&t3)))
            }
        };
        let f = self.pw_user.read(cx).focus_handle(cx);
        w::text_box(&f, div().relative().w_full().child(row).child(measure(cx.entity())), window, cx)
    }
}

// ─── The body: a panel (History, Clear browsing data, Passwords), or the empty state ───

#[derive(Clone, PartialEq, Default)]
struct BodyCopy {
    panel: String,
    has_tab: bool,
    /// History: (url, title, how long ago), and its empty line.
    hist: Vec<(String, String, String)>,
    hist_empty: &'static str,
    clr: drawer::Clear,
    /// Passwords: kept here, why not; (id, username, host); the sites never
    /// offered; the login waiting for the PIN; the shown passwords.
    pw_ok: bool,
    pw_reason: String,
    pw_rows: Vec<(String, String, String)>,
    pw_never: Vec<String>,
    pin_for: Option<String>,
    revealed: Vec<(String, String)>,
    /// Homepool for this browser.
    hp: crate::hpdrawer::PanelCopy,
}

impl BodyCopy {
    fn take(panel: &str, has_tab: bool) -> Self {
        let mut c = BodyCopy { panel: panel.to_string(), has_tab, ..Default::default() };
        match panel {
            "history" => { let (rows, empty) = drawer::history_rows(); c.hist = rows; c.hist_empty = empty; }
            "clear" => c.clr = drawer::with(|d| d.clr.clone()),
            "homepool" => c.hp = crate::hpdrawer::panel(),
            "passwords" => {
                (c.pw_ok, c.pw_reason) = passwords::status();
                (c.pw_rows, c.pw_never) = passwords::panel_rows();
                c.pin_for = passwords::pin_for();
                c.revealed = passwords::revealed_all();
            }
            _ => {}
        }
        c
    }
}

struct DrawerBody {
    c: BodyCopy,
    hist: Entity<w::InputState>,
    site: Entity<w::InputState>,
    pin: Entity<w::InputState>,
    _subs: Vec<Subscription>,
}

impl DrawerBody {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hist = cx.new(|cx| w::InputState::new(window, cx).placeholder("Search history"));
        let site = cx.new(|cx| w::InputState::new(window, cx).placeholder("example.com"));
        let pin = cx.new(|cx| w::InputState::new(window, cx).placeholder("PIN").masked(true));
        let mut subs = vec![
            cx.subscribe_in(&hist, window, |_, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev { drawer::history_query(&st.read(cx).value()); }),
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
        for e in [&hist, &site, &pin] { let f = e.read(cx).focus_handle(cx); subs.extend(w::watch_focus(&f, window, cx)); }
        DrawerBody { c: BodyCopy::default(), hist, site, pin, _subs: subs }
    }

    fn set(&mut self, c: BodyCopy, window: &mut Window, cx: &mut Context<Self>) {
        if c.panel == "clear" && self.site.read(cx).value() != c.clr.site.as_str() {
            let s = c.clr.site.clone();
            self.site.update(cx, |st, cx| st.set_value(s, window, cx));
        }
        // History opens with its search empty and focused (showHistory: hq.focus()).
        if c.panel == "history" && self.c.panel != "history" {
            self.hist.update(cx, |st, cx| st.set_value("", window, cx));
            let f = self.hist.read(cx).focus_handle(cx);
            window.focus(&f, cx);
        }
        set_copy(&mut self.c, c, cx);
    }

    fn panel_el(&self, cx: &mut Context<Self>) -> AnyElement {
        let which = self.c.panel.as_str();
        let head = w::toolbar()
            .child(div().flex_1().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text())
                .child(match which { "history" => "History", "clear" => "Clear browsing data", "passwords" => "Saved passwords", _ => "Homepool for this browser" }))
            .when(which == "history", |d| d.child(w::button("to-clear", "Clear browsing data…").on_click(|_, _, _| drawer::open_panel("clear"))))
            .child(w::icon_button("panel-close", IconName::X, "Close").on_click(|_, _, _| drawer::panel_close()));
        let scroll = |body: AnyElement| div().id("drawer-panel").flex_1().min_h_0().overflow_y_scroll().child(body).into_any_element();
        let body: AnyElement = match which {
            // The history: up to 200 rows, only those on screen drawn.
            "history" => {
                let n = self.c.hist.len();
                div().flex().flex_col().flex_1().min_h_0().gap(T::SP_8).p(T::SP_12).child(w::input(&self.hist))
                    .child(if n == 0 { w::empty(self.c.hist_empty).into_any_element() } else {
                        uniform_list("drawer-hist", n, cx.processor(|this, range: std::ops::Range<usize>, _, _| {
                            range.filter_map(|i| this.c.hist.get(i).cloned().map(|(url, title, ago)| {
                                let u = url.clone();
                                div().pb(T::SP_2).child(w::address_row(("hist", i), if title.is_empty() { url.clone() } else { title }, url, ago)
                                    .on_click(move |_, _, _| { let u = u.clone(); browser::defer(move || drawer::history_open(&u)); })).into_any_element()
                            })).collect()
                        })).flex_1().min_h_0().into_any_element()
                    })
                    .into_any_element()
            }
            "clear" => {
                let c = &self.c.clr;
                let set = |f: fn(&mut drawer::Clear, bool)| move |on: bool| { drawer::with(|d| f(&mut d.clr, on)); drawer::render(); };
                let body = scroll(div().flex().flex_col().px(T::SP_12).py(T::SP_8)
                    .child(row("clr-all", false, !c.site_only, "Every site", "", |_| { drawer::with(|d| d.clr.site_only = false); drawer::render(); }))
                    .child(row("clr-site", false, c.site_only, "Only this site", "Its subdomains too", |_| { drawer::with(|d| d.clr.site_only = true); drawer::render(); }))
                    .child(w::input(&self.site))
                    .child(row("clr-history", true, c.history, "Browsing history", "Every address visited, and each tab’s back and forward list", set(|c, on| c.history = on)))
                    .child(row("clr-cookies", true, c.cookies, "Cookies and site data", "Logins, local storage, databases, service workers. Signs you out of sites.", set(|c, on| c.cookies = on)))
                    .child(row("clr-cache", true, c.cache, "Cached images and files", "Pages load again from the network", set(|c, on| c.cache = on)))
                    .child(row("clr-passwords", true, c.passwords, "Saved passwords", "Every saved login, on every site (not limited to one site)", set(|c, on| c.passwords = on)))
                    .child(div().my(T::SP_8).mx(T::SP_2).text_size(T::TEXT_12).text_color(T::text_muted()).child("For this computer’s browser: every tab in every project, on every network. Open tabs stay open. For one site, the back and forward lists are left as they are."))
                    .into_any_element());
                div().flex().flex_col().flex_1().min_h_0().child(body)
                    .child(div().flex().flex_none().justify_end().gap(T::SP_8).px(T::SP_12).py(T::SP_10).border_t_1().border_color(T::border())
                        .child(w::button("clr-cancel", "Cancel").on_click(|_, _, _| drawer::panel_close()))
                        .child(w::danger("clr-run", "Clear").on_click(|_, _, _| drawer::clear_run())))
                    .into_any_element()
            }
            "passwords" => {
                let c = &self.c;
                let mut d = div().flex().flex_col().gap(T::SP_8).p(T::SP_12);
                if !c.pw_ok && !c.pw_reason.is_empty() { d = d.child(div().text_size(T::TEXT_12).text_color(T::error_text()).child(c.pw_reason.clone())); }
                d = d.child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Kept on this computer only, encrypted for your account on it, as Chrome keeps them. Filling needs no PIN; showing one does. Chats can never read them."));
                // The PIN row (.brd-pinrow): under the note, over the list, while a Show waits for it.
                if c.pin_for.is_some() {
                    let pin = self.pin.clone();
                    d = d.child(div().flex().items_center().gap(T::SP_8).my(T::SP_8)
                        .child(div().flex_none().w(px(140.)).child(w::input(&self.pin)))
                        .child(w::primary("pw-unlock", "Unlock").on_click(move |_, _, cx| passwords::unlock(&pin.read(cx).value()))));
                }
                if c.pw_rows.is_empty() { d = d.child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("No saved passwords yet. Sign in to a site in a drawer tab and choose Save.")); }
                for (n, (id, user, host)) in c.pw_rows.iter().enumerate() {
                    let shown = c.revealed.iter().find(|r| &r.0 == id).map(|r| r.1.clone());
                    let (i1, i2) = (id.clone(), id.clone());
                    d = d.child(div().flex().items_center().gap(T::SP_8).py(T::SP_6).px(T::SP_2).border_b_1().border_color(T::border())
                        .child(div().flex().flex_col().flex_1().min_w_0()
                            .child(div().truncate().text_size(T::TEXT_13).text_color(T::text()).child(if user.is_empty() { "(no username)".to_string() } else { user.clone() }))
                            .child(div().truncate().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(host.clone())))
                        .child(div().px(T::SP_6).py(T::SP_2).rounded(T::R_CHIP).bg(T::bg()).font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text()).child(shown.clone().unwrap_or_else(|| "••••••••".into())))
                        .child(w::button(("pw-show", n), if shown.is_some() { "Hide" } else { "Show" }).on_click(move |_, _, _| passwords::show(&i1)))
                        .child(w::sized(Button::new(("pw-del", n)), None, Some(IconName::Trash), false, w::Ctl::Normal).tooltip("Delete").on_click(move |_, _, _| passwords::remove(&i2))));
                }
                if !c.pw_never.is_empty() {
                    d = d.child(div().pt(T::SP_8).text_size(T::TEXT_12).text_color(T::text_secondary()).child("Never offered for:"));
                    for (n, o) in c.pw_never.iter().enumerate() {
                        let o2 = o.clone();
                        d = d.child(div().flex().items_center().gap(T::SP_8)
                            .child(div().flex_1().truncate().text_size(T::TEXT_13).text_color(T::text()).child(o.split("://").nth(1).unwrap_or(o).to_string()))
                            .child(w::button(("pw-unnever", n), "Remove").on_click(move |_, _, _| passwords::unnever(&o2))));
                    }
                }
                scroll(d.into_any_element())
            }
            "homepool" => {
                use crate::hpdrawer as hp;
                let c = &self.c.hp;
                let note = |t: String| div().text_size(T::TEXT_12).text_color(T::text_muted()).child(t);
                let foot = div().flex().flex_none().justify_end().gap(T::SP_8).px(T::SP_12).py(T::SP_10).border_t_1().border_color(T::border());
                if !c.available {
                    scroll(div().p(T::SP_12).child(note("Homepool is not available on this server.".into())).into_any_element())
                } else {
                    let nets = |d: Div| c.nets.iter().enumerate().fold(d, |d, (n, (id, name, me, on))| {
                        let id = id.clone();
                        d.child(chk_row(("hp-net", n), true, *on, *me, name.clone(), if *me { "This PC is a node of this network, so its browser does not use it" } else { "" }, T::text_muted(), move |_| hp::toggle_net(&id)))
                    });
                    let mut d = div().flex().flex_col().px(T::SP_12).py(T::SP_8);
                    if !c.has_key {
                        d = d.child(note("Tabs can go out through your homepool networks: your home line, a friend\u{2019}s. This makes the PC browser key: you choose the networks it may use, and switch it off here or in Homepool \u{203a} Access at any time. Chats ask you before they use it.".into()));
                        d = if c.nets.is_empty() { d.child(note("You have no homepool networks yet: make one in the Homepool tab.".into())) } else { nets(d) };
                        d = d.child(row("hp-fb", true, c.fallback, "Leave from the server when no exit is up", "Off: with no node up, pages do not load (nothing leaves another way)", |_| hp::toggle_fallback()));
                        let none = c.nets.is_empty();
                        div().flex().flex_col().flex_1().min_h_0().child(scroll(d.into_any_element()))
                            .child(foot.child(w::button("hp-cancel", "Cancel").on_click(|_, _, _| drawer::panel_close()))
                                .child(w::disabled(w::danger("hp-create", "Make the PC browser key"), none).on_click(|_, _, _| hp::save())))
                            .into_any_element()
                    } else {
                        let refusal = !c.refusal.is_empty();
                        d = d.child(chk_row("hp-on", true, c.on, false, "PC browser key on", if refusal { c.refusal.clone() } else { "Off: homepool tabs stop loading at once".into() }, if refusal { T::red() } else { T::text_muted() }, |_| hp::toggle_on()));
                        d = d.child(note("Exits now (each network keeps one line, so its tabs keep one address):".into()));
                        if c.exits.is_empty() { d = d.child(note("No networks.".into())); }
                        for (name, text, up) in &c.exits {
                            d = d.child(div().flex().items_center().gap(T::SP_8).py(T::SP_6).text_size(T::TEXT_13)
                                .child(div().text_color(T::text()).child(name.clone())).child(div().flex_1())
                                .child(div().text_size(T::TEXT_12).text_color(if *up { T::green() } else { T::red() }).child(text.clone())));
                        }
                        d = d.child(div().mt(T::SP_12).child(note("Networks it may use:".into())));
                        d = nets(d);
                        d = d.child(row("hp-fb", true, c.fallback, "Leave from the server when no exit is up", "", |_| hp::toggle_fallback()));
                        d = d.child(div().mt(T::SP_12).child(note("Chats you approved (others ask you first; their requests are in Homepool \u{203a} Requests):".into())));
                        if c.approvals.is_empty() { d = d.child(note("None yet.".into())); }
                        for (n, (chat, project, label, under)) in c.approvals.iter().enumerate() {
                            let (ch, pr) = (chat.clone(), project.clone());
                            d = d.child(div().flex().items_center().gap(T::SP_8).py(T::SP_6).text_size(T::TEXT_13)
                                .child(div().text_color(T::text()).child(label.clone()))
                                .when(!under.is_empty(), |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(under.clone())))
                                .child(div().flex_1())
                                .child(w::button(("hp-rm", n), "Remove").on_click(move |_, _, _| hp::revoke(&ch, &pr))));
                        }
                        div().flex().flex_col().flex_1().min_h_0().child(scroll(d.into_any_element()))
                            .child(foot.child(w::button("hp-close", "Close").on_click(|_, _, _| drawer::panel_close()))
                                .child(w::danger("hp-save", "Save").on_click(|_, _, _| hp::save())))
                            .into_any_element()
                    }
                }
            }
            _ => w::empty("Homepool is not available on this server.").into_any_element(),
        };
        div().flex().flex_col().size_full().min_h_0().bg(T::bg_secondary()).child(head).child(body).into_any_element()
    }
}

impl Render for DrawerBody {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("drawer-body");
        let el: AnyElement = if !self.c.panel.is_empty() {
            self.panel_el(cx)
        } else {
            w::empty("No browser tabs in this project yet.")
                .size_full()
                .child(w::button("open", "Open a tab").on_click(|_, _, _| {
                    match drawer::new_tab(&crate::ctx::project(), drawer::START, "client", true, true) { Ok(_) => drawer::show_active(), Err(e) => crate::ctx::toast_error(&e) }
                }))
                .into_any_element()
        };
        // In one layer unless one of its text boxes has the focus (its caret).
        let typing = [&self.hist, &self.site, &self.pin].iter().any(|e| e.read(cx).focus_handle(cx).contains_focused(window, cx));
        let root = div().size_full().child(el);
        if typing { root.into_any_element() } else { w::layer(root).into_any_element() }
    }
}

// ─── A mark's note field ───

struct NoteBox { note: Entity<w::InputState>, open: bool, height: Pixels, _subs: Vec<Subscription> }

impl Measured for NoteBox { fn height(&self) -> Pixels { self.height } fn set_height(&mut self, h: Pixels) { self.height = h; } }

impl NoteBox {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let note = cx.new(|cx| w::InputState::new(window, cx).placeholder("Note (Enter to keep, Esc to skip)"));
        // Enter keeps the note; a click elsewhere keeps what was typed too
        // (VD askNote: blur keeps a non-empty note, then the field goes).
        let mut subs = vec![cx.subscribe_in(&note, window, |_, st, ev: &w::InputEvent, _, cx| match ev {
            w::InputEvent::PressEnter { .. } | w::InputEvent::Blur => { let t = st.read(cx).value().to_string(); drawer::note(&t, true); }
            _ => {}
        })];
        let f = note.read(cx).focus_handle(cx);
        subs.extend(w::watch_focus(&f, window, cx));
        NoteBox { note, open: false, height: T::CONTROL + T::SP_12, _subs: subs }
    }

    /// A mark's note opened (empty, with the keys) or closed.
    fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if open == self.open { return; }
        self.open = open;
        self.note.update(cx, |st, cx| st.set_value("", window, cx));
        if open { self.note.update(cx, |st, cx| st.focus(window, cx)); }
    }
}

impl Render for NoteBox {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("drawer-note");
        let el = div().relative().w_full().p(T::SP_6).rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border())
            .on_key_down(|e: &KeyDownEvent, _, _| if e.keystroke.key == "escape" { drawer::note("", false); })
            .child(w::input(&self.note))
            .child(measure(cx.entity()));
        let f = self.note.read(cx).focus_handle(cx);
        w::text_box(&f, el, window, cx)
    }
}

// ─── The address field's suggestions ───

struct SuggestList { rows: Vec<rules::Visit>, height: Pixels }

impl Measured for SuggestList { fn height(&self) -> Pixels { self.height } fn set_height(&mut self, h: Pixels) { self.height = h; } }

impl Render for SuggestList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("drawer-suggest");
        let mut list = w::card().relative().w_full().p(T::SP_4).bg(T::bg_secondary());
        for (n, h) in self.rows.iter().enumerate() {
            let u = h.url.clone();
            list = list.child(w::address_row(("sug", n), if h.title.is_empty() { h.url.clone() } else { h.title.clone() }, h.url.clone(), "")
                .on_mouse_down(MouseButton::Left, move |_, _, _| { let u = u.clone(); drawer::with(|d| d.suggest.clear()); drawer::render(); browser::defer(move || drawer::go(&u)); }));
        }
        w::layer(list.child(measure(cx.entity())))
    }
}

/// A popover's rows as the widgets crate's pick menu takes them (made when it opens).
/// A row (.brd-chk): its mark, its label and the line under it; the whole row takes the click.
fn row(id: impl Into<ElementId>, multi: bool, on: bool, label: impl Into<SharedString>, sub: impl Into<SharedString>, f: impl Fn(bool) + 'static) -> Stateful<Div> {
    chk_row(id, multi, on, false, label, sub, T::text_muted(), f)
}

/// A row that may be off (a network this PC is a node of), its line in a tone of its own.
#[allow(clippy::too_many_arguments)]
fn chk_row(id: impl Into<ElementId>, multi: bool, on: bool, off: bool, label: impl Into<SharedString>, sub: impl Into<SharedString>, sub_tone: Hsla, f: impl Fn(bool) + 'static) -> Stateful<Div> {
    let sub: SharedString = sub.into();
    div().id(id).flex().items_start().gap(T::SP_10).py(T::SP_8).px(T::SP_2)
        .when(!off, |d| d.cursor_pointer().on_click(move |_, _, _| f(if multi { !on } else { true })))
        .when(off, |d| d.opacity(0.5))
        .child(w::choice_mark(multi, on, false))
        .child(div().flex().flex_col().min_w_0().text_size(T::TEXT_13).text_color(T::text()).child(label.into())
            .when(!sub.is_empty(), |d| d.child(div().pt(T::SP_2).text_size(T::TEXT_12).text_color(sub_tone).child(sub))))
}

fn pick_items(which: &str) -> Vec<w::PickItem> {
    drawer::pop_items(which).1.into_iter().map(|p| w::PickItem { id: p.id, label: p.label.into(), kind: p.kind, on: p.on, icon: p.icon }).collect()
}

/// A row picked in a popover (the drawer's pop_pick, as its popovers do).
fn pick(which: &str, id: &str) {
    drawer::with(|d| d.pop = which.to_string());
    let id = id.to_string();
    browser::defer(move || drawer::pop_pick(&id));
}

/// A popover opened or closed (by a click on its button, a pick, Esc or a
/// click outside): which one is open, for the driver's state.
fn popped(which: &'static str, open: bool) {
    let which = if which == "device" { "dev" } else { which };
    drawer::with(|d| if open { d.pop = which.to_string() } else if d.pop == which { d.pop.clear() });
    if open && which == "chat" { drawer::fill_chats(); }
    if open && which == "net" { crate::hp_refresh(); }
}

/// A control whose place the driver reports (`spots` in the drawer's
/// state: the suite clicks the real button, as a person does).
fn spot(name: &'static str, el: impl IntoElement) -> Div {
    div().relative().flex_none().child(el).child(canvas(move |b, _, _| drawer::set_spot(name, b), |_, _, _, _| {}).absolute().top_0().left_0().size_full())
}
