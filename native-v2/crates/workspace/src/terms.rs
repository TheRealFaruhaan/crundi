//! The Claude terminal panes (3.2; the web's mountXterm, termSendKey,
//! termSendInput and the input bar; native-gpui's terms.rs): each terminal in
//! a pane ("term:<id>") is subscribed on /ws (`subscribe`: the server sends
//! its scrollback, then `output`), fed into its emulator (vt.rs) and drawn in
//! cells; keys typed into it, a paste, the wheel and its size go back as
//! `input` and `resize`. Its own input bar: the text box (Ctrl+Enter or Send
//! sends the text, then Enter on its own), the key tools, attach (to
//! crundi_attachments).
//!
//! Built on ARCHITECTURE.md:
//! - `TermState` (an entity) holds the emulator. The live hub's batch is one
//!   frame's events: each terminal's `output` in it is joined and fed once,
//!   and the state notifies once (at most one update per frame, as Zed's
//!   terminal batches its pty events).
//! - The pane's body is layout only: three cached leaves side by side, the
//!   screen (`TermScreen`), the cursor over it (`TermCursor`: its blink
//!   redraws only itself) and the input bar (`TermInput`: typing in the box
//!   never redraws the screen; output never redraws the box).
//! - The screen is one element (`grid`): the rows as runs of one style,
//!   derived once per change (not per frame), each painted as a shaped line
//!   at its column (the text system keeps shaped lines between frames).

use crate::vt;
use crundi_core::attach as rules;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::Cell as StdCell, collections::HashMap, rc::Rc, sync::{Arc, Mutex}, time::Duration};

pub fn key_of(id: &str) -> String { format!("term:{id}") }
pub fn id_of(key: &str) -> String { key.strip_prefix("term:").unwrap_or(key).to_string() }

/// A pane's text size (A- / A+), as a chat's: 13 px and a pixel a step.
pub fn scale(zoom: i32) -> f32 { (13.0 + zoom as f32) / 13.0 }

fn send(cx: &App, id: &str, data: &str) {
    if data.is_empty() { return; }
    crundi_shell::ws_send(cx, &json!({ "type": "input", "id": id, "data": data }));
}

// ─── The state ───

/// One terminal: its emulator, how far the view is scrolled back, its size
/// as sent to the server, its text size.
pub struct TermState {
    pub id: String,
    pub term: vt::Term,
    pub back: usize,
    pub sized: bool,
    sent: (usize, usize),
    pub zoom: i32,
    /// The output batches fed (one per frame at most).
    pub batches: u64,
    /// A cell's size as last measured by the screen (the cursor reads it).
    pub cell: Rc<StdCell<(f32, f32)>>,
}

impl TermState {
    fn new(id: &str) -> Self {
        TermState { id: id.to_string(), term: vt::Term::new(80, 24), back: 0, sized: false, sent: (0, 0), zoom: 0, batches: 0, cell: Rc::new(StdCell::new((8., 16.))) }
    }

    /// A frame's output, joined: fed once, one notify.
    pub fn output(&mut self, data: &str, cx: &mut Context<Self>) {
        let before = self.term.scrollback.len();
        self.term.feed(data);
        // Scrolled up: the view stays on the same lines as more come.
        if self.back > 0 { self.back = (self.back + self.term.scrollback.len().saturating_sub(before)).min(self.term.scrollback.len()); }
        self.batches += 1;
        for r in std::mem::take(&mut self.term.replies) { send(cx, &self.id, &r); }
        cx.notify();
    }

    /// The pane's size in cells: the terminal and the server follow.
    pub fn size(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let (c, r) = (cols.max(2), rows.max(1));
        if self.sized && self.sent == (c, r) { return; }
        self.term.resize(c, r);
        self.sized = true;
        self.sent = (c, r);
        crundi_shell::ws_send(cx, &json!({ "type": "resize", "id": self.id, "cols": c, "rows": r }));
        cx.notify();
    }

    /// Back online: from the scrollback the server sends on subscribe again
    /// (the web resets each first).
    pub fn resubscribe(&mut self, cx: &mut Context<Self>) {
        self.term.reset();
        self.back = 0;
        crundi_shell::ws_send(cx, &json!({ "type": "subscribe", "id": self.id }));
        crundi_shell::ws_send(cx, &json!({ "type": "resize", "id": self.id, "cols": self.term.cols, "rows": self.term.rows }));
        cx.notify();
    }

    /// A key typed into the terminal: its bytes (xterm's); the newline key
    /// (Settings › Terminal: Shift+Enter, the default, or Ctrl+Enter) a new
    /// line (ESC CR, as the web's); Ctrl+V a paste. True: taken.
    pub fn key(&mut self, text: &str, ctrl: bool, alt: bool, shift: bool, cx: &mut Context<Self>) -> bool {
        if ctrl && (text == "v" || text == "V") { self.paste(cx); return true; }
        let enter = text == "\n" || text == "\r";
        let newline = enter && !alt && (ctrl || shift) && { let c = newline_ctrl(); if c { ctrl && !shift } else { shift && !ctrl } };
        let bytes = if newline { Some("\x1b\r".to_string()) } else { vt::key_bytes(text, ctrl, alt, shift, self.term.app_cursor) };
        let Some(b) = bytes else { return false };
        send(cx, &self.id, &b);
        self.bottom(cx);
        true
    }

    /// Ctrl+V: a picture on the clipboard is saved in the project
    /// (/api/clipboard/paste-image) and its path typed in, "Screenshot
    /// saved: <name>"; else the clipboard's text (bracketed when it asks).
    pub fn paste(&mut self, cx: &mut Context<Self>) {
        let item = cx.read_from_clipboard();
        if let Some(img) = item.as_ref().and_then(clip_image) { return self.paste_image(img, cx); }
        let Some(text) = item.and_then(|c| c.text()).filter(|t| !t.is_empty()) else { return };
        let d = if self.term.bracketed { format!("\x1b[200~{text}\x1b[201~") } else { text };
        send(cx, &self.id, &d);
    }

    /// A picture's bytes to /api/clipboard/paste-image (in the open
    /// project); the path it was saved at goes to the terminal.
    pub fn paste_image(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        if project.is_empty() { return; }
        use base64::Engine;
        let body = json!({ "project": project, "data": base64::engine::general_purpose::STANDARD.encode(&bytes) });
        let id = self.id.clone();
        crundi_shell::call(cx, move |api| api.post("/api/clipboard/paste-image", &body), move |r, cx| {
            let Ok(d) = r else { return };
            let path = d.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
            if d.get("ok").and_then(|o| o.as_bool()) != Some(true) || path.is_empty() { return; }
            send(cx, &id, &path);
            let name = d.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            crate::toast(cx, format!("Screenshot saved: {name}"), w::Toast::Info);
        });
    }

    /// A selection (cells on the lines in view): copied to the clipboard (as xterm.js does).
    pub fn select(&mut self, c0: usize, r0: usize, c1: usize, r1: usize, cx: &mut Context<Self>) {
        let (c0, r0, c1, r1) = if (r1, c1) < (r0, c0) { (c1, r1, c0, r0) } else { (c0, r0, c1, r1) };
        let text = { let v = self.term.view(self.back); selection_text(&v, c0, r0, c1, r1) };
        if text.is_empty() { return; }
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        crate::note_copy(cx, text);
    }

    /// The wheel: up or down the scrollback; on the alternate screen (a
    /// full-screen program) the arrow keys instead.
    pub fn scroll(&mut self, lines: i32, cx: &mut Context<Self>) {
        if self.term.is_alt() {
            let app = self.term.app_cursor;
            let k = if lines > 0 { if app { "\x1bOA" } else { "\x1b[A" } } else if app { "\x1bOB" } else { "\x1b[B" };
            send(cx, &self.id, &k.repeat(lines.unsigned_abs() as usize));
            return;
        }
        let max = self.term.scrollback.len();
        let back = if lines > 0 { (self.back + lines as usize).min(max) } else { self.back.saturating_sub(lines.unsigned_abs() as usize) };
        if back != self.back { self.back = back; cx.notify(); }
    }

    pub fn bottom(&mut self, cx: &mut Context<Self>) { if self.back != 0 { self.back = 0; cx.notify(); } }

    pub fn tool(&mut self, name: &str, cx: &mut Context<Self>) { if let Some(b) = tool_bytes(name) { send(cx, &self.id, b); } }

