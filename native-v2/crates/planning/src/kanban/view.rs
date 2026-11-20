//! Kanban's screen (the web's #kanban-panel; native-gpui's kanban_view.rs):
//! the toolbar (+ Add task; Board, Trash (n), History, Media), the board's
//! columns and their cards, the Trash and History views, and the layers over
//! it (the dragged card's clone, a card's attachments strip).
//!
//! How it is built (ARCHITECTURE.md rules 3 and 4): a host (the tab, or a
//! workbench pane) has its own views, side by side, each a cached leaf:
//! - `Msg`: the muted line (no project, loading, error); always placed (a
//!   zero box while the board shows), so its notify re-lays the host out
//!   when the board's shape changes (its state, view, columns, width);
//! - `Toolbar`;
//! - one `ColumnView` per column: its head and a virtualised list of its
//!   cards, cached at the height it needs (the web's column shrinks to its
//!   cards, up to the board's height); a change splices only the cards
//!   that changed, a scroll draws only the cards on screen;
//! - `StackView`: the narrow board (the web's @container 700px): every
//!   column's head, cards and foot as rows of one virtualised list;
//! - `TrashView`, `HistView`: their lists;
//! - `Overlay`: the dragged card's clone and the attachments strip, redrawn
//!   on a pointer move without touching the columns.

use super::{act, board::{self, Box4, Card, Col}, Drag, KStore};
use crate::util::eid;
use crundi_core::driver::rendered;
use crundi_shell::Stores;
use crundi_theme::T;
use crundi_widgets::{self as w, IconName};
use gpui::{prelude::FluentBuilder as _, *};
use std::{cell::{Cell, RefCell}, rc::Rc};

fn b4(b: Bounds<Pixels>) -> Box4 {
    Box4 { x: f32::from(b.origin.x), y: f32::from(b.origin.y), w: f32::from(b.size.width), h: f32::from(b.size.height) }
}

/// Splice a list for what changed between two item vectors (the common
/// head and tail kept, measured as they were).
fn splice_diff<I: PartialEq>(list: &ListState, old: &[I], new: &[I]) -> bool {
    let pre = old.iter().zip(new.iter()).take_while(|(a, b)| a == b).count();
    if pre == old.len() && pre == new.len() { return false; }
    let max_suf = old.len().min(new.len()) - pre;
    let suf = old.iter().rev().zip(new.iter().rev()).take(max_suf).take_while(|(a, b)| a == b).count();
    list.splice(pre..old.len() - suf, new.len() - pre - suf);
    true
}

/// The board's geometry at a width: toolbar folded, columns stacked, a
/// column's and a card's width.
#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) struct Geom { pub fold: bool, pub stacked: bool, pub col_w: Pixels, pub card_w: Pixels, pub width: Pixels }

fn geom(width: Pixels, ncols: usize) -> Geom {
    let n = ncols.max(1) as f32;
    let pad = f32::from(T::SP_12);
    let stacked = width < T::KANBAN_STACK;
    let col_w = px(((f32::from(width) - 2. * pad - (n - 1.) * pad) / n).clamp(f32::from(T::KANBAN_COL_MIN), f32::from(T::KANBAN_COL_MAX))).round();
    let card_w = if stacked { width - T::SP_12 * 2. - T::SP_8 * 2. } else { col_w - T::SP_8 * 2. };
    Geom { fold: width < T::KANBAN_FOLD, stacked, col_w, card_w, width }
}

// ─── The host ───

/// One place the board shows (the tab, or a pane): its views. Cheap to clone.
#[derive(Clone)]
pub struct Host {
    store: Entity<KStore>,
    msg: Entity<Msg>,
    toolbar: Entity<Toolbar>,
    cols: Rc<RefCell<Vec<Entity<ColumnView>>>>,
    stack: Entity<StackView>,
    trash: Entity<TrashView>,
    history: Entity<HistView>,
    overlay: Entity<Overlay>,
    width: Rc<Cell<f32>>,
    hscroll: ScrollHandle,
}

impl Host {
    pub fn new(_stores: &Stores, embedded: bool, _window: &mut Window, cx: &mut App) -> Self {
        let store = super::store(cx);
        let msg = cx.new(|cx| Msg::new(store.clone(), cx));
        let toolbar = cx.new(|cx| Toolbar::new(store.clone(), cx));
        let stack = cx.new(|cx| StackView::new(store.clone(), cx));
        let trash = cx.new(|cx| TrashView::new(store.clone(), cx));
        let history = cx.new(|cx| HistView::new(store.clone(), cx));
        let overlay = cx.new(|cx| Overlay::new(store.clone(), embedded, cx));
        Host { store, msg, toolbar, cols: Rc::default(), stack, trash, history, overlay, width: Rc::new(Cell::new(0.)), hscroll: ScrollHandle::new() }
    }

    /// The host's width changed (measured as it was laid out): each view
    /// takes its geometry, and notifies only if its own changed.
    fn width_changed(&self, cx: &mut App) {
        let n = self.msg.read(cx).shape.ncols;
        let g = geom(px(self.width.get()), n);
        self.msg.update(cx, |v, cx| v.set_geom(g, cx));
        self.toolbar.update(cx, |v, cx| v.set_fold(g.fold, cx));
        for c in self.cols.borrow().iter() { c.update(cx, |v, cx| v.set_geom(g, cx)); }
        self.stack.update(cx, |v, cx| v.set_geom(g, cx));
    }

