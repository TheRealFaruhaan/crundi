//! The "+" menu by the Workbench tab (the web's #wb-add-menu, toggleWbAddMenu):
//! Terminal, the panels, Panel, Fit to screen, and Layout with its presets
//! in a submenu that opens on hover. Under the "+" (4px below it, its left
//! edge kept 6px inside the window); a press outside closes it.
//!
//! A layer the shell draws over the window (`crundi_shell::register_overlay`):
//! one cached view that draws nothing while closed, so the menu shows over
//! any tab, as the web's, and its hovers redraw it alone.

use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};

/// The items (the web's order and words) and their tooltips.
pub(crate) const ITEMS: [(&str, &str, IconName, &str); 9] = [
    ("terminal", "Terminal", IconName::Terminal, ""), ("files", "Files", IconName::Folder, ""), ("git", "Git", IconName::GitBranch, ""),
    ("kanban", "Kanban", IconName::Kanban, ""), ("notes", "Notes", IconName::FileText, ""), ("mindmap", "Mindmap", IconName::Network, ""),
    ("media", "Media", IconName::Images, ""), ("factory", "Factory", IconName::Factory, "This project's factory run"),
    ("widget", "Panel", IconName::LayoutDashboard, "Open a panel Claude built"),
];
const FIT_TIP: &str = "Share the width out again so every pane fits the window (pinned panes keep their size)";
pub(crate) const PRESETS: [(&str, &str, IconName); 5] = [
    ("cols2", "2 columns", IconName::Columns2), ("cols3", "3 columns", IconName::Columns3),
    ("cross4", "2\u{00d7}2 cross", IconName::Grid2x2), ("left-right2", "Left + right split", IconName::LayoutPanelLeft),
    ("top-bottom2", "Top + bottom split", IconName::LayoutPanelTop),
];

pub struct AddMenu {
    open: bool,
    at: Point<Pixels>,
    layouts: bool,
    /// Open and Layout's submenu as last drawn: what the driver reports, so
    /// a test's next press finds the menu on screen (or its backdrop gone),
    /// never only in state (F145).
    drawn: (bool, bool),
}

struct Handle(Entity<AddMenu>);
impl Global for Handle {}

/// Made at init and given to the shell as a layer.
pub(crate) fn init(cx: &mut App) {
    let m = cx.new(|_| AddMenu { open: false, at: point(px(0.), px(0.)), layouts: false, drawn: (false, false) });
    cx.set_global(Handle(m.clone()));
    crundi_shell::register_overlay(cx, m.into());
}

fn menu(cx: &App) -> Option<Entity<AddMenu>> { cx.try_global::<Handle>().map(|h| h.0.clone()) }

/// The project bar's "+" ("add-menu"): open it under the button, or close it.
pub fn toggle(window: &mut Window, cx: &mut App) {
    let open = menu(cx).is_some_and(|m| m.read(cx).open);
    set_open(!open, window, cx);
}

/// Open (under the "+") or close.
pub fn set_open(open: bool, window: &mut Window, cx: &mut App) {
    let Some(m) = menu(cx) else { return };
    // The "+" is in the project bar under the top bar, centred in it; the
    // press was on it, so its left edge is half a button left of the pointer.
    let vw = window.viewport_size().width;
    let x = (window.mouse_position().x - T::ICON_BTN / 2.).min(vw - T::MENU_MIN_W - T::SP_6).max(T::SP_6);
    let y = T::TOPBAR_H + (T::TABBAR_H + T::ICON_BTN) / 2. + T::SP_4;
    m.update(cx, |m, cx| {
        if m.open == open { return; }
        m.open = open;
        m.layouts = false;
        if open { m.at = point(x, y); }
        cx.notify();
    });
}

