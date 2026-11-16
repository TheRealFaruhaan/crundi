//! A terminal emulator for the Claude terminal panels (the web's xterm.js):
//! the VT/xterm control sequences a shell and Claude Code's interface use
//! (cursor moves, erase, scroll regions, insert and delete, the colours and
//! styles of SGR, the alternate screen, bracketed paste and the cursor-key
//! mode, the status reports), a screen of cells and a scrollback. Pure: the
//! panel feeds it the server's `output` and draws `view`.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Color { #[default] Default, Idx(u8), Rgb(u8, u8, u8) }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Style { pub fg: Color, pub bg: Color, pub bold: bool, pub dim: bool, pub italic: bool, pub underline: bool, pub inverse: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell { pub ch: char, pub st: Style, /// The right half of a wide character.
    pub cont: bool }

impl Default for Cell { fn default() -> Self { Cell { ch: ' ', st: Style::default(), cont: false } } }

#[derive(Clone, Debug, PartialEq, Eq)]
enum Mode { Ground, Esc, EscInter(char), Csi, Osc, OscEsc, Str, StrEsc }

#[derive(Clone)]
struct Screen { rows: Vec<Vec<Cell>>, cx: usize, cy: usize, saved: (usize, usize, Style), top: usize, bottom: usize }

pub struct Term {
    pub cols: usize,
    pub rows: usize,
    main: Screen,
    alt: Option<Screen>,
    pub scrollback: Vec<Vec<Cell>>,
    pub max_scrollback: usize,
    st: Style,
    mode: Mode,
    params: String,
    osc: String,
    /// The cursor shown (?25), the cursor keys' application mode (?1),
    /// bracketed paste (?2004), autowrap (?7).
    pub cursor_on: bool,
    pub app_cursor: bool,
    pub bracketed: bool,
    autowrap: bool,
    /// The next character wraps first (the last column was written).
    wrap_next: bool,
    /// Replies the terminal owes the program (status reports), to send as input.
    pub replies: Vec<String>,
    /// The window title (OSC 0 / 2).
    pub title: String,
    /// Counts the changes, for redrawing.
    pub generation: u64,
}

fn blank_row(cols: usize) -> Vec<Cell> { vec![Cell::default(); cols] }

/// A character's width in cells: 2 for East Asian wide and most emoji, 0 for
/// combining marks, else 1.
pub fn width(c: char) -> usize {
    let u = c as u32;
    if u == 0 { return 0; }
    if (0x300..=0x36f).contains(&u) || (0x200b..=0x200f).contains(&u) || (0xfe00..=0xfe0f).contains(&u) { return 0; }
    if (0x1100..=0x115f).contains(&u) || (0x2e80..=0xa4cf).contains(&u) || (0xac00..=0xd7a3).contains(&u)
        || (0xf900..=0xfaff).contains(&u) || (0xfe30..=0xfe4f).contains(&u) || (0xff00..=0xff60).contains(&u)
        || (0xffe0..=0xffe6).contains(&u) || (0x1f300..=0x1f64f).contains(&u) || (0x1f900..=0x1f9ff).contains(&u)
        || (0x1f680..=0x1f6ff).contains(&u) || (0x20000..=0x3fffd).contains(&u) { return 2; }
    1
}

impl Term {
    pub fn new(cols: usize, rows: usize) -> Term {
        let (cols, rows) = (cols.max(2), rows.max(1));
        let scr = Screen { rows: vec![blank_row(cols); rows], cx: 0, cy: 0, saved: (0, 0, Style::default()), top: 0, bottom: rows - 1 };
        Term { cols, rows, main: scr, alt: None, scrollback: vec![], max_scrollback: 5000, st: Style::default(), mode: Mode::Ground,
            params: String::new(), osc: String::new(), cursor_on: true, app_cursor: false, bracketed: false, autowrap: true, wrap_next: false,
            replies: vec![], title: String::new(), generation: 0 }
    }

