//! Every control the client draws, built once on the theme's tokens (the
//! owner's rule: an area never draws its own copy of one; a duplicate is a
//! blocking finding). gpui-component is wrapped here, once: screens use
//! these functions, not gpui-component's controls directly. A control two
//! areas need and that isn't here yet is a request to the Lead (AREAS.md).
//!
//! The web's names, for finding the reference:
//!   button / primary / danger / ghost   .svc-btn, .svc-add-btn, the dialogs' buttons
//!   icon_button / icon_toggle / toggle  the bars' and the head's buttons
//!   switch, segmented, check            settings' switches, .seg-pref, ticks
//!   input (gpui-component's text box)   .sm-input and the message box
//!   menu_button, context menus          .ctx-menu and the dropdowns
//!   chip, badge, count_badge, tag       chips, CHAT / PLANNING tags, counts, .svc-status
//!   dot, status_badge                   state dots, the connection badge
//!   card, panel, pane_head, toolbar     .svc-card, a pane and its head, bars
//!   tab_row / tab, nav_tab              the drawer's tabs, the project bar's tabs
//!   sidebar_item, dashed_button, outline_button   the sidebar
//!   empty, error_state, loading         the panels' empty, failed and loading lines
//!   list_row                            .file-item
//!   confirm, toast                      confirm(), the toast

use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    Disableable as _, WindowExt as _,
};

pub use crundi_theme::IconName;
pub mod control; pub use control::{sized, sized_text, icon_of, Ctl};
pub use gpui_component::input::{Input, InputEvent, InputState};
// One file per control (AREAS.md "A new control"), each once.
pub mod modal; pub use modal::{modal, modal_layer, modal_open, ModalLayer};
mod spinner; pub use spinner::spinner;
mod shimmer; pub use shimmer::shimmer;
mod markdown; pub use markdown::markdown;
pub mod tip; pub use tip::{tip, tooltip};
mod icon; pub use icon::icon;
pub mod cru_mark; pub use cru_mark::cru_mark;
pub mod brand_mark; pub use brand_mark::{brand_head, brand_mark};
pub mod ref_drag; pub use ref_drag::{drag_ref, ref_target, ref_text, RefDrag};
pub mod textarea; pub use textarea::{code_editor, code_state, message_box, textarea, Editor, EditorState, Textarea, TextareaState};
pub mod square; pub use square::{square_button, SquareLook};
pub mod dropdown; pub use dropdown::{dropdown, dropdown_of};
pub mod tone_button; pub use tone_button::tone_button;
pub mod stop_button; pub use stop_button::stop_button;
pub mod choice; pub use choice::choice_mark;
pub mod choice_row; pub use choice_row::choice_row;
pub mod suggest_row; pub use suggest_row::suggest_row;
pub mod overlay; pub use overlay::{overlay_modal, modal_head, Modal};
mod ask; pub use ask::{ask, ask_confirm, ask_answer, ask_props, ask_layer, menu_at, menu_pick, menu_props, Ask, MenuEntry};
pub mod queue_drawer; pub use queue_drawer::{queue_drawer, Drawer};
pub mod send_button; pub use send_button::send_button;
pub mod disclosure; pub use disclosure::disclosure;
pub mod filter_chip; pub use filter_chip::filter_chip;
pub mod kbd; pub use kbd::kbd;
pub mod key_row; pub use key_row::key_row;
pub mod page_item; pub use page_item::page_item;
pub mod meter; pub use meter::meter;
pub mod seg_tabs; pub use seg_tabs::{seg_tabs, SegTab};
pub mod arm_button; pub use arm_button::{arm_button, ArmButton};
pub mod select; pub use select::select;
pub mod link; pub use link::link;
pub mod spark; pub use spark::{spark, Series};