/// For the driver: open, where, the submenu, each item's rect (window px).
pub fn props(cx: &App) -> serde_json::Value {
    let Some(m) = menu(cx) else { return serde_json::Value::Null };
    let m = m.read(cx);
    if !m.drawn.0 { return serde_json::json!({ "open": false }); }
    let (x, y) = (f32::from(m.at.x), f32::from(m.at.y));
    let row = f32::from(row_h()) + 2.;
    let first = y + 1. + 6.;
    let w = f32::from(T::MENU_MIN_W);
    let mut items: Vec<serde_json::Value> = ITEMS.iter().enumerate().map(|(i, (id, label, _, _))| serde_json::json!({ "id": id, "label": label, "x": x + 7., "y": first + i as f32 * row, "w": w - 14., "h": row - 2. })).collect();
    items.push(serde_json::json!({ "id": "fit", "label": "Fit to screen", "x": x + 7., "y": first + 9. * row, "w": w - 14., "h": row - 2. }));
    items.push(serde_json::json!({ "id": "layout", "label": "Layout", "x": x + 7., "y": first + 10. * row, "w": w - 14., "h": row - 2. }));
    let sx = x + w + f32::from(T::SP_4);
    let sub: Vec<serde_json::Value> = PRESETS.iter().enumerate().map(|(i, (id, label, _))| serde_json::json!({ "id": format!("layout:{id}"), "label": label, "x": sx + 7., "y": first + 10. * row + i as f32 * row, "w": w - 14., "h": row - 2. })).collect();
    serde_json::json!({ "open": true, "x": x, "y": y, "layouts": m.drawn.1, "items": items, "sub": sub })
}

/// A menu row's height (w::menu_item's).
fn row_h() -> Pixels { T::SP_24 + T::SP_6 + px(1.) }

impl AddMenu {
    fn pick(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.layouts = false;
        cx.notify();
        crate::add_pane(id, window, cx);
    }
    fn sub(&mut self, on: bool, cx: &mut Context<Self>) { if self.layouts != on { self.layouts = on; cx.notify(); } }
}

impl Render for AddMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("add-menu");
        self.drawn = (self.open, self.open && self.layouts);
        if !self.open { return div().into_any_element(); }
        let at = self.at;
        let mut m = w::menu_panel("add-menu").absolute().left(at.x).top(at.y).w(T::MENU_MIN_W)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (id, label, icon, tip) in ITEMS {
            let it = w::menu_item(SharedString::from(format!("add-{id}")), icon, label, false, false)
                .on_hover(cx.listener(|this, h: &bool, _, cx| if *h { this.sub(false, cx) }))
                .on_click(cx.listener(move |this, _, window, cx| this.pick(id, window, cx)));
            m = m.child(if tip.is_empty() { it } else { it.tooltip(w::tooltip(tip)) });
        }
        m = m.child(w::menu_item("add-fit", IconName::MoveHorizontal, "Fit to screen", false, false).tooltip(w::tooltip(FIT_TIP))
            .on_hover(cx.listener(|this, h: &bool, _, cx| if *h { this.sub(false, cx) }))
            .on_click(cx.listener(|this, _, window, cx| this.pick("fit", window, cx))));
        m = m.child(w::menu_item("add-layout", IconName::Columns2, "Layout", true, self.layouts)
            .on_hover(cx.listener(|this, h: &bool, _, cx| if *h { this.sub(true, cx) }))
            .on_click(cx.listener(|this, _, _, cx| this.sub(true, cx))));
        // The submenu: 4px right of the menu, its first row level with Layout.
        let row = row_h() + T::SP_2;
        let layout_top = at.y + px(1.) + T::SP_6 + row * (ITEMS.len() as f32 + 1.);
        let sub = self.layouts.then(|| {
            let mut s = w::menu_panel("add-layouts").absolute().left(at.x + T::MENU_MIN_W + T::SP_4).top(layout_top - T::SP_6 - px(1.)).w(T::MENU_MIN_W)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
            for (id, label, icon) in PRESETS {
                s = s.child(w::menu_item(SharedString::from(format!("layout-{id}")), icon, label, false, false)
                    .on_click(cx.listener(move |this, _, window, cx| this.pick(&format!("layout:{id}"), window, cx))));
            }
            s
        });
        div().id("add-menu-back").size_full().occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| { this.open = false; this.layouts = false; cx.notify(); }))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| { this.open = false; this.layouts = false; cx.notify(); }))
            .child(m)
            .when_some(sub, |d, s| d.child(s))
            .into_any_element()
    }
}
