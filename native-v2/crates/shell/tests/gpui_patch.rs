//! The GPUI patch (vendor/gpui/PATCH.md) holds: (a) a cached view that renders
//! again keeps its clean cached children; (b) a cached view that only moved
//! by whole pixels is replayed, not rendered, and renders once when it stops;
//! a resized one renders; (e) inside a wide clip, one coming into view and a
//! pan within the margin are replayed, and nothing outside the real clip is
//! hit. Runs on every OS in CI (`cargo test`).

use gpui::{deferred, div, point, prelude::*, px, rgb, size, Context, Entity, IntoElement, Modifiers, MouseButton, Pixels, Render, StyleRefinement, TestAppContext, VisualTestContext, Window};
use std::{cell::Cell, rc::Rc, time::Duration};

struct Leaf { renders: Rc<Cell<u32>> }
impl Render for Leaf {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_full().bg(rgb(0x336699)).child("leaf")
    }
}

/// A leaf with a mouse listener (its presses counted): a move it is
/// replayed through is followed by one render once still.
struct Pressed { renders: Rc<Cell<u32>>, presses: Rc<Cell<u32>> }
impl Render for Pressed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let p = self.presses.clone();
        div().size_full().bg(rgb(0x336699)).child("leaf").on_mouse_down(MouseButton::Left, move |_, _, _| p.set(p.get() + 1))
    }
}

/// Holds the leaf, cached, at `top`.
struct Mid { leaf: Entity<Pressed>, top: Pixels, wide: bool, renders: Rc<Cell<u32>> }
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
        let leaf = cx.new(|_| Pressed { renders: l, presses: Rc::default() });
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

/// (e) Holds a pressable leaf, cached, at `top` inside a 200 px wide clip
/// recorded 150 px past it on each side.
struct Wide { leaf: Entity<Pressed>, top: Pixels }
impl Render for Wide {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().w(px(200.)).h(px(200.)).overflow_hidden().wide_clip(size(px(150.), px(150.))).child(
            div().absolute().top(self.top).left(px(10.)).w(px(100.)).h(px(40.))
                .child(self.leaf.clone().cached(StyleRefinement::default().size_full())),
        )
    }
}
struct WideRoot { wide: Entity<Wide> }
impl Render for WideRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.wide.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// (e) In a wide clip a view coming into view across the clip's edge is
/// replayed (its last scene holds what was cut off), and a press lands on it
/// only where the clip shows it, before and after a replay moves it.
#[gpui::test]
fn wide_clip_replays_entering_views_and_cuts_hits(cx: &mut TestAppContext) {
    let (n, hits) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (l, h) = (n.clone(), hits.clone());
    let (root, cx) = cx.add_window_view(move |_, cx| {
        let leaf = cx.new(|_| Pressed { renders: l, presses: h });
        WideRoot { wide: cx.new(|_| Wide { leaf, top: px(20.) }) }
    });
    let wide = root.read_with(cx, |r, _| r.wide.clone());
    let at = |cx: &mut VisualTestContext, top: f32| { wide.update(cx, |c, cx| { c.top = px(top); cx.notify(); }); draw(cx); };
    let press = |cx: &mut VisualTestContext, y: f32| { cx.simulate_mouse_down(point(px(50.), px(y)), MouseButton::Left, Modifiers::default()); cx.simulate_mouse_up(point(px(50.), px(y)), MouseButton::Left, Modifiers::default()); };
    draw(cx);
    draw(cx);
    let r = n.get();

    // Out across the edge, then back in: both replayed.
    at(cx, 170.);
    at(cx, 180.);
    assert_eq!(n.get(), r, "a move out across a wide clip's edge is replayed");
    let h0 = hits.get();
    press(cx, 205.);
    assert_eq!(hits.get(), h0, "a press on the leaf outside the clip finds nothing");
    press(cx, 190.);
    assert_eq!(hits.get(), h0 + 1, "a press on the leaf inside the clip lands");
    at(cx, 160.);
    assert_eq!(n.get(), r, "a view coming into view across a wide clip's edge is replayed (patch e)");
    at(cx, 100.);
    assert_eq!(n.get(), r, "…and wholly in view");
    // What came into view is pressed where it now is (the hitbox cut again).
    press(cx, 135.);
    assert_eq!(hits.get(), h0 + 2, "a press on what came into view lands");
    press(cx, 150.);
    assert_eq!(hits.get(), h0 + 2, "…and one past the leaf doesn't");
    settle(cx);
    assert_eq!(n.get(), r + 1, "a view with a listener settles with one render");
    let r = n.get();

    // Wholly outside the real clip (inside the wider mask): skipped; back
    // in, it renders (nothing of it was recorded), then is replayed.
    at(cx, 260.);
    assert_eq!(n.get(), r, "a view wholly outside the real clip isn't rendered");
    press(cx, 270.);
    assert_eq!(hits.get(), h0 + 2, "…nor pressed");
    at(cx, 190.);
    assert_eq!(n.get(), r + 1, "back from outside, it renders");
    at(cx, 150.);
    assert_eq!(n.get(), r + 1, "…then is replayed");
}

