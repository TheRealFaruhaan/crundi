//! Drawing the workbench: the grid of panes at the mosaic's rectangles
//! (scrolling sideways when the panes need more than the room), the
//! gutters, an empty pane (its message, chips and buttons), a launcher, the
//! pane being dragged, the full-screen pane over the window, and the "+"
//! menu. The pointer's moves and releases are heard window-wide while a
//! gutter or a pane is dragged.

use crate::{kind_of, Info, Workbench};
use crundi_core::mosaic::Dir;
use crundi_theme::{IconName, T};
use crundi_widgets::{self as w, HeadContent as C, HeadLook as L};
use gpui::{prelude::FluentBuilder as _, *};

impl Workbench {
    fn pane_frame(&self, i: &Info, full: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let body: AnyElement = match kind_of(cx, &i.key) {
            Some(k) => (k.render)(&i.key, window, cx),
            None if i.kind == "launcher" => self.launcher(i, cx).into_any_element(),
            None => w::empty("Not built yet in the GPUI client.").into_any_element(),
        };
        let head = self.head(i, full, cx);
        // The body clipped to its box (a long log never draws over the header).
        let body = div().relative().size_full().overflow_hidden().child(body);
        w::panel(head, body).when(i.focused && !full, |d| d.border_color(T::accent())).into_any_element()
    }

    fn leaf(&mut self, i: &Info, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = i.l.id;
        let inner: AnyElement = if i.kind == "empty" { self.empty_leaf(i, cx).into_any_element() } else { self.pane_frame(i, false, window, cx) };
        let fh = self.leaf_focus.get(&id).cloned().unwrap_or_else(|| cx.focus_handle());
        div()
            .id(("leaf", id)).track_focus(&fh).absolute().left(px(i.l.x)).top(px(i.l.y)).w(px(i.l.w)).h(px(i.l.h))
            .on_hover(cx.listener(move |this, h: &bool, _, cx| { if *h { this.hover = id; } else if this.hover == id { this.hover = 0; } cx.notify(); }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.focus_pane(id, cx)))
            .child(inner)
            .when(i.drop, |d| d.child(div().absolute().inset_0().rounded(T::R_CARD).border_2().border_color(T::accent()).bg(T::accent_dim())))
            .when(i.menu && i.narrow, |d| d.child(self.narrow_menu(i, cx)))
            .into_any_element()
    }

