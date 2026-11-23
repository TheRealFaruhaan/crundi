//! A container's shell (crundi-docker.js openShell: an xterm.js terminal
//! over /ws, `docker-shell-open` / `-input` / `-resize` / `-close`), on the
//! Workspace area's terminal emulator (crundi_workspace::vt) and its key and
//! colour rules (crundi_workspace::terms), as the Claude terminal panes are.
//!
//! On ARCHITECTURE.md: the emulator lives in `ShellTerm` (an entity); the
//! screen (`ShellScreen`) and its cursor (`ShellCursor`) are cached leaves of
//! their own, so a burst of output redraws the screen alone, never the
//! Docker tab, and the blink redraws only the cursor (and only while the
//! screen has the keys). A line is shaped once when it changes or comes into
//! view (kept by line id), not per frame.

use crundi_theme::T;
use crundi_workspace::{terms, vt};
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::json;
use std::{cell::Cell as StdCell, collections::HashMap, rc::Rc, time::Duration};

/// xterm.js's scrollback in the web's shell (scrollback: 5000).
const SCROLLBACK: usize = 5000;

/// One shell's emulator: the ref it talks on, how far the view is scrolled
/// back, the size last sent.
pub struct ShellTerm {
    pub term: vt::Term,
    pub r#ref: Option<String>,
    pub back: usize,
    sent: (usize, usize),
    /// Bumped when a shell opens: the screen takes the keys (term.focus()).
    focus_gen: u64,
    /// The screen has measured its size in cells (the fit addon's first fit).
    measured: bool,
    /// A shell asked for before the screen measured: opened at the fitted
    /// size (as the web fits xterm before docker-shell-open), never at a
    /// guess then resized (a resize redraws the prompt as the first keys come).
    pending: Option<String>,
    /// A cell's size as measured by the screen (the cursor reads it).
    cell: Rc<StdCell<(f32, f32)>>,
}

