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

use crate::store::{scale, ChatStore, Row};
use crundi_core::{tools, webmd::{self, Inline, Kind, Link}, syntax::Syn};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use std::{cell::RefCell, rc::Rc, time::Duration};

/// A place in the log's text: the entry, its block row, the piece, the byte.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pos { id: String, blk: usize, part: usize, ix: usize }

/// A place as (row, piece, byte): ordered as the log reads.
pub type At = (usize, usize, usize);

/// A text drawn this frame: where it is, to map the pointer onto it.
struct Hit { row: usize, part: usize, layout: TextLayout }

#[derive(Clone, Copy, PartialEq)]
enum Unit { Char, Word, Block }

/// How far from the log's top or bottom a drag scrolls it (the web's SEL_EDGE).
const EDGE: f32 = 56.;

pub struct LogView {
    store: Entity<ChatStore>,
    list: ListState,
    /// Tool cards and thinking blocks open (by entry id).
    open: std::collections::HashSet<String>,
    /// The pane's text size step (its meta's), as last drawn.
    zoom: Option<i32>,
    focus: FocusHandle,
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
    _subs: Vec<Subscription>,
}

impl LogView {
    pub(crate) fn new(store: Entity<ChatStore>, cx: &mut Context<Self>) -> Self {
        let list = store.read(cx).list.clone();
        let subs = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            // The text size changes every row.
            cx.observe(&store.read(cx).meta.clone(), |this: &mut Self, m, cx| { let z = m.read(cx).0.zoom; if this.zoom != Some(z) { this.zoom = Some(z); cx.notify(); } }),
        ];
        let zoom = Some(store.read(cx).meta.read(cx).0.zoom);
        LogView { store, list, open: Default::default(), zoom, focus: cx.focus_handle(), sel: None, drag: None, hits: Default::default(), mouse: Point::default(), edge: false, edge_task: None, over_link: false, range: None, _subs: subs }
    }

    fn toggle(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.open.remove(id) { self.open.insert(id.to_string()); }
        if let Some(i) = self.store.read(cx).rows.iter().position(|r| r.id == id) { self.list.splice(i..i + 1, 1); }
        cx.notify();
    }

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
        let ix = |l: &TextLayout| match l.index_for_position(p) { Ok(i) | Err(i) => i.min(l.len()) };
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
        let on = self.hit(e.position, true).is_some_and(|at| link_at(&self.store.read(cx).rows, at).is_some());
        if on != self.over_link { self.over_link = on; cx.notify(); }
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
    /// background), and a canvas after it that reports its layout.
    fn text(&self, row: usize, part: usize, r: &Row, inl: &Inline, look: Look) -> AnyElement {
        let text = r.parts.get(part).cloned().unwrap_or_default();
        let sel = self.range.and_then(|(s, e)| {
            let k = (row, part);
            if k < (s.0, s.1) || k > (e.0, e.1) { return None; }
            let a = if k == (s.0, s.1) { s.2 } else { 0 };
            let b = if k == (e.0, e.1) { e.2 } else { text.len() };
            (a < b).then_some(a..b.min(text.len()))
        });
        let st = StyledText::new(text).with_runs(runs(inl, look, sel));
        let layout = st.layout().clone();
        let hits = self.hits.clone();
        div().relative().min_w_0().child(st)
            .child(canvas(move |_, _, _| hits.borrow_mut().push(Hit { row, part, layout }), |_, _, _, _| {}).absolute().size_full())
            .into_any_element()
    }

    fn block(&self, ix: usize, r: &Row, fs: f32) -> AnyElement {
        let Some(b) = &r.blk else { return div().into_any_element() };
        let body = T::TEXT_13 * fs;
        let base = |c: Hsla| Look { color: c, mono: false, bold: false, link: Some(T::accent_hover()) };
        let p0 = |look: Look| b.parts.first().map(|i| self.text(ix, 0, r, i, look)).unwrap_or_else(|| div().into_any_element());
        match &b.kind {
            Kind::Para => div().w_full().text_size(body).line_height(relative(T::LH_CHAT)).child(p0(base(T::text()))).into_any_element(),
            Kind::Heading(lv) => div().w_full().text_size(heading_size(*lv, fs)).line_height(relative(T::LH_CHAT))
                .child(p0(Look { bold: true, ..base(T::text()) })).into_any_element(),
            Kind::Quote => div().w_full().pl(T::SP_10).border_l_2().border_color(T::border()).text_size(body).line_height(relative(T::LH_CHAT))
                .child(p0(base(T::text_secondary()))).into_any_element(),
            Kind::Item { ordered, n, .. } => div().w_full().flex().text_size(body).line_height(relative(T::LH_CHAT))
                // The marker outside the item, in the list's 20 px (the web's ul/ol padding).
                .child(div().flex_none().w(T::MD_LIST_INDENT).flex().justify_end().pr(T::MD_MARKER_GAP * fs).text_color(T::text())
                    .child(if *ordered { SharedString::from(format!("{n}.")) } else { SharedString::from("\u{2022}") }))
                .child(div().flex_1().min_w_0().child(p0(base(T::text()))))
                .into_any_element(),
            Kind::Code => div().id(("code", ix)).w_full().flex().rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border_subtle())
                .px(T::MD_CODE_PX).py(T::MD_CODE_PY).overflow_x_scroll()
                .child(div().flex_none().text_size(T::TEXT_12 * fs).line_height(relative(T::LH_CODE)).whitespace_nowrap()
                    .child(p0(Look { mono: true, ..base(T::text()) })))
                .into_any_element(),
            Kind::Rule => div().w_full().flex().flex_col()
                .child(div().w_full().h(px(1.)).bg(T::md_rule_dark()))
                .child(div().w_full().h(px(1.)).bg(T::md_rule()))
                .into_any_element(),
            Kind::Table { cols, rows } => {
                let mut t = div().w_full().flex().flex_col().rounded(T::R_CHIP).border_1().border_color(T::border()).overflow_hidden().text_size(body).line_height(relative(T::LH_CHAT));
                for y in 0..*rows {
                    let mut line = div().w_full().flex().when(y > 0, |d| d.border_t_1().border_color(T::border())).when(y == 0, |d| d.bg(T::bg_secondary()));
                    for x in 0..*cols {
                        let k = y * cols + x;
                        let cell = b.parts.get(k).map(|i| self.text(ix, k, r, i, Look { bold: y == 0, ..base(T::text()) })).unwrap_or_else(|| div().into_any_element());
                        line = line.child(div().flex_1().min_w_0().px(T::SP_8).py(T::SP_4).when(x > 0, |d| d.border_l_1().border_color(T::border())).child(cell));
                    }
                    t = t.child(line);
                }
                t.into_any_element()
            }
        }
    }

    fn item(&mut self, ix: usize, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let fs = scale(self.zoom.unwrap_or(0));
        let me = cx.entity().downgrade();
        let store = self.store.clone();
        let store = store.read(cx);
        let Some(r) = store.rows.get(ix) else { return div().into_any_element() };
        let open = self.open.contains(&r.id);
        let el = match r.shown.kind {
            "assistant" => self.block(ix, r, fs),
            "user" => {
                let inl = r.plain.clone().unwrap_or_default();
                div().flex().justify_end()
                    .child(div().max_w(relative(0.82)).px(T::SP_12).py(T::SP_8)
                        .rounded_tl(T::R_BUBBLE).rounded_tr(T::R_BUBBLE).rounded_bl(T::R_BUBBLE).rounded_br(T::R_CHIP)
                        .bg(T::accent_dim()).border_1().border_color(T::accent_edge())
                        .text_size(T::TEXT_13 * fs).line_height(relative(T::LH_CHAT))
                        // The bubble's links keep its colour (the web's `.cc-user-body a`).
                        .child(self.text(ix, 0, r, &inl, Look { color: T::text(), mono: false, bold: false, link: None })))
                    .into_any_element()
            }
            "error" | "notice" => {
                let inl = r.plain.clone().unwrap_or_default();
                let notice = r.shown.kind == "notice";
                let t = self.text(ix, 0, r, &inl, Look { color: if notice { T::text_muted() } else { T::red() }, mono: false, bold: false, link: None });
                if notice { div().w_full().flex().justify_center().text_size(T::TEXT_12 * fs).italic().child(t).into_any_element() } else { error_box(fs).child(t).into_any_element() }
            }
            _ => row(r, open, fs, &me),
        };
        let top = if ix == 0 { T::SP_12 } else { px(0.) };
        // A block of Claude's text: the margins the web's CSS gives it,
        // collapsed with its neighbour's (and with the entry's 10 px).
        let (above, below) = match &r.block {
            Some(b) => {
                let (t, bt) = margins(&b.kind, fs);
                let above = match &b.prev { None => (t - 10.).max(0.), Some(p) => margins(p, fs).1.max(t) };
                let below = if b.last { (if b.kind == Kind::Para { 0. } else { bt }).max(10.) } else { 0. };
                (px(above), px(below))
            }
            None => (px(0.), T::SP_10),
        };
        div().w_full().px(T::SP_16).pt(top + above).pb(below).child(el).into_any_element()
    }
}