/// (e) A big content view panned inside a 300x200 wide clip recorded 100 px
/// past it: its quads, drawn whole (no cull), 100 px apart.
struct Board { renders: Rc<Cell<u32>> }
impl Render for Board {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let mut d = div().relative().size_full();
        for i in 0..10 { for j in 0..10 {
            d = d.child(div().absolute().left(px(i as f32 * 100. + 10.)).top(px(j as f32 * 100. + 10.)).w(px(60.)).h(px(30.)).bg(rgb(0x404040 + (i * 10 + j) as u32)));
        } }
        d
    }
}
struct Pan { board: Entity<Board>, x: f32, y: f32 }
impl Render for Pan {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().w(px(300.)).h(px(200.)).overflow_hidden().wide_clip(size(px(100.), px(100.)))
            .child(self.board.clone().cached(StyleRefinement::default().absolute().left(px(-self.x)).top(px(-self.y)).w(px(1000.)).h(px(1000.))))
    }
}
struct PanRoot { pan: Entity<Pan> }
impl Render for PanRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.pan.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// (e) A pan within the wide clip's margin is replayed, and draws what a
/// render there draws; one past the margin renders.
#[gpui::test]
fn wide_clip_replays_a_pan_within_its_margin(cx: &mut TestAppContext) {
    let n = Rc::new(Cell::new(0));
    let l = n.clone();
    let (root, cx) = cx.add_window_view(move |_, cx| {
        let board = cx.new(|_| Board { renders: l });
        PanRoot { pan: cx.new(|_| Pan { board, x: 200., y: 200. }) }
    });
    let (pan, board) = root.read_with(cx, |r, cx| (r.pan.clone(), r.pan.read(cx).board.clone()));
    let frame = |cx: &mut VisualTestContext, x: f32, y: f32, render: bool| {
        let (pan, board) = (pan.clone(), board.clone());
        cx.update(move |window, cx| {
            pan.update(cx, |p, cx| { p.x = x; p.y = y; cx.notify(); });
            if render { board.update(cx, |_, cx| cx.notify()); }
            window.draw(cx).clear(cx);
            window.crundi_scene_batches()
        })
    };
    frame(cx, 200., 200., false);
    frame(cx, 200., 200., false);
    let r = n.get();
    let a = frame(cx, 240., 230., false);
    let b = frame(cx, 280., 270., false);
    assert_eq!(n.get(), r, "a pan within the margin is replayed (patch e)");
    let fresh = frame(cx, 280., 270., true);
    assert_eq!(n.get(), r + 1, "(a render at the same place)");
    assert_eq!(b, fresh, "the replayed pan draws what a render there draws");
    assert!(!a.is_empty());
    // From the render at 280,270: 90 px (within), then 60 more (past the margin).
    frame(cx, 190., 270., false);
    assert_eq!(n.get(), r + 1, "within the margin of where it last rendered: replayed");
    frame(cx, 130., 270., false);
    assert_eq!(n.get(), r + 2, "past it: rendered");
}