mod head_button;
pub use head_button::{head_button, menu_item, menu_panel, HeadContent, HeadLook};
pub mod pick_menu; pub use pick_menu::{pick_menu, PickItem};
pub mod segmented_icons; pub use segmented_icons::segmented_icons;
pub mod icon_tag; pub use icon_tag::icon_tag;
pub mod round_button; pub use round_button::round_button;
pub mod liquid_mic; pub use liquid_mic::{liquid_mic, radial_glow, Liquid, MicLook};
pub mod label_icon_button; pub use label_icon_button::{label_icon_button, Look};
pub mod leaf_chip; pub use leaf_chip::{leaf_act, leaf_chip};
pub mod launch_button; pub use launch_button::{launch_button, LaunchLook};

// ─── Buttons ───

/// A quiet text button (Cancel, Retry, a card's action). Every button's
/// dimensions are the theme's (control.rs).
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    sized(Button::new(id), Some(label.into()), None, false, Ctl::Normal)
}

/// A button with an icon before its label (a card's Logs, Restart…).
pub fn icon_label_button(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Button {
    sized(Button::new(id), Some(label.into()), Some(icon), false, Ctl::Normal)
}

/// The accent button with an icon before its label (Upload, + Register service).
pub fn primary_icon(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Button {
    sized(Button::new(id).primary(), Some(label.into()), Some(icon), false, Ctl::Normal)
}

/// The accent button (+ Register service, Schedule…).
pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    sized(Button::new(id).primary(), Some(label.into()), None, false, Ctl::Normal)
}

/// A destructive button (Delete, Remove).
pub fn danger(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    sized(Button::new(id).danger(), Some(label.into()), None, false, Ctl::Normal)
}

/// A text button with no fill until hovered.
pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    sized(Button::new(id).ghost(), Some(label.into()), None, false, Ctl::Normal)
}

/// An icon-only button with a tooltip (the bars' buttons): a square of the
/// theme's ICON_BTN, its icon ICON_BTN_ICON.
pub fn icon_button(id: impl Into<ElementId>, icon: IconName, tip: impl Into<SharedString>) -> Button {
    sized(Button::new(id).ghost(), None, Some(icon), false, Ctl::Normal).tooltip(tip.into())
}

/// An icon button that stays pressed while `on` (Annotate, pin).
pub fn icon_toggle(id: impl Into<ElementId>, icon: IconName, tip: impl Into<SharedString>, on: bool) -> Button {
    let b = Button::new(id);
    let b = if on { b.primary() } else { b.ghost() };
    sized(b, None, Some(icon), false, Ctl::Normal).tooltip(tip.into())
}

