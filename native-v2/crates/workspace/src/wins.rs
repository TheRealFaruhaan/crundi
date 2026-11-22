//! The file windows (the web's feBuildWindow, feOpen, feOpenViewer,
//! feOpenDiff, feWireDrag, feWireResize, feArrange): floating over the whole
//! window (a layer the shell draws, crundi_shell::register_overlay). Each
//! window's header is the web's: the path, expand / restore, Save (green, a
//! file in the editor only) and Close. A file in the editor (one window a
//! file; Save and Ctrl+S, "Saving…", then "Saved ✓" a moment), a picture or
//! a PDF in the viewer, a Git diff (diffview.rs). A window drags by its
//! header and snaps to the edges and corners (36px zones); its right edge,
//! bottom edge and corner resize it; a press brings it to the front; Esc
//! closes the front one. While a window is dragged (and a moment after) the
//! Arrange bar floats at the top: Tile, Grid, Cascade.
//!
//! The layer is one cached view over the window (`register_overlay`); a key
//! in an editor redraws the layer only when it turns Save on or off; a diff
//! is its own cached view inside its window.

use crate::{diffview::DiffWin, toast};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

/// Shown as a picture (FE_IMAGE_EXTS); a PDF is shown too (as a page).
const IMAGE_EXTS: [&str; 9] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg", "avif"];
/// Shown as a picture, but source worth editing: the row menu offers
/// "Edit the source" (FE_SOURCE_PREVIEW_EXTS).
const SOURCE_PREVIEW_EXTS: [&str; 1] = ["svg"];
const TOP: f32 = T::WS_WIN_TOP;
const MIN_W: f32 = T::WS_WIN_MIN_W;
const MIN_H: f32 = T::WS_WIN_MIN_H;
const ZONE: f32 = T::WS_WIN_ZONE;

pub fn ext_of(f: &str) -> String { f.rsplit('.').next().filter(|e| *e != f).unwrap_or("").to_ascii_lowercase() }
/// A file the menu offers to edit as source (an SVG).
pub fn is_source_preview(f: &str) -> bool { SOURCE_PREVIEW_EXTS.contains(&ext_of(f).as_str()) }
fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn ok(v: &Value) -> bool { v.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

/// A new window's place (feBuildWindow): min(900, 80%) by min(72%, 820),
/// each next one 28 down and right, six then round again.
pub fn new_rect(n: u32, vw: f32, vh: f32) -> Rect {
    let step = 28.0 * (n % 6) as f32;
    Rect { x: (vw * 0.18).max(40.0) + step, y: 54.0 + step, w: (vw * 0.8).min(900.0).max(MIN_W), h: (vh * 0.72).min(820.0).max(MIN_H) }
}

/// An expanded window (.fe-window.maximized): 48 down, 6 in, to the edges.
pub fn max_rect(vw: f32, vh: f32) -> Rect { Rect { x: 6.0, y: TOP, w: vw - 12.0, h: vh - TOP - 6.0 } }

/// The snap zone at a pointer (feZoneAt).
pub fn zone_at(x: f32, y: f32, vw: f32, vh: f32) -> Option<&'static str> {
    let (l, r, t, b) = (x < ZONE, x > vw - ZONE, y < ZONE, y > vh - ZONE);
    if t && l { return Some("tl"); } if t && r { return Some("tr"); }
    if b && l { return Some("bl"); } if b && r { return Some("br"); }
    if l { return Some("left"); } if r { return Some("right"); } if t { return Some("max"); }
    None
}

/// A zone's rect (feSnapRect).
pub fn snap_rect(zone: &str, vw: f32, vh: f32) -> Option<Rect> {
    let h = vh - TOP - 4.0;
    let w2 = vw / 2.0;
    let hh = h / 2.0 - 2.0;
    Some(match zone {
        "left" => Rect { x: 4.0, y: TOP, w: w2 - 6.0, h },
        "right" => Rect { x: w2 + 2.0, y: TOP, w: w2 - 6.0, h },
        "max" => Rect { x: 4.0, y: TOP, w: vw - 8.0, h },
        "tl" => Rect { x: 4.0, y: TOP, w: w2 - 6.0, h: hh },
        "tr" => Rect { x: w2 + 2.0, y: TOP, w: w2 - 6.0, h: hh },
        "bl" => Rect { x: 4.0, y: TOP + h / 2.0 + 2.0, w: w2 - 6.0, h: hh },
        "br" => Rect { x: w2 + 2.0, y: TOP + h / 2.0 + 2.0, w: w2 - 6.0, h: hh },
        _ => return None,
    })
}

