//! The file windows (the web's feBuildWindow, feOpen, feOpenViewer,
//! feArrange; the Slint client's wins.rs): floating over the whole window
//! (a layer the shell draws, crundi_shell::register_overlay). A file in the
//! editor (one window a file; Save and Ctrl+S, "Saving…", then "Saved" a
//! moment), a picture in the viewer. A window drags by its title bar and
//! snaps to the edges and corners (36px zones), expands to fill, and Tile /
//! Grid / Cascade arrange them all. A press brings one to the front.
//!
//! The layer is one cached view over the window (`register_overlay`); a key
//! in an editor redraws the layer only when it turns Save on or off.

use crate::toast;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

const IMAGE_EXTS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "svg"];
// The Slint client's (and the web's) window rules: the top they keep under
// (the top bar), the smallest size, the snap zone's depth.
const TOP: f32 = 48.0;
const MIN_W: f32 = 360.0;
const MIN_H: f32 = 200.0;
const ZONE: f32 = 36.0;

fn ext_of(f: &str) -> String { f.rsplit('.').next().filter(|e| *e != f).unwrap_or("").to_ascii_lowercase() }
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

pub struct FileWin {
    pub id: u32,
    pub kind: &'static str,
    pub project: String,
    pub file: String,
    pub original: String,
    pub save_label: String,
    pub text: Option<Entity<w::EditorState>>,
    pub img: Option<Arc<Image>>,
    /// The picture's own size in pixels (None for SVG): it is shown at most this big.
    pub nat: Option<(u32, u32)>,
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

/// The layer: every window, front last; a drag in progress and its snap zone.
pub struct WinLayer {
    pub wins: Vec<FileWin>,
    next: u32,
    drag: Option<(u32, f32, f32, Rect)>,
    zone: Option<&'static str>,
    vw: f32,
    vh: f32,
}

pub struct LayerHandle(pub Entity<WinLayer>);
impl Global for LayerHandle {}
pub fn layer(cx: &App) -> Option<Entity<WinLayer>> { cx.try_global::<LayerHandle>().map(|h| h.0.clone()) }

impl WinLayer {
    fn find(&mut self, id: u32) -> Option<&mut FileWin> { self.wins.iter_mut().find(|w| w.id == id) }
    pub fn has(&self, id: u32) -> bool { self.wins.iter().any(|w| w.id == id) }

    pub fn front(&mut self, id: u32, cx: &mut Context<Self>) {
        if let Some(i) = self.wins.iter().position(|w| w.id == id) { let w = self.wins.remove(i); self.wins.push(w); cx.notify(); }
    }

    pub fn close(&mut self, id: u32, cx: &mut Context<Self>) { self.wins.retain(|w| w.id != id); cx.notify(); }

    pub fn expand(&mut self, id: u32, cx: &mut Context<Self>) {
        if let Some(w) = self.find(id) { w.max = !w.max; }
        self.front(id, cx);
        cx.notify();
    }

    pub fn arrange(&mut self, mode: &str, cx: &mut Context<Self>) {
        let rects = arrange_rects(mode, self.wins.len(), self.vw, self.vh);
        for (w, r) in self.wins.iter_mut().zip(rects) { w.r = r; w.max = false; }
        cx.notify();
    }

    pub fn set_text(&mut self, id: u32, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = self.find(id).and_then(|w| w.text.clone()) { t.update(cx, |st, cx| st.set_value(text.to_string(), window, cx)); }
        if let Some(wn) = self.find(id) { wn.dirty = wn.is_dirty(cx); }
        cx.notify();
    }

