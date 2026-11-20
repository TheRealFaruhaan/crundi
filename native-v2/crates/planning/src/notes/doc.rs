//! Notes (4.4): the page model, pure (app/vendor/crundi-notes.js).
//!
//! A page is a title and blocks (src/notes-store.js). Text blocks hold a
//! little inline HTML on the server (strong, em, u, s, code, a, br). The
//! native client has no rich-text editor (the web's contenteditable; neither
//! Slint nor GPUI has one), so it edits a block as a short markup *source* and
//! draws it formatted when it is not being edited:
//!
//!   **bold**  *italic*  __underline__  ~~strike~~  `code`  [text](https://…)
//!
//! with `\` before a mark that is meant literally. Everything here turns
//! between the three forms: the server's HTML, the source, and runs of text
//! with their styles (the canonical form every edit goes through), and on to
//! the Markdown that Copy and a drag hand out. Moved over from the Slint
//! client (native-client/src/planning/notes_doc.rs) as it was; its StyledText
//! markup (styled, styled_plain) stays for the unit tests, GPUI draws runs.

use serde_json::{json, Value};

// ─── Runs: text with its styles ───

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sty { pub b: bool, pub i: bool, pub u: bool, pub s: bool, pub code: bool, pub href: Option<String> }

#[derive(Clone, Debug, PartialEq)]
pub struct Run { pub text: String, pub st: Sty }

/// Merge neighbours with the same style; drop empty runs.
pub fn norm(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = vec![];
    for r in runs {
        if r.text.is_empty() { continue; }
        if let Some(l) = out.last_mut() { if l.st == r.st { l.text.push_str(&r.text); continue; } }
        out.push(r);
    }
    out
}

pub fn plain(runs: &[Run]) -> String { runs.iter().map(|r| r.text.as_str()).collect() }

fn ok_href(h: &str) -> bool {
    let l = h.trim().to_ascii_lowercase();
    (l.starts_with("http://") || l.starts_with("https://") || l.starts_with("mailto:")) && !h.contains(['"', '<', '>'])
}

// ─── HTML (the server's) ───

fn decode_entities(s: &str) -> String {
    if !s.contains('&') { return s.to_string(); }
    let mut o = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        o.push_str(&rest[..p]);
        let tail = &rest[p..];
        let end = tail.find(';').filter(|e| *e <= 10);
        let ent = end.map(|e| &tail[1..e]);
        let ch = match ent {
            Some("amp") => Some('&'), Some("lt") => Some('<'), Some("gt") => Some('>'), Some("quot") => Some('"'),
            Some("apos") => Some('\''), Some("nbsp") => Some(' '),
            Some(e) if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            Some(e) if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match (ch, end) {
            (Some(c), Some(e)) => { o.push(if c == '\u{a0}' { ' ' } else { c }); rest = &tail[e + 1..]; }
            _ => { o.push('&'); rest = &tail[1..]; }
        }
    }
    o.push_str(rest);
    o.replace('\u{a0}', " ")
}

fn attr_href(attrs: &str) -> Option<String> {
    let l = attrs.to_ascii_lowercase();
    let p = l.find("href")?;
    let after = attrs[p + 4..].trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    let v = if let Some(r) = after.strip_prefix('"') { r.split('"').next()? } else if let Some(r) = after.strip_prefix('\'') { r.split('\'').next()? } else { after.split(|c: char| c.is_whitespace() || c == '>').next()? };
    Some(decode_entities(v))
}