impl ShellTerm {
    pub fn new() -> Self {
        let mut term = vt::Term::new(80, 24);
        term.max_scrollback = SCROLLBACK;
        ShellTerm { term, r#ref: None, back: 0, sent: (80, 24), focus_gen: 0, measured: false, pending: None, cell: Rc::new(StdCell::new((8., 16.))) }
    }

    /// A fresh terminal for a new shell (the web disposes the old xterm and
    /// makes a new one), opened on `id` once the screen knows its size.
    pub fn open(&mut self, r: String, id: String, cx: &mut Context<Self>) {
        let (c, rows) = (self.term.cols, self.term.rows);
        self.term = vt::Term::new(c, rows);
        self.term.max_scrollback = SCROLLBACK;
        self.back = 0;
        self.sent = (c, rows);
        self.r#ref = Some(r);
        self.focus_gen += 1;
        if self.measured { self.send_open(&id, cx); } else { self.pending = Some(id); }
        cx.notify();
    }

    fn send_open(&self, id: &str, cx: &App) {
        let Some(r) = &self.r#ref else { return };
        crundi_shell::ws_send(cx, &json!({ "type": "docker-shell-open", "ref": r, "id": id, "cols": self.term.cols, "rows": self.term.rows }));
    }

    pub fn close(&mut self) -> Option<String> { self.pending = None; self.r#ref.take() }

    /// No shell (the container is not running): an empty screen.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.r#ref = None;
        self.pending = None;
        self.term.reset();
        self.back = 0;
        cx.notify();
    }

    /// Output from the shell: fed, one notify (GPUI draws once a frame).
    pub fn output(&mut self, data: &str, cx: &mut Context<Self>) {
        let before = self.term.scrollback.len();
        self.term.feed(data);
        if self.back > 0 { self.back = (self.back + self.term.scrollback.len().saturating_sub(before)).min(self.term.scrollback.len()); }
        for r in std::mem::take(&mut self.term.replies) { self.input(&r, cx); }
        cx.notify();
    }

    pub fn input(&self, data: &str, cx: &App) {
        if data.is_empty() { return; }
        if let Some(r) = &self.r#ref { crundi_shell::ws_send(cx, &json!({ "type": "docker-shell-input", "ref": r, "data": data })); }
    }

    /// The screen's size in cells (the fit addon): the emulator and the
    /// shell follow.
    fn size(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let (c, r) = (cols.max(2), rows.max(1));
        let first = !self.measured;
        self.measured = true;
        if (self.term.cols, self.term.rows) != (c, r) { self.term.resize(c, r); cx.notify(); }
        // The shell waiting for its size opens now, at it.
        if let Some(id) = self.pending.take() { self.sent = (c, r); self.send_open(&id, cx); return; }
        if self.sent != (c, r) || first {
            if self.sent != (c, r) && let Some(rf) = &self.r#ref { crundi_shell::ws_send(cx, &json!({ "type": "docker-shell-resize", "ref": rf, "cols": c, "rows": r })); }
            self.sent = (c, r);
        }
    }

    fn key(&mut self, text: &str, ctrl: bool, alt: bool, shift: bool, cx: &mut Context<Self>) -> bool {
        if ctrl && shift && (text == "c" || text == "C") { return false; }
        if ctrl && (text == "v" || text == "V") {
            if let Some(t) = cx.read_from_clipboard().and_then(|c| c.text()) {
                let t = t.replace("\r\n", "\r").replace('\n', "\r");
                let t = if self.term.bracketed { format!("\x1b[200~{t}\x1b[201~") } else { t };
                self.input(&t, cx);
            }
            return true;
        }
        let Some(b) = vt::key_bytes(text, ctrl, alt, shift, self.term.app_cursor) else { return false };
        self.input(&b, cx);
        if self.back != 0 { self.back = 0; cx.notify(); }
        true
    }

    fn scroll(&mut self, lines: i32, cx: &mut Context<Self>) {
        if self.term.is_alt() {
            let app = self.term.app_cursor;
            let k = if lines > 0 { if app { "\x1bOA" } else { "\x1b[A" } } else if app { "\x1bOB" } else { "\x1b[B" };
            self.input(&k.repeat(lines.unsigned_abs() as usize), cx);
            return;
        }
        let max = self.term.scrollback.len();
        let back = if lines > 0 { (self.back + lines as usize).min(max) } else { self.back.saturating_sub(lines.unsigned_abs() as usize) };
        if back != self.back { self.back = back; cx.notify(); }
    }

    fn select(&mut self, c0: usize, r0: usize, c1: usize, r1: usize, cx: &mut Context<Self>) {
        let (c0, r0, c1, r1) = if (r1, c1) < (r0, c0) { (c1, r1, c0, r0) } else { (c0, r0, c1, r1) };
        let text = { let v = self.term.view(self.back); terms::selection_text(&v, c0, r0, c1, r1) };
        if !text.is_empty() { cx.write_to_clipboard(ClipboardItem::new_string(text)); }
    }

    /// Everything the shell printed, as text (the driver's shellText).
    pub fn text(&self) -> String { term_text(&self.term) }
}

fn term_text(t: &vt::Term) -> String {
    let mut lines: Vec<String> = t.scrollback.iter().map(|l| vt::Term::text_of(l)).collect();
    lines.extend(t.screen_text());
    while lines.last().is_some_and(|l| l.is_empty()) { lines.pop(); }
    lines.join("\n")
}

// ─── Colours (TERM_THEME, as the terminal panes') ───

fn rgb3(r: u8, g: u8, b: u8) -> Hsla { rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into() }

fn named() -> [Hsla; 16] {
    [T::bg_tertiary(), T::red(), T::green(), T::yellow(), T::accent(), T::ws_term_magenta(), T::ws_term_cyan(), T::text(),
     T::text_muted(), T::ws_diff_del_text(), T::ws_diff_add_text(), T::amber(), T::accent_hover(), T::ws_term_magenta_hi(), T::ws_term_cyan_hi(), T::white()]
}

fn resolve(c: vt::Color, fallback: Hsla, bold: bool, named: &[Hsla]) -> Hsla {
    match c {
        vt::Color::Default => fallback,
        vt::Color::Idx(i) if bold && i < 8 => terms::palette(i + 8, named),
        vt::Color::Idx(i) => terms::palette(i, named),
        vt::Color::Rgb(r, g, b) => rgb3(r, g, b),
    }
}

fn mono(bold: bool, italic: bool) -> Font {
    let mut f = font(T::MONO);
    if bold { f.weight = T::W_BOLD; }
    if italic { f.style = FontStyle::Italic; }
    f
}

/// A run as painted: its column and width in cells, its shaped text, its background.
struct Piece { col: usize, cells: usize, line: ShapedLine, bg: Option<Hsla> }

fn shape_row(row: &[vt::Cell], named: &[Hsla], fsize: Pixels, cw: f32, window: &Window) -> Rc<Vec<Piece>> {
    let mut out = vec![];
    for r in terms::split_runs(row) {
        let st = &r.st;
        let mut fg = resolve(st.fg, T::text(), st.bold, named);
        let bgc = if st.bg == vt::Color::Default { None } else { Some(resolve(st.bg, T::bg(), false, named)) };
        let mut bg = bgc;
        if st.inverse { let f = fg; fg = bgc.unwrap_or(T::bg()); bg = Some(f); }
        if st.dim { fg = fg.opacity(T::WS_TERM_DIM); }
        let underline = st.underline.then(|| UnderlineStyle { thickness: px(1.), color: Some(fg), wavy: false });
        let text: SharedString = r.text.into();
        let tr = TextRun { len: text.len(), font: mono(st.bold, st.italic), color: fg, background_color: None, underline, strikethrough: None };
        let force = text.is_ascii().then_some(px(cw));
        let line = window.text_system().shape_line(text, fsize, &[tr], force);
        out.push(Piece { col: r.col, cells: r.cells, line, bg });
    }
    Rc::new(out)
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Sel { c0: usize, r0: usize, c1: usize, r1: usize, on: bool }

// ─── The screen (a cached leaf) ───

pub struct ShellScreen {
    state: Entity<ShellTerm>,
    pub focus: FocusHandle,
    rows: HashMap<u64, Rc<Vec<Piece>>>,
    rows_for: Pixels,
    shown: Rc<Vec<Rc<Vec<Piece>>>>,
    drawn: Option<(u64, usize, Pixels)>,
    cell: (f32, f32),
    cell_for: Pixels,
    focus_seen: u64,
    origin: Rc<StdCell<Point<Pixels>>>,
    pressing: bool,
    sel: Sel,
    _sub: Subscription,
}

/// The shell's area: the screen with its cursor over it (both cached).
pub struct ShellViews { pub screen: Entity<ShellScreen>, pub cursor: Entity<ShellCursor> }

pub fn views(state: &Entity<ShellTerm>, window: &mut Window, cx: &mut App) -> ShellViews {
    let screen = cx.new(|cx| ShellScreen::new(state.clone(), cx));
    let focus = screen.read(cx).focus.clone();
    let cursor = cx.new(|cx| ShellCursor::new(state.clone(), focus, window, cx));
    ShellViews { screen, cursor }
}

impl ShellViews {
    pub fn element(&self) -> Div {
        div().relative().flex_1().min_w_0().min_h_0().bg(T::bg())
            .child(AnyView::from(self.screen.clone()).cached(StyleRefinement::default().size_full()))
            .child(AnyView::from(self.cursor.clone()).cached(StyleRefinement::default().absolute().top_0().left_0().size_full()))
    }
}

/// The web's .dk-shell padding (6px top, 8px left).
fn pad() -> Point<Pixels> { point(T::SP_8, T::SP_6) }

impl ShellScreen {
    fn new(state: Entity<ShellTerm>, cx: &mut Context<Self>) -> Self {
        let _sub = cx.observe(&state, |_, _, cx| cx.notify());
        ShellScreen { state, focus: cx.focus_handle(), rows: HashMap::new(), rows_for: px(0.), shown: Rc::new(vec![]), drawn: None, cell: (8., 16.), cell_for: px(0.), focus_seen: 0, origin: Rc::new(StdCell::new(point(px(0.), px(0.)))), pressing: false, sel: Sel::default(), _sub }
    }

    fn cell_at(&self, p: Point<Pixels>) -> (usize, usize) {
        let o = self.origin.get() + pad();
        let (cw, ch) = (self.cell.0.max(1.), self.cell.1.max(1.));
        ((f32::from(p.x - o.x).max(0.) / cw) as usize, (f32::from(p.y - o.y).max(0.) / ch) as usize)
    }
}

impl Render for ShellScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("dk-shell-screen");
        let fsize = T::TEXT_13;
        if self.cell_for != fsize {
            const PROBE: &str = "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM";
            let run = TextRun { len: PROBE.len(), font: font(T::MONO), color: T::text(), background_color: None, underline: None, strikethrough: None };
            let line = window.text_system().shape_line(PROBE.into(), fsize, &[run], None);
            self.cell = (f32::from(line.width) / PROBE.len() as f32, f32::from(fsize) * T::WS_TERM_LH);
            self.cell_for = fsize;
        }
        // A new shell takes the keys (the web's term.focus()).
        let fg = self.state.read(cx).focus_gen;
        if fg != self.focus_seen {
            self.focus_seen = fg;
            let f = self.focus.clone();
            window.defer(cx, move |window, cx| f.focus(window, cx));
        }
        let st = self.state.read(cx);
        st.cell.set(self.cell);
        // The rows on show, taken again only when the terminal changed; only
        // lines that changed or came into view are shaped.
        let key = (st.term.generation, st.back, fsize);
        if self.drawn != Some(key) {
            if self.rows_for != fsize { self.rows.clear(); self.rows_for = fsize; }
            let named = named();
            let mut next = HashMap::new();
            let shown: Vec<Rc<Vec<Piece>>> = st.term.view(st.back).iter().map(|l| {
                let r = next.get(&l.id()).or_else(|| self.rows.get(&l.id())).cloned().unwrap_or_else(|| shape_row(l, &named, fsize, self.cell.0, window));
                next.insert(l.id(), r.clone());
                r
            }).collect();
            self.rows = next;
            self.shown = Rc::new(shown);
            self.drawn = Some(key);
        }
        let (cw, ch) = self.cell;
        let cols = st.term.cols;
        let rows = self.shown.clone();
        let sel = self.sel;
        let origin = self.origin.clone();
        let state = self.state.downgrade();
        let grid = canvas(
            move |bounds, _, cx| {
                origin.set(bounds.origin);
                let p = pad();
                let (bw, bh) = (f32::from(bounds.size.width - p.x), f32::from(bounds.size.height - p.y));
                let (c, r) = ((bw / cw.max(1.)).floor().max(2.) as usize, (bh / ch.max(1.)).floor().max(1.) as usize);
                let state = state.clone();
                cx.defer(move |cx| { let _ = state.update(cx, |t, cx| t.size(c, r, cx)); });
            },
            move |bounds, _, window, cx| {
                let o = bounds.origin + pad();
                let lh = px(ch);
                window.paint_layer(bounds, |window| {
                    for (r, row) in rows.iter().enumerate() {
                        let y = o.y + px(r as f32 * ch);
                        for p in row.iter() {
                            if let Some(b) = p.bg { window.paint_quad(fill(Bounds { origin: point(o.x + px(p.col as f32 * cw), y), size: size(px(p.cells as f32 * cw), lh) }, b)); }
                        }
                    }
                    if sel.on {
                        let ((c0, r0), (c1, r1)) = if (sel.r1, sel.c1) < (sel.r0, sel.c0) { ((sel.c1, sel.r1), (sel.c0, sel.r0)) } else { ((sel.c0, sel.r0), (sel.c1, sel.r1)) };
                        for r in r0..=r1 {
                            let from = if r == r0 { c0 } else { 0 };
                            let to = if r == r1 { c1 } else { cols.saturating_sub(1) };
                            if to < from { continue; }
                            window.paint_quad(fill(Bounds { origin: point(o.x + px(from as f32 * cw), o.y + px(r as f32 * ch)), size: size(px((to - from + 1) as f32 * cw), lh) }, T::ws_term_sel()));
                        }
                    }
                    for (r, row) in rows.iter().enumerate() {
                        let y = o.y + px(r as f32 * ch);
                        for p in row.iter() { let _ = p.line.paint(point(o.x + px(p.col as f32 * cw), y), lh, TextAlign::Left, None, window, cx); }
                    }
                });
            },
        ).absolute().top_0().left_0().size_full();
        div().id("dk-shell").track_focus(&self.focus).relative().size_full().overflow_hidden().bg(T::bg()).cursor_text()
            .child(grid)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let m = &e.keystroke.modifiers;
                let Some(text) = terms::key_text(&e.keystroke.key, e.keystroke.key_char.as_deref(), m.control) else { return };
                if this.state.update(cx, |t, cx| t.key(&text, m.control, m.alt, m.shift, cx)) { cx.stop_propagation(); }
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                this.focus.focus(window, cx);
                let (c, r) = this.cell_at(e.position);
                this.pressing = true;
                let was = this.sel.on;
                this.sel = Sel { c0: c, r0: r, c1: c, r1: r, on: false };
                if was { cx.notify(); }
            }))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if !this.pressing || e.pressed_button != Some(MouseButton::Left) { return; }
                let (c, r) = this.cell_at(e.position);
                let mut s = this.sel;
                s.c1 = c; s.r1 = r;
                if (c, r) != (s.c0, s.r0) { s.on = true; }
                if s != this.sel { this.sel = s; cx.notify(); }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if !this.pressing { return; }
                this.pressing = false;
                if this.sel.on { let s = this.sel; this.state.update(cx, |t, cx| t.select(s.c0, s.r0, s.c1, s.r1, cx)); }
            }))
            .on_scroll(cx.listener(move |this, e: &ScrollEvent, _, cx| {
                let y = f32::from(e.delta.pixel_delta(px(ch)).y);
                // (T7: a sideways scroll is not the terminal's: it goes on, to
                // the workbench strip.)
                let d = e.delta.pixel_delta(px(ch));
                if (e.modifiers.shift && d.x == px(0.)) || d.x.abs() > d.y.abs() { return; }
                if y > 0. { this.state.update(cx, |t, cx| t.scroll(T::WS_TERM_WHEEL, cx)); } else if y < 0. { this.state.update(cx, |t, cx| t.scroll(-T::WS_TERM_WHEEL, cx)); }
                cx.stop_propagation();
            }))
    }
}

