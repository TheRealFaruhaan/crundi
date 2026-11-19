//! A reply's markdown cut into blocks, so the chat log can make each block
//! its own list row (ARCHITECTURE.md, "Virtualised lists": the log draws only
//! the blocks on screen, and while a reply streams only its last block
//! changes). Pure: the byte ranges of the source, and what each block needs
//! around it so that the blocks drawn one under another look exactly as the
//! whole document drawn by one TextView (gpui-component 0.10's flow rules,
//! `node.rs` `render_block`: a paragraph gap below a block unless it is the
//! last; a list's items 0.25 of the body size apart; a heading's and a
//! rule's room above them, less the gap the block before leaves).
//!
//! Where the document is cut:
//! - between two top-level blocks that are prose, lists, quotes, code or
//!   tables (their own room is only the gap below them);
//! - before a heading or a rule after such a block (the row then adds the
//!   heading's or rule's room above it, `Lead`), never after one (what
//!   follows a heading depends on it: they stay together);
//! - between the items of a top-level list, where an ordered item's number
//!   is the one the list would show (an item cut off renders as a list of its
//!   own, starting at its own number);
//! - never around anything else (HTML, math, front matter): it stays with
//!   its neighbours. A document with link or footnote references or
//!   definitions is not cut at all (a block alone would lose them).
//!
//! While a reply streams, `update` re-cuts only from the start of the last
//! block: what came before it does not change when text is appended.

use markdown::mdast::Node;
use std::ops::Range;

/// What a block's row adds above it (the room the whole document would have
/// there, beyond the paragraph gap the previous row leaves below itself).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lead {
    /// The first block, or one after a paragraph gap: nothing.
    #[default]
    None,
    /// The next item of the list the previous block is part of: 0.25 of the
    /// body size, and the previous row leaves no gap.
    ListItem,
    /// A heading of this level: its section gap less the paragraph gap.
    Heading(u8),
    /// A horizontal rule: its room above, less the paragraph gap.
    Rule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The block's source, in the reply's text (trailing blank lines left out,
    /// except the last block's, which runs to the end).
    pub range: Range<usize>,
    pub lead: Lead,
    /// A paragraph gap below it (false for the last block, and before the
    /// next item of the same list).
    pub gap: bool,
}

/// The parse options gpui-component's TextView parses markdown with
/// (`MarkdownExtensions::parse_options`, the defaults).
fn options() -> markdown::ParseOptions {
    let mut o = markdown::ParseOptions::gfm();
    o.constructs.math_text = true;
    o.constructs.math_flow = true;
    o
}

#[derive(Clone, Copy, PartialEq)]
enum Class { Flow, Heading(u8), Rule, Glue }

fn class(n: &Node) -> Class {
    match n {
        Node::Paragraph(_) | Node::List(_) | Node::Blockquote(_) | Node::Code(_) | Node::Table(_) => Class::Flow,
        Node::Heading(h) => Class::Heading(h.depth),
        Node::ThematicBreak(_) => Class::Rule,
        _ => Class::Glue,
    }
}

fn has_refs(n: &Node) -> bool {
    match n {
        Node::Definition(_) | Node::FootnoteDefinition(_) | Node::FootnoteReference(_) | Node::LinkReference(_) | Node::ImageReference(_) => true,
        _ => n.children().is_some_and(|c| c.iter().any(has_refs)),
    }
}

fn start_of(n: &Node) -> Option<usize> { n.position().map(|p| p.start.offset) }

/// The number an ordered item starts with in the source ("12." → 12).
fn number_at(text: &str, at: usize) -> Option<u64> {
    let s = text.get(at..)?.trim_start();
    let d: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    d.parse().ok()
}

