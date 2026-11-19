//! The workbench's pane host (Workspace's, AREAS.md; phases 2.1-2.4): the
//! project's panes laid out by the mosaic (crundi_core::mosaic, the Slint
//! client's rules), each drawn by the part that registered its kind. A
//! pane's key says its kind: "live:<chat>" a chat, "pend:<n>" a new
//! terminal's launcher, "panel:<kind>" a panel (Files, Git, Kanban…).
//!
//! This file: the state and every action (the header's rename, text size,
//! pin, full screen, the two-click ×; presets, Fit, split, gutters, moving
//! panes, the empty pane's chips, the launcher), saved as the Slint client
//! saves them (session.json: layouts, font, font_default_chat). view.rs
//! draws it; head.rs is the pane header; menu.rs the "+" menu.

mod head;
mod menu;
mod view;

use crundi_core::{head as rules, mosaic::{Dir, Dropped, LeafRect, Mosaic}, store};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, rc::Rc, time::{Duration, Instant}};

type RenderPane = Rc<dyn Fn(&str, &mut Window, &mut App) -> AnyElement>;
type Title = Rc<dyn Fn(&str, &App) -> (String, String)>;
type PaneProps = Rc<dyn Fn(&str, &App) -> Value>;

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
pub struct Kinds(pub Vec<PaneKind>, pub Vec<(&'static str, PaneProps)>);
impl Global for Kinds {}

/// Register a pane kind (once, at start).
pub fn register_kind(cx: &mut App, kind: PaneKind) {
    if !cx.has_global::<Kinds>() { cx.set_global(Kinds::default()); }
    cx.global_mut::<Kinds>().0.push(kind);
}

/// Add fields to the driver's view of every pane whose key starts with
/// `prefix` (the composer's draft, the log's dock…): the object is merged
/// into that pane's json in "props".
pub fn register_pane_props(cx: &mut App, prefix: &'static str, f: impl Fn(&str, &App) -> Value + 'static) {
    if !cx.has_global::<Kinds>() { cx.set_global(Kinds::default()); }
    cx.global_mut::<Kinds>().1.push((prefix, Rc::new(f)));
}

/// Which conversation a launch starts (2.22, the resume picker: Operations'
/// crundi_chat_log::resume::choose): (project, skip, terminal, cx, go).
/// It calls `go(mode, resume id, cx)` once ("new" | "resume" | "compact"),
/// or never (cancelled), and returns false when it ignores the launch (its
/// list is loading, its picker open). Added by Operations (AREAS.md).
pub type LaunchChooser = Rc<dyn Fn(&str, bool, bool, &mut App, Rc<dyn Fn(&'static str, String, &mut App)>) -> bool>;
struct Chooser(LaunchChooser);
impl Global for Chooser {}

/// Set the launch chooser (once, at start).
pub fn set_launch_chooser(cx: &mut App, f: LaunchChooser) { cx.set_global(Chooser(f)); }

fn kind_of(cx: &App, key: &str) -> Option<PaneKind> {
    cx.try_global::<Kinds>()?.0.iter().filter(|k| key.starts_with(k.prefix)).max_by_key(|k| k.prefix.len()).cloned()
}

/// What the header's lock does (the chat secrets dialog is the composer's,
/// 2.12): Planning sets it.
#[derive(Clone)]
pub struct OnSecrets(pub Rc<dyn Fn(&str, &mut Window, &mut App)>);
impl Global for OnSecrets {}
pub fn set_on_secrets(cx: &mut App, f: impl Fn(&str, &mut Window, &mut App) + 'static) { cx.set_global(OnSecrets(Rc::new(f))); }

/// An empty pane's chips (the web's emptyLeafHtml): Terminal, the panels
/// not in the workbench yet (Notes always), Panel; in rows that fit the pane
/// (a chip is its icon and padding plus about 6.6px a character, as the
/// phase 2 test reads them).
pub(crate) fn empty_chips(open: &[String], width: f32) -> Vec<Vec<(&'static str, &'static str)>> {
    let mut items: Vec<(&'static str, &'static str)> = vec![("terminal", "Terminal")];
    for (k, l) in [("files", "Files"), ("git", "Git"), ("kanban", "Kanban"), ("notes", "Notes"), ("mindmap", "Mindmap"), ("media", "Media"), ("factory", "Factory")] {
        if k == "notes" || !open.iter().any(|o| o == k) { items.push((k, l)); }
    }
    items.push(("widget", "Panel"));
    let room = (width - 40.0).max(120.0);
    let mut rows: Vec<Vec<(&'static str, &'static str)>> = vec![vec![]];
    let mut used = 0.0;
    for (id, label) in items {
        let w = chip_w(label);
        if used > 0.0 && used + 6.0 + w > room { rows.push(vec![]); used = 0.0; }
        used += if used > 0.0 { 6.0 + w } else { w };
        rows.last_mut().unwrap().push((id, label));
    }
    rows
}

/// A chip's width for its label.
pub(crate) fn chip_w(label: &str) -> f32 { f32::from(crundi_theme::T::WS_CHIP_BASE) + label.chars().count() as f32 * crundi_theme::T::WS_CHIP_CHAR }

/// The pane kinds a key can be.
pub fn pane_kind(key: &str) -> &'static str {
    if key.is_empty() { "empty" } else if key.starts_with("live:") { "chat" } else if key.starts_with("term:") { "terminal" } else if key.starts_with("pend:") { "launcher" } else if key.starts_with("panel:") { "panel" } else { "other" }
}

fn chat_of(key: &str) -> Option<&str> { key.strip_prefix("live:") }
/// A Claude terminal's id (3.2), from its pane key "term:<id>".
pub fn term_of(key: &str) -> Option<&str> { key.strip_prefix("term:") }
/// A chat's or a terminal's id (their text size and close are kept by it).
fn item_of(key: &str) -> Option<&str> { chat_of(key).or_else(|| term_of(key)) }

/// A header press that becomes a drag once it moves 6px.
#[derive(Clone, Copy)]
struct Press { leaf: u32, x: f32, y: f32 }
/// A pane being dragged by its header (window coordinates).
#[derive(Clone, Copy)]
pub(crate) struct Drag { pub src: u32, pub target: u32, pub x: f32, pub y: f32, pub dx: f32, pub dy: f32 }
/// A gutter being dragged: the shares at the press and the pair's px.
#[derive(Clone, Copy)]
struct GutterDrag { split: u32, index: usize, a0: f32, b0: f32, total: f32, start: f32, row: bool }

/// One pane as drawn and as the driver sees it.
#[derive(Clone)]
pub(crate) struct Info {
    pub l: LeafRect,
    pub key: String,
    pub kind: &'static str,
    pub title: String,
    pub short: String,
    pub cap: f32,
    pub state: String,
    pub focused: bool,
    pub fs: f32,
    pub armed: bool,
    pub narrow: bool,
    pub menu: bool,
    pub drop: bool,
    pub secrets: bool,
    pub secrets_n: usize,
    pub launch_busy: i32,
    /// A parked chat (2.23): the header keeps only grip, dot, title, CHAT, pin and ×.
    pub parked: bool,
    /// A factory's Manager chat (2.20): the Factory tag.
    pub factory: bool,
    /// An empty pane's chips, in rows that fit it.
    pub chips: Vec<Vec<(&'static str, &'static str)>>,
}

/// The workbench: one mosaic per project.
pub struct Workbench {
    pub layouts: HashMap<String, Mosaic>,
    pub project: String,
    pub focused: u32,
    /// The grid's room (from the last layout), and its top-left in the window.
    pub area: (f32, f32),
    pub(crate) origin: (f32, f32),
    pub(crate) scroll: ScrollHandle,
    pub full: Option<String>,
    pub(crate) armed: u32,
    arm_gen: u64,
    pub(crate) menu: u32,
    pub(crate) renaming: Option<(u32, Entity<w::InputState>)>,
    rename_sub: Option<Subscription>,
    font: HashMap<String, i32>,
    font_default: i32,
    /// Renames made here, until the server's list says the same.
    titles: HashMap<String, String>,
    pending: u32,
    /// Keys put in a leaf here (a launcher's chat) that the server's list
    /// doesn't hold yet: kept until it does (or 15 s).
    protect: HashMap<String, Instant>,
    /// Chats closed here: never placed again.
    closed: HashSet<String>,
    pub(crate) hover: u32,
    press: Option<Press>,
    pub(crate) drag: Option<Drag>,
    gutter: Option<GutterDrag>,
    pub(crate) add_menu: bool,
    pub(crate) layouts_open: bool,
    toast: String,
    toast_gen: u64,
    secrets_cap: bool,
    grants: HashMap<String, Value>,
    pub(crate) launching: HashMap<u32, i32>,
    /// The last create call's body (the driver's "lastCreate").
    last_create: Value,
    pub(crate) window: Option<AnyWindowHandle>,
    pub(crate) win_size: (f32, f32),
    /// The event stream's first state has come: until then no pane goes.
    seen_state: bool,
    /// Keys were typed in the focused pane: scroll it into view.
    pub(crate) reveal: bool,
    /// The sideways scrollbar's thumb being dragged: the pointer's x and the offset at the press.
    pub(crate) sbar: Option<(f32, f32)>,
    /// Each leaf's focus handle (tracked by its container), to know which pane holds the keys.
    pub(crate) leaf_focus: HashMap<u32, FocusHandle>,
    /// Each pane body's own view (view.rs PaneHost), cached by GPUI.
    pub(crate) hosts: HashMap<String, Entity<crate::view::PaneHost>>,
    /// Panes ease to a new place and size (a split, a close, a move, the
    /// window resized) as the web's .mosaic-easing: each leaf's rect as last
    /// drawn, and its running tween (from, to, start). The owner, 18 Nov:
    /// panels and their movement were not animated.
    pub(crate) shown: HashMap<u32, [f32; 4]>,
    pub(crate) tweens: HashMap<u32, ([f32; 4], [f32; 4], std::time::Instant)>,
}

pub struct WorkbenchHandle(pub Entity<Workbench>);
impl Global for WorkbenchHandle {}

impl Workbench {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let saved = store::load();
        Workbench {
            layouts: HashMap::new(), project: String::new(), focused: 0, area: (0., 0.), origin: (0., 0.), scroll: ScrollHandle::new(),
            full: None, armed: 0, arm_gen: 0, menu: 0, renaming: None, rename_sub: None,
            font: saved.font.clone(), font_default: saved.font_default_chat.unwrap_or(rules::FONT_BASE),
            titles: HashMap::new(), pending: 0, protect: HashMap::new(), closed: HashSet::new(),
            hover: 0, press: None, drag: None, gutter: None, add_menu: false, layouts_open: false,
            toast: String::new(), toast_gen: 0, secrets_cap: false, grants: HashMap::new(), launching: HashMap::new(), last_create: Value::Null, window: None, win_size: (0., 0.), seen_state: false, reveal: false, sbar: None, leaf_focus: HashMap::new(), hosts: HashMap::new(), shown: HashMap::new(), tweens: HashMap::new(),
        }
    }

    /// The project's layout (the saved one the first time).
    pub fn mosaic(&mut self) -> &mut Mosaic {
        let p = self.project.clone();
        if !self.layouts.contains_key(&p) {
            let m = store::load().layouts.get(&p).and_then(|v| serde_json::from_value::<Mosaic>(v.clone()).ok()).unwrap_or_default();
            self.layouts.insert(p.clone(), m);
            self.ensure_fonts();
        }
        self.layouts.get_mut(&p).unwrap()
    }

    fn save(&mut self) {
        if self.project.is_empty() { return; }
        let p = self.project.clone();
        if let Some(v) = self.layouts.get(&p).and_then(|m| serde_json::to_value(m).ok()) { store::update(|s| { s.layouts.insert(p, v); }); }
    }

    /// The layout changed: saved, drawn again, and an armed × disarmed (as
    /// the web's full re-render), so a click after it never closes unwarned.
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.armed = 0;
        self.save();
        cx.notify();
    }

    /// A chat seen for the first time starts at the default size and keeps it.
    fn ensure_fonts(&mut self) {
        let Some(m) = self.layouts.get(&self.project) else { return };
        let new: Vec<String> = m.keys().iter().filter_map(|k| item_of(k).map(String::from)).filter(|id| !self.font.contains_key(id)).collect();
        if new.is_empty() { return; }
        let d = self.font_default;
        for id in &new { self.font.insert(id.clone(), d); }
        store::update(|s| { for id in &new { s.font.insert(id.clone(), d); } });
    }

    /// Put a pane in the layout (added to the right, as the web's); its leaf.
    pub fn place(&mut self, key: &str, cx: &mut Context<Self>) -> u32 {
        if item_of(key).is_some_and(|id| self.closed.contains(id)) { return 0; }
        let leaf = self.mosaic().place(key);
        self.focused = leaf;
        self.ensure_fonts();
        self.changed(cx);
        leaf
    }

    /// Close a pane by its leaf (no server call).
    pub fn close(&mut self, leaf: u32, cx: &mut Context<Self>) {
        self.mosaic().close(leaf);
        self.changed(cx);
    }

    /// Keep only the panes whose key passes (the project's live chats…);
    /// a key just put in a leaf here stays until the server lists it.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool, cx: &mut Context<Self>) {
        // Before the first state the chats aren't known: nothing goes (a
        // restart keeps its layout and pins).
        if !self.seen_state && crundi_shell::session::get(cx).terminals.is_empty() { return; }
        let now = Instant::now();
        self.protect.retain(|_, t| now.duration_since(*t) < Duration::from_secs(15));
        let prot: HashSet<String> = self.protect.keys().cloned().collect();
        let before = self.mosaic().keys();
        self.mosaic().retain(|k| keep(k) || prot.contains(k));
        let changed = self.mosaic().keys() != before;
        for k in before { if keep(&k) { self.protect.remove(&k); } }
        let full = self.full.clone();
        if let Some(f) = &full { if self.mosaic().leaf_of(f).is_none() { self.full = None; } }
        // Called on every state from the server (each chat's change): only
        // a pane that went saves the layout and draws the workbench again.
        if changed || self.full != full { self.save(); cx.notify(); }
    }

    fn rect(&mut self, leaf: u32) -> Option<LeafRect> {
        let (aw, ah) = self.grid_size();
        self.mosaic().layout(aw, ah).0.into_iter().find(|l| l.id == leaf)
    }

    /// The grid's content size: as wide as the panes need (wider than the
    /// area, it scrolls sideways, and is 6px less tall for its scrollbar).
    pub(crate) fn grid_size(&mut self) -> (f32, f32) {
        let (aw, ah) = self.area;
        let cw = self.mosaic().content_width(aw);
        (cw, if cw > aw + 0.5 { ah - crundi_core::mosaic::PAD } else { ah })
    }

    // ─── Toasts ───

    pub(crate) fn toast(&mut self, text: &str, error: bool, cx: &mut Context<Self>) {
        self.toast = text.to_string();
        self.toast_gen += 1;
        let g = self.toast_gen;
        if let Some(h) = self.window {
            let t = text.to_string();
            cx.defer(move |cx| { let _ = cx.update_window(h, |_, window, cx| w::toast(window, cx, t, if error { w::Toast::Error } else { w::Toast::Info })); });
        }
        cx.spawn(async move |this: WeakEntity<Workbench>, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(3500)).await;
            let _ = this.update(cx, |w, cx| if w.toast_gen == g { w.toast.clear(); cx.notify(); });
        }).detach();
        cx.notify();
    }

    // ─── Titles, sizes, secrets ───

    fn chat_item(cx: &App, id: &str) -> Option<Value> {
        crundi_shell::session::get(cx).terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(id)).cloned()
    }

    /// A pane's short title, long title and state.
    fn title_of(&self, key: &str, cx: &App) -> (String, String, String) {
        let (kt, ks) = kind_of(cx, key).map(|k| (k.title)(key, cx)).unwrap_or_default();
        if let Some(id) = chat_of(key) {
            let item = Self::chat_item(cx, id);
            let s = |k: &str| item.as_ref().and_then(|c| c.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mut title = s("title");
            let mut long = s("longTitle");
            if let Some(t) = self.titles.get(id) { title = t.clone(); long.clear(); }
            if title.is_empty() { title = if kt.is_empty() { "Chat".into() } else { kt }; }
            let state = item.as_ref().map(rules::chat_state).filter(|s| !s.is_empty()).unwrap_or(if ks.is_empty() { "idle".into() } else { ks });
            return (title, long, state);
        }
        if let Some(id) = term_of(key) {
            // A Claude terminal (3.2): its title (a rename made here first) and state.
            let item = Self::chat_item(cx, id);
            let s = |k: &str| item.as_ref().and_then(|c| c.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mut title = self.titles.get(id).cloned().unwrap_or_else(|| s("title"));
            if title.is_empty() { title = if kt.is_empty() { "Terminal".into() } else { kt }; }
            let st = if s("status") == "exited" { "exited".to_string() } else { s("state") };
            return (title, String::new(), if st.is_empty() { "idle".into() } else { st });
        }
        match pane_kind(key) {
            "empty" => (String::new(), String::new(), String::new()),
            "launcher" => ("New pane".into(), String::new(), String::new()),
            _ => (if kt.is_empty() { key.trim_start_matches("panel:").to_string() } else { kt }, String::new(), ks),
        }
    }

    /// A pane as drawn full screen: the header as wide as the window (the
    /// long title when there is one, never folded).
    pub(crate) fn as_full(&self, mut i: Info, cx: &App) -> Info {
        let (short, long, _) = self.title_of(&i.key, cx);
        let (title, cap) = rules::title_shown(&short, &long, self.win_size.0);
        i.title = title;
        i.cap = cap;
        i.narrow = false;
        i.menu = false;
        i
    }

    fn font_px(&self, id: &str) -> i32 { *self.font.get(id).unwrap_or(&self.font_default) }

    /// Every pane, laid out, as drawn and listed.
    /// The open panes' keys, cheaply (no titles, no JSON): what the other
    /// parts poll every tick.
    pub fn pane_keys(&mut self) -> Vec<String> {
        let (gw, gh) = self.grid_size();
        self.mosaic().layout(gw, gh).0.into_iter().filter_map(|l| l.key).collect()
    }

    /// The chat in the focused pane, else the first chat pane, cheaply.
    pub fn focused_chat(&mut self) -> String {
        let (gw, gh) = self.grid_size();
        let leaves = self.mosaic().layout(gw, gh).0;
        let chat = |k: &Option<String>| k.as_deref().and_then(|k| k.strip_prefix("live:")).map(String::from);
        leaves.iter().find(|l| l.id == self.focused).and_then(|l| chat(&l.key)).or_else(|| leaves.iter().find_map(|l| chat(&l.key))).unwrap_or_default()
    }

    pub(crate) fn infos(&mut self, cx: &App) -> Vec<Info> {
        let (gw, gh) = self.grid_size();
        let leaves = self.mosaic().layout(gw, gh).0;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let open_kinds: Vec<String> = leaves.iter().filter_map(|l| l.key.as_deref()).filter_map(|k| k.strip_prefix("panel:").map(|r| r.split(':').next().unwrap_or("").to_string())).collect();
        leaves.into_iter().map(|l| {
            let key = l.key.clone().unwrap_or_default();
            let kind = pane_kind(&key);
            let (short, long, state) = self.title_of(&key, cx);
            let (title, cap) = rules::title_shown(&short, &long, l.w);
            let id = chat_of(&key).unwrap_or("").to_string();
            let item_id = item_of(&key).unwrap_or("").to_string();
            let collab = Self::chat_item(cx, &id).and_then(|c| c.get("collaborator").cloned()).is_some_and(|v| v.as_bool().unwrap_or(!v.is_null()));
            let n = rules::grant_count(self.grants.get(&id), now);
            let item = Self::chat_item(cx, &id);
            let parked = item.as_ref().is_some_and(|c| c.get("kind").and_then(|k| k.as_str()) == Some("parked"));
            let factory = item.as_ref().is_some_and(rules::factory_manager);
            let chips = if kind == "empty" { empty_chips(&open_kinds, l.w) } else { vec![] };
            // A parked chat's header (2.23) has only grip, dot, title, tag, pin and ×: it never folds.
            let parked = Self::chat_item(cx, &id).is_some_and(|c| c.get("kind").and_then(|k| k.as_str()) == Some("parked"));
            Info {
                focused: l.id == self.focused, fs: if kind == "chat" || kind == "terminal" { rules::font_scale(self.font_px(&item_id)) } else { 1.0 },
                armed: self.armed == l.id, narrow: rules::narrow(l.w) && !parked, menu: self.menu == l.id && rules::narrow(l.w) && !parked,
                drop: self.drag.is_some_and(|d| d.target == l.id && d.src != l.id),
                secrets: kind == "chat" && self.secrets_cap && !collab, secrets_n: n,
                launch_busy: *self.launching.get(&l.id).unwrap_or(&0),
                parked, factory, chips,
                key, kind, title, short, cap, state, l,
            }
        }).collect()
    }

    /// The panes as the test driver lists them (the Slint client's names).
    pub fn panes(&mut self, cx: &App) -> Vec<Value> {
        let extra: Vec<(&'static str, PaneProps)> = cx.try_global::<Kinds>().map(|k| k.1.clone()).unwrap_or_default();
        self.infos(cx).into_iter().map(|i| {
            let mut v = json!({
                "id": i.l.id, "key": i.key, "kind": i.kind, "title": i.title, "shortTitle": i.short, "titleMax": i.cap,
                "state": i.state, "focused": i.focused, "x": i.l.x, "y": i.l.y, "w": i.l.w, "h": i.l.h, "pinned": i.l.pinned,
                "fs": i.fs, "armed": i.armed, "narrow": i.narrow, "menu": i.menu, "drop": i.drop,
                "secrets": i.secrets, "secretsN": i.secrets_n, "secretsTip": rules::secrets_tip(i.secrets_n), "launchBusy": i.launch_busy,
                "parked": i.parked, "factory": i.factory,
                "chips": i.chips.iter().map(|r| r.iter().map(|(id, label)| json!({ "id": id, "label": label })).collect::<Vec<_>>()).collect::<Vec<_>>(),
            });
            for (p, f) in &extra {
                if i.key.starts_with(p) { if let (Some(o), Value::Object(add)) = (v.as_object_mut(), f(&i.key, cx)) { o.extend(add); } }
            }
            v
        }).collect()
    }

    fn props(&mut self, cx: &App) -> Value {
        let panes = self.panes(cx);
        let (gw, gh) = self.grid_size();
        let gutters = self.mosaic().layout(gw, gh).1;
        let full = self.full.clone().map(|k| {
            let (vw, vh) = self.window_size(cx);
            let (short, long, _) = self.title_of(&k, cx);
            let (title, cap) = rules::title_shown(&short, &long, vw);
            json!({ "key": k, "w": vw, "h": vh, "title": title, "titleMax": cap })
        }).unwrap_or(Value::Null);
        let mut v = json!({
            "panes": panes, "full": full, "dragOn": self.drag.is_some(), "addMenu": self.add_menu, "lastCreate": self.last_create,
            "lockedGutters": gutters.iter().filter(|g| g.locked).count(),
            "gutterViews": gutters.iter().map(|g| json!({ "split": g.split, "index": g.index, "row": g.dir == Dir::Row, "x": g.x, "y": g.y, "w": g.w, "h": g.h, "locked": g.locked })).collect::<Vec<_>>(),
        });
        v["toast"] = json!(self.toast);
        // The sideways scroll: how far, and the grid against the room.
        v["scrollX"] = json!(-f32::from(self.scroll.offset().x));
        v["gridW"] = json!(gw);
        v["areaW"] = json!(self.area.0);
        v["origin"] = json!([self.origin.0, self.origin.1]);
        v
    }

    fn window_size(&self, _cx: &App) -> (f32, f32) { self.win_size }

    // ─── The header ───

    pub fn focus_pane(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let covered = self.full.as_ref().is_some_and(|k| self.layouts.get(&self.project).and_then(|m| m.leaf_of(k)) != Some(leaf));
        if covered || self.focused == leaf { return; }
        self.focused = leaf;
        cx.notify();
    }

    /// The ×: a panel's and a launcher's close at once; a chat's takes two
    /// clicks (the first arms "Sure?", which disarms after 3 s).
    pub fn pane_close(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let key = self.rect(leaf).and_then(|l| l.key).unwrap_or_default();
        // A chat's and a Claude terminal's take two clicks.
        if !key.starts_with("live:") && !key.starts_with("term:") { self.armed = 0; return self.close_pane(leaf, cx); }
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

    /// Close a pane. A chat's closes the chat on the server first; refused,
    /// the pane stays and a toast says why.
    pub fn close_pane(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let key = self.rect(leaf).and_then(|l| l.key).unwrap_or_default();
        // A Claude terminal (3.2): the server closes it, then its pane goes.
        if let Some(id) = term_of(&key).map(String::from) {
            let me = cx.entity().downgrade();
            let path = format!("/api/terminals/{id}/close");
            return crundi_shell::call(cx, move |api| api.post(&path, &json!({})), move |r, cx| {
                let _ = me.update(cx, |w, cx| {
                    if let Err(e) = r { return w.toast(&format!("Failed to close terminal: {e}"), true, cx); }
                    let k = format!("term:{id}");
                    w.closed.insert(id.clone());
                    w.protect.remove(&k);
                    if let Some(l) = w.mosaic().leaf_of(&k) { w.mosaic().close(l); }
                    w.font.remove(&id);
                    store::update(|s| { s.font.remove(&id); });
                    if w.full.as_deref() == Some(k.as_str()) { w.full = None; }
                    w.changed(cx);
                });
            });
        }
        let Some(id) = chat_of(&key).map(String::from) else { self.mosaic().close(leaf); if self.menu == leaf { self.menu = 0; } return self.changed(cx); };
        let Some(api) = crundi_shell::api(cx) else { return };
        let me = cx.entity().downgrade();
        // A parked chat (2.23) is discarded for good; a live one closed.
        let parked = crundi_shell::session::get(cx).terminals.iter().any(|t| t.get("id").and_then(|i| i.as_str()) == Some(id.as_str()) && t.get("kind").and_then(|k| k.as_str()) == Some("parked"));
        let path = if parked { format!("/api/panes/{id}/discard") } else { format!("/api/ui-sessions/{id}/close") };
        crundi_shell::run(cx, move || api.post(&path, &json!({})), move |r, cx| {
            match rules::close_outcome(r) {
                rules::Closed::Yes => { let _ = me.update(cx, |w, cx| {
                    let k = format!("live:{id}");
                    w.closed.insert(id.clone());
                    if let Some(l) = w.mosaic().leaf_of(&k) { w.mosaic().close(l); }
                    w.font.remove(&id);
                    store::update(|s| { s.font.remove(&id); });
                    if w.full.as_deref() == Some(k.as_str()) { w.full = None; }
                    w.changed(cx);
                }); }
                rules::Closed::SignedOut => crundi_shell::session::sign_out(cx, "Your sign-in ended. Sign in again."),
                rules::Closed::No(why) => { let _ = me.update(cx, |w, cx| w.toast(&why, true, cx)); }
            }
        });
    }

    pub fn pane_full(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let Some(key) = self.rect(leaf).and_then(|l| l.key) else { return };
        self.full = if self.full.as_deref() == Some(key.as_str()) { None } else { Some(key) };
        self.armed = 0;
        self.menu = 0;
        cx.notify();
    }

    /// A- / A+ (d = -1 / 1) or reset (0): per chat, and the new-chat default.
    pub fn pane_font(&mut self, leaf: u32, d: i32, cx: &mut Context<Self>) {
        let Some(id) = self.rect(leaf).and_then(|l| l.key).and_then(|k| item_of(&k).map(String::from)) else { return };
        let px = if d == 0 { rules::FONT_BASE } else { rules::font_step(self.font_px(&id), d) };
        self.font.insert(id.clone(), px);
        self.font_default = px;
        store::update(|s| { s.font.insert(id, px); s.font_default_chat = Some(px); });
        cx.notify();
    }

    pub fn pane_pin(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let Some(l) = self.rect(leaf) else { return };
        self.mosaic().toggle_pin(leaf, l.w, l.h);
        self.changed(cx);
    }

    pub fn pane_maximize(&mut self, leaf: u32, cx: &mut Context<Self>) {
        self.mosaic().maximize(leaf);
        self.changed(cx);
    }

    /// A narrow header's "…" menu: opening one closes any other and focuses its pane.
    pub fn pane_menu(&mut self, leaf: u32, open: bool, cx: &mut Context<Self>) {
        if open { self.menu = leaf; self.focused = leaf; } else if self.menu == leaf { self.menu = 0; }
        cx.notify();
    }

    /// Rename a chat: the title changes at once, then the server is told;
    /// an unchanged title sends nothing.
    pub fn pane_rename(&mut self, leaf: u32, input: &str, cx: &mut Context<Self>) {
        // A Claude terminal (3.2): /api/terminals/:id/rename.
        if let Some(id) = self.rect(leaf).and_then(|l| l.key).and_then(|k| term_of(&k).map(String::from)) {
            let (current, _, _) = self.title_of(&format!("term:{id}"), cx);
            let Some(title) = rules::rename_value(input, &current) else { return };
            self.titles.insert(id.clone(), title.clone());
            cx.notify();
            return crundi_shell::call(cx, move |api| api.post(&format!("/api/terminals/{id}/rename"), &json!({ "title": title })), |_, _| {});
        }
        let Some(id) = self.rect(leaf).and_then(|l| l.key).and_then(|k| chat_of(&k).map(String::from)) else { return };
        let (current, _, _) = self.title_of(&format!("live:{id}"), cx);
        let Some(title) = rules::rename_value(input, &current) else { return };
        self.titles.insert(id.clone(), title.clone());
        cx.notify();
        crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{id}/rename"), &json!({ "title": title })), |_, _| {});
    }

    /// Click on the title: the input, all selected, focused.
    pub(crate) fn start_rename(&mut self, leaf: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.rect(leaf).and_then(|l| l.key).filter(|k| k.starts_with("live:") || k.starts_with("term:")) else { return };
        let (short, _, _) = self.title_of(&key, cx);
        let st = cx.new(|cx| w::InputState::new(window, cx));
        st.update(cx, |s, cx| { s.set_value(short, window, cx); s.select_all(window, cx); s.focus(window, cx); });
        self.rename_sub = Some(cx.subscribe_in(&st, window, move |this, _, ev: &w::InputEvent, _window, cx| match ev {
            w::InputEvent::PressEnter { .. } | w::InputEvent::Blur => this.commit_rename(leaf, cx),
            _ => {}
        }));
        self.renaming = Some((leaf, st));
        cx.notify();
    }

    pub(crate) fn commit_rename(&mut self, leaf: u32, cx: &mut Context<Self>) {
        let Some((l, st)) = self.renaming.take().filter(|(l, _)| *l == leaf) else { return };
        self.rename_sub = None;
        let text = st.read(cx).value().to_string();
        self.pane_rename(l, &text, cx);
        cx.notify();
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming = None;
        self.rename_sub = None;
        cx.notify();
    }

    pub(crate) fn open_secrets(&mut self, leaf: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.rect(leaf).and_then(|l| l.key).and_then(|k| chat_of(&k).map(String::from)) else { return };
        self.menu = 0;
        if let Some(f) = cx.try_global::<OnSecrets>().cloned() { (f.0)(&id, window, cx); }
        cx.notify();
    }

    // ─── Moving panes ───

    /// A "+" menu entry: a launcher, a panel, a Layout preset or Fit.
    pub fn add_pane(&mut self, kind: &str, cx: &mut Context<Self>) {
        self.add_menu = false;
        self.layouts_open = false;
        if kind == "fit" { return self.fit(cx); }
        if let Some(preset) = kind.strip_prefix("layout:") {
            let project = self.project.clone();
            let order: Vec<String> = crundi_shell::session::get(cx).terminals.iter()
                .filter(|t| t.get("kind").and_then(|k| k.as_str()) == Some("ui") && t.get("project").and_then(|p| p.as_str()) == Some(project.as_str()))
                .filter_map(|t| t.get("id").and_then(|i| i.as_str()).map(|i| format!("live:{i}"))).collect();
            let key = self.rect(self.focused).and_then(|l| l.key);
            let m = self.mosaic().preset_from(preset, &order);
            *self.mosaic() = m;
            if let Some(l) = key.and_then(|k| self.mosaic().leaf_of(&k)) { self.focused = l; }
            return self.changed(cx);
        }
        match self.new_key(kind) {
            Ok(k) => { self.focused = self.mosaic().place(&k); self.changed(cx); }
            Err(e) => self.toast(&e, false, cx),
        }
    }

    /// A new pane's key: a launcher (pend:<n>), a panel (panel:<kind>), or
    /// one of the kinds that open many (panel:notes:<n>, panel:widget:<n>).
    /// A panel already in the workbench is refused (the web's addWbPanel).
    fn new_key(&mut self, kind: &str) -> Result<String, String> {
        let kind = if kind == "panel" { "widget" } else { kind };
        if kind == "terminal" { self.pending += 1; return Ok(format!("pend:{}", self.pending)); }
        let keys = self.mosaic().keys();
        if kind == "notes" || kind == "widget" {
            let n = keys.iter().filter_map(|k| k.strip_prefix(&format!("panel:{kind}:")).and_then(|n| n.parse::<u32>().ok())).max().unwrap_or(0) + 1;
            return Ok(format!("panel:{kind}:{n}"));
        }
        if keys.iter().any(|k| k == &format!("panel:{kind}")) {
            let mut c = kind.chars();
            let label = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
            return Err(format!("{label} is already in the workbench"));
        }
        Ok(format!("panel:{kind}"))
    }

    /// Fit to screen: every split even, the grid back at the left; if it
    /// still scrolls sideways, an error toast says why.
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.mosaic().fit();
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.changed(cx);
        cx.spawn(async move |this: WeakEntity<Workbench>, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(60)).await;
            let _ = this.update(cx, |w, cx| {
                let aw = w.area.0;
                let over = w.mosaic().content_width(aw) > aw + 2.0;
                let pin = w.mosaic().has_pin();
                if over {
                    w.toast(if pin { "Does not fit: the pinned panes plus the others at their minimum width are wider than the window" }
                        else { "Does not fit: too many panes side by side for this window, even at their minimum width" }, true, cx);
                }
            });
        }).detach();
    }

    pub fn split(&mut self, leaf: u32, down: bool, cx: &mut Context<Self>) {
        self.mosaic().split(leaf, if down { Dir::Col } else { Dir::Row });
        self.changed(cx);
    }

    /// An empty pane's chip: that kind in that very pane.
    pub fn chip(&mut self, leaf: u32, kind: &str, cx: &mut Context<Self>) {
        match self.new_key(kind) {
            Ok(k) => { self.mosaic().assign(leaf, Some(k)); self.focused = leaf; self.changed(cx); }
            Err(e) => self.toast(&e, false, cx),
        }
    }

    /// The launcher's UI Mode (and its Skip Permissions): a new chat, which
    /// takes the launcher's leaf. Refused or failed: a toast.
    pub fn launch_chat(&mut self, leaf: u32, skip: bool, cx: &mut Context<Self>) {
        let project = self.project.clone();
        if project.is_empty() || self.launching.contains_key(&leaf) { return; }
        // Which conversation first (the resume picker), when someone chooses.
        if let Some(choose) = cx.try_global::<Chooser>().map(|c| c.0.clone()) {
            let me = cx.entity().downgrade();
            let go: Rc<dyn Fn(&'static str, String, &mut App)> = Rc::new(move |mode, id, cx| {
                // The launcher went meanwhile: nothing to start.
                let _ = me.update(cx, |w, cx| if w.mosaic().layout(1000., 1000.).0.iter().find(|l| l.id == leaf).and_then(|l| l.key.clone()).is_some_and(|k| k.starts_with("pend:")) { w.create_chat(leaf, skip, mode, &id, cx) });
            });
            choose(&project, skip, false, cx, go);
            return;
        }
        self.create_chat(leaf, skip, "new", "", cx);
    }

    /// Start the chat (new | resume | compact) in a launcher's leaf: the
    /// pressed button busy until it answers; refused or failed, a toast.
    pub fn create_chat(&mut self, leaf: u32, skip: bool, mode: &str, resume_id: &str, cx: &mut Context<Self>) {
        let project = self.project.clone();
        if project.is_empty() || self.launching.contains_key(&leaf) { return; }
        self.launching.insert(leaf, if skip { 2 } else { 1 });
        cx.notify();
        let body = crundi_core::resume::create_body(&project, skip, mode, resume_id);
        self.last_create = body.clone();
        let stub = crundi_core::driver::take_stub("create");
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| match stub { Some(v) => stub_result(v), None => api.post("/api/ui-sessions/create", &body) }, move |r, cx| {
            let _ = me.update(cx, |w, cx| {
                w.launching.remove(&leaf);
                let v = match r {
                    Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => {
                        let e = v.get("error").and_then(|e| e.as_str()).unwrap_or("").to_string();
                        return w.toast(if e.is_empty() { "Failed to launch" } else { &e }, true, cx);
                    }
                    Ok(v) => v,
                    Err(e) => return w.toast(&format!("Failed to launch: {e}"), true, cx),
                };
                let Some(id) = v.get("id").and_then(|i| i.as_str()).map(String::from) else { return cx.notify() };
                let key = format!("live:{id}");
                if let Some(other) = w.mosaic().leaf_of(&key) { if other != leaf { w.mosaic().close(other); } }
                w.mosaic().assign(leaf, Some(key.clone()));
                w.protect.insert(key, Instant::now());
                w.focused = leaf;
                w.ensure_fonts();
                w.changed(cx);
            });
        });
    }

    /// The launcher's Empty Shell, Terminal and its Skip Permissions (3.2,
    /// the web's launchTerminal; mode shell | normal | skip): a shell starts
    /// at once; Terminal asks which conversation first, as UI Mode does.
    pub fn launch_term(&mut self, leaf: u32, mode: &str, cx: &mut Context<Self>) {
        let project = self.project.clone();
        if project.is_empty() || self.launching.contains_key(&leaf) { return; }
        if mode == "shell" { return self.create_term(leaf, false, true, "", "", cx); }
        let skip = mode == "skip";
        if let Some(choose) = cx.try_global::<Chooser>().map(|c| c.0.clone()) {
            let me = cx.entity().downgrade();
            let go: Rc<dyn Fn(&'static str, String, &mut App)> = Rc::new(move |m, id, cx| {
                let _ = me.update(cx, |w, cx| if w.mosaic().layout(1000., 1000.).0.iter().find(|l| l.id == leaf).and_then(|l| l.key.clone()).is_some_and(|k| k.starts_with("pend:")) { w.create_term(leaf, skip, false, m, &id, cx) });
            });
            choose(&project, skip, true, cx, go);
            return;
        }
        self.create_term(leaf, skip, false, "new", "", cx);
    }

    /// Start a Claude terminal (/api/terminals/create) in a launcher's leaf:
    /// the pressed button busy (3 shell, 4 Terminal, 5 its Skip Permissions)
    /// until it answers; refused or failed, a toast.
    pub fn create_term(&mut self, leaf: u32, skip: bool, shell: bool, mode: &str, resume_id: &str, cx: &mut Context<Self>) {
        let project = self.project.clone();
        if project.is_empty() { return self.toast("Select a project first", true, cx); }
        if self.launching.contains_key(&leaf) { return; }
        self.launching.insert(leaf, if shell { 3 } else if skip { 5 } else { 4 });
        cx.notify();
        let mut body = json!({ "project": project, "skipPermissions": skip, "shell": shell });
        if !shell && !mode.is_empty() { body["sessionMode"] = json!(mode); body["resumeId"] = json!(resume_id); }
        self.last_create = body.clone();
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.post("/api/terminals/create", &body), move |r, cx| {
            let _ = me.update(cx, |w, cx| {
                w.launching.remove(&leaf);
                let v = match r {
                    Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => {
                        let e = v.get("error").and_then(|e| e.as_str()).unwrap_or("").to_string();
                        return w.toast(if e.is_empty() { "Failed to launch" } else { &e }, true, cx);
                    }
                    Ok(v) => v,
                    Err(e) => return w.toast(&format!("Failed to launch terminal: {e}"), true, cx),
                };
                let Some(id) = v.get("id").and_then(|i| i.as_str()).map(String::from) else { return cx.notify() };
                let key = format!("term:{id}");
                if let Some(other) = w.mosaic().leaf_of(&key) { if other != leaf { w.mosaic().close(other); } }
                w.mosaic().assign(leaf, Some(key.clone()));
                w.protect.insert(key, Instant::now());
                w.focused = leaf;
                w.ensure_fonts();
                w.changed(cx);
            });
        });
    }

    // ─── The pointer (window coordinates) ───

    /// The leaf at a window point (none on a gutter or outside).
    fn leaf_at(&mut self, x: f32, y: f32) -> Option<LeafRect> {
        let off = self.scroll.offset();
        let (gx, gy) = (x - self.origin.0 - f32::from(off.x), y - self.origin.1 - f32::from(off.y));
        let (gw, gh) = self.grid_size();
        self.mosaic().layout(gw, gh).0.into_iter().find(|l| gx >= l.x && gx < l.x + l.w && gy >= l.y && gy < l.y + l.h)
    }

    pub(crate) fn head_press(&mut self, leaf: u32, x: f32, y: f32) {
        if self.full.is_some() { return; }
        self.press = Some(Press { leaf, x, y });
    }

    pub(crate) fn gutter_press(&mut self, split: u32, index: usize, row: bool, span: f32, x: f32, y: f32) {
        let s = self.mosaic().shares(split).unwrap_or_default();
        if let (Some(a0), Some(b0)) = (s.get(index), s.get(index + 1)) {
            self.gutter = Some(GutterDrag { split, index, a0: *a0, b0: *b0, total: span - crundi_core::mosaic::GUTTER, start: if row { x } else { y }, row });
        }
    }

    pub(crate) fn gutter_even(&mut self, split: u32, index: usize, cx: &mut Context<Self>) {
        self.gutter = None;
        self.mosaic().even(split, index);
        self.changed(cx);
    }

    pub(crate) fn pointer_move(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if let Some((x0, off0)) = self.sbar {
            let (gw, _) = self.grid_size();
            let aw = self.area.0.max(1.);
            let to = (off0 + (x - x0) * gw / aw).clamp(0., (gw - aw).max(0.));
            self.scroll.set_offset(point(px(-to), px(0.)));
            return cx.notify();
        }
        if let Some(g) = self.gutter {
            let at = if g.row { x } else { y };
            self.mosaic().drag_from(g.split, g.index, g.a0, g.b0, g.total, at - g.start);
            self.save();
            return cx.notify();
        }
        if let Some(p) = self.press {
            if self.drag.is_none() && ((x - p.x).abs() > 6.0 || (y - p.y).abs() > 6.0) {
                let Some(l) = self.rect(p.leaf) else { return };
                let off = self.scroll.offset();
                let (lx, ly) = (self.origin.0 + f32::from(off.x) + l.x, self.origin.1 + f32::from(off.y) + l.y);
                self.menu = 0;
                self.drag = Some(Drag { src: p.leaf, target: p.leaf, x, y, dx: p.x - lx, dy: p.y - ly });
            }
        }
        if self.drag.is_some() {
            let t = self.leaf_at(x, y).map(|l| l.id).unwrap_or(0);
            if let Some(d) = self.drag.as_mut() { d.target = t; d.x = x; d.y = y; }
            cx.notify();
        }
    }

    pub(crate) fn pointer_up(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.press = None;
        if self.sbar.take().is_some() { return cx.notify(); }
        if self.gutter.take().is_some() { return self.changed(cx); }
        let Some(d) = self.drag.take() else { return };
        let target = self.leaf_at(x, y).map(|l| l.id);
        let r = target.map(|t| self.mosaic().drop_on(d.src, t)).unwrap_or(Dropped::Nothing);
        match r {
            Dropped::Pinned => self.toast("This pane is pinned. Unpin it to move it.", false, cx),
            Dropped::Nothing => cx.notify(),
            _ => { if let Some(t) = target { self.focused = t; } self.changed(cx); }
        }
    }

    /// Esc: a drag puts the pane down where it was; else full screen, a
    /// rename, a menu close. True when it was used.
    fn escape(&mut self, cx: &mut Context<Self>) -> bool {
        if self.drag.take().is_some() { self.press = None; cx.notify(); return true; }
        if self.renaming.is_some() { self.cancel_rename(cx); return true; }
        if self.add_menu { self.add_menu = false; self.layouts_open = false; cx.notify(); return true; }
        if self.menu != 0 { self.menu = 0; cx.notify(); return true; }
        if self.full.is_some() { self.full = None; cx.notify(); return true; }
        false
    }

    // ─── Secrets (the lock's count) ───

    fn load_secrets(&mut self, cx: &mut Context<Self>) {
        let Some(base) = crundi_shell::api(cx).map(|a| a.base().to_string()) else { return };
        let me = cx.entity().downgrade();
        crundi_shell::run(cx, move || crundi_core::api::handshake(&base).ok().and_then(|h| h.pointer("/server/capabilities").cloned()), move |caps, cx| {
            let on = caps.as_ref().and_then(|c| c.as_array()).is_some_and(|c| c.iter().any(|x| x == "chat-secret-preauth"));
            let _ = me.update(cx, |w, cx| { w.secrets_cap = on; cx.notify(); });
            if !on { return; }
            let me2 = me.clone();
            crundi_shell::call(cx, |api| api.get("/api/secrets/chat"), move |r, cx| {
                let Ok(v) = r else { return };
                let _ = me2.update(cx, |w, cx| {
                    if let Some(chats) = v.get("chats").and_then(|c| c.as_object()) { for (sid, g) in chats { w.grants.insert(sid.clone(), g.clone()); } }
                    cx.notify();
                });
            });
        });
    }

    /// The server's list changed: renames it now agrees with are done; new
    /// chats get their size.
    fn on_state(&mut self, cx: &mut Context<Self>) {
        self.seen_state = true;
        let done: Vec<String> = self.titles.iter().filter(|(id, t)| Self::chat_item(cx, id).and_then(|c| c.get("title").and_then(|x| x.as_str()).map(String::from)).as_deref() == Some(t.as_str())).map(|(id, _)| id.clone()).collect();
        for id in done { self.titles.remove(&id); }
        self.ensure_fonts();
        cx.notify();
    }
}