    /// The host's body: layout only, placing the cached views (built in the
    /// tab's render, or in the Workbench's as the pane's body).
    pub fn body(&self, cx: &mut App) -> AnyElement {
        // What it lays out, from the Msg view's copy (set by its observer,
        // notified only when the shape changed): never the store itself, so a
        // store notify that changes no shape costs this layout view nothing (F57).
        let (state, view, n) = { let m = &self.msg.read(cx).shape; (m.state.clone(), m.view.clone(), m.ncols) };
        // One view per column (made the first time the board has it).
        let made = self.cols.borrow().len();
        if made < n {
            let g = geom(px(self.width.get()), n);
            for i in made..n {
                let st = self.store.clone();
                let v = cx.new(|cx| ColumnView::new(st, i, g, cx));
                self.cols.borrow_mut().push(v);
            }
        }
        // The columns' and the stack's widths follow the column count too
        // (each notifies only if its own changed).
        let g = geom(px(self.width.get()), n);
        for c in self.cols.borrow().iter() { c.update(cx, |v, cx| v.set_geom(g, cx)); }
        self.stack.update(cx, |v, cx| v.set_geom(g, cx));
        let me = self.clone();
        let measure = canvas(move |b, _, cx| {
            let w = f32::from(b.size.width);
            if (me.width.get() - w).abs() > 0.5 { me.width.set(w); let me = me.clone(); cx.defer(move |cx| me.width_changed(cx)); }
        }, |_, _, _, _| {}).absolute().top_0().left_0().size_full();
        let mut root = div().relative().size_full().flex().flex_col().min_h_0().bg(T::bg()).child(measure);
        if state != "ok" || self.width.get() <= 0. {
            root = root.child(self.msg.clone().cached(StyleRefinement::default().size_full()));
        } else {
            // (Placed at no size: its notify is the host's re-layout.)
            root = root.child(self.msg.clone().cached(StyleRefinement::default().absolute().top_0().left_0().size_0()));
            let th = self.toolbar.read(cx).height;
            root = root.child(self.toolbar.clone().cached(StyleRefinement::default().w_full().h(th).flex_none()));
            let fill = StyleRefinement::default().w_full().flex_1().min_h_0();
            root = root.child(match view.as_str() {
                "trash" => self.trash.clone().cached(fill).into_any_element(),
                "history" => self.history.clone().cached(fill).into_any_element(),
                _ if g.stacked => self.stack.clone().cached(fill).into_any_element(),
                _ => {
                    let cols = self.cols.borrow();
                    div().id("kanban-board").w_full().flex_1().min_h_0().overflow_x_scroll().track_scroll(&self.hscroll)
                        .p(T::SP_12).flex().items_start().gap(T::SP_12)
                        .children(cols.iter().take(n).map(|c| {
                            let h = c.read(cx).needed;
                            c.clone().cached(StyleRefinement::default().w(g.col_w).h(h).max_h_full().min_h_0().flex_none())
                        }))
                        .into_any_element()
                }
            });
        }
        root.child(self.overlay.clone().cached(StyleRefinement::default().absolute().top_0().left_0().size_full())).into_any_element()
    }
}

/// The Kanban tab's view: layout only (its body's views are the leaves).
pub struct Tab { host: Host }
impl Tab { pub fn new(host: Host, _: &mut Context<Self>) -> Self { Tab { host } } }
impl Render for Tab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().id("kanban-tab").size_full().child(self.host.body(cx))
    }
}

// ─── The muted line, and the host's re-layout ───

/// What the host's layout depends on.
#[derive(Clone, PartialEq, Default)]
struct Shape { state: String, error: String, view: String, ncols: usize, geom: Geom }

pub struct Msg { store: Entity<KStore>, width: Pixels, shape: Shape, _sub: Subscription }

impl Msg {
    fn new(store: Entity<KStore>, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&store, |this: &mut Self, _, cx| this.sync(cx));
        let mut m = Msg { store, width: px(0.), shape: Shape::default(), _sub: sub };
        m.shape = m.read(cx);
        m
    }
    fn read(&self, cx: &App) -> Shape {
        let k = self.store.read(cx);
        Shape { state: k.state.clone(), error: k.error.clone(), view: k.view.clone(), ncols: k.cols.len(), geom: geom(self.width, k.cols.len()) }
    }
    fn sync(&mut self, cx: &mut Context<Self>) { let s = self.read(cx); if s != self.shape { self.shape = s; cx.notify(); } }
    fn set_geom(&mut self, g: Geom, cx: &mut Context<Self>) { self.width = g.width; self.sync(cx); }
}

/// The muted centred line (.kanban-empty).
fn muted(text: impl Into<SharedString>) -> Div {
    div().w_full().p(T::SP_24).flex().justify_center().text_size(T::TEXT_13).text_color(T::text_muted()).child(text.into())
}

impl Render for Msg {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        rendered("kanban-msg");
        match self.shape.state.as_str() {
            "none" => muted("Select a project to use its Kanban board.").into_any_element(),
            "error" => muted(self.shape.error.clone()).into_any_element(),
            // Loading: nothing yet (the web's renderKanban waits for the board).
            _ => div().into_any_element(),
        }
    }
}

// ─── The toolbar ───

pub struct Toolbar { store: Entity<KStore>, view: String, trash: usize, fold: bool, pub height: Pixels, _sub: Subscription }

impl Toolbar {
    fn new(store: Entity<KStore>, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&store, |this: &mut Self, s, cx| {
            let k = s.read(cx);
            let (v, n) = (k.view.clone(), k.trash.0.len());
            if v != this.view || n != this.trash { this.view = v; this.trash = n; cx.notify(); }
        });
        Toolbar { store, view: "board".into(), trash: 0, fold: false, height: T::BTN_H + T::SP_10 * 2. + px(1.), _sub: sub }
    }
    fn set_fold(&mut self, f: bool, cx: &mut Context<Self>) { if f != self.fold { self.fold = f; cx.notify(); } }
}

impl Render for Toolbar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("kanban-toolbar");
        let _ = &self.store;
        let (fold, view) = (self.fold, self.view.clone());
        let vb = |id: &'static str, label: String, on: bool| w::toggle(id, label, on).on_click(move |_, window, cx| act(id, "", "", window, cx));
        let media = if fold { w::icon_button("kanban-media", IconName::Images, "Media") } else { w::button("kanban-media", "Media").icon(IconName::Images) };
        let me = cx.entity().downgrade();
        let bar = div().flex().flex_none().items_center()
            .gap(if fold { T::SP_4 } else { T::SP_8 })
            .px(if fold { T::SP_8 } else { T::SP_12 }).py(if fold { T::SP_6 } else { T::SP_10 })
            .border_b_1().border_color(T::border())
            .child(w::primary("kanban-add", "+ Add task").on_click(|_, window, cx| act("add-task", "", "", window, cx)))
            .child(div().flex_1())
            .child(vb("view-board", "Board".into(), view == "board"))
            .child(vb("view-trash", format!("Trash ({})", self.trash), view == "trash"))
            .child(vb("view-history", "History".into(), view == "history"))
            .child(media.on_click(|_, window, cx| act("media", "", "", window, cx)))
            // Cached at the height it laid out at (a fold changes it).
            .child(canvas(move |b, _, cx| {
                let h = b.size.height;
                if let Some(me) = me.upgrade() { if (me.read(cx).height - h).abs() > px(0.5) { cx.defer(move |cx| me.update(cx, |t, cx| { t.height = h; cx.notify(); })); } }
            }, |_, _, _, _| {}).absolute().top_0().left_0().size_full());
        // One paint layer (w::layer): the bar is replayed every frame.
        w::layer(div().w_full().child(bar.relative()))
    }
}

// ─── A card ───

/// One row of a column's list: a card (with the drop line before it), or
/// the column's foot ("+ Add task", the blank card; the end's drop line).
#[derive(Clone, PartialEq)]
pub(crate) enum Item {
    Card { status: String, card: Rc<Card>, dragged: bool, clip: bool, line: bool },
    Foot { status: String, adding: bool, line: bool, other: bool },
}