    /// Send (and Ctrl+Enter): the text, then Enter as its own write 60 ms
    /// later (an agent's interface takes a burst as a paste and would keep a
    /// trailing CR in it).
    pub fn send_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.is_empty() { return; }
        send(cx, &self.id, text);
        let id = self.id.clone();
        cx.spawn(async move |_, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(60)).await;
            let _ = cx.update(|cx| send(cx, &id, "\r"));
        }).detach();
        self.bottom(cx);
    }

    pub fn set_zoom(&mut self, d: i32, cx: &mut Context<Self>) {
        let z = if d == 0 { 0 } else { (self.zoom + d).clamp(-4, 12) };
        if z != self.zoom { self.zoom = z; cx.notify(); }
    }

    pub fn props(&self) -> Value {
        let (cx, cy) = self.term.cursor();
        json!({
            "id": self.id, "key": key_of(&self.id), "cols": self.term.cols, "rows": self.term.rows, "sized": self.sized,
            "screen": self.term.screen_text(), "cursor": [cx, cy], "alt": self.term.is_alt(), "back": self.back,
            "scrollback": self.term.scrollback.len(), "title": self.term.title, "zoom": self.zoom, "batches": self.batches,
        })
    }
}

// ─── Colours and runs (derived once per change) ───

fn rgb3(r: u8, g: u8, b: u8) -> Hsla { rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into() }

/// TERM_THEME's sixteen: black, red, green, yellow, blue, magenta, cyan,
/// white, then the bright ones.
fn named() -> [Hsla; 16] {
    [T::bg_tertiary(), T::red(), T::green(), T::yellow(), T::accent(), T::ws_term_magenta(), T::ws_term_cyan(), T::text(),
     T::text_muted(), T::ws_diff_del_text(), T::ws_diff_add_text(), T::amber(), T::accent_hover(), T::ws_term_magenta_hi(), T::ws_term_cyan_hi(), T::white()]
}

/// The xterm 256-colour palette's colour (16 named from the theme).
pub fn palette(i: u8, named: &[Hsla]) -> Hsla {
    let i = i as usize;
    if i < 16 { return named.get(i).copied().unwrap_or_default(); }
    if i < 232 {
        let lv = [0u8, 95, 135, 175, 215, 255];
        let n = i - 16;
        return rgb3(lv[n / 36], lv[(n / 6) % 6], lv[n % 6]);
    }
    let v = (8 + 10 * (i - 232)) as u8;
    rgb3(v, v, v)
}

fn resolve(c: vt::Color, fallback: Hsla, bold: bool, named: &[Hsla]) -> Hsla {
    match c {
        vt::Color::Default => fallback,
        // Bold text in the eight colours is drawn bright (xterm.js's default).
        vt::Color::Idx(i) if bold && i < 8 => palette(i + 8, named),
        vt::Color::Idx(i) => palette(i, named),
        vt::Color::Rgb(r, g, b) => rgb3(r, g, b),
    }
}

/// A run's colours as drawn (inverse swapped, dim faded): fg, bg.
fn run_colors(st: &vt::Style, named: &[Hsla]) -> (Hsla, Option<Hsla>) {
    let mut fg = resolve(st.fg, T::text(), st.bold, named);
    let bgc = if st.bg == vt::Color::Default { None } else { Some(resolve(st.bg, T::bg(), false, named)) };
    let mut bg = bgc;
    if st.inverse { let f = fg; fg = bgc.unwrap_or(T::bg()); bg = Some(f); }
    if st.dim { fg = fg.opacity(T::WS_TERM_DIM); }
    (fg, bg)
}

/// A line's runs, each character past ASCII on its own (a font's fallback
/// glyph may not be a cell wide: each lands on its own column).
pub fn split_runs(row: &[vt::Cell]) -> Vec<vt::Run> {
    let mut out = vec![];
    for r in vt::runs(row) {
        if r.text.is_ascii() { out.push(r); continue; }
        let mut col = r.col;
        let mut buf = String::new();
        let mut start = col;
        for ch in r.text.chars() {
            let wd = vt::width(ch).max(1);
            if ch.is_ascii() { if buf.is_empty() { start = col; } buf.push(ch); col += 1; continue; }
            if !buf.is_empty() { out.push(vt::Run { col: start, cells: buf.len(), text: std::mem::take(&mut buf), st: r.st }); }
            out.push(vt::Run { col, cells: wd, text: ch.to_string(), st: r.st });
            col += wd;
        }
        if !buf.is_empty() { out.push(vt::Run { col: start, cells: buf.len(), text: buf, st: r.st }); }
    }
    out
}

/// A run as painted: its column and width in cells, its text, colours and face.
#[derive(Clone)]
struct PRun { col: usize, cells: usize, text: SharedString, fg: Hsla, bg: Option<Hsla>, bold: bool, italic: bool, underline: bool }

/// A line's runs, derived once per change of the line (not per frame).
fn paint_runs(row: &[vt::Cell], named: &[Hsla]) -> Vec<PRun> {
    split_runs(row).into_iter().filter_map(|r| {
        let (fg, bg) = run_colors(&r.st, named);
        if bg.is_none() && r.text.trim().is_empty() && !r.st.underline { return None; }
        Some(PRun { col: r.col, cells: r.cells, text: r.text.into(), fg, bg, bold: r.st.bold, italic: r.st.italic, underline: r.st.underline })
    }).collect()
}

/// A glyph of a row, as painted: its place from the row's top left (on its
/// run's baseline), its face and colour.
#[derive(Clone, Copy)]
struct PGlyph { at: Point<Pixels>, font: FontId, id: GlyphId, emoji: bool, color: Hsla }

/// A character as shaped once in a face (bold, italic) at the screen's text
/// size: its glyphs from the cell's left (an ASCII one fills its cell, as
/// the web's xterm.js draws a cell at a time: no ligatures), the baseline
/// and the underline's offset from the row's top.
struct CharShape { glyphs: Vec<(Pixels, FontId, GlyphId, bool)>, base: Pixels, uy: Pixels }

type CharKey = (char, bool, bool);

fn shape_char(c: char, bold: bool, italic: bool, fsize: Pixels, cw: f32, lh: Pixels, window: &Window) -> CharShape {
    let mut buf = [0u8; 4];
    let text = c.encode_utf8(&mut buf);
    let tr = TextRun { len: text.len(), font: mono(bold, italic), color: T::text(), background_color: None, underline: None, strikethrough: None };
    let layout = window.text_system().layout_line(text, fsize, &[tr], c.is_ascii().then_some(px(cw)));
    let glyphs = layout.runs.iter().flat_map(|sr| sr.glyphs.iter().map(move |g| (g.position.x, sr.font_id, g.id, g.is_emoji))).collect();
    CharShape { glyphs, base: (lh - layout.ascent - layout.descent) / 2. + layout.ascent, uy: underline_y_offset(lh, layout.ascent, layout.descent) }
}

/// One row as painted (one per visible line): its backgrounds, its glyphs
/// (from the characters' shapes, at the text size and cell width it was
/// made for) and its underlines (column, width in cells, colour, offset
/// from the row's top).
struct RowPaint { bgs: Vec<(usize, usize, Hsla)>, glyphs: Vec<PGlyph>, unders: Vec<(usize, usize, Hsla, Pixels)> }

impl RowPaint {
    fn new(row: &[vt::Cell], named: &[Hsla], fsize: Pixels, cw: f32, lh: Pixels, chars: &mut HashMap<CharKey, Rc<CharShape>>, window: &Window) -> RowPaint {
        let mut p = RowPaint { bgs: vec![], glyphs: vec![], unders: vec![] };
        let mut shape = |c: char, b: bool, i: bool| chars.entry((c, b, i)).or_insert_with(|| Rc::new(shape_char(c, b, i, fsize, cw, lh, window))).clone();
        for run in paint_runs(row, named) {
            if let Some(b) = run.bg { p.bgs.push((run.col, run.cells, b)); }
            if run.underline { let s = shape(' ', run.bold, run.italic); p.unders.push((run.col, run.cells, run.fg, s.uy)); }
            // Each character at its cell (split_runs: an ASCII run one cell
            // a character, anything else a run of its own); spaces draw nothing.
            let mut col = run.col;
            for c in run.text.chars() {
                if c != ' ' {
                    let s = shape(c, run.bold, run.italic);
                    let x0 = px(col as f32 * cw);
                    for &(x, font, id, emoji) in &s.glyphs { p.glyphs.push(PGlyph { at: point(x0 + x, s.base), font, id, emoji, color: run.fg }); }
                }
                col += if c.is_ascii() { 1 } else { vt::width(c).max(1) };
            }
        }
        p
    }
}

