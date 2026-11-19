//! The workbench (ARCHITECTURE.md, "The view tree"): the pane host. The
//! Workbench view is layout only: the mosaic's rectangles (core::mosaic, the
//! web's), the gutters, empty panes, and each pane as two cached views side
//! by side, its head (PaneHead) and its body (the pane kind's view). A
//! pane's change redraws that pane; the Workbench re-renders only when the
//! layout changes (a pane placed or closed, a gutter dragged, the area
//! resized, focus, hover over a head's split controls).
//!
//! A pane kind registers once (`register_kind`): its key prefix ("live:"
//! for a chat), how its body and head are made, what A- / A+ and × do.
//! The layout per project is saved (session.json's `layouts`, as native-gpui).

mod head;

pub use head::{HeadInfo, PaneHead};

use crundi_core::{mosaic::{Dir, Mosaic}, store};
use crundi_shell::Stores;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use std::{collections::HashMap, rc::Rc, time::Duration};

/// What a pane kind makes for one pane.
pub struct PaneParts {
    pub body: AnyView,
    /// The head's title and state, read by the head when it renders.
    pub info: Rc<dyn Fn(&App) -> HeadInfo>,
    /// The head observes what its info reads (the chat's store).
    pub watch: Box<dyn FnOnce(&mut Context<PaneHead>) -> Vec<Subscription>>,
}

pub type MakePane = Rc<dyn Fn(&str, &Stores, &mut Window, &mut App) -> PaneParts>;
/// × on a pane: done(true) when it may go (the server closed the chat), else
/// done(false) and the pane stays (the kind said why).
pub type ClosePane = Rc<dyn Fn(&str, &mut App, Box<dyn FnOnce(bool, &mut App)>)>;
pub type ZoomPane = Rc<dyn Fn(&str, i32, &mut App)>;

#[derive(Clone)]
pub struct PaneKind {
    pub prefix: &'static str,
    /// A chat's head (dot, badge, A- ↺ A+, full screen, the two-click ×).
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
    body: AnyView,
}

struct Handle(WeakEntity<Workbench>);
impl Global for Handle {}

/// The workbench view (for handlers: the chat area places its panes).
pub fn workbench(cx: &App) -> Option<Entity<Workbench>> { cx.try_global::<Handle>().and_then(|h| h.0.upgrade()) }

/// A gutter being dragged: its split and index, the pair's shares and px at
/// the press, along a row or not, and where the press was.
struct GutterDrag { split: u32, index: usize, a0: f32, b0: f32, total: f32, row: bool, from: f32 }

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
}