    /// A leaf with nothing in it (.mosaic-empty): a dashed edge, the
    /// message, the chips, then Split right / Split down / remove, in a centred column.
    fn empty_leaf(&self, i: &Info, cx: &mut Context<Self>) -> Div {
        let id = i.l.id;
        let btn = |key: &'static str, icon: IconName, tip: &'static str| w::head_button((key, id), C::IconText(icon, "Split".into()), L::Rest, T::SP_48 + T::SP_24, T::CONTROL_SM, tip);
        div().size_full().p(T::SP_4).child(
            div().size_full().rounded(T::R_CARD).border_1().border_dashed().border_color(T::border()).overflow_hidden()
                .flex().flex_col().items_center().justify_center()
                .child(div().h(T::SP_16).flex().items_center().text_size(T::TEXT_13).text_color(T::text_muted()).child("Drop a panel here or launch a new one"))
                .child(div().h(T::SP_10))
                .child({
                    let mut rows = div().flex().flex_col().gap(T::SP_6).mb(T::SP_2);
                    for row in &i.chips {
                        let mut r = div().h(T::SP_24).flex().items_center().justify_center().gap(T::SP_6);
                        for (k, label) in row.iter().copied() {
                            let icon = match k { "terminal" => IconName::Terminal, "files" => IconName::Folder, "git" => IconName::GitBranch, "kanban" => IconName::Kanban, "notes" => IconName::FileText, "mindmap" => IconName::Network, "media" => IconName::Images, "factory" => IconName::Factory, _ => IconName::PanelsTopLeft };
                            r = r.child(div().id(SharedString::from(format!("chip-{k}-{id}"))).flex_none().w(px(crate::chip_w(label))).cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| this.chip(id, k, cx)))
                                .child(w::chip(icon, label, false)));
                        }
                        rows = rows.child(r);
                    }
                    rows
                })
                .child(div().h(T::SP_10))
                .child(div().flex().items_center().gap(T::SP_6)
                    .child(btn("e-right", IconName::Columns2, "Split right").on_click(cx.listener(move |this, _, _, cx| this.split(id, false, cx))))
                    .child(btn("e-down", IconName::Rows2, "Split down").on_click(cx.listener(move |this, _, _, cx| this.split(id, true, cx))))
                    .child(w::head_button(("e-x", id), C::Icon(IconName::X), L::Close, T::CONTROL_SM, T::CONTROL_SM, "Remove this pane")
                        .on_click(cx.listener(move |this, _, _, cx| this.close_pane(id, cx))))))
    }

    /// A new pane's launcher: Claude in UI Mode, or with Skip Permissions.
    /// The pressed one spins (here: says Starting…) until the chat is up.
    fn launcher(&self, i: &Info, cx: &mut Context<Self>) -> Div {
        let id = i.l.id;
        let busy = i.launch_busy;
        let mut row = div().flex().items_center().gap(T::SP_8);
        if busy != 0 {
            row = row.child(w::loading("Starting…"));
        } else {
            row = row.child(w::primary(("ui-mode", id), "UI Mode").on_click(cx.listener(move |this, _, _, cx| this.launch_chat(id, false, cx))))
                .child(w::icon_button(("ui-skip", id), IconName::ShieldAlert, "UI Mode — Skip Permissions").on_click(cx.listener(move |this, _, _, cx| this.launch_chat(id, true, cx))));
        }
        div().size_full().flex().flex_col().items_center().justify_center().gap(T::SP_12)
            .child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child("Start Claude in this pane"))
            .child(row)
    }
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.window.is_none() { self.window = Some(window.window_handle()); }
        let vs = window.viewport_size();
        self.win_size = (f32::from(vs.width), f32::from(vs.height));
        let me = cx.entity().downgrade();
        let infos = self.infos(cx);
        let (gw, gh) = self.grid_size();
        let gutters = self.mosaic().layout(gw, gh).1;
        let full = self.full.clone();
        // Typing in a pane brings it into view: the grid scrolls sideways to
        // the focused pane (Slint's Flickable does it for its focused input).
        if self.reveal {
            self.reveal = false;
            if let Some(i) = infos.iter().find(|i| i.l.id == self.focused) {
                let aw = self.area.0;
                let at = -f32::from(self.scroll.offset().x);
                let pad = crundi_core::mosaic::PAD;
                // Right of the room: only as far as its inside (Slint brought the
                // focused input into view, not the pane's edge).
                let inset = f32::from(T::SP_10);
                let to = if i.l.x < at { Some(i.l.x - pad) } else if i.l.x + i.l.w - inset > at + aw { Some(i.l.x + i.l.w - inset - aw) } else { None };
                if let Some(to) = to { self.scroll.set_offset(point(px(-to.max(0.)), px(0.))); }
            }
        }
        let mut grid = div().relative().w(px(gw)).h(px(gh)).flex_none();
        for i in &infos { if !self.leaf_focus.contains_key(&i.l.id) { let h = cx.focus_handle(); self.leaf_focus.insert(i.l.id, h); } }
        self.leaf_focus.retain(|l, _| infos.iter().any(|i| i.l.id == *l));
        for i in &infos {
            if full.as_deref() == Some(i.key.as_str()) { continue; }
            grid = grid.child(self.leaf(i, window, cx));
        }
        for (n, g) in gutters.into_iter().enumerate() {
            let (split, index, row, span, locked) = (g.split, g.index, g.dir == Dir::Row, g.span, g.locked);
            grid = grid.child(div().id(("gutter", n)).absolute().left(px(g.x)).top(px(g.y)).w(px(g.w)).h(px(g.h))
                .when(!locked, |d| d.when(row, |d| d.cursor_col_resize()).when(!row, |d| d.cursor_row_resize()).hover(|s| s.bg(T::accent_edge())))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if locked { return; }
                    if e.click_count == 2 { this.gutter_even(split, index, cx); } else { this.gutter_press(split, index, row, span, f32::from(e.position.x), f32::from(e.position.y)); }
                })));
        }
        let me2 = me.clone();
        let mut root = div().id("workbench").relative().size_full()
            .child(canvas(
                move |bounds, _, cx| {
                    let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let o = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
                    let _ = me.update(cx, |this, cx| { this.origin = o; if this.area != (bw, bh) { this.area = (bw, bh); cx.notify(); } });
                },
                move |_, _, window, _| {
                    let m = me2.clone();
                    window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| if phase == DispatchPhase::Bubble {
                        let _ = m.update(cx, |this, cx| if this.gutter.is_some() || this.press.is_some() || this.sbar.is_some() { this.pointer_move(f32::from(e.position.x), f32::from(e.position.y), cx) });
                    });
                    let m = me2.clone();
                    window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| if phase == DispatchPhase::Bubble {
                        let _ = m.update(cx, |this, cx| if this.gutter.is_some() || this.press.is_some() || this.drag.is_some() || this.sbar.is_some() { this.pointer_up(f32::from(e.position.x), f32::from(e.position.y), cx) });
                    });
                },
            ).absolute().size_full())
            .child(div().id("wb-grid").size_full().overflow_x_scroll().track_scroll(&self.scroll).child(grid));
        // Wider than the room (panes at their 340px least): the row scrolls
        // sideways, with its scrollbar under the panes (the web's grid).
        let aw = self.area.0;
        if gw > aw + 0.5 && aw > 0. {
            let at = -f32::from(self.scroll.offset().x);
            let (tw, tx) = (aw * aw / gw, at * aw / gw);
            root = root.child(div().id("wb-scrollbar").absolute().left_0().bottom_0().w(px(aw)).h(T::SP_6)
                .child(div().id("wb-thumb").absolute().top_0().left(px(tx)).w(px(tw)).h_full().rounded(T::R_PILL).bg(T::border()).hover(|s| s.bg(T::text_muted()))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| { cx.stop_propagation(); this.sbar = Some((f32::from(e.position.x), at)); }))));
        }
        // The pane being dragged follows the pointer: its header's frame.
        if let Some(d) = self.drag {
            if let Some(i) = infos.iter().find(|i| i.l.id == d.src) {
                root = root.child(deferred(anchored().position(point(px(d.x - d.dx), px(d.y - d.dy))).child(
                    div().w(px(i.l.w)).h(T::PANE_HEAD).px(T::SP_10).flex().items_center().rounded(T::R_CARD).border_1().border_color(T::accent())
                        .bg(T::bg_tertiary()).text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(i.title.clone()))).with_priority(2));
            }
        }
        // Full screen: the pane over the whole window.
        if let Some(i) = full.as_ref().and_then(|k| infos.iter().find(|i| &i.key == k)).cloned() {
            let i = self.as_full(i, cx);
            let frame = self.pane_frame(&i, true, window, cx);
            root = root.child(deferred(anchored().position(point(px(0.), px(0.))).child(
                div().occlude().w(vs.width).h(vs.height).bg(T::bg()).child(frame))).with_priority(1));
        }
        if self.add_menu { root = root.child(self.add_menu_view(cx)); }
        root
    }
}