/// The rows on show, by line id: what the screen derived from each line,
/// kept while the line is on show and unchanged (a batch derives only the
/// lines it wrote; scrolling only the lines that came into view), and the
/// characters shaped so far, for one text size and cell width.
#[derive(Default)]
struct RowCache { rows: HashMap<u64, Rc<RowPaint>>, chars: HashMap<CharKey, Rc<CharShape>>, for_size: (Pixels, u32), derived: u64 }

impl RowCache {
    /// The visible lines' rows, each from the cache or derived now; lines
    /// no longer on show are let go.
    fn visible(&mut self, lines: &[&vt::Line], fsize: Pixels, cell: (f32, f32), window: &Window) -> Vec<Rc<RowPaint>> {
        let key = (fsize, cell.0.to_bits());
        if self.for_size != key { self.rows.clear(); self.chars.clear(); self.for_size = key; }
        let named = named();
        let lh = px(cell.1);
        let mut next: HashMap<u64, Rc<RowPaint>> = HashMap::with_capacity(lines.len());
        let out = lines.iter().map(|l| {
            let id = l.id();
            let r = match next.get(&id).or_else(|| self.rows.get(&id)) {
                Some(r) => r.clone(),
                None => { self.derived += 1; Rc::new(RowPaint::new(l, &named, fsize, cell.0, lh, &mut self.chars, window)) }
            };
            next.insert(id, r.clone());
            r
        }).collect();
        self.rows = next;
        out
    }
}

/// The text of a selection on the lines in view (from c0,r0 to c1,r1 in
/// order), each line's trailing spaces dropped.
pub fn selection_text(lines: &[&vt::Line], c0: usize, r0: usize, c1: usize, r1: usize) -> String {
    let mut out = vec![];
    for r in r0..=r1.min(lines.len().saturating_sub(1)) {
        let row: &[vt::Cell] = lines[r];
        let from = if r == r0 { c0 } else { 0 };
        let to = if r == r1 { (c1 + 1).min(row.len()) } else { row.len() };
        let s: String = row.get(from.min(row.len())..to.max(from).min(row.len())).unwrap_or(&[]).iter().filter(|c| !c.cont).map(|c| c.ch).collect();
        out.push(s.trim_end().to_string());
    }
    out.join("\n")
}

/// A key of the tools (termSendKey's map).
pub fn tool_bytes(name: &str) -> Option<&'static str> {
    Some(match name {
        "Escape" => "\x1b", "Tab" => "\t", "shift+tab" => "\x1b[Z",
        "ArrowUp" => "\x1b[A", "ArrowDown" => "\x1b[B", "ArrowRight" => "\x1b[C", "ArrowLeft" => "\x1b[D",
        "Home" => "\x1b[H", "End" => "\x1b[F",
        "ctrl+c" => "\x03", "ctrl+z" => "\x1a", "ctrl+a" => "\x01", "ctrl+e" => "\x05", "ctrl+l" => "\x0c", "ctrl+d" => "\x04",
        "Enter" => "\r", "Space" => " ",
        _ => return None,
    })
}

/// Which key makes a new line in a terminal (Settings › Terminal, kept on
/// this device by Operations' Settings in ops-settings.json: "ctrl" or the
/// default Shift). Read when Enter comes with Ctrl or Shift, so a change
/// applies to the terminals already open (the web reads it live).
pub fn newline_ctrl() -> bool {
    let f = crundi_core::store::config_dir().join("ops-settings.json");
    std::fs::read(f).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.get("newline").and_then(|n| n.as_str()).map(|n| n == "ctrl")).unwrap_or(false)
}

/// A picture on the clipboard, as bytes.
pub fn clip_image(item: &ClipboardItem) -> Option<Vec<u8>> {
    item.entries().iter().find_map(|e| if let ClipboardEntry::Image(img) = e { Some(img.bytes.clone()) } else { None })
}

/// Text put in the box where the caret is (insertIntoTermInput): a space
/// before it unless one is there, and one after.
pub fn insert_at(text: &str, at: usize, ins: &str) -> (String, usize) {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) { at -= 1; }
    let (before, after) = text.split_at(at);
    let lead = if !before.is_empty() && !before.ends_with(char::is_whitespace) { " " } else { "" };
    let trail = if after.is_empty() || !after.starts_with(char::is_whitespace) { " " } else { "" };
    let piece = format!("{lead}{ins}{trail}");
    (format!("{before}{piece}{after}"), at + piece.len())
}

/// A path put in the text box (insertIntoTermInput, at its end): a space
/// each side unless one is there.
pub fn with_path(text: &str, path: &str) -> String {
    let lead = if !text.is_empty() && !text.ends_with(char::is_whitespace) { " " } else { "" };
    format!("{text}{lead}{path} ")
}

/// A GPUI key as the emulator's key text (vt::key_bytes reads the Slint
/// client's: its private-use codes for the arrows, Home, End, F-keys…).
pub fn key_text(key: &str, key_char: Option<&str>, ctrl: bool) -> Option<String> {
    let s = match key {
        "up" => "\u{f700}", "down" => "\u{f701}", "left" => "\u{f702}", "right" => "\u{f703}",
        "home" => "\u{f729}", "end" => "\u{f72b}", "pageup" => "\u{f72c}", "pagedown" => "\u{f72d}",
        "insert" => "\u{f727}", "delete" => "\u{7f}", "backspace" => "\u{8}",
        "enter" => "\n", "tab" => "\t", "escape" => "\u{1b}", "space" => " ",
        k if k.len() >= 2 && k.starts_with('f') && k[1..].parse::<u32>().is_ok_and(|n| (1..=12).contains(&n)) => {
            let n: u32 = k[1..].parse().unwrap_or(1);
            return char::from_u32(0xf704 + n - 1).map(|c| c.to_string());
        }
        k => {
            if ctrl && k.chars().count() == 1 { return Some(k.to_string()); }
            return key_char.filter(|c| !c.is_empty()).map(String::from).or_else(|| (k.chars().count() == 1).then(|| k.to_string()));
        }
    };
    Some(s.to_string())
}

// ─── The screen (a cached leaf) ───

/// A drag over the terminal, by cells: from (c0, r0) to (c1, r1).
#[derive(Clone, Copy, Default, PartialEq)]
struct Sel { c0: usize, r0: usize, c1: usize, r1: usize, on: bool }

fn mono(bold: bool, italic: bool) -> Font {
    let mut f = font(T::MONO);
    if bold { f.weight = T::W_BOLD; }
    if italic { f.style = FontStyle::Italic; }
    f
}

/// The time the screens spent rendering and painting (µs, all terminals),
/// and how many paints: the driver's props show it (what a terminal frame
/// costs the UI thread, measured, not sampled).
pub static SCREEN_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static SCREEN_PAINTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn add_us(t: std::time::Instant) { SCREEN_US.fetch_add(t.elapsed().as_micros() as u64, std::sync::atomic::Ordering::Relaxed); }

pub struct TermScreen {
    state: Entity<TermState>,
    pub focus: FocusHandle,
    /// Each visible line's row as painted, kept by line id (RowCache), and
    /// the rows on show with what they were taken for (the terminal's
    /// generation, the scroll and the text size; the selection does not
    /// count: drawn over).
    cache: RowCache,
    shown: Rc<Vec<Rc<RowPaint>>>,
    drawn: Option<(u64, usize, Pixels)>,
    /// A cell's size, for the text size it was measured at.
    cell: (f32, f32),
    cell_for: Pixels,
    origin: Rc<StdCell<Point<Pixels>>>,
    pressing: bool,
    sel: Sel,
    _sub: Subscription,
}