/// The whole text cut into blocks (always at least one).
pub fn split(text: &str) -> Vec<Block> {
    let whole = || vec![Block { range: 0..text.len(), lead: Lead::None, gap: false }];
    let Ok(root) = markdown::to_mdast(text, &options()) else { return whole() };
    if has_refs(&root) { return whole(); }
    let Some(top) = root.children() else { return whole() };
    // The places a block may start: (offset, lead).
    let mut cuts: Vec<(usize, Lead)> = vec![];
    let mut prev: Option<Class> = None;
    for n in top {
        let c = class(n);
        let Some(at) = start_of(n) else { prev = Some(Class::Glue); continue };
        if prev == Some(Class::Flow) {
            match c {
                Class::Flow => cuts.push((at, Lead::None)),
                Class::Heading(l) => cuts.push((at, Lead::Heading(l))),
                Class::Rule => cuts.push((at, Lead::Rule)),
                Class::Glue => {}
            }
        }
        if let Node::List(l) = n {
            let first = l.start.unwrap_or(1) as u64;
            for (j, item) in l.children.iter().enumerate().skip(1) {
                let Some(at) = start_of(item) else { continue };
                if l.ordered && number_at(text, at) != Some(first + j as u64) { continue; }
                cuts.push((at, Lead::ListItem));
            }
        }
        prev = Some(c);
    }
    let mut out: Vec<Block> = Vec::with_capacity(cuts.len() + 1);
    let mut from = (0usize, Lead::None);
    for &(at, lead) in &cuts {
        if at <= from.0 { continue; }
        let end = from.0 + text[from.0..at].trim_end().len();
        out.push(Block { range: from.0..end, lead: from.1, gap: lead != Lead::ListItem });
        from = (at, lead);
    }
    out.push(Block { range: from.0..text.len(), lead: from.1, gap: false });
    out
}

/// The blocks of `new`, given those of `old` (`old_blocks`): when `new`
/// extends what came before the last two blocks (a streamed reply), only the
/// text from the start of the one before the last is cut again (with the
/// block before it in the parse, the last block's lead and the one before's
/// gap follow what the last block turns out to be: an empty item becoming a
/// rule, say); otherwise all of it.
pub fn update(old_blocks: &[Block], old: &str, new: &str) -> Vec<Block> {
    let n = old_blocks.len();
    if n < 3 { return split(new); }
    let anchor = &old_blocks[n - 2];
    let from = anchor.range.start;
    if new.len() < from || old.get(..from) != new.get(..from) { return split(new); }
    let tail = &new[from..];
    // A reference showing up in the tail means the document is not cut at all.
    if markdown::to_mdast(tail, &options()).map(|r| has_refs(&r)).unwrap_or(true) { return split(new); }
    let mut out = old_blocks[..n - 2].to_vec();
    for (i, mut b) in split(tail).into_iter().enumerate() {
        b.range = b.range.start + from..b.range.end + from;
        if i == 0 { b.lead = anchor.lead; }
        out.push(b);
    }
    out
}