/// A column's items, from its cards and the drag.
fn items_of(c: &Col, drag: &Drag, clip: Option<&str>) -> Vec<Item> {
    let dragging = drag.drag.as_ref();
    let here = dragging.is_some() && !c.other && drag.target.as_ref().is_some_and(|t| t.0 == c.status);
    let before = if here { drag.target.as_ref().map(|t| t.2.clone()).unwrap_or_default() } else { String::new() };
    let mut v: Vec<Item> = c.cards.iter().map(|cd| Item::Card {
        status: c.status.clone(), card: Rc::new(cd.clone()),
        dragged: dragging.is_some_and(|d| d.id == cd.id), clip: clip == Some(cd.id.as_str()), line: here && before == cd.id,
    }).collect();
    v.push(Item::Foot { status: c.status.clone(), adding: c.adding, line: here && before.is_empty(), other: c.other });
    v
}

/// The inputs the items draw (the blank card's, the new subtask's).
#[derive(Clone)]
struct Inputs { task: Option<Entity<w::InputState>>, todo: Option<Entity<w::InputState>>, boxes: Rc<RefCell<super::Boxes>>, cards: super::card_el::Cache }

fn drop_line() -> Div { div().flex_none().h(T::SP_2).rounded(T::R_PILL).bg(T::accent()) }

/// An item's elements (its top gap is the column body's gap or padding).
fn item_el(it: &Item, card_w: Pixels, inp: &Inputs, live: bool, cx: &App) -> AnyElement {
    match it {
        Item::Card { status, card, dragged, clip, line } => div().w_full().flex().flex_col().gap(T::SP_8).px(T::SP_8).pt(T::SP_8)
            .when(*line, |d| d.child(drop_line()))
            // At rest: one element, laid out once (card_el.rs); the card under
            // the pointer, or in use, the live controls.
            .map(|d| if live || card.menu_open || card.adding { d.child(card_el(card, status, *dragged, *clip, card_w, inp, true, cx)) } else {
                d.child(super::card_el::CardEl { key: super::card_el::Key { card: (**card).clone(), dragged: *dragged, clip: *clip, width: px(0.) }, cache: inp.cards.clone(), status: status.clone(), boxes: inp.boxes.clone() })
            })
            .into_any_element(),
        Item::Foot { status, adding, line, other } => div().w_full().flex().flex_col().gap(T::SP_8).px(T::SP_8).pt(T::SP_8).pb(T::SP_8)
            .when(*line, |d| d.child(drop_line()))
            .when(!*other, |d| d.child(add_task(status, *adding, inp)))
            .when(*other && !*line, |d| d.pt_0())
            .into_any_element(),
    }
}

/// The end of a column: "+ Add task", or the blank card it opens.
fn add_task(status: &str, adding: bool, inp: &Inputs) -> AnyElement {
    let s = status.to_string();
    if !adding {
        return w::dashed_button(SharedString::from(format!("kanban-add-{status}")), "+ Add task")
            .on_click(move |_, window, cx| act("add-task-inline", "", &s, window, cx)).into_any_element();
    }
    let Some(i) = inp.task.clone() else { return div().into_any_element() };
    div().p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::accent_edge()).bg(T::bg_card())
        .capture_key_down(|e, window, cx| if e.keystroke.key == "escape" { super::cancel_line(false, window, cx); })
        .child(w::input(&i)).into_any_element()
}