impl Workbench {
    pub fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
        cx.set_global(Handle(cx.entity().downgrade()));
        let project = stores.projects.read(cx).open.clone();
        Workbench { stores, project, layouts: HashMap::new(), panes: HashMap::new(), area: (0., 0.), focused: 0, armed: 0, arm_gen: 0, full: None, gutter: None, scroll: ScrollHandle::new(), frames: HashMap::new() }
    }

    /// The open project's mosaic (loaded from the saved layouts the first time).
    pub fn mosaic(&mut self) -> &mut Mosaic {
        let p = self.project.clone();
        self.layouts.entry(p.clone()).or_insert_with(|| store::load().layouts.get(&p).and_then(|v| serde_json::from_value::<Mosaic>(v.clone()).ok()).unwrap_or_default())
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
        cx.notify();
    }

    /// Put a pane in the layout (an empty leaf, else a new one to the right).
    pub fn place(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.mosaic().leaf_of(key).is_some() { return; }
        let leaf = self.mosaic().place(key);
        self.focused = leaf;
        self.changed(cx);
    }

    /// Keep only the panes `keep` says.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool, cx: &mut Context<Self>) {
        let before = self.mosaic().keys();
        self.mosaic().retain(&keep);
        if self.mosaic().keys() != before { self.changed(cx); }
    }

    pub fn keys(&mut self) -> Vec<String> { self.mosaic().keys() }

    /// The focused pane's key (else the first pane's).
    pub fn focused_key(&mut self) -> Option<String> {
        let (w, h) = self.area;
        let leaves = self.mosaic().layout(w.max(1.), h.max(1.)).0;
        leaves.iter().find(|l| l.id == self.focused).and_then(|l| l.key.clone()).or_else(|| leaves.iter().find_map(|l| l.key.clone()))
    }

    pub fn focus_key(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(l) = self.mosaic().leaf_of(key) { if self.focused != l { self.focused = l; cx.notify(); } }
    }

    fn key_of(&mut self, leaf: u32) -> Option<String> {
        let (w, h) = self.area;
        self.mosaic().layout(w.max(1.), h.max(1.)).0.into_iter().find(|l| l.id == leaf).and_then(|l| l.key)
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
        cx.notify();
    }

    fn pane_maximize(&mut self, leaf: u32, cx: &mut Context<Self>) {
        self.mosaic().maximize(leaf);
        self.changed(cx);
    }

    fn pane_pin(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let (w, h) = self.area;
        let r = self.mosaic().layout(w.max(1.), h.max(1.)).0.into_iter().find(|l| l.id == leaf);
        if let Some(r) = r { self.mosaic().toggle_pin(leaf, r.w, r.h); self.changed(cx); }
    }

    fn pane_zoom(&mut self, leaf: u32, d: i32, cx: &mut Context<Self>) {
        let Some(key) = self.key_of(leaf) else { return };
        if let Some(z) = kind_of(cx, &key).and_then(|k| k.zoom) { z(&key, d, cx); }
    }

    /// A pane's views, made the first time it is laid out (in a handler: the
    /// layout pass that found it, before its frame is drawn).
    fn pane(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<(Entity<PaneHead>, AnyView)> {
        if !self.panes.contains_key(key) {
            let kind = kind_of(cx, key)?;
            let parts = (kind.make)(key, &self.stores, window, cx);
            let me = cx.entity().downgrade();
            let k = key.to_string();
            let head = cx.new(|cx| PaneHead::new(k, kind.chat, parts.info, me, parts.watch, cx));
            self.panes.insert(key.to_string(), Pane { head, body: parts.body });
        }
        self.panes.get(key).map(|p| (p.head.clone(), p.body.clone()))
    }

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
            let icon = match k { "terminal" => IconName::Terminal, "files" => IconName::Folder, "git" => IconName::GitBranch, "kanban" => IconName::Kanban, "notes" => IconName::FileText, "mindmap" => IconName::Network, "media" => IconName::Images, "factory" => IconName::Factory, _ => IconName::PanelsTopLeft };
            let s = self.stores.clone();
            chips = chips.child(w::leaf_chip(SharedString::from(format!("chip-{k}-{id}")), icon, label, format!("Add {label}"))
                .on_click(move |_, window, cx| { crundi_shell::run_action(&s, &format!("wb-chip:{k}"), window, cx); }));
        }
        // The web's .mosaic-empty: 3px in, a 2px dashed border, the message,
        // the chips and the actions 10 apart, centred.
        div().size_full().p(T::LEAF_INSET).child(
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

    /// A pane: its head and body side by side, each cached (rule 3).
    fn frame(&mut self, key: &str, f: head::Frame, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some((head, body)) = self.pane(key, window, cx) else {
            return w::empty("Not built yet in the v2 client.").into_any_element();
        };
        // The head's frame (armed, pinned, narrow, focused) is the
        // workbench's: the head reads it, and is notified here only when its
        // own frame changed (F11: a focus change redraws two heads, not all).
        // A notify while the window draws marks the head dirty for this
        // draw, before its cached element is laid out, without asking for
        // another frame.
        if self.frames.insert(key.to_string(), f) != Some(f) { head.update(cx, |_, cx| cx.notify()); }
        let hh = if f.narrow { T::PANE_HEAD_NARROW } else { T::PANE_HEAD };
        // A wheel inside a pane stays in it (the workbench's sideways scroll
        // would otherwise move with a log: native-gpui's 19 Nov fix).
        let body = div().relative().flex_1().min_h_0().w_full().overflow_hidden()
            .on_scroll(|_, _, cx| cx.stop_propagation())
            // The body is the kind's layout view (uncached: its parts are the cached leaves).
            .child(div().size_full().child(body));
        w::panel(head.cached(StyleRefinement::default().w_full().h(hh).flex_none()), body)
            .size_full()
            .when(f.focused && !f.full, |d| d.border_color(T::accent()))
            .into_any_element()
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

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("workbench");
        let (aw, ah) = self.area;
        let gw = self.mosaic().content_width(aw);
        let (leaves, gutters) = self.mosaic().layout(gw, ah);
        let open: Vec<String> = leaves.iter().filter_map(|l| l.key.as_deref()).filter_map(|k| k.strip_prefix("panel:").map(|r| r.split(':').next().unwrap_or("").to_string())).collect();
        // Panes no longer laid out drop their views.
        let keys: Vec<String> = leaves.iter().filter_map(|l| l.key.clone()).collect();
        self.panes.retain(|k, _| keys.contains(k));
        self.frames.retain(|k, _| keys.contains(k));
        let me = cx.entity();
        let measure = canvas(
            move |b, _, cx| {
                let a = (f32::from(b.size.width), f32::from(b.size.height));
                // (After the frame: a notify while drawing would not draw again.)
                if me.read(cx).area != a { let me = me.clone(); cx.defer(move |cx| me.update(cx, |this, cx| { this.area = a; cx.notify(); })); }
            },
            |_, _, _, _| {},
        ).absolute().size_full();
        // Full screen: that pane alone, over the grid.
        if let Some(full) = self.full.clone() {
            let leaf = leaves.iter().find(|l| l.key.as_deref() == Some(full.as_str())).map(|l| (l.id, l.pinned));
            if let Some((id, pinned)) = leaf {
                let f = self.head_state(id, pinned, aw, true);
                return div().id("workbench").relative().size_full().child(measure).child(div().absolute().inset_0().p(T::SP_6).child(self.frame(&full, f, window, cx))).into_any_element();
            }
            self.full = None;
        }
        let mut grid = div().relative().w(px(gw)).h(px(ah));
        for l in &leaves {
            let id = l.id;
            let inner = match &l.key {
                Some(k) => { let f = self.head_state(id, l.pinned, l.w, false); self.frame(k, f, window, cx) }
                None => self.empty_leaf(id, &open, cx).into_any_element(),
            };
            grid = grid.child(div().id(("leaf", id)).absolute().left(px(l.x)).top(px(l.y)).w(px(l.w)).h(px(l.h))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.focus_pane(id, cx)))
                .child(inner));
        }
        for (n, g) in gutters.into_iter().enumerate() {
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
        div().id("workbench").relative().size_full()
            .child(measure)
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| if this.gutter.is_some() { this.pointer_move(f32::from(e.position.x), f32::from(e.position.y), cx) }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.pointer_up(cx)))
            .child(div().id("wb-grid").size_full().overflow_x_scroll().track_scroll(&self.scroll).child(grid))
            .into_any_element()
    }
}

/// The workbench tab and its "+" (the area's registration).
pub fn init(cx: &mut App) {
    crundi_shell::register(cx, crundi_shell::Area::new("workbench")
        .tab("workbench", |stores, window, cx| {
            let w = cx.new(|cx| Workbench::new(stores.clone(), cx));
            // The kinds' areas place their panes now that there is a workbench
            // (the chats the state listed before it was made).
            let (s, h) = (stores.clone(), window.window_handle());
            cx.defer(move |cx| { let _ = h.update(cx, |_, window, cx| { crundi_shell::run_action(&s, "workbench-ready", window, cx); }); });
            w.into()
        })
        .on_project(|_, alias, cx| if let Some(w) = workbench(cx) { w.update(cx, |w, cx| w.set_project(alias, cx)); })
        .state(|_, _, cx| {
            let Some(w) = workbench(cx) else { return serde_json::json!({}) };
            let (keys, focused) = w.update(cx, |w, _| (w.keys(), w.focused_key()));
            serde_json::json!({ "panes": keys, "focusedPane": focused })
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