/// Arrange's rects (feArrange) for n windows.
pub fn arrange_rects(mode: &str, n: usize, vw: f32, vh: f32) -> Vec<Rect> {
    let (gap, h) = (6.0, vh - TOP - 4.0);
    match mode {
        "cascade" => {
            let (dw, dh) = ((vw * 0.7).min(900.0), (vh * 0.7).min(760.0));
            (0..n).map(|i| { let off = 28.0 * (i % 8) as f32; Rect { x: 40.0 + off, y: TOP + off, w: dw, h: dh } }).collect()
        }
        "tile" => {
            let cw = (vw - gap) / n.max(1) as f32;
            (0..n).map(|i| Rect { x: gap / 2.0 + i as f32 * cw, y: TOP, w: cw - gap, h }).collect()
        }
        _ => {
            let cols = (n as f32).sqrt().ceil().max(1.0) as usize;
            let rows = n.div_ceil(cols).max(1);
            let cw = (vw - gap) / cols as f32;
            let ch = (h - gap * (rows as f32 - 1.0)) / rows as f32;
            (0..n).map(|i| { let (c, r) = (i % cols, i / cols); Rect { x: gap / 2.0 + c as f32 * cw, y: TOP + r as f32 * (ch + gap), w: cw - gap, h: ch } }).collect()
        }
    }
}

/// A resize by a handle (feWireResize): 'e' the right edge, 's' the bottom,
/// 'c' the corner; from where it started, by the pointer's travel.
pub fn resized(r: Rect, dir: char, dx: f32, dy: f32) -> Rect {
    let mut n = r;
    if dir == 'e' || dir == 'c' { n.w = (r.w + dx).max(MIN_W); }
    if dir == 's' || dir == 'c' { n.h = (r.h + dy).max(MIN_H); }
    n
}

pub struct FileWin {
    pub id: u32,
    /// edit | view | diff
    pub kind: &'static str,
    pub project: String,
    pub file: String,
    pub original: String,
    pub save_label: String,
    pub text: Option<Entity<w::EditorState>>,
    pub img: Option<Arc<Image>>,
    /// The picture's own size in pixels (None for SVG): it is shown at most this big.
    pub nat: Option<(u32, u32)>,
    /// A PDF's address (shown as a page).
    pub page: Option<String>,
    pub diff: Option<Entity<DiffWin>>,
    pub loading: String,
    pub r: Rect,
    pub max: bool,
    /// Edited since opened or saved (Save on), as last seen.
    pub dirty: bool,
}

impl FileWin {
    pub fn value(&self, cx: &App) -> String { self.text.as_ref().map(|t| t.read(cx).value().to_string()).unwrap_or_default() }
    pub fn is_dirty(&self, cx: &App) -> bool { self.text.is_some() && self.value(cx) != self.original }
}

/// What a pointer is doing to a window: moving it by its header, or sizing it.
#[derive(Clone, Copy)]
enum Grab { Move { id: u32, sx: f32, sy: f32, r: Rect }, Size { id: u32, dir: char, sx: f32, sy: f32, r: Rect } }

/// The layer: every window, front last; a drag in progress and its snap
/// zone; the Arrange bar (shown while dragging, and a moment after).
pub struct WinLayer {
    pub wins: Vec<FileWin>,
    next: u32,
    grab: Option<Grab>,
    zone: Option<&'static str>,
    pub arrange: bool,
    arrange_gen: u64,
    vw: f32,
    vh: f32,
    _keys: Option<Subscription>,
}

pub struct LayerHandle(pub Entity<WinLayer>);
impl Global for LayerHandle {}
pub fn layer(cx: &App) -> Option<Entity<WinLayer>> { cx.try_global::<LayerHandle>().map(|h| h.0.clone()) }

