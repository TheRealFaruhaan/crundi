//! The workbench (ARCHITECTURE.md, "The view tree"): the pane host. The
//! Workbench view is layout only: the mosaic's rectangles (core::mosaic, the
//! web's), the gutters, empty panes, and each pane: its head (PaneHead, a
//! cached view) and its body (the kind's elements around its own cached
//! views). A pane's change redraws that pane's view; the Workbench, an
//! ancestor of every pane, re-renders with it (GPUI marks every ancestor of
//! a notified view dirty), so its render stays small: the mosaic's
//! rectangles and one div per leaf, nothing per row or per message.
//!
//! A pane kind registers once (`register_kind`): its key prefix ("live:"
//! for a chat), how its body and head are made, what A- / A+ and × do.
//! What a kind adds to its heads (icon, buttons, tags, the secrets lock) is
//! registered beside it (`register_head`, extra.rs).
//! The layout per project is saved (session.json's `layouts`, as native-gpui).
//!
//! Moving a pane (the web's makeDraggable + termDragHandlers): a press on a
//! head that moves 6px picks the pane up; a card with its title follows the
//! pointer (its own small view: a move redraws the card and the layout
//! views, never a pane), every leaf is outlined and the one under the
//! pointer lit; let go on a pane swaps the two, on an empty leaf moves it
//! there. A pinned pane does not move ("This pane is pinned. Unpin it to
//! move it.").
//!
//! Full screen (the web's setPaneFull): the pane over the whole window, not
//! only the workbench: drawn deferred from the window's corner, over the
//! bars (dialogs, menus and toasts stay above it).

mod add_menu;
mod extra;
mod head;
pub mod launcher;

pub use add_menu::{set_open as set_add_menu, toggle as toggle_add_menu};
pub use extra::{register_head, ExtraFn, HeadAction, HeadButton, HeadExtra, HeadTag, KindHead};
pub use head::{HeadInfo, PaneHead};

use crundi_core::{mosaic::{Dir, Dropped, LeafRect, Mosaic}, store};
use crundi_shell::Stores;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use std::{cell::Cell, collections::HashMap, rc::Rc, time::{Duration, Instant}};

/// What a pane kind makes for one pane.
pub struct PaneParts {
    /// The pane's body as elements, built on every Workbench render: layout
    /// only, placing the kind's views cached (a body is not a view of its
    /// own: a view between the Workbench and the leaves would re-render with
    /// every leaf's notify, and a cached one would re-render its cached
    /// leaves with it; ARCHITECTURE.md, rule 3).
    pub body: Body,
    /// The head's title and state, read by the head when it renders.
    pub info: Rc<dyn Fn(&App) -> HeadInfo>,
    /// The head observes what its info (and its `register_head` extra) reads.
    pub watch: Box<dyn FnOnce(&mut Context<PaneHead>) -> Vec<Subscription>>,
}

pub type Body = Rc<dyn Fn(&mut App) -> AnyElement>;
pub type MakePane = Rc<dyn Fn(&str, &Stores, &mut Window, &mut App) -> PaneParts>;
/// × on a pane: done(true) when it may go (the server closed the chat), else
/// done(false) and the pane stays (the kind said why).
pub type ClosePane = Rc<dyn Fn(&str, &mut App, Box<dyn FnOnce(bool, &mut App)>)>;
pub type ZoomPane = Rc<dyn Fn(&str, i32, &mut App)>;

#[derive(Clone)]
pub struct PaneKind {
    pub prefix: &'static str,
    /// A chat's head (dot, badge, A- ↺ A+, full screen, the two-click ×,
    /// the tools folded into "…" when narrow).
    pub chat: bool,
    pub make: MakePane,
    pub close: Option<ClosePane>,
    pub zoom: Option<ZoomPane>,
}

#[derive(Default)]
struct Kinds(Vec<PaneKind>);
impl Global for Kinds {}

/// Register a pane kind (at start; read in handlers only).
pub fn register_kind(cx: &mut App, k: PaneKind) { cx.default_global::<Kinds>().0.push(k); }

fn kind_of(cx: &App, key: &str) -> Option<PaneKind> {
    cx.try_global::<Kinds>().and_then(|k| k.0.iter().find(|k| key.starts_with(k.prefix)).cloned())
}

struct Pane {
    head: Entity<PaneHead>,
    body: Body,
}

struct Handle(WeakEntity<Workbench>);
impl Global for Handle {}

/// The workbench view (for handlers: the chat area places its panes).
pub fn workbench(cx: &App) -> Option<Entity<Workbench>> { cx.try_global::<Handle>().and_then(|h| h.0.upgrade()) }

/// A gutter being dragged: its split and index, the pair's shares and px at
/// the press, along a row or not, and where the press was.
struct GutterDrag { split: u32, index: usize, a0: f32, b0: f32, total: f32, row: bool, from: f32 }

/// A press on a head (window px): a drag once it moves 6px.
#[derive(Clone, Copy)]
struct Press { leaf: u32, x: f32, y: f32 }
/// A pane being moved: its leaf, the leaf it would land on (0: none, or a
/// pinned one), the pointer, and where in the pane it was taken.
#[derive(Clone, Copy)]
struct Drag { src: u32, target: u32, x: f32, y: f32, dx: f32, dy: f32 }

/// The card that follows the pointer while a pane moves: the pane's title
/// on a header's strip as wide as the pane (a view of its own, cached).
struct Ghost { title: SharedString }
impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("drag-card");
        div().size_full().flex().items_center().px(T::SP_10).rounded(T::R_CARD).border_1().border_color(T::accent()).bg(T::bg_tertiary()).opacity(0.92)
            .shadow_lg().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text())
            .child(div().truncate().child(self.title.clone()))
    }
}