/// How a piece of text is drawn.
#[derive(Clone, Copy)]
struct Look { color: Hsla, mono: bool, bold: bool, link: Option<Hsla> }

fn font(mono: bool, bold: bool, italic: bool) -> Font {
    let mut f = crundi_theme::ui_font();
    if mono { f.family = T::MONO.into(); }
    if bold { f.weight = T::W_BOLD; }
    if italic { f.style = FontStyle::Italic; }
    f
}

fn syn_color(s: Syn) -> Hsla {
    match s {
        Syn::Comment => T::syn_comment(), Syn::String => T::syn_string(), Syn::Number => T::syn_number(), Syn::Keyword => T::syn_keyword(),
        Syn::Type => T::syn_type(), Syn::Function => T::syn_function(), Syn::Tag => T::syn_keyword(), Syn::Attribute => T::syn_number(),
    }
}

/// The text's runs: its spans' styles, and the selection's background.
fn runs(inl: &Inline, look: Look, sel: Option<std::ops::Range<usize>>) -> Vec<TextRun> {
    let len = inl.text.len();
    let mut cuts = vec![0, len];
    for s in &inl.spans { cuts.push(s.range.start); cuts.push(s.range.end); }
    if let Some(r) = &sel { cuts.push(r.start); cuts.push(r.end); }
    cuts.retain(|&c| c <= len && inl.text.is_char_boundary(c));
    cuts.sort_unstable();
    cuts.dedup();
    let mut out = Vec::with_capacity(cuts.len());
    let mut k = 0;
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a == b { continue; }
        while k < inl.spans.len() && inl.spans[k].range.end <= a { k += 1; }
        let span = inl.spans.get(k).filter(|s| s.range.start <= a && s.range.end >= b);
        let flags = span.map_or(0, |s| s.flags);
        let mut color = look.color;
        let mut underline = None;
        let mut bg = None;
        if let Some(s) = span {
            if let Some(c) = s.syn { color = syn_color(c); }
            if flags & webmd::CODE != 0 && !look.mono { bg = Some(T::bg_tertiary()); }
            if let Some(l) = &s.link {
                if let (Link::Url(_), Some(c)) = (l, look.link) { color = c; }
                underline = Some(UnderlineStyle { thickness: px(1.), color: Some(color), wavy: false });
            }
        }
        if sel.as_ref().is_some_and(|r| r.start <= a && r.end >= b) { bg = Some(T::selection()); }
        out.push(TextRun { len: b - a, font: font(look.mono || flags & webmd::CODE != 0, look.bold || flags & webmd::BOLD != 0, flags & webmd::ITALIC != 0), color, background_color: bg, underline, strikethrough: None });
    }
    if out.is_empty() { out.push(TextRun { len, font: font(look.mono, look.bold, false), color: look.color, background_color: None, underline: None, strikethrough: None }); }
    out
}

