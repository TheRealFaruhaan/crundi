//! The log's text as drawn (ARCHITECTURE.md, "What a scroll frame costs"
//! and "Text and selection"): Claude's blocks as one element each (BlockEl,
//! TableEl), shaped once per width and only painted while the log scrolls;
//! a selectable StyledText that reports its layout (HitText); the runs of a
//! piece of text (spans, links, the selection) and the web's margins.

use crundi_core::{webmd::{self, Inline, Kind, Link}, syntax::Syn};
use crundi_theme::T;
use gpui::*;
use std::{cell::RefCell, ops::Range, rc::Rc};

/// The link under the pointer in a piece of text (its byte range): a path
/// there takes the hover colour (the web's `.cc-path:hover`).
pub(crate) type Hot = Option<Range<usize>>;

/// The streaming caret (the web's `.cc-cursor`: 6 x 13, the accent, on a line
/// of its own under the reply's last block, blinking each half second).
pub(crate) const CARET_W: f32 = 6.;
pub(crate) const CARET_H: f32 = 13.;
/// The space the caret takes under its block: the block's own margin (it is
/// no longer the last child), then one line of the body text.
pub(crate) fn caret_room(kind: &Kind, fs: f32) -> f32 { margins(kind, fs).1 + 13. * fs * T::LH_CHAT }

/// A text drawn this frame: where it is, to map the pointer onto it.
pub(crate) struct Hit { pub(crate) row: usize, pub(crate) part: usize, pub(crate) layout: HitLayout }

/// Where a drawn text's characters are: a StyledText's layout, or a block
/// element's cached lines (BlockEl) at the bounds it was drawn at.
pub(crate) enum HitLayout { Text(TextLayout), Lines { lines: Lines, bounds: Bounds<Pixels>, text: SharedString } }

impl HitLayout {
    pub(crate) fn bounds(&self) -> Bounds<Pixels> { match self { HitLayout::Text(l) => l.bounds(), HitLayout::Lines { bounds, .. } => *bounds } }
    pub(crate) fn len(&self) -> usize { match self { HitLayout::Text(l) => l.len(), HitLayout::Lines { text, .. } => text.len() } }
    pub(crate) fn text(&self) -> String { match self { HitLayout::Text(l) => l.text(), HitLayout::Lines { text, .. } => text.to_string() } }
    /// Where byte `ix` is drawn (its line's top) and the line's height.
    pub(crate) fn point_for(&self, ix: usize) -> Option<(Point<Pixels>, Pixels)> {
        match self {
            HitLayout::Text(l) => l.position_for_index(ix).map(|p| (p, l.line_height())),
            HitLayout::Lines { lines, bounds, .. } => {
                let mut origin = bounds.origin;
                let mut start = 0;
                for line in lines.lines.iter() {
                    let end = start + line.len();
                    if ix <= end { return line.position_for_index(ix - start, lines.line_height).map(|p| (origin + p, lines.line_height)); }
                    origin.y += line.size(lines.line_height).height;
                    start = end + 1;
                }
                None
            }
        }
    }
    /// The byte under a point (TextLayout::index_for_position, over the cached lines).
    pub(crate) fn index_for_position(&self, p: Point<Pixels>) -> Result<usize, usize> {
        let (laid, bounds) = match self { HitLayout::Text(l) => return l.index_for_position(p), HitLayout::Lines { lines, bounds, .. } => (lines, bounds) };
        if p.y < bounds.top() { return Err(0); }
        let mut origin = bounds.origin;
        let mut start = 0;
        for line in laid.lines.iter() {
            let bottom = origin.y + line.size(laid.line_height).height;
            if p.y > bottom { origin.y = bottom; start += line.len() + 1; continue; }
            return match line.index_for_position(p - origin, laid.line_height) { Ok(i) => Ok(start + i), Err(i) => Err(start + i) };
        }
        Err(start.saturating_sub(1))
    }
}

