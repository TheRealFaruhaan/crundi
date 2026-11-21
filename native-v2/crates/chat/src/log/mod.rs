//! The chat log (ARCHITECTURE.md, "Virtualised lists" and "Text and
//! selection"): `gpui::list` over the store's rows, only the rows on screen
//! laid out, a row measured again only when the store splices it; following
//! the end (FollowMode::Tail) until the person scrolls up.
//!
//! Claude's text is drawn as the web draws it (core::webmd, the CSS of
//! app/vendor/claude-chat.js's `.cc-assistant`): one row per block, the
//! margins the browser would leave between them (collapsed, as CSS does).
//!
//! One selection across the whole log, as in the web (F15): a drag selects
//! from block to block and message to message; double-click a word,
//! triple-click a block; shift-click extends; a drag past the top or the
//! bottom scrolls the log (the web's "Selection stays in the conversation");
//! Ctrl+C (Cmd+C) copies. The selection is kept as places in the text
//! (entry, block, piece, byte), so it survives scrolling and streaming; each
//! text drawn reports its layout after it is laid out (a canvas after it),
//! and the pointer is mapped onto those (Zed's markdown does the same).
//!
//! The log's features (PLAN.md, Chat: log; app/vendor/claude-chat.js is the
//! reference, native-gpui's chat-log the port it follows), one module each:
//!
//! | module | PLAN.md | what |
//! |---|---|---|
//! | text | F10/F31 | Claude's blocks as one element each, the runs, the caret |
//! | shared | — | what every log of one chat shares (in its store) |
//! | tools | 2.14 | tool cards, groups under their summary line, agent rows |
//! | thinking | 2.15 | thinking blocks |
//! | prose | 2.16 | bubble, results, errors, notices, the streaming caret, "Go to the newest message" |
//! | copy | 2.16 | the copy button on code and links, the link menu |
//! | md | — | markdown drawn outside the list (the plan, a side answer, an agent's transcript) |
//! | questions | 2.13 | question cards and permission cards |
//! | media | 2.16m | what a show_* call showed, the picture viewer, downloads |
//! | frames | 2.16f | live frames (the Desktop area's browser, through a hook) |
//! | suggest | 2.17 | suggestions and recaps |
//! | agents | 2.18 | the agent dock, its list and panel |
//! | goal | 2.19 | the goal bar, the plan panel |
//! | cards | 2.20 | a factory's result card, a widget check's screenshots |
//! | aside | 2.21 | side questions |
//! | resume | 2.22 | the resume picker (the launcher's chooser) |
//! | parked | 2.23 | parked panes |
//! | driver | — | the log's test-driver actions and state |

pub mod text;
pub mod shared;
pub mod tools;
pub mod thinking;
pub mod prose;
pub mod copy;
pub mod md;
pub mod questions;
pub mod media;
pub mod frames;
pub mod suggest;
pub mod agents;
pub mod goal;
pub mod cards;
pub mod aside;
pub mod resume;
pub mod parked;
pub mod driver;
pub mod tail;

pub use shared::{follow, Shared};
pub use goal::{open_plan, plan_status};

use crate::store::{scale, ChatStore, Row};
use crundi_core::webmd::{Inline, Kind, Link};
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use std::{cell::RefCell, collections::HashSet, ops::Range, rc::Rc, time::Duration};
use text::*;

/// A place in the log's text: the entry, its block row, the piece, the byte.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pos { id: String, blk: usize, part: usize, ix: usize }

/// A place as (row, piece, byte): ordered as the log reads.
pub type At = (usize, usize, usize);

#[derive(Clone, Copy, PartialEq)]
enum Unit { Char, Word, Block }

/// How far from the log's top or bottom a drag scrolls it (the web's SEL_EDGE).
const EDGE: f32 = 56.;

pub struct LogView {
    pub(crate) store: Entity<ChatStore>,
    pub(crate) list: ListState,
    /// Tool cards and groups ("g:<group>") the reader opened, by entry id.
    pub(crate) open: HashSet<String>,
    /// The pane's text size step (its meta's), as last drawn.
    pub(crate) zoom: Option<i32>,
    pub(crate) focus: FocusHandle,
    /// The selection: where it started, where it ends now.
    sel: Option<(Pos, Pos)>,
    /// A press being dragged (its unit: a double-click drags by words).
    drag: Option<(Unit, At)>,
    /// The texts drawn in the last frame.
    hits: Rc<RefCell<Vec<Hit>>>,
    /// The pointer, while dragging; whether the edge scroll is running.
    mouse: Point<Pixels>,
    edge: bool,
    edge_task: Option<Task<()>>,
    over_link: bool,
    /// The selection as rows, for this render.
    range: Option<(At, At)>,
    /// Each block element's layout, by its block (BlockEl; F31).
    laid: std::collections::HashMap<usize, Slot>,
    tables: std::collections::HashMap<usize, TSlot>,
    /// The path under the pointer: (row, piece, its range), drawn in the hover colour.
    pub(crate) hot: Option<(usize, usize, Range<usize>)>,
    /// The features' own state for this log (a pane's and a bubble's differ).
    pub(crate) ui: Ui,
    /// This log notes where its suggestion, badges and parked card are, for
    /// Mini (the chat pane's log; not a second one's).
    pub(crate) spots: bool,
    /// The streaming tail (tail.rs): the reply's last block drawn by a view
    /// of its own while the log follows the end.
    pub(crate) tail: tail::Tail,
    _subs: Vec<Subscription>,
}

