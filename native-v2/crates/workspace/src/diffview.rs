//! The diff window (the web's feOpenDiff, native-gpui's diffview.rs):
//! ORIGINAL | MODIFIED side by side with line numbers, the removed and added
//! lines, a bar per chunk with Stage (then Staged), ↑ n/m ↓ between chunks;
//! a chunk is staged with /api/git/stageHunk. Drawn as the body of a
//! floating file window (wins.rs, kind "diff"), as the web's feNewWindow.
//!
//! The diff is worked out once, off the UI thread (a long file's diff is
//! not cheap), into rows; the rows are a virtualised list (`gpui::list`:
//! only those on screen are laid out).

use crate::diff::{self, Chunk, Op, Row};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{collections::HashSet, rc::Rc};

pub struct DiffWin {
    pub id: u32,
    pub file: String,
    project: String,
    ops: Vec<Op>,
    chunks: Vec<Chunk>,
    rows: Rc<Vec<Row>>,
    /// The chunk shown (-1: none yet, "—/n").
    pub cur: i32,
    pub staged: HashSet<usize>,
    list: ListState,
}

impl DiffWin {
    pub fn counter(&self) -> String {
        let n = self.chunks.len();
        if self.cur < 0 { format!("—/{n}") } else { format!("{}/{n}", self.cur + 1) }
    }

    /// ↑ / ↓: the previous or next chunk, scrolled to.
    pub fn step(&mut self, d: i32, cx: &mut Context<Self>) {
        let n = self.chunks.len() as i32;
        if n == 0 { return; }
        self.cur = if d < 0 { (self.cur - 1).max(0) } else { (self.cur + 1).min(n - 1) };
        let cur = self.cur;
        if let Some(i) = self.rows.iter().position(|r| r.chunk == cur) { self.list.scroll_to(ListOffset { item_ix: i, offset_in_item: px(0.) }); }
        cx.notify();
    }

    /// Stage one chunk: its patch to /api/git/stageHunk; staged, its button
    /// says so and goes quiet, and Git loads again.
    pub fn stage(&mut self, chunk: usize, cx: &mut Context<Self>) {
        let Some(ch) = self.chunks.get(chunk).cloned() else { return crate::toast(cx, "Invalid chunk".into(), w::Toast::Error) };
        let patch = diff::chunk_patch(&self.file, &self.ops, &ch);
        let project = self.project.clone();
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.post("/api/git/stageHunk", &json!({ "project": project, "patch": patch })), move |r, cx| match r {
            Ok(d) if d.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) => {
                crate::toast(cx, format!("Chunk {} staged", chunk + 1), w::Toast::Success);
                let _ = me.update(cx, |w, cx| { w.staged.insert(chunk); cx.notify(); });
                if let Some(g) = crate::git::view(cx) { g.update(cx, |g, cx| g.load(cx)); }
            }
            Ok(d) => { let e = d.get("error").and_then(|e| e.as_str()).unwrap_or("unknown").to_string(); crate::toast(cx, format!("Stage failed: {e}"), w::Toast::Error) }
            Err(e) => crate::toast(cx, format!("Error: {e}"), w::Toast::Error),
        });
    }

    /// One side of a row: its line number, then the line in its colour, the
    /// changed words of a replaced line on a stronger background.
    fn half(ln: &str, text: &str, kind: &str, other: &str, words: &[(usize, usize)]) -> Div {
        let (bg, fg, word) = match kind {
            "del" => (T::ws_diff_del(), T::ws_diff_del_text(), T::ws_diff_del_word()),
            "add" => (T::ws_diff_add(), T::ws_diff_add_text(), T::ws_diff_add_word()),
            // The empty side of a removed or added line (the web's #1a1215 / #0f1a15).
            "pad" => { let p = match other { "del" => T::ws_diff_del_pad(), "add" => T::ws_diff_add_pad(), _ => T::bg_secondary() }; (p, T::text_muted(), p) }
            _ => (T::bg(), T::text(), T::bg()),
        };
        let line: SharedString = if text.is_empty() { " ".into() } else { text.to_string().into() };
        let marks: Vec<(std::ops::Range<usize>, HighlightStyle)> = words.iter().filter(|(a, b)| a < b && *b <= text.len())
            .map(|(a, b)| (*a..*b, HighlightStyle { background_color: Some(word), ..Default::default() })).collect();
        div().flex().flex_1().min_w_0().bg(bg).text_size(T::TEXT_12).font_family(T::MONO)
            .child(div().flex_none().w(T::WS_DIFF_LN).pr(T::SP_8).flex().justify_end().text_color(T::text_muted()).child(ln.to_string()))
            .child(div().flex_1().min_w_0().text_color(fg).whitespace_nowrap().overflow_hidden().child(StyledText::new(line).with_highlights(marks)))
    }

    fn row(&mut self, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(r) = self.rows.get(i).cloned() else { return div().into_any_element() };
        if r.chunk >= 0 {
            let c = r.chunk as usize;
            let done = self.staged.contains(&c);
            return div().w_full().flex().items_center().gap(T::SP_8).h(T::SP_24).px(T::SP_8).bg(T::ws_diff_chunk())
                .child(if done { w::tag("Staged", "ok").into_any_element() } else { w::tone_outline(("stage-chunk", c), "Stage", T::green()).on_click(cx.listener(move |w, _, _, cx| w.stage(c, cx))).into_any_element() })
                .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(format!("Chunk {}", c + 1)))
                .into_any_element();
        }
        div().w_full().flex().gap(px(1.)).bg(T::border())
            .child(Self::half(&r.old_ln, &r.old, r.old_kind, r.new_kind, &r.old_words))
            .child(Self::half(&r.new_ln, &r.new, r.new_kind, r.old_kind, &r.new_words))
            .into_any_element()
    }
}

