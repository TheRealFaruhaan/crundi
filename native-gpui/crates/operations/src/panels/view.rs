//! Panels, drawn (ui/ops/panels.slint's parts): the tab (list, bar, stage,
//! Checks, History), a copy of a panel (its banners over its page), the
//! chooser and the panel in a workbench pane, the inline card, the docks,
//! the top bar's chips and a chip's popover.

use super::*;
use crundi_theme::{IconName, T};
use gpui::prelude::FluentBuilder as _;

fn msg(title: &'static str, strong: bool, hint: &'static str) -> Div {
    div().flex().flex_col().flex_1().min_w_0().size_full().items_center().justify_center().gap(T::SP_6).p(T::SP_24)
        .child(div().text_size(T::TEXT_26).text_color(T::text_secondary()).opacity(T::PANELS_O_SOFT).child(w::icon(IconName::PanelsTopLeft, T::TEXT_26)))
        .child(div().text_size(T::TEXT_13).font_weight(if strong { T::W_BOLD } else { T::W_REGULAR }).text_color(if strong { T::text() } else { T::text_secondary() }).child(title))
        .when(!hint.is_empty(), |d| d.child(div().max_w(T::DIALOG_W).text_center().text_size(T::TEXT_12).text_color(T::text_secondary()).child(hint)))
}

fn tone(t: &str) -> &'static str { match t { "warn" => "warn", "error" => "error", _ => "" } }

/// The tab's state: "loading", "error", "empty", "empty-global", "list".
fn tab_state() -> &'static str {
    with(|st| {
        if !st.error.is_empty() && (!st.loaded || st.widgets.is_empty()) { "error" }
        else if !st.loaded { "loading" }
        else if st.widgets.is_empty() { if st.project.is_empty() { "empty-global" } else { "empty" } }
        else { "list" }
    })
}

/// The Panels tab (.wg-tab): the list on the left, the selected panel's bar
/// and stage on the right; this screen's overlays (a chip's popover, the
/// driver's inline card) over it.
pub(crate) fn tab(window: &mut Window, cx: &mut App) -> AnyElement {
    let body: AnyElement = match tab_state() {
        "loading" => w::loading("Loading panels…").size_full().into_any_element(),
        "error" => w::error_state("wg-retry", with(|st| st.error.clone()), |_, _, cx| retry(cx)).size_full().into_any_element(),
        "empty-global" => msg("No global panels yet.", false, "Select a project to see its panels. Any of them can be made global from here, to show in every project.").into_any_element(),
        "empty" => msg("No panels yet", true, "Claude builds these when something is better seen than read: progress on a long task, a table from a database, a build board. Ask for one, or let it decide.").into_any_element(),
        _ => list(window, cx),
    };
    div().size_full().flex().flex_col().min_h_0().bg(T::bg()).child(body).children(overlay(window, cx)).into_any_element()
}

fn list(window: &mut Window, cx: &mut App) -> AnyElement {
    let (list, selected, pane, project) = with(|st| (tab_order(&st.widgets), st.selected.clone(), st.pane.clone(), st.project.clone()));
    let mut col = div().id("wg-list").flex().flex_col().gap(T::SP_4).p(T::SP_8).size_full().overflow_y_scroll();
    for (i, wv) in list.iter().enumerate() {
        let id = s(wv, "id");
        let (badge, tn) = item_badge(wv);
        let closed = s(wv, "state") != "open";
        let row = w::list_row(("wg-item", i), IconName::PanelsTopLeft, s(wv, "title"), "", if badge.is_empty() { item_sub(wv) } else { String::new() }, id == selected)
            .min_h(T::PANELS_ITEM_H)
            .when(closed, |d| d.opacity(T::PANELS_O_SOFT))
            .when(flag(wv, "global"), |d| d.child(w::tag("GLOBAL", "")))
            .when(!badge.is_empty(), |d| d.child(w::tag(badge.to_uppercase(), tone(tn))))
            .on_click(move |_, _, cx| select(&id, cx));
        col = col.child(row);
    }
    let sel = list.iter().find(|w| s(w, "id") == selected).cloned();
    let mut main = div().flex().flex_col().flex_1().min_w_0().min_h_0();
    if let Some(sv) = sel {
        main = main.child(bar(&sv, &pane, &project));
        let id = s(&sv, "id");
        let stage: AnyElement = if pane == "checks" { checks().into_any_element() }
            else if pane == "history" { history(&id).into_any_element() }
            else if s(&sv, "state") != "open" { msg("This panel is closed.", false, "Reopen it to see it here and wherever it was placed.").into_any_element() }
            else { frame(&frame_key("tab", &project, &id), false, T::PANELS_INLINE_MAX, true, window, cx) };
        main = main.child(div().flex().flex_col().flex_1().min_w_0().min_h_0().child(stage));
    }
    div().flex().flex_1().min_w_0().size_full().min_h_0()
        .child(div().flex_none().w(T::PANELS_LIST_W).h_full().bg(T::bg_secondary()).border_r_1().border_color(T::border()).child(col))
        .child(main)
        .into_any_element()
}