/// The server's inline HTML as runs (unknown tags unwrapped, as sanitize()).
pub fn html_to_runs(html: &str) -> Vec<Run> {
    let mut out = vec![];
    let mut st = Sty::default();
    // A stack of the open tags, to undo each at its close.
    let mut stack: Vec<(String, Sty)> = vec![];
    let mut text = String::new();
    let flush = |text: &mut String, st: &Sty, out: &mut Vec<Run>| { if !text.is_empty() { out.push(Run { text: decode_entities(text), st: st.clone() }); text.clear(); } };
    let mut rest = html;
    let mut skip_until: Option<String> = None;
    while !rest.is_empty() {
        if let Some(lt) = rest.find('<') {
            let (before, tail) = rest.split_at(lt);
            if skip_until.is_none() { text.push_str(before); }
            let Some(gt) = tail.find('>') else { if skip_until.is_none() { text.push_str(tail); } break; };
            let tag = &tail[1..gt];
            rest = &tail[gt + 1..];
            let closing = tag.starts_with('/');
            let body = tag.trim_start_matches('/');
            let name: String = body.chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
            if let Some(s) = &skip_until { if closing && &name == s { skip_until = None; } continue; }
            if name.is_empty() { if skip_until.is_none() { text.push('<'); text.push_str(tag); text.push('>'); } continue; }
            if !closing && matches!(name.as_str(), "script" | "style" | "iframe" | "object" | "embed" | "template") { skip_until = Some(name); continue; }
            match name.as_str() {
                "br" => text.push('\n'),
                "div" | "p" => { if !closing && !(out.is_empty() && text.is_empty()) { text.push('\n'); } }
                "strong" | "b" | "em" | "i" | "u" | "s" | "strike" | "del" | "code" | "a" => {
                    flush(&mut text, &st, &mut out);
                    if closing {
                        if let Some(i) = stack.iter().rposition(|(n, _)| *n == name) { st = stack[i].1.clone(); stack.truncate(i); }
                    } else {
                        stack.push((name.clone(), st.clone()));
                        match name.as_str() {
                            "strong" | "b" => st.b = true,
                            "em" | "i" => st.i = true,
                            "u" => st.u = true,
                            "s" | "strike" | "del" => st.s = true,
                            "code" => st.code = true,
                            _ => { if let Some(h) = attr_href(&tag[name.len()..]).filter(|h| ok_href(h)) { st.href = Some(h.trim().to_string()); } }
                        }
                    }
                }
                _ => {}
            }
        } else {
            if skip_until.is_none() { text.push_str(rest); }
            break;
        }
    }
    flush(&mut text, &st, &mut out);
    norm(out)
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

// The nesting order, outermost first: link, strike, bold, italic, underline, code.
fn level_key(st: &Sty, lvl: usize) -> Option<String> {
    match lvl {
        0 => st.href.clone(),
        1 => st.s.then(|| "s".into()),
        2 => st.b.then(|| "b".into()),
        3 => st.i.then(|| "i".into()),
        4 => st.u.then(|| "u".into()),
        5 => st.code.then(|| "c".into()),
        _ => None,
    }
}

/// Runs nested by the fixed order: `wrap(level, key, inner)` for a styled
/// group, `leaf(text)` for the text.
fn nest(runs: &[Run], lvl: usize, leaf: &mut dyn FnMut(&str, &Sty) -> String, wrap: &mut dyn FnMut(usize, &str, String) -> String) -> String {
    if lvl > 5 { return runs.iter().map(|r| leaf(&r.text, &r.st)).collect(); }
    let mut o = String::new();
    let mut i = 0;
    while i < runs.len() {
        let k = level_key(&runs[i].st, lvl);
        let mut j = i + 1;
        while j < runs.len() && level_key(&runs[j].st, lvl) == k { j += 1; }
        let inner = nest(&runs[i..j], lvl + 1, leaf, wrap);
        o.push_str(&match k { Some(k) => wrap(lvl, &k, inner), None => inner });
        i = j;
    }
    o
}

pub fn runs_to_html(runs: &[Run]) -> String {
    nest(runs, 0, &mut |t, _| esc_html(t).replace('\n', "<br>"), &mut |lvl, k, inner| match lvl {
        0 => format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{inner}</a>", esc_html(k)),
        1 => format!("<s>{inner}</s>"),
        2 => format!("<strong>{inner}</strong>"),
        3 => format!("<em>{inner}</em>"),
        4 => format!("<u>{inner}</u>"),
        _ => format!("<code>{inner}</code>"),
    })
}

// ─── The editing source ───

/// The source of runs, and where each plain character starts in it (byte
/// offsets, one more at the end).
pub fn runs_to_src_map(runs: &[Run]) -> (String, Vec<usize>) {
    let mut map: Vec<usize> = vec![];
    let mut out = String::new();
    // Built run by run (not through nest()) so the map can follow.
    let order = |st: &Sty| -> Vec<(usize, String)> { (0..6).filter_map(|l| level_key(st, l).map(|k| (l, k))).collect() };
    let mut open: Vec<(usize, String)> = vec![];
    let opener = |l: usize| -> &'static str { ["[", "~~", "**", "*", "__", "`"][l] };
    let closer = |l: usize, k: &str| -> String { if l == 0 { format!("]({})", k.replace(')', "%29").replace(' ', "%20")) } else { opener(l).to_string() } };
    for r in runs {
        let want = order(&r.st);
        // Close what this run doesn't share (from the innermost), open what it adds.
        let keep = open.iter().zip(want.iter()).take_while(|(a, b)| a == b).count();
        while open.len() > keep { let (l, k) = open.pop().unwrap(); out.push_str(&closer(l, &k)); }
        for w in &want[keep..] { out.push_str(opener(w.0)); open.push(w.clone()); }
        let in_code = r.st.code;
        let chars: Vec<char> = r.text.chars().collect();
        for (ci, c) in chars.iter().enumerate() {
            map.push(out.len());
            if in_code {
                if *c == '`' { out.push('\''); } else { out.push(*c); }
                continue;
            }
            let next = chars.get(ci + 1).copied();
            let prev = if ci > 0 { Some(chars[ci - 1]) } else { None };
            let need = match c {
                '\\' | '*' | '`' | '[' => true,
                ']' => r.st.href.is_some(),
                '~' => next == Some('~') || prev == Some('~') || ci == 0 || ci + 1 == chars.len(),
                '_' => next == Some('_') || prev == Some('_') || ci == 0 || ci + 1 == chars.len(),
                _ => false,
            };
            if need { out.push('\\'); }
            out.push(*c);
        }
    }
    while let Some((l, k)) = open.pop() { out.push_str(&closer(l, &k)); }
    map.push(out.len());
    (out, map)
}

pub fn runs_to_src(runs: &[Run]) -> String { runs_to_src_map(runs).0 }

struct P<'a> { c: &'a [(usize, char)], len: usize }

impl P<'_> {
    fn at(&self, i: usize, s: &str) -> bool {
        let mut k = i;
        for ch in s.chars() { if k >= self.c.len() || self.c[k].1 != ch { return false; } k += 1; }
        true
    }
    fn off(&self, i: usize) -> usize { self.c.get(i).map(|x| x.0).unwrap_or(self.len) }
}

/// Parse source into runs; `map[k]` is the source byte offset of plain char k
/// (and one more at the end).
pub fn src_to_runs_map(src: &str) -> (Vec<Run>, Vec<usize>) {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let p = P { c: &chars, len: src.len() };
    let mut runs = vec![];
    let mut map = vec![];
    let mut i = 0;
    parse(&p, &mut i, None, &Sty::default(), &mut runs, &mut map);
    map.push(src.len());
    (norm_keep_map(runs), map)
}

pub fn src_to_runs(src: &str) -> Vec<Run> { src_to_runs_map(src).0 }

// norm() but the map stays per plain char (merging never changes the text).
fn norm_keep_map(runs: Vec<Run>) -> Vec<Run> { norm(runs) }

fn push_char(runs: &mut Vec<Run>, map: &mut Vec<usize>, st: &Sty, c: char, off: usize) {
    map.push(off);
    if let Some(l) = runs.last_mut() { if &l.st == st { l.text.push(c); return; } }
    runs.push(Run { text: c.to_string(), st: st.clone() });
}