/// (e) A cached pane holding two cached leaves side by side, 300 px apart.
struct TwoLeaves { a: Entity<Leaf>, b: Entity<Leaf> }
impl Render for TwoLeaves {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full()
            .child(div().absolute().left(px(0.)).top(px(0.)).w(px(100.)).h(px(40.)).child(self.a.clone().cached(StyleRefinement::default().size_full())))
            .child(div().absolute().left(px(300.)).top(px(0.)).w(px(100.)).h(px(40.)).child(self.b.clone().cached(StyleRefinement::default().size_full())))
    }
}
struct NestClip { pane: Entity<TwoLeaves>, left: Pixels }
impl Render for NestClip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().w(px(200.)).h(px(200.)).overflow_hidden().wide_clip(size(px(300.), px(0.))).child(
            div().absolute().top(px(20.)).left(self.left).w(px(400.)).h(px(40.))
                .child(self.pane.clone().cached(StyleRefinement::default().size_full())),
        )
    }
}
struct NestRoot { clip: Entity<NestClip> }
impl Render for NestRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.clip.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// (e) A view inside a wide clip whose cached child was skipped (wholly
/// outside the real clip: nothing of it recorded) is not replayed over
/// that child's place: moving it so the child comes into view renders it,
/// and the child with it.
#[gpui::test]
fn wide_clip_never_replays_over_a_skipped_child(cx: &mut TestAppContext) {
    let (na, nb) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (a, b) = (na.clone(), nb.clone());
    let (root, cx) = cx.add_window_view(move |_, cx| {
        let a = cx.new(|_| Leaf { renders: a });
        let b = cx.new(|_| Leaf { renders: b });
        let pane = cx.new(|_| TwoLeaves { a, b });
        NestRoot { clip: cx.new(|_| NestClip { pane, left: px(0.) }) }
    });
    let (clip, pane) = root.read_with(cx, |r, cx| (r.clip.clone(), r.clip.read(cx).pane.clone()));
    let at = |cx: &mut VisualTestContext, left: f32| { clip.update(cx, |c, cx| { c.left = px(left); cx.notify(); }); draw(cx); };
    draw(cx);
    draw(cx);
    let b0 = nb.get();
    // The second leaf (300..400) is outside the 200 px clip: while the pane
    // renders there, it is skipped (nothing of it recorded).
    at(cx, -20.);
    pane.update(cx, |_, cx| cx.notify());
    draw(cx);
    assert_eq!(nb.get(), b0, "a leaf wholly outside the real clip isn't rendered");
    // Moved so it comes into view (130..230): it must be drawn, not left out.
    at(cx, -170.);
    assert_eq!(nb.get(), b0 + 1, "a skipped child coming into view renders (its parent isn't replayed over it)");
    at(cx, -150.);
    assert_eq!(nb.get(), b0 + 1, "…then, drawn whole, it is replayed as it moves on");
}

// ─── Downstream checks of patch (a) (the Manager's, F247's step) ───

/// A parent that makes its child (a cached view) the first time it renders.
struct Maker { child: Option<Entity<Leaf>>, renders: Rc<Cell<u32>>, child_renders: Rc<Cell<u32>> }
impl Render for Maker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let n = self.child_renders.clone();
        let child = self.child.get_or_insert_with(|| cx.new(|_| Leaf { renders: n })).clone();
        div().size_full().child(child.cached(StyleRefinement::default().size_full()))
    }
}
struct MakerRoot { maker: Entity<Maker> }
impl Render for MakerRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.maker.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// (a) radiosilence/koan#2031: a child made by its parent with `cx.new`,
/// reused while the parent renders again (and while the root renders around
/// both, reused whole), still wakes the window when it notifies: its notify
/// draws a frame that renders it, with no other input.
#[gpui::test]
fn a_child_made_in_render_and_reused_still_wakes_the_window(cx: &mut TestAppContext) {
    let (pn, cn) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (p, c) = (pn.clone(), cn.clone());
    let (root, cx) = cx.add_window_view(move |_, cx| MakerRoot { maker: cx.new(|_| Maker { child: None, renders: p, child_renders: c }) });
    let maker = root.read_with(cx, |r, _| r.maker.clone());
    draw(cx);
    draw(cx);
    let child = maker.read_with(cx, |m, _| m.child.clone().expect("made"));
    // The parent renders again: the clean child is reused.
    let c0 = cn.get();
    maker.update(cx, |_, cx| cx.notify());
    draw(cx);
    assert_eq!(cn.get(), c0, "the clean child is reused while its parent renders");
    // The root renders again: the parent (and the child in it) reused whole.
    root.update(cx, |_, cx| cx.notify());
    draw(cx);
    draw(cx);
    assert_eq!(cn.get(), c0, "…and while the root renders around them");
    // The child notifies: the window draws on its own, and the child renders.
    child.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(cn.get(), c0 + 1, "a reused child's notify reaches the window (it draws, the child renders)");
    // And again, after one more reuse.
    maker.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    let c1 = cn.get();
    child.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(cn.get(), c1 + 1, "…every time");
}