impl TermScreen {
    fn new(state: Entity<TermState>, cx: &mut Context<Self>) -> Self {
        let _sub = cx.observe(&state, |_, _, cx| cx.notify());
        TermScreen { state, focus: cx.focus_handle(), cache: RowCache::default(), shown: Rc::new(vec![]), drawn: None, cell: (8., 16.), cell_for: px(0.), origin: Rc::new(StdCell::new(point(px(0.), px(0.)))), pressing: false, sel: Sel::default(), _sub }
    }

    fn cell_at(&self, p: Point<Pixels>) -> (usize, usize) {
        let pad = f32::from(T::SP_4);
        let o = self.origin.get();
        let (cw, ch) = (self.cell.0.max(1.), self.cell.1.max(1.));
        let x = (f32::from(p.x - o.x) - pad).max(0.);
        let y = (f32::from(p.y - o.y) - pad).max(0.);
        ((x / cw) as usize, (y / ch) as usize)
    }
}

impl Render for TermScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("term-screen");
        let t0 = std::time::Instant::now();
        let st = self.state.read(cx);
        let fsize = T::TEXT_13 * scale(st.zoom);
        // The cell: the mono face's advance at this size, measured on a
        // shaped line of cells (a glyph's advance alone may be off by the
        // renderer's rounding over a long run); once per text size.
        if self.cell_for != fsize {
            const PROBE: &str = "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM";
            let run = TextRun { len: PROBE.len(), font: font(T::MONO), color: T::text(), background_color: None, underline: None, strikethrough: None };
            let line = window.text_system().shape_line(PROBE.into(), fsize, &[run], None);
            self.cell = (f32::from(line.width) / PROBE.len() as f32, f32::from(fsize) * T::WS_TERM_LH);
            self.cell_for = fsize;
            st.cell.set(self.cell);
        }
        // The rows on show, taken again only when the terminal changed; of
        // those only the lines that changed or came into view are derived
        // and shaped (the rest are the cache's).
        let key = (st.term.generation, st.back, fsize);
        if self.drawn != Some(key) {
            let lines = st.term.view(st.back);
            self.shown = Rc::new(self.cache.visible(&lines, fsize, self.cell, window));
            self.drawn = Some(key);
        }
        let (cw, ch) = self.cell;
        let (back, cols) = (st.back, st.term.cols);
        let id = st.id.clone();
        let rows = self.shown.clone();
        let sel = self.sel;
        let origin = self.origin.clone();
        let state = self.state.downgrade();
        let pad = T::SP_4;
        let grid = canvas(
            move |bounds, _, cx| {
                origin.set(bounds.origin);
                let (bw, bh) = (f32::from(bounds.size.width) - 2. * f32::from(pad), f32::from(bounds.size.height) - 2. * f32::from(pad));
                let (c, r) = ((bw / cw.max(1.)).floor().max(2.) as usize, (bh / ch.max(1.)).floor().max(1.) as usize);
                // (After the frame: the server and the emulator follow the pane.)
                let state = state.clone();
                cx.defer(move |cx| { let _ = state.update(cx, |t, cx| t.size(c, r, cx)); });
            },
            move |bounds, _, window, _| {
                let t0 = std::time::Instant::now();
                SCREEN_PAINTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let o = bounds.origin + point(pad, pad);
                let lh = px(ch);
                // One paint layer for the whole screen (as the chat's log):
                // its quads and glyphs take the layer's order, so the screen
                // costs one insert into the scene's bounds tree, not one per
                // glyph. Each row paints what it shaped when its line last
                // changed: backgrounds and the selection (quads, under the
                // text), then its glyphs and underlines.
                window.paint_layer(bounds, |window| {
                    for (r, row) in rows.iter().enumerate() {
                        let y = o.y + px(r as f32 * ch);
                        for &(col, cells, b) in &row.bgs { window.paint_quad(fill(Bounds { origin: point(o.x + px(col as f32 * cw), y), size: size(px(cells as f32 * cw), lh) }, b)); }
                    }
                    // The selection: whole cells, row by row.
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
                        let top = point(o.x, o.y + px(r as f32 * ch));
                        for g in &row.glyphs {
                            let at = top + g.at;
                            let _ = if g.emoji { window.paint_emoji(at, g.font, g.id, fsize) } else { window.paint_glyph(at, g.font, g.id, fsize, g.color) };
                        }
                        for &(col, cells, c, uy) in &row.unders {
                            window.paint_underline(point(top.x + px(col as f32 * cw), top.y + uy), px(cells as f32 * cw), &UnderlineStyle { thickness: px(1.), color: Some(c), wavy: false });
                        }
                    }
                });
                add_us(t0);
            },
        ).absolute().top_0().left_0().size_full();
        add_us(t0);
        div().id(SharedString::from(format!("term-{id}"))).track_focus(&self.focus).relative().size_full().overflow_hidden().bg(T::bg()).cursor_text()
            .child(grid)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let m = &e.keystroke.modifiers;
                let Some(text) = key_text(&e.keystroke.key, e.keystroke.key_char.as_deref(), m.control) else { return };
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
                if y > 0. { this.state.update(cx, |t, cx| t.scroll(T::WS_TERM_WHEEL, cx)); } else if y < 0. { this.state.update(cx, |t, cx| t.scroll(-T::WS_TERM_WHEEL, cx)); }
                cx.stop_propagation();
            }))
            // Scrolled up: the jump to the bottom (.term-scroll-bottom).
            .when(back > 0, |d| d.child(div().absolute().right(T::SP_12).bottom(T::SP_12)
                .child(w::icon_button(SharedString::from(format!("term-bottom-{id}")), IconName::ArrowDown, "Scroll to bottom").on_click(cx.listener(|this, _, _, cx| this.state.update(cx, |t, cx| t.bottom(cx)))))))
    }
}

// ─── The cursor (a cached leaf over the screen: its blink redraws it alone) ───

pub struct TermCursor {
    state: Entity<TermState>,
    focus: FocusHandle,
    blink_on: bool,
    _blink: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl TermCursor {
    fn new(state: Entity<TermState>, focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subs = vec![
            cx.observe(&state, |this: &mut Self, _, cx| { this.blink_on = true; cx.notify(); }),
            // While the terminal has the keys the cursor blinks (rule 6: only
            // then; a blur stops it).
            cx.on_focus(&focus, window, |this, _, cx| { this.blink_on = true; this.blink(cx); cx.notify(); }),
            cx.on_blur(&focus, window, |this, _, cx| { this._blink = None; this.blink_on = true; cx.notify(); }),
        ];
        TermCursor { state, focus, blink_on: true, _blink: None, _subs: subs }
    }

    fn blink(&mut self, cx: &mut Context<Self>) {
        self._blink = Some(cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| loop {
            cx.background_executor().timer(Duration::from_millis(T::WS_TERM_BLINK_MS)).await;
            if this.update(cx, |c, cx| { c.blink_on = !c.blink_on; cx.notify(); }).is_err() { return; }
        }));
    }
}

impl Render for TermCursor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("term-cursor");
        let st = self.state.read(cx);
        let root = div().size_full();
        if !st.term.cursor_on || st.back > 0 { return root; }
        let (cw, ch) = st.cell.get();
        let (x, y) = st.term.cursor();
        let pad = T::SP_4;
        let focused = self.focus.is_focused(window);
        // A block while the terminal has the keys, a frame when not.
        let c = div().absolute().top(pad + px(y as f32 * ch)).left(pad + px(x as f32 * cw)).w(px(cw)).h(px(ch));
        root.child(if focused { c.when(self.blink_on, |d| d.bg(T::accent_edge())) } else { c.border_1().border_color(T::accent()) })
    }
}

// ─── The input bar (a cached leaf; its height measured, as the composer's) ───

/// An upload under way (the strip over the input).
#[derive(Default, Clone)]
struct Up { on: bool, busy: bool, name: String, label: String, frac: f32, hide_gen: u64 }

pub struct TermInput {
    state: Entity<TermState>,
    id: String,
    zoom: i32,
    tools: bool,
    pub input: Entity<w::TextareaState>,
    up: Up,
    pub height: Pixels,
    _subs: Vec<Subscription>,
}