/// The slot <select> (.wg-select): "Where this panel sits".
fn slot_select(eid: impl Into<SharedString>, wv: &Value) -> impl IntoElement {
    let (labels, values, cur) = slot_options(wv);
    let id = s(wv, "id");
    let eid: SharedString = eid.into();
    div().flex_none().w(T::PANELS_SELECT_W).child(w::dropdown(ElementId::Name(eid), &labels, cur, false, move |i, _, cx| {
        if i != cur { if let Some(v) = values.get(i) { slot(&id, v, cx); } }
    }))
}

/// The tab's bar (.wg-tab-bar) for the selected panel.
fn bar(sv: &Value, pane: &str, project: &str) -> impl IntoElement {
    let id = s(sv, "id");
    let open = s(sv, "state") == "open";
    let pinned = s(sv, "lifecycle") == "pinned";
    let scope = scope_of(sv, project);
    let (i1, i2, i3, i4, i5, i6, i7) = (id.clone(), id.clone(), id.clone(), id.clone(), id.clone(), id.clone(), id);
    let (pc, ph) = (pane == "checks", pane == "history");
    w::toolbar().px(T::SP_10).bg(T::bg_secondary())
        .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(s(sv, "title")))
        .when(open, |d| d.child(slot_select("wg-slot", sv)))
        .child(w::button("wg-pin", if pinned { "Pinned" } else { "Pin" }).icon(IconName::Pin).on_click(move |_, _, cx| pin(&i1, cx)))
        .when(scope == "local", |d| d.child(w::button("wg-global", "Make global").icon(IconName::Globe).on_click(move |_, _, cx| set_scope(&i2, true, cx))))
        .when(scope == "global", |d| d.child(w::primary("wg-global", "Global").icon(IconName::Globe).on_click(move |_, _, cx| set_scope(&i3, false, cx))))
        .when(scope == "other", |d| d.child(w::tag(format!("GLOBAL · FROM {}", s(sv, "home").to_uppercase()), "")))
        .child(w::icon_button("wg-reload", IconName::RefreshCw, "Reload").on_click(move |_, _, cx| { show_pane("", cx); reload_id(&i4, cx); }))
        .child(w::toggle("wg-checks", "Checks", pc).icon(IconName::Eye).on_click(move |_, _, cx| show_pane(if pc { "" } else { "checks" }, cx)))
        .child(w::toggle("wg-history", "History", ph).icon(IconName::RotateCcw).on_click(move |_, _, cx| show_pane(if ph { "" } else { "history" }, cx)))
        .when(open, |d| d.child(w::button("wg-close", "Close").on_click(move |_, _, cx| close(&i5, cx))))
        .when(!open, |d| d.child(w::primary("wg-reopen", "Reopen").on_click(move |_, _, cx| reopen(&i6, cx))))
        .child(w::danger("wg-delete", "").icon(IconName::Trash).tooltip("Delete this panel and its files").on_click(move |_, window, cx| delete(&i7, window, cx)))
}