impl WinLayer {
    fn find(&mut self, id: u32) -> Option<&mut FileWin> { self.wins.iter_mut().find(|w| w.id == id) }
    pub fn has(&self, id: u32) -> bool { self.wins.iter().any(|w| w.id == id) }
    pub fn front_id(&self) -> Option<u32> { self.wins.last().map(|w| w.id) }

    pub fn front(&mut self, id: u32, cx: &mut Context<Self>) {
        if let Some(i) = self.wins.iter().position(|w| w.id == id) {
            if i + 1 == self.wins.len() { return; }
            let w = self.wins.remove(i);
            self.wins.push(w);
            cx.notify();
        }
    }

    pub fn close(&mut self, id: u32, cx: &mut Context<Self>) {
        self.wins.retain(|w| w.id != id);
        if self.wins.is_empty() { self.arrange = false; self.grab = None; self.zone = None; }
        cx.notify();
    }

    /// Expand / restore (feExpandWin).
    pub fn expand(&mut self, id: u32, cx: &mut Context<Self>) {
        if let Some(w) = self.find(id) { w.max = !w.max; }
        self.front(id, cx);
        cx.notify();
    }

    /// The Arrange bar on (cancelling a hide under way).
    fn show_arrange(&mut self, cx: &mut Context<Self>) {
        self.arrange_gen += 1;
        if !self.arrange { self.arrange = true; cx.notify(); }
    }

