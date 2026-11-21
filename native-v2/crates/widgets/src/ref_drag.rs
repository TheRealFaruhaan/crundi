//! Dragging a reference between panels (the web's `data-drag-ref`, its
//! `makeDraggable` + `dragRefHandlers`, `wbDropTargetAt`, `insertRefToTarget`
//! and `wbAutoScroll`), done once here for every area (F84):
//!
//! - the source: `drag_ref` (a Files or Git row, a Kanban card or subtask, a
//!   Mindmap idea, a Media card, a Notes block). The ghost is the row itself
//!   under the pointer (`drag_ref_row`, the web's `drag-clone` at 0.92) or a
//!   pill with its name (`drag_ref`).
//! - the drop targets: `ref_target` / `ref_target_drag` (a chat's box, a
//!   terminal's screen and input, a notes page, Cru): a dashed accent outline
//!   while a reference is over it (the web's `.term-drop-hover`), the
//!   reference handed over on the drop. The text dropped is labelled with its
//!   kind ("[File Path: /a/b]", the web's DRAG_LABELS); a note takes a path's
//!   plain value (`RefDrag::plain`).
//! - edge auto-scroll while anything is dragged (`edge_scroll`): within 60 px
//!   of a scrolling container's edge it scrolls, faster nearer the edge (up to
//!   22 px a frame at 60 Hz), and keeps scrolling while the pointer rests
//!   there, until the drag ends or the pointer leaves the edge.

use crundi_theme::T;
use gpui::*;
use std::{cell::Cell, rc::Rc, time::Duration};

/// What is dragged: its kind and value (the text a drop inserts is
/// `text`), and the name the pill ghost shows.
#[derive(Clone)]
pub struct RefDrag { pub text: SharedString, pub label: SharedString, pub kind: SharedString, pub value: SharedString }

impl RefDrag {
    pub fn new(kind: &str, value: &str, label: impl Into<SharedString>) -> Self {
        RefDrag { text: ref_text(kind, value).into(), label: label.into(), kind: kind.to_string().into(), value: value.to_string().into() }
    }
    /// What a note takes: a path plain (it reads as a path), anything else labelled.
    pub fn plain(&self) -> SharedString {
        if matches!(self.kind.as_ref(), "file" | "folder" | "media") { self.value.clone() } else { self.text.clone() }
    }
}

/// The web's labels (DRAG_LABELS) around a value: "[File Path: /a/b]".
pub fn ref_text(kind: &str, value: &str) -> String {
    let label = match kind {
        "file" => "File Path", "folder" => "Folder Path", "kanban-task" => "Kanban Task ID",
        "kanban-subtask" => "Kanban Subtask ID", "mindmap" => "Mindmap Idea ID", "media" => "Media Path",
        _ => return value.to_string(),
    };
    format!("[{label}: {value}]")
}

/// The pill ghost under the pointer while dragging: the name.
pub struct RefGhost(SharedString);
impl Render for RefGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px(T::SP_8).py(T::SP_4).rounded(T::R_PILL).bg(T::bg_tertiary()).border_1().border_color(T::accent())
            .text_size(T::TEXT_12).text_color(T::text()).child(self.0.clone())
    }
}

/// The row's own look as the ghost (the web's drag-clone: the row, at the
/// row's width, at 0.92, held where it was grabbed).
pub struct RowGhost { row: Rc<dyn Fn() -> AnyElement>, width: Pixels, grab: Point<Pixels> }
impl Render for RowGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().ml(-self.grab.x).mt(-self.grab.y).w(self.width).opacity(T::DRAG_GHOST_O).child((self.row)())
    }
}

/// Make `el` a drag source for `kind` and `value` (the pill ghost says `label`).
pub fn drag_ref(el: Stateful<Div>, kind: &str, value: &str, label: impl Into<SharedString>) -> Stateful<Div> {
    el.on_drag(RefDrag::new(kind, value, label), |d, _, _, cx| cx.new(|_| RefGhost(d.label.clone())))
}

/// A drag source whose ghost is the row itself (`row` draws it, `width` wide).
pub fn drag_ref_row(el: Stateful<Div>, kind: &str, value: &str, label: impl Into<SharedString>, width: Pixels, row: impl Fn() -> AnyElement + 'static) -> Stateful<Div> {
    let row: Rc<dyn Fn() -> AnyElement> = Rc::new(row);
    el.on_drag(RefDrag::new(kind, value, label), move |_, grab, _, cx| { let row = row.clone(); cx.new(|_| RowGhost { row, width, grab }) })
}