/// Where a control was painted (its pop opens against it: the web's placePop).
fn report(boxes: &Rc<RefCell<super::Boxes>>, task: &str, ctl: String) -> impl IntoElement {
    let (boxes, task) = (boxes.clone(), task.to_string());
    canvas(move |b, _, _| boxes.borrow_mut().controls.push((task, ctl, b4(b))), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
}


/// The live card (the one under the pointer, or in use; one at rest is
/// card_el.rs's element).
#[allow(clippy::too_many_arguments)]
fn card_el(cd: &Card, status: &str, dragged: bool, clip_open: bool, card_w: Pixels, inp: &Inputs, _live: bool, _cx: &App) -> AnyElement {
    let fold = card_w < if cd.compact { T::KANBAN_CARD_FOLD_COMPACT } else { T::KANBAN_CARD_FOLD };
    let id = cd.id.clone();
    let sid = SharedString::from(cd.id.clone());
    let a = |name: &'static str, id: String| move |_: &ClickEvent, window: &mut Window, cx: &mut App| act(name, &id, "", window, cx);

    // The title row: title, the mind map count, the paperclip, the menu (compact).
    let mut title = div().flex().items_start().gap(T::SP_8)
        .child(div().flex_1().min_w_0().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(cd.title.clone()));
    if cd.mm > 0 {
        title = title.child(div().id(eid("kanban-mm", &sid)).flex_none().mt(T::SP_2).cursor_pointer()
            .child(w::icon_tag(IconName::Share2, cd.mm.to_string(), "accent"))
            .tooltip(w::tooltip(format!("{} linked mindmap idea(s)", cd.mm)))
            .on_click(a("goto-mindmap", id.clone())));
    }
    let tid = id.clone();
    let clip_btn = if cd.media > 0 { w::toggle(eid("kanban-clip", &sid), cd.media.to_string(), clip_open).icon(IconName::Paperclip).tooltip("Attachments") }
        else { w::icon_toggle(eid("kanban-clip", &sid), IconName::Paperclip, "Attachments", clip_open) };
    title = title.child(div().relative().flex_none().child(clip_btn.on_click(move |_, _, cx| {
        let b = super::control_box(&tid, "clip", cx).unwrap_or_default();
        super::clip_open(&tid, b, f32::from(card_w), cx);
    })).child(report(&inp.boxes, &id, "clip".into())));
    if cd.compact { title = title.child(w::icon_toggle(eid("kanban-menu-c", &sid), IconName::Menu, "Task actions", cd.menu_open).on_click(a("card-menu", id.clone()))); }

    let mut col = div().flex().flex_col().px(T::SP_12).pt(T::SP_12).pb(T::SP_10).child(title);
    if !cd.desc.is_empty() { col = col.child(div().w_full().min_w_0().text_size(T::TEXT_12).text_color(T::text_secondary()).child(cd.desc.clone())); }

    // Subtasks, the new-subtask row, and the progress line.
    if cd.total > 0 || cd.adding {
        let mut list = div().flex().flex_col().pt(T::SP_8).gap(px(1.));
        for td in cd.todos.iter() { list = list.child(todo_row(&cd.id, td, card_w, &inp.boxes)); }
        if cd.adding {
            if let Some(i) = inp.todo.clone() {
                list = list.child(div().flex().items_center().gap(T::SP_8).pl(T::SP_6).pr(T::SP_4).py(T::SP_4).rounded(T::R_CONTROL)
                    .bg(T::bg_tertiary()).border_1().border_color(T::accent_edge())
                    .capture_key_down(|e, window, cx| if e.keystroke.key == "escape" { super::cancel_line(true, window, cx); })
                    .child(w::check(eid("kanban-ghost", &sid), "", false, |_, _, _| {}))
                    .child(div().flex_1().child(w::input(&i))));
            }
        }
        col = col.child(list);
    }
    if cd.total > 0 {
        let frac = cd.done as f32 / cd.total.max(1) as f32;
        col = col.child(div().flex().items_center().gap(T::SP_8).pt(T::SP_8)
            .child(div().flex_1().h(T::SP_2 + px(1.)).rounded(T::R_PILL).bg(T::bg_tertiary()).overflow_hidden()
                .child(div().h_full().w(relative(frac)).rounded(T::R_PILL).bg(if cd.done == cd.total { T::green() } else { T::accent() })))
            .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(format!("{} of {}", cd.done, cd.total))));
    }
    // The footer: the menu, then + Subtask (not on a compact card).
    if !cd.compact {
        col = col.child(div().flex().items_center().gap(T::SP_6).pt(T::SP_10)
            .child(w::icon_toggle(eid("kanban-menu", &sid), IconName::Menu, "Task actions", cd.menu_open).on_click(a("card-menu", id.clone())))
            .child(w::button(eid("kanban-sub", &sid), "Subtask").icon(IconName::Plus).tooltip("Add a subtask").on_click(a("add-subtask", id.clone()))));
    }
    // The card's actions, under its menu.
    if cd.menu_open {
        let edit = if fold { w::icon_button(eid("kanban-edit", &sid), IconName::Pencil, "Edit task") } else { w::button(eid("kanban-edit", &sid), "Edit").icon(IconName::Pencil).tooltip("Edit task") };
        let del = if fold { w::icon_button(eid("kanban-del", &sid), IconName::Trash, "Delete task") } else { w::ghost(eid("kanban-del", &sid), "Delete").icon(IconName::Trash).tooltip("Delete task") };
        col = col.child(div().flex().flex_col().gap(T::SP_8).pt(T::SP_8).child(w::rule())
            .child(div().flex().items_center().gap(T::SP_6)
                .when(cd.compact, |d| d.child(w::button(eid("kanban-sub2", &sid), "Subtask").icon(IconName::Plus).tooltip("Add a subtask").on_click(a("add-subtask", id.clone()))))
                .child(w::icon_button(eid("kanban-brain", &sid), IconName::Share2, "Brainstorm this task in the Mindmap").on_click(a("brainstorm", id.clone())))
                .child(edit.on_click(a("edit-task", id.clone())))
                .child(div().flex_1())
                .child(del.on_click(a("del-task", id.clone())))));
    }
    let title_text = cd.title.clone();
    let (st_, id2, boxes, b2) = (status.to_string(), cd.id.clone(), inp.boxes.clone(), inp.boxes.clone());
    div().id(eid("kanban-card", &sid)).relative().flex_none().rounded(T::R_CARD).border_1().bg(T::bg_card())
        // Dropped on a chat's box or a terminal: its task id goes in (the
        // shared drag layer; on the board the drag below moves it).
        .on_drag(w::RefDrag::new("kanban-task", &cd.id, cd.title.clone()), |_, _, _, cx| cx.new(|_| crate::util::NoGhost))
        .border_color(T::border()).hover(|d| d.border_color(T::kanban_slate_edge()))
        .when(dragged, |d| d.opacity(T::KANBAN_O_DIM))
        .cursor(if dragged { CursorStyle::ClosedHand } else { CursorStyle::OpenHand })
        // Its box, as painted (the drop target, a press's grab).
        .child(canvas(move |b, _, _| boxes.borrow_mut().cards.push((st_, id2, b4(b))), |_, _, _, _| {}).absolute().top_0().left_0().size_full())
        // A press anywhere arms a drag; past a few px it moves (mod.rs pointer_move).
        .on_mouse_down(MouseButton::Left, move |e, _, cx| {
            // A press on a subtask is the subtask's own drag (the web's interactiveSelector).
            if std::mem::take(&mut b2.borrow_mut().sub_press) { return; }
            super::press(&id, &title_text, e.position, cx)
        })
        .child(col)
        .into_any_element()
}

/// One subtask row (.kanban-todo): checkbox, text, media count, "…". It is
/// a drag source of its own (a subtask id dropped on a chat or terminal).
fn todo_row(task: &str, td: &board::Todo, card_w: Pixels, boxes: &Rc<RefCell<super::Boxes>>) -> AnyElement {
    let key = SharedString::from(format!("{task}-{}", td.id));
    let (t1, d1, done) = (task.to_string(), td.id.clone(), td.done);
    let (t2, d2) = (task.to_string(), td.id.clone());
    let (text, b1) = (td.text.clone(), boxes.clone());
    let row = div().id(eid("kanban-todo", &key)).group("kanban-todo").flex().items_start().gap(T::SP_8).pl(T::SP_6).pr(T::SP_4).py(T::SP_4)
        .rounded(T::R_CONTROL).hover(|d| d.bg(T::bg_hover()))
        .tooltip(w::tooltip("Drag onto a terminal to insert this subtask id"))
        .on_mouse_down(MouseButton::Left, move |_, _, _| b1.borrow_mut().sub_press = true)
        .child(div().pt(T::SP_2).child(w::check(eid("kanban-tick", &key), "", done, move |v, _, cx| super::toggle_todo(&t1, &d1, *v, cx))))
        .child(div().flex_1().min_w_0().text_size(T::TEXT_13).text_color(if done { T::text_muted() } else { T::text() }).child(td.text.clone()))
        .when(td.media > 0, |d| d.child(w::button(eid("kanban-tmedia", &key), td.media.to_string()).icon(IconName::Paperclip).tooltip("Subtask media").on_click({ let (t, d) = (task.to_string(), td.id.clone()); move |_, _, cx| super::media_todo(&t, &d, cx) })))
        .child(div().relative().opacity(T::KANBAN_O_CARET).group_hover("kanban-todo", |d| d.opacity(1.))
            .child(w::icon_button(eid("kanban-tmore", &key), IconName::Ellipsis, "Subtask actions").on_click(move |_, _, cx| {
                let b = super::control_box(&t2, &format!("more:{d2}"), cx).unwrap_or_default();
                super::todo_menu(&t2, &d2, b, cx);
            }))
            .child(report(boxes, task, format!("more:{}", td.id))));
    // The ghost: the row itself (the web's drag-clone of it).
    let ghost_w = card_w - T::SP_12 * 2. + T::SP_6 * 2.;
    w::drag_ref_row(row, "kanban-subtask", &td.id, text.clone(), ghost_w, move || {
        div().flex().items_start().gap(T::SP_8).pl(T::SP_6).pr(T::SP_4).py(T::SP_4).rounded(T::R_CONTROL).bg(T::bg_card()).border_1().border_color(T::border())
            .child(div().pt(T::SP_2).child(w::check("kanban-ghost-tick", "", done, |_, _, _| {})))
            .child(div().flex_1().min_w_0().text_size(T::TEXT_13).text_color(if done { T::text_muted() } else { T::text() }).child(text.clone()))
            .into_any_element()
    }).into_any_element()
}