/// Parse until `close` (returns true when it was found and consumed).
fn parse(p: &P, i: &mut usize, close: Option<&str>, st: &Sty, runs: &mut Vec<Run>, map: &mut Vec<usize>) -> bool {
    while *i < p.c.len() {
        let c = p.c[*i].1;
        if let Some(cl) = close {
            // `*` closes italics, but not where a `**` opens bold inside it.
            if p.at(*i, cl) && !(cl == "*" && p.at(*i, "**") && try_span(p, *i, "**", st, |s| s.b = true).is_some()) { *i += cl.chars().count(); return true; }
        }
        if c == '\\' && *i + 1 < p.c.len() && p.c[*i + 1].1.is_ascii_punctuation() {
            push_char(runs, map, st, p.c[*i + 1].1, p.off(*i));
            *i += 2;
            continue;
        }
        if c == '`' && !st.code {
            if let Some(end) = (*i + 1..p.c.len()).find(|k| p.c[*k].1 == '`') {
                if end > *i + 1 {
                    let mut s2 = st.clone(); s2.code = true;
                    for k in *i + 1..end { push_char(runs, map, &s2, p.c[k].1, p.off(k)); }
                    *i = end + 1;
                    continue;
                }
            }
        }
        let tries: [(&str, fn(&mut Sty)); 4] = [("**", |s| s.b = true), ("__", |s| s.u = true), ("~~", |s| s.s = true), ("*", |s| s.i = true)];
        let mut done = false;
        for (mk, f) in tries {
            if !p.at(*i, mk) { continue; }
            // Not again inside itself; `*` not as the start of `**`.
            let mut probe = st.clone(); f(&mut probe);
            if probe == *st { continue; }
            if let Some((sub, sub_map, ni)) = try_span(p, *i, mk, st, f) {
                runs.extend(sub);
                map.extend(sub_map);
                *i = ni;
                done = true;
                break;
            }
        }
        if done { continue; }
        if c == '[' && st.href.is_none() {
            if let Some((sub, sub_map, ni)) = try_link(p, *i, st) {
                runs.extend(sub);
                map.extend(sub_map);
                *i = ni;
                continue;
            }
        }
        push_char(runs, map, st, c, p.off(*i));
        *i += 1;
    }
    false
}

type Span = (Vec<Run>, Vec<usize>, usize);

fn try_span(p: &P, at: usize, mk: &str, st: &Sty, f: fn(&mut Sty)) -> Option<Span> {
    let n = mk.chars().count();
    // An opener must be followed by text (not a space, not the end).
    if at + n >= p.c.len() || p.c[at + n].1.is_whitespace() { return None; }
    let mut s2 = st.clone(); f(&mut s2);
    let mut i = at + n;
    let mut runs = vec![];
    let mut map = vec![];
    if parse(p, &mut i, Some(mk), &s2, &mut runs, &mut map) && !map.is_empty() { Some((runs, map, i)) } else { None }
}

fn try_link(p: &P, at: usize, st: &Sty) -> Option<Span> {
    // [text](href): the text ends at the first unescaped "](".
    let mut k = at + 1;
    let mut depth = 0;
    while k < p.c.len() {
        let c = p.c[k].1;
        if c == '\\' { k += 2; continue; }
        if c == '[' { depth += 1; }
        if c == ']' { if depth == 0 { break; } depth -= 1; }
        k += 1;
    }
    if k >= p.c.len() || !p.at(k, "](") { return None; }
    let h0 = k + 2;
    let h1 = (h0..p.c.len()).find(|j| p.c[*j].1 == ')' || p.c[*j].1.is_whitespace())?;
    if p.c[h1].1 != ')' { return None; }
    let href: String = p.c[h0..h1].iter().map(|x| x.1).collect();
    let href = href.replace("%29", ")").replace("%20", " ");
    if k == at + 1 { return None; }
    let mut s2 = st.clone();
    s2.href = Some(if ok_href(&href) { href } else { String::new() });
    let sub = P { c: &p.c[..k], len: p.off(k) };
    let mut i = at + 1;
    let mut runs = vec![];
    let mut map = vec![];
    parse(&sub, &mut i, None, &s2, &mut runs, &mut map);
    // A link to nowhere (not http/mailto) keeps its text only.
    for r in runs.iter_mut() { if r.st.href.as_deref() == Some("") { r.st.href = None; } }
    Some((runs, map, h1 + 1))
}

// ─── Conversions in one step ───

pub fn html_to_src(html: &str) -> String { runs_to_src(&html_to_runs(html)) }
pub fn src_to_html(src: &str) -> String { runs_to_html(&src_to_runs(src)) }
pub fn src_plain(src: &str) -> String { plain(&src_to_runs(src)) }

/// A source byte offset as a plain-character offset.
pub fn src_to_plain_at(src: &str, byte: usize) -> usize {
    let (_, map) = src_to_runs_map(src);
    map.iter().position(|o| *o >= byte).unwrap_or(map.len().saturating_sub(1))
}

/// A plain-character offset as a byte offset of `src` (as runs_to_src writes it).
pub fn plain_to_src_at(map: &[usize], k: usize) -> usize { map.get(k).copied().unwrap_or_else(|| map.last().copied().unwrap_or(0)) }

// ─── StyledText markup (the block drawn when not being edited) ───

fn esc_styled(s: &str, line_start: bool) -> String {
    let mut o = String::new();
    let mut lead = line_start;
    let mut prev_space = false;
    for ch in s.chars() {
        match ch {
            ' ' if lead || prev_space => { o.push('\u{a0}'); prev_space = true; continue; }
            ' ' => { o.push(' '); prev_space = true; continue; }
            '\t' => { o.push_str("\u{a0}\u{a0}"); }
            c if c.is_ascii_punctuation() => { o.push('\\'); o.push(c); }
            c if c.is_control() => {}
            c => o.push(c),
        }
        lead = false;
        prev_space = false;
    }
    o
}

/// One line of runs (no newlines) as markup.
fn line_markup(runs: &[Run], done: bool) -> String {
    let mut first = true;
    let body = nest(runs, 0, &mut |t, st| {
        if st.code { let t = t.replace('`', "'"); let r = if t.starts_with(' ') || t.ends_with(' ') { format!("` {t} `") } else { format!("`{t}`") }; first = false; return r; }
        let e = esc_styled(t, first);
        first = false;
        e
    }, &mut |lvl, k, inner| {
        // A mark must hug its text: spaces go outside it.
        let lead: String = inner.chars().take_while(|c| *c == ' ' || *c == '\u{a0}').collect();
        if lead.len() == inner.len() { return inner; }
        let tail: String = inner[lead.len()..].chars().rev().take_while(|c| *c == ' ' || *c == '\u{a0}').collect::<String>().chars().rev().collect();
        let core = &inner[lead.len()..inner.len() - tail.len()];
        if core.is_empty() { return inner; }
        let w = match lvl {
            0 => format!("[{core}]({})", k.replace(')', "%29").replace(' ', "%20")),
            1 => format!("~~{core}~~"),
            2 => format!("**{core}**"),
            3 => format!("*{core}*"),
            4 => format!("<u>{core}</u>"),
            _ => core.to_string(),
        };
        format!("{lead}\u{200b}{w}\u{200b}{tail}")
    });
    let body = if done && !body.is_empty() { format!("~~{body}~~") } else { body };
    if body.is_empty() { "\u{a0}".into() } else { body }
}