// ─── The cursor (its blink redraws it alone, only while the screen has the keys) ───

pub struct ShellCursor {
    state: Entity<ShellTerm>,
    focus: FocusHandle,
    blink_on: bool,
    _blink: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl ShellCursor {
    fn new(state: Entity<ShellTerm>, focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subs = vec![
            cx.observe(&state, |this: &mut Self, _, cx| { this.blink_on = true; cx.notify(); }),
            cx.on_focus(&focus, window, |this, _, cx| { this.blink_on = true; this.blink(cx); cx.notify(); }),
            cx.on_blur(&focus, window, |this, _, cx| { this._blink = None; this.blink_on = true; cx.notify(); }),
        ];
        ShellCursor { state, focus, blink_on: true, _blink: None, _subs: subs }
    }

    fn blink(&mut self, cx: &mut Context<Self>) {
        self._blink = Some(cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| loop {
            cx.background_executor().timer(Duration::from_millis(T::WS_TERM_BLINK_MS)).await;
            if this.update(cx, |c, cx| { c.blink_on = !c.blink_on; cx.notify(); }).is_err() { return; }
        }));
    }
}

impl Render for ShellCursor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("dk-shell-cursor");
        let st = self.state.read(cx);
        let root = div().size_full();
        if st.r#ref.is_none() || !st.term.cursor_on || st.back > 0 { return root; }
        let (cw, ch) = st.cell.get();
        let (x, y) = st.term.cursor();
        let p = pad();
        let focused = self.focus.is_focused(window);
        let c = div().absolute().top(p.y + px(y as f32 * ch)).left(p.x + px(x as f32 * cw)).w(px(cw)).h(px(ch));
        root.child(if focused { c.when(self.blink_on, |d| d.bg(T::accent_edge())) } else { c.border_1().border_color(T::accent()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn text_keeps_scrollback() {
        let mut t = vt::Term::new(10, 2);
        t.feed("a\r\nb\r\nc\r\n$ ");
        assert_eq!(term_text(&t), "a\nb\nc\n$");
    }
}