/// Each feature's state for one log (the store's `log` holds what logs share).
#[derive(Default)]
pub(crate) struct Ui {
    pub copy: copy::Hint,
    pub jump: prose::Jump,
    pub blink: prose::Blink,
    pub agents: agents::View,
    pub goal: goal::View,
    pub parked: parked::View,
    pub questions: questions::View,
    pub md: md::Memo,
}

impl LogView {
    pub(crate) fn new(store: Entity<ChatStore>, cx: &mut Context<Self>) -> Self {
        let list = store.read(cx).list.clone();
        let projects = crundi_shell::stores(cx).projects.clone();
        let subs = vec![
            // A store change redraws the log; something new below while the
            // reader is up the log lights the jump button's dot.
            // (Once the pane watches the tail, its own observer decides: tail.rs.)
            cx.observe(&store, |this: &mut Self, _, cx| if !this.tail.watched { prose::arrived(this, cx); cx.notify(); }),
            // The parked card follows the projects store (a chat parked, or live again).
            cx.observe(&projects, |this: &mut Self, _, cx| parked::follow(this, cx)),
            // The text size changes every row.
            cx.observe(&store.read(cx).meta.clone(), |this: &mut Self, m, cx| { let z = m.read(cx).0.zoom; if this.zoom != Some(z) { this.zoom = Some(z); cx.notify(); } }),
        ];
        // A text box in a card taking or losing the keyboard: the paint layer follows.
        let focus = cx.focus_handle();
        let zoom = Some(store.read(cx).meta.read(cx).0.zoom);
        let seen = store.read(cx).conv.entries.len();
        let mut v = LogView { store, list, open: Default::default(), zoom, focus, sel: None, drag: None, hits: Default::default(), mouse: Point::default(), edge: false, edge_task: None, over_link: false, range: None, laid: Default::default(), tables: Default::default(), hot: None, ui: Ui::default(), spots: false, tail: Default::default(), _subs: subs };
        v.ui.jump.seen = seen;
        parked::follow(&mut v, cx);
        v
    }

    /// The chat's id.
    pub fn chat(&self, cx: &App) -> String { self.store.read(cx).id.clone() }

    /// A spot for Mini over its parent ("<what>:<chat>"), in this log's group; None for a second log.
    pub(crate) fn spot(&self, what: &str, cx: &mut App) -> Option<Canvas<()>> {
        if !self.spots { return None; }
        let id = self.chat(cx);
        Some(crundi_shell::spots::spot(cx, &format!("log:{id}"), format!("{what}:{id}")))
    }

    /// Open or close a card or a group (its head clicked): its rows measured again in this log.
    pub(crate) fn toggle(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.open.remove(id) { self.open.insert(id.to_string()); }
        match id.strip_prefix("g:") {
            Some(g) => { let ms = self.store.read(cx).log.groups.get(g).cloned().unwrap_or_default(); for m in ms { self.remeasure(&m, cx); } }
            None => self.remeasure(id, cx),
        }
        cx.notify();
    }

    /// An entry's rows measured again in this log only.
    pub(crate) fn remeasure(&self, id: &str, cx: &App) {
        if let Some((s, n)) = self.store.read(cx).span_of(id) { if n > 0 { self.list.splice(s..s + n, n); } }
    }

    /// The pane's text scale.
    pub(crate) fn fs(&self) -> f32 { scale(self.zoom.unwrap_or(0)) }

    /// This log's list (the driver's scroll).
    pub fn list(&self) -> &ListState { &self.list }

    /// A parked chat hides its composer unless peeked at (2.23; the pane's body asks).
    pub fn hides_composer(&self) -> bool { self.ui.parked.parked && !self.ui.parked.peek }

    // ─── Selection ───

    fn at_of(&self, p: &Pos, cx: &App) -> Option<At> { self.store.read(cx).row_index(&p.id, p.blk).map(|r| (r, p.part, p.ix)) }

    fn pos_of(&self, at: At, cx: &App) -> Option<Pos> {
        let r = self.store.read(cx).rows.get(at.0)?;
        Some(Pos { id: r.id.clone(), blk: r.block.as_ref().map_or(0, |b| b.ix), part: at.1, ix: at.2 })
    }

    /// The selection in reading order (None: nothing, or nothing between its ends).
    fn ordered(&self, cx: &App) -> Option<(At, At)> {
        let (a, b) = self.sel.as_ref()?;
        let (a, b) = (self.at_of(a, cx)?, self.at_of(b, cx)?);
        let (s, e) = if a <= b { (a, b) } else { (b, a) };
        (s != e).then_some((s, e))
    }

    /// The selected text (what Ctrl+C copies).
    pub fn selected_text(&self, cx: &App) -> String {
        match self.ordered(cx) { Some((s, e)) => selected_text(&self.store.read(cx).rows, s, e), None => String::new() }
    }