/// A block of Claude's text laid out (BlockEl): for one block, width, text
/// size and selection; kept while only the scroll moves (F31).
pub(crate) struct Laid { pub(crate) blk: Rc<webmd::Block>, pub(crate) fs: f32, pub(crate) sel: Option<std::ops::Range<usize>>, pub(crate) hot: Hot, pub(crate) width: Pixels, pub(crate) text: Lines, pub(crate) marker: Option<ShapedLine> }
pub(crate) type Slot = Rc<RefCell<Option<Laid>>>;

/// A text shaped for one width: its wrapped lines (shared with the hits),
/// each hard line's decoration runs, its line height and its height.
#[derive(Clone)]
pub(crate) struct Lines { pub(crate) lines: Rc<Vec<WrappedLine>>, pub(crate) deco: Rc<Vec<Vec<Deco>>>, pub(crate) line_height: Pixels, pub(crate) h: Pixels }

/// A line's decoration run (GPUI's DecorationRun, built the same way).
#[derive(Clone, PartialEq)]
pub(crate) struct Deco { pub(crate) len: u32, pub(crate) color: Hsla, pub(crate) bg: Option<Hsla>, pub(crate) underline: Option<UnderlineStyle> }

impl Lines {
    pub(crate) fn shape(text: &SharedString, inl: &Inline, look: Look, sel: Option<std::ops::Range<usize>>, hot: Hot, font_size: Pixels, width: Pixels, window: &mut Window) -> Lines {
        let line_height = window.pixel_snap(font_size * T::LH_CHAT);
        let runs = runs_hot(inl, look, sel, hot);
        let lines: Vec<WrappedLine> = window.text_system().shape_text(text.clone(), font_size, &runs, Some(width), None).map(|l| l.into_iter().collect()).unwrap_or_default();
        let deco = Rc::new(decorations(text, &runs));
        // A measured text's height, rounded up to device pixels as GPUI's layout does.
        let h = lines.iter().map(|l| l.size(line_height).height).fold(px(0.), |a, b| a + b);
        let sf = window.scale_factor();
        Lines { lines: Rc::new(lines), deco, line_height, h: px((f32::from(h) * sf).ceil() / sf) }
    }
    pub(crate) fn none() -> Lines { Lines { lines: Rc::new(vec![]), deco: Rc::new(vec![]), line_height: px(0.), h: px(0.) } }
    /// Backgrounds then glyphs, line by line (StyledText's paint).
    fn paint(&self, tb: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let mut o = tb.origin;
        for (i, line) in self.lines.iter().enumerate() {
            let deco = self.deco.get(i).map_or(&[][..], |d| &d[..]);
            if deco.iter().any(|d| d.bg.is_some()) {
                // A background (a code span, the selection): GPUI's own two
                // passes, each its own layer, glyphs over the backgrounds.
                let _ = line.paint_background(o, self.line_height, TextAlign::Left, Some(tb), window, cx);
                let _ = line.paint(o, self.line_height, TextAlign::Left, Some(tb), window, cx);
            } else {
                paint_glyphs(line, deco, o, self.line_height, window, cx);
            }
            o.y += line.size(self.line_height).height;
        }
    }
}

/// Each hard line's decoration runs, as GPUI's `shape_text` makes them (a
/// run split at the line's end, neighbours with the same look merged, the
/// '\n' taken from the run it falls in).
pub(crate) fn decorations(text: &str, runs: &[TextRun]) -> Vec<Vec<Deco>> {
    let mut runs: std::collections::VecDeque<(usize, &TextRun)> = runs.iter().filter(|r| r.len > 0).map(|r| (r.len, r)).collect();
    let mut out = vec![];
    let mut start = 0;
    for line in text.split('\n') {
        let end = start + line.len();
        let mut ds: Vec<Deco> = vec![];
        let mut at = start;
        while at < end {
            let Some((left, r)) = runs.front_mut() else { break };
            let n = (end - at).min(*left);
            let d = Deco { len: n as u32, color: r.color, bg: r.background_color, underline: r.underline };
            match ds.last_mut() { Some(l) if l.color == d.color && l.bg == d.bg && l.underline == d.underline => l.len += n as u32, _ => ds.push(d) }
            *left -= n;
            if *left == 0 { runs.pop_front(); }
            at += n;
        }
        out.push(ds);
        if let Some((left, _)) = runs.front_mut() { *left -= 1; if *left == 0 { runs.pop_front(); } }
        start = end + 1;
    }
    out
}

