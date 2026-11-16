//! The diff window (the web's feOpenDiff, the Slint client's wins.rs diff):
//! ORIGINAL | MODIFIED side by side with line numbers, the removed and added
//! lines, a bar per chunk with Stage (then Staged), ↑ n/m ↓ between chunks;
//! a chunk is staged with /api/git/stageHunk. Drawn in the one modal
//! (widgets::modal) for now; the floating file windows come with Files.

use crate::diff::{self, Chunk, Op, Row};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::collections::HashSet;

pub struct DiffWin {
    pub id: u32,
    pub file: String,
    project: String,
    ops: Vec<Op>,
    chunks: Vec<Chunk>,
    rows: Vec<Row>,
    /// The chunk shown (-1: none yet, "—/n").
    pub cur: i32,
    pub staged: HashSet<usize>,
    scroll: ScrollHandle,
    window: Option<AnyWindowHandle>,
}

#[derive(Default)]
pub struct Wins { pub list: Vec<Entity<DiffWin>>, next: u32 }
impl Global for Wins {}

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
        if let Some(i) = self.rows.iter().position(|r| r.chunk == cur) { self.scroll.scroll_to_item(i); }
        cx.notify();
    }

    /// Stage one chunk: its patch to /api/git/stageHunk; staged, its button
    /// says so and goes quiet, and Git loads again.
    pub fn stage(&mut self, chunk: usize, cx: &mut Context<Self>) {
        let win = self.window;
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

    fn half(ln: &str, text: &str, kind: &str) -> Div {
        let (bg, fg) = match kind {
            "del" => (T::ws_diff_del(), T::ws_diff_del_text()),
            "add" => (T::ws_diff_add(), T::ws_diff_add_text()),
            "pad" => (T::bg_secondary(), T::text_muted()),
            _ => (T::bg(), T::text()),
        };
        div().flex().flex_1().min_w_0().bg(bg).text_size(T::TEXT_12).font_family(T::MONO)
            .child(div().flex_none().w(T::WS_DIFF_LN).pr(T::SP_8).flex().justify_end().text_color(T::text_muted()).child(ln.to_string()))
            .child(div().flex_1().min_w_0().text_color(fg).whitespace_nowrap().overflow_hidden().child(if text.is_empty() { " ".to_string() } else { text.to_string() }))
    }
}

impl Render for DiffWin {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.window.is_none() { self.window = Some(window.window_handle()); }
        let mut list = div().id(("diff-rows", self.id)).flex().flex_col().flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll);
        for r in &self.rows {
            if r.chunk >= 0 {
                let c = r.chunk as usize;
                let done = self.staged.contains(&c);
                list = list.child(div().flex().items_center().gap(T::SP_8).h(T::SP_24).px(T::SP_8).bg(T::ws_diff_chunk())
                    .child(if done { w::tag("Staged", "ok").into_any_element() } else { w::button(("stage-chunk", c), "Stage").on_click(cx.listener(move |w, _, _, cx| w.stage(c, cx))).into_any_element() })
                    .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(format!("Chunk {}", c + 1))));
                continue;
            }
            list = list.child(div().flex().gap(px(1.)).bg(T::border())
                .child(Self::half(&r.old_ln, &r.old, r.old_kind))
                .child(Self::half(&r.new_ln, &r.new, r.new_kind)));
        }
        let col = |t: &'static str| div().flex_1().px(T::SP_8).text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(T::text_muted()).child(t);
        div().flex().flex_col().w(T::WS_DIFF_W).h(T::WS_DIFF_H)
            .child(w::modal_head("diff-head", self.file.clone(), false, |_, window, cx| w::close_dialog(window, cx)))
            .child(div().flex().items_center().gap(T::SP_8).px(T::SP_10).h(T::CONTROL_SM).border_b_1().border_color(T::border())
                .child(div().flex_1().text_size(T::TEXT_12).font_family(T::MONO).text_color(T::text()).child(self.file.clone()))
                .child(w::icon_button("diff-prev", IconName::ArrowUp, "Previous chunk").on_click(cx.listener(|w, _, _, cx| w.step(-1, cx))))
                .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(self.counter()))
                .child(w::icon_button("diff-next", IconName::ArrowDown, "Next chunk").on_click(cx.listener(|w, _, _, cx| w.step(1, cx)))))
            .child(div().flex().h(T::SP_20).items_center().bg(T::bg_secondary()).child(col("ORIGINAL")).child(col("MODIFIED")))
            .child(list)
            .when(self.chunks.is_empty(), |d| d.child(w::empty("No changes")))
    }
}

/// Open a diff (the old and new texts of a file).
pub fn open(win: Option<AnyWindowHandle>, project: &str, file: &str, old: &str, new: &str, cx: &mut App) {
    let ops = diff::diff(&diff::lines(old), &diff::lines(new));
    let chunks = diff::chunks(&ops);
    let rows = diff::rows(&ops);
    if !cx.has_global::<Wins>() { cx.set_global(Wins::default()); }
    let id = { let ws = cx.global_mut::<Wins>(); ws.next += 1; ws.next };
    let v = cx.new(|_| DiffWin { id, file: file.to_string(), project: project.to_string(), ops, chunks, rows, cur: -1, staged: HashSet::new(), scroll: ScrollHandle::new(), window: win });
    cx.global_mut::<Wins>().list = vec![v.clone()];
    let Some(h) = win else { return };
    let _ = cx.update_window(h, |_, window, cx| {
        w::modal(window, cx, T::WS_DIFF_W, v.clone().into(), move |_, cx| { if cx.has_global::<Wins>() { let keep: Vec<_> = cx.global::<Wins>().list.iter().filter(|x| x.read(cx).id != id).cloned().collect(); cx.global_mut::<Wins>().list = keep; } });
    });
}

pub fn find(cx: &App, id: u32) -> Option<Entity<DiffWin>> {
    cx.try_global::<Wins>()?.list.iter().find(|w| w.read(cx).id == id).cloned()
}

pub fn props(cx: &App) -> Value {
    Value::Array(cx.try_global::<Wins>().map(|ws| ws.list.iter().map(|w| { let w = w.read(cx); json!({
        "id": w.id, "kind": "diff", "title": w.file, "chunks": w.chunks.len(), "counter": w.counter(), "staged": w.staged.len(),
    }) }).collect()).unwrap_or_default())
}