pub struct Workbench {
    stores: Stores,
    project: String,
    layouts: HashMap<String, Mosaic>,
    panes: HashMap<String, Pane>,
    /// The workbench's size (measured each frame; a change re-lays it out).
    area: (f32, f32),
    pub focused: u32,
    armed: u32,
    arm_gen: u64,
    full: Option<String>,
    gutter: Option<GutterDrag>,
    scroll: ScrollHandle,
    /// Each pane head's frame, as last laid out.
    frames: HashMap<String, head::Frame>,
    /// The launcher put in because there were no panes (the next pane
    /// placed takes its place), and the launchers' numbers.
    seeded: Option<String>,
    pend: u32,
    /// Panes just started here, kept for a while even if the server's list
    /// does not have them yet.
    protect: HashMap<String, Instant>,
    /// The empty leaf whose chip is being clicked: the chip's action places
    /// its pane there (the web fills the clicked leaf), set only while the
    /// action runs.
    chip_leaf: Option<u32>,
    /// The workbench's top-left in the window (set as it is laid out).
    origin: Rc<Cell<(f32, f32)>>,
    press: Option<Press>,
    drag: Option<Drag>,
    /// The last press moved a pane (its title's click is then no rename).
    moved: bool,
    ghost: Option<Entity<Ghost>>,
    /// The mosaic laid out (F99): once per change of the mosaic, the
    /// project or the area, not on every frame the Workbench renders.
    laid: Option<Rc<Laid>>,
}

/// The mosaic's rectangles for a project and an area (the grid's width,
/// the leaves, the gutters).
struct Laid { project: String, area: (f32, f32), gw: f32, leaves: Vec<LeafRect>, gutters: Vec<crundi_core::mosaic::GutterRect> }