/// A wrapped line's glyphs and underlines, as GPUI's `paint_line` paints
/// them (left aligned; the log has no strikethrough), but straight into the
/// log's layer: GPUI opens a layer per line (one more bounds-tree insert per
/// line, every frame this pane is drawn or replayed; F31).
pub(crate) fn paint_glyphs(line: &WrappedLine, deco: &[Deco], origin: Point<Pixels>, lh: Pixels, window: &mut Window, cx: &mut App) {
    let layout = &line.unwrapped_layout;
    let wraps = &line.wrap_boundaries;
    let padding_top = (lh - layout.ascent - layout.descent) / 2.;
    let baseline = point(px(0.), padding_top + layout.ascent);
    let uoff = underline_y_offset(lh, layout.ascent, layout.descent);
    let mut decos = deco.iter();
    let mut next_wrap = wraps.iter().peekable();
    let mut run_end = 0usize;
    let mut color = black();
    let mut ul: Option<(Point<Pixels>, UnderlineStyle)> = None;
    let ts = cx.text_system().clone();
    let mut g = origin;
    let mut prev = Point::default();
    let mut max = size(px(0.), px(0.));
    let mut first_x = origin.x;
    for (run_ix, run) in layout.runs.iter().enumerate() {
        max = ts.bounding_box(run.font_id, layout.font_size).size;
        for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
            g.x += glyph.position.x - prev.x;
            if glyph_ix == 0 && run_ix == 0 { first_x = g.x; }
            if next_wrap.peek().is_some_and(|w| w.run_ix == run_ix && w.glyph_ix == glyph_ix) {
                next_wrap.next();
                if let Some((uo, style)) = ul.as_mut() {
                    if g.x == uo.x { uo.x -= max.width / 2.; }
                    window.paint_underline(*uo, g.x - uo.x, style);
                    if glyph.index < run_end { uo.x = origin.x; uo.y += lh; } else { ul = None; }
                }
                g.x = origin.x;
                g.y += lh;
            }
            prev = glyph.position;
            let mut finished = None;
            if glyph.index >= run_end {
                let mut r = decos.next();
                while let Some(d) = r { if glyph.index < run_end + d.len as usize { break; } run_end += d.len as usize; r = decos.next(); }
                if let Some(d) = r {
                    if let Some((_, st)) = &ul { if d.underline.as_ref() != Some(st) { finished = ul.take(); } }
                    if let Some(u) = d.underline.as_ref() {
                        ul.get_or_insert((point(g.x, g.y + uoff), UnderlineStyle { color: Some(u.color.unwrap_or(d.color)), thickness: u.thickness, wavy: u.wavy }));
                    }
                    run_end += d.len as usize;
                    color = d.color;
                } else {
                    run_end = layout.len;
                    finished = ul.take();
                }
            }
            if let Some((mut uo, st)) = finished {
                if uo.x == g.x { uo.x -= max.width / 2.; }
                window.paint_underline(uo, g.x - uo.x, &st);
            }
            if (Bounds { origin: g, size: max }).intersects(&window.content_mask().bounds) {
                let at = g + baseline + point(px(0.), glyph.position.y);
                let _ = if glyph.is_emoji { window.paint_emoji(at, run.font_id, glyph.id, layout.font_size) } else { window.paint_glyph(at, run.font_id, glyph.id, layout.font_size, color) };
            }
        }
    }
    let mut last_end = first_x + layout.width;
    if let Some(b) = wraps.last() { last_end -= layout.runs[b.run_ix].glyphs[b.glyph_ix].position.x; }
    if let Some((mut us, st)) = ul.take() {
        if last_end == us.x { us.x -= max.width / 2.; }
        window.paint_underline(us, last_end - us.x, &st);
    }
}