    /// The texts on screen and where they are (the driver's, to aim a drag).
    pub fn text_rects(&self) -> Vec<(usize, usize, Bounds<Pixels>, String)> {
        self.hits.borrow().iter().map(|h| (h.row, h.part, h.layout.bounds(), h.layout.text())).collect()
    }

    /// The text under or nearest to a point of the window.
    fn hit(&self, p: Point<Pixels>, strict: bool) -> Option<At> {
        let hits = self.hits.borrow();
        let ix = |l: &HitLayout| match l.index_for_position(p) { Ok(i) | Err(i) => i.min(l.len()) };
        if let Some(h) = hits.iter().find(|h| h.layout.bounds().contains(&p)) { return Some((h.row, h.part, ix(&h.layout))); }
        if strict { return None; }
        let band = hits.iter().filter(|h| { let b = h.layout.bounds(); p.y >= b.top() && p.y <= b.bottom() })
            .min_by(|a, b| xdist(a, p).partial_cmp(&xdist(b, p)).unwrap_or(std::cmp::Ordering::Equal));
        if let Some(h) = band { return Some((h.row, h.part, ix(&h.layout))); }
        let mut v: Vec<&Hit> = hits.iter().collect();
        v.sort_by_key(|h| (h.row, h.part));
        if let Some(h) = v.iter().find(|h| h.layout.bounds().top() > p.y) { return Some((h.row, h.part, 0)); }
        v.last().map(|h| (h.row, h.part, h.layout.len()))
    }

    fn press(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let Some(at) = self.hit(e.position, false) else { if self.sel.take().is_some() { cx.notify(); } return };
        let unit = match e.click_count { 2 => Unit::Word, n if n >= 3 => Unit::Block, _ => Unit::Char };
        let (a, b) = self.span(at, unit, cx);
        match (&self.sel, e.modifiers.shift && unit == Unit::Char) {
            (Some((anchor, _)), true) => { let anchor = anchor.clone(); if let Some(h) = self.pos_of(b, cx) { self.sel = Some((anchor, h)); } }
            _ => self.sel = self.pos_of(a, cx).zip(self.pos_of(b, cx)),
        }
        self.drag = Some((unit, a));
        self.mouse = e.position;
        cx.notify();
    }

    /// The span a press selects: a place, its word, or its whole piece.
    fn span(&self, at: At, unit: Unit, cx: &App) -> (At, At) {
        let rows = &self.store.read(cx).rows;
        let text = rows.get(at.0).and_then(|r| r.parts.get(at.1)).map(|t| t.as_ref()).unwrap_or("");
        match unit {
            Unit::Char => (at, at),
            Unit::Block => ((at.0, at.1, 0), (at.0, at.1, text.len())),
            Unit::Word => { let (s, e) = word(text, at.2); ((at.0, at.1, s), (at.0, at.1, e)) }
        }
    }