/// Split runs into lines at their newlines.
pub fn lines(runs: &[Run]) -> Vec<Vec<Run>> {
    let mut out: Vec<Vec<Run>> = vec![vec![]];
    for r in runs {
        let mut parts = r.text.split('\n');
        if let Some(f) = parts.next() { if !f.is_empty() { out.last_mut().unwrap().push(Run { text: f.into(), st: r.st.clone() }); } }
        for p in parts {
            out.push(vec![]);
            if !p.is_empty() { out.last_mut().unwrap().push(Run { text: p.into(), st: r.st.clone() }); }
        }
    }
    out
}

/// A block's text as StyledText markup (`done`: a ticked to-do, struck
/// through; `bold`: a heading, as StyledText has no weight of its own).
pub fn styled(src: &str, done: bool, bold: bool) -> String {
    let mut runs = src_to_runs(src);
    if bold { for r in runs.iter_mut() { if !r.st.code { r.st.b = true; } } }
    lines(&runs).iter().map(|l| line_markup(l, done)).collect::<Vec<_>>().join("\n")
}

/// The same with no styles at all (when Slint refuses the markup).
pub fn styled_plain(src: &str) -> String {
    src_plain(src).split('\n').map(|l| { let e = esc_styled(l, true); if e.is_empty() { "\u{a0}".to_string() } else { e } }).collect::<Vec<_>>().join("\n")
}

// ─── Markdown (Copy, and a block dragged onto a chat) ───

/// The web's inlineMd: **b**, *i*, ~~s~~, `code`, [text](href); underline plain.
pub fn runs_md(runs: &[Run]) -> String {
    nest(runs, 0, &mut |t, st| if st.code { format!("`{t}`") } else { t.to_string() }, &mut |lvl, k, inner| {
        if lvl == 5 { return inner; }
        if inner.trim().is_empty() && lvl != 0 { return inner; }
        match lvl { 0 => format!("[{inner}]({k})"), 1 => format!("~~{inner}~~"), 2 => format!("**{inner}**"), 3 => format!("*{inner}*"), _ => inner }
    })
}

pub fn src_md(src: &str) -> String { runs_md(&src_to_runs(src)) }

// ─── Blocks ───

pub const TYPES: &[(&str, &str, &str, &str)] = &[
    ("p", "T", "Text", ""),
    ("h1", "H1", "Heading 1", "#"),
    ("h2", "H2", "Heading 2", "##"),
    ("h3", "H3", "Heading 3", "###"),
    ("todo", "\u{2610}", "To-do", "[]"),
    ("bullet", "\u{2022}", "Bulleted list", "-"),
    ("number", "1.", "Numbered list", "1."),
    ("quote", "\u{201c}", "Quote", ">"),
    ("code", "</>", "Code", "```"),
    ("table", "\u{25a6}", "Table", ""),
    ("divider", "\u{2014}", "Divider", "---"),
];

pub fn placeholder(t: &str) -> &'static str {
    match t { "p" => "Type '/' for blocks", "h1" => "Heading 1", "h2" => "Heading 2", "h3" => "Heading 3", "todo" => "To-do", "bullet" | "number" => "List", "quote" => "Quote", _ => "" }
}
pub fn texty(t: &str) -> bool { matches!(t, "p" | "h1" | "h2" | "h3" | "todo" | "bullet" | "number" | "quote") }
pub fn listy(t: &str) -> bool { matches!(t, "todo" | "bullet" | "number") }

#[derive(Clone, Debug, PartialEq, Default, Hash)]
pub struct Block {
    pub id: String,
    pub kind: String,
    /// Texty: the source; code: the code.
    pub text: String,
    pub checked: bool,
    pub lang: String,
    pub wrap: bool,
    /// A table's cells, as sources.
    pub rows: Vec<Vec<String>>,
    pub header: bool,
}

pub fn gen_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let mut x = t ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (std::process::id() as u64).rotate_left(32);
    // splitmix64
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    format!("{:016x}", z ^ (z >> 31))
}

impl Block {
    pub fn new(kind: &str, text: &str) -> Block { Block { id: gen_id(), kind: kind.into(), text: text.into(), ..Default::default() } }

    /// From the server's JSON.
    pub fn from_json(v: &Value) -> Block {
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let mut kind = s("type");
        if !TYPES.iter().any(|t| t.0 == kind) { kind = "p".into(); }
        let id = { let i = s("id"); if i.is_empty() { gen_id() } else { i } };
        let mut b = Block { id, kind: kind.clone(), ..Default::default() };
        match kind.as_str() {
            "divider" => {}
            "code" => { b.text = s("text"); b.lang = s("lang"); b.wrap = v.get("wrap").and_then(|x| x.as_bool()).unwrap_or(false); }
            "table" => {
                b.rows = v.get("rows").and_then(|r| r.as_array()).map(|rs| rs.iter().map(|r| r.as_array().map(|cs| cs.iter().map(|c| html_to_src(c.as_str().unwrap_or(""))).collect()).unwrap_or_default()).collect()).unwrap_or_default();
                if b.rows.is_empty() { b.rows = vec![vec![String::new(); 2]; 2]; }
                b.header = v.get("header").and_then(|x| x.as_bool()).unwrap_or(false);
            }
            _ => { b.text = html_to_src(&s("text")); b.checked = kind == "todo" && v.get("checked").and_then(|x| x.as_bool()).unwrap_or(false); }
        }
        b
    }

    /// For the server (readBlock()).
    pub fn to_json(&self) -> Value {
        match self.kind.as_str() {
            "divider" => json!({ "id": self.id, "type": "divider" }),
            "code" => { let mut o = json!({ "id": self.id, "type": "code", "text": self.text, "lang": if self.lang == "plain" { "" } else { self.lang.as_str() } }); if self.wrap { o["wrap"] = json!(true); } o }
            "table" => json!({ "id": self.id, "type": "table", "rows": self.rows.iter().map(|r| r.iter().map(|c| src_to_html(c)).collect::<Vec<_>>()).collect::<Vec<_>>(), "header": self.header }),
            "todo" => json!({ "id": self.id, "type": "todo", "text": src_to_html(&self.text), "checked": self.checked }),
            k => json!({ "id": self.id, "type": k, "text": src_to_html(&self.text) }),
        }
    }