/// A text button that stays pressed while `on` (a device, a filter).
pub fn toggle(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Button {
    let b = Button::new(id);
    let b = if on { b.primary() } else { b.ghost() };
    sized(b, Some(label.into()), None, false, Ctl::Normal)
}

/// A button's look by name, for buttons built elsewhere (dialogs).
pub fn variant(danger: bool) -> ButtonVariant {
    if danger { ButtonVariant::Danger } else { ButtonVariant::Primary }
}

/// Disable a button (a thin re-export, so areas need not import the trait).
pub fn disabled(b: Button, off: bool) -> Button { b.disabled(off) }

// ─── Choices ───

/// An on/off switch (settings).
pub fn switch(id: impl Into<ElementId>, on: bool, toggled: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Switch {
    Switch { id: id.into(), on, disabled: false, toggled: std::rc::Rc::new(toggled) }
}

/// The switch (every dimension a token: F69); off, the track shows on a card (F67).
#[derive(IntoElement)]
pub struct Switch { id: ElementId, on: bool, disabled: bool, toggled: std::rc::Rc<dyn Fn(&bool, &mut Window, &mut App)> }
impl gpui_component::Disableable for Switch { fn disabled(mut self, d: bool) -> Self { self.disabled = d; self } }
impl RenderOnce for Switch {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (on, off, f) = (self.on, self.disabled, self.toggled);
        div().id(self.id).flex_none().flex().items_center().w(T::TOGGLE_W).h(T::TOGGLE_H).p(T::TOGGLE_INSET).rounded(T::R_PILL)
            .bg(if on { T::accent() } else { T::bg_hover() }).border_1().border_color(if on { T::accent() } else { T::text_muted() })
            .when(on, |d| d.justify_end())
            .when(off, |d| d.opacity(0.5))
            .when(!off, |d| d.cursor_pointer().on_click(move |_, window, cx| f(&!on, window, cx)))
            .child(div().flex_none().size(T::TOGGLE_THUMB).rounded(T::R_PILL).bg(if on { T::white() } else { T::text_secondary() }))
    }
}

/// A ticked row: the box and its label.
pub fn check(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool, toggled: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Check {
    Check { id: id.into(), label: label.into(), on, disabled: false, toggled: std::rc::Rc::new(toggled) }
}

/// The checkbox (every dimension a token: F69).
#[derive(IntoElement)]
pub struct Check { id: ElementId, label: SharedString, on: bool, disabled: bool, toggled: std::rc::Rc<dyn Fn(&bool, &mut Window, &mut App)> }
impl gpui_component::Disableable for Check { fn disabled(mut self, d: bool) -> Self { self.disabled = d; self } }
impl RenderOnce for Check {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (on, off, f, label) = (self.on, self.disabled, self.toggled, self.label);
        div().id(self.id).flex().items_center().gap(T::CHECK_GAP)
            .when(off, |d| d.opacity(0.5))
            .when(!off, |d| d.cursor_pointer().on_click(move |_, window, cx| f(&!on, window, cx)))
            .child(div().flex_none().flex().items_center().justify_center().size(T::CHECK_BOX).rounded(T::R_CHIP).border_1()
                .border_color(if on { T::accent() } else { T::text_muted() }).bg(if on { T::accent() } else { T::bg() })
                .when(on, |d| d.text_color(T::white()).child(crate::icon(IconName::Check, T::CHECK_BOX - T::SP_2 * 2.))))
            .when(!label.is_empty(), |d| d.child(div().min_w_0().text_size(T::CHECK_TEXT).text_color(T::text()).child(label)))
    }
}

/// Two or more choices side by side, one on (the Enter-sends switch, .seg-pref).
pub fn segmented(id: &'static str, labels: &[&'static str], on: usize, pick: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Div {
    let pick = std::rc::Rc::new(pick);
    let mut d = div().flex().items_center().p(T::SP_2).gap(T::SP_2).rounded(T::R_PILL).bg(T::bg_card()).border_1().border_color(T::border());
    for (i, l) in labels.iter().copied().enumerate() {
        let pick = pick.clone();
        d = d.child(div().id((id, i)).px(T::SP_6).rounded(T::R_PILL).cursor_pointer().text_size(T::TEXT_11)
            .text_color(if i == on { T::accent_hover() } else { T::text_secondary() })
            .when(i == on, |d| d.bg(T::accent_dim()))
            .child(l)
            .on_click(move |_, w, cx| pick(&i, w, cx)));
    }
    d
}

// ─── Text boxes ───

/// A one-line text box over an InputState (create the state once, with
/// `InputState::new(window, cx)`, and keep it in the view).
pub fn input(state: &Entity<InputState>) -> Input {
    Input::new(state).h(T::INPUT_H).px(T::INPUT_PAD_X).text_size(T::INPUT_TEXT)
}

/// A form field's label: small, upper case, muted, wide.
pub fn field_label(text: impl Into<SharedString>) -> Div {
    div().text_size(T::TEXT_11).text_color(T::text_muted()).child(text.into())
}

// ─── Menus ───

/// An icon button that opens a menu of choices, the current one ticked
/// (the drawer's Network and Device). `pick` runs on the view. One menu
/// control: this is `pick_menu` with fixed items.
pub fn menu_button<V: 'static>(id: &'static str, icon: IconName, tip: &'static str, items: &[&'static str], on: usize, view: WeakEntity<V>, pick: impl Fn(&mut V, usize, &mut Context<V>) + 'static) -> impl IntoElement {
    let items: Vec<&'static str> = items.to_vec();
    let pick = std::rc::Rc::new(pick);
    pick_menu(id, icon, tip, "", move || items.iter().enumerate().map(|(i, l)| PickItem { id: i.to_string(), label: (*l).into(), kind: "item", on: i == on, icon: None }).collect(),
        move |id, _, cx| { let i: usize = id.parse().unwrap_or(0); let pick = pick.clone(); let _ = view.update(cx, |v, cx| pick(v, i, cx)); })
}