impl TermInput {
    fn new(state: Entity<TermState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(1, 5).placeholder("Type here... (Ctrl+Enter or Send button)"));
        let subs = vec![
            // Only the text size redraws the bar (never the output).
            cx.observe(&state, |this: &mut Self, s, cx| { let z = s.read(cx).zoom; if z != this.zoom { this.zoom = z; cx.notify(); } }),
            cx.subscribe_in(&input, window, |this, st, ev: &w::InputEvent, window, cx| {
                if let w::InputEvent::PressEnter { secondary: true, .. } = ev {
                    let t = st.read(cx).value().to_string();
                    this.send(t.trim_end_matches('\n'), window, cx);
                }
            }),
        ];
        let mut subs = subs;
        let focus = input.focus_handle(cx);
        subs.extend(w::watch_focus(&focus, window, cx));
        let (id, zoom) = { let s = state.read(cx); (s.id.clone(), s.zoom) };
        TermInput { state, id, zoom, tools: false, input, up: Up::default(), height: px(48.), _subs: subs }
    }

    pub fn text(&self, cx: &App) -> String { self.input.read(cx).value().to_string() }

    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (t, n) = (text.to_string(), text.len());
        self.input.update(cx, |st, cx| { st.set_value(t, window, cx); st.set_selected_range(n..n, cx); });
    }

    /// Send (and Ctrl+Enter): the text to the terminal, the box empties.
    pub fn send(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if text.is_empty() { return; }
        let t = text.to_string();
        self.state.update(cx, |s, cx| s.send_text(&t, cx));
        self.set_text("", window, cx);
    }

    pub fn toggle_tools(&mut self, cx: &mut Context<Self>) { self.tools = !self.tools; cx.notify(); }
    pub fn tools_open(&self) -> bool { self.tools }
    pub fn uploading(&self) -> bool { self.up.on }

    /// Text into the box (insertIntoTermInput): at the caret when the box
    /// has the keys, else at its end; spaced from what is there; the box
    /// takes the keys.
    pub fn insert(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.input.focus_handle(cx).is_focused(window);
        let old = self.text(cx);
        let at = if focused { self.input.read(cx).selected_range().start } else { old.len() };
        let (new, caret) = insert_at(&old, at, text);
        self.input.update(cx, |st, cx| { st.set_value(new, window, cx); st.set_selected_range(caret..caret, cx); st.focus(window, cx); });
        cx.notify();
    }

    /// Attach: the system's picker, then the file uploads.
    pub fn attach(&mut self, cx: &mut Context<Self>) {
        if self.up.busy { return; }
        if crundi_shell::project(cx).is_empty() { return; }
        let me = cx.entity().downgrade();
        crundi_shell::run(cx, || crundi_core::driver::take_pick().or_else(|| rfd::FileDialog::new().pick_file()), move |p, cx| {
            let Some(path) = p else { return };
            let _ = me.update(cx, |t, cx| t.upload_path(&path, cx));
        });
    }

    /// A file from this machine, uploaded.
    pub fn upload_path(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
        match std::fs::read(path) { Ok(b) => self.upload(name, b, cx), Err(e) => crate::toast(cx, format!("Upload failed: {e}"), w::Toast::Error) }
    }

    /// Files dropped on the terminal (the web's .terminal-wrap drop): on the
    /// machine Crundi runs on, their own paths go into the box ("File added",
    /// "N files added"); otherwise each uploads. `upload_anyway`: as from
    /// another machine (the driver's).
    pub fn drop_files(&mut self, paths: Vec<std::path::PathBuf>, upload_anyway: bool, window: &mut Window, cx: &mut Context<Self>) {
        if paths.is_empty() || crundi_shell::project(cx).is_empty() { return; }
        let local = crundi_shell::api(cx).is_some_and(|a| rules::is_local(a.base()));
        if local && !upload_anyway {
            let joined = paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(" ");
            self.insert(&joined, window, cx);
            crate::toast(cx, rules::added_toast(paths.len()), w::Toast::Info);
        } else {
            for p in paths { self.upload_path(&p, cx); }
        }
    }

    /// The box's paste: a picture uploads (uploadAttachment, as image.png); a
    /// copied file's path goes in; anything else is the ordinary paste.
    fn paste_handler(cx: &mut Context<Self>) -> impl Fn(&ClipboardItem, &mut Window, &mut App) -> bool + 'static {
        let me = cx.entity().downgrade();
        move |item, window, cx| {
            let Some(t) = me.upgrade() else { return false };
            if crundi_shell::project(cx).is_empty() { return false; }
            if let Some(img) = clip_image(item) { t.update(cx, |t, cx| t.upload("image.png".into(), img, cx)); return true; }
            if item.text().is_none_or(|x| x.is_empty()) {
                let paths: Vec<String> = item.entries().iter().filter_map(|e| if let ClipboardEntry::ExternalPaths(p) = e { Some(p.paths().iter().map(|x| x.to_string_lossy().to_string()).collect::<Vec<_>>()) } else { None }).flatten().collect();
                if !paths.is_empty() { t.update(cx, |t, cx| t.insert(&paths.join(" "), window, cx)); return true; }
            }
            false
        }
    }

    /// An upload to /api/attachments/upload in the project (the strip shows
    /// its progress), its path into the box, "Attached: <name>".
    pub fn upload(&mut self, name: String, bytes: Vec<u8>, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        if project.is_empty() { return; }
        let me = cx.entity().downgrade();
        let Some(api) = crundi_shell::api(cx) else { return };
        self.up = Up { on: true, busy: true, name: name.clone(), label: "0%".into(), frac: 0.0, hide_gen: self.up.hide_gen + 1 };
        cx.notify();
        // The strip follows the body going out (10 times a second, only while it goes).
        let prog = Arc::new(Mutex::new((0u64, 0u64, false)));
        {
            let (prog, me) = (prog.clone(), me.clone());
            cx.spawn(async move |_, cx: &mut AsyncApp| loop {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let (sent, total, done) = *prog.lock().unwrap();
                if done { break; }
                let _ = me.update(cx, |t, cx| {
                    let label = rules::progress_label(sent, total);
                    if label != t.up.label { t.up.label = label; t.up.frac = if total == 0 { 0.0 } else { sent as f32 / total as f32 }; cx.notify(); }
                });
            }).detach();
        }
        let n2 = name.clone();
        crundi_shell::run(cx, move || {
            use base64::Engine;
            let body = serde_json::to_vec(&json!({ "project": project, "name": n2, "data": base64::engine::general_purpose::STANDARD.encode(&bytes) })).unwrap_or_default();
            let p2 = prog.clone();
            let r = api.post_with_progress("/api/attachments/upload", &body, &move |sent: u64, total: u64| { let mut g = p2.lock().unwrap(); g.0 = sent; g.1 = total; });
            prog.lock().unwrap().2 = true;
            r
        }, move |r, cx| {
            let answer = match r {
                Err(crundi_core::api::ApiError::SignedOut) => { let s = crundi_shell::stores(cx); s.session.update(cx, |s, cx| s.sign_out("Your sign-in ended. Sign in again.", cx)); return; }
                Err(_) => rules::upload_answer(0, "", &name),
                Ok((status, body)) => rules::upload_answer(status, &body, &name),
            };
            let path = answer.as_ref().ok().map(|(p, _)| p.clone());
            let g = me.update(cx, |t, cx| { t.up.busy = false; cx.notify(); t.up.hide_gen }).ok();
            if let Some(p) = path {
                let me = me.clone();
                crate::with_window(cx, move |window, cx| { let _ = me.update(cx, |t, cx| t.insert(&p, window, cx)); });
            }
            match answer { Ok((_, n)) => crate::toast(cx, format!("Attached: {n}"), w::Toast::Info), Err(e) => crate::toast(cx, e, w::Toast::Error) }
            // The strip stays a moment, then goes.
            if let Some(g) = g {
                let me = me.clone();
                cx.spawn(async move |cx: &mut AsyncApp| {
                    cx.background_executor().timer(Duration::from_millis(350)).await;
                    let _ = me.update(cx, |t, cx| if t.up.hide_gen == g && !t.up.busy { t.up.on = false; cx.notify(); });
                }).detach();
            }
        });
    }

    /// The key tools (.term-tool-panel): Esc / Tab / ⇧Tab in a column;
    /// the Ctrl keys two a row and Space across both; Home and End over the
    /// arrows round Enter, at the right. Narrow, the groups wrap.
    fn tools_view(&self, cx: &mut Context<Self>) -> Div {
        let id = self.id.clone();
        let st = self.state.clone();
        let _ = cx;
        let key = |k: &'static str, label: &'static str, tip: &'static str| -> Stateful<Div> {
            let st = st.clone();
            let b = w::button(SharedString::from(format!("tk-{k}-{id}")), label).w_full().on_click(move |_, _, cx| st.update(cx, |t, cx| t.tool(k, cx)));
            let b = if tip.is_empty() { b } else { b.tooltip(tip) };
            div().id(SharedString::from(format!("tkc-{k}-{id}"))).min_w(T::WS_TERM_TOOL_W).child(b)
        };
        let kb = |k: &'static str, label: &'static str| key(k, label, "");
        let col = || div().flex().flex_col().gap(T::SP_4);
        let row = || div().flex().gap(T::SP_4);
        let cell = || div().flex_none().w(T::WS_TERM_NAV_W);
        div().mx(T::SP_8).my(T::SP_4).p(T::SP_10).rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg_secondary()).shadow_lg()
            .flex().flex_wrap().items_start().justify_between().gap(T::SP_16)
            .child(col().child(kb("Escape", "Esc")).child(kb("Tab", "Tab")).child(key("shift+tab", "⇧Tab", "Shift+Tab")))
            .child(col()
                .child(row().child(kb("ctrl+c", "Ctrl+C")).child(kb("ctrl+z", "Ctrl+Z")))
                .child(row().child(kb("ctrl+a", "Ctrl+A")).child(kb("ctrl+e", "Ctrl+E")))
                .child(row().child(kb("ctrl+d", "Ctrl+D")).child(kb("ctrl+l", "Ctrl+L")))
                .child(kb("Space", "Space")))
            .child(col().ml_auto()
                .child(row().child(div().flex_none().w(T::WS_TERM_NAV_W * 2. + T::SP_4).child(kb("Home", "Home"))).child(cell().child(kb("End", "End"))))
                .child(row().child(cell()).child(cell().child(kb("ArrowUp", "▲"))).child(cell()))
                .child(row().child(cell().child(kb("ArrowLeft", "◀"))).child(cell().child(kb("Enter", "Enter"))).child(cell().child(kb("ArrowRight", "▶"))))
                .child(row().child(cell()).child(cell().child(kb("ArrowDown", "▼"))).child(cell())))
    }

    fn upload_strip(&self) -> Option<Div> {
        let u = &self.up;
        if !u.on { return None; }
        let frac = u.frac.clamp(0.0, 1.0);
        Some(div().flex().items_center().gap(T::SP_8).px(T::SP_2).pb(T::SP_6).text_size(T::TEXT_11).text_color(T::text_secondary())
            .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(u.name.clone()))
            .child(div().flex_1().min_w_0().child(w::meter(frac, T::accent(), T::SP_6)))
            .child(div().flex_none().font_family(T::MONO).text_color(T::text_muted()).child(u.label.clone())))
    }
}