    /// The Arrange bar off after `ms`, unless shown again meanwhile.
    fn hide_arrange_soon(&mut self, ms: u64, cx: &mut Context<Self>) {
        self.arrange_gen += 1;
        let g = self.arrange_gen;
        cx.spawn(async move |this: WeakEntity<WinLayer>, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(ms)).await;
            let _ = this.update(cx, |l, cx| if l.arrange_gen == g && l.arrange { l.arrange = false; cx.notify(); });
        }).detach();
    }

    pub fn arrange(&mut self, mode: &str, cx: &mut Context<Self>) {
        if self.wins.is_empty() { return; }
        self.show_arrange(cx);
        self.hide_arrange_soon(T::WS_WIN_ARRANGE_MS, cx);
        let rects = arrange_rects(mode, self.wins.len(), self.vw, self.vh);
        for (w, r) in self.wins.iter_mut().zip(rects) { w.r = r; w.max = false; }
        cx.notify();
    }

    pub fn set_text(&mut self, id: u32, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = self.find(id).and_then(|w| w.text.clone()) { t.update(cx, |st, cx| st.set_value(text.to_string(), window, cx)); }
        if let Some(wn) = self.find(id) { wn.dirty = wn.is_dirty(cx); }
        cx.notify();
    }

    /// The window's rect as shown (expanded or not).
    fn shown_rect(&self, wn: &FileWin) -> Rect { if wn.max { max_rect(self.vw, self.vh) } else { wn.r } }

    /// A drag by the header: 0 start, 1 move, 2 release (window px).
    pub fn drag(&mut self, id: u32, phase: u8, x: f32, y: f32, cx: &mut Context<Self>) {
        let (vw, vh) = (self.vw, self.vh);
        match phase {
            0 => {
                let Some(r) = self.wins.iter().find(|w| w.id == id).map(|w| self.shown_rect(w)) else { return };
                if let Some(w) = self.find(id) { w.max = false; w.r = r; }
                self.grab = Some(Grab::Move { id, sx: x, sy: y, r });
                self.zone = None;
                self.front(id, cx);
                self.show_arrange(cx);
            }
            1 => {
                let Some(Grab::Move { id: did, sx, sy, r }) = self.grab else { return };
                if did != id { return; }
                if let Some(w) = self.find(id) { w.r.x = r.x + x - sx; w.r.y = (r.y + y - sy).max(TOP); }
                self.zone = zone_at(x, y, vw, vh);
            }
            _ => {
                self.grab = None;
                if let Some(r) = self.zone.take().and_then(|z| snap_rect(z, vw, vh)) { if let Some(w) = self.find(id) { w.r = r; w.max = false; } }
                self.hide_arrange_soon(T::WS_WIN_ARRANGE_MS, cx);
            }
        }
        cx.notify();
    }

    /// A resize by a handle: 0 start, 1 move, 2 release.
    pub fn size(&mut self, id: u32, dir: char, phase: u8, x: f32, y: f32, cx: &mut Context<Self>) {
        match phase {
            0 => {
                let Some(r) = self.wins.iter().find(|w| w.id == id).map(|w| self.shown_rect(w)) else { return };
                if let Some(w) = self.find(id) { w.max = false; w.r = r; }
                self.grab = Some(Grab::Size { id, dir, sx: x, sy: y, r });
                self.front(id, cx);
            }
            1 => {
                let Some(Grab::Size { id: did, dir, sx, sy, r }) = self.grab else { return };
                if did != id { return; }
                if let Some(w) = self.find(id) { w.r = resized(r, dir, x - sx, y - sy); }
            }
            _ => { self.grab = None; }
        }
        cx.notify();
    }

    /// The pointer moved or was let go anywhere (a drag or resize in progress).
    fn pointer(&mut self, phase: u8, x: f32, y: f32, cx: &mut Context<Self>) {
        match self.grab {
            Some(Grab::Move { id, .. }) => self.drag(id, phase, x, y, cx),
            Some(Grab::Size { id, dir, .. }) => self.size(id, dir, phase, x, y, cx),
            None => {}
        }
    }

    /// Save (and Ctrl+S): POST /api/files/write.
    pub fn save(&mut self, id: u32, cx: &mut Context<Self>) {
        let Some(wn) = self.wins.iter().find(|w| w.id == id) else { return };
        if wn.text.is_none() || wn.save_label != "Save" { return; }
        let content = wn.value(cx);
        let body = json!({ "project": wn.project, "file": wn.file, "content": content });
        if let Some(wn) = self.find(id) { wn.save_label = "Saving…".into(); }
        cx.notify();
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.post("/api/files/write", &body), move |r, cx| {
            let saved = matches!(&r, Ok(d) if ok(d));
            match r {
                Ok(d) if ok(&d) => toast(cx, "Saved".into(), w::Toast::Success),
                Ok(d) => { let e = s(&d, "error"); toast(cx, format!("Save failed: {}", if e.is_empty() { "unknown".into() } else { e }), w::Toast::Error) }
                Err(e) => toast(cx, format!("Error: {e}"), w::Toast::Error),
            }
            let _ = me.update(cx, |l, cx| {
                if let Some(wn) = l.find(id) { if saved { wn.original = content; wn.save_label = "Saved ✓".into(); wn.dirty = wn.is_dirty(cx); } else { wn.save_label = "Save".into(); } }
                cx.notify();
                if saved {
                    cx.spawn(async move |this: WeakEntity<WinLayer>, cx: &mut AsyncApp| {
                        cx.background_executor().timer(Duration::from_millis(1500)).await;
                        let _ = this.update(cx, |l, cx| { if let Some(wn) = l.find(id) { wn.save_label = "Save".into(); } cx.notify(); });
                    }).detach();
                }
            });
        });
    }

    /// Esc anywhere closes the front window; Ctrl+S saves it (the web's
    /// document keydown: feClose, feSave).
    fn watch_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self._keys.is_some() { return; }
        // Before the focused element (an editor would keep Esc to itself).
        let me = cx.entity().downgrade();
        self._keys = Some(cx.intercept_keystrokes(move |e: &KeystrokeEvent, _, cx| {
            let k = &e.keystroke;
            let Some(l) = me.upgrade() else { return };
            let Some(id) = l.read(cx).front_id() else { return };
            if k.key == "escape" && !k.modifiers.modified() {
                // A dialog or menu over the windows takes Esc itself.
                if w::modal_open(cx) || !w::menu_props(cx).is_null() { return; }
                l.update(cx, |l, cx| l.close(id, cx));
                cx.stop_propagation();
            } else if k.key == "s" && (k.modifiers.control || k.modifiers.platform) && !k.modifiers.shift {
                l.update(cx, |l, cx| l.save(id, cx));
                cx.stop_propagation();
            }
        }));
        let _ = window;
    }

    fn window(&self, wn: &FileWin, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = wn.id;
        let r = self.shown_rect(wn);
        let dirty = wn.dirty;
        let saving = wn.save_label != "Save";
        let body: AnyElement = if !wn.loading.is_empty() {
            w::empty(wn.loading.clone()).into_any_element()
        } else if let Some(d) = &wn.diff {
            d.clone().cached(StyleRefinement::default().size_full()).into_any_element()
        } else if let Some(u) = &wn.page {
            div().relative().size_full().bg(T::bg()).child(crate::frames::page(&format!("win-{id}"), u, window, cx)).into_any_element()
        } else if let Some(img) = &wn.img {
            // As the web's .fe-viewer-img: at its own size, scaled down only
            // to fit (contain, never enlarged past 1:1), centred.
            let pic = gpui::img(img.clone()).object_fit(ObjectFit::Contain).max_w_full().max_h_full();
            let pic = match wn.nat { Some((w_, h_)) => pic.w(px(w_ as f32)).h(px(h_ as f32)), None => pic.size_full() };
            div().size_full().min_h_0().min_w_0().bg(T::bg_secondary()).p(T::SP_16).flex().items_center().justify_center()
                .child(pic).into_any_element()
        } else if let Some(t) = &wn.text {
            div().size_full().font_family(T::MONO).text_size(T::TEXT_13).child(w::code_editor(t).size_full()).into_any_element()
        } else { div().into_any_element() };
        // The header (.fe-header): the path, expand, Save (an editor's), Close.
        let mut tools = div().flex().flex_none().items_center().gap(T::SP_8).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        tools = tools.child(w::icon_button(("win-max", id as usize), IconName::Maximize2, "Expand / restore").on_click(cx.listener(move |l, _, _, cx| l.expand(id, cx))));
        if wn.text.is_some() {
            let label = wn.save_label.clone();
            tools = tools.child(w::disabled(w::success(("win-save", id as usize), label), !dirty || saving).on_click(cx.listener(move |l, _, _, cx| l.save(id, cx))));
        }
        tools = tools.child(w::button(("win-x", id as usize), "Close").on_click(cx.listener(move |l, _, _, cx| l.close(id, cx))));
        let head = div().id(("win-head", id as usize)).flex().flex_none().items_center().gap(T::SP_8).h(T::CONTROL).px(T::SP_12)
            .bg(T::bg_secondary()).border_b_1().border_color(T::border()).cursor_move()
            .on_mouse_down(MouseButton::Left, cx.listener(move |l, e: &MouseDownEvent, _, cx| { cx.stop_propagation(); l.drag(id, 0, f32::from(e.position.x), f32::from(e.position.y), cx); }))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).font_family(T::MONO).text_color(T::text_secondary()).child(wn.file.clone()))
            .child(tools);
        // The resize handles (.fe-resize-e, -s, -se), not while expanded.
        let handle = |dir: char| div().id((SharedString::from(format!("win-rs-{dir}")), id as usize)).absolute()
            .on_mouse_down(MouseButton::Left, cx.listener(move |l, e: &MouseDownEvent, _, cx| { cx.stop_propagation(); l.size(id, dir, 0, f32::from(e.position.x), f32::from(e.position.y), cx); }));
        let edge = T::WS_WIN_EDGE;
        let corner = T::WS_WIN_CORNER;
        div().id(("win", id as usize)).occlude().absolute().left(px(r.x)).top(px(r.y)).w(px(r.w)).h(px(r.h))
            .on_mouse_down(MouseButton::Left, cx.listener(move |l, _, _, cx| l.front(id, cx)))
            .child(div().size_full().flex().flex_col().bg(T::bg()).border_1().border_color(T::border()).rounded(T::R_CARD).overflow_hidden().shadow_lg()
                .child(head)
                .child(div().flex_1().min_h_0().child(body)))
            .when(!wn.max, |d| d
                .child(handle('e').top_0().right(-px(2.)).w(edge).h_full().cursor_ew_resize())
                .child(handle('s').left_0().bottom(-px(2.)).h(edge).w_full().cursor_ns_resize())
                .child(handle('c').right(-px(2.)).bottom(-px(2.)).size(corner).cursor_nwse_resize()
                    .child(div().absolute().right(T::SP_4).bottom(T::SP_4).size(T::SP_6 + px(1.)).border_r_2().border_b_2().border_color(T::text_muted()).rounded_br(T::SP_4))))
            .into_any_element()
    }

    /// The Arrange bar (#fe-arrange): top centre, a pill of three.
    fn arrange_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        div().absolute().top(T::WS_WIN_ARRANGE_TOP).left_0().w_full().flex().justify_center()
            .child(div().id("fe-arrange").occlude().flex().items_center().gap(T::SP_6).px(T::SP_8).py(T::SP_4).rounded(T::R_PILL)
                .bg(T::bg_secondary()).border_1().border_color(T::border()).shadow_lg()
                .on_hover(cx.listener(|l, over: &bool, _, cx| if *over { l.show_arrange(cx) } else { l.hide_arrange_soon(T::WS_WIN_ARRANGE_LEAVE_MS, cx) }))
                .child(div().px(T::SP_4).text_size(T::TEXT_11).text_color(T::text_muted()).child("Arrange"))
                .child(w::pill_button("fe-tile", "Tile", "Tile windows side by side").on_click(cx.listener(|l, _, _, cx| l.arrange("tile", cx))))
                .child(w::pill_button("fe-grid", "Grid", "Tile windows in a grid").on_click(cx.listener(|l, _, _, cx| l.arrange("grid", cx))))
                .child(w::pill_button("fe-cascade", "Cascade", "Cascade windows").on_click(cx.listener(|l, _, _, cx| l.arrange("cascade", cx)))))
            .into_any_element()
    }
}

