//! The pane header (specs/pane-header-wide.md and -narrow.md, native-gpui's
//! head.rs): the drag grip, the state dot, the title, the state badge; then
//! A- ↺ A+, pin, full screen and the two-click ×. A cached leaf view of its
//! own, redrawn only when what it shows changes: its info (the kind's
//! `watch` observes what the title reads and calls `refresh`, which
//! notifies only on a change) and its frame (armed, pinned, narrow, focused:
//! the workbench notifies the head whose frame changed, F11).

use crate::Workbench;
use crundi_core::head as rules;
use crundi_theme::{IconName, T};
use crundi_widgets::{self as w, HeadContent as C, HeadLook as L};
use gpui::{prelude::FluentBuilder as _, *};
use std::rc::Rc;

/// A pane's title, its long title (wide panes) and its state.
#[derive(Clone, Default, PartialEq)]
pub struct HeadInfo { pub title: String, pub long: String, pub state: String }

/// What the workbench decides about a head.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Frame { pub leaf: u32, pub w: f32, pub armed: bool, pub pinned: bool, pub narrow: bool, pub full: bool, pub focused: bool }

pub struct PaneHead {
    key: String,
    chat: bool,
    info: Rc<dyn Fn(&App) -> HeadInfo>,
    wb: WeakEntity<Workbench>,
    /// The info as last drawn (`refresh` compares with it).
    last: Option<HeadInfo>,
    _subs: Vec<Subscription>,
}

impl PaneHead {
    pub(crate) fn new(key: String, chat: bool, info: Rc<dyn Fn(&App) -> HeadInfo>, wb: WeakEntity<Workbench>, watch: Box<dyn FnOnce(&mut Context<PaneHead>) -> Vec<Subscription>>, cx: &mut Context<Self>) -> Self {
        let subs = watch(cx);
        PaneHead { key, chat, info, wb, last: None, _subs: subs }
    }

    /// What the title reads may have changed (the kind's `watch`): redraw
    /// only if the info did.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let i = (self.info)(cx);
        if self.last.as_ref() != Some(&i) { self.last = Some(i); cx.notify(); }
    }

    /// The title as last drawn (the driver's "heads").
    pub fn drawn_title(&self) -> Option<String> { self.last.as_ref().map(|i| i.title.clone()) }

    fn act(&self, cx: &mut App, f: impl FnOnce(&mut Workbench, &mut Context<Workbench>) + 'static) {
        if let Some(wb) = self.wb.upgrade() { wb.update(cx, f); }
    }
}

/// A chat's dot by its state (the web's .term-dot): working yellow, needs
/// input accent, waiting sky, idle green, exited grey.
pub(crate) fn state_color(state: &str) -> Hsla {
    match state { "working" => T::yellow(), "needs-input" => T::accent(), "waiting" => T::sky(), "exited" => T::text_muted(), _ => T::green() }
}

impl Render for PaneHead {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let f = self.wb.upgrade().and_then(|wb| wb.read(cx).frame_of(&self.key)).unwrap_or_default();
        crundi_core::driver::rendered("head");
        let info = (self.info)(cx);
        self.last = Some(info.clone());
        let id = f.leaf;
        let _ = window;
        let (title, cap) = rules::title_shown(&info.title, &info.long, f.w);
        let mut row = div().flex().items_center().size_full().pl(T::SP_2).pr(T::SP_4).gap(T::SP_4);
        if !f.full {
            let pend = self.key.starts_with("pend:");
            row = row.child(div().flex_none().w(T::SP_24).h(T::SP_12).flex().items_center().justify_center().text_color(T::text_muted()).when(pend, |d| d.opacity(0.25)).child(w::icon(IconName::GripVertical, T::ICON_SM)));
        }
        if self.chat {
            row = row.child(div().flex_none().size(T::DOT).flex().items_center().justify_center()
                .when(info.state == "needs-input", |d| d.child(div().absolute().size(T::SP_12).rounded_full().bg(T::accent_dim())))
                .child(div().size(T::DOT).rounded(T::R_CHIP).bg(state_color(&info.state))));
        }
        row = row.child(div().id(("title", id)).flex_none().max_w(px(cap)).min_w_0().h(T::SP_16).px(T::SP_6).rounded(T::R_CONTROL)
            .flex().items_center().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text())
            .child(div().truncate().child(title)));
        if self.chat {
            let badge = match info.state.as_str() { "working" => Some(("WORKING", "warn")), "needs-input" => Some(("NEEDS INPUT", "accent")), "waiting" => Some(("WAITING", "sky")), _ => None };
            if let Some((b, tone)) = badge { row = row.child(w::tag(b, tone).font_family(T::UI_FONT).text_size(T::TEXT_11).font_weight(T::W_BOLD)); }
        }
        row = row.child(div().flex_1().min_w_0());
        let h = T::SP_20;
        let mut t = div().flex().flex_none().items_center().gap(T::SP_4)
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| { this.act(cx, move |w, cx| w.focus_pane(id, cx)); cx.stop_propagation(); }));
        if !f.narrow {
            if self.chat {
                t = t.child(w::head_button(("a-", id), C::Text("A-".into()), L::Rest, T::SP_24, h, "Smaller text").on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_zoom(id, -1, cx)))))
                    .child(w::head_button(("a0", id), C::Icon(IconName::RotateCcw), L::Rest, T::SP_24, h, "Reset text size").on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_zoom(id, 0, cx)))))
                    .child(w::head_button(("a+", id), C::Text("A+".into()), L::Rest, T::SP_32, h, "Larger text").on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_zoom(id, 1, cx)))));
            }
            let pinned = f.pinned;
            t = t.child(w::head_button(("pin", id), C::Icon(if pinned { IconName::PinOff } else { IconName::Pin }), if pinned { L::Pinned } else { L::Rest }, T::SP_24, h,
                if pinned { "Unpin (size and position are locked)" } else { "Pin size and position" }).on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_pin(id, cx)))));
        }
        if self.chat {
            t = t.child(w::head_button(("full", id), C::Icon(IconName::Maximize), if f.full { L::Lit } else { L::Rest }, T::SP_24, h, "Full screen")
                .on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_full(id, cx)))));
        }
        if !f.full {
            let tip = if f.armed { "Click again to close" } else if self.chat { "Close chat" } else if self.key.starts_with("pend:") { "Remove" } else { "Close panel" };
            t = t.child(if f.armed {
                w::head_button(("x", id), C::Text("Sure?".into()), L::Armed, T::SP_40 + T::SP_4, T::SP_16, tip)
            } else {
                w::head_button(("x", id), C::Icon(IconName::X), L::Close, T::SP_24 + T::SP_4, T::SP_16, tip)
            }.on_click(cx.listener(move |this, _, _, cx| this.act(cx, move |w, cx| w.pane_close(id, cx)))));
        }
        row = row.child(t);
        // One paint layer (w::layer): every pane's head is replayed each frame.
        w::layer(div()
            .id(("head", id)).relative().size_full().bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                if e.click_count == 2 { this.act(cx, move |w, cx| w.pane_maximize(id, cx)); } else { this.act(cx, move |w, cx| w.focus_pane(id, cx)); }
            }))
            .child(row))
    }
}