impl Render for TermInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("term-input");
        let id = self.id.clone();
        let fs = scale(self.zoom);
        let me = cx.entity();
        let measure = canvas(move |b, _, cx| {
            let h = b.size.height;
            // (After the frame: a notify while drawing would not draw again.)
            if (me.read(cx).height - h).abs() > px(0.5) { let me = me.clone(); cx.defer(move |cx| me.update(cx, |c, cx| { c.height = h; cx.notify(); })); }
        }, |_, _, _, _| {}).absolute().top_0().left_0().size_full();
        // The input bar (.term-input-bar): tools, the box, attach, Send.
        let bar = div().flex_none().flex().flex_col().px(T::SP_8).py(T::SP_6).bg(T::bg_secondary()).border_t_1().border_color(T::border())
            .when_some(self.upload_strip(), |d, s| d.child(s))
            // The buttons each in a paint layer (w::layer: the bar is replayed
            // every frame); the box only while not focused (w::text_box: its caret).
            .child(div().flex().items_end().gap(T::SP_6)
                .child(w::layer(w::square_button(SharedString::from(format!("term-tools-{id}")), IconName::Settings, "Keyboard tools", if self.tools { w::SquareLook::On } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_tools(cx)))))
                .child(div().flex_1().min_w_0().max_h(T::WS_TERM_INPUT_MAX).child({
                    // The box in a paint layer only while it does not have the keys. Focused
                    // from code (a drop, an upload), it is drawn as itself at once: in its
                    // layer GPUI never sees it focused, and its blur (which stops the caret's
                    // blink) would never come.
                    let f = self.input.focus_handle(cx);
                    let el = w::message_box(&self.input, fs).on_paste(Self::paste_handler(cx));
                    if f.is_focused(window) { el.into_any_element() } else { w::text_box(&f, el, window, cx) }
                }))
                .child(w::layer(w::square_button(SharedString::from(format!("term-attach-{id}")), IconName::Paperclip, "Attach a file (uploads to crundi_attachments)", if self.up.busy { w::SquareLook::Busy } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
                    .when(!self.up.busy, |d| d.on_click(cx.listener(|this, _, _, cx| this.attach(cx))))))
                .child(w::layer(w::send_button(SharedString::from(format!("term-send-{id}")), "Send", 1.0).tooltip("Send to terminal")
                    .on_click(cx.listener(|this, _, window, cx| { let t = this.text(cx); this.send(&t, window, cx); })))));
        let me = cx.entity().downgrade();
        // A dragged reference: into this terminal's input (insertIntoTermInput).
        w::ref_target(div().id(SharedString::from(format!("term-drop-{id}"))), move |text, window, cx| {
            let _ = me.update(cx, |t, cx| t.insert(text, window, cx));
        })
            .relative().w_full().flex().flex_col().bg(T::bg())
            .when(self.tools, |d| d.child(self.tools_view(cx)))
            .child(bar)
            .child(measure)
    }
}

// ─── The panes ───

/// One terminal's entities.
#[derive(Clone)]
pub struct TermParts {
    pub state: Entity<TermState>,
    pub screen: Entity<TermScreen>,
    pub cursor: Entity<TermCursor>,
    pub input: Entity<TermInput>,
}

impl TermParts {
    /// The pane's body (the workbench's PaneParts::body): screen and cursor
    /// over the input bar, each cached (the bar at its measured height).
    pub fn body(&self, cx: &App) -> AnyElement {
        let h = self.input.read(cx).height;
        let id = self.state.read(cx).id.clone();
        // A dragged reference dropped on the screen goes to the program (the
        // web's insertRefToTarget 'term': the reference and a space; several
        // lines as one bracketed paste), and the terminal takes the keys.
        let screen = w::ref_target_drag(div().id(SharedString::from(format!("term-screen-drop-{id}"))), {
            let (id, focus) = (id.clone(), self.screen.read(cx).focus.clone());
            move |d, window, cx| {
                let r = d.text.to_string();
                let data = if r.contains('\n') { format!("\x1b[200~{r}\x1b[201~") } else { format!("{r} ") };
                send(cx, &id, &data);
                focus.focus(window, cx);
                crate::toast(cx, "Sent to terminal".into(), w::Toast::Info);
            }
        });
        // Files from the system dropped anywhere on the terminal: into its box.
        let input = self.input.clone();
        div().id(SharedString::from(format!("term-files-{id}"))).flex().flex_col().size_full().min_h_0().bg(T::bg())
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_2().border_color(T::accent()))
            .on_drop(move |p: &ExternalPaths, window, cx| { let v = p.paths().to_vec(); input.update(cx, |t, cx| t.drop_files(v, false, window, cx)); cx.stop_propagation(); })
            .child(screen.relative().w_full().flex_1().min_h_0().overflow_hidden()
                .child(self.screen.clone().cached(StyleRefinement::default().size_full()))
                .child(self.cursor.clone().cached(StyleRefinement::default().absolute().top_0().left_0().size_full())))
            .child(div().w_full().h(h).flex_none().child(self.input.clone().cached(StyleRefinement::default().w_full().h(h))))
            .into_any_element()
    }
}

/// Every terminal in a pane, by id (a handle registry: read in handlers),
/// and the ones closed here (the server may list them a while longer: no
/// pane comes back for them).
#[derive(Default)]
pub struct Terms(pub HashMap<String, TermParts>, pub std::collections::HashSet<String>);
impl Global for Terms {}

pub fn find(cx: &App, id: &str) -> Option<TermParts> { cx.try_global::<Terms>()?.0.get(id).cloned() }

