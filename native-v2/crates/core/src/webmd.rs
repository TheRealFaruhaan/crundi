//! Claude's reply as the web draws it: a port of app/vendor/claude-chat.js's
//! `md()`, `inline()` and `linkify()` (the web and Electron client's chat),
//! cut into blocks so the chat log makes each block its own list row
//! (ARCHITECTURE.md, "Virtualised lists"). Pure: no GPUI.
//!
//! The web's rules, kept as they are:
//! - fenced code (```` ```lang ````…```` ``` ````) is taken out first, anywhere
//!   in the text; an unclosed fence is text until it closes;
//! - then line by line: a blank line ends a paragraph and a list; `#`…`######`
//!   are headings (h3…h6: `#` 14 px bold, `##` bold at body size, `###`+
//!   smaller, the browser's own h4-h6); `---`, `***`, `___` a rule; `>` a
//!   quote line; `-`/`*`/`+` and `1.`/`1)` list items, indented or not (a
//!   nested item is drawn flat, as the web does), numbered by their place
//!   in their list; any other line joins the paragraph, its break kept;
//! - inline: code spans, `[text](http…)` links, `**bold**`, `*italic*`, bare
//!   URLs and file paths (absolute, `./` `../`, or with a known extension).
//!
//! One thing the web does not have, by the owner's call (F19, 19 Nov): a
//! GFM table (a header row, a `|---|` row, its rows) is drawn as a table,
//! not as its pipe text; and a code block's text is coloured by its
//! language (core::syntax).
//!
//! Streaming: `update` parses again only from the last point where the
//! parser stood with nothing open (no paragraph, no list, no unclosed
//! fence after it); the blocks before it cannot change when text is added.

use crate::syntax::{self, Syn};
use std::ops::Range;

pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;
pub const CODE: u8 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link { Url(String), Path(String) }

/// A styled run of an inline text (what is not in a run is plain).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span { pub range: Range<usize>, pub flags: u8, pub link: Option<Link>, pub syn: Option<Syn> }

/// A piece of text as drawn: its characters (line breaks as '\n') and runs.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Inline { pub text: String, pub spans: Vec<Span> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Para,
    /// The HTML level the web gives it (3 for `#` … 6 for `####`+).
    Heading(u8),
    Rule,
    Quote,
    /// A list item: numbered or not, its number, first and last of its list.
    Item { ordered: bool, n: u32, first: bool, last: bool },
    Code,
    /// A table (the owner's call): `cols` columns, `rows` rows with the header.
    Table { cols: usize, rows: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// Its source in the reply's text.
    pub src: Range<usize>,
    pub kind: Kind,
    /// Its text: one piece (a table: each cell, row by row; a rule: none).
    pub parts: Vec<Inline>,
}

/// A reply cut into blocks, and where a longer text would be parsed again.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Cut {
    pub blocks: Vec<Block>,
    /// Where to parse again: a line start with no paragraph open, nor an
    /// unclosed fence before it; and the list open there (numbered, items).
    pub resume: usize,
    pub list: Option<(bool, u32)>,
}

pub fn split(text: &str) -> Cut { parse_from(text, 0, vec![], None) }

/// `prev` was cut from `prev_text`; `text` is it with more added (or
/// anything else: then all of it is cut again).
pub fn update(prev: &Cut, prev_text: &str, text: &str) -> Cut {
    let r = prev.resume;
    if r > text.len() || prev_text.get(..r).is_none() || text.get(..r) != prev_text.get(..r) { return split(text); }
    let keep: Vec<Block> = prev.blocks.iter().filter(|b| b.src.end <= r).cloned().collect();
    parse_from(text, r, keep, prev.list)
}

// ─── Lines ───

struct Fence { start: usize, end: usize, lang: String, body: Range<usize> }