/// Accept a dragged reference (`put` gets its labelled text).
pub fn ref_target(el: Stateful<Div>, put: impl Fn(&str, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    ref_target_drag(el, move |d, window, cx| put(&d.text, window, cx))
}

/// Accept a dragged reference (`put` gets the whole of it: a note takes
/// `plain()`), with the dashed accent outline while one is over it.
pub fn ref_target_drag(el: Stateful<Div>, put: impl Fn(&RefDrag, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    el.drag_over::<RefDrag>(|s, _, _, _| s.border_2().border_dashed().border_color(T::accent()))
        .on_drop(move |d: &RefDrag, window, cx| { put(d, window, cx); cx.stop_propagation(); })
}

/// What `edge_scroll` scrolls.
#[derive(Clone)]
pub enum Scroller {
    /// A div's `overflow_*_scroll` with `track_scroll`.
    Handle(ScrollHandle),
    /// A `gpui::list` (vertical).
    List(ListState),
    /// A `uniform_list` (vertical).
    Uniform(UniformListScrollHandle),
}

const EDGE: f32 = 60.;
const SPEED: f32 = 22.;
const TICK: Duration = Duration::from_millis(16);

/// How far to move this tick, from the pointer and the box (the web's
/// wbScrollTick: ramped by how deep into the 60 px band the pointer is).
fn delta(p: Point<Pixels>, b: Bounds<Pixels>, x: bool, y: bool) -> Point<Pixels> {
    let ramp = |d: f32| SPEED * (d / EDGE).clamp(0., 1.);
    let (l, r, t, bt) = (f32::from(b.left()), f32::from(b.right()), f32::from(b.top()), f32::from(b.bottom()));
    let (px_, py_) = (f32::from(p.x), f32::from(p.y));
    let mut d = point(px(0.), px(0.));
    if !b.contains(&p) { return d; }
    if x { if px_ > r - EDGE { d.x = px(ramp(px_ - (r - EDGE))); } else if px_ < l + EDGE { d.x = px(-ramp(l + EDGE - px_)); } }
    if y { if py_ > bt - EDGE { d.y = px(ramp(py_ - (bt - EDGE))); } else if py_ < t + EDGE { d.y = px(-ramp(t + EDGE - py_)); } }
    d
}

/// Move `s` by `d` (content moves the other way: a positive d scrolls on).
fn scroll(s: &Scroller, d: Point<Pixels>) -> bool {
    match s {
        Scroller::Handle(h) => {
            let (o, m) = (h.offset(), h.max_offset());
            let n = point((o.x - d.x).clamp(-m.x, px(0.)), (o.y - d.y).clamp(-m.y, px(0.)));
            if n == o { return false; }
            h.set_offset(n);
            true
        }
        Scroller::List(l) => { if d.y == px(0.) { return false; } l.scroll_by(d.y); true }
        Scroller::Uniform(u) => {
            let h = u.0.borrow().base_handle.clone();
            scroll(&Scroller::Handle(h), point(px(0.), d.y))
        }
    }
}

/// One tick of the edge ramp, for a drag a view tracks itself (a pane moved
/// by its head, F181): how far `s` moves for a pointer at `p` in `b` (60 px
/// band, 22 px a tick, ramped), and whether it moved. Call every `EDGE_TICK`.
pub fn edge_step(s: &Scroller, b: Bounds<Pixels>, p: Point<Pixels>, x: bool, y: bool) -> (bool, bool) {
    let d = delta(p, b, x, y);
    if d == Point::default() { return (false, false); }
    (true, scroll(s, d))
}
/// The edge ramp's tick.
pub const EDGE_TICK: Duration = TICK;

type EdgeKey = (EntityId, Option<ElementId>, std::any::TypeId, bool, bool);
type EdgeCells = (Rc<Cell<(Point<Pixels>, Bounds<Pixels>)>>, Rc<Cell<bool>>);
thread_local! {
    static EDGE_STATE: std::cell::RefCell<std::collections::HashMap<EdgeKey, EdgeCells>> = Default::default();
}

/// Scroll `s` while something of type `D` is dragged near the element's
/// edges (`x` / `y`: the axes it scrolls on); `view` is the view that draws
/// it (notified when it moved: nothing else redraws).
pub fn edge_scroll<D: 'static>(mut el: Stateful<Div>, s: Scroller, x: bool, y: bool, view: EntityId) -> Stateful<Div> {
    // The pointer and the running flag outlive a render (F226): a loop that
    // is already running reads the pointer the redrawn element records, and
    // no second loop starts beside it. Keyed by the view, the element's id
    // and the drag's type.
    let key = (view, el.interactivity().element_id.clone(), std::any::TypeId::of::<D>(), x, y);
    let (at, running) = EDGE_STATE.with(|m| m.borrow_mut().entry(key).or_insert_with(|| {
        (Rc::new(Cell::new((Point::default(), Bounds::default()))), Rc::new(Cell::new(false)))
    }).clone());
    el.on_drag_move::<D>(move |e: &DragMoveEvent<D>, window, cx| {
        at.set((e.event.position, e.bounds));
        if running.get() || delta(e.event.position, e.bounds, x, y) == Point::default() { return; }
        running.set(true);
        let (at, running, s) = (at.clone(), running.clone(), s.clone());
        window.spawn(cx, async move |cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let go = cx.update(|_, cx| {
                    if !cx.has_active_drag() { return false; }
                    let (p, b) = at.get();
                    let d = delta(p, b, x, y);
                    if d == Point::default() { return false; }
                    if scroll(&s, d) { cx.notify(view); }
                    true
                }).unwrap_or(false);
                if !go { break; }
            }
            running.set(false);
        }).detach();
    })
}

#[cfg(test)]
mod tests {
    use gpui::{point, px, size, Bounds};
    #[test]
    fn labels() {
        assert_eq!(super::ref_text("file", "/p/a.rs"), "[File Path: /p/a.rs]");
        assert_eq!(super::ref_text("folder", "/p/src"), "[Folder Path: /p/src]");
        assert_eq!(super::ref_text("other", "x"), "x");
        let d = super::RefDrag::new("media", "/m/a.png", "a.png");
        assert_eq!(d.plain().as_ref(), "/m/a.png");
        assert_eq!(super::RefDrag::new("kanban-task", "t1", "T").plain().as_ref(), "[Kanban Task ID: t1]");
    }
    #[test]
    fn edge_ramp() {
        let b = Bounds { origin: point(px(0.), px(0.)), size: size(px(400.), px(400.)) };
        assert_eq!(super::delta(point(px(200.), px(200.)), b, true, true), point(px(0.), px(0.)));
        assert_eq!(super::delta(point(px(200.), px(400.) - px(1.)), b, false, true).y, px(22. * 59. / 60.));
        assert_eq!(super::delta(point(px(200.), px(30.)), b, false, true).y, px(-11.));
        assert_eq!(super::delta(point(px(500.), px(399.)), b, true, true), point(px(0.), px(0.)));
    }
}