    fn drag_to(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((unit, from)) = self.drag else { return };
        self.mouse = p;
        if let Some(at) = self.hit(p, false) {
            let (anchor, head) = if unit == Unit::Char { (None, at) } else {
                // By words or blocks: the anchor's span stays whole.
                let (fa, fb) = self.span(from, unit, cx);
                let (ha, hb) = self.span(at, unit, cx);
                if at < from { (Some(fb), ha) } else { (Some(fa), hb) }
            };
            let head = self.pos_of(head, cx);
            let anchor = match anchor { Some(a) => self.pos_of(a, cx), None => self.sel.as_ref().map(|s| s.0.clone()) };
            let new = anchor.zip(head);
            if new != self.sel { self.sel = new; cx.notify(); }
        }
        let vb = self.list.viewport_bounds();
        let near = p.y < vb.top() + px(EDGE) || p.y > vb.bottom() - px(EDGE);
        if near && !self.edge {
            self.edge = true;
            // While the pointer stays by an edge: scroll each frame, then
            // stop (nothing runs while nothing moves, rule 6).
            self.edge_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                if !this.update(cx, |this, cx| this.edge_tick(cx)).unwrap_or(false) { break; }
            }));
        }
    }

    fn edge_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let vb = self.list.viewport_bounds();
        let p = self.mouse;
        let d = if p.y < vb.top() + px(EDGE) { p.y - (vb.top() + px(EDGE)) } else if p.y > vb.bottom() - px(EDGE) { p.y - (vb.bottom() - px(EDGE)) } else { px(0.) };
        if self.drag.is_none() || d == px(0.) { self.edge = false; return false; }
        self.list.scroll_by((d * 0.5).clamp(px(-40.), px(40.)));
        cx.notify();
        // The rows moved under the pointer: where it points now (in the
        // frame just drawn; after this notify's frame, the next tick).
        self.drag_to(p, cx);
        true
    }

    fn release(&mut self, e: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some((unit, from)) = self.drag.take() else { return };
        self.edge = false;
        self.edge_task = None;
        if self.ordered(cx).is_none() {
            // A click leaves the caret where it was (the selection's anchor,
            // empty): a shift-click then selects up to its point, as the
            // browser's selection does.
            // A click (not a drag) on a link opens it.
            if unit == Unit::Char && e.click_count <= 1 && self.hit(e.position, true).is_some_and(|at| at == from) {
                if let Some(l) = link_at(&self.store.read(cx).rows, from) { open_link(&l, cx); }
            }
        }
        cx.notify();
    }

    fn hover(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.drag.is_some() { return; }
        let at = self.hit(e.position, true);
        let link = at.and_then(|at| link_span(&self.store.read(cx).rows, at).map(|(rg, l)| (at, rg, l)));
        let mut changed = false;
        if link.is_some() != self.over_link { self.over_link = link.is_some(); changed = true; }
        // A path takes the hover colour (the web's `.cc-path:hover`).
        let hot = link.as_ref().filter(|l| matches!(l.2, Link::Path(_))).map(|(at, rg, _)| (at.0, at.1, rg.clone()));
        if hot != self.hot { self.hot = hot; changed = true; }
        changed |= copy::hover(self, e.position, at, cx);
        if changed { cx.notify(); }
    }

    /// The pointer left the log: the hover colour goes, the copy button soon after.
    fn leave(&mut self, cx: &mut Context<Self>) {
        let mut changed = self.hot.take().is_some();
        if std::mem::take(&mut self.over_link) { changed = true; }
        copy::left(self, cx);
        if changed { cx.notify(); }
    }

    /// A right-click: on a web link, its menu (Open in browser, Copy link).
    fn context(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(at) = self.hit(e.position, true) else { return };
        if let Some((_, Link::Url(u))) = link_span(&self.store.read(cx).rows, at) { copy::link_menu(u, e.position, cx); }
    }

    /// Ctrl+C: the selection to the clipboard (false: nothing selected).
    pub fn copy(&self, cx: &mut App) -> bool {
        let t = self.selected_text(cx);
        if t.is_empty() { return false; }
        cx.write_to_clipboard(ClipboardItem::new_string(t));
        true
    }

    // ─── Rows ───

    /// A piece of text that can be selected: its runs (with the selection's
    /// background), one element that reports its layout as it is laid out
    /// (HitText: no wrapper, no canvas; a row's tree stays small, F31).
    pub(crate) fn text(&self, row: usize, part: usize, r: &Row, inl: &Inline, look: Look) -> AnyElement {
        let text = r.parts.get(part).cloned().unwrap_or_default();
        let sel = self.range.and_then(|(s, e)| {
            let k = (row, part);
            if k < (s.0, s.1) || k > (e.0, e.1) { return None; }
            let a = if k == (s.0, s.1) { s.2 } else { 0 };
            let b = if k == (e.0, e.1) { e.2 } else { text.len() };
            (a < b).then_some(a..b.min(text.len()))
        });
        let hot = self.hot.as_ref().filter(|h| h.0 == row && h.1 == part).map(|h| h.2.clone());
        HitText { st: StyledText::new(text).with_runs(runs_hot(inl, look, sel, hot)), row, part, hits: self.hits.clone() }.into_any_element()
    }

    /// A block of Claude's text: one div (its text style, and the row's
    /// padding `item` adds) around its text; an item, a code block and a
    /// table only add the boxes they draw.
    pub(crate) fn block(&self, ix: usize, r: &Row, fs: f32) -> Div {
        let Some(b) = &r.blk else { return div() };
        let body = T::TEXT_13 * fs;
        let base = |c: Hsla| Look { color: c, mono: false, bold: false, link: Some(T::accent_hover()) };
        let p0 = |look: Look| b.parts.first().map(|i| self.text(ix, 0, r, i, look)).unwrap_or_else(|| div().into_any_element());
        let lh = relative(T::LH_CHAT);
        match &b.kind {
            Kind::Para => div().text_size(body).line_height(lh).child(p0(base(T::text()))),
            Kind::Heading(lv) => div().text_size(heading_size(*lv, fs)).line_height(lh).child(p0(Look { bold: true, ..base(T::text()) })),
            // The quote's bar is its padding box's left border.
            Kind::Quote => div().text_size(body).line_height(lh).child(div().pl(T::SP_10).border_l_2().border_color(T::border()).child(p0(base(T::text_secondary())))),
            Kind::Item { ordered, n, .. } => div().flex().text_size(body).line_height(lh)
                // The marker outside the item, in the list's 20 px (the web's ul/ol padding).
                .child(div().flex_none().w(T::MD_LIST_INDENT).flex().justify_end().pr(T::MD_MARKER_GAP * fs).text_color(T::text())
                    .child(if *ordered { SharedString::from(format!("{n}.")) } else { SharedString::from("\u{2022}") }))
                .child(div().flex_1().min_w_0().child(p0(base(T::text())))),
            Kind::Code => div().child(div().id(("code", ix)).w_full().flex().rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border_subtle())
                .px(T::MD_CODE_PX).py(T::MD_CODE_PY).overflow_x_scroll()
                .child(div().flex_none().text_size(T::TEXT_12 * fs).line_height(relative(T::LH_CODE)).whitespace_nowrap()
                    .child(p0(Look { mono: true, ..base(T::text()) })))),
            // The rule's two lines (dark over light): one 2 px box, its top border dark.
            Kind::Rule => div().child(div().w_full().h(px(2.)).border_t_1().border_color(T::md_rule_dark()).bg(T::md_rule())),
            Kind::Table { cols, rows } => {
                let mut t = div().w_full().flex().flex_col().rounded(T::R_CHIP).border_1().border_color(T::border()).overflow_hidden().text_size(body).line_height(lh);
                for y in 0..*rows {
                    let mut line = div().w_full().flex().when(y > 0, |d| d.border_t_1().border_color(T::border())).when(y == 0, |d| d.bg(T::bg_secondary()));
                    for x in 0..*cols {
                        let k = y * cols + x;
                        let cell = b.parts.get(k).map(|i| self.text(ix, k, r, i, Look { bold: y == 0, ..base(T::text()) })).unwrap_or_else(|| div().into_any_element());
                        line = line.child(div().flex_1().min_w_0().px(T::SP_8).py(T::SP_4).when(x > 0, |d| d.border_l_1().border_color(T::border())).child(cell));
                    }
                    t = t.child(line);
                }
                div().child(t)
            }
        }
    }

    fn item(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let n = self.store.read(cx).rows.len();
        let el = self.item_el(ix, window, cx);
        tail::mark_last(self, ix, n, el)
    }

    fn item_el(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.tail.max_ix = self.tail.max_ix.max(ix);
        // The streaming tail draws itself (tail.rs): its room, at its height.
        if let Some(t) = self.tail.drawn.filter(|t| t.ix == ix) { return div().w_full().h(t.h).into_any_element(); }
        let Some(r) = self.store.read(cx).rows.get(ix).cloned() else { return div().into_any_element() };
        let body = self.row_el(ix, &r, window, cx);
        // Side questions asked after this entry (2.21), under its last row.
        let asides = aside::under(self, ix, &r, window, cx);
        if asides.is_empty() { return body; }
        div().flex().flex_col().w_full().child(body).child(div().flex().flex_col().gap(T::SP_10).px(T::SP_16).pb(T::SP_10).children(asides)).into_any_element()
    }

    fn row_el(&mut self, ix: usize, r: &Row, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let fs = self.fs();
        let (above, below) = self.block_margins(ix, r, fs);
        // Claude's text: the block's own element takes the row's padding (one
        // element less per row); the others are wrapped in it.
        if r.shown.kind == "assistant" {
            let caret = prose::caret_of(self, r, cx);
            let below = match caret { Some(_) => px(caret_room(&r.block.as_ref().map_or(Kind::Para, |b| b.kind.clone()), fs) + 10.), None => below };
            if let Some(el) = self.block_el(ix, r, fs, (above, below), window.scale_factor(), caret) { return el; }
            return self.block(ix, r, fs).w_full().px(T::SP_16).pt(above).pb(below).children(caret.map(|c| prose::caret_line(c, fs))).into_any_element();
        }
        if r.shown.kind == "tool" { return tools::row(self, ix, r, fs, (above, below), window, cx); }
        let el = match r.shown.kind {
            "user" => prose::bubble(self, ix, r, fs),
            "error" | "notice" => prose::plain(self, ix, r, fs),
            "result" => prose::result(r, fs),
            "thinking" => thinking::row(self, r, fs, cx),
            "permission" => questions::permission(self, r, fs, cx),
            "question" => questions::row(self, r, fs, window, cx),
            "goal-verdict" => goal::verdict(r, fs),
            "factory-result" => cards::row(self, r, fs, window, cx),
            _ => div().w_full().text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).child(r.text.clone()).into_any_element(),
        };
        div().w_full().px(T::SP_16).pt(above).pb(below).child(el).into_any_element()
    }

    /// The selection's range in one piece of text, for this render.
    fn sel_in(&self, ix: usize, part: usize, len: usize) -> Option<std::ops::Range<usize>> {
        let (s, e) = self.range?;
        let k = (ix, part);
        if k < (s.0, s.1) || k > (e.0, e.1) { return None; }
        let a = if k == (s.0, s.1) { s.2 } else { 0 };
        let b = if k == (e.0, e.1) { e.2 } else { len };
        (a < b).then_some(a..b.min(len))
    }

    /// Claude's paragraphs, headings, quotes, list items, rules and tables:
    /// one element each (BlockEl, TableEl), laid out once per width and
    /// reused while the log only scrolls. A code block keeps its box (it
    /// scrolls sideways): None. Lengths snap to device pixels as a div's.
    fn block_el(&mut self, ix: usize, r: &Row, fs: f32, pad: (Pixels, Pixels), sf: f32, caret: Option<(Pixels, bool)>) -> Option<AnyElement> {
        let blk = r.blk.clone()?;
        let sn = |v: Pixels| snap(v, sf);
        let edge = sn(T::SP_16);
        let pad = (sn(pad.0), sn(pad.1));
        if let Kind::Table { cols, rows } = blk.kind {
            let sels = (0..cols * rows).map(|k| self.sel_in(ix, k, r.parts.get(k).map_or(0, |t| t.len()))).collect();
            let slot = self.tables.entry(Rc::as_ptr(&blk) as usize).or_default().clone();
            let hot = self.hot.as_ref().filter(|h| h.0 == ix).map(|h| (h.1, h.2.clone()));
            let (pad, below) = if caret.is_some() { ((pad.0, px(0.)), pad.1) } else { (pad, px(0.)) };
            let el = TableEl { slot, blk, parts: r.parts.clone(), sels, fs, pad, edge, sf, row: ix, hits: self.hits.clone(), hot }.into_any_element();
            return Some(match caret { Some(c) => div().w_full().child(el).child(div().px(T::SP_16).pb(below).child(prose::caret_line(c, fs))).into_any_element(), None => el });
        }
        self.block_raw(ix, r, fs, pad, sf, caret).map(|b| b.into_any_element())
    }

    /// A block of Claude's text other than a table or code as its element
    /// (`pad` snapped), unpainted: the streaming tail measures it (tail.rs).
    pub(crate) fn block_raw(&mut self, ix: usize, r: &Row, fs: f32, pad: (Pixels, Pixels), sf: f32, caret: Option<(Pixels, bool)>) -> Option<BlockEl> {
        let blk = r.blk.clone()?;
        let sn = |v: Pixels| snap(v, sf);
        let edge = sn(T::SP_16);
        let (font_size, bold, left, color, bar, rule, marker) = match &blk.kind {
            Kind::Para => (T::TEXT_13 * fs, false, edge, T::text(), false, false, None),
            Kind::Heading(lv) => (heading_size(*lv, fs), true, edge, T::text(), false, false, None),
            // The bar (2 px) and the quote's 10 px before its text.
            Kind::Quote => (T::TEXT_13 * fs, false, edge + sn(px(2.)) + sn(T::SP_10), T::text_secondary(), true, false, None),
            Kind::Item { ordered, n, .. } => (T::TEXT_13 * fs, false, edge + sn(T::MD_LIST_INDENT), T::text(), false, false,
                Some(if *ordered { SharedString::from(format!("{n}.")) } else { SharedString::from("\u{2022}") })),
            Kind::Rule => (T::TEXT_13 * fs, false, edge, T::text(), false, true, None),
            Kind::Code | Kind::Table { .. } => return None,
        };
        let text = if rule { SharedString::default() } else { r.parts.first().cloned().unwrap_or_default() };
        let sel = self.sel_in(ix, 0, text.len());
        let slot = self.laid.entry(Rc::as_ptr(&blk) as usize).or_default().clone();
        Some(BlockEl {
            slot, blk, text, look: Look { color, mono: false, bold, link: Some(T::accent_hover()) }, sel, fs, font_size, pad, left, edge, bar, rule,
            marker, marker_right: edge + sn(T::MD_LIST_INDENT) - sn(T::MD_MARKER_GAP * fs), row: ix, hits: self.hits.clone(),
            hot: self.hot.as_ref().filter(|h| h.0 == ix && h.1 == 0).map(|h| h.2.clone()), caret,
        })
    }

    /// A row's space above and below: the log's 12 px over the first row;
    /// a block of Claude's text, the margins the web's CSS gives it,
    /// collapsed with its neighbour's (and with the entry's 10 px).
    pub(crate) fn block_margins(&self, ix: usize, r: &Row, fs: f32) -> (Pixels, Pixels) {
        let top = if ix == 0 { T::SP_12 } else { px(0.) };
        let (above, below) = match &r.block {
            Some(b) => {
                let (t, bt) = margins(&b.kind, fs);
                let above = match &b.prev { None => (t - 10.).max(0.), Some(p) => margins(p, fs).1.max(t) };
                let below = if b.last { (if b.kind == Kind::Para { 0. } else { bt }).max(10.) } else { 0. };
                (px(above), px(below))
            }
            None => (px(0.), T::SP_10),
        };
        (top + above, below)
    }
}