/// The fences from `from` on (the web's regex, non-greedy), and where an
/// unclosed one starts.
fn fences(text: &str, from: usize) -> (Vec<Fence>, Option<usize>) {
    let b = text.as_bytes();
    let mut out = vec![];
    let mut i = from;
    while let Some(k) = text[i..].find("```") {
        let s = i + k;
        let mut j = s + 3;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b"_+-".contains(&b[j])) { j += 1; }
        let lang = text[s + 3..j].to_string();
        if j < b.len() && b[j] == b'\n' { j += 1; }
        let Some(c) = text[j..].find("```") else { return (out, Some(s)) };
        let close = j + c;
        let body_end = if close > j && b[close - 1] == b'\n' { close - 1 } else { close };
        out.push(Fence { start: s, end: close + 3, lang, body: j..body_end.max(j) });
        i = close + 3;
    }
    (out, None)
}

enum LineKind { Text(String), Fence(usize) }
struct Line { src: Range<usize>, kind: LineKind }

/// The text's lines from `from` as the web splits them after taking the
/// fences out: a fence is part of the line it is on.
fn lines(text: &str, from: usize, fs: &[Fence]) -> Vec<Line> {
    let mut out = vec![];
    let mut pos = from;
    let mut f = 0;
    loop {
        let start = pos;
        let mut content = String::new();
        let mut only: Option<usize> = None;
        let mut pieces = 0;
        loop {
            let nl = text[pos..].find('\n').map(|k| pos + k).unwrap_or(text.len());
            match fs.get(f) {
                Some(fe) if fe.start < nl => {
                    if fe.start > pos { content.push_str(&text[pos..fe.start]); pieces += 1; }
                    content.push_str(&text[fe.start..fe.end].replace('\n', " "));
                    only = if pieces == 0 { Some(f) } else { None };
                    pieces += 1;
                    pos = fe.end;
                    f += 1;
                }
                _ => {
                    if nl > pos { content.push_str(&text[pos..nl]); pieces += 1; only = None; }
                    let kind = match only { Some(i) if pieces == 1 => LineKind::Fence(i), _ => LineKind::Text(content) };
                    out.push(Line { src: start..nl, kind });
                    if nl >= text.len() { return out; }
                    pos = nl + 1;
                    break;
                }
            }
        }
    }
}

// ─── Blocks ───

fn heading(l: &str) -> Option<(u8, &str)> {
    let n = l.bytes().take_while(|c| *c == b'#').count();
    if !(1..=6).contains(&n) { return None; }
    let rest = &l[n..];
    let t = rest.trim_start();
    if t.len() == rest.len() { return None; }
    Some(((n as u8 + 2).min(6), t))
}

fn rule(l: &str) -> bool {
    ["---", "***", "___"].iter().any(|p| l.strip_prefix(p).is_some_and(|r| r.trim().is_empty()))
}

fn quote(l: &str) -> Option<&str> {
    let r = l.strip_prefix('>')?;
    Some(r.strip_prefix(|c: char| c.is_whitespace()).unwrap_or(r))
}

fn after(r: &str) -> Option<&str> { let x = r.trim_start(); (x.len() < r.len()).then_some(x) }

/// `^\s*[-*+]\s+(.*)$` (false) or `^\s*\d+[.)]\s+(.*)$` (true).
fn item(l: &str) -> Option<(bool, &str)> {
    let t = l.trim_start();
    if let Some(r) = t.strip_prefix(['-', '*', '+']) { if let Some(x) = after(r) { return Some((false, x)); } }
    let d = t.bytes().take_while(|c| c.is_ascii_digit()).count();
    if d > 0 { if let Some(r) = t[d..].strip_prefix(['.', ')']) { if let Some(x) = after(r) { return Some((true, x)); } } }
    None
}