/// The block ranges as text (tests, the driver).
pub fn texts<'a>(text: &'a str, blocks: &[Block]) -> Vec<&'a str> {
    blocks.iter().map(|b| &text[b.range.clone()]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut(t: &str) -> Vec<(&str, Lead, bool)> {
        let b = split(t);
        b.iter().map(|b| (&t[b.range.clone()], b.lead, b.gap)).collect()
    }

    #[test]
    fn prose_code_and_lists_are_cut_per_block_and_item() {
        let t = "Hello there.\n\nSecond para.\n\n```rust\nfn a() {}\n```\n\n- one\n- two\n- three\n\nAfter.";
        assert_eq!(cut(t), vec![
            ("Hello there.", Lead::None, true),
            ("Second para.", Lead::None, true),
            ("```rust\nfn a() {}\n```", Lead::None, true),
            ("- one", Lead::None, false),
            ("- two", Lead::ListItem, false),
            ("- three", Lead::ListItem, true),
            ("After.", Lead::None, false),
        ]);
    }

    #[test]
    fn headings_and_rules_open_a_block_and_keep_what_follows() {
        let t = "Intro.\n\n## Part\n\nBody.\n\nMore.\n\n---\n\nEnd.";
        assert_eq!(cut(t), vec![
            ("Intro.", Lead::None, true),
            ("## Part\n\nBody.", Lead::Heading(2), true),
            ("More.", Lead::None, true),
            ("---\n\nEnd.", Lead::Rule, false),
        ]);
        // A heading first, or after a heading: not cut.
        assert_eq!(cut("# A\n## B\ntext").len(), 1);
    }

    #[test]
    fn ordered_items_are_cut_only_where_their_number_is_the_shown_one() {
        let t = "1. one\n2. two\n3. three";
        assert_eq!(texts(t, &split(t)), vec!["1. one", "2. two", "3. three"]);
        // "1. 1. 1." shows 1, 2, 3: an item alone would show 1 again.
        let t = "1. one\n1. two\n1. three";
        assert_eq!(split(t).len(), 1);
        // A list starting at 5.
        let t = "5. five\n6. six";
        assert_eq!(texts(t, &split(t)), vec!["5. five", "6. six"]);
    }

    #[test]
    fn items_keep_their_nested_content() {
        let t = "- a\n  - a1\n  - a2\n- b\n\n  more of b\n- c";
        assert_eq!(texts(t, &split(t)), vec!["- a\n  - a1\n  - a2", "- b\n\n  more of b", "- c"]);
    }

    #[test]
    fn references_and_html_are_not_cut_apart() {
        assert_eq!(split("See [x].\n\nPara.\n\n[x]: https://a.b").len(), 1);
        assert_eq!(split("Para.\n\n<div>\nhi\n</div>\n\nPara.").len(), 1);
        assert_eq!(split("").len(), 1);
    }

    /// A streamed reply: every prefix, cut incrementally, gives the blocks
    /// a cut of the whole prefix gives.
    #[test]
    fn streaming_recuts_only_the_tail_and_agrees_with_a_whole_cut() {
        let full = "Intro line.\n\n## Numbers\n\n1. one\n2. two\n3. three\n\nA paragraph with **bold** and `code`.\n\n```\nx = 1\n\ny = 2\n```\n\n- a\n- b\n\n---\n\n* c\n* d\n\n> quoted\n> more\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nThe end. Ünïcode ✓";
        for step in [1, 3, 7] {
        let mut blocks = vec![];
        let mut old = "";
        let mut i = 0;
        while i < full.len() {
            i = (i + step).min(full.len());
            while !full.is_char_boundary(i) { i += 1; }
            let new = &full[..i];
            blocks = update(&blocks, old, new);
            assert_eq!(blocks, split(new), "at {i}: {new:?}");
            old = new;
        }
        assert!(blocks.len() >= 12, "{blocks:?}");
        }
    }

    /// The owner's PC case: a 300-item list is 300 blocks, and appending to
    /// the last item re-cuts that item alone.
    #[test]
    fn a_long_list_is_one_block_per_item() {
        let t: String = (1..=300).map(|i| format!("{i}. item {i}\n")).collect();
        let b = split(&t);
        assert_eq!(b.len(), 300);
        let more = format!("{t}301. item");
        let b2 = update(&b, &t, &more);
        assert_eq!(b2.len(), 301);
        assert_eq!(&b2[..299], &b[..299]);
        assert_eq!(b2, split(&more));
    }
}

#[cfg(test)]
mod bench {
    #[test]
    #[ignore]
    fn speed() {
        let para = "A paragraph with **bold**, `code` and a [link](https://x.y), long enough to wrap a few times in the pane.\n\n";
        let t: String = (0..400).map(|i| if i % 5 == 0 { format!("{i}. item\n") } else { para.to_string() }).collect();
        let s = std::time::Instant::now();
        let b = super::split(&t);
        eprintln!("{} bytes, {} blocks, {:?}", t.len(), b.len(), s.elapsed());
        let more = format!("{t}tail");
        let s = std::time::Instant::now();
        let _ = super::update(&b, &t, &more);
        eprintln!("update {:?}", s.elapsed());
    }
}