/// "Checks": the screenshots from Claude's last check of the panel.
fn checks() -> impl IntoElement {
    let (state, shots) = with(|st| (st.checks_state.clone(), st.shots.clone()));
    let mut row = div().id("wg-shots").flex().gap(T::SP_8).pt(T::SP_8).pb(T::SP_4).overflow_x_scroll();
    for (cap, img) in shots {
        let pic: AnyElement = match img {
            Some((im, iw, ih)) => div().h(T::PANELS_SHOT_H).w(T::PANELS_SHOT_H * (iw / ih)).rounded(T::R_CONTROL).border_1().border_color(T::border()).overflow_hidden()
                .child(img_el(im)).into_any_element(),
            None => div().size(T::PANELS_SHOT_H).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg()).flex().items_center().justify_center().child(w::spinner(T::ICON_LG)).into_any_element(),
        };
        row = row.child(div().flex().flex_col().flex_none().gap(T::SP_4).child(pic).child(div().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(cap)));
    }
    div().id("wg-checks-pane").flex().flex_col().gap(T::SP_10).p(T::SP_12).size_full().overflow_y_scroll()
        .child(div().text_size(T::TEXT_13).font_weight(T::W_BOLD).text_color(T::text()).child("What Claude saw when it last checked this panel."))
        .when(state == "loading", |d| d.child(w::loading("Loading…")))
        .when(state == "list", |d| d.child(row))
        .when(state == "none", |d| d.child(div().text_size(T::TEXT_13).text_color(T::text_muted()).child("No check has been run yet. Claude renders a panel with widget_render before calling it done.")))
}

fn img_el(im: Arc<Image>) -> Img { img(im).size_full().object_fit(ObjectFit::Contain) }

/// "History": earlier versions, the newest first; Restore on all but it.
fn history(id: &str) -> impl IntoElement {
    let (state, versions) = with(|st| (st.versions_state.clone(), st.versions.clone()));
    let mut col = div().id("wg-history-pane").flex().flex_col().gap(T::SP_10).p(T::SP_12).size_full().overflow_y_scroll()
        .when(state == "loading", |d| d.child(w::loading("Loading versions…")))
        .when(state == "empty", |d| d.child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child("No versions yet.")));
    for (i, v) in versions.iter().enumerate() {
        let ver = s(v, "version");
        let (id2, ver2) = (id.to_string(), ver.clone());
        col = col.child(div().flex().flex_none().items_center().gap(T::SP_10).px(T::SP_10).py(T::SP_8).rounded(T::R_CONTROL).border_1().border_color(T::border())
            .child(div().text_size(T::TEXT_12).font_weight(T::W_BOLD).text_color(T::text()).child(format!("v{ver}")))
            .child(div().flex_1().min_w_0().text_size(T::TEXT_12).text_color(T::text_secondary()).child(when(n(v, "at"))))
            .when(i == 0, |d| d.child(w::tag("CURRENT", "")))
            .when(i > 0, |d| d.child(w::button(("wg-restore", i), "Restore").on_click(move |_, _, cx| restore(&id2, &ver2, cx)))));
    }
    col
}

/// The approval banner (.wg-banner.grant): what the panel asks for, then Approve and Not now.
fn grant_banner(key: &str, items: Vec<(String, String)>, approving: bool) -> impl IntoElement {
    let (k1, k2) = (key.to_string(), key.to_string());
    let mut d = div().flex().flex_col().flex_none().gap(T::SP_6).px(T::SP_10).py(T::SP_8).bg(T::yellow_dim()).border_b_1().border_color(T::border())
        .child(div().text_size(T::TEXT_12).text_color(T::text()).child("This panel is asking for more than it gets by default. It keeps doing these after the chat that built it is gone."));
    for (what, detail) in items {
        d = d.child(div().flex().flex_col().px(T::SP_8).py(T::SP_4).rounded(T::R_CONTROL).bg(T::backdrop())
            .child(div().text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(T::yellow()).child(what.to_uppercase()))
            .when(!detail.is_empty(), |d| d.child(div().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text()).child(detail))));
    }
    d.child(div().flex().gap(T::SP_6)
        .child(w::disabled(w::primary("wg-approve", "Approve").icon(IconName::Shield), approving).on_click(move |_, _, cx| approve(&k1, cx)))
        .child(w::button("wg-not-now", "Not now").on_click(move |_, _, cx| not_now(&k2, cx))))
}

