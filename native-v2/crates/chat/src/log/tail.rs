//! The streaming tail (ARCHITECTURE.md, "Virtualised lists": the streaming
//! path). While a reply streams and the log follows its end, a batch mostly
//! grows the reply's last block by a few words: its height stays, and no
//! other row moves. GPUI would still rebuild, lay out and paint every visible
//! row of the log for it (the LogView is the one view the list notifies).
//! So the last block is drawn by a view of its own, the TailView, a sibling
//! of the LogView in the chat's body, at the log's foot where the list puts
//! that row; the list keeps a room of the same height in its place. A batch
//! that only changed that block and kept its height notifies the TailView
//! alone: the LogView is replayed from its cache, the tail is one block.
//! Anything else (a new block, a height that changed, a scroll, a hover, a
//! selection, the end of the stream) redraws the LogView as before, and the
//! TailView with it (it observes the LogView).
//!
//! The height is known before the frame: the store's observer shapes the
//! block at the log's width (the BlockEl's own `lay`, kept in the LogView's
//! `laid` as the list's row would), so the room in the list and the
//! TailView's box are the same height in the same frame.
//!
//! Only a paragraph, a heading, a quote line, a list item or a rule streams
//! this way (one element, measured by shaping); a code block or a table,
//! a log with a goal bar or a parked card, a side question under the reply,
//! or a log not at its end draws the last row in the list as before.

use super::{prose, LogView};
use crate::store::Touch;
use crundi_core::webmd::Kind;
use crundi_widgets as w;
use gpui::*;
use std::{cell::Cell, rc::Rc};

/// The tail being drawn by its own view: its row and its height.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct On { pub ix: usize, pub h: Pixels }

/// Where the last row was laid out, as the list last placed it.
#[derive(Clone, Copy, Default)]
struct Geo {
    /// It ended at the log's foot (following the end, the list taller than the log).
    at_foot: bool,
    /// From its bottom to the log's bottom (the list's room under the last row).
    gap: Pixels,
    /// The list's width (the row's).
    width: Pixels,
    rows: usize,
    /// (The driver's: the log's bottom, the last row's, the room.)
    seen: (f32, f32, f32),
}

#[derive(Default)]
pub struct Tail {
    /// The pane's log watches its tail (ChatPane: a second log does not).
    pub(crate) watched: bool,
    pub(crate) view: Option<Entity<TailView>>,
    pub(crate) on: Option<On>,
    /// The tail as this render of the LogView places its room (its rows are
    /// made while the list holds its state: no asking the list then).
    pub(crate) drawn: Option<On>,
    seen: Option<Touch>,
    geo: Rc<Cell<Geo>>,
    /// The last row's bounds as the list laid it out this frame (a canvas on
    /// that row: the list's own arithmetic is provisional while it follows).
    foot: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The list's room under the last row (the agents' dock), as rendered.
    pub(crate) room: Pixels,
    /// The last row the list made in its last layout (overdraw included),
    /// and whether "Go to the newest message" showed: a reader up the log.
    pub(crate) max_ix: usize,
    pub(crate) jump_shown: bool,
    /// As the LogView last drew: a text box of a card had the keys (no paint
    /// layer), a goal bar or a parked card boxed the log.
    pub(crate) typing: bool,
    pub(crate) boxed: bool,
    /// The activity row's own view (activity.rs), the dock's room under it
    /// as last rendered, whether this render draws it in the list, and
    /// whether a turn was running when last drawn.
    pub(crate) act_view: Option<Entity<super::activity::ActivityView>>,
    pub(crate) dock_room: Pixels,
    pub(crate) act_inline: bool,
    pub(crate) act_seen: bool,
}

impl Tail {
    /// The last row ended at the log's foot in the last layout.
    pub(crate) fn at_foot(&self) -> bool { self.geo.get().at_foot }
}