impl Workbench {
    pub fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        cx.set_global(Handle(cx.entity().downgrade()));
        let project = stores.projects.read(cx).open.clone();
        Workbench { stores, project, layouts: HashMap::new(), panes: HashMap::new(), area: (0., 0.), focused: 0, armed: 0, arm_gen: 0, full: None, gutter: None, scroll: ScrollHandle::new(), frames: HashMap::new(), seeded: None, pend: 0, protect: HashMap::new(), chip_leaf: None,
            origin: Rc::new(Cell::new((0., 0.))), press: None, drag: None, moved: false, ghost: None, laid: None }
    }

    /// The open project's mosaic (loaded from the saved layouts the first
    /// time), to change: its layout is made again on the next render.
    pub fn mosaic(&mut self) -> &mut Mosaic {
        self.laid = None;
        self.mosaic_ro_mut()
    }

    fn mosaic_ro_mut(&mut self) -> &mut Mosaic {
        let p = self.project.clone();
        self.layouts.entry(p.clone()).or_insert_with(|| store::load().layouts.get(&p).and_then(|v| serde_json::from_value::<Mosaic>(v.clone()).ok()).unwrap_or_default())
    }

    /// The open project's mosaic, to read (its layout stays).
    fn mosaic_ro(&mut self) -> &Mosaic { self.mosaic_ro_mut() }

    /// The mosaic laid out for the workbench's area (kept until it changes).
    fn laid(&mut self) -> Rc<Laid> {
        if let Some(l) = self.laid.as_ref().filter(|l| l.project == self.project && l.area == self.area) { return l.clone(); }
        let (aw, ah) = self.area;
        let m = self.mosaic_ro();
        let gw = m.content_width(aw);
        let (leaves, gutters) = m.layout(gw, ah);
        let l = Rc::new(Laid { project: self.project.clone(), area: self.area, gw, leaves, gutters });
        self.laid = Some(l.clone());
        l
    }

    fn save(&mut self) {
        let p = self.project.clone();
        if let Some(v) = self.layouts.get(&p).and_then(|m| serde_json::to_value(m).ok()) { store::update(|s| { s.layouts.insert(p, v); }); }
    }

    fn changed(&mut self, cx: &mut Context<Self>) { self.save(); cx.notify(); }

    /// Another project: its own layout.
    pub fn set_project(&mut self, alias: &str, cx: &mut Context<Self>) {
        if self.project == alias { return; }
        self.project = alias.to_string();
        self.full = None;
        self.seeded = None;
        self.press = None;
        self.drag = None;
        self.ghost = None;
        cx.notify();
    }

    /// Put a pane in the layout (the launcher put in for an empty
    /// workbench, else an empty leaf, else a new one to the right).
    pub fn place(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.mosaic().leaf_of(key).is_some() { return; }
        // An empty leaf's chip: that leaf.
        if let Some(l) = self.chip_leaf.take().filter(|l| self.leaf_empty(*l)) {
            self.mosaic().assign(l, Some(key.to_string()));
            self.focused = l;
            return self.changed(cx);
        }
        if let Some(seed) = self.seeded.take().filter(|_| !key.starts_with("pend:")) {
            if let Some(l) = self.mosaic().leaf_of(&seed) { self.mosaic().assign(l, Some(key.to_string())); self.focused = l; return self.changed(cx); }
        }
        let leaf = self.mosaic().place(key);
        self.focused = leaf;
        self.changed(cx);
    }

    /// A launcher's pane becomes what it launched.
    pub fn replace(&mut self, old: &str, new: &str, cx: &mut Context<Self>) {
        self.protect.insert(new.to_string(), Instant::now());
        if self.seeded.as_deref() == Some(old) { self.seeded = None; }
        let Some(l) = self.mosaic().leaf_of(old) else { return self.place(new, cx) };
        if let Some(other) = self.mosaic().leaf_of(new) { if other != l { self.mosaic().close(other); } }
        let l = self.mosaic().leaf_of(old).unwrap_or(l);
        self.mosaic().assign(l, Some(new.to_string()));
        self.focused = l;
        self.changed(cx);
    }

    /// Keep only the panes `keep` says (and the ones just launched here).
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool, cx: &mut Context<Self>) {
        self.protect.retain(|_, at| at.elapsed() < Duration::from_secs(15));
        let protect: Vec<String> = self.protect.keys().cloned().collect();
        let before = self.mosaic().keys();
        self.mosaic().retain(|k| keep(k) || k.starts_with("pend:") || protect.iter().any(|p| p == k));
        if self.mosaic().keys() != before { self.changed(cx); }
    }

    /// A workbench with no panes gets a launcher (the web's placeholder
    /// cell). Laid out in the same frame; saved with the next change.
    fn ensure_launcher(&mut self) {
        let keys = self.mosaic_ro().keys();
        if !keys.is_empty() { if self.seeded.as_ref().is_some_and(|s| !keys.contains(s)) { self.seeded = None; } return; }
        let n = keys.iter().filter_map(|k| k.strip_prefix("pend:")?.parse::<u32>().ok()).max().unwrap_or(0).max(self.pend) + 1;
        self.pend = n;
        let k = format!("pend:{n}");
        let leaf = self.mosaic().place(&k);
        self.focused = leaf;
        self.seeded = Some(k);
    }

    pub fn keys(&mut self) -> Vec<String> { self.mosaic_ro().keys() }

    /// The laid-out leaves (the grid's own coordinates).
    fn leaves(&mut self) -> Vec<LeafRect> {
        let (w, h) = self.area;
        let m = self.mosaic_ro();
        let gw = m.content_width(w.max(1.));
        m.layout(gw, h.max(1.)).0
    }

    /// The focused pane's key (else the first pane's).
    pub fn focused_key(&mut self) -> Option<String> {
        let leaves = self.leaves();
        leaves.iter().find(|l| l.id == self.focused).and_then(|l| l.key.clone()).or_else(|| leaves.iter().find_map(|l| l.key.clone()))
    }

    pub fn focus_key(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(l) = self.mosaic().leaf_of(key) { if self.focused != l { self.focused = l; cx.notify(); } }
    }

    fn key_of(&mut self, leaf: u32) -> Option<String> {
        self.leaves().into_iter().find(|l| l.id == leaf).and_then(|l| l.key)
    }

    /// A leaf that is laid out and holds no pane.
    fn leaf_empty(&mut self, leaf: u32) -> bool {
        self.leaves().iter().any(|l| l.id == leaf && l.key.is_none())
    }

    /// An empty leaf's chip, clicked: its action runs with that leaf as the
    /// place for what it opens (the driver's "leaf-chip" too).
    pub fn chip(this: &Entity<Self>, leaf: u32, kind: &str, window: &mut Window, cx: &mut App) {
        let stores = this.update(cx, |w, _| { w.chip_leaf = Some(leaf); w.stores.clone() });
        crundi_shell::run_action(&stores, &format!("wb-chip:{kind}"), window, cx);
        this.update(cx, |w, _| w.chip_leaf = None);
    }

    fn focus_pane(&mut self, leaf: u32, cx: &mut Context<Self>) {
        if self.focused != leaf { self.focused = leaf; cx.notify(); }
    }

    pub fn split(&mut self, leaf: u32, down: bool, cx: &mut Context<Self>) {
        let l = self.mosaic().split(leaf, if down { Dir::Col } else { Dir::Row });
        self.focused = l;
        self.changed(cx);
    }

    /// × : a chat's takes two clicks ("Sure?" for 3 s), then the kind closes it.
    pub fn pane_close(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let key = self.key_of(leaf).unwrap_or_default();
        let kind = kind_of(cx, &key);
        if !kind.as_ref().is_some_and(|k| k.chat) { self.armed = 0; return self.close_pane(leaf, cx); }
        if self.armed == leaf { self.armed = 0; cx.notify(); return self.close_pane(leaf, cx); }
        self.armed = leaf;
        self.arm_gen += 1;
        let g = self.arm_gen;
        cx.spawn(async move |this: WeakEntity<Workbench>, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(3000)).await;
            let _ = this.update(cx, |w, cx| if w.armed == leaf && w.arm_gen == g { w.armed = 0; cx.notify(); });
        }).detach();
        cx.notify();
    }

    fn close_pane(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let key = self.key_of(leaf).unwrap_or_default();
        let close = kind_of(cx, &key).and_then(|k| k.close);
        let Some(close) = close else { self.mosaic().close(leaf); return self.changed(cx); };
        let me = cx.entity().downgrade();
        let k2 = key.clone();
        close(&k2, cx, Box::new(move |ok, cx| if ok {
            let _ = me.update(cx, |w, cx| {
                if let Some(l) = w.mosaic().leaf_of(&key) { w.mosaic().close(l); }
                if w.full.as_deref() == Some(key.as_str()) { w.full = None; }
                w.changed(cx);
            });
        }));
    }

    fn pane_full(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let Some(key) = self.key_of(leaf) else { return };
        self.full = if self.full.as_deref() == Some(key.as_str()) { None } else { Some(key) };
        self.armed = 0;
        self.press = None;
        cx.notify();
    }

    fn pane_maximize(&mut self, leaf: u32, cx: &mut Context<Self>) {
        self.mosaic().maximize(leaf);
        self.changed(cx);
    }

    fn pane_pin(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let r = self.leaves().into_iter().find(|l| l.id == leaf);
        if let Some(r) = r { self.mosaic().toggle_pin(leaf, r.w, r.h); self.changed(cx); }
    }

    fn pane_zoom(&mut self, leaf: u32, d: i32, cx: &mut Context<Self>) {
        let Some(key) = self.key_of(leaf) else { return };
        if let Some(z) = kind_of(cx, &key).and_then(|k| k.zoom) { z(&key, d, cx); }
    }

    /// The "+" menu's Fit to screen (the web's mosaicFitToScreen): every
    /// split even, the row back at its start; if it still scrolls sideways,
    /// an error says why.
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.mosaic().fit();
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.changed(cx);
        cx.spawn(async move |this: WeakEntity<Workbench>, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(60)).await;
            let msg = this.update(cx, |w, _| {
                let aw = w.area.0;
                let over = w.mosaic().content_width(aw) > aw + 2.0;
                over.then(|| if w.mosaic().has_pin() { "Does not fit: the pinned panes plus the others at their minimum width are wider than the window" }
                    else { "Does not fit: too many panes side by side for this window, even at their minimum width" })
            }).ok().flatten();
            if let Some(m) = msg { let _ = cx.update(|cx| if let Some(h) = cx.active_window() { let _ = h.update(cx, |_, window, cx| w::toast(window, cx, m, w::Toast::Error)); }); }
        }).detach();
    }

    /// A Layout preset (the web's mosaicApplyPreset): the panes in display
    /// order (the project's chats in the server's order first), pins kept;
    /// the focused pane stays focused.
    pub fn preset(&mut self, name: &str, cx: &mut Context<Self>) {
        let project = self.project.clone();
        let order: Vec<String> = self.stores.projects.read(cx).terminals.iter()
            .filter(|t| t.get("kind").and_then(|k| k.as_str()) == Some("ui") && t.get("project").and_then(|p| p.as_str()) == Some(project.as_str()))
            .filter_map(|t| t.get("id").and_then(|i| i.as_str()).map(|i| format!("live:{i}"))).collect();
        let key = self.key_of(self.focused);
        let m = self.mosaic().preset_from(name, &order);
        *self.mosaic() = m;
        if let Some(l) = key.and_then(|k| self.mosaic().leaf_of(&k)) { self.focused = l; }
        self.changed(cx);
    }

    /// A new launcher pane ("+" Terminal, when no area places one).
    pub fn add_launcher(&mut self, cx: &mut Context<Self>) {
        let n = self.keys().iter().filter_map(|k| k.strip_prefix("pend:")?.parse::<u32>().ok()).max().unwrap_or(0).max(self.pend) + 1;
        self.pend = n;
        self.place(&format!("pend:{n}"), cx);
    }

    // ─── Moving panes ───

    /// The last press on a head became a drag.
    pub(crate) fn dragged(&self) -> bool { self.moved || self.drag.is_some() }

    pub(crate) fn head_press(&mut self, leaf: u32, x: f32, y: f32, _cx: &mut Context<Self>) {
        if self.full.is_some() { return; }
        self.press = Some(Press { leaf, x, y });
        self.moved = false;
    }

    /// A leaf's rect in the window.
    fn window_rect(&self, l: &LeafRect) -> (f32, f32, f32, f32) {
        let (ox, oy) = self.origin.get();
        let off = self.scroll.offset();
        (ox + f32::from(off.x) + l.x, oy + f32::from(off.y) + l.y, l.w, l.h)
    }

    /// The leaf at a window point (none on a gutter or outside).
    fn leaf_at(&mut self, x: f32, y: f32) -> Option<LeafRect> {
        let leaves = self.leaves();
        leaves.into_iter().find(|l| { let (lx, ly, lw, lh) = self.window_rect(l); x >= lx && x < lx + lw && y >= ly && y < ly + lh })
    }

    fn drag_move(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if self.drag.is_none() {
            let Some(p) = self.press else { return };
            if (x - p.x).abs() <= 6. && (y - p.y).abs() <= 6. { return; }
            let Some(l) = self.leaves().into_iter().find(|l| l.id == p.leaf) else { self.press = None; return };
            let (lx, ly, _, _) = self.window_rect(&l);
            let title = l.key.as_ref().and_then(|k| self.panes.get(k)).and_then(|p| p.head.read(cx).drawn_title()).unwrap_or_default();
            self.ghost = Some(cx.new(|_| Ghost { title: title.into() }));
            self.drag = Some(Drag { src: p.leaf, target: 0, x, y, dx: p.x - lx, dy: p.y - ly });
            self.moved = true;
        }
        // The leaf under the pointer, unless it is pinned (the web lights none then).
        let t = self.leaf_at(x, y).filter(|l| !l.pinned).map(|l| l.id).unwrap_or(0);
        if let Some(d) = self.drag.as_mut() { d.target = t; d.x = x; d.y = y; }
        cx.notify();
    }

    fn drag_end(&mut self, x: f32, y: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.press = None;
        let Some(d) = self.drag.take() else { return };
        self.ghost = None;
        let target = self.leaf_at(x, y).map(|l| l.id);
        match target.map(|t| self.mosaic().drop_on(d.src, t)).unwrap_or(Dropped::Nothing) {
            Dropped::Pinned => { w::toast(window, cx, "This pane is pinned. Unpin it to move it.", w::Toast::Info); cx.notify(); }
            Dropped::Nothing => cx.notify(),
            _ => { if let Some(t) = target { self.focused = t; } self.changed(cx); }
        }
    }

    /// A pane's views, made the first time it is laid out (in a handler: the
    /// layout pass that found it, before its frame is drawn).
    fn pane(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<(Entity<PaneHead>, Body)> {
        if !self.panes.contains_key(key) {
            let kind = kind_of(cx, key)?;
            let parts = (kind.make)(key, &self.stores, window, cx);
            let me = cx.entity().downgrade();
            let k = key.to_string();
            let projects = self.stores.projects.clone();
            let head = cx.new(|cx| PaneHead::new(k, kind.chat, parts.info, projects, me, parts.watch, cx));
            self.panes.insert(key.to_string(), Pane { head, body: parts.body });
        }
        self.panes.get(key).map(|p| (p.head.clone(), p.body.clone()))
    }

    fn head_of(&self, key: &str) -> Option<Entity<PaneHead>> { self.panes.get(key).map(|p| p.head.clone()) }

    /// What a head needs from the workbench (its leaf, armed, pinned, narrow…).
    pub(crate) fn frame_of(&self, key: &str) -> Option<head::Frame> { self.frames.get(key).copied() }

    fn head_state(&self, leaf: u32, pinned: bool, w: f32, full: bool) -> head::Frame {
        head::Frame { leaf, w, armed: self.armed == leaf, pinned, narrow: crundi_core::head::narrow(w), full, focused: self.focused == leaf }
    }

    fn pointer_move(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        let Some(g) = &self.gutter else { return };
        let (split, index, a0, b0, total, d) = (g.split, g.index, g.a0, g.b0, g.total, if g.row { x } else { y } - g.from);
        self.mosaic().drag_from(split, index, a0, b0, total, d);
        cx.notify();
    }

    fn pointer_up(&mut self, cx: &mut Context<Self>) {
        if self.gutter.take().is_some() { self.changed(cx); }
    }

    /// An empty leaf (.mosaic-empty): a dashed edge, the message, the chips,
    /// then Split right / Split down / remove.
    fn empty_leaf(&self, id: u32, open: &[String], cx: &mut Context<Self>) -> Div {
        // The chips wrap in a row at most 300 wide, centred (.me-chips).
        let mut chips = div().flex().flex_wrap().justify_center().gap(T::SP_6).max_w(T::LEAF_CHIPS_W).mb(T::SP_2);
        for (k, label) in empty_chips(open) {
            let icon = match k { "terminal" => IconName::Terminal, "files" => IconName::Folder, "git" => IconName::GitBranch, "kanban" => IconName::Kanban, "notes" => IconName::FileText, "mindmap" => IconName::Network, "media" => IconName::Images, "factory" => IconName::Factory, _ => IconName::LayoutDashboard };
            let me = cx.entity();
            chips = chips.child(w::leaf_chip(SharedString::from(format!("chip-{k}-{id}")), icon, label, format!("Add {label}"))
                .on_click(move |_, window, cx| Workbench::chip(&me, id, k, window, cx)));
        }
        // The web's .mosaic-empty: 3px in, a 2px dashed border, the message,
        // the chips and the actions 10 apart, centred. A double-click on it
        // fills its split (the web's dblclick: mosaicMaximizeLeaf).
        div().size_full().p(T::LEAF_INSET)
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| if e.click_count == 2 { this.pane_maximize(id, cx); }))
            .child(
            div().size_full().rounded(T::R_CARD).border_2().border_dashed().border_color(T::border()).overflow_hidden()
                .flex().flex_col().gap(T::SP_10).items_center().justify_center().text_size(T::TEXT_13).text_color(T::text_muted())
                .child("Drop a panel here or launch a new one")
                .child(chips)
                .child(div().flex().items_center().gap(T::SP_6)
                    .child(w::leaf_act(("e-right", id), IconName::Plus, Some("Split \u{25b8}"), "Split right").on_click(cx.listener(move |this, _, _, cx| this.split(id, false, cx))))
                    .child(w::leaf_act(("e-down", id), IconName::Plus, Some("Split \u{25be}"), "Split down").on_click(cx.listener(move |this, _, _, cx| this.split(id, true, cx))))
                    .child(w::leaf_act(("e-x", id), IconName::X, None, "Remove pane")
                        .on_click(cx.listener(move |this, _, _, cx| { this.mosaic().close(id); this.changed(cx); })))))
    }

    /// A pane: its head and body side by side, each cached (rule 3). The
    /// panel's own box, which the caller places (the leaf): the Workbench
    /// re-renders on every frame that draws any pane, so a pane is as few
    /// elements as it can be (the box, its head, its body's box, the kind's).
    fn frame(&mut self, key: &str, f: head::Frame, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let Some((head, body)) = self.pane(key, window, cx) else {
            return div().size_full().child(w::empty("Not built yet in the v2 client."));
        };
        // The head's frame (armed, pinned, narrow, focused) is the
        // workbench's: the head reads it, and is notified here only when its
        // own frame changed (F11: a focus change redraws two heads, not all).
        // A notify while the window draws marks the head dirty for this
        // draw, before its cached element is laid out, without asking for
        // another frame.
        if self.frames.get(key) != Some(&f) { self.frames.insert(key.to_string(), f); head.update(cx, |_, cx| cx.notify()); }
        let hh = if f.narrow { T::PANE_HEAD_NARROW } else { T::PANE_HEAD };
        // A wheel inside a pane stays in it (the workbench's sideways scroll
        // would otherwise move with a log: native-gpui's 19 Nov fix).
        let body = div().relative().flex_1().min_h_0().w_full().overflow_hidden()
            .on_scroll(|_, _, cx| cx.stop_propagation())
            // The kind's body: elements around its cached views.
            .child(body(cx));
        w::panel_box()
            .child(head.cached(StyleRefinement::default().w_full().h(hh).flex_none()))
            .child(body)
            .when(f.focused && !f.full, |d| d.border_color(T::accent()))
            .when(f.full, |d| d.rounded_none())
    }

    /// While a pane moves, every leaf is outlined (1px dashed) and the one
    /// it would land on lit (2px accent), 2px inside its edge (the web's
    /// body.mosaic-cell-drag .mosaic-leaf outlines).
    fn drag_outline(&self, leaf: u32) -> Option<Div> {
        let d = self.drag?;
        let on = d.target == leaf && d.src != leaf;
        Some(div().absolute().inset(T::SP_2).rounded(T::R_CARD).border_color(if on { T::accent() } else { T::border() })
            .when(on, |d| d.border_2()).when(!on, |d| d.border_1().border_dashed()))
    }
}