    /// A drag by the title bar: 0 start, 1 move, 2 release (window px).
    pub fn drag(&mut self, id: u32, phase: u8, x: f32, y: f32, cx: &mut Context<Self>) {
        let (vw, vh) = (self.vw, self.vh);
        match phase {
            0 => { if let Some(w) = self.find(id) { w.max = false; let r = w.r; self.drag = Some((id, x, y, r)); self.zone = None; } self.front(id, cx); }
            1 => {
                let Some((did, sx, sy, r)) = self.drag else { return };
                if did != id { return; }
                if let Some(w) = self.find(id) { w.r.x = r.x + x - sx; w.r.y = (r.y + y - sy).max(TOP); }
                self.zone = zone_at(x, y, vw, vh);
            }
            _ => {
                self.drag = None;
                if let Some(r) = self.zone.take().and_then(|z| snap_rect(z, vw, vh)) { if let Some(w) = self.find(id) { w.r = r; w.max = false; } }
            }
        }
        cx.notify();
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
                if let Some(wn) = l.find(id) { if saved { wn.original = content; wn.save_label = "Saved".into(); wn.dirty = wn.is_dirty(cx); } else { wn.save_label = "Save".into(); } }
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

    fn window(&self, wn: &FileWin, cx: &mut Context<Self>) -> AnyElement {
        let id = wn.id;
        let r = if wn.max { Rect { x: 4.0, y: TOP, w: self.vw - 8.0, h: self.vh - TOP - 4.0 } } else { wn.r };
        let dirty = wn.dirty;
        let saving = wn.save_label != "Save";
        let body: AnyElement = if !wn.loading.is_empty() {
            w::loading(wn.loading.clone()).into_any_element()
        } else if let Some(img) = &wn.img {
            // The picture fills the body, kept in proportion (Contain): with
            // only max sizes in a centring box it laid out at nothing (F88).
            // As the web's: at its own size, scaled down only to fit (contain,
            // never enlarged past 1:1; F98), centred.
            let pic = gpui::img(img.clone()).object_fit(ObjectFit::Contain).max_w_full().max_h_full();
            let pic = match wn.nat { Some((w_, h_)) => pic.w(px(w_ as f32)).h(px(h_ as f32)), None => pic.size_full() };
            div().size_full().min_h_0().min_w_0().bg(T::bg()).p(T::SP_12).flex().items_center().justify_center()
                .child(pic).into_any_element()
        } else if let Some(t) = &wn.text {
            div().size_full().font_family(T::MONO).text_size(T::TEXT_13).child(w::code_editor(t).size_full()).into_any_element()
        } else { div().into_any_element() };
        let mut tools = div().flex().flex_none().items_center().gap(T::SP_4).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        if wn.text.is_some() {
            let label = wn.save_label.clone();
            let b = w::primary(("win-save", id), label.clone());
            let b = if label == "Saved" { b.icon(IconName::Check) } else { b };
            tools = tools.child(w::disabled(b, !dirty || saving).on_click(cx.listener(move |l, _, _, cx| l.save(id, cx))));
        }
        tools = tools
            .child(w::icon_button(("win-tile", id), IconName::Columns2, "Tile").on_click(cx.listener(|l, _, _, cx| l.arrange("tile", cx))))
            .child(w::icon_button(("win-grid", id), IconName::Grid2x2, "Grid").on_click(cx.listener(|l, _, _, cx| l.arrange("grid", cx))))
            .child(w::icon_button(("win-cascade", id), IconName::Layers, "Cascade").on_click(cx.listener(|l, _, _, cx| l.arrange("cascade", cx))))
            .child(w::icon_button(("win-max", id), if wn.max { IconName::Minimize2 } else { IconName::Maximize2 }, if wn.max { "Restore" } else { "Expand" }).on_click(cx.listener(move |l, _, _, cx| l.expand(id, cx))))
            .child(w::icon_button(("win-x", id), IconName::X, "Close").on_click(cx.listener(move |l, _, _, cx| l.close(id, cx))));
        let head = div().id(("win-head", id)).flex().flex_none().items_center().gap(T::SP_6).h(T::CONTROL_SM).px(T::SP_10)
            .bg(T::bg_secondary()).border_b_1().border_color(T::border()).cursor_move()
            .on_mouse_down(MouseButton::Left, cx.listener(move |l, e: &MouseDownEvent, _, cx| {
                if e.click_count == 2 { l.expand(id, cx); } else { l.drag(id, 0, f32::from(e.position.x), f32::from(e.position.y), cx); }
            }))
            .child(div().flex_none().text_color(T::text_secondary()).child(w::icon(if wn.kind == "view" { IconName::Image } else { IconName::FileText }, T::ICON_MD)))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).font_family(T::MONO).text_color(T::text()).child(wn.file.clone()))
            .child(tools);
        div().id(("win", id)).occlude().absolute().left(px(r.x)).top(px(r.y)).w(px(r.w)).h(px(r.h))
            .flex().flex_col().bg(T::bg()).border_1().border_color(T::border()).rounded(T::R_CARD).overflow_hidden().shadow_lg()
            .on_mouse_down(MouseButton::Left, cx.listener(move |l, _, _, cx| { if l.wins.last().map(|w| w.id) != Some(id) { l.front(id, cx); } }))
            .on_key_down(cx.listener(move |l, e: &KeyDownEvent, _, cx| if e.keystroke.key == "s" && (e.keystroke.modifiers.control || e.keystroke.modifiers.platform) { l.save(id, cx); }))
            .child(head)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}

impl Render for WinLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("wins");
        let vs = window.viewport_size();
        (self.vw, self.vh) = (f32::from(vs.width), f32::from(vs.height));
        if self.wins.is_empty() { return div().into_any_element(); }
        let me = cx.entity().downgrade();
        let mut root = div().absolute().top_0().left_0().size_full()
            .child(canvas(|_, _, _| {}, move |_, _, window, _| {
                let m = me.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| if phase == DispatchPhase::Bubble {
                    let _ = m.update(cx, |l, cx| if let Some((id, ..)) = l.drag { l.drag(id, 1, f32::from(e.position.x), f32::from(e.position.y), cx) });
                });
                let m = me.clone();
                window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| if phase == DispatchPhase::Bubble {
                    let _ = m.update(cx, |l, cx| if let Some((id, ..)) = l.drag { l.drag(id, 2, f32::from(e.position.x), f32::from(e.position.y), cx) });
                });
            }).absolute().top_0().left_0().size_full());
        // While dragging into a zone: where the window will land.
        if let Some(r) = self.zone.and_then(|z| snap_rect(z, self.vw, self.vh)) {
            root = root.child(div().absolute().left(px(r.x)).top(px(r.y)).w(px(r.w)).h(px(r.h)).rounded(T::R_CARD).border_2().border_color(T::accent()).bg(T::accent_dim()));
        }
        let els: Vec<AnyElement> = self.wins.iter().map(|wn| self.window(wn, cx)).collect();
        root.children(els).into_any_element()
    }
}

