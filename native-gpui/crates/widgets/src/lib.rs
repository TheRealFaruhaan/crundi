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
    dialog::DialogButtonProps,
    Disableable as _, Sizable as _, WindowExt as _,
};

pub use crundi_theme::IconName;
pub use gpui_component::input::{Input, InputEvent, InputState};
pub mod square; pub use square::{square_button, SquareLook};
pub mod textarea; pub use textarea::{textarea, Textarea, TextareaState};
pub mod dropdown; pub use dropdown::dropdown;
pub mod tip; pub use tip::tip;
pub mod tone_button; pub use tone_button::tone_button;
pub mod stop_button; pub use stop_button::stop_button;
pub mod choice_row; pub use choice_row::choice_row;
pub mod suggest_row; pub use suggest_row::suggest_row;
pub mod overlay; pub use overlay::{overlay_modal, modal_head, Modal};
pub mod queue_drawer; pub use queue_drawer::{queue_drawer, Drawer};
pub mod send_button; pub use send_button::send_button;

// ─── Buttons ───

/// A quiet text button (Cancel, Retry, a card's action).
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).label(label.into()).small()
}

/// The accent button (Send-like actions, + Register service).
pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).label(label.into()).small().primary()
}

/// A destructive button (Delete, Remove).
pub fn danger(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).label(label.into()).small().danger()
}

/// A text button with no fill until hovered.
pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).label(label.into()).small().ghost()
}

/// A small icon-only button with a tooltip (the bars' and the head's buttons).
pub fn icon_button(id: impl Into<ElementId>, icon: IconName, tip: impl Into<SharedString>) -> Button {
    Button::new(id).icon(icon).ghost().small().tooltip(tip.into())
}

/// An icon button that stays pressed while `on` (Annotate, pin).
pub fn icon_toggle(id: impl Into<ElementId>, icon: IconName, tip: impl Into<SharedString>, on: bool) -> Button {
    let b = Button::new(id).icon(icon).small().tooltip(tip.into());
    if on { b.primary() } else { b.ghost() }
}

/// A text button that stays pressed while `on` (a device, a filter).
pub fn toggle(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Button {
    let b = Button::new(id).label(label.into()).compact();
    if on { b.primary() } else { b.ghost() }
}

/// A button's look by name, for buttons built elsewhere (dialogs).
pub fn variant(danger: bool) -> ButtonVariant {
    if danger { ButtonVariant::Danger } else { ButtonVariant::Primary }
}

/// Disable a button (a thin re-export, so areas need not import the trait).
pub fn disabled(b: Button, off: bool) -> Button { b.disabled(off) }

// ─── Choices ───

/// An on/off switch (settings).
pub fn switch(id: impl Into<ElementId>, on: bool, toggled: impl Fn(&bool, &mut Window, &mut App) + 'static) -> gpui_component::switch::Switch {
    gpui_component::switch::Switch::new(id).checked(on).small().on_click(toggled)
}

/// A ticked row: the box and its label.
pub fn check(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool, toggled: impl Fn(&bool, &mut Window, &mut App) + 'static) -> gpui_component::checkbox::Checkbox {
    gpui_component::checkbox::Checkbox::new(id).label(label.into()).checked(on).small().on_click(toggled)
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
    Input::new(state).small()
}

/// A form field's label: small, upper case, muted, wide.
pub fn field_label(text: impl Into<SharedString>) -> Div {
    div().text_size(T::TEXT_11).text_color(T::text_muted()).child(text.into())
}

// ─── Menus ───

/// An icon button that opens a menu of choices, the current one ticked
/// (the drawer's Network and Device). `pick` runs on the view.
pub fn menu_button<V: 'static>(id: &'static str, icon: IconName, tip: &'static str, items: &[&'static str], on: usize, view: WeakEntity<V>, pick: impl Fn(&mut V, usize, &mut Context<V>) + 'static) -> impl IntoElement {
    use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
    let items: Vec<&'static str> = items.to_vec();
    let pick = std::rc::Rc::new(pick);
    icon_button(id, icon, tip).dropdown_menu(move |mut menu, _, _| {
        for (i, label) in items.iter().enumerate() {
            let (view, pick) = (view.clone(), pick.clone());
            menu = menu.item(PopupMenuItem::new(*label).checked(i == on).on_click(move |_, _, cx| {
                let _ = view.update(cx, |v, cx| pick(v, i, cx));
            }));
        }
        menu
    })
}