/// One copy of a panel (the web's createView): its banners over its page.
/// `auto`: as tall as its content (an inline card, a dock), between
/// panels-frame-min and `max_h`; else it fills what it is given.
pub(crate) fn frame(key: &str, auto: bool, max_h: Pixels, wheel: bool, _window: &mut Window, cx: &mut App) -> AnyElement {
    let _ = wheel;
    let page = ensure_view(key, cx);
    let f = frame_info(key);
    let painted = painted(key);
    let err = f.state == "error";
    let body_h = f.height.map(px).unwrap_or(T::PANELS_FRAME_START).min(max_h).max(T::PANELS_FRAME_MIN);
    let k = key.to_string();
    let body = div().relative().w_full().overflow_hidden().bg(T::bg())
        .map(|d| if auto { d.flex_none().h(body_h) } else { d.flex_1().min_w_0().min_h_0() })
        .children(page.map(|p| div().absolute().inset_0().size_full().child(p)))
        .when(!painted, |d| d.child(div().absolute().inset_0().flex().items_center().justify_center().bg(T::bg()).child(w::spinner(T::ICON_LG))))
        .child(canvas(move |b, _, cx| seen_box(&k, b, cx), |_, _, _, _| {}).absolute().inset_0().size_full());
    div().flex().flex_col().w_full().bg(T::bg())
        .map(|d| if auto { d.flex_none() } else { d.flex_1().min_w_0().size_full().min_h_0() })
        .when(err, |d| d.child(div().flex_none().px(T::SP_10).py(T::SP_8).bg(T::red_dim()).text_size(T::TEXT_12).text_color(T::red())
            .child(if f.error.is_empty() { "This panel could not be loaded.".to_string() } else { f.error.clone() })))
        .when_some(f.grant, |d, items| d.child(grant_banner(key, items, f.approving)))
        .when(!err, |d| d.child(body))
        .into_any_element()
}

/// A workbench pane (kind "panel:widget"): "panel:widget:@<id>" that
/// panel; a chooser (the "+" menu's Panel) until a panel is picked in it.
pub(crate) fn pane(key: &str, window: &mut Window, cx: &mut App) -> AnyElement {
    let id = picked_of(key);
    let project = with(|st| st.project.clone());
    if !id.is_empty() { return div().flex().flex_col().size_full().child(frame(&frame_key("cell", &project, &id), false, T::PANELS_INLINE_MAX, true, window, cx)).into_any_element(); }
    picker(key, cx).into_any_element()
}

/// The chooser in a "Panel" pane (the web's mountPicker): every panel of
/// the project, the newest first; one already in the workbench is greyed.
fn picker(key: &str, cx: &App) -> AnyElement {
    let (p, mut list) = with(|st| (st.project.clone(), st.widgets.clone()));
    if list.is_empty() {
        return msg("No panels in this project yet", true, "Ask Claude for one in a chat: \"make a panel showing…\", or \"show me a live panel of this task\".").into_any_element();
    }
    list.sort_by(|a, b| n(b, "updatedAt").total_cmp(&n(a, "updatedAt")));
    let live = live_chats(&p, cx);
    let mut col = div().id("wg-picker").flex().flex_col().gap(T::SP_4).p(T::SP_10).size_full().overflow_y_scroll()
        .child(div().pb(T::SP_4).text_size(T::TEXT_11).font_weight(T::W_BOLD).text_color(T::text_secondary()).child("CHOOSE A PANEL TO OPEN HERE"));
    for (i, wv) in list.iter().enumerate() {
        let place = place_of(wv, &live);
        let here = place == "cell";
        let (k, id) = (key.to_string(), s(wv, "id"));
        col = col.child(w::list_row(("wg-pick", i), IconName::PanelsTopLeft, s(wv, "title"), "", pick_status(wv, place), false)
            .min_h(T::PANELS_ITEM_H)
            .when(here, |d| d.opacity(T::PANELS_O_SOFT).cursor_default())
            .when(!here, |d| d.on_click(move |_, _, cx| pick(&k, &id, cx))));
    }
    col.into_any_element()
}