/// The web's h3-h6 sizes (`#` 14 px; `##` the body; `###` 0.83, `####`+ 0.67 of it).
fn heading_size(lv: u8, fs: f32) -> Pixels {
    match lv { 3 => T::TEXT_14 * fs, 4 => T::TEXT_13 * fs, 5 => T::TEXT_13 * fs * 0.83, _ => T::TEXT_13 * fs * 0.67 }
}

/// A block's top and bottom margins (px), as the web's CSS gives them: its
/// reset (`* { margin: 0 }`, webapp-html.js) leaves h4-h6 and hr with none;
/// `.cc-assistant` gives p, lists, pre, blockquote and h1-h3 theirs.
fn margins(k: &Kind, _fs: f32) -> (f32, f32) {
    match k {
        Kind::Para | Kind::Quote | Kind::Code | Kind::Table { .. } => (0., 8.),
        Kind::Item { last, .. } => (2., if *last { 8. } else { 2. }),
        Kind::Heading(3) => (12., 6.),
        Kind::Heading(_) | Kind::Rule => (0., 0.),
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
fn link_at(rows: &[Row], at: At) -> Option<Link> {
    let inl = inline_of(rows.get(at.0)?, at.1)?;
    inl.spans.iter().find(|s| s.range.start <= at.2 && at.2 < s.range.end).and_then(|s| s.link.clone())
}

/// A link: in the browser; a path: to the file viewer (the Files area's
/// "open-path" action, with the path in `OpenPath`).
fn open_link(l: &Link, cx: &mut App) {
    match l {
        Link::Url(u) => cx.open_url(u),
        Link::Path(p) => {
            cx.set_global(crundi_shell::OpenPath(p.clone()));
            let stores = crundi_shell::stores(cx);
            if let Some(h) = cx.active_window() { let _ = h.update(cx, |_, window, cx| { crundi_shell::run_action(&stores, "open-path", window, cx); }); }
        }
    }
}

/// A red box (an error, a failed turn's end).
fn error_box(fs: f32) -> Div {
    div().w_full().px(T::SP_10).py(T::SP_6).rounded(T::R_CONTROL).border_1().border_color(T::red()).bg(T::red_dim())
        .text_color(T::red()).text_size(T::TEXT_12 * fs)
}

fn one_line(t: &str) -> String { t.split_whitespace().collect::<Vec<_>>().join(" ") }

/// The other rows as the PC client draws them (native-gpui's chat-log:
/// tools, thinking, the end of a turn).
fn row(r: &Row, open: bool, fs: f32, me: &WeakEntity<LogView>) -> AnyElement {
    let e = &r.shown;
    match e.kind {
        "thinking" => {
            let size = T::TEXT_12 * fs;
            if e.text.is_empty() {
                return div().flex().items_center().gap(T::SP_6)
                    .child(w::dot(T::text_muted()).size(T::SP_6))
                    .child(div().italic().text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_muted()).child(e.detail.clone()))
                    .into_any_element();
            }
            let live = e.status == "streaming";
            let id = e.id.clone();
            let label: AnyElement = if live { w::shimmer(SharedString::from(format!("thl-{}", e.id)), "Thinking").text_color(T::text_secondary()).into_any_element() }
                else { div().child("Thinking").into_any_element() };
            let head = div().id(SharedString::from(format!("thh-{}", e.id))).flex().items_center().gap(T::SP_6).cursor_pointer()
                .text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_secondary())
                .child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM * fs))
                .child(label)
                .on_click({ let me = me.clone(); move |_: &ClickEvent, _: &mut Window, cx: &mut App| { let _ = me.update(cx, |this, cx| this.toggle(&id, cx)); } });
            let body = div().id(SharedString::from(format!("thb-{}", e.id))).mt(T::SP_4)
                .max_h(T::COMPOSER_MAX + T::COMPOSER_MAX / 2.).overflow_y_scroll()
                .italic().text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_muted())
                .child(r.text.clone());
            div().flex()
                .child(div().flex_none().w(T::SP_2).bg(T::border()))
                .child(div().flex().flex_col().flex_1().min_w_0().pl(T::SP_10 - T::SP_2).child(head).when(open, |d| d.child(body)))
                .into_any_element()
        }
        "tool" => {
            let id = e.id.clone();
            let mark = if e.status == "running" { w::spinner(T::ICON_SM).into_any_element() }
                else { div().flex_none().w(T::ICON_SM).flex().justify_center().child(w::dot(if e.error { T::red() } else { T::green() }).size(T::SP_6)).into_any_element() };
            let head = div().id(SharedString::from(format!("th-{}", e.id)))
                .flex().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_6).cursor_pointer()
                .child(mark)
                .child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_12 * fs).font_weight(T::W_SEMIBOLD)
                    .text_color(if e.error { T::red() } else { T::accent_hover() }).child(e.title.clone()))
                .child(div().flex_1().min_w_0().text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).truncate().child(one_line(&e.detail)))
                .child(div().flex_none().text_color(T::text_muted()).child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM * fs)))
                .on_click({ let me = me.clone(); move |_: &ClickEvent, _: &mut Window, cx: &mut App| { let _ = me.update(cx, |this, cx| this.toggle(&id, cx)); } });
            let mut card = w::card().rounded(T::R_CONTROL).bg(T::bg_secondary()).border_color(T::border_subtle()).overflow_hidden().child(head);
            if open {
                let text = |t: &str, c: Hsla| div().font_family(T::MONO).text_size(T::TEXT_12 * fs).text_color(c).whitespace_normal().child(t.to_string());
                let mut body = div().id(SharedString::from(format!("tb-{}", e.id))).flex().flex_col().px(T::SP_10).py(T::SP_8).max_h(T::COMPOSER_MAX * 2.).overflow_y_scroll();
                body = match &e.body {
                    tools::Body::Bash { command, description } => body.child(text(command, T::text_secondary())).when(!description.is_empty(), |d| d.child(text(description, T::text_muted()).pt(T::SP_6))),
                    tools::Body::Json => body.child(text(&e.args, T::text_secondary())),
                    // (The diff, write and todo bodies are the chat-log area's: PLAN.md.)
                    _ => body.child(text(&e.args, T::text_secondary())),
                };
                if !e.text.is_empty() {
                    body = body.child(div().mt(T::SP_8).mb(T::SP_8).h(px(1.)).w_full().bg(T::border_subtle())).child(text(&e.text, if e.error { T::red() } else { T::text_muted() }));
                }
                card = card.child(div().h(px(1.)).w_full().bg(T::border_subtle())).child(body);
            }
            card.into_any_element()
        }
        "result" if e.error => error_box(fs).child(if e.text.is_empty() { SharedString::from("Error") } else { r.text.clone() }).into_any_element(),
        "result" => div().flex().flex_col().w_full()
            .child(div().w_full().h(px(1.)).bg(T::border_subtle()))
            .child(div().w_full().py(T::SP_4).text_center().text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(e.detail.clone()))
            .into_any_element(),
        // Permission and question cards are the chat-log area's (PLAN.md 2.13).
        _ => div().w_full().text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).child(if e.title.is_empty() { r.text.clone() } else { e.title.clone().into() }).into_any_element(),
    }
}