    /// Turn into another type, keeping the text where both have text (convert()).
    pub fn converted(&self, kind: &str) -> Block {
        let mut nb = Block { id: self.id.clone(), kind: kind.into(), ..Default::default() };
        match kind {
            "code" => { nb.text = if self.kind == "code" { self.text.clone() } else { src_plain(&self.text) }; nb.lang = self.lang.clone(); nb.wrap = self.wrap; }
            "table" => nb.rows = vec![vec![String::new(); 3]; 3],
            "divider" => {}
            _ => nb.text = if self.kind == "code" { runs_to_src(&[Run { text: self.text.clone(), st: Sty::default() }]) } else if self.kind == "table" || self.kind == "divider" { String::new() } else { self.text.clone() },
        }
        nb
    }
}

pub fn blocks_from(v: &Value) -> Vec<Block> {
    let mut b: Vec<Block> = v.as_array().map(|a| a.iter().map(Block::from_json).collect()).unwrap_or_default();
    if b.is_empty() { b.push(Block::new("p", "")); }
    b
}

/// The list's numbers ("1.", "2." …) for numbered items, "" for the rest.
pub fn numbers(blocks: &[Block]) -> Vec<String> {
    let mut n = 0;
    blocks.iter().map(|b| if b.kind == "number" { n += 1; format!("{n}.") } else { n = 0; String::new() }).collect()
}

/// Blocks as Markdown (toMarkdown()), consecutive numbered items numbered.
pub fn to_markdown(blocks: &[Block]) -> String {
    let mut out: Vec<String> = vec![];
    let mut n = 0;
    for b in blocks {
        if b.kind != "number" { n = 0; }
        let t = if matches!(b.kind.as_str(), "code" | "table" | "divider") { String::new() } else { src_md(&b.text) };
        let rest = |s: &str, pad: &str| s.replace('\n', &format!("\n{pad}"));
        out.push(match b.kind.as_str() {
            "h1" => format!("# {t}"),
            "h2" => format!("## {t}"),
            "h3" => format!("### {t}"),
            "todo" => format!("- [{}] {}", if b.checked { "x" } else { " " }, rest(&t, "  ")),
            "bullet" => format!("- {}", rest(&t, "  ")),
            "number" => { n += 1; format!("{n}. {}", rest(&t, "   ")) }
            "quote" => format!("> {}", t.replace('\n', "\n> ")),
            "divider" => "---".into(),
            "code" => format!("```{}\n{}\n```", if b.lang == "plain" { "" } else { &b.lang }, b.text),
            "table" => {
                let rows: Vec<Vec<String>> = b.rows.iter().map(|r| r.iter().map(|c| src_md(c).replace('\n', " ").replace('|', "\\|")).collect()).collect();
                if rows.is_empty() { continue; }
                let line = |r: &Vec<String>| format!("| {} |", r.join(" | "));
                let mut l = vec![line(&rows[0]), line(&rows[0].iter().map(|_| "---".to_string()).collect())];
                l.extend(rows[1..].iter().map(line));
                l.join("\n")
            }
            _ => t,
        });
    }
    let mut res = String::new();
    for (i, o) in out.iter().enumerate() {
        if i > 0 {
            let tight = listy(&blocks[i].kind) && listy(&blocks[i - 1].kind);
            res.push_str(if tight { "\n" } else { "\n\n" });
        }
        res.push_str(o);
    }
    res
}

// ─── Typing rules ───