/// One item of a menu: an id for the answer, its label, an icon, danger.
#[derive(Clone)]
pub struct MenuItem { pub id: &'static str, pub label: SharedString, pub icon: Option<IconName>, pub danger: bool, pub sep_before: bool }

/// A "…" button opening a menu of actions (a row's, a head's).
pub fn more_menu<V: 'static>(id: impl Into<ElementId>, items: Vec<MenuItem>, view: WeakEntity<V>, pick: impl Fn(&mut V, &'static str, &mut Context<V>) + 'static) -> impl IntoElement {
    use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
    let pick = std::rc::Rc::new(pick);
    icon_button(id, IconName::Ellipsis, "More").dropdown_menu(move |mut menu, _, _| {
        for it in items.iter() {
            if it.sep_before { menu = menu.separator(); }
            let (view, pick, item_id) = (view.clone(), pick.clone(), it.id);
            let mut m = PopupMenuItem::new(it.label.clone());
            if let Some(i) = it.icon.clone() { m = m.icon(i); }
            menu = menu.item(m.on_click(move |_, _, cx| { let _ = view.update(cx, |v, cx| pick(v, item_id, cx)); }));
        }
        menu
    })
}

// ─── Labels and marks ───

/// A small coloured dot (a chat's state, the connection).
pub fn dot(color: Hsla) -> Div {
    div().flex_none().size(T::DOT).rounded_full().bg(color)
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

/// A row of tabs (the drawer's): tabs sit on its bottom edge.
pub fn tab_row() -> Div {
    div().flex().flex_none().items_end().gap(T::SP_2).px(T::SP_6).pt(T::SP_4).min_h(T::CONTROL).bg(T::bg()).border_b_1().border_color(T::border())
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

/// A project in the sidebar: its dot and name; the open one highlighted.
pub fn sidebar_item(name: impl Into<SharedString>, state: &str, on: bool) -> Div {
    let d = div().flex().items_center().gap(T::SP_8).h(T::CONTROL_LG).px(T::SP_12).rounded(T::R_FIELD)
        .child(dot(state_color(state)))
        .child(div().flex_1().min_w_0().truncate().child(name.into()));
    if on { d.bg(T::accent_dim()).border_l_2().border_color(T::accent()).text_color(T::accent_hover()) } else { d.text_color(T::text_secondary()) }
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
        .child(gpui_component::spinner::Spinner::new().small())
        .child(div().text_size(T::TEXT_13).text_color(T::text_muted()).child(text.into()))
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
/// scrim close it. One dialog layer for the whole window (gpui-component's Root).
pub fn confirm(window: &mut Window, cx: &mut App, c: Confirm, on_ok: impl Fn(&mut Window, &mut App) + 'static) {
    let on_ok = std::rc::Rc::new(on_ok);
    window.open_alert_dialog(cx, move |d, _, _| {
        let on_ok = on_ok.clone();
        d.confirm()
            .title(c.title.clone())
            .description(c.message.clone())
            .button_props(DialogButtonProps::default().ok_text(c.ok.clone()).ok_variant(variant(c.danger)))
            .on_ok(move |_, w, cx| { on_ok(w, cx); true })
    });
}

/// Close the dialog on top (the test driver's Cancel, Esc).
pub fn close_dialog(window: &mut Window, cx: &mut App) { window.close_dialog(cx); }

/// A dialog is open.
pub fn dialog_open(window: &mut Window, cx: &mut App) -> bool { window.has_active_dialog(cx) }

/// The toast, top right: info, error or success.
pub fn toast(window: &mut Window, cx: &mut App, text: impl Into<SharedString>, kind: Toast) {
    let n = match kind {
        Toast::Info => gpui_component::notification::Notification::info(text),
        Toast::Error => gpui_component::notification::Notification::error(text),
        Toast::Success => gpui_component::notification::Notification::success(text),
    };
    window.push_notification(n, cx);
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Toast { Info, Error, Success }

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