/// The pane's log draws its streaming tail with a view of its own (ChatPane).
pub fn watch(log: &Entity<LogView>, window: &mut Window, cx: &mut App) {
    let weak = log.downgrade();
    let view = cx.new(|cx: &mut Context<TailView>| {
        let sub = cx.observe(log, |_, _, cx| cx.notify());
        TailView { log: weak, _sub: sub }
    });
    let act = cx.new(|cx: &mut Context<super::activity::ActivityView>| {
        let sub = cx.observe(log, |_, _, cx| cx.notify());
        super::activity::ActivityView { log: log.downgrade(), _sub: sub }
    });
    log.update(cx, |l, cx| {
        l.tail.watched = true;
        l.tail.view = Some(view);
        l.tail.act_view = Some(act);
        let store = l.store.clone();
        l._subs.push(cx.observe_in(&store, window, |this: &mut LogView, _, window, cx| changed(this, window, cx)));
    });
}

/// The store changed: the tail alone, or the whole log.
fn changed(v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) {
    let dot = v.ui.jump.new;
    prose::arrived(v, cx);
    let touch = v.store.read(cx).touch;
    let fresh = v.tail.seen.is_none_or(|s| s.seq != touch.seq);
    v.tail.seen = Some(touch);
    let n = v.store.read(cx).rows.len();
    // The reader is up the log ("Go to the newest message" showing, its dot
    // as it was) and the batch touched only rows below what the list made:
    // nothing on screen changed, the log is not redrawn (its list measures
    // those rows when it next lays them out).
    let below = fresh && !touch.all && touch.from != usize::MAX && touch.from > v.tail.max_ix && n >= touch.before;
    if below && !v.list.is_following_tail() && v.tail.jump_shown && dot == v.ui.jump.new {
        v.tail.on = None;
        return;
    }
    // Only the last row changed, and the rows are as many as before.
    let only_last = fresh && !touch.all && n > 0 && touch.before == n && touch.from == n - 1;
    let next = candidate(v, window, cx);
    let same = only_last && next.is_some() && next == v.tail.on;
    v.tail.on = next;
    if same && v.tail_live(cx).is_some() {
        if let Some(t) = v.tail.view.clone() { t.update(cx, |_, cx| cx.notify()); }
    } else {
        cx.notify();
    }
}

/// The last row, if it may draw itself, and its height at the log's width.
fn candidate(v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<On> {
    let g = v.tail.geo.get();
    if !v.tail.watched || v.tail.boxed || !g.at_foot || g.width <= px(0.) || !v.list.is_following_tail() { return None; }
    let (r, ix) = {
        let st = v.store.read(cx);
        let n = st.rows.len();
        // (Rows appended since the list's layout land at the same foot: it
        // follows the end. Rows gone may leave it short of the foot.)
        if n == 0 || n < g.rows || !st.log.asides.is_empty() { return None; }
        (st.rows[n - 1].clone(), n - 1)
    };
    let b = r.block.as_ref()?;
    if r.shown.kind != "assistant" || r.shown.status != "streaming" || !b.last || matches!(b.kind, Kind::Code | Kind::Table { .. }) { return None; }
    let h = height(v, ix, &r, g.width, window, cx)?;
    Some(On { ix, h })
}

/// The row's height as the list measures it (LogView::row_el's assistant
/// path: the margins, the caret's room, the block's lines at this width).
fn height(v: &mut LogView, ix: usize, r: &crate::store::Row, width: Pixels, window: &mut Window, cx: &mut Context<LogView>) -> Option<Pixels> {
    let fs = v.fs();
    let (above, below) = v.block_margins(ix, r, fs);
    let caret = prose::caret_of(v, r, cx);
    let kind = r.block.as_ref().map_or(Kind::Para, |b| b.kind.clone());
    let below = match caret { Some(_) => px(super::text::caret_room(&kind, fs) + 10.), None => below };
    let sf = window.scale_factor();
    let pad = (super::text::snap(above, sf), super::text::snap(below, sf));
    let el = v.block_raw(ix, r, fs, pad, sf, caret)?;
    let h = el.lay(width, window);
    Some(el.pad.0 + h + el.pad.1)
}

/// The LogView's render: where its last row will be laid out is read after
/// the list's layout (the track canvas' prepaint, after the list's).
pub(crate) fn measure(v: &LogView, cx: &App) -> impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) + 'static {
    let (geo, foot, list, rows, room) = (v.tail.geo.clone(), v.tail.foot.clone(), v.list.clone(), v.store.read(cx).rows.len(), v.tail.room);
    foot.set(None);
    move |_, _, _| {
        // (The list's own bounds: the log's, the row's width.)
        let log = list.viewport_bounds();
        let width = log.size.width;
        let seen = (f32::from(log.bottom()), foot.get().map_or(-1., |b| f32::from(b.bottom())), f32::from(room));
        let g = match foot.get() {
            // At the foot: following the end, the last row's bottom on the
            // list's room (a short log's rows sit at its top instead).
            Some(b) if list.is_following_tail() && (log.bottom() - room - b.bottom()).abs() < px(0.5) =>
                Geo { at_foot: true, gap: log.bottom() - b.bottom(), width, rows, seen },
            _ => Geo { width, rows, seen, ..Geo::default() },
        };
        geo.set(g);
    }
}