impl Render for WinLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("wins");
        let vs = window.viewport_size();
        (self.vw, self.vh) = (f32::from(vs.width), f32::from(vs.height));
        if self.wins.is_empty() { return div().into_any_element(); }
        self.watch_keys(window, cx);
        let me = cx.entity().downgrade();
        let mut root = div().absolute().top_0().left_0().size_full()
            .child(canvas(|_, _, _| {}, move |_, _, window, _| {
                let m = me.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| if phase == DispatchPhase::Bubble {
                    let _ = m.update(cx, |l, cx| if l.grab.is_some() { l.pointer(1, f32::from(e.position.x), f32::from(e.position.y), cx) });
                });
                let m = me.clone();
                window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| if phase == DispatchPhase::Bubble {
                    let _ = m.update(cx, |l, cx| if l.grab.is_some() { l.pointer(2, f32::from(e.position.x), f32::from(e.position.y), cx) });
                });
            }).absolute().top_0().left_0().size_full());
        // While dragging into a zone: where the window will land (.fe-snap-hint).
        if let Some(r) = self.zone.and_then(|z| snap_rect(z, self.vw, self.vh)) {
            root = root.child(div().absolute().left(px(r.x)).top(px(r.y)).w(px(r.w)).h(px(r.h)).rounded(T::R_CARD).border_2().border_color(T::accent()).bg(T::accent_dim()));
        }
        let els: Vec<AnyElement> = self.wins.iter().map(|wn| self.window(wn, window, cx)).collect();
        root = root.children(els);
        if self.arrange { root = root.child(self.arrange_bar(cx)); }
        root.into_any_element()
    }
}