/// An empty pane's chips (the web's emptyLeafHtml): Terminal, the panels not
/// in the workbench yet (Notes always), Panel (the web's widgetsOn: always
/// for the owner; v2 has no collaborator sign-in yet).
fn empty_chips(open: &[String]) -> Vec<(&'static str, &'static str)> {
    let mut items: Vec<(&'static str, &'static str)> = vec![("terminal", "Terminal")];
    for (k, l) in [("files", "Files"), ("git", "Git"), ("kanban", "Kanban"), ("notes", "Notes"), ("mindmap", "Mindmap"), ("media", "Media"), ("factory", "Factory")] {
        if k == "notes" || !open.iter().any(|o| o == k) { items.push((k, l)); }
    }
    items.push(("widget", "Panel"));
    items
}

/// A "+" menu item (and the driver's "add-pane"): Terminal, a panel
/// ("files", "git", "kanban", "notes", "mindmap", "media", "factory"),
/// Panel ("widget" or "panel"), "fit", or "layout:<preset>". The workbench
/// tab comes forward, as the web's; a panel already in it is refused with
/// the web's words (Notes and Panel open many).
pub fn add_pane(kind: &str, window: &mut Window, cx: &mut App) {
    let stores = crundi_shell::stores(cx);
    if stores.projects.read(cx).open.is_empty() { return w::toast(window, cx, "Select a project first", w::Toast::Error); }
    stores.nav.update(cx, |n, cx| n.show_tab("workbench", cx));
    let wb = workbench(cx);
    if kind == "fit" { if let Some(wb) = wb { wb.update(cx, |w, cx| w.fit(cx)); } return; }
    if let Some(p) = kind.strip_prefix("layout:") { if let Some(wb) = wb { let p = p.to_string(); wb.update(cx, |w, cx| w.preset(&p, cx)); } return; }
    let kind = if kind == "panel" { "widget" } else { kind };
    if kind == "terminal" {
        if !crundi_shell::run_action(&stores, "wb-chip:terminal", window, cx) { if let Some(wb) = wb { wb.update(cx, |w, cx| w.add_launcher(cx)); } }
        return;
    }
    if kind != "notes" && kind != "widget" {
        let key = format!("panel:{kind}");
        if wb.is_some_and(|wb| wb.update(cx, |w, _| w.keys().contains(&key))) {
            let label = add_menu::ITEMS.iter().find(|i| i.0 == kind).map(|i| i.1).unwrap_or(kind);
            return w::toast(window, cx, format!("{label} is already in the workbench"), w::Toast::Info);
        }
    }
    crundi_shell::run_action(&stores, &format!("wb-chip:{kind}"), window, cx);
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("workbench");
        if !self.project.is_empty() { self.ensure_launcher(); }
        let laid = self.laid();
        let (gw, ah) = (laid.gw, self.area.1);
        let (leaves, gutters) = (&laid.leaves, &laid.gutters);
        let open: Vec<String> = leaves.iter().filter_map(|l| l.key.as_deref()).filter_map(|k| k.strip_prefix("panel:").map(|r| r.split(':').next().unwrap_or("").to_string())).collect();
        // Panes no longer laid out drop their views.
        let keys: Vec<String> = leaves.iter().filter_map(|l| l.key.clone()).collect();
        self.panes.retain(|k, _| keys.contains(k));
        self.frames.retain(|k, _| keys.contains(k));
        let me = cx.entity();
        let (me2, origin) = (me.downgrade(), self.origin.clone());
        let measure = canvas(
            move |b, _, cx| {
                let a = (f32::from(b.size.width), f32::from(b.size.height));
                origin.set((f32::from(b.origin.x), f32::from(b.origin.y)));
                // (After the frame: a notify while drawing would not draw again.)
                if me.read(cx).area != a { let me = me.clone(); cx.defer(move |cx| me.update(cx, |this, cx| { this.area = a; cx.notify(); })); }
            },
            // A pane being moved: the pointer anywhere in the window (a
            // check of two fields while nothing is pressed).
            move |_, _, window, _| {
                let m = me2.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| if phase == DispatchPhase::Bubble {
                    let Some(m) = m.upgrade() else { return };
                    if m.read(cx).press.is_none() { return; }
                    if e.pressed_button != Some(MouseButton::Left) { m.update(cx, |this, cx| { this.press = None; if this.drag.take().is_some() { this.ghost = None; cx.notify(); } }); return; }
                    m.update(cx, |this, cx| this.drag_move(f32::from(e.position.x), f32::from(e.position.y), cx));
                });
                let m = me2.clone();
                window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                    let Some(m) = m.upgrade() else { return };
                    if m.read(cx).press.is_none() { return; }
                    m.update(cx, |this, cx| this.drag_end(f32::from(e.position.x), f32::from(e.position.y), window, cx));
                });
            },
        ).absolute().size_full();
        // Full screen: that pane alone, over the whole window (the web's
        // .pane-full: fixed, inset 0), the grid not drawn under it.
        if let Some(full) = self.full.clone() {
            let leaf = leaves.iter().find(|l| l.key.as_deref() == Some(full.as_str())).map(|l| (l.id, l.pinned));
            if let Some((id, pinned)) = leaf {
                let vp = window.viewport_size();
                let f = self.head_state(id, pinned, f32::from(vp.width), true);
                let pane = self.frame(&full, f, window, cx);
                let cover = deferred(anchored().position(point(px(0.), px(0.))).child(
                    div().id("pane-full").occlude().w(vp.width).h(vp.height).bg(T::bg()).child(pane)));
                return div().id("workbench").relative().size_full().child(measure).child(cover).into_any_element();
            }
            self.full = None;
        }
        let mut grid = div().relative().w(px(gw)).h(px(ah));
        for l in leaves {
            let id = l.id;
            // The leaf is the pane's panel itself (one element less per pane).
            let leaf = match &l.key {
                Some(k) => { let f = self.head_state(id, l.pinned, l.w, false); self.frame(k, f, window, cx) }
                None => self.empty_leaf(id, &open, cx),
            };
            grid = grid.child(leaf.id(("leaf", id)).absolute().left(px(l.x)).top(px(l.y)).w(px(l.w)).h(px(l.h))
                .children(self.drag_outline(id))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.focus_pane(id, cx))));
        }
        for (n, g) in gutters.iter().enumerate() {
            let (split, index, row, span, locked) = (g.split, g.index, g.dir == Dir::Row, g.span, g.locked);
            grid = grid.child(div().id(("gutter", n)).absolute().left(px(g.x)).top(px(g.y)).w(px(g.w)).h(px(g.h))
                .when(!locked, |d| d.when(row, |d| d.cursor_col_resize()).when(!row, |d| d.cursor_row_resize()).hover(|s| s.bg(T::accent_edge())))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if locked { return; }
                    if e.click_count == 2 { this.mosaic().even(split, index); return this.changed(cx); }
                    let shares = this.mosaic().shares(split).unwrap_or_default();
                    let (a0, b0) = (shares.get(index).copied().unwrap_or(50.), shares.get(index + 1).copied().unwrap_or(50.));
                    let from = f32::from(if row { e.position.x } else { e.position.y });
                    this.gutter = Some(GutterDrag { split, index, a0, b0, total: span - crundi_core::mosaic::GUTTER, row, from });
                })));
        }
        // The card that follows the pointer while a pane moves.
        let card = self.drag.zip(self.ghost.clone()).and_then(|(d, g)| leaves.iter().find(|l| l.id == d.src).map(|l| (d, g, l.w))).map(|(d, g, lw)| {
            deferred(anchored().position(point(px(d.x - d.dx), px(d.y - d.dy))).child(
                g.cached(StyleRefinement::default().w(px(lw)).h(T::PANE_HEAD)))).with_priority(2)
        });
        div().id("workbench").relative().size_full()
            .child(measure)
            .when(self.drag.is_some(), |d| d.cursor_grabbing())
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| if this.gutter.is_some() { this.pointer_move(f32::from(e.position.x), f32::from(e.position.y), cx) }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.pointer_up(cx)))
            .child(div().id("wb-grid").size_full().overflow_x_scroll().track_scroll(&self.scroll).child(grid))
            .children(card)
            .into_any_element()
    }
}