/// One item of a menu: an id for the answer, its label, an icon, danger.
#[derive(Clone)]
pub struct MenuItem { pub id: &'static str, pub label: SharedString, pub icon: Option<IconName>, pub danger: bool, pub sep_before: bool }

/// A "…" button opening a menu of actions (a row's, a head's): `pick_menu`
/// with these items.
pub fn more_menu<V: 'static>(id: impl Into<ElementId>, items: Vec<MenuItem>, view: WeakEntity<V>, pick: impl Fn(&mut V, &'static str, &mut Context<V>) + 'static) -> impl IntoElement {
    let pick = std::rc::Rc::new(pick);
    let ids: Vec<&'static str> = items.iter().map(|i| i.id).collect();
    pick_menu(id, IconName::Ellipsis, "More", "", move || {
        let mut out = vec![];
        for it in &items {
            if it.sep_before { out.push(PickItem { kind: "group", ..Default::default() }); }
            out.push(PickItem { id: it.id.to_string(), label: it.label.clone(), kind: "item", on: false, icon: it.icon.clone() });
        }
        out
    }, move |id, _, cx| {
        let Some(item) = ids.iter().copied().find(|i| *i == id) else { return };
        let pick = pick.clone();
        let _ = view.update(cx, |v, cx| pick(v, item, cx));
    })
}

// ─── Labels and marks ───

/// A small coloured dot (a chat's state, the connection).
pub fn dot(color: Hsla) -> Div {
    div().flex_none().size(T::DOT).rounded_full().bg(color)
}

/// A project's dot in the sidebar, by the web UI's priority (renderProjects;
/// the Slint client's ProjectDot): needs input accent, working yellow,
/// waiting sky, running green, nothing running muted.
pub fn project_dot_color(state: &str) -> Hsla {
    match state {
        "needs-input" => T::accent(),
        "working" => T::yellow(),
        "waiting" => T::sky(),
        "running" => T::green(),
        _ => T::text_muted(),
    }
}

/// The colour of an agent state (idle, working, needs-input, error, exited).
pub fn state_color(state: &str) -> Hsla {
    match state {
        "working" => T::green(),
        "needs-input" | "waiting" => T::amber(),
        "error" => T::red(),
        "exited" => T::text_muted(),
        _ => T::text_secondary(),
    }
}

/// A small tag in capitals (CHAT, PLANNING).
pub fn badge(text: impl Into<SharedString>) -> Div {
    div().flex_none().px(T::SP_6).rounded(T::R_CHIP).bg(T::accent_dim())
        .text_color(T::accent_hover()).text_size(T::TEXT_11).font_weight(T::W_BOLD)
        .child(text.into())
}

/// A count on a tab or a button (the inbox, the factories); "" draws nothing.
pub fn count_badge(text: impl Into<SharedString>) -> Div {
    let t: SharedString = text.into();
    div().flex_none().when(t.is_empty(), |d| d.invisible()).min_w(T::SP_16).h(T::SP_16).px(T::SP_4).rounded(T::R_PILL)
        .bg(T::accent()).text_color(T::white()).text_size(T::TEXT_10).font_weight(T::W_BOLD)
        .flex().items_center().justify_center().child(t)
}

/// A status pill: mono, upper case (.svc-status). tone: running/ok, error, warn, accent, "".
pub fn tag(text: impl Into<SharedString>, tone: &str) -> Div {
    let (fg, bg) = match tone {
        "running" | "ok" => (T::green(), T::green_dim()),
        "error" => (T::red(), T::red_dim()),
        "warn" => (T::yellow(), T::yellow_dim()),
        "accent" => (T::accent_hover(), T::accent_dim()),
        "sky" => (T::sky(), T::sky_dim()),
        _ => (T::text_muted(), T::bg_tertiary()),
    };
    div().flex_none().px(T::SP_8).rounded(T::R_PILL).bg(bg).text_color(fg).text_size(T::TEXT_10).font_family(T::MONO).child(text.into())
}