/// A column's head (.kanban-col-head): its label and count.
fn col_head(c: &Col) -> Div {
    div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_12).pt(T::SP_10).pb(T::SP_8)
        .child(div().flex_1().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(if c.other { T::yellow() } else { T::text() }).child(c.label.clone()))
        .child(w::count_pill(c.cards.len().to_string()))
}

/// A column's edge: the Other column's yellow (a column under a drag is not
/// marked: the web shows the drop line alone).
fn edge(_drop_here: bool, other: bool) -> Hsla { if other { T::yellow() } else { T::border() } }

fn inputs(store: &Entity<KStore>, cards: &super::card_el::Cache, cx: &App) -> Inputs {
    let k = store.read(cx);
    Inputs { task: k.task_input.clone(), todo: k.todo_input.clone(), boxes: k.boxes.clone(), cards: cards.clone() }
}

// ─── A column (the wide board) ───

pub struct ColumnView {
    store: Entity<KStore>,
    idx: usize,
    col: Option<Rc<Col>>,
    items: Rc<Vec<Item>>,
    list: ListState,
    geom: Geom,
    drop_here: bool,
    /// The item under the pointer (its card has the live controls).
    hot: Option<usize>,
    /// The scroll as last drawn (a scroll lets go of the live card).
    last_top: Option<ListOffset>,
    all_live: bool,
    /// Its cards at rest, laid out (card_el.rs).
    cards: super::card_el::Cache,
    /// The height it needs (head and cards; the board caps it), as last laid out.
    pub needed: Pixels,
    _subs: Vec<Subscription>,
}

impl ColumnView {
    fn new(store: Entity<KStore>, idx: usize, geom: Geom, cx: &mut Context<Self>) -> Self {
        let drag = store.read(cx).drag.clone();
        let subs = vec![
            cx.observe(&store, |this: &mut Self, _, cx| if this.sync(cx) { cx.notify() }),
            cx.observe(&drag, |this: &mut Self, _, cx| if this.sync(cx) { cx.notify() }),
        ];
        let mut v = ColumnView { store, idx, col: None, items: Rc::new(vec![]), list: ListState::new(0, ListAlignment::Top, T::SP_48 * 4.).measure_all(), geom, drop_here: false, hot: None, last_top: None, all_live: false, cards: Rc::default(), needed: px(4000.), _subs: subs };
        v.sync(cx);
        v
    }

    /// Take the store's column and the drag; splice what changed.
    fn sync(&mut self, cx: &App) -> bool {
        let k = self.store.read(cx);
        let d = k.drag.read(cx);
        let col = k.cols.get(self.idx).cloned();
        let Some(c) = col.as_ref() else { let had = self.col.take().is_some(); return had };
        let items = items_of(c, d, k.clip.as_ref().map(|c| c.task.as_str()));
        let drop_here = d.drag.is_some() && !c.other && d.target.as_ref().is_some_and(|t| t.0 == c.status);
        let head_changed = self.col.as_ref().is_none_or(|o| o.label != c.label || o.cards.len() != c.cards.len() || o.other != c.other || o.status != c.status);
        let spliced = splice_diff(&self.list, &self.items, &items);
        let changed = spliced || head_changed || drop_here != self.drop_here || k.all_live != self.all_live;
        self.all_live = k.all_live;
        self.col = col;
        self.items = Rc::new(items);
        self.drop_here = drop_here;
        changed
    }

    fn set_geom(&mut self, g: Geom, cx: &mut Context<Self>) {
        if g.card_w != self.geom.card_w { self.list.remeasure(); cx.notify(); }
        self.geom = g;
    }
}

/// The list item under the pointer, at the list's current scroll.
fn hot_at(list: &ListState, n: usize, at: Point<Pixels>) -> Option<usize> {
    if !list.viewport_bounds().contains(&at) { return None; }
    (0..n).find(|&i| list.bounds_for_item(i).is_some_and(|b| b.contains(&at)))
}

impl Render for ColumnView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("kanban-col");
        let Some(c) = self.col.clone() else { return div().into_any_element() };
        let inp = inputs(&self.store, &self.cards, cx);
        // This column's boxes are taken again as it paints.
        { let mut b = inp.boxes.borrow_mut(); b.cards.retain(|x| x.0 != c.status || c.other); b.cols.retain(|x| x.0 != c.status || c.other); let ids: Vec<&String> = c.cards.iter().map(|x| &x.id).collect(); b.controls.retain(|x| !ids.contains(&&x.0)); }
        let (items, card_w, inp2) = (self.items.clone(), self.geom.card_w, inp.clone());
        // The card under the pointer (as this frame lays the list out) has the live controls.
        // (While it scrolls, no card is live: the pointer's next move makes
        // the one under it live; a click before that is the card at rest's.)
        let top = self.list.logical_scroll_top();
        if self.last_top != Some(top) {
            // A scroll lets go of the live card, and closes the attachments strip (the web's scroll capture).
            if self.last_top.is_some() { self.hot = None; if self.store.read(cx).clip.is_some() { cx.defer(super::clip_close); } }
            self.last_top = Some(top);
        }
        if self.store.read(cx).boxes.borrow().pressing { self.hot = None; }
        let _ = window;
        let (hot, all) = (self.hot, self.store.read(cx).all_live);
        let body = div().w_full().flex_1().min_h_0().child(w::layer(list(self.list.clone(), move |ix, _, cx| items.get(ix).map(|it| item_el(it, card_w, &inp2, all || hot == Some(ix), cx)).unwrap_or_else(|| div().into_any_element()))
            .size_full().into_any_element()));
        let (ls, me, status, other, boxes) = (self.list.clone(), cx.entity().downgrade(), c.status.clone(), c.other, inp.boxes.clone());
        // After the list has laid out: the column's box (a drop target), and
        // the height it needs (the web's column shrinks to its cards).
        let report = canvas(move |b, _, cx| {
            if !other { boxes.borrow_mut().cols.push((status.clone(), b4(b))); }
            let vp = ls.viewport_bounds();
            let m = ls.max_offset_for_scrollbar().y;
            let n = ls.item_count();
            let content = if m > px(0.5) { vp.size.height + m } else if n > 0 { ls.bounds_for_item(n - 1).map(|r| r.bottom() - vp.top()).unwrap_or(vp.size.height) } else { px(0.) };
            let needed = (b.size.height - vp.size.height + content).ceil();
            if let Some(me) = me.upgrade() {
                if (me.read(cx).needed - needed).abs() > px(0.5) { cx.defer(move |cx| me.update(cx, |v, cx| { v.needed = needed; cx.notify(); })); }
            }
        }, |_, _, _, _| {}).absolute().top_0().left_0().size_full();
        div().id(("kanban-col", self.idx)).relative().size_full().flex().flex_col().min_h_0()
            // The pointer onto another card: that card gets the live controls.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                // (Not while a press on a card at rest is held: that card's click is its own.)
                if this.store.read(cx).boxes.borrow().pressing { return; }
                let h = hot_at(&this.list, this.items.len(), e.position); if h != this.hot { this.hot = h; cx.notify(); }
            }))
            .on_hover(cx.listener(|this, on: &bool, _, cx| if !*on && this.hot.is_some() { this.hot = None; cx.notify(); }))
            .rounded(T::R_CARD).border_1().bg(T::bg_secondary()).border_color(edge(self.drop_here, c.other))
            .child(col_head(&c))
            .child(w::rule())
            .child(body)
            .child(report)
            .into_any_element()
    }
}