/// The terminal's parts, made (and subscribed) the first time.
pub fn ensure(id: &str, window: &mut Window, cx: &mut App) -> TermParts {
    if let Some(p) = find(cx, id) { return p; }
    let state = cx.new(|_| TermState::new(id));
    let screen = cx.new(|cx| TermScreen::new(state.clone(), cx));
    let focus = screen.read(cx).focus.clone();
    let cursor = cx.new(|cx| TermCursor::new(state.clone(), focus, window, cx));
    let input = cx.new(|cx| TermInput::new(state.clone(), window, cx));
    let p = TermParts { state, screen, cursor, input };
    cx.default_global::<Terms>().0.insert(id.to_string(), p.clone());
    crundi_shell::ws_send(cx, &json!({ "type": "subscribe", "id": id }));
    p
}

fn drop_term(cx: &mut App, id: &str) {
    if cx.default_global::<Terms>().0.remove(id).is_some() { crundi_shell::ws_send(cx, &json!({ "type": "unsubscribe", "id": id })); }
}

pub fn clear(cx: &mut App) {
    let ids: Vec<String> = cx.default_global::<Terms>().0.keys().cloned().collect();
    for id in ids { drop_term(cx, &id); }
}

/// The open project's Claude terminals (the event stream's terminals of
/// kind "terminal").
fn listed(stores: &crundi_shell::Stores, cx: &App) -> Vec<Value> {
    let p = stores.projects.read(cx);
    p.terminals.iter().filter(|t| t.get("kind").and_then(|k| k.as_str()) == Some("terminal") && t.get("project").and_then(|x| x.as_str()) == Some(p.open.as_str())).cloned().collect()
}

/// A pane each, as the chats; panes of ones gone close, and their
/// terminals are let go.
pub fn sync(stores: &crundi_shell::Stores, cx: &mut App) {
    if stores.projects.read(cx).open.is_empty() { return; }
    let Some(wb) = crundi_workbench::workbench(cx) else { return };
    let closed = cx.default_global::<Terms>().1.clone();
    let ids: Vec<String> = listed(stores, cx).iter().filter_map(|t| t.get("id").and_then(|i| i.as_str()).map(String::from)).filter(|i| !closed.contains(i)).collect();
    let keys = wb.update(cx, |w, cx| {
        let keep = ids.clone();
        w.retain(move |k| !k.starts_with("term:") || keep.iter().any(|i| *k == key_of(i)), cx);
        for id in &ids { w.place(&key_of(id), cx); }
        w.keys()
    });
    let gone: Vec<String> = cx.default_global::<Terms>().0.keys().filter(|id| !keys.contains(&key_of(id))).cloned().collect();
    for id in gone { drop_term(cx, &id); }
}

/// A frame's `output` messages joined per terminal, in order.
pub fn join_output(batch: &[crundi_core::live::Event]) -> Vec<(String, String)> {
    let mut by: Vec<(String, String)> = vec![];
    for e in batch {
        let crundi_core::live::Event::Ws(t, v) = e else { continue };
        if t != "output" { continue; }
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
        let data = v.get("data").and_then(|x| x.as_str()).unwrap_or("");
        match by.iter_mut().find(|(i, _)| i == id) { Some((_, d)) => d.push_str(data), None => by.push((id.to_string(), data.to_string())) }
    }
    by
}

/// The live hub's frame: each terminal's output joined and fed once (one
/// notify per terminal per frame, however fast it prints).
pub fn route(batch: &[crundi_core::live::Event], cx: &mut App) {
    for (id, data) in join_output(batch) {
        if let Some(p) = find(cx, &id) { p.state.update(cx, |t, cx| t.output(&data, cx)); }
    }
}

pub fn resubscribe(cx: &mut App) {
    let all: Vec<TermParts> = cx.try_global::<Terms>().map(|t| t.0.values().cloned().collect()).unwrap_or_default();
    for p in all { p.state.update(cx, |t, cx| t.resubscribe(cx)); }
}

/// The head: its title from the event stream's list (a rename reaches every
/// client there), its state.
pub fn head_info(stores: &crundi_shell::Stores, id: &str, cx: &App) -> crundi_workbench::HeadInfo {
    let p = stores.projects.read(cx);
    let item = p.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(id));
    let s = |k: &str| item.and_then(|c| c.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut title = s("title");
    if title.is_empty() { title = "Terminal".into(); }
    let st = if s("status") == "exited" { "exited".to_string() } else { s("state") };
    crundi_workbench::HeadInfo { title, long: String::new(), state: if st.is_empty() { "idle".into() } else { st } }
}

/// Start a terminal in a launcher's pane (the launcher's Empty Shell,
/// Terminal and its Skip Permissions): Terminal asks the resume picker
/// first, as the web's openSessionChooser.
pub fn launch(key: &str, mode: &str, cx: &mut App) {
    let project = crundi_shell::project(cx);
    if project.is_empty() { return crate::toast(cx, "Select a project first".into(), w::Toast::Error); }
    let key = key.to_string();
    if mode == "shell" { return create(&key, false, true, "", "", cx); }
    let skip = mode == "skip";
    if let Some(choose) = crundi_workbench::launcher::chooser(cx) {
        let k = key.clone();
        let go: Rc<dyn Fn(&'static str, String, &mut App)> = Rc::new(move |m, id, cx| create(&k, skip, false, m, &id, cx));
        return choose(&project, skip, true, cx, go);
    }
    create(&key, skip, false, "new", "", cx);
}

/// POST /api/terminals/create; the terminal takes the launcher's place;
/// refused or failed, a toast.
fn create(key: &str, skip: bool, shell: bool, mode: &str, resume_id: &str, cx: &mut App) {
    let project = crundi_shell::project(cx);
    let mut body = json!({ "project": project, "skipPermissions": skip, "shell": shell });
    if !shell && !mode.is_empty() { body["sessionMode"] = json!(mode); body["resumeId"] = json!(resume_id); }
    let key = key.to_string();
    // The pressed button is busy until the server answers. (Deferred: a
    // click on it runs this inside the launcher's own update.)
    let (k, busy) = (key.clone(), if shell { 3 } else if skip { 5 } else { 4 });
    cx.defer(move |cx| crundi_workbench::launcher::set_busy(cx, &k, busy));
    crundi_shell::call(cx, move |api| api.post("/api/terminals/create", &body), move |r, cx| {
        crundi_workbench::launcher::set_busy(cx, &key, 0);
        let v = match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => {
                let e = v.get("error").and_then(|e| e.as_str()).unwrap_or("").to_string();
                return crate::toast(cx, if e.is_empty() { "Failed to launch".into() } else { e }, w::Toast::Error);
            }
            Ok(v) => v,
            Err(e) => return crate::toast(cx, format!("Failed to launch terminal: {e}"), w::Toast::Error),
        };
        let Some(id) = v.get("id").and_then(|i| i.as_str()).map(String::from) else { return };
        if let Some(wb) = crundi_workbench::workbench(cx) { wb.update(cx, |w, cx| w.replace(&key, &key_of(&id), cx)); }
    });
}

/// × on a terminal's pane: the server closes it, then its pane goes.
pub fn close(key: &str, cx: &mut App, done: Box<dyn FnOnce(bool, &mut App)>) {
    let id = id_of(key);
    let path = format!("/api/terminals/{id}/close");
    crundi_shell::call(cx, move |api| api.post(&path, &json!({})), move |r, cx| match r {
        Ok(_) => { drop_term(cx, &id); cx.default_global::<Terms>().1.insert(id); done(true, cx) }
        Err(e) => { crate::toast(cx, format!("Failed to close terminal: {e}"), w::Toast::Error); done(false, cx) }
    });
}