/// The inline card under a widget_open call (.wg-inline): a head with the
/// title and "Open in the Panels tab", then the panel at its content's
/// height (at most panels-inline-max). For the chat log: the chat's
/// project, the call's input id, the chat's id.
pub fn inline_card(project: &str, id: &str, chat: &str, window: &mut Window, cx: &mut App) -> AnyElement {
    let key = frame_key(&format!("inline:{chat}"), project, id);
    let f = frame_info(&key);
    let name = if !f.title.is_empty() { f.title.clone() } else { Some(title_of(id)).filter(|t| !t.is_empty()).unwrap_or_else(|| id.to_string()) };
    let id2 = id.to_string();
    w::card().overflow_hidden()
        .child(div().flex().flex_none().items_center().gap(T::SP_6).pl(T::SP_10).pr(T::SP_6).py(T::SP_4).border_b_1().border_color(T::border_subtle())
            .child(div().text_color(T::text_secondary()).child(w::icon(IconName::PanelsTopLeft, T::ICON_MD)))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(name))
            .child(w::icon_button(SharedString::from(format!("wg-open-{chat}-{id}")), IconName::Maximize2, "Open in the Panels tab").on_click(move |_, _, cx| open_tab(&id2, cx))))
        .child(frame(&key, true, T::PANELS_INLINE_MAX, false, window, cx))
        .into_any_element()
}

/// The panels docked to a chat (.wg-dock): each a head (its title, which
/// folds it too; "approve"; the slot select; collapse; ×) over the panel at
/// its content's height (at most `max_h`). None when none is docked. For
/// the chat pane: at the top of its body.
pub fn dock(chat: &str, max_h: Pixels, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let list = docks(chat, cx);
    if list.is_empty() { return None; }
    let project = with(|st| st.project.clone());
    let mut col = div().flex().flex_col().flex_none().w_full().bg(T::bg()).border_b_1().border_color(T::border());
    for (i, wv) in list.iter().enumerate() {
        let id = s(wv, "id");
        let folded = flag(wv, "collapsed");
        let (i1, i2, i3) = (id.clone(), id.clone(), id.clone());
        col = col.child(div().id(("wg-dock-head", i)).flex().flex_none().items_center().gap(T::SP_6).pl(T::SP_8).pr(T::SP_6).py(T::SP_2).bg(T::bg_secondary())
            .when(!folded, |d| d.border_b_1().border_color(T::border_subtle()))
            .child(div().id(("wg-dock-fold", i)).flex().flex_1().min_w_0().items_center().gap(T::SP_6).cursor_pointer().on_click(move |_, _, cx| collapse(&i1, !folded, cx))
                .child(div().text_color(T::accent_hover()).child(w::icon(IconName::PanelsTopLeft, T::ICON_MD)))
                .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_11).font_weight(T::W_BOLD).text_color(T::text_secondary()).child(s(wv, "title").to_uppercase())))
            .when(flag(wv, "needsGrant"), |d| d.child(w::tag("APPROVE", "warn")))
            .child(slot_select(format!("wg-dock-slot-{chat}-{i}"), wv))
            .child(w::icon_button(("wg-dock-collapse", i), if folded { IconName::ChevronDown } else { IconName::ChevronUp }, if folded { "Expand" } else { "Collapse" }).on_click(move |_, _, cx| collapse(&i2, !folded, cx)))
            .child(w::icon_button(("wg-dock-x", i), IconName::X, "Close panel").on_click(move |_, _, cx| close(&i3, cx))));
        if !folded { col = col.child(frame(&frame_key(&format!("dock:{chat}"), &project, &id), true, max_h, true, window, cx)); }
    }
    Some(col.into_any_element())
}

