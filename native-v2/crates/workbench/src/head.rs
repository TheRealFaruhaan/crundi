//! The pane header (specs/pane-header-wide.md and -narrow.md, the web's
//! headHtmlLive / headHtmlPanel, native-gpui's head.rs): the drag grip, the
//! state dot or the kind's icon, the title (a chat's: click to rename), the
//! state badge; then the tools (a chat's tags, its secrets lock, the kind's
//! buttons, A- ↺ A+, pin), full screen, "…" and the two-click ×. A narrow
//! chat head folds its tools into the "…" menu (the web's .term-head-tools
//! under @container (max-width: 559px)); a panel's head never folds, as the
//! web's.
//!
//! A cached leaf view of its own, redrawn only when what it shows changes:
//! its info and extra (the kind's `watch` observes what they read and calls
//! `refresh`, which notifies only on a change), its frame (armed, pinned,
//! narrow, focused: the workbench notifies the head whose frame changed,
//! F11), its own menu and rename box.

use crate::{extra::{self, HeadExtra, KindHead}, Workbench};
use crundi_core::head as rules;
use crundi_shell::Projects;
use crundi_theme::{IconName, T};
use crundi_widgets::{self as w, HeadContent as C, HeadLook as L};
use gpui::{prelude::FluentBuilder as _, *};
use std::rc::Rc;

/// A pane's title, its long title (wide panes) and its state.
#[derive(Clone, Default, PartialEq)]
pub struct HeadInfo { pub title: String, pub long: String, pub state: String }

/// What the workbench decides about a head.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Frame { pub leaf: u32, pub w: f32, pub armed: bool, pub pinned: bool, pub narrow: bool, pub full: bool, pub focused: bool }

pub struct PaneHead {
    key: String,
    chat: bool,
    info: Rc<dyn Fn(&App) -> HeadInfo>,
    /// The kind's head (icon, buttons, extra, actions), if it registered one.
    kind: Option<KindHead>,
    projects: Entity<Projects>,
    wb: WeakEntity<Workbench>,
    /// The info and extra as last drawn (`refresh` compares with them).
    last: Option<(HeadInfo, HeadExtra)>,
    /// The narrow head's "…" menu is open.
    menu: bool,
    /// Where the press that closed the menu from outside was (the "…"
    /// button's own press then does not open it again).
    closed_at: Option<Point<Pixels>>,
    /// The rename box while renaming.
    renaming: Option<(Entity<w::InputState>, Subscription)>,
    /// A rename just made: (the new title, the title it replaced). Shown
    /// until the server's title changes (the web sets it at once).
    renamed: Option<(String, String)>,
    _subs: Vec<Subscription>,
}

/// Where a pane's rename goes (the web's cellApiBase + '/rename'): a chat's
/// or a Claude terminal's; None: its title can't be renamed.
pub(crate) fn rename_path(key: &str) -> Option<String> {
    if let Some(id) = key.strip_prefix("live:") { return Some(format!("/api/ui-sessions/{id}/rename")); }
    key.strip_prefix("term:").map(|id| format!("/api/terminals/{id}/rename"))
}

impl PaneHead {
    pub(crate) fn new(key: String, chat: bool, info: Rc<dyn Fn(&App) -> HeadInfo>, projects: Entity<Projects>, wb: WeakEntity<Workbench>, watch: Box<dyn FnOnce(&mut Context<PaneHead>) -> Vec<Subscription>>, cx: &mut Context<Self>) -> Self {
        let subs = watch(cx);
        let kind = extra::head_of(cx, &key);
        PaneHead { key, chat, info, kind, projects, wb, last: None, menu: false, closed_at: None, renaming: None, renamed: None, _subs: subs }
    }