// ─── The stacked board (a narrow host) ───

/// A row of the stacked board: a column's head, one of its items, the gaps.
#[derive(Clone, PartialEq)]
enum SRow { Gap(Pixels), Head { col: Rc<Col>, drop: bool }, It { item: Item, drop: bool, other: bool, last: bool } }

pub struct StackView { store: Entity<KStore>, rows: Rc<Vec<SRow>>, list: ListState, geom: Geom, hot: Option<usize>, last_top: Option<ListOffset>, all_live: bool, cards: super::card_el::Cache, _subs: Vec<Subscription> }

impl StackView {
    fn new(store: Entity<KStore>, cx: &mut Context<Self>) -> Self {
        let drag = store.read(cx).drag.clone();
        let subs = vec![
            cx.observe(&store, |this: &mut Self, _, cx| if this.sync(cx) { cx.notify() }),
            cx.observe(&drag, |this: &mut Self, _, cx| if this.sync(cx) { cx.notify() }),
        ];
        let mut v = StackView { store, rows: Rc::new(vec![]), list: ListState::new(0, ListAlignment::Top, T::SP_48 * 4.), geom: Geom::default(), hot: None, last_top: None, all_live: false, cards: Rc::default(), _subs: subs };
        v.sync(cx);
        v
    }

    fn sync(&mut self, cx: &App) -> bool {
        let k = self.store.read(cx);
        let d = k.drag.read(cx);
        let clip = k.clip.as_ref().map(|c| c.task.as_str());
        let mut rows = vec![SRow::Gap(T::SP_12)];
        for (i, c) in k.cols.iter().enumerate() {
            if i > 0 { rows.push(SRow::Gap(T::SP_10)); }
            let drop = d.drag.is_some() && !c.other && d.target.as_ref().is_some_and(|t| t.0 == c.status);
            rows.push(SRow::Head { col: c.clone(), drop });
            let items = items_of(c, d, clip);
            let n = items.len();
            for (j, it) in items.into_iter().enumerate() { rows.push(SRow::It { item: it, drop, other: c.other, last: j + 1 == n }); }
        }
        rows.push(SRow::Gap(T::SP_12));
        let changed = splice_diff(&self.list, &self.rows, &rows) || k.all_live != self.all_live;
        self.all_live = k.all_live;
        self.rows = Rc::new(rows);
        changed
    }

    fn set_geom(&mut self, g: Geom, cx: &mut Context<Self>) {
        if g.card_w != self.geom.card_w { self.list.remeasure(); cx.notify(); }
        self.geom = g;
    }
}

fn srow_el(r: &SRow, card_w: Pixels, inp: &Inputs, live: bool, cx: &App) -> AnyElement {
    let status_of = |it: &Item| match it { Item::Card { status, .. } | Item::Foot { status, .. } => status.clone() };
    // A row of a column paints its part of the column's box, and reports
    // it as part of the column (the drop target).
    let colbox = |status: String, other: bool, boxes: Rc<RefCell<super::Boxes>>| canvas(move |b, _, _| if !other { boxes.borrow_mut().cols.push((status, b4(b))) }, |_, _, _, _| {}).absolute().top_0().left_0().size_full();
    match r {
        SRow::Gap(h) => div().w_full().h(*h).into_any_element(),
        SRow::Head { col, drop } => (div().relative().w_full().flex().flex_col().bg(T::bg_secondary()).border_t_1().border_x_1().border_color(edge(*drop, col.other))
            .rounded_t(T::R_CARD)
            .child(colbox(col.status.clone(), col.other, inp.boxes.clone()))
            .child(col_head(col)).child(w::rule())).into_any_element(),
        SRow::It { item, drop, other, last } => (div().relative().w_full().bg(T::bg_secondary()).border_x_1().border_color(edge(*drop, *other))
            .when(*last, |d| d.border_b_1().rounded_b(T::R_CARD))
            .child(colbox(status_of(item), *other, inp.boxes.clone()))
            .child(item_el(item, card_w, inp, live, cx))).into_any_element(),
    }
}

impl Render for StackView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("kanban-stack");
        let inp = inputs(&self.store, &self.cards, cx);
        { let mut b = inp.boxes.borrow_mut(); b.cards.clear(); b.cols.clear(); b.controls.clear(); }
        let (rows, card_w) = (self.rows.clone(), self.geom.card_w);
        let top = self.list.logical_scroll_top();
        if self.last_top != Some(top) {
            // A scroll lets go of the live card, and closes the attachments strip (the web's scroll capture).
            if self.last_top.is_some() { self.hot = None; if self.store.read(cx).clip.is_some() { cx.defer(super::clip_close); } }
            self.last_top = Some(top);
        }
        if self.store.read(cx).boxes.borrow().pressing { self.hot = None; }
        let _ = window;
        let (hot, all) = (self.hot, self.store.read(cx).all_live);
        div().id("kanban-stack").size_full()
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.store.read(cx).boxes.borrow().pressing { return; }
                let h = hot_at(&this.list, this.rows.len(), e.position); if h != this.hot { this.hot = h; cx.notify(); }
            }))
            .on_hover(cx.listener(|this, on: &bool, _, cx| if !*on && this.hot.is_some() { this.hot = None; cx.notify(); }))
            // (The stack's sides, sp-12, are the list's padding.)
            .child(w::layer(list(self.list.clone(), move |ix, _, cx| rows.get(ix).map(|r| srow_el(r, card_w, &inp, all || hot == Some(ix), cx)).unwrap_or_else(|| div().into_any_element())).size_full().px(T::SP_12).into_any_element()))
    }
}

