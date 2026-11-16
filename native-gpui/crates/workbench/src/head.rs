//! The pane header (2.1 wide, 2.2 narrow; specs/pane-header-wide.md and
//! -narrow.md, the Slint client's PaneHead): the drag grip, the state dot,
//! the title (click to rename), the state badge; then the lock, A- ↺ A+,
//! pin, full screen and the two-click ×. Narrow, the tools fold into "…".
//! Its geometry is the Slint client's: the title 43px in, the × 4px from
//! the edge (the phase 2 test's pointer checks read it).

use crate::{Info, Workbench};
use crundi_theme::{IconName, T};
use crundi_widgets::{self as w, HeadContent as C, HeadLook as L};
use gpui::{prelude::FluentBuilder as _, *};

impl Workbench {
    pub(crate) fn head(&self, i: &Info, full: bool, cx: &mut Context<Self>) -> Div {
        let id = i.l.id;
        let chat = i.kind == "chat";
        let renaming = self.renaming.as_ref().filter(|(l, _)| *l == id).map(|(_, s)| s.clone());
        let mut row = div().flex().items_center().size_full().pl(T::SP_2).pr(T::SP_4).gap(T::SP_4);
        if !full {
            row = row.child(div().flex_none().w(T::SP_24).h(T::SP_12).flex().items_center().justify_center().text_color(T::text_muted()).child(w::icon(IconName::GripVertical, T::ICON_SM)));
        }
        if chat {
            let color = pane_state_color(&i.state);
            row = row.child(div().flex_none().size(T::DOT).flex().items_center().justify_center()
                .when(i.state == "needs-input", |d| d.child(div().absolute().size(T::SP_12).rounded_full().bg(T::accent_dim())))
                .child(div().size(T::DOT).rounded(T::R_CHIP).bg(color)));
        } else if i.kind == "panel" {
            row = row.child(div().flex_none().text_size(T::ICON_MD).text_color(T::text_secondary()).child(panel_icon(&i.key)));
        }
        if let Some(st) = renaming {
            row = row.child(div().id(("rename", id)).flex_none().w(T::TITLE_CAP).h(T::SP_20)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(cx.listener(move |this, _, _, cx| this.commit_rename(id, cx)))
                .child(w::input(&st)));
        } else {
            // With the hover split controls on show (the header's middle), the
            // title stops short of them.
            let split_on = self.hover == id && !full && self.drag.is_none();
            let room = if split_on { (i.l.w / 2. - f32::from(T::SP_24 + T::SP_48 + T::SP_6 + T::SP_8)).max(0.) } else { i.cap };
            let t = div().id(("title", id)).flex_none().max_w(px(i.cap.min(room))).min_w_0().h(T::SP_16).px(T::SP_6).rounded(T::R_CONTROL)
                .flex().items_center().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text())
                .child(div().truncate().child(i.title.clone()));
            row = row.child(if chat && !i.parked {
                t.cursor_text().hover(|s| s.bg(T::bg_tertiary()))
                    .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| if e.click_count() == 1 && this.drag.is_none() { this.start_rename(id, window, cx); }))
            } else { t });
        }
        if chat {
            let badge = match i.state.as_str() { "working" => Some(("WORKING", "warn")), "needs-input" => Some(("NEEDS INPUT", "accent")), "waiting" => Some(("WAITING", "sky")), _ => None };
            // The badge's words as the web's .term-state (11px bold, not the tag's 10px mono).
            if let Some((b, tone)) = badge { row = row.child(w::tag(b, tone).font_family(T::UI_FONT).text_size(T::TEXT_11).font_weight(T::W_BOLD)); }
        }
        row = row.child(div().flex_1().min_w_0());
        row = row.child(self.tools(i, full, cx));
        let narrow = i.narrow;
        div()
            .relative().flex_none().h(if narrow { T::PANE_HEAD_NARROW } else { T::PANE_HEAD }).bg(T::bg_secondary()).border_b_1().border_color(T::border())
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                if e.click_count == 2 { if this.renaming.is_none() { this.pane_maximize(id, cx); } }
                else { this.head_press(id, f32::from(e.position.x), f32::from(e.position.y)); }
            }))
            .child(row)
            .when(self.hover == id && !full && self.drag.is_none(), |d| d.child(self.split_controls(id, cx)))
    }

    /// The header's tools, right to left from the ×.
    fn tools(&self, i: &Info, full: bool, cx: &mut Context<Self>) -> Div {
        let id = i.l.id;
        // A parked chat keeps only pin and × here (specs/parked-panes.md).
        let chat = i.kind == "chat" && !i.parked;
        let h = T::SP_20;
        let mut t = div().flex().flex_none().items_center().gap(T::SP_4)
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| { this.focus_pane(id, cx); cx.stop_propagation(); }));
        if i.factory && !i.narrow { t = t.child(w::tag("FACTORY", "accent")); }
        if i.parked { t = t.child(w::tag("CHAT", "accent")); }
        if !i.narrow {
            if chat && i.secrets {
                let n = i.secrets_n;
                let content = if n > 0 { C::IconText(IconName::Lock, n.to_string().into()) } else { C::Icon(IconName::Lock) };
                t = t.child(w::head_button(("lock", id), content, if n > 0 { L::On } else { L::Rest }, if n > 0 { T::SP_24 + T::SP_12 } else { T::SP_24 }, h, crundi_core::head::secrets_tip(n))
                    .on_click(cx.listener(move |this, _, window, cx| this.open_secrets(id, window, cx))));
            }
            if chat {
                t = t.child(w::head_button(("a-", id), C::Text("A-".into()), L::Rest, T::SP_24, h, "Smaller text").on_click(cx.listener(move |this, _, _, cx| this.pane_font(id, -1, cx))))
                    .child(w::head_button(("a0", id), C::Icon(IconName::RotateCcw), L::Rest, T::SP_24, h, "Reset text size").on_click(cx.listener(move |this, _, _, cx| this.pane_font(id, 0, cx))))
                    .child(w::head_button(("a+", id), C::Text("A+".into()), L::Rest, T::SP_32, h, "Larger text").on_click(cx.listener(move |this, _, _, cx| this.pane_font(id, 1, cx))));
            }
            let pinned = i.l.pinned;
            t = t.child(w::head_button(("pin", id), C::Icon(if pinned { IconName::PinOff } else { IconName::Pin }), if pinned { L::Pinned } else { L::Rest }, T::SP_24, h,
                if pinned { "Unpin (size and position are locked)" } else { "Pin size and position" }).on_click(cx.listener(move |this, _, _, cx| this.pane_pin(id, cx))));
        }
        if chat {
            t = t.child(w::head_button(("full", id), C::Icon(IconName::Maximize), if full { L::Lit } else { L::Rest }, T::SP_24, h, "Full screen")
                .on_click(cx.listener(move |this, _, _, cx| this.pane_full(id, cx))));
        }
        if i.narrow {
            let open = i.menu;
            t = t.child(w::head_button(("more", id), C::Icon(IconName::Ellipsis), if open { L::Lit } else { L::Rest }, T::SP_24, h, "More")
                .on_click(cx.listener(move |this, _, _, cx| this.pane_menu(id, !open, cx))));
        }
        if !full {
            let tip = if i.armed { "Click again to close" } else if chat { "Close chat" } else if i.kind == "launcher" { "Remove" } else { "Close panel" };
            t = t.child(if i.armed {
                w::head_button(("x", id), C::Text("Sure?".into()), L::Armed, T::SP_40 + T::SP_4, T::SP_16, tip)
            } else {
                w::head_button(("x", id), C::Icon(IconName::X), L::Close, T::SP_24 + T::SP_4, T::SP_16, tip)
            }.on_click(cx.listener(move |this, _, _, cx| { this.menu = 0; this.pane_close(id, cx) })));
        }
        t
    }

    /// The hover split controls in the header's middle (the web's
    /// .mosaic-split-btns): Split right, Split down. They don't focus the pane.
    fn split_controls(&self, id: u32, cx: &mut Context<Self>) -> Div {
        div().absolute().top(T::SP_2 + px(1.)).left_0().right_0().flex().justify_center().gap(T::SP_4)
            .child(div().flex().gap(T::SP_4).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(w::head_button(("split-r", id), C::Icon(IconName::Columns2), L::Rest, T::SP_20, T::SP_16 + T::SP_2, "Split right").on_click(cx.listener(move |this, _, _, cx| this.split(id, false, cx))))
                .child(w::head_button(("split-d", id), C::Icon(IconName::Rows2), L::Rest, T::SP_20, T::SP_16 + T::SP_2, "Split down").on_click(cx.listener(move |this, _, _, cx| this.split(id, true, cx)))))
    }

    /// A narrow header's "…" menu (the Slint client's HeadMenu): a strip of
    /// the folded tools under the header, right-aligned: the lock, A- ↺ A+,
    /// pin. Pin and the sizes leave it open; the lock and a press outside
    /// close it.
    pub(crate) fn narrow_menu(&self, i: &Info, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = i.l.id;
        let chat = i.kind == "chat";
        let h = T::SP_20 + px(1.);
        let mut row = div().flex().items_center().gap(T::SP_6);
        if chat && i.secrets {
            let n = i.secrets_n;
            let label = if n > 0 { format!("{n} secrets") } else { "Secrets".into() };
            row = row.child(w::head_button(("m-lock", id), C::IconText(IconName::Lock, label.into()), if n > 0 { L::On } else { L::Rest }, T::SP_48 + T::SP_24, h, crundi_core::head::secrets_tip(n))
                .on_click(cx.listener(move |this, _, window, cx| this.open_secrets(id, window, cx))));
        }
        if chat {
            row = row.child(w::head_button(("m-a-", id), C::Text("A-".into()), L::Rest, T::SP_24 + T::SP_2, h, "Smaller text").on_click(cx.listener(move |this, _, _, cx| this.pane_font(id, -1, cx))))
                .child(w::head_button(("m-a0", id), C::Icon(IconName::RotateCcw), L::Rest, T::SP_24 + px(1.), h, "Reset text size").on_click(cx.listener(move |this, _, _, cx| this.pane_font(id, 0, cx))))
                .child(w::head_button(("m-a+", id), C::Text("A+".into()), L::Rest, T::SP_32, h, "Larger text").on_click(cx.listener(move |this, _, _, cx| this.pane_font(id, 1, cx))));
        }
        let pinned = i.l.pinned;
        row = row.child(w::head_button(("m-pin", id), C::Icon(if pinned { IconName::PinOff } else { IconName::Pin }), if pinned { L::Pinned } else { L::Rest }, T::SP_24 + T::SP_2 + px(1.), h,
            if pinned { "Unpin (size and position are locked)" } else { "Pin size and position" }).on_click(cx.listener(move |this, _, _, cx| this.pane_pin(id, cx))));
        div().id(("pane-menu", id)).occlude().absolute().top(T::PANE_HEAD_NARROW + px(1.)).right(T::SP_4 + px(1.)).p(T::SP_8)
            .bg(T::bg_secondary()).border_1().border_color(T::border()).rounded(T::R_CARD).shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(move |this, _, _, cx| this.pane_menu(id, false, cx)))
            .child(row)
    }
}

/// A panel's icon by its kind.
pub(crate) fn panel_icon(key: &str) -> IconName {
    match key.trim_start_matches("panel:").split(':').next().unwrap_or("") {
        "files" => IconName::Folder, "git" => IconName::GitBranch, "kanban" => IconName::Kanban, "notes" => IconName::FileText,
        "mindmap" => IconName::Network, "media" => IconName::Images, "factory" => IconName::Factory, _ => IconName::PanelsTopLeft,
    }
}

/// A chat's dot by its state (the Slint client's PaneState.color, the web's
/// .term-dot): working yellow, needs input accent, waiting sky, idle green,
/// exited grey.
pub(crate) fn pane_state_color(state: &str) -> Hsla {
    match state {
        "working" => T::yellow(),
        "needs-input" => T::accent(),
        "waiting" => T::sky(),
        "exited" => T::text_muted(),
        _ => T::green(),
    }
}