    /// What the title or the extra reads may have changed (the kind's
    /// `watch`): redraw only if the info or the extra did.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let now = ((self.info)(cx), self.extra(cx));
        if self.last.as_ref() != Some(&now) { self.last = Some(now); cx.notify(); }
    }

    fn extra(&self, cx: &App) -> HeadExtra {
        match self.kind.as_ref().and_then(|k| k.extra.clone()) {
            Some(f) => f(&self.key, cx),
            None if self.key.starts_with("live:") => {
                let id = &self.key["live:".len()..];
                let p = self.projects.read(cx);
                HeadExtra::chat_from(p.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(id)), "")
            }
            None => HeadExtra::default(),
        }
    }

    /// The title as last drawn (the driver's "heads").
    pub fn drawn_title(&self) -> Option<String> {
        let (i, _) = self.last.as_ref()?;
        Some(self.shown_title(&i.title).to_string())
    }

    /// The driver's view of the head: its tags, buttons, menu and rename box.
    pub(crate) fn props(&self) -> serde_json::Value {
        let e = self.last.as_ref().map(|(_, e)| e.clone()).unwrap_or_default();
        let mut buttons: Vec<&'static str> = self.kind.as_ref().map(|k| k.buttons.iter().map(|b| b.action).collect()).unwrap_or_default();
        buttons.extend(e.buttons.iter().map(|b| b.action));
        serde_json::json!({
            "tags": e.tags.iter().map(|t| t.text.to_string()).collect::<Vec<_>>(),
            "secrets": e.secrets, "buttons": buttons,
            "icon": self.kind.as_ref().and_then(|k| k.icon).is_some(),
            "menu": self.menu, "renaming": self.renaming.is_some(),
        })
    }

    fn shown_title<'a>(&'a self, title: &'a str) -> &'a str {
        match &self.renamed { Some((new, old)) if old == title => new, _ => title }
    }

    fn act(&self, cx: &mut App, f: impl FnOnce(&mut Workbench, &mut Context<Workbench>) + 'static) {
        if let Some(wb) = self.wb.upgrade() { wb.update(cx, f); }
    }

    /// A tag's, a button's or the lock's action: the kind's.
    fn run(&self, action: &str, window: &mut Window, cx: &mut App) {
        if let Some(f) = self.kind.as_ref().and_then(|k| k.on_action.clone()) { f(&self.key, action, window, cx); }
    }

    /// The "…" menu, open or closed.
    pub(crate) fn set_menu(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.menu != open { self.menu = open; cx.notify(); }
    }

    /// Click on the title: the box, its text all selected, focused.
    pub(crate) fn start_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming.is_some() || rename_path(&self.key).is_none() { return; }
        let current = (self.info)(cx).title;
        let current = self.shown_title(&current).to_string();
        let st = cx.new(|cx| w::InputState::new(window, cx));
        st.update(cx, |s, cx| { s.set_value(current, window, cx); s.select_all(window, cx); s.focus(window, cx); });
        let sub = cx.subscribe_in(&st, window, |this, _, ev: &w::InputEvent, _, cx| match ev {
            w::InputEvent::PressEnter { .. } | w::InputEvent::Blur => this.end_rename(true, cx),
            _ => {}
        });
        self.renaming = Some((st, sub));
        cx.notify();
    }

    /// Enter or a click away saves; Escape puts the title back.
    fn end_rename(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some((st, _)) = self.renaming.take() else { return };
        let text = st.read(cx).value().to_string();
        if save { self.rename_to(&text, cx); }
        cx.notify();
    }

    /// Rename: trimmed ("Terminal" when empty), shown at once, then told to
    /// the server; an unchanged title sends nothing (the web's renameTerminal).
    pub(crate) fn rename_to(&mut self, input: &str, cx: &mut Context<Self>) {
        let Some(path) = rename_path(&self.key) else { return };
        let server = (self.info)(cx).title;
        let Some(title) = rules::rename_value(input, self.shown_title(&server)) else { return };
        self.renamed = Some((title.clone(), server));
        cx.notify();
        crundi_shell::call(cx, move |api| api.post(&path, &serde_json::json!({ "title": title })), |_, _| {});
    }
}

/// A chat's dot by its state (the web's .term-dot): working yellow, needs
/// input accent, waiting sky, idle green, exited grey.
pub(crate) fn state_color(state: &str) -> Hsla {
    match state { "working" => T::yellow(), "needs-input" => T::accent(), "waiting" => T::sky(), "exited" => T::text_muted(), _ => T::green() }
}