fn parse_from(text: &str, from: usize, mut blocks: Vec<Block>, list0: Option<(bool, u32)>) -> Cut {
    let (fs, open) = fences(text, from);
    let ls = lines(text, from, &fs);
    let mut neutral: Vec<(usize, Option<(bool, u32)>)> = vec![];
    let mut para: Vec<(Range<usize>, String)> = vec![];
    // The open list: ordered, how many items so far.
    let mut list: Option<(bool, u32)> = list0;
    // The list goes on here: its item before is not its last (yet).
    if list.is_some() { if let Some(Block { kind: Kind::Item { last, .. }, .. }) = blocks.last_mut() { *last = false; } }
    fn close_list(blocks: &mut [Block], list: &mut Option<(bool, u32)>) {
        if list.take().is_some() { if let Some(Block { kind: Kind::Item { last, .. }, .. }) = blocks.last_mut() { *last = true; } }
    }
    for l in &ls {
        if para.is_empty() { neutral.push((l.src.start, list)); }
        let ln = match &l.kind {
            LineKind::Fence(i) => {
                flush(&mut blocks, &mut para);
                close_list(&mut blocks, &mut list);
                let fe = &fs[*i];
                let code = &text[fe.body.clone()];
                let spans = syntax::highlight(&fe.lang, code).into_iter().map(|(range, s)| Span { range, flags: 0, link: None, syn: Some(s) }).collect();
                blocks.push(Block { src: l.src.clone(), kind: Kind::Code, parts: vec![Inline { text: code.to_string(), spans }] });
                continue;
            }
            LineKind::Text(t) => t.as_str(),
        };
        if ln.trim().is_empty() { flush(&mut blocks, &mut para); close_list(&mut blocks, &mut list); continue; }
        if let Some((lv, t)) = heading(ln) {
            flush(&mut blocks, &mut para); close_list(&mut blocks, &mut list);
            blocks.push(Block { src: l.src.clone(), kind: Kind::Heading(lv), parts: vec![inline(t)] });
            continue;
        }
        if rule(ln) { flush(&mut blocks, &mut para); close_list(&mut blocks, &mut list); blocks.push(Block { src: l.src.clone(), kind: Kind::Rule, parts: vec![] }); continue; }
        if let Some(q) = quote(ln) {
            flush(&mut blocks, &mut para); close_list(&mut blocks, &mut list);
            blocks.push(Block { src: l.src.clone(), kind: Kind::Quote, parts: vec![inline(q)] });
            continue;
        }
        if let Some((ordered, t)) = item(ln) {
            flush(&mut blocks, &mut para);
            if list.is_some_and(|(o, _)| o != ordered) { close_list(&mut blocks, &mut list); }
            let n = match &mut list { Some((_, n)) => { *n += 1; *n } None => { list = Some((ordered, 1)); 1 } };
            blocks.push(Block { src: l.src.clone(), kind: Kind::Item { ordered, n, first: n == 1, last: false }, parts: vec![inline(t)] });
            continue;
        }
        close_list(&mut blocks, &mut list);
        para.push((l.src.clone(), ln.to_string()));
    }
    flush(&mut blocks, &mut para);
    if let Some(Block { kind: Kind::Item { last, .. }, .. }) = blocks.last_mut() { if list.is_some() { *last = true; } }
    // Where to parse again: the last line start with no paragraph open
    // (every block before it is complete, but for a list item's being
    // last), not after an unclosed fence's line.
    let limit = open.and_then(|u| ls.iter().find(|l| l.src.end >= u).map(|l| l.src.start)).unwrap_or(usize::MAX);
    let (resume, list) = neutral.into_iter().filter(|n| n.0 <= limit).max_by_key(|n| n.0).unwrap_or((from, list0));
    Cut { blocks, resume, list }
}

/// A paragraph's lines: a paragraph (its breaks kept), or a table and the
/// paragraphs around it (the owner's call).
fn flush(blocks: &mut Vec<Block>, para: &mut Vec<(Range<usize>, String)>) {
    if para.is_empty() { return; }
    let lines = std::mem::take(para);
    let mut rest = &lines[..];
    while !rest.is_empty() {
        let head = (0..rest.len().saturating_sub(1)).find(|&i| rest[i].1.contains('|') && separator(&rest[i + 1].1));
        match head {
            Some(h) => {
                if h > 0 { blocks.push(para_block(&rest[..h])); }
                let mut end = h + 2;
                while end < rest.len() && rest[end].1.contains('|') { end += 1; }
                blocks.push(table(&rest[h..end]));
                rest = &rest[end..];
            }
            None => { blocks.push(para_block(rest)); break; }
        }
    }
}

