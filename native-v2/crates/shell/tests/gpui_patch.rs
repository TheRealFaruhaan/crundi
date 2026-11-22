//! The GPUI patch (vendor/gpui/PATCH.md) holds: (a) a cached view that renders
//! again keeps its clean cached children; (b) a cached view that only moved
//! by whole pixels is replayed, not rendered, and renders once when it stops;
//! a resized one renders. Runs on every OS in CI (`cargo test`).

use gpui::{div, prelude::*, px, rgb, Context, Entity, IntoElement, Pixels, Render, StyleRefinement, TestAppContext, VisualTestContext, Window};
use std::{cell::Cell, rc::Rc, time::Duration};

struct Leaf { renders: Rc<Cell<u32>> }
impl Render for Leaf {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_full().bg(rgb(0x336699)).child("leaf")
    }
}

/// Holds the leaf, cached, at `top`.
struct Mid { leaf: Entity<Leaf>, top: Pixels, wide: bool, renders: Rc<Cell<u32>> }
impl Render for Mid {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().relative().size_full().child(
            div().absolute().top(self.top).left(px(10.)).w(px(if self.wide { 160. } else { 100. })).h(px(40.))
                .child(self.leaf.clone().cached(StyleRefinement::default().size_full())),
        )
    }
}

struct Root { mid: Entity<Mid> }
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.mid.clone().cached(StyleRefinement::default().size_full()))
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn cached_views_keep_clean_children_and_replay_moves(cx: &mut TestAppContext) {
    let (leaf_n, mid_n) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (l, m) = (leaf_n.clone(), mid_n.clone());
    let (root, cx) = cx.add_window_view(move |_, cx| {
        let leaf = cx.new(|_| Leaf { renders: l });
        let mid = cx.new(|_| Mid { leaf, top: px(20.), wide: false, renders: m });
        Root { mid }
    });
    let mid = root.read_with(cx, |r, _| r.mid.clone());
    draw(cx);
    draw(cx);
    let (l0, m0) = (leaf_n.get(), mid_n.get());
    assert!(l0 >= 1 && m0 >= 1, "both drawn once at least");

    // (a) The middle view changes: it renders, its clean cached leaf doesn't.
    mid.update(cx, |_, cx| cx.notify());
    draw(cx);
    assert_eq!(mid_n.get(), m0 + 1, "the notified view renders");
    assert_eq!(leaf_n.get(), l0, "its clean cached child is replayed (patch a)");

    // (b) The leaf moves by whole pixels: replayed at the offset, not rendered.
    mid.update(cx, |m, cx| { m.top = px(60.); cx.notify(); });
    draw(cx);
    assert_eq!(leaf_n.get(), l0, "a cached view that only moved is replayed (patch b)");
    mid.update(cx, |m, cx| { m.top = px(90.); cx.notify(); });
    draw(cx);
    assert_eq!(leaf_n.get(), l0, "…while it keeps moving");

    // Once still, it renders once (what its listeners captured is current again).
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(leaf_n.get(), l0 + 1, "it settles with one render");
    draw(cx);
    assert_eq!(leaf_n.get(), l0 + 1, "and is cached again after");

    // A view whose size changed renders (only a move is replayed).
    let l1 = leaf_n.get();
    mid.update(cx, |m, cx| { m.wide = true; cx.notify(); });
    draw(cx);
    assert_eq!(leaf_n.get(), l1 + 1, "a resized view renders");
}

/// Holds the leaf, cached, at `top` inside a 200 px clip (F241).
struct Clip { leaf: Entity<Leaf>, top: Pixels }
impl Render for Clip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().w(px(200.)).h(px(200.)).overflow_hidden().child(
            div().absolute().top(self.top).left(px(10.)).w(px(100.)).h(px(40.))
                .child(self.leaf.clone().cached(StyleRefinement::default().size_full())),
        )
    }
}
struct ClipRoot { clip: Entity<Clip> }
impl Render for ClipRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.clip.clone().cached(StyleRefinement::default().size_full()))
    }
}