impl PaneHead {
    /// The tools the web keeps in .term-head-tools: a chat's tags, its lock,
    /// the kind's buttons, A- ↺ A+ (a chat's), pin. `menu`: drawn in the
    /// "…" menu (the lock with its words, the buttons a little bigger).
    fn tools(&self, e: &HeadExtra, f: Frame, menu: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let id = f.leaf;
        let h = if menu { T::SP_20 + px(1.) } else { T::SP_20 };
        let p = if menu { "m-" } else { "" };
        let mut out: Vec<AnyElement> = vec![];
        for (i, t) in e.tags.iter().enumerate() {
            let action = t.action;
            out.push(w::head_tag(SharedString::from(format!("{p}tag-{id}-{i}")), t.text.clone(), t.look, t.tip.clone(), action.is_some())
                .when_some(action, |d, a| d.on_click(cx.listener(move |this, _, window, cx| this.run(a, window, cx))))
                .into_any_element());
        }
        if let Some(n) = e.secrets {
            let tip = rules::secrets_tip(n);
            let look = if n > 0 { L::On } else { L::Rest };
            let b = if menu {
                let label = if n > 0 { format!("{n} Secrets for this chat") } else { "Secrets for this chat".into() };
                w::head_label_button(SharedString::from(format!("m-lock-{id}")), IconName::Lock, label, look, tip)
            } else if n > 0 {
                w::head_button(("lock", id), C::IconText(IconName::Lock, n.to_string().into()), look, T::SP_24 + T::SP_12, h, tip)
            } else {
                w::head_button(("lock", id), C::Icon(IconName::Lock), look, T::SP_24, h, tip)
            };
            out.push(b.on_click(cx.listener(|this, _, window, cx| { this.menu = false; cx.notify(); this.run("chat-secrets", window, cx); })).into_any_element());
        }
        let kind_buttons = self.kind.as_ref().map(|k| k.buttons.clone()).unwrap_or_default();
        for b in kind_buttons.iter().chain(e.buttons.iter()) {
            let content = match b.icon { Some(i) => C::Icon(i), None => C::Text(b.text.clone()) };
            let a = b.action;
            out.push(w::head_button(SharedString::from(format!("{p}btn-{a}-{id}")), content, L::Rest, T::SP_24, h, b.tip.clone())
                .on_click(cx.listener(move |this, _, window, cx| this.run(a, window, cx))).into_any_element());
        }
        if self.chat {
            out.push(w::head_button(SharedString::from(format!("{p}a--{id}")), C::Text("A-".into()), L::Rest, T::SP_24, h, "Smaller text").on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_zoom(id, -1, cx)))).into_any_element());
            out.push(w::head_button(SharedString::from(format!("{p}a0-{id}")), C::Icon(IconName::RotateCcw), L::Rest, T::SP_24, h, "Reset text size").on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_zoom(id, 0, cx)))).into_any_element());
            out.push(w::head_button(SharedString::from(format!("{p}a+-{id}")), C::Text("A+".into()), L::Rest, T::SP_32, h, "Larger text").on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_zoom(id, 1, cx)))).into_any_element());
        }
        let pinned = f.pinned;
        out.push(w::head_button(SharedString::from(format!("{p}pin-{id}")), C::Icon(if pinned { IconName::PinOff } else { IconName::Pin }), if pinned { L::Pinned } else { L::Rest }, T::SP_24, h,
            if pinned { "Unpin (size and position are locked)" } else { "Pin size and position" }).on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_pin(id, cx)))).into_any_element());
        out
    }
}