// ─── Trash and History ───

fn section(text: &'static str) -> Div {
    div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(text)
}

fn hist_row(id: impl Into<ElementId>, ts: String, text: String, sub: String, restore: Option<Box<dyn Fn(&mut Window, &mut App)>>) -> AnyElement {
    div().id(id).flex().items_center().gap(T::SP_8).py(T::SP_4).border_b_1().border_color(T::border_subtle())
        .when(!ts.is_empty(), |d| d.child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(ts)))
        .child(div().flex_1().min_w_0().text_size(T::TEXT_12).text_color(T::text_secondary()).child(text))
        .when(!sub.is_empty(), |d| d.child(div().flex_none().max_w(T::KANBAN_COL_MIN).truncate().text_size(T::TEXT_11).text_color(T::text_muted()).child(sub)))
        .when_some(restore, |d, f| d.child(w::button("restore", "Restore").on_click(move |_, window, cx| f(window, cx))))
        .into_any_element()
}

#[derive(Clone, PartialEq)]
enum TRow { Section(&'static str), Empty(&'static str), Space, Task(usize, board::Row), Todo(usize, board::Row) }

pub struct TrashView { store: Entity<KStore>, rows: Rc<Vec<TRow>>, list: ListState, _sub: Subscription }

impl TrashView {
    fn new(store: Entity<KStore>, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&store, |this: &mut Self, _, cx| if this.sync(cx) { cx.notify() });
        let mut v = TrashView { store, rows: Rc::new(vec![]), list: ListState::new(0, ListAlignment::Top, T::SP_48 * 4.), _sub: sub };
        v.sync(cx);
        v
    }
    fn sync(&mut self, cx: &App) -> bool {
        let t = self.store.read(cx).trash.clone();
        let mut rows = vec![TRow::Section("DELETED TASKS")];
        if t.0.is_empty() { rows.push(TRow::Empty("No deleted tasks.")); }
        rows.extend(t.0.iter().cloned().enumerate().map(|(i, r)| TRow::Task(i, r)));
        rows.push(TRow::Space);
        rows.push(TRow::Section("DELETED TODOS"));
        if t.1.is_empty() { rows.push(TRow::Empty("No deleted todos.")); }
        rows.extend(t.1.iter().cloned().enumerate().map(|(i, r)| TRow::Todo(i, r)));
        let changed = splice_diff(&self.list, &self.rows, &rows);
        self.rows = Rc::new(rows);
        changed
    }
}

fn trow_el(r: &TRow, first: bool, last: bool) -> AnyElement {
    let el: AnyElement = match r {
        TRow::Section(t) => section(t).into_any_element(),
        TRow::Empty(t) => muted(*t).into_any_element(),
        TRow::Space => div().h(T::SP_8).into_any_element(),
        TRow::Task(i, t) => {
            let id = t.task.clone();
            div().flex().flex_col().gap(T::SP_8).px(T::SP_12).pt(T::SP_12).pb(T::SP_10).rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg_card())
                .child(div().w_full().min_w_0().opacity(T::KANBAN_O_SOFT).text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(t.text.clone()))
                .child(div().flex().child(w::button(("kanban-restore", *i), "Restore").on_click(move |_, window, cx| act("restore-task", &id, "", window, cx))))
                .into_any_element()
        }
        TRow::Todo(i, t) => {
            let (task, todo) = (t.task.clone(), t.todo.clone());
            div().id(("kanban-dtodo", *i)).child(hist_row(("kanban-dtodo-row", *i), String::new(), t.text.clone(), format!("in {}", t.sub),
                Some(Box::new(move |window, cx| act("restore-todo", &task, &todo, window, cx))))).into_any_element()
        }
    };
    // The list's padding (sp-12) and its gap (sp-8), per row.
    div().w_full().px(T::SP_12).pt(if first { T::SP_12 } else { T::SP_8 }).when(last, |d| d.pb(T::SP_12)).child(el).into_any_element()
}

impl Render for TrashView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        rendered("kanban-trash");
        let rows = self.rows.clone();
        let n = rows.len();
        div().id("kanban-trash").size_full()
            .child(list(self.list.clone(), move |ix, _, _| rows.get(ix).map(|r| trow_el(r, ix == 0, ix + 1 == n)).unwrap_or_else(|| div().into_any_element())).size_full())
    }
}

pub struct HistView { store: Entity<KStore>, rows: Rc<Vec<board::Row>>, list: ListState, _sub: Subscription }

impl HistView {
    fn new(store: Entity<KStore>, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&store, |this: &mut Self, _, cx| if this.sync(cx) { cx.notify() });
        let mut v = HistView { store, rows: Rc::new(vec![]), list: ListState::new(0, ListAlignment::Top, T::SP_48 * 4.), _sub: sub };
        v.sync(cx);
        v
    }
    fn sync(&mut self, cx: &App) -> bool {
        let h = self.store.read(cx).history.clone();
        if Rc::ptr_eq(&h, &self.rows) { return false; }
        let changed = splice_diff(&self.list, &self.rows, &h);
        self.rows = h;
        changed
    }
}

impl Render for HistView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        rendered("kanban-history");
        let rows = self.rows.clone();
        let n = rows.len();
        let body = if n == 0 { muted("No history yet.").into_any_element() } else {
            list(self.list.clone(), move |ix, _, _| rows.get(ix).map(|r| div().w_full().px(T::SP_12).when(ix == 0, |d| d.pt(T::SP_12)).when(ix + 1 == n, |d| d.pb(T::SP_12))
                .child(hist_row(("kanban-hist", ix), r.sub.clone(), r.text.clone(), String::new(), None)).into_any_element()).unwrap_or_else(|| div().into_any_element()))
                .size_full().into_any_element()
        };
        div().id("kanban-history").size_full().child(body)
    }
}

// ─── The layers over the board ───

pub struct Overlay { store: Entity<KStore>, drag: Entity<Drag>, focus: FocusHandle, focused_for: Option<String>, _subs: Vec<Subscription> }