/// A cached leaf under a parent that may be `invisible()`.
struct Hider { leaf: Entity<Leaf>, hidden: bool }
impl Render for Hider {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().bg(rgb(0x101010)).child(
            div().absolute().top(px(10.)).left(px(10.)).w(px(100.)).h(px(40.)).when(self.hidden, |d| d.invisible())
                .child(self.leaf.clone().cached(StyleRefinement::default().size_full())),
        )
    }
}
struct HiderRoot { hider: Entity<Hider> }
impl Render for HiderRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.hider.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// (a) MaestrIA-rs#52: a cached view under an `invisible()` parent is
/// prepainted but not painted. Its parent rendering again or reused whole
/// while it is hidden, then shown again: no panic (`reuse_layouts`), and it
/// draws what a render draws (nothing stale).
#[gpui::test]
fn a_cached_view_under_an_invisible_parent(cx: &mut TestAppContext) {
    let n = Rc::new(Cell::new(0));
    let l = n.clone();
    let (root, cx) = cx.add_window_view(move |_, cx| { let leaf = cx.new(|_| Leaf { renders: l }); HiderRoot { hider: cx.new(|_| Hider { leaf, hidden: false }) } });
    let (hider, leaf) = root.read_with(cx, |r, cx| (r.hider.clone(), r.hider.read(cx).leaf.clone()));
    let frame = |cx: &mut VisualTestContext, f: &dyn Fn(&mut gpui::App)| {
        cx.update(|window, cx| { f(cx); window.draw(cx).clear(cx); window.crundi_scene_batches() })
    };
    draw(cx);
    let shown = frame(cx, &|_| {});
    // Hidden: the parent renders; then it is reused whole (the root renders)
    // and renders again, a few frames each, the leaf changing meanwhile.
    let h = hider.clone();
    frame(cx, &move |cx| h.update(cx, |h, cx| { h.hidden = true; cx.notify(); }));
    let r = root.clone();
    for _ in 0..3 { frame(cx, &|cx| r.update(cx, |_, cx| cx.notify())); }
    let h = hider.clone();
    frame(cx, &move |cx| h.update(cx, |_, cx| cx.notify()));
    let lf = leaf.clone();
    frame(cx, &move |cx| lf.update(cx, |_, cx| cx.notify()));
    for _ in 0..2 { frame(cx, &|cx| r.update(cx, |_, cx| cx.notify())); }
    // Shown again: it draws as before.
    let h = hider.clone();
    let back = frame(cx, &move |cx| h.update(cx, |h, cx| { h.hidden = false; cx.notify(); }));
    assert_eq!(back, shown, "shown again, it draws what it drew");
    let again = frame(cx, &|cx| r.update(cx, |_, cx| cx.notify()));
    assert_eq!(again, shown, "…and replayed after");
    let lf = leaf.clone();
    let fresh = frame(cx, &move |cx| lf.update(cx, |_, cx| cx.notify()));
    assert_eq!(fresh, shown, "…as a render of it does");
}

/// A cached view drawn `deferred` inside a cached ancestor.
struct Holder { leaf: Entity<Leaf>, renders: Rc<Cell<u32>> }
impl Render for Holder {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().relative().size_full().bg(rgb(0x101010)).child(
            div().absolute().top(px(10.)).left(px(10.)).w(px(100.)).h(px(40.))
                .child(deferred(div().size_full().child(self.leaf.clone().cached(StyleRefinement::default().size_full())))),
        )
    }
}
struct HolderRoot { holder: Entity<Holder>, top: Pixels }
impl Render for HolderRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().child(
            div().absolute().top(self.top).left(px(0.)).w(px(300.)).h(px(200.))
                .child(self.holder.clone().cached(StyleRefinement::default().size_full())),
        )
    }
}