/// A length snapped to device pixels as GPUI snaps authored lengths
/// (round, a midpoint toward zero): a custom element lands where a div would.
pub(crate) fn snap(v: Pixels, sf: f32) -> Pixels {
    let d = f32::from(v) * sf;
    let r = if (d - d.trunc()).abs() == 0.5 { d.trunc() } else { d.round() };
    px(r / sf)
}


/// A selectable text: a StyledText that, once laid out, reports where it is
/// (the pointer is mapped onto it; Zed's markdown does the same).
pub(crate) struct HitText { pub(crate) st: StyledText, pub(crate) row: usize, pub(crate) part: usize, pub(crate) hits: Rc<RefCell<Vec<Hit>>> }

impl IntoElement for HitText {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl Element for HitText {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, id: Option<&GlobalElementId>, iid: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        self.st.request_layout(id, iid, window, cx)
    }
    fn prepaint(&mut self, id: Option<&GlobalElementId>, iid: Option<&InspectorElementId>, bounds: Bounds<Pixels>, rl: &mut (), window: &mut Window, cx: &mut App) {
        self.st.prepaint(id, iid, bounds, rl, window, cx);
        self.hits.borrow_mut().push(Hit { row: self.row, part: self.part, layout: HitLayout::Text(self.st.layout().clone()) });
    }
    fn paint(&mut self, id: Option<&GlobalElementId>, iid: Option<&InspectorElementId>, bounds: Bounds<Pixels>, rl: &mut (), pp: &mut (), window: &mut Window, cx: &mut App) {
        self.st.paint(id, iid, bounds, rl, pp, window, cx)
    }
}


/// One block of Claude's text as one element (F31, ARCHITECTURE.md "What a
/// scroll frame costs"): a measured leaf (no taffy subtree), its lines
/// shaped once per block, width, text size and selection (the LogView's
/// `laid`), then only painted while the log scrolls: the quote's bar, the
/// item's marker, the rule's two lines, the text's runs (backgrounds, then
/// glyphs, as StyledText paints them).
pub(crate) struct BlockEl {
    pub(crate) slot: Slot, pub(crate) blk: Rc<webmd::Block>, pub(crate) text: SharedString, pub(crate) look: Look, pub(crate) sel: Option<std::ops::Range<usize>>, pub(crate) fs: f32,
    pub(crate) font_size: Pixels, pub(crate) pad: (Pixels, Pixels), pub(crate) left: Pixels, pub(crate) edge: Pixels, pub(crate) bar: bool, pub(crate) rule: bool,
    pub(crate) marker: Option<SharedString>, pub(crate) marker_right: Pixels, pub(crate) row: usize, pub(crate) hits: Rc<RefCell<Vec<Hit>>>,
    pub(crate) hot: Hot,
    /// The streaming caret under the text: its gap below the text, and
    /// whether it is lit this half second.
    pub(crate) caret: Option<(Pixels, bool)>,
}