/// The list's last row, wrapped so its bounds are known after the list's
/// layout (`measure`); other rows as they are.
pub(crate) fn mark_last(v: &LogView, ix: usize, n: usize, el: AnyElement) -> AnyElement {
    if ix + 1 != n || !v.tail.watched { return el; }
    let foot = v.tail.foot.clone();
    div().relative().w_full().flex().flex_col().child(el)
        .child(canvas(move |b, _, _| foot.set(Some(b)), |_, _, _, _| {}).absolute().top_0().left_0().size_full())
        .into_any_element()
}

/// The driver's view of it: the tail drawn by its own view (row, height),
/// and where the list last put the last row.
pub(crate) fn state(v: &LogView, cx: &App) -> serde_json::Value {
    let g = v.tail.geo.get();
    serde_json::json!({ "live": v.tail_live(cx).map(|t| serde_json::json!({ "row": t.ix, "h": f32::from(t.h) })), "atFoot": g.at_foot, "gap": f32::from(g.gap), "width": f32::from(g.width), "rows": g.rows, "seen": [g.seen.0, g.seen.1, g.seen.2] })
}

/// The blink turned: the tail's view redraws its caret, if it draws it.
pub(crate) fn redraw(v: &LogView, cx: &mut Context<LogView>) -> bool {
    if v.tail_live(cx).is_none() { return false; }
    let Some(t) = v.tail.view.clone() else { return false };
    t.update(cx, |_, cx| cx.notify());
    true
}

impl LogView {
    /// The tail drawn by its own view this frame (the body and the list ask
    /// the same: the reader scrolling up ends it before the frame).
    pub(crate) fn tail_live(&self, cx: &App) -> Option<On> {
        let t = self.tail.on?;
        let n = self.store.read(cx).rows.len();
        (self.tail.watched && !self.tail.boxed && t.ix + 1 == n && self.list.is_following_tail()).then_some(t)
    }

    /// For the chat's body: the tail's view, its height and its gap above
    /// the log's bottom, while it draws itself.
    pub fn tail_place(&self, cx: &App) -> Option<(Entity<TailView>, Pixels, Pixels)> {
        let t = self.tail_live(cx)?;
        Some((self.tail.view.clone()?, t.h, self.tail.geo.get().gap))
    }

    /// The TailView's element: the row as the list would draw it.
    fn tail_el(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.tail_live(cx)?;
        let r = self.store.read(cx).rows.get(t.ix).cloned()?;
        self.hits.borrow_mut().retain(|h| h.row != t.ix);
        let el = self.row_el(t.ix, &r, window, cx);
        prose::blink_after(self, cx);
        Some(if self.tail.typing { el } else { w::layer(el).into_any_element() })
    }
}

/// The streaming tail's view (cached by the chat's body at the log's foot).
pub struct TailView { log: WeakEntity<LogView>, _sub: Subscription }

impl Render for TailView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("log-tail");
        let el = self.log.upgrade().and_then(|l| l.update(cx, |l, cx| l.tail_el(window, cx)));
        div().size_full().flex().flex_col().children(el)
    }
}