/// The window's workbench (None before the window opens).
pub fn workbench(cx: &App) -> Option<Entity<Workbench>> {
    cx.try_global::<WorkbenchHandle>().map(|h| h.0.clone())
}

/// The text scale of a chat's pane (its A- / A+), for the chat's log and composer.
pub fn text_scale(cx: &App, chat: &str) -> f32 {
    workbench(cx).map(|w| rules::font_scale(w.read(cx).font_px(chat))).unwrap_or(1.0)
}

/// Open or close the "+" menu (the project bar's + by Workbench).
pub fn toggle_add_menu(cx: &mut App) {
    if let Some(w) = workbench(cx) { w.update(cx, |w, cx| { w.add_menu = !w.add_menu; w.layouts_open = false; cx.notify(); }); }
}

fn num(a: &[String], i: usize) -> u32 { a.get(i).and_then(|s| s.parse().ok()).unwrap_or(0) }

/// Make the workbench and register its tab (once, at start). Its panes'
/// kinds are registered by the parts that draw them (`register_kind`).
pub fn init(cx: &mut App) {
    let wb = cx.new(Workbench::new);
    cx.set_global(WorkbenchHandle(wb.clone()));
    // The project bar's "+" by Workbench opens the add menu (F21).
    crundi_shell::register_add_menu(cx, toggle_add_menu);
    // Esc goes to the workbench first while it has a use for it.
    cx.intercept_keystrokes(|e, _window, cx| {
        if e.keystroke.key != "escape" { return; }
        if let Some(w) = workbench(cx) { if w.update(cx, |w, cx| w.escape(cx)) { cx.stop_propagation(); } }
    }).detach();
    // Typing brings the focused pane into view (as Slint's focus does).
    cx.observe_keystrokes(|e, window, cx| {
        if e.keystroke.key_char.is_none() { return; }
        let Some(w) = workbench(cx) else { return };
        let holder = w.read(cx).leaf_focus.iter().find(|(_, h)| h.contains_focused(window, cx)).map(|(l, _)| *l);
        let Some(leaf) = holder else { return };
        w.update(cx, |w, cx| { if w.focused != leaf || !w.reveal { w.focus_pane(leaf, cx); w.reveal = true; cx.notify(); } });
    }).detach();
    crundi_shell::register(cx, crundi_shell::Area::new("workbench")
        .tabs(&["workbench"], move |_, _, _| wb.clone().into_any_element())
        .on_project(|alias, cx| if let Some(wb) = workbench(cx) { let a = alias.to_string(); wb.update(cx, |w, cx| { w.project = a; w.full = None; w.armed = 0; w.menu = 0; w.renaming = None; let _ = w.mosaic(); if !w.secrets_cap { w.load_secrets(cx); } cx.notify(); }); })
        .on_signed_in(|cx| if let Some(wb) = workbench(cx) { wb.update(cx, |w, cx| w.load_secrets(cx)); })
        .on_event(|e, cx| {
            let Some(wb) = workbench(cx) else { return };
            match e {
                crundi_shell::AreaEvent::State(_) => wb.update(cx, |w, cx| w.on_state(cx)),
                crundi_shell::AreaEvent::Server("chat-secrets", v) => {
                    let sid = v.get("sessionId").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    if sid.is_empty() { return; }
                    let g = v.get("grants").cloned().unwrap_or(Value::Null);
                    wb.update(cx, |w, cx| { w.grants.insert(sid, g); cx.notify(); });
                }
                _ => {}
            }
        })
        .test_ui(|name, a, window, cx| {
            let wb = workbench(cx)?;
            let s = |i: usize| a.get(i).cloned().unwrap_or_default();
            match name {
                "pane-close" => wb.update(cx, |w, cx| w.pane_close(num(a, 0), cx)),
                "pane-full" => wb.update(cx, |w, cx| w.pane_full(num(a, 0), cx)),
                "pane-font" => { let d: i32 = s(1).parse().unwrap_or(0); wb.update(cx, |w, cx| w.pane_font(num(a, 0), d, cx)) }
                "pane-pin" => wb.update(cx, |w, cx| w.pane_pin(num(a, 0), cx)),
                "pane-rename" => wb.update(cx, |w, cx| w.pane_rename(num(a, 0), &s(1), cx)),
                "pane-maximize" => wb.update(cx, |w, cx| w.pane_maximize(num(a, 0), cx)),
                "pane-menu" => wb.update(cx, |w, cx| w.pane_menu(num(a, 0), s(1) != "false", cx)),
                "pane-secrets" => wb.update(cx, |w, cx| w.open_secrets(num(a, 0), window, cx)),
                "add-pane" => wb.update(cx, |w, cx| w.add_pane(&s(0), cx)),
                "add-menu" => wb.update(cx, |w, cx| { w.add_menu = s(0) == "true"; w.layouts_open = false; cx.notify(); }),
                "close-pane" => wb.update(cx, |w, cx| w.close_pane(num(a, 0), cx)),
                "focus-pane" => wb.update(cx, |w, cx| w.focus_pane(num(a, 0), cx)),
                "split" => wb.update(cx, |w, cx| w.split(num(a, 0), s(1) == "true", cx)),
                "chip" => wb.update(cx, |w, cx| w.chip(num(a, 0), &s(1), cx)),
                "launch-chat" => wb.update(cx, |w, cx| w.launch_chat(num(a, 0), s(1) == "skip", cx)),
                "launch-term" => wb.update(cx, |w, cx| w.launch_term(num(a, 0), &s(1), cx)),
                _ => return None,
            }
            Some(json!({ "ok": true }))
        })
        .props(|_, cx| workbench(cx).map(|wb| wb.update(cx, |w, cx| w.props(cx))).unwrap_or_else(|| json!({ "panes": [] }))));
}

/// A driver stub's answer, as a server's (delayMs, status >= 400, net).
fn stub_result(v: Value) -> crundi_core::api::ApiResult {
    use crundi_core::api::ApiError;
    let st = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    if let Some(ms) = v.get("delayMs").and_then(|x| x.as_u64()) { std::thread::sleep(Duration::from_millis(ms)); }
    match v.get("status").and_then(|x| x.as_u64()) {
        Some(c) if c >= 400 => Err(ApiError::Status(c as u16, st("error"))),
        _ if v.get("net").is_some() => Err(ApiError::Net(st("net"))),
        _ => Ok(v),
    }
}