impl IntoElement for BlockEl {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl BlockEl {
    /// The lines for a width: the cached ones, or shaped now.
    pub(crate) fn lay(&self, width: Pixels, window: &mut Window) -> Pixels {
        let tw = (width - self.left - self.edge).max(px(0.));
        let mut slot = self.slot.borrow_mut();
        let fresh = slot.as_ref().is_some_and(|l| Rc::ptr_eq(&l.blk, &self.blk) && l.fs == self.fs && l.sel == self.sel && l.hot == self.hot && l.width == tw);
        if !fresh {
            let text = if self.rule { Lines { h: px(2.), ..Lines::none() } } else if self.blk.parts.is_empty() { Lines::none() } else {
                Lines::shape(&self.text, &self.blk.parts[0], self.look, self.sel.clone(), self.hot.clone(), self.font_size, tw, window)
            };
            let marker = self.marker.as_ref().map(|m| {
                let run = TextRun { len: m.len(), font: font(false, false, false), color: T::text(), background_color: None, underline: None, strikethrough: None };
                window.text_system().shape_line(m.clone(), self.font_size, &[run], None)
            });
            *slot = Some(Laid { blk: self.blk.clone(), fs: self.fs, sel: self.sel.clone(), hot: self.hot.clone(), width: tw, text, marker });
        }
        slot.as_ref().map_or(px(0.), |l| l.text.h)
    }
}

impl Element for BlockEl {
    type RequestLayoutState = ();
    type PrepaintState = Bounds<Pixels>;
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, _: &mut App) -> (LayoutId, ()) {
        let me = BlockEl { slot: self.slot.clone(), blk: self.blk.clone(), text: self.text.clone(), look: self.look, sel: self.sel.clone(), fs: self.fs,
            font_size: self.font_size, pad: self.pad, left: self.left, edge: self.edge, bar: self.bar, rule: self.rule,
            marker: self.marker.clone(), marker_right: self.marker_right, row: self.row, hits: self.hits.clone(), hot: self.hot.clone(), caret: self.caret };
        let id = window.request_measured_layout(Style::default(), move |known, avail, window, _| {
            let w = known.width.or(match avail.width { AvailableSpace::Definite(w) => Some(w), _ => None }).unwrap_or(px(0.));
            let h = me.lay(w, window);
            size(w, me.pad.0 + h + me.pad.1)
        });
        (id, ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, _: &mut App) -> Bounds<Pixels> {
        let h = self.lay(bounds.size.width, window);
        let tb = Bounds { origin: bounds.origin + point(self.left, self.pad.0), size: size((bounds.size.width - self.left - self.edge).max(px(0.)), h) };
        if !self.rule && !self.blk.parts.is_empty() {
            if let Some(l) = self.slot.borrow().as_ref() {
                self.hits.borrow_mut().push(Hit { row: self.row, part: 0, layout: HitLayout::Lines { lines: l.text.clone(), bounds: tb, text: self.text.clone() } });
            }
        }
        tb
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), tb: &mut Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        // (In the log's one layer: LogView::render.)
        self.paint_in(bounds, *tb, window, cx);
    }
}

impl BlockEl {
    pub(crate) fn paint_in(&self, bounds: Bounds<Pixels>, tb: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let tb = &tb;
        let laid = self.slot.borrow();
        let Some(laid) = laid.as_ref() else { return };
        if self.rule {
            let w = (bounds.size.width - self.edge * 2.).max(px(0.));
            let o = bounds.origin + point(self.edge, self.pad.0);
            window.paint_quad(fill(Bounds { origin: o, size: size(w, px(1.)) }, T::md_rule_dark()));
            window.paint_quad(fill(Bounds { origin: o + point(px(0.), px(1.)), size: size(w, px(1.)) }, T::md_rule()));
            return;
        }
        if self.bar {
            window.paint_quad(fill(Bounds { origin: bounds.origin + point(self.edge, self.pad.0), size: size(px(2.), tb.size.height) }, T::border()));
        }
        if let Some(m) = &laid.marker {
            // Right-aligned as the flex box placed it: its width rounded up (TextLayout's).
            let o = point(bounds.origin.x + self.marker_right - m.width.ceil(), tb.origin.y);
            let _ = m.paint(o, laid.text.line_height, TextAlign::Left, None, window, cx);
        }
        laid.text.paint(*tb, window, cx);
        if let Some((gap, true)) = self.caret {
            // On its own line under the text (vertical-align: -2px on the baseline).
            let lh = 13. * self.fs * T::LH_CHAT;
            let top = tb.origin.y + tb.size.height + gap + px(((lh - CARET_H) / 2. + 1.).max(0.));
            window.paint_quad(fill(Bounds { origin: point(bounds.origin.x + self.edge, top), size: size(px(CARET_W), px(CARET_H)) }, T::accent()));
        }
    }
}

/// A border quad's background as a div paints it: the border's colour at
/// no alpha (the shader blends the border's inner edge towards it).
pub(crate) fn clear(c: Hsla) -> Hsla { Hsla { a: 0., ..c } }

/// A table laid out (TableEl): each cell's text for one width and
/// selection, and each row's height (its cells' tallest, with the padding).
pub(crate) struct LaidTable { pub(crate) blk: Rc<webmd::Block>, pub(crate) fs: f32, pub(crate) sels: Vec<Option<std::ops::Range<usize>>>, pub(crate) hot: Option<(usize, Range<usize>)>, pub(crate) width: Pixels, pub(crate) cells: Vec<Lines>, pub(crate) rows: Vec<Pixels> }
pub(crate) type TSlot = Rc<RefCell<Option<LaidTable>>>;

