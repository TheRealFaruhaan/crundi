//! The "+" menu by Workbench (the web's wbAddMenu): a new terminal's
//! launcher, the panels, Fit to screen, and Layout with its presets in a
//! submenu that opens on hover. It sits under the project bar's "+"; a
//! press outside closes it, and a press on it reaches no pane.

use crate::Workbench;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};

const ITEMS: [(&str, &str, IconName); 9] = [
    ("terminal", "Terminal", IconName::Terminal), ("files", "Files", IconName::Folder), ("git", "Git", IconName::GitBranch),
    ("kanban", "Kanban", IconName::Kanban), ("notes", "Notes", IconName::FileText), ("mindmap", "Mindmap", IconName::Network),
    ("media", "Media", IconName::Images), ("factory", "Factory", IconName::Factory), ("panel", "Panel", IconName::PanelsTopLeft),
];
const PRESETS: [(&str, &str, IconName); 5] = [
    ("layout:cols2", "2 columns", IconName::Columns2), ("layout:cols3", "3 columns", IconName::Columns3),
    ("layout:cross4", "2×2 cross", IconName::Grid2x2), ("layout:left-right2", "Left + right split", IconName::PanelsTopLeft),
    ("layout:top-bottom2", "Top + bottom split", IconName::Rows2),
];

impl Workbench {
    pub(crate) fn add_menu_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (vw, vh) = self.win_size;
        // Under the "+": the project bar's padding, the Workbench tab, the gap.
        let x = T::SIDEBAR_W + T::SP_12 + T::SP_48 + T::SP_48 + T::SP_24;
        let y = T::TOPBAR_H + T::SP_6 + T::SP_24 + T::SP_4;
        let row = T::SP_24 + T::SP_6 + px(1.) + T::SP_2;
        let mut menu = w::menu_panel("add-menu").absolute().left(x).top(y).w(T::MENU_MIN_W)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (id, label, icon) in ITEMS {
            menu = menu.child(w::menu_item(id, icon, label, false, false)
                .on_hover(cx.listener(|this, h: &bool, _, cx| if *h && this.layouts_open { this.layouts_open = false; cx.notify(); }))
                .on_click(cx.listener(move |this, _, _, cx| this.add_pane(id, cx))));
        }
        menu = menu.child(w::menu_item("fit", IconName::Maximize2, "Fit to screen", false, false)
            .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new("Share the width out again so every pane fits the window (pinned panes keep their size)").build(window, cx))
            .on_hover(cx.listener(|this, h: &bool, _, cx| if *h && this.layouts_open { this.layouts_open = false; cx.notify(); }))
            .on_click(cx.listener(|this, _, _, cx| this.add_pane("fit", cx))));
        menu = menu.child(w::menu_item("layout", IconName::Columns3, "Layout", true, self.layouts_open)
            .on_hover(cx.listener(|this, h: &bool, _, cx| if *h && !this.layouts_open { this.layouts_open = true; cx.notify(); }))
            .on_click(cx.listener(|this, _, _, cx| { this.layouts_open = true; cx.notify(); })));
        // The submenu: beside Layout, its top row level with it.
        let sub_y = y + px(1.) + T::SP_6 + row * (ITEMS.len() as f32 + 1.) - T::SP_6 - px(1.);
        let mut sub = w::menu_panel("layouts").absolute().left(x + T::MENU_MIN_W - T::SP_2).top(sub_y).min_w(T::MENU_MIN_W)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (id, label, icon) in PRESETS {
            sub = sub.child(w::menu_item(id, icon, label, false, false).on_click(cx.listener(move |this, _, _, cx| this.add_pane(id, cx))));
        }
        deferred(anchored().position(point(px(0.), px(0.))).child(
            div().id("add-menu-backdrop").occlude().relative().w(px(vw)).h(px(vh))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| { this.add_menu = false; this.layouts_open = false; cx.notify(); }))
                .child(menu)
                .when(self.layouts_open, |d| d.child(sub)))).with_priority(3)
    }
}