/// A small bordered chip with an icon (the permission mode); red when `hot`.
pub fn chip(icon: IconName, label: impl Into<SharedString>, hot: bool) -> Div {
    div().flex().items_center().gap(T::SP_4).h(T::SP_20 + T::SP_2).px(T::SP_6).rounded(T::R_CHIP).border_1()
        .border_color(if hot { T::red() } else { T::border() })
        .text_color(if hot { T::error_text() } else { T::text_secondary() })
        .child(div().text_size(T::TEXT_11).child(icon))
        .child(label.into())
}

/// The top bar's connection pill (connected / offline).
pub fn status_badge(label: impl Into<SharedString>, ok: bool) -> Div {
    div().flex().items_center().gap(T::SP_6).h(T::SP_24).px(T::SP_10).rounded(T::R_PILL)
        .bg(if ok { T::green_dim() } else { T::red_dim() })
        .text_color(if ok { T::green() } else { T::error_text() }).text_size(T::TEXT_11).font_family(T::MONO)
        .child(dot(if ok { T::green() } else { T::red() }))
        .child(label.into())
}

// ─── Frames ───

/// A bordered card (.svc-card, tool calls, notices).
pub fn card() -> Div {
    div().flex().flex_col().rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg_card())
}

/// A workbench pane: the frame, its head, then the body filling the rest.
pub fn panel(head: impl IntoElement, body: impl IntoElement) -> Div {
    div()
        .flex().flex_col().size_full().min_h_0().min_w_0()
        .bg(T::bg_secondary()).border_1().border_color(T::border()).rounded(T::R_CARD)
        .overflow_hidden()
        .child(head)
        .child(div().flex().flex_col().flex_1().min_h_0().child(body))
}

/// A pane's head: its state dot, the title, then whatever goes on the right
/// (add the buttons as children).
pub fn pane_head(state: &str, title: impl Into<SharedString>) -> Div {
    div()
        .flex().items_center().gap(T::SP_8).h(T::PANE_HEAD).px(T::SP_10).flex_none()
        .border_b_1().border_color(T::border())
        .child(dot(state_color(state)))
        .child(div().flex_1().min_w_0().font_weight(T::W_SEMIBOLD).text_size(T::TEXT_13).text_color(T::text()).truncate().child(title.into()))
}

/// A bar of controls (the address bar, a panel's toolbar).
pub fn toolbar() -> Div {
    div().flex().flex_none().items_center().gap(T::SP_6).px(T::SP_8).py(T::SP_6).border_b_1().border_color(T::border())
}

/// A panel's title row: the title, then its actions on the right (.svc-toolbar).
pub fn title_bar(title: impl Into<SharedString>) -> Div {
    div().flex().flex_none().items_center().gap(T::SP_8).pb(T::SP_4)
        .child(div().flex_1().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(title.into()))
}

/// A tab's page: padding and gap, scrolling (.services-panel).
pub fn page(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).flex().flex_col().size_full().p(T::SP_16).gap(T::SP_12).overflow_y_scroll().bg(T::bg())
}

// ─── Tabs ───

/// A row of tabs (the drawer's): tabs sit on its bottom edge. The project
/// tab bar's height (TABBAR_H, its border inside it), so the two bars' lines
/// run straight across the drawer's divider.
pub fn tab_row() -> Div {
    div().flex().flex_none().items_end().gap(T::SP_2).px(T::SP_6).pt(T::SP_4).h(T::TABBAR_H).bg(T::bg()).border_b_1().border_color(T::border())
}