/// The driver's terminal actions ("term-…"): the pane's key first.
pub fn act(name: &str, a: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let s = |i: usize| a.get(i).cloned().unwrap_or_default();
    let n = |i: usize| s(i).parse::<i64>().unwrap_or(0);
    let Some(p) = find(cx, &id_of(&s(0))) else { return Some(json!({ "ok": false, "error": "no such terminal" })) };
    let (st, input) = (p.state.clone(), p.input.clone());
    match name {
        "term-send" => { let t = s(1); input.update(cx, |x, cx| x.send(&t, window, cx)); }
        "term-input" => { let t = s(1); input.update(cx, |x, cx| x.set_text(&t, window, cx)); }
        "term-tool" => { let t = s(1); st.update(cx, |x, cx| x.tool(&t, cx)); }
        "term-tools" => input.update(cx, |x, cx| x.toggle_tools(cx)),
        "term-attach" => input.update(cx, |x, cx| x.attach(cx)),
        "term-bottom" => st.update(cx, |x, cx| x.bottom(cx)),
        "term-paste" => st.update(cx, |x, cx| x.paste(cx)),
        // A picture pasted on the screen (Ctrl+V) or in the box, from a file.
        "term-paste-image" | "term-input-paste-image" => {
            let b = match std::fs::read(s(1)) { Ok(b) => b, Err(e) => return Some(json!({ "ok": false, "error": e.to_string() })) };
            if name == "term-paste-image" { st.update(cx, |x, cx| x.paste_image(b, cx)); } else { input.update(cx, |x, cx| x.upload("image.png".into(), b, cx)); }
        }
        // Files from the system dropped on the terminal ("upload" first: as from another machine).
        "term-drop-files" => {
            let v: Vec<String> = a.iter().skip(1).filter(|x| !x.is_empty()).cloned().collect();
            let force = v.first().is_some_and(|x| x == "upload");
            let paths: Vec<std::path::PathBuf> = v.into_iter().filter(|x| x != "upload").map(Into::into).collect();
            input.update(cx, |x, cx| x.drop_files(paths, force, window, cx));
        }
        // The pane's × (twice): the server closes the terminal, the pane goes.
        "term-close" => {
            let key = s(0);
            let k2 = key.clone();
            close(&k2, cx, Box::new(move |ok, cx| if ok { if let Some(w) = crundi_workbench::workbench(cx) { w.update(cx, |w, cx| w.retain(|k| k != key, cx)); } }));
        }
        // The pane's title renamed (/api/terminals/:id/rename): every client
        // sees it through the event stream's list, the head with it.
        "term-rename" => {
            let (id, title) = (id_of(&s(0)), s(1));
            crundi_shell::call(cx, move |api| api.post(&format!("/api/terminals/{id}/rename"), &json!({ "title": title })), |r, cx| if let Err(e) = r { crate::toast(cx, e, w::Toast::Error) });
        }
        "term-focus" => { let f = p.screen.read(cx).focus.clone(); f.focus(window, cx); }
        // The input bar's box takes the keys (a click in it).
        "term-input-focus" => { let b = input.read(cx).input.clone(); b.update(cx, |x, cx| x.focus(window, cx)); }
        // The keys taken away from the terminal and its box (a click elsewhere).
        "term-blur" => window.blur(cx),
        "term-select" => st.update(cx, |x, cx| x.select(n(1).max(0) as usize, n(2).max(0) as usize, n(3).max(0) as usize, n(4).max(0) as usize, cx)),
        "term-scroll" => { let l = n(1) as i32; st.update(cx, |x, cx| x.scroll(l, cx)); }
        "term-zoom" => { let d = n(1) as i32; st.update(cx, |x, cx| x.set_zoom(d, cx)); }
        "term-key" => {
            let (t, m) = (s(1), s(2));
            let took = st.update(cx, |x, cx| x.key(&t, m == "ctrl", m == "alt", m == "shift", cx));
            return Some(json!({ "ok": took }));
        }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

pub fn props(cx: &App) -> Value {
    Value::Array(cx.try_global::<Terms>().map(|t| t.0.values().map(|p| {
        let mut v = p.state.read(cx).props();
        let i = p.input.read(cx);
        v["text"] = json!(i.text(cx));
        v["tools"] = json!(i.tools_open());
        v["upload"] = json!(i.uploading());
        v["inputH"] = json!(f32::from(i.height));
        // The rows the screen derived and shaped so far (a batch: the lines it wrote).
        v["derived"] = json!(p.screen.read(cx).cache.derived);
        v["screenUs"] = json!(SCREEN_US.load(std::sync::atomic::Ordering::Relaxed));
        v["screenPaints"] = json!(SCREEN_PAINTS.load(std::sync::atomic::Ordering::Relaxed));
        v
    }).collect()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{join_output, key_text, palette, resolve, rgb3, selection_text, split_runs, tool_bytes, with_path, TermState};
    use crate::vt;
    use crundi_core::live::Event;
    use gpui::{AppContext as _, Hsla};
    use serde_json::json;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn a_frames_output_is_joined() {
        let mut batch: Vec<Event> = (0..200).map(|i| Event::Ws("output".into(), json!({ "id": "a", "data": format!("{i}\r\n") }))).collect();
        batch.insert(5, Event::Ws("output".into(), json!({ "id": "b", "data": "x" })));
        batch.push(Event::Ws("other".into(), json!({ "id": "a", "data": "no" })));
        let j = join_output(&batch);
        assert_eq!(j.len(), 2);
        assert_eq!(j[0].0, "a");
        assert!(j[0].1.starts_with("0\r\n1\r\n") && j[0].1.ends_with("199\r\n") && !j[0].1.contains("no"));
        assert_eq!(j[1], ("b".into(), "x".into()));
    }

    /// A frame's output (200 lines) is one feed and one notify of the
    /// terminal's state: its views redraw once.
    #[gpui::test]
    fn a_frames_output_is_one_update(cx: &mut gpui::TestAppContext) {
        let st = cx.new(|_| TermState::new("t"));
        let n = Rc::new(Cell::new(0));
        let n2 = n.clone();
        let _sub = cx.update(|cx| cx.observe(&st, move |_, _| n2.set(n2.get() + 1)));
        let data: String = (1..=200).map(|i| format!("{i}\r\n")).collect();
        st.update(cx, |t, cx| t.output(&data, cx));
        cx.run_until_parked();
        assert_eq!(n.get(), 1);
        st.read_with(cx, |t, _| { assert_eq!(t.batches, 1); assert!(t.term.scrollback.len() > 150); });
    }

    #[test]
    fn colours() {
        let named: Vec<Hsla> = (0..16).map(|i| rgb3(i, i, i)).collect();
        assert_eq!(palette(3, &named), rgb3(3, 3, 3));
        assert_eq!(palette(16, &named), rgb3(0, 0, 0));
        assert_eq!(palette(231, &named), rgb3(255, 255, 255));
        assert_eq!(palette(232, &named), rgb3(8, 8, 8));
        assert_eq!(resolve(vt::Color::Idx(1), Hsla::default(), true, &named), rgb3(9, 9, 9));
    }

    #[test]
    fn splits_wide() {
        let mut t = vt::Term::new(10, 1);
        t.feed("ab●cd");
        let r = split_runs(&t.view(0)[0]);
        assert_eq!(r.iter().map(|r| (r.col, r.text.as_str())).collect::<Vec<_>>(), vec![(0, "ab"), (2, "●"), (3, "cd")]);
    }

    #[test]
    fn selections() {
        let mut t = vt::Term::new(10, 3);
        t.feed("hello\r\nworld  x\r\nend");
        let v = t.view(0);
        assert_eq!(selection_text(&v, 1, 0, 2, 0), "el");
        assert_eq!(selection_text(&v, 3, 0, 4, 1), "lo\nworld");
        assert_eq!(selection_text(&v, 0, 1, 9, 2), "world  x\nend");
    }

    #[test]
    fn input_bits() {
        assert_eq!(with_path("", "/a/b.png"), "/a/b.png ");
        assert_eq!(with_path("see", "/a"), "see /a ");
        assert_eq!(with_path("see ", "/a"), "see /a ");
        assert_eq!(tool_bytes("ctrl+c"), Some("\x03"));
        assert_eq!(tool_bytes("nope"), None);
    }

    #[test]
    fn gpui_keys() {
        assert_eq!(key_text("up", None, false).as_deref(), Some("\u{f700}"));
        assert_eq!(key_text("enter", None, false).as_deref(), Some("\n"));
        assert_eq!(key_text("a", Some("A"), false).as_deref(), Some("A"));
        assert_eq!(key_text("c", None, true).as_deref(), Some("c"));
        assert_eq!(key_text("f1", None, false).as_deref(), Some("\u{f704}"));
        assert_eq!(key_text("shift", None, false), None);
        assert_eq!(vt::key_bytes(&key_text("c", None, true).unwrap(), true, false, false, false).as_deref(), Some("\x03"));
    }
}