/// A GFM table as one element (the owner's call: drawn as a table), as the
/// div tree drew it: a 1 px rounded border around the rows, the header row
/// on the secondary background and bold, rows split by a 1 px line, equal
/// columns split by a 1 px line, each cell padded 8 x 4. Laid out once per
/// width; edges rounded to device pixels as GPUI rounds a div's.
pub(crate) struct TableEl {
    pub(crate) slot: TSlot, pub(crate) blk: Rc<webmd::Block>, pub(crate) parts: Vec<SharedString>, pub(crate) sels: Vec<Option<std::ops::Range<usize>>>, pub(crate) fs: f32,
    pub(crate) pad: (Pixels, Pixels), pub(crate) edge: Pixels, pub(crate) sf: f32, pub(crate) row: usize, pub(crate) hits: Rc<RefCell<Vec<Hit>>>,
    /// The hovered link: its cell and range.
    pub(crate) hot: Option<(usize, Range<usize>)>,
}

impl IntoElement for TableEl {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

/// A table's geometry in a row's bounds: the table, each row, each cell and
/// where its text starts.
pub(crate) struct TableGeom { pub(crate) table: Bounds<Pixels>, pub(crate) rows: Vec<Bounds<Pixels>>, pub(crate) cells: Vec<(Bounds<Pixels>, Point<Pixels>, Pixels)> }

impl TableEl {
    pub(crate) fn dims(&self) -> (usize, usize) { match self.blk.kind { Kind::Table { cols, rows } => (cols.max(1), rows), _ => (1, 0) } }
    /// A cell's padding and border (x > 0 has its 1 px left line): its base
    /// size in the row's flex layout (each cell `flex: 1 1 0`).
    pub(crate) fn cell_base(&self, x: usize) -> Pixels { snap(T::SP_8, self.sf) * 2. + if x > 0 { snap(px(1.), self.sf) } else { px(0.) } }
    /// The text's width in every column for a table of width `tw`: the
    /// row's space past the cells' bases, shared equally (unrounded, as the
    /// flex layout gives it to the cell's text).
    pub(crate) fn cell_w(&self, tw: Pixels, _x: usize) -> Pixels {
        let (cols, _) = self.dims();
        let base = (0..cols).fold(px(0.), |a, x| a + self.cell_base(x));
        ((tw - snap(px(1.), self.sf) * 2. - base) / cols as f32).max(px(0.))
    }
    pub(crate) fn lay(&self, width: Pixels, window: &mut Window) -> Pixels {
        let tw = (width - self.edge * 2.).max(px(0.));
        let (cols, rows) = self.dims();
        let mut slot = self.slot.borrow_mut();
        let fresh = slot.as_ref().is_some_and(|l| Rc::ptr_eq(&l.blk, &self.blk) && l.fs == self.fs && l.sels == self.sels && l.hot == self.hot && l.width == tw);
        if !fresh {
            let body = T::TEXT_13 * self.fs;
            let mut cells = Vec::with_capacity(cols * rows);
            let mut hs = Vec::with_capacity(rows);
            for y in 0..rows {
                let mut h = px(0.);
                for x in 0..cols {
                    let k = y * cols + x;
                    let look = Look { color: T::text(), mono: false, bold: y == 0, link: Some(T::accent_hover()) };
                    let l = match (self.blk.parts.get(k), self.parts.get(k)) {
                        (Some(inl), Some(t)) => Lines::shape(t, inl, look, self.sels.get(k).cloned().flatten(), self.hot.as_ref().filter(|h| h.0 == k).map(|h| h.1.clone()), body, self.cell_w(tw, x), window),
                        _ => Lines::none(),
                    };
                    h = h.max(l.h);
                    cells.push(l);
                }
                hs.push(h + snap(T::SP_4, self.sf) * 2. + if y > 0 { snap(px(1.), self.sf) } else { px(0.) });
            }
            *slot = Some(LaidTable { blk: self.blk.clone(), fs: self.fs, sels: self.sels.clone(), hot: self.hot.clone(), width: tw, cells, rows: hs });
        }
        slot.as_ref().map_or(px(0.), |l| l.rows.iter().fold(px(0.), |a, b| a + *b) + snap(px(1.), self.sf) * 2.)
    }
    pub(crate) fn geom(&self, b: Bounds<Pixels>, l: &LaidTable) -> TableGeom {
        let (cols, _) = self.dims();
        let sf = self.sf;
        let one = snap(px(1.), sf);
        let r = |v: Pixels| snap(v, sf);
        let h = l.rows.iter().fold(px(0.), |a, b| a + *b) + one * 2.;
        let table = Bounds { origin: point(r(b.origin.x + self.edge), r(b.origin.y + self.pad.0)), size: size(r(b.origin.x + b.size.width - self.edge) - r(b.origin.x + self.edge), h) };
        let inner = Bounds { origin: table.origin + point(one, one), size: size(table.size.width - one * 2., table.size.height - one * 2.) };
        // Each cell's edges: its base plus an equal share, from the left,
        // rounded where they fall in the window (as GPUI rounds a box).
        let share = self.cell_w(b.size.width - self.edge * 2., 0);
        let mut raw = inner.origin.x;
        let mut xs = vec![inner.origin.x];
        for x in 0..cols { raw = raw + self.cell_base(x) + share; xs.push(if x + 1 == cols { inner.origin.x + inner.size.width } else { r(raw) }); }
        let (mut rows, mut cells) = (vec![], vec![]);
        let mut y = inner.origin.y;
        for (ry, rh) in l.rows.iter().enumerate() {
            let row = Bounds { origin: point(inner.origin.x, y), size: size(inner.size.width, *rh) };
            let top = if ry > 0 { one } else { px(0.) };
            for x in 0..cols {
                let cell = Bounds { origin: point(xs[x], y + top), size: size(xs[x + 1] - xs[x], *rh - top) };
                let at = cell.origin + point(if x > 0 { one } else { px(0.) } + r(T::SP_8), r(T::SP_4));
                cells.push((cell, at, self.cell_w(table.size.width, x)));
            }
            rows.push(row);
            y += *rh;
        }
        TableGeom { table, rows, cells }
    }
}

impl Element for TableEl {
    type RequestLayoutState = ();
    type PrepaintState = Option<TableGeom>;
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, _: &mut App) -> (LayoutId, ()) {
        let me = TableEl { slot: self.slot.clone(), blk: self.blk.clone(), parts: self.parts.clone(), sels: self.sels.clone(), fs: self.fs, pad: self.pad, edge: self.edge, sf: self.sf, row: self.row, hits: self.hits.clone(), hot: self.hot.clone() };
        let id = window.request_measured_layout(Style::default(), move |known, avail, window, _| {
            let w = known.width.or(match avail.width { AvailableSpace::Definite(w) => Some(w), _ => None }).unwrap_or(px(0.));
            let h = me.lay(w, window);
            size(w, me.pad.0 + h + me.pad.1)
        });
        (id, ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, _: &mut App) -> Option<TableGeom> {
        self.lay(bounds.size.width, window);
        let slot = self.slot.borrow();
        let l = slot.as_ref()?;
        let g = self.geom(bounds, l);
        let mut hits = self.hits.borrow_mut();
        for (k, ((_, at, w), text)) in g.cells.iter().zip(&l.cells).enumerate() {
            if self.blk.parts.get(k).is_none() { continue; }
            hits.push(Hit { row: self.row, part: k, layout: HitLayout::Lines { lines: text.clone(), bounds: Bounds { origin: *at, size: size(*w, text.h) }, text: self.parts.get(k).cloned().unwrap_or_default() } });
        }
        Some(g)
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), g: &mut Option<TableGeom>, window: &mut Window, cx: &mut App) {
        let (Some(g), Some(l)) = (g.as_ref(), self.slot.borrow().as_ref().map(|l| l.cells.clone())) else { return };
        let (cols, _) = self.dims();
        let one = snap(px(1.), self.sf);
        // As the divs painted: (overflow hidden) the header's background,
        // each cell's text then its left line, each row's top line, then the
        // table's rounded border over them.
        let inner = Bounds { origin: g.table.origin + point(one, one), size: size(g.table.size.width - one * 2., g.table.size.height - one * 2.) };
        window.with_content_mask(Some(ContentMask { bounds: inner }), |window| {
            for (y, row) in g.rows.iter().enumerate() {
                if y == 0 { window.paint_quad(fill(*row, T::bg_secondary())); }
                for x in 0..cols {
                    let k = y * cols + x;
                    let Some((cell, at, w)) = g.cells.get(k) else { continue };
                    if let Some(t) = l.get(k) { t.paint(Bounds { origin: *at, size: size(*w, t.h) }, window, cx); }
                    if x > 0 { window.paint_quad(quad(*cell, Corners::default(), clear(T::border()), Edges { left: one, ..Default::default() }, T::border(), BorderStyle::Solid)); }
                }
                if y > 0 { window.paint_quad(quad(*row, Corners::default(), clear(T::border()), Edges { top: one, ..Default::default() }, T::border(), BorderStyle::Solid)); }
            }
        });
        window.paint_quad(quad(g.table, Corners::all(T::R_CHIP), clear(T::border()), Edges::all(one), T::border(), BorderStyle::Solid));
    }
}