/// One tab in a tab row; the active one joins the area below it.
pub fn tab(title: impl Into<SharedString>, on: bool) -> Div {
    let d = div().flex().items_center().gap(T::SP_6).h(T::CONTROL_SM).min_w(T::SP_48 + T::SP_16).max_w(T::DRAWER_W / 2.8).pl(T::SP_10).pr(T::SP_6)
        .rounded_t(T::R_CONTROL).text_size(T::TEXT_11)
        .child(div().flex_1().min_w_0().truncate().child(title.into()));
    if on { d.bg(T::bg_secondary()).border_1().border_b_0().border_color(T::border()).text_color(T::text()) } else { d.text_color(T::text_secondary()) }
}

/// A project-bar tab (Workbench, Git, Files…): an icon and a label; the
/// active one is underlined in the accent.
pub fn nav_tab(icon: IconName, label: impl Into<SharedString>, on: bool) -> Div {
    div().flex().flex_none().items_center().gap(T::SP_6).h_full().px(T::SP_10)
        .border_b_2().border_color(if on { T::accent() } else { gpui::transparent_black() })
        .text_size(T::TEXT_13).font_weight(T::W_MEDIUM)
        .text_color(if on { T::accent_hover() } else { T::text_muted() })
        .child(div().text_size(T::TEXT_14).child(icon))
        .child(label.into())
}

// ─── The sidebar ───

/// A project in the sidebar (the Slint client's and the web's row): its
/// dot (with a soft ring while it needs input, works or waits; green for
/// the open project with nothing running) and its name; the open one
/// accent-dim with the accent bar at its left.
/// `icon`: the project's icon (18, rounded, after the dot), as the web's .proj-ico.
pub fn sidebar_item(name: impl Into<SharedString>, state: &str, on: bool, icon: Option<std::sync::Arc<Image>>) -> Stateful<Div> {
    let name: SharedString = name.into();
    let color = if state.is_empty() && on { T::green() } else { project_dot_color(state) };
    let ring = match state { "needs-input" => Some(T::accent_dim()), "working" => Some(T::yellow_dim()), "waiting" => Some(T::sky_dim()), _ => None };
    let dot = div().flex_none().size(T::SP_12).flex().items_center().justify_center()
        .when_some(ring, |d, r| d.rounded(T::R_PILL).bg(r))
        .child(div().size(T::DOT).rounded(T::R_PILL).bg(color));
    // An id: GPUI follows the pointer's hover (and draws the row again when
    // it comes or goes) only on an element with one. Without it the
    // highlight stayed on a row the pointer had left (the owner, 19 Nov).
    let d = div().id(SharedString::from(format!("sidebar-item:{name}"))).relative().flex().items_center().gap(T::SP_10).h(T::SP_32).pl(T::SP_10).pr(T::SP_8).rounded(T::R_CONTROL)
        .child(dot)
        .children(icon.map(project_icon))
        .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_14).child(name));
    if on {
        d.bg(T::accent_dim()).text_color(T::accent_hover())
            .child(div().absolute().left(px(0.)).top(T::SP_8).bottom(T::SP_4).w(T::SP_4).rounded_r(T::R_CHIP).bg(T::accent()))
    } else { d.text_color(T::text_secondary()).hover(|d| d.bg(T::bg_hover()).text_color(T::text())) }
}

/// A project in the folded rail (the Slint client's avatar): its initials
/// in a round badge, the border in the state's colour (running green,
/// working yellow, needs input accent); the open one filled with the accent.
/// With an `icon`, the icon fills the badge instead of the letters (the web's .proj-initials.has-ico).
pub fn rail_avatar(initials: impl Into<SharedString>, state: &str, on: bool, icon: Option<std::sync::Arc<Image>>) -> Div {
    let st = if state == "waiting" { "" } else { state };
    let border = match st { "running" => T::green(), "working" => T::yellow(), "needs-input" => T::accent(), _ if on => T::accent(), _ => T::border() };
    let fg = match st { "running" | "working" => T::text(), "needs-input" => T::white(), _ if on => T::white(), _ => T::text_secondary() };
    // A state shows as a ring the rail can read at a glance (2 px); at rest a hairline.
    let ringed = matches!(st, "running" | "working" | "needs-input");
    div().flex_none().size(T::CONTROL_LG).rounded(T::R_PILL).flex().items_center().justify_center()
        .when(ringed, |d| d.border_2()).when(!ringed, |d| d.border_1()).border_color(border)
        .bg(if on { T::accent() } else { T::bg_tertiary() })
        .text_color(fg).font_family(T::MONO).text_size(T::TEXT_12).font_weight(T::W_BOLD)
        .overflow_hidden()
        .map(|d| match icon {
            Some(i) => d.bg(T::bg()).child(img(i).size_full().rounded(T::R_PILL).object_fit(ObjectFit::Cover)),
            None => d.child(initials.into()),
        })
}

