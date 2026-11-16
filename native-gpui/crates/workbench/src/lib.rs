//! The workbench's pane host (Workspace's, AREAS.md; phases 2.1-2.4 build it
//! out): the project's panes laid out by the mosaic (crundi_core::mosaic,
//! the Slint client's rules), each drawn by the part that registered its
//! kind. A pane's key says its kind: "live:<chat>" a chat, "pend:<n>" a new
//! terminal's launcher, "panel:<kind>" a panel (Files, Git, Kanban…).
//!
//! The foundation's version draws the layout and each pane's frame; the
//! header's buttons, gutters, moving panes and the "+" menu are 2.1-2.4.

use crundi_core::mosaic::Mosaic;
use crundi_theme::T;
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};
use std::{collections::HashMap, rc::Rc};

type RenderPane = Rc<dyn Fn(&str, &mut Window, &mut App) -> AnyElement>;
type Title = Rc<dyn Fn(&str, &App) -> (String, String)>;

/// A kind of pane, registered by the part that draws it.
#[derive(Clone)]
pub struct PaneKind {
    /// The key's prefix ("live:", "pend:", "panel:files"…).
    pub prefix: &'static str,
    /// The pane's body.
    pub render: RenderPane,
    /// Its title and state ("idle", "working", "needs-input", "error", "exited").
    pub title: Title,
}

#[derive(Default)]
pub struct Kinds(pub Vec<PaneKind>);
impl Global for Kinds {}

/// Register a pane kind (once, at start).
pub fn register_kind(cx: &mut App, kind: PaneKind) {
    if !cx.has_global::<Kinds>() { cx.set_global(Kinds::default()); }
    cx.global_mut::<Kinds>().0.push(kind);
}

fn kind_of(cx: &App, key: &str) -> Option<PaneKind> {
    cx.try_global::<Kinds>()?.0.iter().filter(|k| key.starts_with(k.prefix)).max_by_key(|k| k.prefix.len()).cloned()
}

/// The workbench: one mosaic per project.
pub struct Workbench {
    pub layouts: HashMap<String, Mosaic>,
    pub project: String,
    pub focused: u32,
    /// The grid's room (from the last layout).
    pub area: (f32, f32),
}

pub struct WorkbenchHandle(pub Entity<Workbench>);
impl Global for WorkbenchHandle {}

impl Workbench {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Workbench { layouts: HashMap::new(), project: String::new(), focused: 0, area: (0., 0.) }
    }

    pub fn mosaic(&mut self) -> &mut Mosaic {
        self.layouts.entry(self.project.clone()).or_default()
    }

    /// Put a pane in the layout (added to the right, as the web's); its leaf.
    pub fn place(&mut self, key: &str, cx: &mut Context<Self>) -> u32 {
        let leaf = self.mosaic().place(key);
        self.focused = leaf;
        cx.notify();
        leaf
    }

    /// Close a pane by its leaf.
    pub fn close(&mut self, leaf: u32, cx: &mut Context<Self>) {
        self.mosaic().close(leaf);
        cx.notify();
    }

    /// Keep only the panes whose key passes (the project's live chats…).
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool, cx: &mut Context<Self>) {
        self.mosaic().retain(keep);
        cx.notify();
    }

    /// The panes as the test driver lists them (the Slint client's names).
    pub fn panes(&mut self, cx: &App) -> Vec<Value> {
        let (aw, ah) = self.area;
        let focused = self.focused;
        let m = self.mosaic().clone();
        let cw = m.content_width(aw);
        m.layout(cw, ah).0.into_iter().map(|l| {
            let key = l.key.clone().unwrap_or_default();
            let (title, state) = kind_of(cx, &key).map(|k| (k.title)(&key, cx)).unwrap_or_default();
            let kind = if key.is_empty() { "empty" } else if key.starts_with("live:") { "chat" } else if key.starts_with("pend:") { "launcher" } else if key.starts_with("panel:") { "panel" } else { "other" };
            json!({ "id": l.id, "key": key, "kind": kind, "title": title, "state": state, "focused": l.id == focused, "x": l.x, "y": l.y, "w": l.w, "h": l.h, "pinned": l.pinned })
        }).collect()
    }
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity().downgrade();
        let (aw, ah) = self.area;
        let m = self.mosaic().clone();
        let cw = m.content_width(aw);
        let (leaves, _gutters) = m.layout(cw, ah);
        let mut grid = div().relative().size_full();
        for l in leaves {
            let key = l.key.clone().unwrap_or_default();
            let pane = match kind_of(cx, &key) {
                Some(k) => {
                    let (title, state) = (k.title)(&key, cx);
                    w::panel(w::pane_head(&state, title), (k.render)(&key, window, cx))
                }
                None if key.is_empty() => w::panel(w::pane_head("idle", "Empty pane"), w::empty("Open a chat or a panel here.")),
                None => w::panel(w::pane_head("idle", key.clone()), w::empty("Not built yet in the GPUI client.")),
            };
            grid = grid.child(div().absolute().left(px(l.x)).top(px(l.y)).w(px(l.w)).h(px(l.h)).p(T::GUTTER / 2.).child(pane));
        }
        if m.keys().is_empty() {
            grid = grid.child(w::empty("No panes yet: start a chat with the + by Workbench."));
        }
        div().size_full().p(T::GUTTER / 2.)
            .child(canvas(
                move |bounds, _, cx| {
                    let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let _ = me.update(cx, |this, cx| if this.area != (bw, bh) { this.area = (bw, bh); cx.notify(); });
                },
                |_, _, _, _| {},
            ).absolute().size_full())
            .child(grid)
    }
}

/// The window's workbench (None before the window opens).
pub fn workbench(cx: &App) -> Option<Entity<Workbench>> {
    cx.try_global::<WorkbenchHandle>().map(|h| h.0.clone())
}

/// Make the workbench and register its tab (once, at start). Its panes'
/// kinds are registered by the parts that draw them (`register_kind`).
pub fn init(cx: &mut App) {
    let wb = cx.new(Workbench::new);
    cx.set_global(WorkbenchHandle(wb.clone()));
    crundi_shell::register(cx, crundi_shell::Area::new("workbench")
        .tabs(&["workbench"], move |_, _, _| wb.clone().into_any_element())
        .on_project(|alias, cx| if let Some(wb) = workbench(cx) { let a = alias.to_string(); wb.update(cx, |w, cx| { w.project = a; cx.notify(); }); })
        .props(|_, cx| {
            let panes = workbench(cx).map(|wb| wb.update(cx, |w, cx| w.panes(cx))).unwrap_or_default();
            serde_json::json!({ "panes": panes })
        }));
}