fn xdist(h: &Hit, p: Point<Pixels>) -> f32 {
    let b = h.layout.bounds();
    f32::from(if p.x < b.left() { b.left() - p.x } else if p.x > b.right() { p.x - b.right() } else { px(0.) })
}

/// The word around byte `at` (letters, digits, '_'), else that one character.
fn word(t: &str, at: usize) -> (usize, usize) {
    let w = |c: char| c.is_alphanumeric() || c == '_';
    let at = at.min(t.len());
    let mut s = at;
    while s > 0 && !t.is_char_boundary(s) { s -= 1; }
    let next = t[s..].chars().next();
    if !next.is_some_and(w) {
        return match next { Some(c) => (s, s + c.len_utf8()), None => (s, s) };
    }
    let start = t[..s].char_indices().rev().take_while(|(_, c)| w(*c)).last().map_or(s, |(i, _)| i);
    let end = t[s..].char_indices().find(|(_, c)| !w(*c)).map_or(t.len(), |(i, _)| s + i);
    (start, end)
}

/// The text between two places of the log, as the web copies it: pieces of
/// a block by lines (a table's cells by tabs), list items by lines, other
/// blocks and messages by a blank line.
pub fn selected_text(rows: &[Row], s: At, e: At) -> String {
    let mut out = String::new();
    let mut prev: Option<&Row> = None;
    for (ri, r) in rows.iter().enumerate().take(e.0 + 1).skip(s.0) {
        if r.parts.is_empty() { continue; }
        if let Some(p) = prev {
            let items = matches!(p.block.as_ref().map(|b| &b.kind), Some(Kind::Item { .. })) && matches!(r.block.as_ref().map(|b| &b.kind), Some(Kind::Item { .. })) && p.id == r.id;
            out.push_str(if items { "\n" } else { "\n\n" });
        }
        let cols = match r.block.as_ref().map(|b| &b.kind) { Some(Kind::Table { cols, .. }) => *cols, _ => 0 };
        let first = if ri == s.0 { s.1 } else { 0 };
        let last = if ri == e.0 { e.1 } else { r.parts.len().saturating_sub(1) };
        for k in first..=last.min(r.parts.len().saturating_sub(1)) {
            let t = r.parts[k].as_ref();
            let a = if (ri, k) == (s.0, s.1) { s.2.min(t.len()) } else { 0 };
            let b = if (ri, k) == (e.0, e.1) { e.2.min(t.len()) } else { t.len() };
            if k > first { out.push(if cols > 0 && k % cols != 0 { '\t' } else { '\n' }); }
            if a < b { out.push_str(&t[a..b]); }
        }
        prev = Some(r);
    }
    out
}