/// (a) MaestrIA-rs#52: a `deferred` cached view carried by a cached ancestor
/// that is reused whole (and moved, which (b) renders rather than replays:
/// a deferred draw is placed in window coordinates). No panic, nothing
/// stale.
#[gpui::test]
fn a_deferred_cached_view_in_a_reused_ancestor(cx: &mut TestAppContext) {
    let (n, hn) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (l, h) = (n.clone(), hn.clone());
    let (root, cx) = cx.add_window_view(move |_, cx| { let leaf = cx.new(|_| Leaf { renders: l }); HolderRoot { holder: cx.new(|_| Holder { leaf, renders: h }), top: px(0.) } });
    let (holder, leaf) = root.read_with(cx, |r, cx| (r.holder.clone(), r.holder.read(cx).leaf.clone()));
    let frame = |cx: &mut VisualTestContext, f: &dyn Fn(&mut gpui::App)| {
        cx.update(|window, cx| { f(cx); window.draw(cx).clear(cx); window.crundi_scene_batches() })
    };
    draw(cx);
    let r = root.clone();
    let first = frame(cx, &|cx| r.update(cx, |_, cx| cx.notify()));
    // Reused whole a few times (the root renders around it).
    for _ in 0..3 { assert_eq!(frame(cx, &|cx| r.update(cx, |_, cx| cx.notify())), first, "reused whole, it draws the same"); }
    // The leaf in the deferred draw changes while the ancestor is reused.
    let lf = leaf.clone();
    let l0 = n.get();
    assert_eq!(frame(cx, &move |cx| lf.update(cx, |_, cx| cx.notify())), first, "the deferred leaf renders, the same");
    assert_eq!(n.get(), l0 + 1, "(it rendered)");
    for _ in 0..2 { assert_eq!(frame(cx, &|cx| r.update(cx, |_, cx| cx.notify())), first); }
    // The ancestor renders again around its clean deferred leaf.
    let hd = holder.clone();
    assert_eq!(frame(cx, &move |cx| hd.update(cx, |_, cx| cx.notify())), first, "the ancestor renders around it");
    for _ in 0..2 { assert_eq!(frame(cx, &|cx| r.update(cx, |_, cx| cx.notify())), first); }
    // Moved: the ancestor renders (a deferred draw isn't replayed moved).
    let h0 = hn.get();
    let r2 = root.clone();
    frame(cx, &move |cx| r2.update(cx, |r, cx| { r.top = px(30.); cx.notify(); }));
    assert_eq!(hn.get(), h0 + 1, "moved, the ancestor with a deferred draw renders");
    let r3 = root.clone();
    assert_eq!(frame(cx, &move |cx| r3.update(cx, |r, cx| { r.top = px(0.); cx.notify(); })), first, "and back, it draws as before");
    for _ in 0..2 { assert_eq!(frame(cx, &|cx| r.update(cx, |_, cx| cx.notify())), first); }
}

/// (e) A clip that becomes a wide clip and back (the workbench grid while a
/// pane is moved) around a cached leaf wholly in view.
struct Toggle { leaf: Entity<Pressed>, wide: bool }
impl Render for Toggle {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().relative().w(px(200.)).h(px(200.)).overflow_hidden().when(self.wide, |d| d.wide_clip(size(px(200.), px(0.)))).child(
            div().absolute().top(px(20.)).left(px(10.)).w(px(100.)).h(px(40.))
                .child(self.leaf.clone().cached(StyleRefinement::default().size_full())),
        )
    }
}
struct ToggleRoot { t: Entity<Toggle> }
impl Render for ToggleRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.t.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// (e) A leaf wholly in view isn't drawn again when its clip becomes a wide
/// clip or a plain one again (it draws the same), and is still pressed.
#[gpui::test]
fn a_clip_turning_wide_keeps_views_in_view_cached(cx: &mut TestAppContext) {
    let (n, hits) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (l, h) = (n.clone(), hits.clone());
    let (root, cx) = cx.add_window_view(move |_, cx| { let leaf = cx.new(|_| Pressed { renders: l, presses: h }); ToggleRoot { t: cx.new(|_| Toggle { leaf, wide: false }) } });
    let t = root.read_with(cx, |r, _| r.t.clone());
    draw(cx);
    draw(cx);
    let r = n.get();
    for wide in [true, false, true] {
        t.update(cx, |t, cx| { t.wide = wide; cx.notify(); });
        draw(cx);
        assert_eq!(n.get(), r, "the leaf is reused as its clip turns wide: {wide}");
    }
    let h0 = hits.get();
    cx.simulate_mouse_down(point(px(50.), px(40.)), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(point(px(50.), px(40.)), MouseButton::Left, Modifiers::default());
    assert_eq!(hits.get(), h0 + 1, "…and pressed as before");
}