fn settle(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn partly_clipped_moves_replay_and_hidden_views_skip(cx: &mut TestAppContext) {
    let leaf_n = Rc::new(Cell::new(0));
    let l = leaf_n.clone();
    let (root, cx) = cx.add_window_view(move |_, cx| {
        let leaf = cx.new(|_| Leaf { renders: l });
        ClipRoot { clip: cx.new(|_| Clip { leaf, top: px(20.) }) }
    });
    let (clip, leaf) = root.read_with(cx, |r, cx| (r.clip.clone(), r.clip.read(cx).leaf.clone()));
    let at = |cx: &mut VisualTestContext, top: f32| { clip.update(cx, |c, cx| { c.top = px(top); cx.notify(); }); draw(cx); };
    draw(cx);
    draw(cx);
    let n = leaf_n.get();

    // Moving across the clip's edge, and further out: nothing new comes into
    // view, so it is replayed (patch b, F241).
    at(cx, 170.);
    assert_eq!(leaf_n.get(), n, "a move that crosses the clip's edge is replayed");
    at(cx, 180.);
    assert_eq!(leaf_n.get(), n, "…and one further out of it");
    // Back in: what the clip cut off must be drawn, so it renders.
    at(cx, 160.);
    assert_eq!(leaf_n.get(), n + 1, "a partly clipped view moving into view renders");
    settle(cx);
    let n = leaf_n.get();

    // Wholly outside its clip: neither rendered nor painted, even when it changes.
    at(cx, 300.);
    assert_eq!(leaf_n.get(), n, "a view moved out of its clip isn't rendered");
    leaf.update(cx, |_, cx| cx.notify());
    draw(cx);
    at(cx, 320.);
    assert_eq!(leaf_n.get(), n, "…nor when it changes or moves while outside");
    settle(cx);
    assert_eq!(leaf_n.get(), n, "…nor when the moves settle");
    // In view again, it renders.
    at(cx, 20.);
    assert_eq!(leaf_n.get(), n + 1, "it renders once in view again");
    draw(cx);
    assert_eq!(leaf_n.get(), n + 1, "and is cached again after");
}

/// Overlapping quads and shadows in a cached leaf, and a quad of its parent's
/// over it: the draw order decides how they batch.
struct Stack { renders: Rc<Cell<u32>> }
impl Render for Stack {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().relative().size_full().bg(rgb(0x202020))
            .child(div().absolute().top(px(5.)).left(px(5.)).w(px(60.)).h(px(30.)).bg(rgb(0x404040)).shadow_md())
            .child(div().absolute().top(px(10.)).left(px(10.)).w(px(30.)).h(px(10.)).bg(rgb(0x606060)))
            .child(div().absolute().top(px(70.)).left(px(70.)).w(px(10.)).h(px(10.)).bg(rgb(0x808080)).shadow_sm())
    }
}
struct StackParent { leaf: Entity<Stack>, top: Pixels, over: bool }
impl Render for StackParent {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().bg(rgb(0x101010))
            .child(div().absolute().top(self.top).left(px(10.)).w(px(100.)).h(px(100.))
                .child(self.leaf.clone().cached(StyleRefinement::default().size_full())))
            .when(self.over, |d| d.child(div().absolute().top(px(0.)).left(px(0.)).w(px(300.)).h(px(300.)).bg(rgb(0xa0a0a0)).shadow_sm()))
    }
}
struct StackRoot { p: Entity<StackParent> }
impl Render for StackRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.p.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// (d) A replayed range keeps its draw orders, shifted: a frame that replays
/// a cached view (in place or moved) batches exactly as one that renders it.
#[gpui::test]
fn replayed_views_draw_in_the_same_order(cx: &mut TestAppContext) {
    let n = Rc::new(Cell::new(0));
    let l = n.clone();
    let (root, cx) = cx.add_window_view(move |_, cx| {
        let leaf = cx.new(|_| Stack { renders: l });
        StackRoot { p: cx.new(|_| StackParent { leaf, top: px(20.), over: true }) }
    });
    let (p, leaf) = root.read_with(cx, |r, cx| (r.p.clone(), r.p.read(cx).leaf.clone()));
    // The change, the draw and the read in one update: a test window draws
    // when an update's effects are flushed, so a draw between them would be
    // a replay of the frame to compare with.
    let frame = |cx: &mut VisualTestContext, top: f32, over: bool, leaf_too: bool| {
        let (p, leaf) = (p.clone(), leaf.clone());
        cx.update(move |window, cx| {
            p.update(cx, |p, cx| { p.top = px(top); p.over = over; cx.notify(); });
            if leaf_too { leaf.update(cx, |_, cx| cx.notify()); }
            window.draw(cx).clear(cx);
            window.crundi_scene_batches()
        })
    };
    for over in [true, false] {
        let r0 = n.get();
        let fresh = frame(cx, 20., over, true);
        assert_eq!(n.get(), r0 + 1, "the leaf renders");
        assert!(fresh.len() >= 3, "kinds interleave by order: {fresh:?}");
        // In place: the parent renders, the leaf is replayed.
        assert_eq!(frame(cx, 20., over, false), fresh, "a replay batches as the render did (over: {over})");
        // Moved: replayed at an offset (patch b), the same batches.
        assert_eq!(frame(cx, 31., over, false), fresh, "a moved replay batches as the render did (over: {over})");
        // And again (orders shifted from shifted orders).
        assert_eq!(frame(cx, 40., over, false), fresh, "a second moved replay batches the same (over: {over})");
        assert_eq!(n.get(), r0 + 1, "the leaf was replayed each time");
        settle(cx);
    }
}