impl Overlay {
    fn new(store: Entity<KStore>, _embedded: bool, cx: &mut Context<Self>) -> Self {
        let drag = store.read(cx).drag.clone();
        let mut clip = store.read(cx).clip.clone();
        let subs = vec![
            cx.observe(&drag, |_, _, cx| cx.notify()),
            // The strip and its pictures: redrawn when they change.
            cx.observe(&store, move |_, s, cx| { let c = s.read(cx).clip.clone(); if c != clip || c.is_some() { clip = c; cx.notify(); } }),
        ];
        Overlay { store, drag, focus: cx.focus_handle(), focused_for: None, _subs: subs }
    }
}

impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("kanban-overlay");
        let d = self.drag.read(cx);
        let held = d.press.is_some();
        let mut root = div().size_full()
            .child(canvas(|_, _, _| {}, move |_, _, window, _| {
                // While a card is held, the pointer anywhere on the window moves it (and lets go of it).
                if !held { return; }
                window.on_mouse_event(|e: &MouseMoveEvent, phase, _, cx| {
                    // The test driver's moves carry no pressed button, so the held card is ours to track.
                    if phase == DispatchPhase::Bubble && (e.pressed_button == Some(MouseButton::Left) || std::env::var("CRUNDI_TEST_PORT").is_ok()) { super::pointer_move(e.position, cx); }
                });
                window.on_mouse_event(|e: &MouseUpEvent, phase, _, cx| { if phase == DispatchPhase::Bubble && e.button == MouseButton::Left { super::pointer_up(cx); } });
            }).absolute().top_0().left_0().size_full());
        // The dragged card's clone (.drag-clone): its box and title, following the pointer.
        if let Some(d) = d.drag.as_ref() {
            root = root.child(deferred(anchored().position(point(px(d.x - d.grab.0), px(d.y - d.grab.1))).child(
                div().w(px(d.w)).h(px(d.h)).overflow_hidden().opacity(T::KANBAN_O_CLONE).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::accent()).bg(T::bg_card())
                    .shadow_lg().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(d.title.clone()),
            )).with_priority(1));
        }
        // The strip takes the keys as it opens (its Esc closes it).
        let task = self.store.read(cx).clip.as_ref().map(|c| c.task.clone());
        if task != self.focused_for {
            self.focused_for = task.clone();
            if task.is_some() { let f = self.focus.clone(); cx.defer_in(window, move |_, window, cx| window.focus(&f, cx)); }
        }
        root.children(clip_pop(&self.store, &self.focus, window, cx))
    }
}

/// A card's attachments (.kc-media-pop): thumbnails (or "No attachments
/// yet") and the + that opens the task's media, against the paperclip (the
/// web's placePop: under it, right edges together; above it when there is no
/// room below). It blocks nothing: a press anywhere else, Esc, or the board
/// scrolling closes it (a press on the paperclip is the paperclip's toggle).
/// Where the strip goes (the web's placePop): under the paperclip, right
/// edges together; above it when there is no room below; 8 px inside the window.
pub(crate) fn pop_rect(c: &super::Clip, vp: Size<Pixels>) -> Bounds<Pixels> {
    let btn = &c.btn;
    let pw = (px(c.w) - T::SP_8).clamp(T::KANBAN_POP_MIN, T::KANBAN_POP_MAX);
    let ph = T::KANBAN_THUMB + T::SP_8 * 2. + px(2.);
    let gap = T::SP_4;
    let left = (px(btn.x + btn.w) - pw).min(vp.width - pw - T::SP_8).max(T::SP_8);
    let below = px(btn.y + btn.h) + gap;
    let top = if below + ph > vp.height - T::SP_8 { (px(btn.y) - ph - gap).max(T::SP_8) } else { below };
    Bounds { origin: point(left, top), size: size(pw, ph) }
}

fn clip_pop(store: &Entity<KStore>, focus: &FocusHandle, window: &Window, cx: &App) -> Option<AnyElement> {
    let k = store.read(cx);
    let clip = k.clip.as_ref()?;
    let super::Clip { task, btn, .. } = clip;
    let r = pop_rect(clip, window.viewport_size());
    let (left, top, pw, ph) = (r.origin.x, r.origin.y, r.size.width, r.size.height);
    let items = k.media_task.get(task).cloned().unwrap_or_default();
    let mut strip = div().id("kanban-clip-strip").flex_1().min_w_0().overflow_x_scroll().flex().items_center().gap(T::SP_6).p(T::SP_8);
    if items.is_empty() { strip = strip.child(div().px(T::SP_4).text_size(T::TEXT_12).text_color(T::text_muted()).child("No attachments yet")); }
    for (i, it) in items.iter().enumerate() {
        let id = it.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let kind = it.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let pic = k.thumbs.get(&id).cloned().flatten();
        let icon = match kind.as_str() { "video" => IconName::Film, "audio" => IconName::Music, "pdf" => IconName::FileText, "image" => IconName::Image, _ => IconName::File };
        strip = strip.child(div().id(("kanban-thumb", i)).flex_none().size(T::KANBAN_THUMB).rounded(T::R_CONTROL).border_1().border_color(T::border())
            .hover(|d| d.border_color(T::accent())).bg(T::bg_tertiary()).overflow_hidden().cursor_pointer().flex().items_center().justify_center()
            .text_color(T::text_secondary())
            .map(|d| match pic { Some(p) => d.child(img(ImageSource::Image(p)).size_full().object_fit(ObjectFit::Cover)), None => d.child(w::icon(icon, T::ICON_XL)) })
            .on_click(move |_, _, cx| super::open_media(&id, cx)));
    }
    let b = *btn;
    Some(deferred(anchored().position(point(left, top)).child(
        div().id("kanban-clip-pop").w(pw).h(ph).flex().rounded(T::R_FIELD).border_1().border_color(T::border())
            .bg(T::bg_secondary()).shadow_lg().overflow_hidden().occlude()
            // Esc closes it (the web's keydown on the document).
            .track_focus(focus)
            .on_key_down(|e: &KeyDownEvent, _, cx| if e.keystroke.key == "escape" { super::clip_close(cx); cx.stop_propagation(); })
            .on_mouse_down_out(move |e: &MouseDownEvent, _, cx| {
                let (x, y) = (f32::from(e.position.x), f32::from(e.position.y));
                if !b.has(x, y) { super::clip_close(cx); }
            })
            .child(strip)
            .child(div().flex_none().h_full().border_l_1().border_color(T::border_subtle()).flex().items_center().px(T::SP_4)
                .child(w::square_button("kanban-clip-add", IconName::Plus, "Attach a file", w::SquareLook::Plain, T::SP_40, T::ICON_XL)
                    .on_click({ let t = task.to_string(); move |_, _, cx| super::media_task(&t, cx) }))),
    )).with_priority(1).into_any_element())
}