impl Render for LogView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("log");
        // The texts report themselves again as the list lays them out.
        self.hits.borrow_mut().clear();
        self.range = self.ordered(cx);
        let me = cx.entity().downgrade();
        // The pointer while a press is held, wherever it goes (over the
        // window, past the log's edges): every frame's, cheap when idle.
        let track = canvas(|_, _, _| {}, move |_, _, window, _| {
            let m = me.clone();
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| if phase == DispatchPhase::Bubble && e.pressed_button == Some(MouseButton::Left) { let _ = m.update(cx, |this, cx| this.drag_to(e.position, cx)); });
            let m = me.clone();
            window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| if phase == DispatchPhase::Bubble && e.button == MouseButton::Left { let _ = m.update(cx, |this, cx| this.release(e, cx)); });
        }).absolute().size_full();
        div().id("log").track_focus(&self.focus).relative().size_full().min_h_0().bg(T::bg())
            .when(self.over_link, |d| d.cursor_pointer())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| this.press(e, window, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| this.hover(e, cx)))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let m = &e.keystroke.modifiers;
                if (m.control || m.platform) && e.keystroke.key == "c" && this.copy(cx) { cx.stop_propagation(); }
            }))
            .child(list(self.list.clone(), cx.processor(|this, ix: usize, window, cx| this.item(ix, window, cx))).size_full())
            .child(track)
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

    #[test]
    fn words() {
        assert_eq!(super::word("hello world_x!", 8), (6, 13));
        assert_eq!(super::word("a, b", 1), (1, 2));
    }
}