fn inline_of(r: &Row, part: usize) -> Option<&Inline> {
    match &r.blk { Some(b) => b.parts.get(part), None => r.plain.as_deref().filter(|_| part == 0) }
}

/// The link under a place, if any.
fn link_at(rows: &[Row], at: At) -> Option<Link> { link_span(rows, at).map(|x| x.1) }

/// The link under a place and its byte range.
pub(crate) fn link_span(rows: &[Row], at: At) -> Option<(Range<usize>, Link)> {
    let inl = inline_of(rows.get(at.0)?, at.1)?;
    inl.spans.iter().find(|s| s.range.start <= at.2 && at.2 < s.range.end).and_then(|s| s.link.clone().map(|l| (s.range.clone(), l)))
}

/// The code span or link under a place (the copy button's targets, the
/// web's copyTargetOf: a code span or link in Claude's reply, a link in the
/// bubble): its range, what it copies (a link's address or path, a code
/// span's text) and what the button's tooltip calls it.
pub(crate) fn copy_span(rows: &[Row], at: At) -> Option<(Range<usize>, String, &'static str)> {
    let r = rows.get(at.0)?;
    let inl = inline_of(r, at.1)?;
    let sp = inl.spans.iter().find(|s| s.range.start <= at.2 && at.2 < s.range.end)?;
    let reply = r.shown.kind == "assistant";
    match &sp.link {
        Some(Link::Url(u)) => Some((sp.range.clone(), u.clone(), "link")),
        Some(Link::Path(p)) if reply || sp.flags & crundi_core::webmd::CODE == 0 => Some((sp.range.clone(), p.clone(), "path")),
        None if reply && sp.flags & crundi_core::webmd::CODE != 0 => Some((sp.range.clone(), inl.text.get(sp.range.clone())?.to_string(), "code")),
        _ => None,
    }
}