/// A project's icon, as the web's .proj-ico and #current-project-ico: 18, rounded, cover.
pub fn project_icon(i: std::sync::Arc<Image>) -> Img {
    img(i).flex_none().size(T::PROJECT_ICON).rounded(T::R_CHIP).object_fit(ObjectFit::Cover)
}

/// A full-width button with a dashed border (+ Add project).
pub fn dashed_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).flex().items_center().justify_center().h(T::CONTROL).rounded(T::R_CONTROL).border_1().border_dashed()
        .border_color(T::border()).text_color(T::text_muted()).cursor_pointer().hover(|d| d.bg(T::accent_dim()).text_color(T::accent_hover())).child(label.into())
}

/// A full-width outlined button with an icon (Inbox).
pub fn outline_button(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).flex().items_center().justify_center().gap(T::SP_8).h(T::CONTROL).rounded(T::R_CONTROL).border_1()
        .border_color(T::border_subtle()).text_color(T::text_muted()).cursor_pointer().hover(|d| d.bg(T::bg_hover()))
        .child(icon).child(label.into())
}

// ─── States ───

/// Nothing to show: a line (and a hint), centred.
pub fn empty(text: impl Into<SharedString>) -> Div {
    div().flex().flex_col().flex_1().items_center().justify_center().gap(T::SP_12).p(T::SP_24)
        .text_size(T::TEXT_14).text_color(T::text_muted()).child(text.into())
}

/// A load that failed: the server's words in red, and Retry.
pub fn error_state(id: impl Into<ElementId>, text: impl Into<SharedString>, retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    div().flex().flex_col().flex_1().items_center().justify_center().gap(T::SP_12).p(T::SP_24)
        .child(div().text_size(T::TEXT_13).text_color(T::red()).child(text.into()))
        .child(button(id, "Retry").on_click(retry))
}

/// Loading: the spinner and a muted line.
pub fn loading(text: impl Into<SharedString>) -> Div {
    div().flex().items_center().justify_center().gap(T::SP_8).p(T::SP_24)
        .child(spinner::spinner(T::SPINNER))
        .child(div().text_size(T::TEXT_13).text_color(T::text_muted()).child(text.into()))
}

/// A web page whose browser stopped (its page process, or the whole
/// browser host, crashed): the words and Reload, over the page's place.
pub fn page_crashed(id: impl Into<ElementId>, reload: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap(T::SP_12).p(T::SP_24).bg(T::bg())
        .child(div().text_color(T::text_muted()).child(icon(IconName::TriangleAlert, T::ICON_LG)))
        .child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child("This page crashed"))
        .child(icon_label_button(id, IconName::RotateCcw, "Reload").on_click(reload))
}

/// A web page on its way (the browser starting, the first frame not in
/// yet): a quiet spinner over the page's place.
pub fn page_loading() -> Div {
    div().absolute().inset_0().flex().items_center().justify_center()
        .child(spinner(T::SPINNER))
}

// ─── Lists ───