fn para_block(ls: &[(Range<usize>, String)]) -> Block {
    let joined = ls.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n");
    Block { src: ls[0].0.start..ls[ls.len() - 1].0.end, kind: Kind::Para, parts: vec![inline(&joined)] }
}

fn separator(l: &str) -> bool {
    let t = l.trim();
    t.contains('|') && t.contains('-') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

fn cells(l: &str) -> Vec<String> {
    let t = l.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    let mut out = vec![String::new()];
    let mut esc = false;
    for c in t.chars() {
        if esc { out.last_mut().unwrap().push(c); esc = false; continue; }
        match c { '\\' => esc = true, '|' => out.push(String::new()), _ => out.last_mut().unwrap().push(c) }
    }
    out.into_iter().map(|c| c.trim().to_string()).collect()
}

fn table(ls: &[(Range<usize>, String)]) -> Block {
    let header = cells(&ls[0].1);
    let cols = header.len().max(1);
    let mut parts: Vec<Inline> = header.iter().map(|c| inline(c)).collect();
    parts.resize(cols, Inline::default());
    for (_, l) in &ls[2..] {
        let mut row: Vec<Inline> = cells(l).iter().take(cols).map(|c| inline(c)).collect();
        row.resize(cols, Inline::default());
        parts.extend(row);
    }
    Block { src: ls[0].0.start..ls[ls.len() - 1].0.end, kind: Kind::Table { cols, rows: parts.len() / cols }, parts }
}

// ─── Inline ───

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tk { C(char), Br, Held(usize), BO, BC, IO, IC }

enum Held { Code(String, bool), Link(String, String), Url(String), Path(String) }

impl Tk {
    fn is(self, c: char) -> bool { self == Tk::C(c) }
    /// What a tag's '>' stands for in the web's escaped HTML (a path may follow it).
    fn tag(self) -> bool { matches!(self, Tk::Br | Tk::BO | Tk::BC | Tk::IO | Tk::IC) }
}

fn text_of(ts: &[Tk], held: &[Held]) -> String {
    ts.iter().map(|t| match *t { Tk::C(c) => c.to_string(), Tk::Br => "\n".into(), Tk::Held(i) => held_text(&held[i]).to_string(), _ => String::new() }).collect()
}

fn held_text(h: &Held) -> &str { match h { Held::Code(t, _) | Held::Link(t, _) | Held::Url(t) | Held::Path(t) => t } }

/// The web's inline(): code spans and links set aside, bold, italic, then
/// bare URLs and paths.
pub fn inline(s: &str) -> Inline {
    let mut ts: Vec<Tk> = s.chars().map(|c| if c == '\n' { Tk::Br } else { Tk::C(c) }).collect();
    let mut held: Vec<Held> = vec![];
    // `([^`]+)`
    let mut out = Vec::with_capacity(ts.len());
    let mut i = 0;
    while i < ts.len() {
        if ts[i].is('`') {
            if let Some(k) = ts[i + 1..].iter().position(|t| t.is('`')).filter(|&k| k > 0) {
                let c = text_of(&ts[i + 1..i + 1 + k], &held);
                let path = crate::md::code_path(&c);
                held.push(Held::Code(c, path));
                out.push(Tk::Held(held.len() - 1));
                i += k + 2;
                continue;
            }
        }
        out.push(ts[i]);
        i += 1;
    }
    ts = out;
    // \[([^\]]+)\]\((https?:[^)\s]+)\)
    let mut out = Vec::with_capacity(ts.len());
    let mut i = 0;
    while i < ts.len() {
        if ts[i].is('[') {
            if let Some(m) = link_at(&ts, i) {
                let (t, u, end) = m;
                held.push(Held::Link(text_of(&ts[t.clone()], &held), text_of(&ts[u], &held)));
                out.push(Tk::Held(held.len() - 1));
                i = end;
                continue;
            }
        }
        out.push(ts[i]);
        i += 1;
    }
    ts = out;
    // \*\*([^*]+)\*\*
    let mut out = Vec::with_capacity(ts.len());
    let mut i = 0;
    while i < ts.len() {
        if ts[i].is('*') && ts.get(i + 1).is_some_and(|t| t.is('*')) {
            if let Some(k) = ts[i + 2..].iter().position(|t| t.is('*')).filter(|&k| k > 0) {
                let close = i + 2 + k;
                if ts.get(close + 1).is_some_and(|t| t.is('*')) {
                    out.push(Tk::BO);
                    out.extend_from_slice(&ts[i + 2..close]);
                    out.push(Tk::BC);
                    i = close + 2;
                    continue;
                }
            }
        }
        out.push(ts[i]);
        i += 1;
    }
    ts = out;
    // (^|[\s(])\*([^*\n]+)\*
    let mut out = Vec::with_capacity(ts.len());
    let mut i = 0;
    let em = |ts: &[Tk], star: usize| -> Option<usize> {
        if !ts.get(star).is_some_and(|t| t.is('*')) { return None; }
        let k = ts[star + 1..].iter().position(|t| t.is('*'))?;
        (k > 0).then_some(star + 1 + k)
    };
    while i < ts.len() {
        if i == 0 { if let Some(close) = em(&ts, 0) { out.push(Tk::IO); out.extend_from_slice(&ts[1..close]); out.push(Tk::IC); i = close + 1; continue; } }
        if matches!(ts[i], Tk::C(c) if c.is_whitespace() || c == '(') {
            if let Some(close) = em(&ts, i + 1) {
                out.push(ts[i]); out.push(Tk::IO); out.extend_from_slice(&ts[i + 2..close]); out.push(Tk::IC);
                i = close + 1;
                continue;
            }
        }
        out.push(ts[i]);
        i += 1;
    }
    ts = out;
    linkify(&mut ts, &mut held);
    build(&ts, &held)
}

/// `[text](http(s):…)` at `i`: the text's and the URL's tokens, and the end.
fn link_at(ts: &[Tk], i: usize) -> Option<(Range<usize>, Range<usize>, usize)> {
    let k = ts[i + 1..].iter().position(|t| t.is(']'))?;
    if k == 0 { return None; }
    let close = i + 1 + k;
    if !ts.get(close + 1)?.is('(') { return None; }
    let u0 = close + 2;
    let starts = |p: &str| p.chars().enumerate().all(|(j, c)| ts.get(u0 + j).is_some_and(|t| t.is(c)));
    let skip = if starts("https:") { 6 } else if starts("http:") { 5 } else { return None };
    let mut j = u0 + skip;
    while j < ts.len() && matches!(ts[j], Tk::C(c) if c != ')' && !c.is_whitespace()) { j += 1; }
    if j == u0 + skip || !ts.get(j)?.is(')') { return None; }
    Some((i + 1..close, u0..j, j + 1))
}

const FILE_EXT: &[&str] = &["js", "mjs", "cjs", "ts", "tsx", "jsx", "json", "md", "txt", "log", "py", "sh", "bash", "yml", "yaml", "toml", "ini", "conf", "cfg", "env", "html", "htm", "css", "scss", "xml", "svg", "png", "jpg", "jpeg", "gif", "webp", "ico", "pdf", "csv", "sql", "go", "rs", "rb", "php", "java", "kt", "kts", "gradle", "swift", "c", "h", "cc", "cpp", "hpp", "vue", "lock", "dockerfile", "service"];

fn seg(t: Tk) -> bool { matches!(t, Tk::C(c) if c.is_ascii_alphanumeric() || "_.@+~%,=-".contains(c)) }
fn trail(c: char) -> bool { ".,;:!?)]'\">".contains(c) }

/// `(?:seg/)+seg(?::\d+(?::\d+)?)?` from `j`: its end.
fn path_end(ts: &[Tk], mut j: usize) -> Option<usize> {
    let mut segs = 0;
    let mut end = j;
    loop {
        let s = j;
        while j < ts.len() && seg(ts[j]) { j += 1; }
        if j == s { break; }
        segs += 1;
        end = j;
        if ts.get(j).is_some_and(|t| t.is('/')) && ts.get(j + 1).is_some_and(|t| seg(*t)) { j += 1; } else { break; }
    }
    if segs < 2 { return None; }
    let digits = |k: usize| { let mut e = k; while ts.get(e).is_some_and(|t| matches!(t, Tk::C(c) if c.is_ascii_digit())) { e += 1; } e };
    for _ in 0..2 {
        if ts.get(end).is_some_and(|t| t.is(':')) { let e = digits(end + 1); if e > end + 1 { end = e; continue; } }
        break;
    }
    Some(end)
}

fn path_prefix(ts: &[Tk], i: usize) -> bool {
    i == 0 || match ts[i - 1] { Tk::C(c) => c.is_whitespace() || "([>\"'".contains(c), t => t.tag() }
}

fn linkify(ts: &mut Vec<Tk>, held: &mut Vec<Held>) {
    let chars = |ts: &[Tk], a: usize, b: usize| -> String { ts[a..b].iter().map(|t| if let Tk::C(c) = t { *c } else { ' ' }).collect() };
    let trim = |ts: &[Tk], a: usize, mut b: usize| { while b > a && matches!(ts[b - 1], Tk::C(c) if trail(c)) { b -= 1; } b };
    // \bhttps?:\/\/[^\s<]+
    let mut out = Vec::with_capacity(ts.len());
    let mut i = 0;
    while i < ts.len() {
        let word_before = i > 0 && matches!(ts[i - 1], Tk::C(c) if c.is_ascii_alphanumeric() || c == '_');
        let head = chars(ts, i, (i + 8).min(ts.len()));
        if !word_before && (head.starts_with("http://") || head.starts_with("https://")) {
            let mut j = i;
            while j < ts.len() && matches!(ts[j], Tk::C(c) if !c.is_whitespace() && c != '<') { j += 1; }
            let e = trim(ts, i, j);
            if e > i { held.push(Held::Url(chars(ts, i, e))); out.push(Tk::Held(held.len() - 1)); out.extend_from_slice(&ts[e..j]); i = j; continue; }
        }
        out.push(ts[i]);
        i += 1;
    }
    *ts = out;
    for absolute in [true, false] {
        let mut out = Vec::with_capacity(ts.len());
        let mut i = 0;
        while i < ts.len() {
            if path_prefix(ts, i) && (if absolute { ts[i].is('/') } else { ts[i] != Tk::C('/') }) {
                let from = if absolute { i + 1 } else if chars(ts, i, (i + 3).min(ts.len())) == "../" { i + 3 } else if chars(ts, i, (i + 2).min(ts.len())) == "./" { i + 2 } else { i };
                if let Some(end) = path_end(ts, from) {
                    let e = trim(ts, i, end);
                    let p = chars(ts, i, e);
                    let bare = crate::md::text_path_bare(&p);
                    let ok = absolute || bare.starts_with("./") || bare.starts_with("../") || bare.rsplit_once('.').is_some_and(|(_, x)| FILE_EXT.contains(&x.to_ascii_lowercase().as_str()));
                    if ok && e > i { held.push(Held::Path(p)); out.push(Tk::Held(held.len() - 1)); out.extend_from_slice(&ts[e..end]); }
                    else { out.extend_from_slice(&ts[i..end]); }
                    i = end;
                    continue;
                }
            }
            out.push(ts[i]);
            i += 1;
        }
        *ts = out;
    }
}

fn build(ts: &[Tk], held: &[Held]) -> Inline {
    let mut text = String::new();
    let mut spans: Vec<Span> = vec![];
    let (mut b, mut it) = (0u8, 0u8);
    let mut push = |text: &mut String, s: &str, flags: u8, link: Option<Link>| {
        let start = text.len();
        text.push_str(s);
        if (flags == 0 && link.is_none()) || s.is_empty() { return; }
        match spans.last_mut() {
            Some(l) if l.range.end == start && l.flags == flags && l.link == link && link.is_none() => l.range.end = text.len(),
            _ => spans.push(Span { range: start..text.len(), flags, link, syn: None }),
        }
    };
    for t in ts {
        let f = b | it;
        match *t {
            Tk::C(c) => { let mut buf = [0u8; 4]; push(&mut text, c.encode_utf8(&mut buf), f, None) }
            Tk::Br => push(&mut text, "\n", 0, None),
            Tk::BO => b = BOLD, Tk::BC => b = 0, Tk::IO => it = ITALIC, Tk::IC => it = 0,
            Tk::Held(i) => match &held[i] {
                Held::Code(c, path) => push(&mut text, c, f | CODE, path.then(|| Link::Path(c.clone()))),
                Held::Link(t, u) => push(&mut text, t, f, Some(Link::Url(u.clone()))),
                Held::Url(u) => push(&mut text, u, f, Some(Link::Url(u.clone()))),
                Held::Path(p) => push(&mut text, p, f, Some(Link::Path(p.clone()))),
            },
        }
    }
    Inline { text, spans }
}

/// The user's bubble (the web's `linkify(esc(text))`): the text as typed,
/// its URLs and paths as links.
pub fn plain(s: &str) -> Inline {
    let mut ts: Vec<Tk> = s.chars().map(|c| if c == '\n' { Tk::Br } else { Tk::C(c) }).collect();
    let mut held = vec![];
    linkify(&mut ts, &mut held);
    build(&ts, &held)
}

/// A block's text for copying (a table: its cells by tabs and lines).
pub fn copy_text(b: &Block) -> String {
    match &b.kind {
        Kind::Table { cols, .. } => b.parts.chunks(*cols).map(|r| r.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n"),
        _ => b.parts.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(t: &str) -> Vec<Kind> { split(t).blocks.into_iter().map(|b| b.kind).collect() }
    fn texts(t: &str) -> Vec<String> { split(t).blocks.iter().map(|b| b.parts.first().map(|p| p.text.clone()).unwrap_or_default()).collect() }

    #[test]
    fn blocks_as_the_web_cuts_them() {
        let t = "Intro line\nsecond line\n\n# Big\n## Mid\n### Small\n#### Tiny\n---\n> q one\n> q two\n- a\n  - nested\n* b\n1. x\n7. y\n\n```js\nlet a = 1;\n```\nAfter.";
        assert_eq!(kinds(t), vec![
            Kind::Para, Kind::Heading(3), Kind::Heading(4), Kind::Heading(5), Kind::Heading(6), Kind::Rule, Kind::Quote, Kind::Quote,
            Kind::Item { ordered: false, n: 1, first: true, last: false }, Kind::Item { ordered: false, n: 2, first: false, last: false }, Kind::Item { ordered: false, n: 3, first: false, last: true },
            Kind::Item { ordered: true, n: 1, first: true, last: false }, Kind::Item { ordered: true, n: 2, first: false, last: true },
            Kind::Code, Kind::Para,
        ]);
        let tx = texts(t);
        assert_eq!(tx[0], "Intro line\nsecond line");
        assert_eq!(tx[9], "nested");
        assert_eq!(tx[13], "let a = 1;");
        // A blank line closes a list: the next one counts from 1 again.
        assert_eq!(kinds("1. a\n\n2. b").iter().filter(|k| matches!(k, Kind::Item { n: 1, .. })).count(), 2);
        // "#x" is no heading; an unclosed fence is text.
        assert_eq!(kinds("#x\n```js\nlet"), vec![Kind::Para]);
    }

    #[test]
    fn inline_as_the_web_styles_it() {
        let i = inline("a **bold** and *it* and `code` [site](https://x.io/a) see https://y.org/p. or /etc/hosts/x, ./a/b and src/main.rs:12 not and/or");
        let styled: Vec<(&str, u8, Option<&Link>)> = i.spans.iter().map(|s| (&i.text[s.range.clone()], s.flags, s.link.as_ref())).collect();
        assert!(styled.contains(&("bold", BOLD, None)));
        assert!(styled.contains(&("it", ITALIC, None)));
        assert!(styled.contains(&("code", CODE, None)));
        assert!(styled.contains(&("site", 0, Some(&Link::Url("https://x.io/a".into())))));
        assert!(styled.contains(&("https://y.org/p", 0, Some(&Link::Url("https://y.org/p".into())))));
        assert!(styled.contains(&("/etc/hosts/x", 0, Some(&Link::Path("/etc/hosts/x".into())))));
        assert!(styled.contains(&("./a/b", 0, Some(&Link::Path("./a/b".into())))));
        assert!(styled.contains(&("src/main.rs:12", 0, Some(&Link::Path("src/main.rs:12".into())))));
        assert!(!styled.iter().any(|s| s.0 == "and/or"));
        assert!(!i.text.contains('*') && !i.text.contains('`'));
        // Italic needs a space or "(" before it (or the start).
        assert!(inline("a*b*").spans.is_empty());
        assert_eq!(inline("*x*").spans[0].flags, ITALIC);
    }

    #[test]
    fn tables_are_drawn() {
        let b = split("Before\n| a | b |\n|---|:-:|\n| 1 | 2 |\n| 3 |\nAfter").blocks;
        assert_eq!(b.iter().map(|b| b.kind.clone()).collect::<Vec<_>>(), vec![Kind::Para, Kind::Table { cols: 2, rows: 3 }, Kind::Para]);
        assert_eq!(b[1].parts.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), vec!["a", "b", "1", "2", "3", ""]);
        assert_eq!(copy_text(&b[1]), "a\tb\n1\t2\n3\t");
    }

    #[test]
    fn streaming_gives_what_a_whole_cut_gives() {
        let full = "Here:\n\n1. one **b**\n2. two\n\n```rust\nfn a() {}\n\nfn b() {}\n```\n\n| h | i |\n|---|---|\n| 1 | 2 |\n\n## Head\ntext `x` and /a/b/c\n- l\n- m\n  - n\n> q\n\n1. x\n2. y\n3. z\nEnd.";
        for step in [1, 3, 7, 23] {
            let mut cut = split("");
            let mut sent = 0;
            while sent < full.len() {
                let mut end = (sent + step).min(full.len());
                while !full.is_char_boundary(end) { end += 1; }
                let prev = &full[..sent];
                cut = update(&cut, prev, &full[..end]);
                sent = end;
                assert_eq!(cut.blocks, split(&full[..sent]).blocks, "step {step} at {sent}");
                assert!(cut.resume <= sent);
            }
        }
        // Only the tail is parsed again: the resume point moves on.
        assert!(split(full).resume > full.find("## Head").unwrap());
        // An open list: from its last item, not from its first.
        let list: String = (1..=300).map(|i| format!("{i}. n\n")).collect();
        let c = split(&list);
        assert!(c.resume > list.len() - 10, "{}", c.resume);
        assert_eq!(c.list, Some((true, 300)));
    }

    #[test]
    fn the_users_text_links_only() {
        let p = plain("see **this** https://a.b/c and /x/y");
        assert!(p.text.contains("**this**"));
        assert_eq!(p.spans.len(), 2);
    }
}
