//! Claude's markdown drawn outside the log's list (the plan panel, a side
//! question's answer, an agent's summary and transcript, a factory card's
//! summary): core::webmd's blocks, cut once per text and kept (`Memo`, by
//! the text's key and length: a text that does not change is never cut
//! again; render only lays out what is cut), drawn as the log draws them,
//! with the web's margins, not selectable.

use super::text::{heading_size, margins, runs, Look};
use crundi_core::webmd::{self, Block, Kind};
use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};
use std::{collections::HashMap, rc::Rc};

/// Cut texts by their key (the text's length and a hash of it with it).
#[derive(Default)]
pub struct Memo(HashMap<String, (u64, Rc<Vec<Block>>)>);

fn hash(t: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

impl Memo {
    /// The blocks of `text` under `key` (cut now only when it changed).
    pub fn blocks(&mut self, key: &str, text: &str) -> Rc<Vec<Block>> {
        let h = hash(text);
        if let Some((k, b)) = self.0.get(key) { if *k == h { return b.clone(); } }
        let b = Rc::new(webmd::split(text).blocks);
        if self.0.len() > 256 { self.0.clear(); }
        self.0.insert(key.to_string(), (h, b.clone()));
        b
    }
}

fn base(c: Hsla) -> Look { Look { color: c, mono: false, bold: false, link: Some(T::accent_hover()) } }

fn styled(inl: &webmd::Inline, look: Look) -> StyledText { StyledText::new(inl.text.clone()).with_runs(runs(inl, look, None)) }

/// The blocks as elements, their margins collapsed as the browser does (the
/// last one's bottom margin dropped), at text scale `fs` in colour `color`.
pub fn draw(blocks: &[Block], fs: f32, color: Hsla) -> Vec<AnyElement> {
    let n = blocks.len();
    let body = T::TEXT_13 * fs;
    let lh = relative(T::LH_CHAT);
    blocks.iter().enumerate().map(|(i, b)| {
        let (top, _) = margins(&b.kind, fs);
        let above = if i == 0 { 0. } else { margins(&blocks[i - 1].kind, fs).1.max(top) };
        let p0 = |look: Look| b.parts.first().map(|x| styled(x, look).into_any_element()).unwrap_or_else(|| div().into_any_element());
        let el = match &b.kind {
            Kind::Para => div().text_size(body).line_height(lh).child(p0(base(color))),
            Kind::Heading(lv) => div().text_size(heading_size(*lv, fs)).line_height(lh).child(p0(Look { bold: true, ..base(color) })),
            Kind::Quote => div().text_size(body).line_height(lh).child(div().pl(T::SP_10).border_l_2().border_color(T::border()).child(p0(base(T::text_secondary())))),
            Kind::Item { ordered, n, .. } => div().flex().text_size(body).line_height(lh)
                .child(div().flex_none().w(T::MD_LIST_INDENT).flex().justify_end().pr(T::MD_MARKER_GAP * fs).text_color(color)
                    .child(if *ordered { SharedString::from(format!("{n}.")) } else { SharedString::from("\u{2022}") }))
                .child(div().flex_1().min_w_0().child(p0(base(color)))),
            Kind::Code => div().child(div().id(SharedString::from(format!("mdc-{i}-{}", b.parts.first().map_or(0, |p| p.text.len())))).w_full().flex().rounded(T::R_CONTROL).bg(T::bg_secondary()).border_1().border_color(T::border_subtle())
                .px(T::MD_CODE_PX).py(T::MD_CODE_PY).overflow_x_scroll()
                .child(div().flex_none().text_size(T::TEXT_12 * fs).line_height(relative(T::LH_CODE)).whitespace_nowrap().child(p0(Look { mono: true, ..base(color) })))),
            Kind::Rule => div().child(div().w_full().h(px(2.)).border_t_1().border_color(T::md_rule_dark()).bg(T::md_rule())),
            Kind::Table { cols, rows } => {
                let mut t = div().w_full().flex().flex_col().rounded(T::R_CHIP).border_1().border_color(T::border()).overflow_hidden().text_size(body).line_height(lh);
                for y in 0..*rows {
                    let mut line = div().w_full().flex().when(y > 0, |d| d.border_t_1().border_color(T::border())).when(y == 0, |d| d.bg(T::bg_secondary()));
                    for x in 0..*cols {
                        let k = y * cols + x;
                        let cell = b.parts.get(k).map(|i| styled(i, Look { bold: y == 0, ..base(color) }).into_any_element()).unwrap_or_else(|| div().into_any_element());
                        line = line.child(div().flex_1().min_w_0().px(T::SP_8).py(T::SP_4).when(x > 0, |d| d.border_l_1().border_color(T::border())).child(cell));
                    }
                    t = t.child(line);
                }
                div().child(t)
            }
        };
        let _ = n;
        el.w_full().pt(px(above)).into_any_element()
    }).collect()
}