/// A link: in the browser; a path: to the file viewer (the Files area's
/// "open-path" action, with the path in `OpenPath`).
pub(crate) fn open_link(l: &Link, cx: &mut App) {
    match l {
        // (Through the shell's: a test gets the address in CRUNDI_OPEN_URL_FILE, never a browser.)
        Link::Url(u) => { if let Err(e) = crundi_shell::session::open_url(u) { log::warn!("open {u}: {e}"); } }
        Link::Path(p) => {
            cx.set_global(crundi_shell::OpenPath(p.clone()));
            let stores = crundi_shell::stores(cx);
            if let Some(h) = cx.active_window() { let _ = h.update(cx, |_, window, cx| { crundi_shell::run_action(&stores, "open-path", window, cx); }); }
        }
    }
}

impl Render for LogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("log");
        if self.spots { let g = format!("log:{}", self.chat(cx)); crundi_shell::spots::begin(cx, &g); }
        // The texts report themselves again as the list lays them out (the
        // streaming tail's own view keeps its row's: tail.rs).
        // (Asked once here: the list's rows are made while it holds its state.)
        self.tail.drawn = self.tail_live(cx);
        self.tail.max_ix = 0;
        match self.tail.drawn { Some(t) => self.hits.borrow_mut().retain(|h| h.row == t.ix), None => self.hits.borrow_mut().clear() }
        self.range = self.ordered(cx);
        // Layouts of blocks no longer in the store (only the cache holds them).
        if self.laid.len() > 2 * self.store.read(cx).rows.len() + 256 {
            self.laid.retain(|_, s| s.borrow().as_ref().is_none_or(|l| Rc::strong_count(&l.blk) > 1));
        }
        if self.tables.len() > self.store.read(cx).rows.len() + 64 {
            self.tables.retain(|_, s| s.borrow().as_ref().is_none_or(|l| Rc::strong_count(&l.blk) > 1));
        }
        let me = cx.entity().downgrade();
        // Where the last row ended up (tail.rs: the streaming tail may draw
        // itself only where the list puts it, at the log's foot).
        // The floating dock's room at the end (the newest line scrolls clear of it).
        let room = agents::room(self);
        self.tail.room = room;
        let geo = tail::measure(self, cx);
        // The pointer while a press is held, wherever it goes (over the
        // window, past the log's edges): every frame's, cheap when idle.
        let track = canvas(geo, move |_, _, window, _| {
            let m = me.clone();
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| if phase == DispatchPhase::Bubble && e.pressed_button == Some(MouseButton::Left) { let _ = m.update(cx, |this, cx| this.drag_to(e.position, cx)); });
            let m = me.clone();
            window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| if phase == DispatchPhase::Bubble && e.button == MouseButton::Left { let _ = m.update(cx, |this, cx| this.release(e, cx)); });
        }).absolute().size_full();
        let lst = list(self.list.clone(), cx.processor(|this, ix: usize, window, cx| this.item(ix, window, cx))).size_full().pb(room);
        // A text box in a card has the keyboard: its caret is a quad over its
        // text, so the log is not one paint layer meanwhile (ARCHITECTURE.md).
        let typing = self.focus.contains_focused(window, cx) && !self.focus.is_focused(window);
        self.tail.typing = typing;
        let lst: AnyElement = if typing { lst.into_any_element() } else { w::layer(lst).into_any_element() };
        // Over the log, each only while it exists, each in a paint layer of
        // its own (drawn over the list's): the copy button, "Go to the newest
        // message", the dock, the plan panel, the agents' sheet. A scroll frame
        // of a plain log builds the list and nothing else (F75's frame budget).
        let mut over: Vec<AnyElement> = vec![];
        if self.store.read(cx).rows.is_empty() { over.extend(aside::orphans(self, window, cx)); }
        over.extend(copy::button(self, cx).map(|e| w::layer(e).into_any_element()));
        let jump = prose::jump(self, cx);
        self.tail.jump_shown = jump.is_some();
        over.extend(jump.map(|e| w::layer(e).into_any_element()));
        over.extend(agents::dock(self, window, cx).map(|e| w::layer(e).into_any_element()));
        over.extend(goal::panel(self, window, cx));
        over.extend(agents::sheet(self, window, cx));
        let bar = goal::bar(self, cx);
        let overlay = media::overlay(self, window, cx);
        prose::blink_after(self, cx);
        // The pointer leaving the log matters only while a path is lit or the
        // copy button shows.
        let hovered = self.hot.is_some() || self.over_link || self.ui.copy.on.is_some();
        let parked = self.ui.parked.parked;
        // The log's own box: the root itself (as before the features came),
        // or, with the goal bar under it or a parked card over it, a box of
        // its own that clips what floats over it.
        let boxed = bar.is_some() || parked;
        self.tail.boxed = boxed;
        let logbox = div().id(if boxed { "log-box" } else { "log" }).relative()
            .when(self.over_link, |d| d.cursor_pointer())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| this.press(e, window, cx)))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, e: &MouseDownEvent, _, cx| this.context(e, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| this.hover(e, cx)))
            .when(hovered, |d| d.on_hover(cx.listener(|this, on: &bool, _, cx| if !*on { this.leave(cx) })))
            .child(lst)
            .child(track)
            .children(over);
        let key = cx.listener(|this, e: &KeyDownEvent, _, cx| {
            let m = &e.keystroke.modifiers;
            if (m.control || m.platform) && e.keystroke.key == "c" && this.copy(cx) { cx.stop_propagation(); }
        });
        if !boxed {
            return logbox.track_focus(&self.focus).size_full().min_h_0().bg(T::bg()).on_key_down(key).children(overlay).into_any_element();
        }
        let logbox = parked::wrap(self, logbox.flex_1().min_h_0().w_full().overflow_hidden().into_any_element(), window, cx);
        div().id("log").track_focus(&self.focus).relative().size_full().min_h_0().flex().flex_col().bg(T::bg())
            .on_key_down(key)
            .child(logbox)
            .children(bar)
            .children(overlay)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    //! F15: one selection across a reply's blocks and across messages.
    use super::selected_text;
    use crate::store::ChatStore;
    use gpui::{AppContext as _, TestAppContext};
    use serde_json::json;

    #[gpui::test]
    fn a_selection_copies_across_blocks_and_messages(cx: &mut TestAppContext) {
        let store = cx.new(|cx| ChatStore::new("c1", cx));
        let history = json!({ "type": "ui-history", "id": "c1", "session": { "id": "c1", "state": "idle", "messages": [
            { "id": "u1", "kind": "user", "text": "Show me" },
            { "id": "a1", "kind": "assistant-text", "text": "First **para**.\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nLast." },
        ] } });
        store.update(cx, |s, cx| s.apply_batch(&[history], cx));
        store.read_with(cx, |s, _| {
            let texts: Vec<&str> = s.rows.iter().map(|r| r.text.as_ref()).collect();
            assert_eq!(texts, vec!["Show me", "First para.", "one", "two", "a\tb\n1\t2", "Last."]);
            // From "me" in the bubble to "La" of the last block.
            let t = selected_text(&s.rows, (0, 0, 5), (5, 0, 2));
            assert_eq!(t, "me\n\nFirst para.\n\none\ntwo\n\na\tb\n1\t2\n\nLa");
            // Inside one table: from cell b to cell 1.
            assert_eq!(selected_text(&s.rows, (4, 1, 0), (4, 2, 1)), "b\n1");
        });
    }

    #[::core::prelude::v1::test]
    fn words() {
        assert_eq!(super::word("hello world_x!", 8), (6, 13));
        assert_eq!(super::word("a, b", 1), (1, 2));
    }
}