/// How a piece of text is drawn.
#[derive(Clone, Copy)]
pub(crate) struct Look { pub(crate) color: Hsla, pub(crate) mono: bool, pub(crate) bold: bool, pub(crate) link: Option<Hsla> }

pub(crate) fn font(mono: bool, bold: bool, italic: bool) -> Font {
    let mut f = crundi_theme::ui_font();
    if mono { f.family = T::MONO.into(); }
    if bold { f.weight = T::W_BOLD; }
    if italic { f.style = FontStyle::Italic; }
    f
}

pub(crate) fn syn_color(s: Syn) -> Hsla {
    match s {
        Syn::Comment => T::syn_comment(), Syn::String => T::syn_string(), Syn::Number => T::syn_number(), Syn::Keyword => T::syn_keyword(),
        Syn::Type => T::syn_type(), Syn::Function => T::syn_function(), Syn::Tag => T::syn_keyword(), Syn::Attribute => T::syn_number(),
    }
}

/// The text's runs: its spans' styles, and the selection's background.
pub(crate) fn runs(inl: &Inline, look: Look, sel: Option<std::ops::Range<usize>>) -> Vec<TextRun> { runs_hot(inl, look, sel, None) }

/// The runs, the hovered path in the hover colour (the web's `.cc-path:hover`).
pub(crate) fn runs_hot(inl: &Inline, look: Look, sel: Option<std::ops::Range<usize>>, hot: Hot) -> Vec<TextRun> {
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
                if matches!(l, Link::Path(_)) && hot.as_ref().is_some_and(|h| h.start <= a && h.end >= b) { color = T::accent_hover(); }
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
pub(crate) fn heading_size(lv: u8, fs: f32) -> Pixels {
    match lv { 3 => T::TEXT_14 * fs, 4 => T::TEXT_13 * fs, 5 => T::TEXT_13 * fs * 0.83, _ => T::TEXT_13 * fs * 0.67 }
}

/// A block's top and bottom margins (px), as the web's CSS gives them: its
/// reset (`* { margin: 0 }`, webapp-html.js) leaves h4-h6 and hr with none;
/// `.cc-assistant` gives p, lists, pre, blockquote and h1-h3 theirs.
pub(crate) fn margins(k: &Kind, _fs: f32) -> (f32, f32) {
    match k {
        Kind::Para | Kind::Quote | Kind::Code | Kind::Table { .. } => (0., 8.),
        Kind::Item { last, .. } => (2., if *last { 8. } else { 2. }),
        Kind::Heading(3) => (12., 6.),
        Kind::Heading(_) | Kind::Rule => (0., 0.),
    }
}