pub fn init(cx: &mut App) {
    let l = cx.new(|_| WinLayer { wins: vec![], next: 0, drag: None, zone: None, vw: 0., vh: 0. });
    cx.set_global(LayerHandle(l.clone()));
    crundi_shell::register_overlay(cx, l.into());
}

fn add(kind: &'static str, project: &str, file: &str, cx: &mut App) -> Option<u32> {
    let l = layer(cx)?;
    Some(l.update(cx, |l, cx| {
        l.next += 1;
        let id = l.next;
        let r = new_rect(l.wins.len() as u32, l.vw.max(MIN_W * 2.), l.vh.max(MIN_H * 3.));
        l.wins.push(FileWin { id, kind, project: project.into(), file: file.into(), original: String::new(), save_label: "Save".into(), text: None, img: None, nat: None, loading: String::new(), r, max: false, dirty: false });
        cx.notify();
        id
    }))
}

/// Open a file: a picture in the viewer, anything else in the editor (one
/// window a file: an open one comes to the front).
pub fn open_file(project: &str, file: &str, cx: &mut App) {
    let ext = ext_of(file);
    if IMAGE_EXTS.contains(&ext.as_str()) { return open_viewer(project, file, &ext, cx); }
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
                    l.update(cx, |l, cx| { if let Some(wn) = l.find(id) { wn.original = content; wn.text = Some(st); } cx.notify(); });
                }
            });
        }
        Ok(d) => { let e = s(&d, "error"); toast(cx, if e.is_empty() { "Cannot open file".into() } else { e }, w::Toast::Error) }
        Err(e) => toast(cx, format!("Error: {e}"), w::Toast::Error),
    });
}

/// A picture: a short-lived link, then its bytes.
fn open_viewer(project: &str, file: &str, ext: &str, cx: &mut App) {
    let Some(id) = add("view", project, file, cx) else { return };
    if let Some(l) = layer(cx) { l.update(cx, |l, _| if let Some(wn) = l.find(id) { wn.loading = "Loading…".into(); }); }
    let body = json!({ "project": project, "file": file });
    let format = match ext { "jpg" | "jpeg" => ImageFormat::Jpeg, "gif" => ImageFormat::Gif, "webp" => ImageFormat::Webp, "svg" => ImageFormat::Svg, _ => ImageFormat::Png };
    let Some(api) = crundi_shell::api(cx) else { return };
    crundi_shell::run(cx, move || -> Result<Vec<u8>, String> {
        let d = api.post("/api/files/download-link", &body).map_err(|e| e.to_string())?;
        if !ok(&d) { return Err(s(&d, "error")); }
        let (st, bytes) = api.get_bytes(&format!("{}?inline=1", s(&d, "url"))).map_err(|e| e.to_string())?;
        if st != 200 { return Err("Cannot open file".into()); }
        Ok(bytes)
    }, move |r, cx| if let Some(l) = layer(cx) { l.update(cx, |l, cx| {
        if let Some(wn) = l.find(id) { match r { Ok(b) => { wn.nat = image::ImageReader::new(std::io::Cursor::new(&b)).with_guessed_format().ok().and_then(|r| r.into_dimensions().ok()); wn.img = Some(Arc::new(Image::from_bytes(format, b))); wn.loading.clear(); } Err(e) => wn.loading = if e.is_empty() { "Cannot open file".into() } else { e } } }
        cx.notify();
    }); });
}

pub fn props(cx: &App) -> Vec<Value> {
    layer(cx).map(|l| l.read(cx).wins.iter().map(|w| json!({
        "id": w.id, "kind": w.kind, "title": w.file, "dirty": w.is_dirty(cx), "save": w.save_label, "loading": w.loading, "img": w.img.is_some(),
        "lines": w.text.as_ref().map(|_| w.value(cx).split('\n').count()).unwrap_or(0),
        "x": w.r.x, "y": w.r.y, "w": w.r.w, "h": w.r.h, "max": w.max,
    })).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{arrange_rects, snap_rect, zone_at, Rect};

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
}