fn placeholder() -> FileWin {
    FileWin { id: 0, kind: "edit", project: String::new(), file: String::new(), original: String::new(), save_label: String::new(), text: None, img: None, nat: None, page: None, diff: None, loading: String::new(), r: Rect { x: 0., y: 0., w: 0., h: 0. }, max: false, dirty: false }
}

pub fn init(cx: &mut App) {
    let l = cx.new(|_| WinLayer { wins: vec![], next: 0, grab: None, zone: None, arrange: false, arrange_gen: 0, vw: 0., vh: 0., _keys: None });
    cx.set_global(LayerHandle(l.clone()));
    crundi_shell::register_overlay(cx, l.into());
}

fn add(kind: &'static str, project: &str, file: &str, cx: &mut App) -> Option<u32> {
    let l = layer(cx)?;
    Some(l.update(cx, |l, cx| {
        l.next += 1;
        let id = l.next;
        let r = new_rect(l.next - 1, l.vw.max(MIN_W * 2.), l.vh.max(MIN_H * 3.));
        let mut wn = placeholder();
        (wn.id, wn.kind, wn.project, wn.file, wn.save_label, wn.r) = (id, kind, project.into(), file.into(), "Save".into(), r);
        l.wins.push(wn);
        cx.notify();
        id
    }))
}