impl Render for DiffWin {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("diff");
        let col = |t: &'static str| div().flex_1().px(T::SP_8).text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(T::text_muted()).child(t);
        let rows = list(self.list.clone(), cx.processor(|this, i: usize, _, cx| this.row(i, cx))).w_full().flex_1().min_h_0();
        div().flex().flex_col().size_full().bg(T::bg())
            .child(div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_12).h(T::CONTROL_SM).bg(T::bg_secondary()).border_b_1().border_color(T::border())
                .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).font_family(T::MONO).text_color(T::text()).child(self.file.clone()))
                .child(w::icon_button(("diff-prev", self.id as usize), IconName::ArrowUp, "Previous chunk").on_click(cx.listener(|w, _, _, cx| w.step(-1, cx))))
                .child(div().text_size(T::TEXT_11).font_family(T::MONO).text_color(T::text_muted()).child(self.counter()))
                .child(w::icon_button(("diff-next", self.id as usize), IconName::ArrowDown, "Next chunk").on_click(cx.listener(|w, _, _, cx| w.step(1, cx)))))
            .child(div().flex().flex_none().h(T::SP_20).items_center().bg(T::bg_tertiary()).border_b_1().border_color(T::border()).child(col("ORIGINAL")).child(col("MODIFIED")))
            .child(rows)
            .when(self.chunks.is_empty(), |d| d.child(w::empty("No changes")))
    }
}

/// Open a diff (the old and new texts of a file): worked out off the UI
/// thread, then shown in a new file window.
pub fn open(project: &str, file: &str, old: String, new: String, cx: &mut App) {
    let (project, file) = (project.to_string(), file.to_string());
    crundi_shell::run(cx, move || {
        let ops = diff::diff(&diff::lines(&old), &diff::lines(&new));
        let chunks = diff::chunks(&ops);
        let rows = diff::rows(&ops);
        (ops, chunks, rows)
    }, move |(ops, chunks, rows), cx| {
        let n = rows.len();
        let (f2, p2) = (file.clone(), project.clone());
        crate::wins::add_diff(&project, &file, cx, move |id, cx| cx.new(|_| DiffWin { id, file: f2, project: p2, ops, chunks, rows: Rc::new(rows), cur: -1, staged: HashSet::new(), list: ListState::new(n, ListAlignment::Top, px(200.)) }));
    });
}

pub fn find(cx: &App, id: u32) -> Option<Entity<DiffWin>> {
    crate::wins::layer(cx)?.read(cx).wins.iter().find(|w| w.id == id).and_then(|w| w.diff.clone())
}

pub fn props(cx: &App) -> Vec<Value> {
    crate::wins::layer(cx).map(|l| l.read(cx).wins.iter().filter_map(|w| w.diff.as_ref()).map(|w| { let w = w.read(cx); json!({
        "id": w.id, "kind": "diff", "title": w.file, "chunks": w.chunks.len(), "counter": w.counter(), "staged": w.staged.len(),
    }) }).collect()).unwrap_or_default()
}