#[derive(Debug, PartialEq)]
pub enum Shortcut { To(&'static str, bool), Divider, Code(String) }

/// Markdown-style shortcuts typed at the start of a block (shortcuts()).
pub fn shortcut(kind: &str, src: &str) -> Option<Shortcut> {
    let t = src.replace('\u{a0}', " ");
    let rules: &[(&str, &'static str, bool)] = &[("# ", "h1", false), ("## ", "h2", false), ("### ", "h3", false), ("- ", "bullet", false), ("* ", "bullet", false), ("\\* ", "bullet", false),
        ("1. ", "number", false), ("1) ", "number", false), ("[] ", "todo", false), ("[ ] ", "todo", false), ("\\[] ", "todo", false), ("\\[ ] ", "todo", false),
        ("[x] ", "todo", true), ("[X] ", "todo", true), ("\\[x] ", "todo", true), ("\\[X] ", "todo", true), ("> ", "quote", false)];
    for (pat, k, ch) in rules { if t == *pat { return Some(Shortcut::To(k, *ch)); } }
    if kind == "p" && t == "---" { return Some(Shortcut::Divider); }
    if kind == "p" {
        let u = t.replace("\\`", "`");
        if let Some(rest) = u.strip_prefix("```") {
            if let Some(lang) = rest.strip_suffix(' ') {
                if lang.chars().all(|c| c.is_ascii_alphanumeric() || "+#._-".contains(c)) { return Some(Shortcut::Code(norm_lang(lang))); }
            }
        }
    }
    None
}

/// The query of an open slash menu, or None when it should close (follow()).
pub fn slash_query(plain: &str, slash_at: usize, caret: usize) -> Option<String> {
    let ch: Vec<char> = plain.chars().collect();
    if ch.get(slash_at) != Some(&'/') || caret <= slash_at || caret > ch.len() { return None; }
    let q: String = ch[slash_at + 1..caret].iter().collect();
    if q.chars().any(char::is_whitespace) || q.chars().count() > 20 { return None; }
    Some(q)
}

/// Whether a "/" just typed (at plain offset caret-1) opens the menu.
pub fn slash_opens(plain: &str, caret: usize) -> bool {
    let ch: Vec<char> = plain.chars().collect();
    caret >= 1 && ch.get(caret - 1) == Some(&'/') && (caret == 1 || ch.get(caret - 2).is_some_and(|c| c.is_whitespace()))
}

/// The block types a slash query shows (TYPES filtered by name or id).
pub fn slash_items(q: &str) -> Vec<&'static (&'static str, &'static str, &'static str, &'static str)> {
    let q = q.to_lowercase();
    TYPES.iter().filter(|t| q.is_empty() || t.2.to_lowercase().contains(&q) || t.0.starts_with(&q)).collect()
}

// ─── Code languages (the web's CM.langs) ───

pub const LANGS: &[(&str, &str)] = &[
    ("plain", "Plain text"), ("bash", "Shell"), ("javascript", "JavaScript"), ("typescript", "TypeScript"), ("jsx", "JSX"), ("tsx", "TSX"), ("json", "JSON"), ("html", "HTML"), ("css", "CSS"), ("markdown", "Markdown"), ("python", "Python"), ("go", "Go"), ("rust", "Rust"), ("java", "Java"), ("kotlin", "Kotlin"), ("csharp", "C#"), ("scala", "Scala"), ("c", "C"), ("cpp", "C++"), ("swift", "Swift"), ("ruby", "Ruby"), ("php", "PHP"), ("lua", "Lua"), ("sql", "SQL"), ("yaml", "YAML"), ("toml", "TOML"), ("ini", "INI / .env"), ("xml", "XML"), ("dockerfile", "Dockerfile"), ("powershell", "PowerShell"), ("diff", "Diff"), ("vue", "Vue"),
];

/// Every other name a language goes by (CM.langs' aliases).
const ALIASES: &[(&str, &str)] = &[
    ("text", "plain"), ("txt", "plain"), ("plaintext", "plain"), ("sh", "bash"), ("shell", "bash"), ("zsh", "bash"), ("console", "bash"), ("js", "javascript"), ("mjs", "javascript"), ("cjs", "javascript"), ("node", "javascript"), ("ts", "typescript"), ("mts", "typescript"), ("cts", "typescript"), ("jsonl", "json"), ("json5", "json"), ("htm", "html"), ("svelte", "html"), ("scss", "css"), ("less", "css"), ("md", "markdown"), ("mdx", "markdown"), ("py", "python"), ("pyw", "python"), ("python3", "python"), ("golang", "go"), ("rs", "rust"), ("kt", "kotlin"), ("kts", "kotlin"), ("cs", "csharp"), ("c#", "csharp"), ("h", "c"), ("cxx", "cpp"), ("cc", "cpp"), ("hpp", "cpp"), ("c++", "cpp"), ("rb", "ruby"), ("psql", "sql"), ("mysql", "sql"), ("yml", "yaml"), ("env", "ini"), ("properties", "ini"), ("conf", "ini"), ("cfg", "ini"), ("svg", "xml"), ("xsl", "xml"), ("xsd", "xml"), ("plist", "xml"), ("docker", "dockerfile"), ("ps1", "powershell"), ("psm1", "powershell"), ("pwsh", "powershell"), ("patch", "diff"),
];

/// A language id from whatever was written ('sh', 'ts', 'Python'…); '' is plain.
pub fn norm_lang(name: &str) -> String {
    let n = name.trim().to_lowercase();
    if n.is_empty() { return "plain".into(); }
    if let Some(l) = LANGS.iter().find(|l| l.0 == n || l.1.to_lowercase() == n) { return l.0.into(); }
    if let Some(a) = ALIASES.iter().find(|a| a.0 == n) { return a.1.into(); }
    let c: String = n.chars().filter(|c| c.is_ascii_alphanumeric() || "+#._-".contains(*c)).take(24).collect();
    if c.is_empty() { "plain".into() } else { c }
}

pub fn lang_name(id: &str) -> String {
    LANGS.iter().find(|l| l.0 == id).map(|l| l.1.to_string()).unwrap_or_else(|| if id.is_empty() || id == "plain" { "Plain text".into() } else { id.to_string() })
}

/// The languages a search shows (openLangMenu's filter).
pub fn lang_items(q: &str) -> Vec<(&'static str, &'static str)> {
    let q = q.trim().to_lowercase();
    LANGS.iter().copied().filter(|l| q.is_empty() || l.1.to_lowercase().contains(&q) || l.0.starts_with(&q)).collect()
}

/// A language for the highlighter (src/workspace/hl.rs works by file name).
pub fn hl_name(id: &str) -> String {
    if id == "dockerfile" { return "Dockerfile".into(); }
    let ext = match id { "bash" => "sh", "javascript" => "js", "typescript" => "ts", "python" => "py", "rust" => "rs", "ruby" => "rb", "markdown" => "md", "csharp" => "cs", "kotlin" => "kt", "powershell" => "ps1", "plain" => "txt", x => x };
    format!("x.{ext}")
}

// ─── Inline edits on runs (format bar, Ctrl+B…) ───

/// Split runs at a plain offset.
pub fn split_runs(runs: &[Run], at: usize) -> (Vec<Run>, Vec<Run>) {
    let (mut a, mut b) = (vec![], vec![]);
    let mut k = 0;
    for r in runs {
        let n = r.text.chars().count();
        if k + n <= at { a.push(r.clone()); } else if k >= at { b.push(r.clone()); } else {
            let cut = r.text.char_indices().nth(at - k).map(|x| x.0).unwrap_or(r.text.len());
            a.push(Run { text: r.text[..cut].into(), st: r.st.clone() });
            b.push(Run { text: r.text[cut..].into(), st: r.st.clone() });
        }
        k += n;
    }
    (norm(a), norm(b))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark { B, I, U, S, Code }

fn has(st: &Sty, m: Mark) -> bool { match m { Mark::B => st.b, Mark::I => st.i, Mark::U => st.u, Mark::S => st.s, Mark::Code => st.code } }
fn set(st: &mut Sty, m: Mark, v: bool) { match m { Mark::B => st.b = v, Mark::I => st.i = v, Mark::U => st.u = v, Mark::S => st.s = v, Mark::Code => st.code = v } }

/// Apply `f` to the runs between plain offsets a..b.
fn on_range(runs: &[Run], a: usize, b: usize, f: &mut dyn FnMut(&mut Sty)) -> Vec<Run> {
    let (l, rest) = split_runs(runs, a);
    let (mut m, r) = split_runs(&rest, b - a);
    for x in m.iter_mut() { f(&mut x.st); }
    norm(l.into_iter().chain(m).chain(r).collect())
}

/// Toggle a mark over a..b: off when all of it has it, else on.
pub fn toggle(runs: &[Run], a: usize, b: usize, m: Mark) -> Vec<Run> {
    let (a, b) = (a.min(b), a.max(b));
    if a == b { return runs.to_vec(); }
    let (_, rest) = split_runs(runs, a);
    let (mid, _) = split_runs(&rest, b - a);
    let all = !mid.is_empty() && mid.iter().all(|r| has(&r.st, m));
    on_range(runs, a, b, &mut |st| set(st, m, !all))
}

/// Link a..b to `href` (None: unlink).
pub fn link(runs: &[Run], a: usize, b: usize, href: Option<String>) -> Vec<Run> {
    let (a, b) = (a.min(b), a.max(b));
    on_range(runs, a, b, &mut |st| st.href = href.clone())
}

/// The link address under a..b, if any (the link input's starting value).
pub fn link_at(runs: &[Run], a: usize, b: usize) -> String {
    let (a, b) = (a.min(b), a.max(b));
    let mut k = 0;
    for r in runs {
        let n = r.text.chars().count();
        if k < b.max(a + 1) && k + n > a { if let Some(h) = &r.st.href { return h.clone(); } }
        k += n;
    }
    String::new()
}

/// The URL as the link input applies it (an address without a scheme gets https://).
pub fn fix_url(u: &str) -> String {
    let u = u.trim();
    let l = u.to_ascii_lowercase();
    if l.starts_with("http://") || l.starts_with("https://") || l.starts_with("mailto:") { u.to_string() } else { format!("https://{}", u.trim_start_matches('/')) }
}

pub fn is_url(s: &str) -> bool { let t = s.trim(); (t.starts_with("http://") || t.starts_with("https://")) && !t.contains(char::is_whitespace) && t.len() > 8 }

/// Plain text as runs with its URLs as links (linkifyText()).
pub fn linkify(text: &str) -> Vec<Run> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(p) = rest.find("http://").into_iter().chain(rest.find("https://")).min() {
        let ok_start = p == 0 || !rest[..p].chars().last().is_some_and(|c| c.is_alphanumeric());
        let end = rest[p..].find(|c: char| c.is_whitespace() || c == '<').map(|e| p + e).unwrap_or(rest.len());
        let mut u = &rest[p..end];
        while let Some(c) = u.chars().last() { if ".,;:!?)]'\"".contains(c) { u = &u[..u.len() - c.len_utf8()]; } else { break; } }
        if !ok_start || u.len() <= 8 {
            out.push(Run { text: rest[..p + 4].into(), st: Sty::default() });
            rest = &rest[p + 4..];
            continue;
        }
        out.push(Run { text: rest[..p].into(), st: Sty::default() });
        out.push(Run { text: u.into(), st: Sty { href: Some(u.into()), ..Default::default() } });
        rest = &rest[p + u.len()..];
    }
    out.push(Run { text: rest.into(), st: Sty::default() });
    norm(out)
}

// ─── The page list ───

/// The web's ago(): just now, 5m ago, 3h ago, 2d ago.
pub fn ago(now: f64, then: f64) -> String {
    if then <= 0.0 { return String::new(); }
    let s = (now - then).max(0.0);
    if s < 60.0 { "just now".into() } else if s < 3600.0 { format!("{}m ago", (s / 60.0) as i64) } else if s < 86400.0 { format!("{}h ago", (s / 3600.0) as i64) } else { format!("{}d ago", (s / 86400.0) as i64) }
}

/// The pages shown: pages or trash, matching the search, newest first as the server sends.
pub fn filter(pages: &[Value], trash: Option<bool>, q: &str) -> Vec<Value> {
    let q = q.trim().to_lowercase();
    pages.iter().filter(|p| {
        let del = p.get("deleted").and_then(|d| d.as_bool()).unwrap_or(false);
        if let Some(t) = trash { if t != del { return false; } }
        let s = |k: &str| p.get(k).and_then(|x| x.as_str()).unwrap_or("").to_lowercase();
        q.is_empty() || s("title").contains(&q) || s("preview").contains(&q)
    }).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_round_trip() {
        let h = "Hi <strong>bold <em>both</em></strong> and <a href=\"https://x.io/a\" target=\"_blank\">link</a><br>next &amp; <code>c*d</code>";
        let src = html_to_src(h);
        assert_eq!(src, "Hi **bold *both*** and [link](https://x.io/a)\nnext & `c*d`");
        let back = src_to_html(&src);
        assert_eq!(html_to_src(&back), src);
        assert!(back.contains("<strong>bold <em>both</em></strong>"));
        assert!(back.contains("<br>"));
    }

    #[test]
    fn literals_are_escaped() {
        let r = vec![Run { text: "2*3 = [x] a\\b __init__".into(), st: Sty::default() }];
        let s = runs_to_src(&r);
        assert_eq!(src_to_runs(&s), r);
        assert_eq!(src_plain("a \\*b\\*"), "a *b*");
    }

    #[test]
    fn unclosed_marks_stay_text() {
        assert_eq!(src_plain("**bo"), "**bo");
        assert_eq!(src_to_runs("a * b *").len(), 1);
        assert_eq!(src_to_html("~~gone~~ __u__"), "<s>gone</s> <u>u</u>");
        // a link that is not http stays text
        assert_eq!(src_to_html("[x](javascript:alert)"), "x");
    }

    #[test]
    fn unknown_tags_unwrapped() {
        assert_eq!(html_to_src("<span style=x>a</span><script>bad()</script><div>b</div>"), "a\nb");
        assert_eq!(html_to_src("<b>x</b><i>y</i><del>z</del>"), "**x***y*~~z~~");
    }

    #[test]
    fn maps() {
        let (s, m) = runs_to_src_map(&src_to_runs("a **b** c"));
        assert_eq!(s, "a **b** c");
        assert_eq!(m, vec![0, 1, 4, 7, 8, 9]);
        assert_eq!(src_to_plain_at("a **b** c", 4), 2);
        assert_eq!(src_to_plain_at("a **b** c", 9), 5);
    }

    #[test]
    fn toggling() {
        let r = src_to_runs("hello world");
        let b = toggle(&r, 0, 5, Mark::B);
        assert_eq!(runs_to_src(&b), "**hello** world");
        assert_eq!(runs_to_src(&toggle(&b, 0, 5, Mark::B)), "hello world");
        assert_eq!(runs_to_src(&toggle(&b, 3, 8, Mark::B)), "**hello wo**rld");
        let l = link(&r, 6, 11, Some("https://a.b".into()));
        assert_eq!(runs_to_src(&l), "hello [world](https://a.b)");
        assert_eq!(link_at(&l, 7, 7), "https://a.b");
        assert_eq!(runs_to_src(&link(&l, 6, 11, None)), "hello world");
    }

    #[test]
    fn splitting() {
        let r = src_to_runs("ab **cd** ef");
        let (a, b) = split_runs(&r, 4);
        assert_eq!(runs_to_src(&a), "ab **c**");
        assert_eq!(runs_to_src(&b), "**d** ef");
    }

    #[test]
    fn markdown_export() {
        let b = vec![
            Block { kind: "h1".into(), text: "Title".into(), ..Default::default() },
            Block { kind: "todo".into(), text: "do **it**".into(), checked: true, ..Default::default() },
            Block { kind: "number".into(), text: "one".into(), ..Default::default() },
            Block { kind: "number".into(), text: "two".into(), ..Default::default() },
            Block { kind: "code".into(), text: "x = 1".into(), lang: "python".into(), ..Default::default() },
            Block { kind: "table".into(), rows: vec![vec!["a".into(), "b|c".into()], vec!["1".into(), "2".into()]], ..Default::default() },
            Block { kind: "divider".into(), ..Default::default() },
        ];
        assert_eq!(to_markdown(&b), "# Title\n\n- [x] do **it**\n1. one\n2. two\n\n```python\nx = 1\n```\n\n| a | b\\|c |\n| --- | --- |\n| 1 | 2 |\n\n---");
        assert_eq!(numbers(&b), vec!["", "", "1.", "2.", "", "", ""]);
    }

    #[test]
    fn shortcuts() {
        assert_eq!(shortcut("p", "# "), Some(Shortcut::To("h1", false)));
        assert_eq!(shortcut("p", "\\[x] "), Some(Shortcut::To("todo", true)));
        assert_eq!(shortcut("p", "- "), Some(Shortcut::To("bullet", false)));
        assert_eq!(shortcut("p", "---"), Some(Shortcut::Divider));
        assert_eq!(shortcut("h1", "---"), None);
        assert_eq!(shortcut("p", "```js "), Some(Shortcut::Code("javascript".into())));
        assert_eq!(shortcut("p", "#x"), None);
    }

    #[test]
    fn slash() {
        assert!(slash_opens("/", 1));
        assert!(slash_opens("a /", 3));
        assert!(!slash_opens("a/", 2));
        assert_eq!(slash_query("x /hea", 2, 6), Some("hea".into()));
        assert_eq!(slash_query("x /he a", 2, 7), None);
        assert_eq!(slash_items("head").len(), 3);
        assert_eq!(slash_items("to")[0].0, "todo");
    }

    #[test]
    fn json_blocks() {
        let v = json!([{ "id": "a", "type": "todo", "text": "<b>x</b>", "checked": true }, { "type": "weird", "text": "y" }, { "id": "t", "type": "table", "rows": [["<i>a</i>", "b"]], "header": true }]);
        let b = blocks_from(&v);
        assert_eq!(b[0].text, "**x**");
        assert!(b[0].checked);
        assert_eq!(b[1].kind, "p");
        assert_eq!(b[2].rows[0][0], "*a*");
        assert_eq!(b[0].to_json()["text"], "<strong>x</strong>");
        assert_eq!(b[2].to_json()["rows"][0][0], "<em>a</em>");
        assert!(blocks_from(&json!([])).len() == 1);
    }

    #[test]
    fn convert_keeps_text() {
        let p = Block { id: "x".into(), kind: "p".into(), text: "**a** *b*".into(), ..Default::default() };
        assert_eq!(p.converted("code").text, "a b");
        assert_eq!(p.converted("h2").text, "**a** *b*");
        assert_eq!(p.converted("table").rows.len(), 3);
        let c = Block { id: "x".into(), kind: "code".into(), text: "a*b".into(), ..Default::default() };
        assert_eq!(c.converted("p").text, "a\\*b");
    }

    #[test]
    fn styled_markup() {
        assert_eq!(styled("a **b**", false, false), "a \u{200b}**b**\u{200b}");
        assert_eq!(styled("", false, false), "\u{a0}");
        assert_eq!(styled("x\n\ny", false, false), "x\n\u{a0}\ny");
        assert_eq!(styled("done", true, false), "~~done~~");
        assert_eq!(styled("Head", false, true), "\u{200b}**Head**\u{200b}");
        assert!(styled("[l](https://a.b)", false, false).contains("[l](https://a.b)"));
        assert_eq!(styled_plain("a\\*"), "a\\*");
    }

    #[test]
    fn linkified() {
        let r = linkify("see https://a.io/x. ok");
        assert_eq!(runs_to_src(&r), "see [https://a.io/x](https://a.io/x). ok");
        assert!(is_url(" https://a.io/x "));
        assert_eq!(fix_url("a.io"), "https://a.io");
    }

    #[test]
    fn langs() {
        assert_eq!(norm_lang("sh"), "bash");
        assert_eq!(norm_lang("Python"), "python");
        assert_eq!(norm_lang(""), "plain");
        assert_eq!(lang_name("plain"), "Plain text");
        assert_eq!(lang_items("java").len(), 2);
    }

    #[test]
    fn list_filter_and_ago() {
        let p = vec![json!({ "id": "a", "title": "Alpha", "preview": "x" }), json!({ "id": "b", "title": "Beta", "deleted": true })];
        assert_eq!(filter(&p, Some(false), "").len(), 1);
        assert_eq!(filter(&p, Some(true), "").len(), 1);
        assert_eq!(filter(&p, Some(false), "zz").len(), 0);
        assert_eq!(ago(1000.0, 990.0), "just now");
        assert_eq!(ago(10000.0, 10000.0 - 7200.0), "2h ago");
    }

    #[test]
    fn ids() { let a = gen_id(); assert_eq!(a.len(), 16); assert_ne!(a, gen_id()); }
}