impl Render for PaneHead {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let f = self.wb.upgrade().and_then(|wb| wb.read(cx).frame_of(&self.key)).unwrap_or_default();
        crundi_core::driver::rendered("head");
        let info = (self.info)(cx);
        let e = self.extra(cx);
        self.last = Some((info.clone(), e.clone()));
        // A rename shows until the server's title moves on.
        if self.renamed.as_ref().is_some_and(|(_, old)| *old != info.title) { self.renamed = None; }
        let id = f.leaf;
        let fold = self.chat && f.narrow;
        if !fold && self.menu { self.menu = false; }
        let shown = self.shown_title(&info.title).to_string();
        let (title, cap) = rules::title_shown(&shown, if self.renamed.is_some() { "" } else { &info.long }, f.w);
        let mut row = div().flex().items_center().size_full().pl(T::SP_2).pr(T::SP_4).gap(T::SP_4);
        if !f.full {
            let pend = self.key.starts_with("pend:");
            let grip = div().id(("grip", id)).flex_none().w(T::SP_24).h(T::SP_12).flex().items_center().justify_center().cursor_grab().text_color(T::text_muted())
                .when(pend, |d| d.opacity(0.25)).child(w::icon(IconName::GripVertical, T::ICON_SM));
            row = row.child(if pend { grip } else { grip.tooltip(w::tooltip("Drag to reorder")) });
        }
        if self.chat {
            row = row.child(div().flex_none().size(T::DOT).flex().items_center().justify_center()
                .when(info.state == "needs-input", |d| d.child(div().absolute().size(T::SP_12).rounded_full().bg(T::accent_dim())))
                .child(div().size(T::DOT).rounded(T::R_CHIP).bg(state_color(&info.state))));
        } else if let Some(icon) = self.kind.as_ref().and_then(|k| k.icon) {
            row = row.child(div().flex_none().flex().items_center().text_color(T::accent_hover()).child(w::icon(icon, T::ICON_MD)));
        }
        if let Some((st, _)) = &self.renaming {
            row = row.child(div().id(("rename", id)).flex_none()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .capture_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| if e.keystroke.key == "escape" { cx.stop_propagation(); this.end_rename(false, cx); }))
                .child(w::title_input(st)));
        } else {
            let t = div().id(("title", id)).flex_none().max_w(px(cap)).min_w_0().h(T::SP_16).px(T::SP_6).rounded(T::R_CONTROL)
                .flex().items_center().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text())
                .child(div().truncate().child(title));
            row = row.child(if rename_path(&self.key).is_some() {
                t.cursor_text().hover(|s| s.bg(T::bg_tertiary())).tooltip(w::tooltip("Click to rename"))
                    .on_click(cx.listener(|this, e: &ClickEvent, window, cx| {
                        // (Not the end of a drag that began on the title.)
                        let dragged = this.wb.upgrade().is_some_and(|wb| wb.read(cx).dragged());
                        if e.click_count() == 1 && !dragged { this.start_rename(window, cx); }
                    }))
            } else { t });
        }
        if self.chat {
            let badge = match info.state.as_str() { "working" => Some(("WORKING", "warn")), "needs-input" => Some(("NEEDS INPUT", "accent")), "waiting" => Some(("WAITING", "sky")), _ => None };
            if let Some((b, tone)) = badge { row = row.child(w::state_badge(b, tone)); }
        }
        row = row.child(div().flex_1().min_w_0());
        let h = T::SP_20;
        let mut t = div().flex().flex_none().items_center().gap(T::SP_4)
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| { this.act(cx, move |w, cx| w.focus_pane(id, cx)); cx.stop_propagation(); }));
        if !fold { t = t.children(self.tools(&e, f, false, cx)); }
        if self.chat {
            t = t.child(w::head_button(("full", id), C::Icon(IconName::Maximize), if f.full { L::Lit } else { L::Rest }, T::SP_24, h, "Full screen")
                .on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_full(id, cx)))));
        }
        if fold {
            let open = self.menu;
            t = t.child(w::head_button(("more", id), C::Icon(IconName::Ellipsis), if open { L::Lit } else { L::Rest }, T::SP_24, h, "More")
                .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    // The press that just closed the menu from outside was on "…": it stays closed.
                    let closing = this.closed_at.take() == Some(e.position);
                    if !closing { this.menu = !this.menu; cx.notify(); }
                })));
        }
        if !f.full {
            let tip = if f.armed { "Click again to close" } else if self.key.starts_with("term:") { "Close terminal" } else if self.chat { "Close chat" } else if self.key.starts_with("pend:") { "Remove" } else { "Close panel" };
            t = t.child(if f.armed {
                w::head_button(("x", id), C::Text("Sure?".into()), L::Armed, T::SP_40 + T::SP_4, T::SP_16, tip)
            } else {
                w::head_button(("x", id), C::Icon(IconName::X), L::Close, T::SP_24 + T::SP_4, T::SP_16, tip)
            }.on_click(cx.listener(move |this, _, _, cx| { this.menu = false; this.act(cx, move |w, cx| w.pane_close(id, cx)) })));
        }
        row = row.child(t);
        let hh = if f.narrow { T::PANE_HEAD_NARROW } else { T::PANE_HEAD };
        let mut head = div()
            .id(("head", id)).relative().size_full().bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                if this.renaming.is_some() { return; }
                if e.click_count == 2 { this.act(cx, move |w, cx| w.pane_maximize(id, cx)); return; }
                let (x, y) = (f32::from(e.position.x), f32::from(e.position.y));
                this.act(cx, move |w, cx| { w.focus_pane(id, cx); if !f.full { w.head_press(id, x, y, cx); } });
            }))
            .child(row);
        if fold && self.menu {
            // The folded tools under the header, right-aligned (the web's
            // .term-head-tools in a narrow pane): wrapped, 6 apart, padding 8.
            let menu = div().id(("pane-menu", id)).occlude().flex().flex_wrap().items_center().justify_end().gap(T::SP_6).p(T::SP_8)
                .max_w(px((f.w - 8.).max(0.)))
                .bg(T::bg_secondary()).border_1().border_color(T::border()).rounded(T::R_FIELD).shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(cx.listener(|this, e: &MouseDownEvent, _, cx| if this.menu { this.closed_at = Some(e.position); this.set_menu(false, cx); }))
                .children(self.tools(&e, f, true, cx));
            head = head.child(div().absolute().top(hh + T::SP_2).right(T::SP_4).size_0()
                .child(deferred(anchored().anchor(Anchor::TopRight).snap_to_window_with_margin(T::SP_4).child(menu)).with_priority(1)));
        }
        let _ = window;
        // One paint layer (w::layer): every pane's head is replayed each
        // frame. Not while renaming: the box's caret would go under its text.
        if self.renaming.is_some() { head.into_any_element() } else { w::layer(head).into_any_element() }
    }
}