    fn scr(&mut self) -> &mut Screen { if let Some(a) = self.alt.as_mut() { a } else { &mut self.main } }
    fn scr_ref(&self) -> &Screen { self.alt.as_ref().unwrap_or(&self.main) }
    pub fn is_alt(&self) -> bool { self.alt.is_some() }
    pub fn cursor(&self) -> (usize, usize) { let s = self.scr_ref(); (s.cx.min(self.cols - 1), s.cy) }

    /// Back to the start (a reconnect: the server sends the scrollback again).
    pub fn reset(&mut self) { let (c, r, m) = (self.cols, self.rows, self.max_scrollback); *self = Term::new(c, r); self.max_scrollback = m; self.generation += 1; }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (cols, rows) = (cols.max(2), rows.max(1));
        if cols == self.cols && rows == self.rows { return; }
        let old_rows = self.rows;
        for s in [Some(&mut self.main), self.alt.as_mut()].into_iter().flatten() {
            for r in s.rows.iter_mut() { r.resize(cols, Cell::default()); }
            // Shrinking: rows above the cursor go to the scrollback first (main only).
            while s.rows.len() > rows {
                if s.cy + 1 < s.rows.len() && s.rows.last().is_some_and(|r| r.iter().all(|c| c.ch == ' ')) { s.rows.pop(); }
                else { s.rows.remove(0); s.cy = s.cy.saturating_sub(1); }
            }
            while s.rows.len() < rows { s.rows.push(blank_row(cols)); }
            s.cx = s.cx.min(cols - 1); s.cy = s.cy.min(rows - 1);
            s.top = 0; s.bottom = rows - 1;
        }
        let _ = old_rows;
        for r in self.scrollback.iter_mut() { r.resize(cols, Cell::default()); }
        self.cols = cols; self.rows = rows; self.wrap_next = false;
        self.generation += 1;
    }

    pub fn feed(&mut self, s: &str) {
        for c in s.chars() { self.byte(c); }
        self.generation += 1;
    }

    fn byte(&mut self, c: char) {
        match self.mode.clone() {
            Mode::Ground => self.ground(c),
            Mode::Esc => self.esc(c),
            Mode::EscInter(_) => { self.mode = Mode::Ground; }
            Mode::Csi => {
                if ('\u{40}'..='\u{7e}').contains(&c) { let p = std::mem::take(&mut self.params); self.mode = Mode::Ground; self.csi(&p, c); }
                else if c == '\x1b' { self.params.clear(); self.mode = Mode::Esc; }
                else if c == '\x18' || c == '\x1a' { self.params.clear(); self.mode = Mode::Ground; }
                else if (c as u32) >= 0x20 { self.params.push(c); }
                else { self.ctrl(c); }
            }
            Mode::Osc => match c {
                '\x07' => { self.osc_done(); self.mode = Mode::Ground; }
                '\x1b' => self.mode = Mode::OscEsc,
                _ => { if self.osc.len() < 4096 { self.osc.push(c); } }
            },
            Mode::OscEsc => { self.osc_done(); self.mode = Mode::Ground; if c != '\\' { self.byte(c); } }
            Mode::Str => if c == '\x1b' { self.mode = Mode::StrEsc } else if c == '\x07' { self.mode = Mode::Ground },
            Mode::StrEsc => { self.mode = if c == '\\' { Mode::Ground } else { Mode::Str }; }
        }
    }

    fn osc_done(&mut self) {
        let s = std::mem::take(&mut self.osc);
        if let Some((n, t)) = s.split_once(';') { if n == "0" || n == "2" { self.title = t.to_string(); } }
    }

    fn ctrl(&mut self, c: char) {
        match c {
            '\x07' => {}
            '\x08' => { self.wrap_next = false; let s = self.scr(); s.cx = s.cx.saturating_sub(1); }
            '\t' => { let cols = self.cols; let s = self.scr(); s.cx = ((s.cx / 8 + 1) * 8).min(cols - 1); }
            '\n' | '\x0b' | '\x0c' => { self.wrap_next = false; self.linefeed(); }
            '\r' => { self.wrap_next = false; self.scr().cx = 0; }
            '\x1b' => { self.params.clear(); self.mode = Mode::Esc; }
            _ => {}
        }
    }

    fn ground(&mut self, c: char) {
        if (c as u32) < 0x20 || c == '\x7f' { return self.ctrl(c); }
        let w = width(c);
        let cols = self.cols;
        if w == 0 { return; }
        if self.wrap_next && self.autowrap { self.wrap_next = false; self.scr().cx = 0; self.linefeed(); }
        if w == 2 && self.scr_ref().cx + 1 >= cols { if self.autowrap { self.scr().cx = 0; self.linefeed(); } else { return; } }
        let st = self.st;
        let s = self.scr();
        let (x, y) = (s.cx.min(cols - 1), s.cy);
        s.rows[y][x] = Cell { ch: c, st, cont: false };
        if w == 2 && x + 1 < cols { s.rows[y][x + 1] = Cell { ch: ' ', st, cont: true }; }
        if x + w >= cols { s.cx = cols - 1; self.wrap_next = true; } else { s.cx = x + w; }
    }

    fn linefeed(&mut self) {
        let s = self.scr_ref();
        if s.cy == s.bottom { self.scroll_up(1); } else if s.cy + 1 < self.rows { self.scr().cy += 1; }
    }

    fn scroll_up(&mut self, n: usize) {
        let cols = self.cols;
        let alt = self.alt.is_some();
        let (top, bottom) = { let s = self.scr_ref(); (s.top, s.bottom) };
        for _ in 0..n {
            let s = self.scr();
            let row = s.rows.remove(top);
            s.rows.insert(bottom, blank_row(cols));
            if !alt && top == 0 {
                self.scrollback.push(row);
                if self.scrollback.len() > self.max_scrollback { let over = self.scrollback.len() - self.max_scrollback; self.scrollback.drain(..over); }
            }
        }
    }

    fn scroll_down(&mut self, n: usize) {
        let cols = self.cols;
        let s = self.scr();
        for _ in 0..n { s.rows.remove(s.bottom); s.rows.insert(s.top, blank_row(cols)); }
    }

    fn esc(&mut self, c: char) {
        self.mode = Mode::Ground;
        match c {
            '[' => { self.params.clear(); self.mode = Mode::Csi; }
            ']' => { self.osc.clear(); self.mode = Mode::Osc; }
            'P' | 'X' | '^' | '_' => self.mode = Mode::Str,
            '(' | ')' | '*' | '+' | '#' | '%' => self.mode = Mode::EscInter(c),
            '7' => { let st = self.st; let s = self.scr(); s.saved = (s.cx, s.cy, st); }
            '8' => { let (x, y, st) = self.scr_ref().saved; self.st = st; let s = self.scr(); s.cx = x; s.cy = y; self.wrap_next = false; }
            'D' => self.linefeed(),
            'E' => { self.scr().cx = 0; self.linefeed(); }
            'M' => { let s = self.scr_ref(); if s.cy == s.top { self.scroll_down(1); } else { let s = self.scr(); s.cy = s.cy.saturating_sub(1); } }
            'c' => self.reset(),
            _ => {}
        }
    }

    fn erase(&mut self, y: usize, from: usize, to: usize) {
        let st = Style { bg: self.st.bg, ..Style::default() };
        let s = self.scr();
        let to = to.min(s.rows[y].len());
        for x in from.min(to)..to { s.rows[y][x] = Cell { ch: ' ', st, cont: false }; }
    }

    fn csi(&mut self, p: &str, f: char) {
        let private = p.starts_with('?');
        let gt = p.starts_with('>');
        let body = p.trim_start_matches(['?', '>', '=']);
        let args: Vec<usize> = body.split(';').map(|x| x.split(':').next().unwrap_or("").parse().unwrap_or(0)).collect();
        let a = |i: usize, d: usize| -> usize { match args.get(i) { Some(0) | None => d, Some(v) => *v } };
        let (cols, rows) = (self.cols, self.rows);
        // A colour change keeps a pending wrap (xterm's): a prompt's colour
        // code after a full last column must not overwrite it.
        if f != 'm' { self.wrap_next = false; }
        match f {
            'A' => { let n = a(0, 1); let s = self.scr(); let lim = if s.cy >= s.top { s.top } else { 0 }; s.cy = s.cy.saturating_sub(n).max(lim); }
            'B' | 'e' => { let n = a(0, 1); let s = self.scr(); let lim = if s.cy <= s.bottom { s.bottom } else { rows - 1 }; s.cy = (s.cy + n).min(lim); }
            'C' | 'a' => { let n = a(0, 1); let s = self.scr(); s.cx = (s.cx + n).min(cols - 1); }
            'D' => { let n = a(0, 1); let s = self.scr(); s.cx = s.cx.min(cols - 1).saturating_sub(n); }
            'E' => { let n = a(0, 1); let s = self.scr(); s.cy = (s.cy + n).min(rows - 1); s.cx = 0; }
            'F' => { let n = a(0, 1); let s = self.scr(); s.cy = s.cy.saturating_sub(n); s.cx = 0; }
            'G' | '`' => { let n = a(0, 1); self.scr().cx = (n - 1).min(cols - 1); }
            'd' => { let n = a(0, 1); self.scr().cy = (n - 1).min(rows - 1); }
            'H' | 'f' => { let (y, x) = (a(0, 1), a(1, 1)); let s = self.scr(); s.cy = (y - 1).min(rows - 1); s.cx = (x - 1).min(cols - 1); }
            'J' => {
                let (cx, cy) = { let s = self.scr_ref(); (s.cx.min(cols - 1), s.cy) };
                match args.first().copied().unwrap_or(0) {
                    0 => { self.erase(cy, cx, cols); for y in cy + 1..rows { self.erase(y, 0, cols); } }
                    1 => { for y in 0..cy { self.erase(y, 0, cols); } self.erase(cy, 0, cx + 1); }
                    2 => { for y in 0..rows { self.erase(y, 0, cols); } }
                    3 => { self.scrollback.clear(); }
                    _ => {}
                }
            }
            'K' => {
                let (cx, cy) = { let s = self.scr_ref(); (s.cx.min(cols - 1), s.cy) };
                match args.first().copied().unwrap_or(0) { 0 => self.erase(cy, cx, cols), 1 => self.erase(cy, 0, cx + 1), 2 => self.erase(cy, 0, cols), _ => {} }
            }
            'X' => { let n = a(0, 1); let (cx, cy) = { let s = self.scr_ref(); (s.cx.min(cols - 1), s.cy) }; self.erase(cy, cx, cx + n); }
            'L' | 'M' => {
                let n = a(0, 1);
                let s = self.scr();
                if s.cy < s.top || s.cy > s.bottom { return; }
                for _ in 0..n.min(s.bottom - s.cy + 1) {
                    if f == 'L' { s.rows.remove(s.bottom); s.rows.insert(s.cy, blank_row(cols)); }
                    else { s.rows.remove(s.cy); s.rows.insert(s.bottom, blank_row(cols)); }
                }
                s.cx = 0;
            }
            'P' => { let n = a(0, 1); let s = self.scr(); let (x, y) = (s.cx.min(cols - 1), s.cy); for _ in 0..n.min(cols - x) { s.rows[y].remove(x); s.rows[y].push(Cell::default()); } }
            '@' => { let n = a(0, 1); let s = self.scr(); let (x, y) = (s.cx.min(cols - 1), s.cy); for _ in 0..n.min(cols - x) { s.rows[y].insert(x, Cell::default()); s.rows[y].pop(); } }
            'S' => { if !private { self.scroll_up(a(0, 1)); } }
            'T' => { if !private && args.len() <= 1 { self.scroll_down(a(0, 1)); } }
            'r' => {
                if private { return; }
                let (t, b) = (a(0, 1), a(1, rows));
                if t < b && b <= rows { let s = self.scr(); s.top = t - 1; s.bottom = b - 1; s.cx = 0; s.cy = 0; }
            }
            's' if !private => { let st = self.st; let s = self.scr(); s.saved = (s.cx, s.cy, st); }
            'u' if !private => { let (x, y, st) = self.scr_ref().saved; self.st = st; let s = self.scr(); s.cx = x; s.cy = y; }
            'm' if !gt => self.sgr(body),
            'n' if !private => {
                if a(0, 0) == 6 { let (x, y) = self.cursor(); self.replies.push(format!("\x1b[{};{}R", y + 1, x + 1)); }
                else if a(0, 0) == 5 { self.replies.push("\x1b[0n".into()); }
            }
            'c' => { if gt { self.replies.push("\x1b[>0;276;0c".into()); } else if !private { self.replies.push("\x1b[?1;2c".into()); } }
            'h' | 'l' if private => {
                let on = f == 'h';
                for m in &args {
                    match m {
                        1 => self.app_cursor = on,
                        7 => self.autowrap = on,
                        25 => self.cursor_on = on,
                        2004 => self.bracketed = on,
                        47 | 1047 | 1049 => {
                            if on && self.alt.is_none() {
                                if *m == 1049 { let st = self.st; let s = &mut self.main; s.saved = (s.cx, s.cy, st); }
                                self.alt = Some(Screen { rows: vec![blank_row(cols); rows], cx: self.main.cx, cy: self.main.cy, saved: self.main.saved, top: 0, bottom: rows - 1 });
                            } else if !on && self.alt.is_some() {
                                self.alt = None;
                                if *m == 1049 { let (x, y, st) = self.main.saved; self.main.cx = x; self.main.cy = y; self.st = st; }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn sgr(&mut self, body: &str) {
        let args: Vec<u32> = if body.is_empty() { vec![0] } else { body.split([';', ':']).map(|x| x.parse().unwrap_or(0)).collect() };
        let mut i = 0;
        while i < args.len() {
            let v = args[i];
            match v {
                0 => self.st = Style::default(),
                1 => self.st.bold = true,
                2 => self.st.dim = true,
                3 => self.st.italic = true,
                4 => self.st.underline = true,
                7 => self.st.inverse = true,
                21 | 22 => { self.st.bold = false; self.st.dim = false; }
                23 => self.st.italic = false,
                24 => self.st.underline = false,
                27 => self.st.inverse = false,
                30..=37 => self.st.fg = Color::Idx((v - 30) as u8),
                39 => self.st.fg = Color::Default,
                40..=47 => self.st.bg = Color::Idx((v - 40) as u8),
                49 => self.st.bg = Color::Default,
                90..=97 => self.st.fg = Color::Idx((v - 90 + 8) as u8),
                100..=107 => self.st.bg = Color::Idx((v - 100 + 8) as u8),
                38 | 48 => {
                    let c = match args.get(i + 1) {
                        Some(5) => { i += 2; args.get(i).map(|n| Color::Idx(*n as u8)) }
                        Some(2) => { let c = (args.get(i + 2), args.get(i + 3), args.get(i + 4)); i += 4; match c { (Some(r), Some(g), Some(b)) => Some(Color::Rgb(*r as u8, *g as u8, *b as u8)), _ => None } }
                        _ => None,
                    };
                    if let Some(c) = c { if v == 38 { self.st.fg = c; } else { self.st.bg = c; } }
                }
                _ => {}
            }
            i += 1;
        }
    }


    /// The `rows` lines on show when scrolled `back` lines up from the bottom.
    pub fn view(&self, back: usize) -> Vec<&Vec<Cell>> {
        let scr = self.scr_ref();
        if self.alt.is_some() { return scr.rows.iter().collect(); }
        let back = back.min(self.scrollback.len());
        let start = self.scrollback.len() - back;
        self.scrollback[start..].iter().chain(scr.rows.iter()).take(self.rows).collect()
    }

    /// A line's text (no trailing spaces).
    pub fn text_of(row: &[Cell]) -> String {
        let s: String = row.iter().filter(|c| !c.cont).map(|c| c.ch).collect();
        s.trim_end().to_string()
    }

    /// The screen's text, for tests and the driver.
    pub fn screen_text(&self) -> Vec<String> { self.scr_ref().rows.iter().map(|r| Term::text_of(r)).collect() }
}

/// A run of cells in one style: the column it starts at, its width in cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Run { pub col: usize, pub cells: usize, pub text: String, pub st: Style }

/// A line as runs of one style each (spaces with no background and no
/// underline are left out: nothing to draw).
pub fn runs(row: &[Cell]) -> Vec<Run> {
    let mut out: Vec<Run> = vec![];
    for (x, c) in row.iter().enumerate() {
        if c.cont { if let Some(r) = out.last_mut() { r.cells += 1; } continue; }
        let blank = c.ch == ' ' && c.st.bg == Color::Default && !c.st.inverse && !c.st.underline;
        if blank { continue; }
        if let Some(r) = out.last_mut() { if r.st == c.st && r.col + r.cells == x { r.text.push(c.ch); r.cells += 1; continue; } }
        out.push(Run { col: x, cells: 1, text: c.ch.to_string(), st: c.st });
    }
    out
}

/// The bytes a key sends (xterm's), by Slint's key text and modifiers.
pub fn key_bytes(text: &str, ctrl: bool, alt: bool, shift: bool, app_cursor: bool) -> Option<String> {
    let cur = |c: char| if app_cursor { format!("\x1bO{c}") } else { format!("\x1b[{c}") };
    let s = match text {
        "\u{f700}" => cur('A'), "\u{f701}" => cur('B'), "\u{f703}" => cur('C'), "\u{f702}" => cur('D'),
        "\u{f729}" => cur('H'), "\u{f72b}" => cur('F'),
        "\u{f72c}" => "\x1b[5~".into(), "\u{f72d}" => "\x1b[6~".into(),
        "\u{f727}" => "\x1b[2~".into(), "\u{7f}" => "\x1b[3~".into(),
        "\u{8}" => "\x7f".into(),
        "\n" | "\r" => "\r".into(),
        "\t" => if shift { "\x1b[Z".into() } else { "\t".into() },
        "\u{19}" => "\x1b[Z".into(),
        "\u{1b}" => "\x1b".into(),
        t if t.chars().next().is_some_and(|c| ('\u{f704}'..='\u{f70f}').contains(&c)) => {
            let n = t.chars().next().unwrap() as u32 - 0xf704 + 1;
            match n { 1 => "\x1bOP".into(), 2 => "\x1bOQ".into(), 3 => "\x1bOR".into(), 4 => "\x1bOS".into(),
                5 => "\x1b[15~".into(), 6 => "\x1b[17~".into(), 7 => "\x1b[18~".into(), 8 => "\x1b[19~".into(),
                9 => "\x1b[20~".into(), 10 => "\x1b[21~".into(), 11 => "\x1b[23~".into(), _ => "\x1b[24~".into() }
        }
        t if t.chars().next().is_some_and(|c| ('\u{f700}'..='\u{f8ff}').contains(&c)) => return None,
        t if ctrl && t.chars().count() == 1 => {
            let c = t.chars().next().unwrap().to_ascii_lowercase();
            match c {
                'a'..='z' => ((c as u8 - b'a' + 1) as char).to_string(),
                '[' => "\x1b".into(), '\\' => "\x1c".into(), ']' => "\x1d".into(), ' ' | '@' | '2' => "\0".into(),
                _ => return None,
            }
        }
        t if t.is_empty() => return None,
        t => t.to_string(),
    };
    Some(if alt && !s.starts_with('\x1b') { format!("\x1b{s}") } else { s })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_keeps_pending_wrap() {
        let mut t = Term::new(5, 3);
        t.feed("abcde\x1b[0m$ x");
        assert_eq!(t.screen_text(), vec!["abcde", "$ x", ""]);
    }

    #[test]
    fn prints_and_wraps() {
        let mut t = Term::new(5, 3);
        t.feed("hello world");
        assert_eq!(t.screen_text(), vec!["hello", " worl", "d"]);
        t.feed("\r\nx\ry");
        assert_eq!(t.screen_text()[2], "y");
        assert_eq!(t.scrollback.len(), 1);
        assert_eq!(Term::text_of(&t.scrollback[0]), "hello");
    }

    #[test]
    fn cursor_and_erase() {
        let mut t = Term::new(10, 3);
        t.feed("abcdef\x1b[1;3H\x1b[K");
        assert_eq!(t.screen_text()[0], "ab");
        t.feed("\x1b[2J\x1b[2;2HX");
        assert_eq!(t.screen_text(), vec!["", " X", ""]);
        t.feed("\x1b[6n");
        assert_eq!(t.replies, vec!["\x1b[2;3R".to_string()]);
    }

    #[test]
    fn colours() {
        let mut t = Term::new(10, 2);
        t.feed("\x1b[1;31mR\x1b[0m \x1b[38;2;1;2;3mT\x1b[38;5;200mI");
        let r = runs(&t.view(0)[0]);
        assert_eq!(r[0].text, "R");
        assert!(r[0].st.bold);
        assert_eq!(r[0].st.fg, Color::Idx(1));
        assert_eq!(r[1].col, 2);
        assert_eq!(r[1].st.fg, Color::Rgb(1, 2, 3));
        assert_eq!(r[2].st.fg, Color::Idx(200));
    }

    #[test]
    fn alt_screen_and_regions() {
        let mut t = Term::new(6, 3);
        t.feed("main");
        t.feed("\x1b[?1049h\x1b[Halt");
        assert!(t.is_alt());
        assert_eq!(t.screen_text()[0], "alt");
        t.feed("\x1b[?1049l");
        assert_eq!(t.screen_text()[0], "main");
        // A scroll region: line 2-3 scroll, line 1 stays.
        let mut t = Term::new(6, 3);
        t.feed("top\x1b[2;3r\x1b[2;1Ha\r\nb\r\nc");
        assert_eq!(t.screen_text(), vec!["top", "b", "c"]);
        assert!(t.scrollback.is_empty());
    }

    #[test]
    fn osc_title_and_wide() {
        let mut t = Term::new(6, 2);
        t.feed("\x1b]0;my title\x07ok");
        assert_eq!(t.title, "my title");
        assert_eq!(t.screen_text()[0], "ok");
        t.feed("\r中x");
        assert_eq!(t.screen_text()[0], "中x");
        assert_eq!(runs(&t.view(0)[0])[0].cells, 3);
    }

    #[test]
    fn keys() {
        assert_eq!(key_bytes("c", true, false, false, false).as_deref(), Some("\x03"));
        assert_eq!(key_bytes("\u{f700}", false, false, false, false).as_deref(), Some("\x1b[A"));
        assert_eq!(key_bytes("\u{f700}", false, false, false, true).as_deref(), Some("\x1bOA"));
        assert_eq!(key_bytes("\n", false, false, false, false).as_deref(), Some("\r"));
        assert_eq!(key_bytes("\u{8}", false, false, false, false).as_deref(), Some("\x7f"));
        assert_eq!(key_bytes("x", false, true, false, false).as_deref(), Some("\x1bx"));
        assert_eq!(key_bytes("\u{f710}", false, false, false, false), None);
    }

    #[test]
    fn resize_keeps_text() {
        let mut t = Term::new(10, 4);
        t.feed("a\r\nb\r\nc");
        t.resize(8, 2);
        assert_eq!(t.screen_text(), vec!["b", "c"]);
        assert_eq!(t.cursor().1, 1);
    }
}