/// A diff in a new window (feOpenDiff): `make` builds its view.
pub fn add_diff(project: &str, file: &str, cx: &mut App, make: impl FnOnce(u32, &mut App) -> Entity<DiffWin>) {
    let Some(id) = add("diff", project, file, cx) else { return };
    let v = make(id, cx);
    if let Some(l) = layer(cx) { l.update(cx, |l, cx| { if let Some(wn) = l.find(id) { wn.diff = Some(v); } cx.notify(); }); }
}

/// Open a file: a picture or a PDF in the viewer, anything else in the
/// editor (one window a file: an open one comes to the front). `source`:
/// straight to the editor (an SVG's "Edit the source").
pub fn open_file(project: &str, file: &str, cx: &mut App) { open(project, file, false, cx) }
pub fn open_source(project: &str, file: &str, cx: &mut App) { open(project, file, true, cx) }

fn open(project: &str, file: &str, source: bool, cx: &mut App) {
    let ext = ext_of(file);
    if !source && (IMAGE_EXTS.contains(&ext.as_str()) || ext == "pdf") { return open_viewer(project, file, &ext, cx); }
    let Some(l) = layer(cx) else { return };
    if let Some(id) = l.read(cx).wins.iter().find(|w| w.kind == "edit" && w.file == file && w.project == project).map(|w| w.id) { return l.update(cx, |l, cx| l.front(id, cx)); }
    let path = format!("/api/files/read?project={}&file={}", crate::enc(project), crate::enc(file));
    let (project, file) = (project.to_string(), file.to_string());
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| match r {
        Ok(d) if ok(&d) => {
            let content = s(&d, "content");
            let Some(id) = add("edit", &project, &file, cx) else { return };
            let file2 = file.clone();
            crate::with_window(cx, move |window, cx| {
                let st = cx.new(|cx| { let mut st = w::code_state(&file2, window, cx); st.set_value(content.clone(), window, cx); st });
                if let Some(l) = layer(cx) {
                    let l2 = l.clone();
                    // Save turns on or off: the layer redraws (not on every key).
                    cx.subscribe(&st, move |_, _: &w::InputEvent, cx| l2.update(cx, |l, cx| {
                        let Some(wn) = l.find(id) else { return };
                        let d = wn.is_dirty(cx);
                        if d != wn.dirty { wn.dirty = d; cx.notify(); }
                    })).detach();
                    l.update(cx, |l, cx| { if let Some(wn) = l.find(id) { wn.original = content; wn.text = Some(st.clone()); } cx.notify(); });
                    // The editor takes the keys (the web focuses it).
                    st.update(cx, |st, cx| st.focus(window, cx));
                }
            });
        }
        Ok(d) => { let e = s(&d, "error"); toast(cx, if e.is_empty() { "Cannot open file".into() } else { e }, w::Toast::Error) }
        Err(e) => toast(cx, format!("Error: {e}"), w::Toast::Error),
    });
}