/// The workbench tab and its "+" (the area's registration).
pub fn init(cx: &mut App) {
    // A new pane's launcher (F18): the workbench's own kind.
    register_kind(cx, PaneKind {
        prefix: "pend:",
        chat: false,
        make: Rc::new(|key, _, _, cx| {
            let k = key.to_string();
            PaneParts {
                body: { let v = cx.new(|cx| launcher::Launcher::new(&k, cx)); Rc::new(move |_| v.clone().cached(StyleRefinement::default().size_full()).into_any_element()) },
                info: Rc::new(|_| HeadInfo { title: "New terminal".into(), long: String::new(), state: String::new() }),
                watch: Box::new(|_| vec![]),
            }
        }),
        close: None,
        zoom: None,
    });
    // The "+" menu: a layer over the window (it shows over any tab).
    add_menu::init(cx);
    crundi_shell::register(cx, crundi_shell::Area::new("workbench")
        .tab("workbench", |stores, window, cx| {
            let w = cx.new(|cx| Workbench::new(stores.clone(), cx));
            // The kinds' areas place their panes now that there is a workbench
            // (the chats the state listed before it was made).
            let (s, h) = (stores.clone(), window.window_handle());
            cx.defer(move |cx| { let _ = h.update(cx, |_, window, cx| { crundi_shell::run_action(&s, "workbench-ready", window, cx); }); });
            w.into()
        })
        // The project bar's "+" (F77).
        .action("add-menu", |_, window, cx| add_menu::toggle(window, cx))
        .on_project(|_, alias, cx| if let Some(w) = workbench(cx) { w.update(cx, |w, cx| w.set_project(alias, cx)); })
        // The driver's head and leaf actions, as a person's clicks: ×
        // (twice: the first arms "Sure?"), split, A- / A+, an empty leaf's
        // chip, full screen, pin, the double-click's maximise, a rename, the
        // narrow head's "…", the "+" menu and its items (add-pane).
        .test_ui(|_, name, a, window, cx| {
            let s = |i: usize| a.get(i).cloned().unwrap_or_default();
            match name {
                "add-menu" => { add_menu::set_open(s(0) != "false", window, cx); return Some(serde_json::json!({ "ok": true })); }
                "add-pane" => { add_pane(&s(0), window, cx); return Some(serde_json::json!({ "ok": true })); }
                _ => {}
            }
            let w = workbench(cx)?;
            let leaf_of = |w: &Entity<Workbench>, key: &str, cx: &mut App| -> Option<u32> { if let Ok(n) = key.parse::<u32>() { return Some(n); } w.update(cx, |w, _| w.mosaic().leaf_of(key)) };
            let head_at = |w: &Entity<Workbench>, key: &str, cx: &mut App| -> Option<Entity<PaneHead>> {
                let l = leaf_of(w, key, cx)?;
                w.update(cx, |w, _| w.key_of(l).and_then(|k| w.head_of(&k)))
            };
            match name {
                "pane-close" => { let l = leaf_of(&w, &s(0), cx)?; w.update(cx, |w, cx| w.pane_close(l, cx)); }
                "pane-split" => { let l = leaf_of(&w, &s(0), cx)?; let down = s(1) == "true" || s(1) == "down"; w.update(cx, |w, cx| w.split(l, down, cx)); }
                "pane-zoom" => { let l = leaf_of(&w, &s(0), cx)?; let d: i32 = s(1).parse().unwrap_or(0); w.update(cx, |w, cx| w.pane_zoom(l, d, cx)); }
                "pane-full" => { let l = leaf_of(&w, &s(0), cx)?; w.update(cx, |w, cx| w.pane_full(l, cx)); }
                "pane-pin" => { let l = leaf_of(&w, &s(0), cx)?; w.update(cx, |w, cx| w.pane_pin(l, cx)); }
                "pane-maximize" => { let l = leaf_of(&w, &s(0), cx)?; w.update(cx, |w, cx| w.pane_maximize(l, cx)); }
                "pane-rename" => { let h = head_at(&w, &s(0), cx)?; let t = s(1); h.update(cx, |h, cx| h.rename_to(&t, cx)); }
                "pane-menu" => { let h = head_at(&w, &s(0), cx)?; let open = s(1) != "false"; h.update(cx, |h, cx| h.set_menu(open, cx)); }
                "leaf-chip" => { let l: u32 = s(0).parse().ok()?; let k = s(1); Workbench::chip(&w, l, &k, window, cx); }
                _ => return None,
            }
            Some(serde_json::json!({ "ok": true }))
        })
        .state(|stores, _, cx| {
            let add = add_menu::props(cx);
            let last = launcher::last_mode(&stores.projects.read(cx).open);
            let Some(w) = workbench(cx) else { return serde_json::json!({ "addMenu": add, "launchLast": last }) };
            let (keys, focused, heads, head_props, leaves, full, drag) = w.update(cx, |w, cx| {
                let heads: serde_json::Map<String, serde_json::Value> = w.panes.iter().filter_map(|(k, p)| p.head.read(cx).drawn_title().map(|t| (k.clone(), t.into()))).collect();
                let props: serde_json::Map<String, serde_json::Value> = w.panes.iter().map(|(k, p)| (k.clone(), p.head.read(cx).props())).collect();
                let armed = w.armed;
                let rects = w.leaves();
                let leaves: Vec<serde_json::Value> = rects.iter().map(|l| {
                    let (x, y, lw, lh) = w.window_rect(l);
                    let narrow = crundi_core::head::narrow(l.w);
                    serde_json::json!({ "id": l.id, "key": l.key, "armed": armed == l.id, "pinned": l.pinned, "narrow": narrow, "x": x, "y": y, "w": lw, "h": lh })
                }).collect();
                let drag = w.drag.map(|d| serde_json::json!({ "src": d.src, "target": d.target }));
                (w.keys(), w.focused_key(), heads, props, leaves, w.full.clone(), drag)
            });
            // "heads": each pane head's title as it last drew it (F29);
            // "headProps": its tags, buttons, lock, "…" menu and rename box;
            // "leaves": every leaf (empty ones too), its pane, its ×'s
            // "Sure?", pinned, narrow, its rect in the window; "full": the
            // pane over the window; "drag": a pane being moved; "addMenu":
            // the "+" menu; "launchLast": the launcher's last-used mode.
            serde_json::json!({ "panes": keys, "focusedPane": focused, "heads": heads, "headProps": head_props, "leaves": leaves, "full": full, "drag": drag, "addMenu": add, "launchLast": last })
        }));
}

#[cfg(test)]
mod tests {
    use super::empty_chips;
    #[test]
    fn chips_leave_out_what_is_open() {
        let chips = empty_chips(&["git".into(), "notes".into()]);
        assert!(!chips.iter().any(|(k, _)| *k == "git"));
        assert!(chips.iter().any(|(k, _)| *k == "notes"));
        assert_eq!(chips.first().unwrap().1, "Terminal");
        assert_eq!(chips.last().unwrap().1, "Panel");
    }
}