/// The top bar's chips (.wg-chips): one per panel placed there, a pill
/// (its icon, label and value, "approve" while waiting); a click opens its
/// popover. None when no panel is a chip. For the top bar, after the
/// project's name.
pub fn chips(_window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let (p, list, values) = with(|st| (st.project.clone(), st.widgets.clone(), st.chip_values.clone()));
    let live = live_chats(&p, cx);
    let chips: Vec<&Value> = list.iter().filter(|w| place_of(w, &live) == "chip").collect();
    if chips.is_empty() { return None; }
    let mut row = div().flex().items_center().gap(T::SP_6);
    for (i, wv) in chips.into_iter().enumerate() {
        let id = s(wv, "id");
        let grant = flag(wv, "needsGrant");
        let label = wv.get("chip").map(|c| s(c, "label")).filter(|l| !l.is_empty()).unwrap_or_else(|| s(wv, "title"));
        let value = values.get(&id).cloned().unwrap_or_default();
        let tip = format!("{}{}", s(wv, "title"), if grant { " (waiting for your approval)" } else { "" });
        let el = div().id(("wg-chip", i)).flex().items_center().gap(T::SP_6).h(T::CONTROL_SM - T::SP_2).px(T::SP_8).rounded(T::R_PILL)
            .border_1().border_color(T::border()).bg(T::bg_tertiary()).cursor_pointer().hover(|d| d.border_color(T::accent()))
            .child(div().text_color(T::accent_hover()).child(w::icon(IconName::PanelsTopLeft, T::ICON_MD)))
            .child(div().max_w(T::PANELS_CHIP_MAX).truncate().text_size(T::TEXT_12).text_color(T::text_secondary()).child(label))
            .when(!value.is_empty(), |d| d.child(div().max_w(T::PANELS_CHIP_MAX).truncate().text_size(T::TEXT_12).text_color(T::text()).child(value)))
            .when(grant, |d| d.child(w::tag("APPROVE", "warn")))
            .on_click(move |_, _, cx| open_pop(&id, cx));
        row = row.child(w::tip(el, tip));
    }
    Some(row.into_any_element())
}

/// This screen's overlays over the window: a chip's popover (.wg-pop: its
/// head, title, slot select, ×, over the panel; the backdrop closes it),
/// and the driver's look at an inline card. None when neither is open.
pub fn overlay(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let (pop, preview, project) = with(|st| (by_id(st, &st.pop), st.preview.clone(), st.project.clone()));
    if pop.is_none() && preview.1.is_empty() { return None; }
    let vp = window.viewport_size();
    let mut root = div().absolute().top_0().left_0().w(vp.width).h(vp.height);
    if let Some(pw) = pop {
        let id = s(&pw, "id");
        let pop_w = T::PANELS_POP_W.min(vp.width - T::SP_24);
        let pop_h = T::PANELS_POP_H.min(vp.height - T::TOPBAR_H - T::SP_24);
        root = root
            .child(div().id("wg-pop-back").occlude().absolute().inset_0().bg(T::scrim()).on_mouse_down(MouseButton::Left, |_, _, cx| close_pop(cx)))
            .child(div().id("wg-pop").occlude().absolute().top(T::TOPBAR_H + T::SP_8).right(T::SP_12).w(pop_w).h(pop_h)
                .flex().flex_col().rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg()).overflow_hidden()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(div().flex().flex_none().items_center().gap(T::SP_6).pl(T::SP_8).pr(T::SP_6).py(T::SP_2).bg(T::bg_secondary()).border_b_1().border_color(T::border_subtle())
                    .child(div().text_color(T::accent_hover()).child(w::icon(IconName::PanelsTopLeft, T::ICON_MD)))
                    .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_11).font_weight(T::W_BOLD).text_color(T::text_secondary()).child(s(&pw, "title").to_uppercase()))
                    .child(slot_select("wg-pop-slot", &pw))
                    .child(w::icon_button("wg-pop-x", IconName::X, "Close").on_click(|_, _, cx| close_pop(cx))))
                .child(frame(&frame_key("pop", &project, &id), false, T::PANELS_INLINE_MAX, true, window, cx)));
    }
    if !preview.1.is_empty() {
        let card = inline_card(&preview.0, &preview.1, &preview.2, window, cx);
        root = root.child(div().absolute().top(T::TOPBAR_H + T::SP_48).left((vp.width - T::PANELS_POP_W - T::SP_48) / 2.).w(T::PANELS_POP_W + T::SP_48)
            .p(T::SP_16).rounded(T::R_CARD).bg(T::bg()).child(card));
    }
    Some(deferred(anchored().position(point(px(0.), px(0.))).child(root)).with_priority(1).into_any_element())
}