/// A picture or a PDF: a short-lived link (served inline), then the
/// picture's bytes, or the PDF as a page.
fn open_viewer(project: &str, file: &str, ext: &str, cx: &mut App) {
    let Some(id) = add("view", project, file, cx) else { return };
    if let Some(l) = layer(cx) { l.update(cx, |l, _| if let Some(wn) = l.find(id) { wn.loading = "Loading…".into(); }); }
    let body = json!({ "project": project, "file": file });
    // A PDF (and an AVIF, which the renderer cannot decode) is a page.
    let pdf = ext == "pdf" || ext == "avif";
    let format = match ext { "jpg" | "jpeg" => ImageFormat::Jpeg, "gif" => ImageFormat::Gif, "webp" => ImageFormat::Webp, "svg" => ImageFormat::Svg, "bmp" => ImageFormat::Bmp, "ico" => ImageFormat::Ico, _ => ImageFormat::Png };
    let Some(api) = crundi_shell::api(cx) else { return };
    enum Got { Pic(Vec<u8>), Page(String) }
    crundi_shell::run(cx, move || -> Result<Got, String> {
        let d = api.post("/api/files/download-link", &body).map_err(|e| format!("Error: {e}"))?;
        if !ok(&d) { let e = s(&d, "error"); return Err(if e.is_empty() { "Cannot open file".into() } else { e }); }
        let url = format!("{}?inline=1", s(&d, "url"));
        if pdf { return Ok(Got::Page(format!("{}{url}", api.base().trim_end_matches('/')))); }
        let (st, bytes) = api.get_bytes(&url).map_err(|e| format!("Error: {e}"))?;
        if st != 200 { return Err("Cannot open file".into()); }
        Ok(Got::Pic(bytes))
    }, move |r, cx| if let Some(l) = layer(cx) { l.update(cx, |l, cx| {
        if let Some(wn) = l.find(id) {
            match r {
                Ok(Got::Page(u)) => { wn.page = Some(u); wn.loading.clear(); }
                Ok(Got::Pic(b)) => {
                    wn.nat = image::ImageReader::new(std::io::Cursor::new(&b)).with_guessed_format().ok().and_then(|r| r.into_dimensions().ok());
                    wn.img = Some(Arc::new(Image::from_bytes(format, b)));
                    wn.loading.clear();
                }
                Err(e) => wn.loading = e,
            }
        }
        cx.notify();
    }); });
}

pub fn props(cx: &App) -> Vec<Value> {
    layer(cx).map(|l| { let l = l.read(cx); l.wins.iter().filter(|w| w.diff.is_none()).map(|w| { let r = if w.max { max_rect(l.vw, l.vh) } else { w.r }; json!({
        "id": w.id, "kind": w.kind, "title": w.file, "dirty": w.is_dirty(cx), "save": w.save_label, "loading": w.loading, "img": w.img.is_some(), "page": w.page.is_some(),
        "lines": w.text.as_ref().map(|_| w.value(cx).split('\n').count()).unwrap_or(0),
        "x": r.x, "y": r.y, "w": r.w, "h": r.h, "max": w.max, "front": l.front_id() == Some(w.id), "arrange": l.arrange,
    }) }).collect() }).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{arrange_rects, ext_of, is_source_preview, resized, snap_rect, zone_at, Rect};

    #[test]
    fn zones_and_arranging() {
        assert_eq!(zone_at(10.0, 400.0, 1400.0, 860.0), Some("left"));
        assert_eq!(zone_at(10.0, 10.0, 1400.0, 860.0), Some("tl"));
        assert_eq!(zone_at(700.0, 400.0, 1400.0, 860.0), None);
        assert_eq!(snap_rect("left", 1400.0, 860.0), Some(Rect { x: 4.0, y: 48.0, w: 694.0, h: 808.0 }));
        let t = arrange_rects("tile", 2, 1400.0, 860.0);
        assert_eq!((t[0].y, t[1].y), (48.0, 48.0));
        assert!(t[0].x < t[1].x);
    }

    #[test]
    fn resizing() {
        let r = Rect { x: 10., y: 60., w: 500., h: 400. };
        assert_eq!(resized(r, 'e', 40., 90.), Rect { w: 540., ..r });
        assert_eq!(resized(r, 's', 40., 90.), Rect { h: 490., ..r });
        assert_eq!(resized(r, 'c', -400., -400.), Rect { w: 360., h: 200., ..r });
        assert_eq!(ext_of("a/b.SVG"), "svg");
        assert!(is_source_preview("x.svg") && !is_source_preview("x.png"));
    }
}