/// A list row (.file-item): icon, title, an optional second line, meta on
/// the right; hover and selected. Add a "…" (more_menu) as a child.
pub fn list_row(id: impl Into<ElementId>, icon: IconName, title: impl Into<SharedString>, sub: impl Into<SharedString>, meta: impl Into<SharedString>, selected: bool) -> Stateful<Div> {
    let sub: SharedString = sub.into();
    let meta: SharedString = meta.into();
    div().id(id).flex().items_center().gap(T::SP_8).min_h(T::CONTROL).px(T::SP_8).rounded(T::R_CONTROL).cursor_pointer()
        .when(selected, |d| d.bg(T::accent_dim())).when(!selected, |d| d.hover(|d| d.bg(T::bg_hover())))
        .child(div().text_color(T::text_muted()).child(icon))
        .child(div().flex().flex_col().flex_1().min_w_0()
            .child(div().text_size(T::TEXT_13).text_color(T::text()).truncate().child(title.into()))
            .when(!sub.is_empty(), |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).truncate().child(sub))))
        .when(!meta.is_empty(), |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(meta)))
}

// ─── Dialogs and toasts ───

/// What a confirm says: its title, its words, the action's label, and
/// whether the action is destructive (red).
pub struct Confirm { pub title: SharedString, pub message: SharedString, pub ok: SharedString, pub danger: bool }

/// Ask first (the web's confirm()): `on_ok` runs on OK; Cancel, Esc or the
/// scrim close it. The ask layer's confirm (one confirm for the whole
/// window: the shell draws the layer; the driver reads and answers it
/// through ask_props / ask_answer).
pub fn confirm(window: &mut Window, cx: &mut App, c: Confirm, on_ok: impl Fn(&mut Window, &mut App) + 'static) {
    ask_confirm(window, cx, Ask { title: c.title, message: c.message, ok: c.ok, danger: c.danger, prompt: None, placeholder: SharedString::default() },
        move |ok, w, cx| if ok { on_ok(w, cx) });
}

/// Close the dialog on top (the test driver's Cancel, Esc).
pub fn close_dialog(window: &mut Window, cx: &mut App) { if !modal::close(window, cx) { window.close_dialog(cx); } }

/// A dialog is open.
pub fn dialog_open(window: &mut Window, cx: &mut App) -> bool { modal::modal_open(cx) || !ask_props(cx).is_null() || window.has_active_dialog(cx) }

/// The toast, top right: info, error or success.
/// The toast on show, as long as it shows (the Slint client's 3.5 s): the
/// one record of it, for the test driver's "toast" prop.
#[derive(Default)]
struct LastToast(Option<(SharedString, std::time::Instant)>);
impl Global for LastToast {}

pub fn toast_text(cx: &App) -> SharedString {
    cx.try_global::<LastToast>().and_then(|t| t.0.clone())
        .filter(|(_, at)| at.elapsed() < std::time::Duration::from_millis(3500)).map(|(t, _)| t).unwrap_or_default()
}

pub fn toast(window: &mut Window, cx: &mut App, text: impl Into<SharedString>, kind: Toast) {
    let text: SharedString = text.into();
    cx.set_global(LastToast(Some((text.clone(), std::time::Instant::now()))));
    let n = match kind {
        Toast::Info => gpui_component::notification::Notification::info(text),
        Toast::Error => gpui_component::notification::Notification::error(text),
        Toast::Success => gpui_component::notification::Notification::success(text),
    };
    // One toast at a time, as the web's: a new one replaces the one on show
    // (never two stacked over each other: F51, F59).
    window.push_notification(n.id::<ToastSlot>(), cx);
}

/// The one toast's slot (its notification id).
struct ToastSlot;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Toast { Info, Error, Success }

/// Take every toast down (an error that an action has since answered, as a
/// right PIN after a wrong one).
pub fn clear_toasts(window: &mut Window, cx: &mut App) {
    cx.set_global(LastToast(None));
    window.clear_notifications(cx);
}

// ─── Dividers ───

/// The draggable divider between two parts (the drawer's): the accent while
/// dragged or hovered.
pub fn divider(id: impl Into<ElementId>, dragging: bool) -> Stateful<Div> {
    div().id(id).flex_none().w(T::DIVIDER).h_full().cursor_col_resize()
        .bg(if dragging { T::accent() } else { T::border() })
        .hover(|d| d.bg(T::accent()))
}

/// A hairline between rows or sections.
pub fn rule() -> Div {
    div().flex_none().h(px(1.)).w_full().bg(T::border_subtle())
}
