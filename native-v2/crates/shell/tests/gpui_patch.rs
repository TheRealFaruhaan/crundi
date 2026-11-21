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
