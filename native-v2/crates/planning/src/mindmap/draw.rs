//! The Mind map's views (the web's #mindmap-panel; native-gpui's
//! mindmap_draw.rs): the toolbar (+ Add idea, Media, Filter ideas…,
//! Shortcuts) and the project bar, the canvas (the ideas laid out as a tree,
//! the links, the "add idea" boxes, the box an idea is typed in, the ring
//! the keyboard moves, a drag's clone) and the dialogs (an idea's details,
//! the link picker, the shortcut sheet) in the one modal layer. Zoom scales
//! every size on the canvas by M.zoom (GPUI has no transforms).
//!
//! How it is built (ARCHITECTURE.md rules 3, 4 and 6): a host (the tab, or a
//! workbench pane) places cached leaves side by side: `Msg` (loading, an
//! error, no ideas; its notify re-lays the host out), `Toolbar`, `Dots` (the
//! canvas's dotted ground: it does not move, so it is drawn once) and
//! `MapView`, which pans itself (its own `Pan`) and draws only the ideas and
//! links that cross the view: a pan or a wheel redraws the map alone.

use super::{help, tree, Key, Pan, Row, M};
use crundi_core::driver::rendered;
use std::{collections::HashMap, rc::Rc};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::input::{Enter, Escape, IndentInline, OutdentInline, SelectToNextWordEnd, SelectToPreviousWordStart};

#[derive(Clone, Copy, PartialEq)]
enum Mode { Normal, Open, Editing }

fn key_of(k: &Keystroke) -> Key<'_> {
    Key { key: k.key.as_str(), ctrl: k.modifiers.control, shift: k.modifiers.shift, alt: k.modifiers.alt || k.modifiers.platform }
}

/// The keys a text box would take as its own actions (Enter, Tab, Escape,
/// Ctrl+Shift+Left/Right): the map reads them first, as the Slint client's
/// draft-key / edit-key did.
fn keys_first(d: Div) -> Div {
    let go = |k: Key, window: &mut Window, cx: &mut App| if super::key(&k, window, cx) { cx.stop_propagation(); };
    d.capture_action(move |a: &Enter, window, cx| go(Key { key: "enter", ctrl: a.secondary, shift: a.shift, alt: false }, window, cx))
        .capture_action(move |_: &IndentInline, window, cx| go(Key { key: "tab", ctrl: false, shift: false, alt: false }, window, cx))
        .capture_action(move |_: &OutdentInline, window, cx| go(Key { key: "tab", ctrl: false, shift: true, alt: false }, window, cx))
        .capture_action(move |_: &Escape, window, cx| go(Key { key: "escape", ctrl: false, shift: false, alt: false }, window, cx))
        .capture_action(move |_: &SelectToPreviousWordStart, window, cx| go(Key { key: "left", ctrl: true, shift: true, alt: false }, window, cx))
        .capture_action(move |_: &SelectToNextWordEnd, window, cx| go(Key { key: "right", ctrl: true, shift: true, alt: false }, window, cx))
}

/// An idea's words wrap past two lines (it opens up under the pointer).
fn clamped(text: &str, z: f32, window: &Window) -> bool {
    if text.is_empty() { return false; }
    let font = Font { family: T::UI_FONT.into(), weight: T::W_SEMIBOLD, ..crundi_theme::ui_font() };
    let run = TextRun { len: text.len(), font, color: T::text(), background_color: None, underline: None, strikethrough: None };
    let wrap = T::MINDMAP_NODE * z - T::SP_10 * z * 2. - T::SP_2;
    window.text_system().shape_text(SharedString::from(text.to_string()), T::TEXT_13 * z, &[run], Some(wrap), None)
        .map(|ls| ls.iter().map(|l| l.wrap_boundaries.len() + 1).sum::<usize>() > 2).unwrap_or(false)
}

fn glow() -> Vec<BoxShadow> { vec![BoxShadow::new(px(0.), px(0.), T::accent_edge()).blur_radius(T::SP_10)] }
fn lift() -> Vec<BoxShadow> { vec![BoxShadow::new(px(0.), T::SP_12, T::scrim()).blur_radius(T::SP_32)] }

/// The height of a box as drawn, reported for the layout (unscaled).
fn measure(id: String, z: f32) -> impl IntoElement {
    canvas(move |b, _, cx| super::measured(&id, f32::from(b.size.height) / z, cx), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
}

/// A meta chip (.mm-chip): link (green), missing (muted), scope (accent),
/// inherited (muted, dashed), note (secondary). The web's emoji are flat icons.
fn meta_chip(id: String, kind: &str, text: &str, todo: i32, tip: &str, z: f32, live: bool) -> AnyElement {
    let fg = match kind { "link" => T::green(), "scope" => T::accent_hover(), "note" => T::text_secondary(), _ => T::text_muted() };
    let bg = match kind { "link" => T::green_dim(), "scope" => T::accent_dim(), _ => T::bg_tertiary() };
    let icon = match kind { "link" | "missing" => Some(IconName::Link), "scope" => Some(IconName::Folder), "note" => Some(IconName::StickyNote), _ => None };
    let d = div().flex().flex_none().items_center().gap(T::SP_2 * z).px(T::SP_6 * z).py(px(1.) * z).rounded(T::R_FIELD * z)
        .bg(bg).text_color(fg).text_size(T::TEXT_11 * z).max_w(T::MINDMAP_NODE * z - T::SP_24 * z)
        .when(kind == "inherited", |d| d.border_1().border_dashed().border_color(T::text_muted()))
        .when_some(icon, |d, i| d.child(w::icon(i, T::ICON_SM * z)))
        .when(todo > 0, |d| d.child(w::icon(if todo == 2 { IconName::SquareCheck } else { IconName::Square }, T::ICON_SM * z)))
        .child(div().min_w_0().truncate().child(text.to_string()));
    if !live { return d.into_any_element(); }
    w::tip(d.id(SharedString::from(id)), tip.to_string()).into_any_element()
}

/// An idea (.mm-node): its words (two lines; Open: all of it, about seven
/// lines, then it scrolls; Editing: a text box in place), its chips, the − / +
/// that folds what is under it.
/// `hot`: the pointer is on this idea (Normal: the map's own hit test, the
/// idea drawn hovered) and on its fold button.
fn card(r: &Row, mode: Mode, z: f32, edit: Option<&Entity<w::TextareaState>>, hot: bool, fold_hot: bool) -> AnyElement {
    let on = mode != Mode::Normal;
    let strong = r.hit || r.target || r.flash || on;
    let tag = match mode { Mode::Normal => "n", Mode::Open => "o", Mode::Editing => "e" };
    let top = (r.y * z - r.h * z / 2.).max(f32::from(T::SP_4) * z);
    let border = if strong { T::accent() } else if r.linked { T::green() } else if r.root || r.scoped { T::accent() } else { T::border() };
    let words = div().min_w_0().text_size(T::TEXT_13 * z).font_weight(T::W_SEMIBOLD).text_color(T::text()).line_height(relative(T::LH_UI));
    let body: AnyElement = match mode {
        Mode::Normal => words.line_clamp(2).text_ellipsis().child(r.text.clone()).into_any_element(),
        Mode::Open => div().id(SharedString::from(format!("mm-open-{}", r.id))).max_h(T::TEXT_13 * z * T::LH_UI * 7.).overflow_y_scroll()
            .child(words.child(r.text.clone())).into_any_element(),
        Mode::Editing => keys_first(div()).children(edit.map(|e| w::textarea(e).appearance(false).text_size(T::TEXT_13 * z).font_weight(T::W_SEMIBOLD))).into_any_element(),
    };
    let chips = (!r.chip.is_empty() || r.notes > 0).then(|| div().flex().gap(T::SP_4 * z)
        .when(!r.chip.is_empty(), |d| d.child(meta_chip(format!("mm-chip-{tag}-{}", r.id), &r.chip, &r.chip_text, r.todo, &r.chip_tip, z, hot || on)))
        .when(r.notes > 0, |d| d.child(meta_chip(format!("mm-note-{tag}-{}", r.id), "note", &format!("{} {}", r.notes, if r.notes == 1 { "note" } else { "notes" }), 0, "", z, hot || on))));
    let id = r.id.clone();
    let mut d = div().id(SharedString::from(format!("mm-{tag}-{}", r.id))).absolute().left(px(r.x * z)).top(px(top)).w(T::MINDMAP_NODE * z)
        .flex().flex_col().gap(T::SP_6 * z).px(T::SP_10 * z + px(1.)).py(T::SP_8 * z).rounded(T::R_BUBBLE * z)
        .bg(if r.root && !on { T::accent_dim() } else { T::bg_card() })
        .when(strong, |d| d.border_2()).when(!strong, |d| d.border_1()).border_color(border)
        // Hovered: the map knows (an idea at rest has no handlers of its own).
        .when(!strong && !r.linked && !r.root && !r.scoped && hot, |d| d.border_color(T::accent()))
        .when(r.pending, |d| d.border_dashed().border_color(T::text_muted()))
        .when(r.lifted || r.dim, |d| d.opacity(T::MINDMAP_O_FAINT))
        .when(r.flash || r.target, |d| d.shadow(glow())).when(on && !(r.flash || r.target), |d| d.shadow(lift()))
        // A root opened up keeps its tint over the card it covers.
        .when(r.root && on, |d| d.child(div().absolute().top_0().left_0().size_full().rounded(T::R_BUBBLE * z).bg(T::accent_dim())))
        .child(body)
        .children(chips);
    // Dropped on a chat's box or a terminal: its id goes in (the shared drag
    // layer; on the map the drag below rearranges it).
    if mode != Mode::Editing && (hot || mode == Mode::Open) && !r.pending {
        d = d.on_drag(w::RefDrag::new("mindmap", &r.id, r.text.clone()), |_, _, _, cx| cx.new(|_| crate::util::NoGhost));
    }
    if mode == Mode::Editing {
        d = d.cursor_text().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    } else if mode == Mode::Normal {
        // Pressed, released and hovered through the map (MapView::hit).
        d = d.child(measure(id.clone(), z));
    } else {
        let (i1, i2) = (id.clone(), id.clone());
        let row = r.clone();
        d = d.cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |e, _, cx| { cx.stop_propagation(); super::node_down(&i1, f32::from(e.position.x), f32::from(e.position.y), cx); })
            .on_mouse_up(MouseButton::Left, move |e, window, cx| { cx.stop_propagation(); super::node_up(&i2, e.click_count, window, cx); })
            .on_hover(move |hot, window, cx| {
                if *hot { if mode == Mode::Normal { let c = clamped(&row.text, z, window); super::hover(&row, true, c, cx); } }
                // The idea opened up decides when it closes (it lies over this one).
                else if mode == Mode::Open || super::store(cx).read(cx).hover.as_ref().is_none_or(|h| h.id != row.id) { super::hover(&row, false, false, cx); }
            });
    }
    // Folds or unfolds what is under it (.mm-collapse), on its right edge.
    if r.kids && mode != Mode::Editing {
        let fid = id.clone();
        let fold = div().size(T::SP_16 * z + T::SP_2 * z).rounded_full().border_1().border_color(if fold_hot { T::accent() } else { T::border() })
            .bg(T::bg_tertiary()).flex().items_center().justify_center()
            .text_color(T::text_secondary()).child(w::icon(if r.collapsed { IconName::Plus } else { IconName::Minus }, T::ICON_SM * z));
        let fold = if mode == Mode::Normal { fold.into_any_element() } else {
            fold.id(SharedString::from(format!("mm-fold-{tag}-{id}"))).hover(|d| d.border_color(T::accent())).cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, window, cx| super::act("toggle", &fid, window, cx)).into_any_element()
        };
        d = d.child(div().absolute().top_0().bottom_0().left(T::MINDMAP_NODE * z - T::SP_10 * z).flex().items_center().child(fold));
    }
    d.into_any_element()
}

/// The dashed "add idea" box under an idea (.mm-ghost): half seen until hovered.
fn ghost(r: &Row, z: f32, hot: bool) -> AnyElement {
    div().absolute().left(px(r.x * z)).top(px(r.y * z - r.h * z / 2.)).w(T::MINDMAP_NODE * z).min_h(T::SP_40 * z)
        .flex().items_center().justify_center().gap(T::SP_6 * z).py(T::SP_8 * z).rounded(T::R_BUBBLE * z)
        .border_1().border_dashed().border_color(T::border()).opacity(T::MINDMAP_O_DIM).text_color(T::text_muted())
        // Hovered (the map's hit test; pressed through the map too).
        .when(hot, |d| d.opacity(1.).border_color(T::accent()).text_color(T::accent_hover()))
        .child(w::icon(IconName::Plus, T::ICON_MD * z))
        .child(div().text_size(T::TEXT_12 * z).font_weight(T::W_MEDIUM).child("add idea"))
        .child(measure(r.id.clone(), z))
        .into_any_element()
}

/// Where a dragged idea would land (.mm-placeholder).
fn drop_box(r: &Row, z: f32) -> AnyElement {
    div().absolute().left(px(r.x * z)).top(px(r.y * z - r.h * z / 2.)).w(T::MINDMAP_NODE * z).h(px(r.h * z)).rounded(T::R_BUBBLE * z)
        .bg(T::accent_dim()).border_2().border_dashed().border_color(T::accent()).into_any_element()
}

/// The idea being typed (.mm-draft), with its hint.
fn draft(s: &M, z: f32, e: &Entity<w::TextareaState>) -> AnyElement {
    let h = s.heights.get(tree::DRAFT).copied().unwrap_or(f32::from(T::SP_48)) * z;
    let top = (s.draft_y * z - h / 2.).max(f32::from(T::SP_4) * z);
    let k = |t: &'static str| div().font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(t);
    let m = |t: &'static str| div().text_color(T::text_muted()).child(t);
    div().id("mm-draft").absolute().left(px(s.draft_x * z)).top(px(top)).w(T::MINDMAP_NODE * z)
        .flex().flex_col().gap(T::SP_4 * z).px(T::SP_8 * z + px(1.)).pt(T::SP_6 * z + px(1.)).pb(T::SP_4 * z + px(1.))
        .rounded(T::R_BUBBLE * z).border_2().border_color(T::accent()).bg(T::bg_card()).shadow(glow()).cursor_text()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(keys_first(div()).child(w::textarea(e).appearance(false).text_size(T::TEXT_13 * z).font_weight(T::W_SEMIBOLD)))
        .child(div().flex().gap(T::SP_2 * z).text_size(T::TEXT_10 * z).overflow_hidden().whitespace_nowrap()
            .child(k("Enter")).child(m("save \u{00b7}")).child(k("Ctrl+Enter")).child(m("next \u{00b7}")).child(k("Tab")).child(m("nested")))
        .child(measure(tree::DRAFT.to_string(), z))
        .into_any_element()
}

/// The ring where the keyboard is (.mm-cursor); amber with what a second
/// Delete would remove.
fn ring(s: &M, z: f32) -> AnyElement {
    let rh = (s.ring_h + f32::from(T::SP_8)) * z;
    let top = (s.ring_y * z - rh / 2.).max(0.);
    let (edge, halo) = if s.ring_warn { (T::yellow(), T::yellow_dim()) } else { (T::accent(), T::accent_dim()) };
    div().absolute().left(px((s.ring_x - f32::from(T::SP_4)) * z)).top(px(top)).w((T::MINDMAP_NODE + T::SP_8) * z).h(px(rh))
        .rounded((T::R_BUBBLE + T::SP_4) * z).border_2().border_color(edge)
        .child(div().absolute().top(-T::SP_4).left(-T::SP_4).right(-T::SP_4).bottom(-T::SP_4).rounded((T::R_BUBBLE + T::SP_4) * z + T::SP_4).border_4().border_color(halo))
        .when(s.ring_warn, |d| d.child(div().absolute().left_0().top(px(rh) + T::SP_8).px(T::SP_8).py(T::SP_4).rounded(T::R_CONTROL).bg(T::yellow())
            .whitespace_nowrap().text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::bg()).child(s.ring_msg.clone())))
        .into_any_element()
}

/// The idea in hand while dragging (.drag-clone: position fixed on the
/// body, so it follows the pointer over the whole window, a chat or a
/// terminal too, not clipped by the map).
fn clone_box(s: &M, z: f32) -> AnyElement {
    deferred(anchored().position(point(px(s.drag_wx - s.grab_x * z), px(s.drag_wy - s.grab_y * z))).child(clone_card(s, z))).with_priority(1).into_any_element()
}

fn clone_card(s: &M, z: f32) -> Div {
    div().w(T::MINDMAP_NODE * z)
        .px(T::SP_10 * z).py(T::SP_8 * z).rounded(T::R_BUBBLE * z).border_1().border_color(T::accent()).bg(T::bg_card())
        .opacity(T::MINDMAP_O_CLONE).shadow(lift())
        .child(div().text_size(T::TEXT_13 * z).font_weight(T::W_SEMIBOLD).text_color(T::text()).line_clamp(2).text_ellipsis().child(s.drag_text.clone()))
}

/// A link curve from (x1, y1) to (x2, y2): out of the right edge, into the left.
fn curves(pb: &mut PathBuilder, list: &[[f32; 4]], o: Point<Pixels>, z: f32) {
    let pt = |x: f32, y: f32| point(o.x + px(x * z), o.y + px(y * z));
    for c in list {
        let m = (c[0] + c[2]) / 2.;
        pb.move_to(pt(c[0], c[1]));
        pb.cubic_bezier_to(pt(c[2], c[3]), pt(m, c[1]), pt(m, c[3]));
    }
}

// ─── The dialogs ───

fn label(t: &'static str) -> Div { div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(t) }

/// An idea's details (#mm-detail-modal).
fn detail(s: &M) -> Option<AnyElement> {
    let u = s.ui.as_ref()?;
    let act = |a: &'static str| move |_: &ClickEvent, window: &mut Window, cx: &mut App| super::d_act(a, window, cx);
    let mut col = div().flex().flex_col().gap(T::SP_10).p(T::SP_16)
        .child(div().text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(s.d_title.clone()));
    if !s.d_editing {
        // The link (.mmd-link): a task or subtask, a project scope, or gone.
        if !s.d_link_kind.is_empty() {
            let missing = s.d_link_kind == "missing";
            let fg = if missing { T::text_muted() } else { T::green() };
            col = col.child(div().flex().gap(T::SP_6).px(T::SP_10).py(T::SP_6 + px(1.)).rounded(T::R_FIELD).bg(if missing { T::bg_tertiary() } else { T::green_dim() })
                .text_color(fg).text_size(T::TEXT_13)
                .child(div().pt(T::SP_2).child(w::icon(if s.d_link_kind == "scope" { IconName::Folder } else { IconName::Link }, T::ICON_MD)))
                .child(div().min_w_0().when(s.d_link_todo == 0, |d| d.flex_1()).child(s.d_link_main.clone()))
                .when(s.d_link_todo > 0, |d| d.child(div().pt(T::SP_2).child(w::icon(if s.d_link_todo == 2 { IconName::SquareCheck } else { IconName::Square }, T::ICON_MD)))
                    .child(div().flex_1().min_w_0().child(s.d_link_text.clone())))
                .when(!s.d_link_st.is_empty(), |d| d.child(div().text_color(T::text_muted()).child(s.d_link_st.clone()))));
        }
        // The notes (.mmd-notes), each with its dot.
        for t in &s.d_view_notes {
            col = col.child(div().flex().items_start().gap(T::SP_10).px(T::SP_10).py(T::SP_8).rounded(T::R_CONTROL).border_1().border_color(T::border_subtle()).bg(T::bg_tertiary())
                .child(div().mt(T::SP_8).flex_none().size(T::SP_4 + px(1.)).rounded_full().bg(T::accent()))
                .child(div().flex_1().min_w_0().text_size(T::TEXT_14).text_color(T::text_secondary()).child(t.clone())));
        }
        // The media strip (.card-media): thumbnails, then the paperclip.
        let mut strip = div().flex().flex_wrap().gap(T::SP_4 + px(1.)).pt(T::SP_6);
        for (i, t) in s.d_thumbs.iter().enumerate() {
            let id = t.id.clone();
            let icon = match t.kind.as_str() { "video" => IconName::Film, "audio" => IconName::Music, "pdf" => IconName::FileText, "image" => IconName::Image, _ => IconName::File };
            let cell = div().id(("mmd-thumb", i)).size(T::SP_40).rounded(T::R_FIELD).border_1().border_color(T::border()).hover(|d| d.border_color(T::accent()))
                .bg(T::bg_tertiary()).overflow_hidden().flex().items_center().justify_center().cursor_pointer().text_color(T::text_muted())
                .child(match &t.image { Some(img_) => img(img_.clone()).size_full().object_fit(ObjectFit::Cover).into_any_element(), None => w::icon(icon, T::ICON_XL - T::SP_2).into_any_element() })
                .on_click(move |_, window, cx| super::open_media(&id, window, cx));
            strip = strip.child(w::tip(cell, t.name.clone()));
        }
        strip = strip.child(w::square_button("mmd-attach", IconName::Paperclip, "Media", w::SquareLook::Outline, T::SP_40, T::ICON_MD).on_click(act("media")));
        col = col.child(strip);
    } else {
        let mut ed = div().flex().flex_col().gap(T::SP_4).child(label("Idea")).child(w::textarea(&u.d_text)).child(div().h(T::SP_6)).child(label("Notes"));
        // The notes as rows (.mmd-note-row): a box each, and ✕.
        for (i, e) in u.d_notes.iter().enumerate() {
            ed = ed.child(div().flex().gap(T::SP_6).pb(T::SP_2)
                .child(div().flex_1().min_w_0().child(w::textarea(e)))
                .child(w::square_button(("mmd-note-del", i), IconName::X, "Remove", w::SquareLook::Outline, T::SP_32, T::ICON_MD)
                    .on_click(move |_, window, cx| super::d_note_del(i, window, cx))));
        }
        ed = ed.child(w::dashed_button("mmd-add-note", "+ Add note").on_click(act("add-note")));
        col = col.child(ed);
    }
    let first = if s.d_editing { w::primary("mmd-save", "Save").on_click(act("save")) } else { w::button("mmd-edit", "Edit").on_click(act("edit")) };
    col = col.child(div().flex().gap(T::SP_8).pt(T::SP_4)
        .child(first.flex_1())
        .child(w::button("mmd-child", "+ Child").flex_1().on_click(act("child")))
        .child(w::button("mmd-link", s.d_link_label.clone()).flex_1().on_click(act("link")))
        .child(w::button("mmd-media", "Media").flex_1().on_click(act("media")))
        .child(w::danger("mmd-delete", "Delete").flex_1().on_click(act("delete")))
        .child(w::button("mmd-close", "Close").flex_1().on_click(act("close"))));
    Some(col.track_focus(&u.d_focus).into_any_element())
}

/// The link picker (#link-modal): the project, a search, "Scope to …", and
/// the project's tasks with their subtasks.
fn link(s: &M) -> Option<AnyElement> {
    let u = s.ui.as_ref()?;
    let mut list = div().id("mml-list").flex().flex_col().max_h(T::MINDMAP_LIST_MAX_H).overflow_y_scroll().rounded(T::R_CONTROL).border_1().border_color(T::border()).p(px(1.));
    if s.l_loading || s.l_rows.is_empty() {
        list = list.child(div().flex().justify_center().py(T::SP_16).text_size(T::TEXT_13).text_color(T::text_muted())
            .child(if s.l_loading { "Loading…" } else { "No matching tasks or subtasks." }));
    } else {
        for (i, r) in s.l_rows.iter().enumerate() {
            let (task, todo) = (r.task.clone(), r.todo.clone());
            let sub = !r.todo.is_empty();
            let icon = if !sub { IconName::SquareKanban } else if r.done { IconName::SquareCheck } else { IconName::Square };
            let meta = if r.status.is_empty() { String::new() } else { format!("[{}]", r.status) };
            list = list.child(w::list_row(("mml-row", i), icon, r.text.clone(), "", meta, false).when(sub, |d| d.pl(T::SP_24 + T::SP_4))
                .on_click(move |_, window, cx| super::l_act("pick", &task, &todo, window, cx)));
        }
    }
    let col = div().flex().flex_col().gap(T::SP_10).p(T::SP_16)
        .child(div().text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Scope to a project \u{2014} or link a task"))
        .child(div().flex().items_center().gap(T::SP_8)
            .child(w::dropdown::dropdown_of("mml-project", s.l_names.clone(), s.l_sel, s.l_locked, false, |i, window, cx| super::l_act("project", &i.to_string(), "", window, cx)))
            .child(div().flex_1().child(w::input(&u.l_search))))
        // .lm-locked: a child keeps its parent's project.
        .when(s.l_locked, |d| d.child(div().flex().gap(T::SP_6).text_size(T::TEXT_12).text_color(T::yellow())
            .child(div().pt(px(1.)).child(w::icon(IconName::Lock, T::ICON_SM))).child(div().flex_1().min_w_0().child(s.l_locked_text.clone()))))
        .child(w::button("mml-scope", s.l_scope_label.clone()).icon(IconName::Folder).w_full().on_click(|_, window, cx| super::l_act("scope", "", "", window, cx)))
        .child(div().flex().justify_center().text_size(T::TEXT_11).text_color(T::text_muted()).child("\u{2014} or link a specific task \u{2014}"))
        .child(list)
        .child(w::button("mml-cancel", "Cancel").w_full().on_click(|_, window, cx| super::l_act("cancel", "", "", window, cx)));
    Some(col.track_focus(&u.l_focus).into_any_element())
}

/// A shortcut's little tree (mmDiagram): boxes in columns and rows, the lines
/// between them, and the note under it.
fn diagram(d: &help::Dia) -> AnyElement {
    let (cw, rh, bw, bh, pad) = (f32::from(T::MINDMAP_DIA_COL), f32::from(T::MINDMAP_DIA_ROW), f32::from(T::MINDMAP_DIA_BOX_W), f32::from(T::MINDMAP_DIA_BOX_H), f32::from(T::SP_6));
    let (dw, dh, _, _) = help::dia_paths(d, cw, rh, bw, bh, pad);
    let links = help::dia_links(d, cw, rh, bw, bh, pad);
    let lines = canvas(|_, _, _| {}, move |b, _, window, _| {
        for dashed in [false, true] {
            let these: Vec<[f32; 4]> = links.iter().filter(|l| l.1 == dashed).map(|l| l.0).collect();
            if these.is_empty() { continue; }
            let mut pb = PathBuilder::stroke(px(1.));
            if dashed { pb = pb.dash_array(&[T::SP_2 + px(1.), T::SP_2 + px(1.)]); }
            curves(&mut pb, &these, b.origin, 1.);
            if let Ok(p) = pb.build() { window.paint_path(p, T::border()); }
        }
    }).absolute().top_0().left_0().size_full();
    let mut pic = div().relative().w(px(dw)).h(px(dh)).child(lines);
    for b in &d.boxes {
        let lit = !b.3.is_empty();
        pic = pic.child(div().absolute().left(T::SP_6 + T::MINDMAP_DIA_COL * b.0 as f32).top(T::SP_6 + T::MINDMAP_DIA_ROW * b.1 as f32)
            .w(T::MINDMAP_DIA_BOX_W).h(T::MINDMAP_DIA_BOX_H).rounded(T::R_CONTROL).flex().items_center().justify_center()
            .bg(if b.3 == "new" { T::accent_dim() } else { T::bg_card() }).border_1().border_color(if lit { T::accent() } else { T::border() })
            .when(b.3 == "held", |d| d.border_dashed())
            .text_size(T::TEXT_10).font_weight(T::W_SEMIBOLD).text_color(if lit { T::accent_hover() } else { T::text_secondary() }).child(b.2));
    }
    div().ml(T::MINDMAP_HELP_KEYS + T::SP_10).mb(T::SP_10).p(T::SP_10).flex().flex_col().gap(T::SP_8).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg())
        .child(pic)
        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(d.note))
        .into_any_element()
}

/// The shortcut sheet (#mm-help-modal): every way of working the map, by
/// section; a row with a picture opens to it.
fn help_sheet(s: &M) -> Option<AnyElement> {
    let u = s.ui.as_ref()?;
    let head = div().flex().items_center().gap(T::SP_8).px(T::SP_16).pt(T::SP_12 + T::SP_2).pb(T::SP_10).border_b_1().border_color(T::border())
        .child(div().flex_1().min_w_0().text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Mindmap shortcuts"))
        .child(w::button("mmh-all", if s.h_all { "Hide all pictures" } else { "Show all pictures" }).on_click(|_, window, cx| super::h_act("all", window, cx)))
        .child(w::icon_button("mmh-close", IconName::X, "Close").on_click(|_, window, cx| super::h_act("close", window, cx)));
    let mut body = div().flex().flex_col().px(T::SP_16).pt(T::SP_4).pb(T::SP_16);
    for (i, r) in help::rows().into_iter().enumerate() {
        match r {
            help::Row::Head(h) => body = body.child(div().pt(T::SP_16).pb(T::SP_4).text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::text_muted()).child(h)),
            help::Row::Item { keys, text, dia } => {
                let open = s.h_open.get(i).copied().unwrap_or(false);
                let mut kc = div().flex_none().w(T::MINDMAP_HELP_KEYS).flex().flex_col().gap(T::SP_4);
                for line in keys {
                    let mut l = div().flex().flex_wrap().items_center().gap(T::SP_2);
                    for (k, t) in line {
                        l = l.child(match k {
                            "kbd" => w::kbd(t).into_any_element(),
                            "word" => div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::accent_hover()).child(t).into_any_element(),
                            _ => div().text_size(T::TEXT_11).text_color(T::text_muted()).child(t).into_any_element(),
                        });
                    }
                    kc = kc.child(l);
                }
                let has = dia.is_some();
                let row = div().id(("mmh-row", i)).flex().flex_col().border_b_1().border_color(T::border_subtle())
                    .child(div().flex().items_center().gap(T::SP_10).py(T::SP_8).px(T::SP_2)
                        .child(kc)
                        .child(div().id(("mmd-text", i)).flex_1().min_w_0().text_size(T::TEXT_13).text_color(T::text_secondary()).when(has, |d| d.hover(|d| d.text_color(T::text()))).child(text))
                        .when(has, |d| d.child(div().text_color(if open { T::accent_hover() } else { T::text_muted() })
                            .child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_MD)))))
                    .when_some(dia.filter(|_| open), |d, dg| d.child(diagram(&dg)))
                    .when(has, |d| d.cursor_pointer().on_click(move |_, _, cx| { super::h_toggle(i, cx); super::redraw(cx); }));
                body = body.child(row);
            }
        }
    }
    Some(div().track_focus(&u.h_focus).flex().flex_col().child(head).child(body).into_any_element())
}

// ─── The dialogs (in the one modal layer) ───

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Dlg { Detail, Link, Help }

/// A dialog's body: a view of its own (it redraws as the map's state does
/// while it is open; it is open only now and then).
pub struct DlgView { kind: Dlg, store: Entity<M>, _sub: Subscription }

impl Render for DlgView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.store.read(cx);
        let body = match self.kind { Dlg::Detail => detail(s), Dlg::Link => link(s), Dlg::Help => help_sheet(s) };
        div().children(body)
    }
}

/// Open a dialog over the window; Esc closes it (as its Close / Cancel).
pub(crate) fn open_dialog(kind: Dlg, window: &mut Window, cx: &mut App) {
    let store = super::store(cx);
    let v = cx.new(|cx| DlgView { kind, store: store.clone(), _sub: cx.observe(&store, |_, _, cx| cx.notify()) });
    let width = if kind == Dlg::Help { T::DIALOG_W_XL } else { T::MINDMAP_DIALOG_W };
    w::modal(window, cx, width, v.into(), move |window, cx| match kind {
        Dlg::Detail => super::d_act("close", window, cx),
        Dlg::Link => super::l_act("cancel", "", "", window, cx),
        Dlg::Help => super::h_act("close", window, cx),
    });
}

// ─── The host ───

/// One place the map shows (the tab, or a pane): its views. Cheap to clone.
#[derive(Clone)]
pub struct Host { msg: Entity<Msg>, toolbar: Entity<Toolbar>, dots: Entity<Dots>, map: Entity<MapView> }

impl Host {
    /// `embedded`: a workbench pane (its head has the "?"; the toolbar's
    /// Shortcuts is not shown there, the web's `.wb-cell-body .mm-help-btn`).
    pub fn new(embedded: bool, _window: &mut Window, cx: &mut App) -> Self {
        let store = super::store(cx);
        let msg = cx.new(|cx| Msg::new(store.clone(), cx));
        let toolbar = cx.new(|cx| Toolbar::new(store.clone(), embedded, cx));
        let dots = cx.new(|_| Dots);
        let map = cx.new(|cx| MapView::new(store.clone(), cx));
        Host { msg, toolbar, dots, map }
    }

    /// The host's body: layout only, placing the cached views.
    pub fn body(&self, cx: &mut App) -> AnyElement {
        // The state from the Msg view's copy (its observer notifies only when
        // it changed), never the store: a store notify costs this layout view
        // nothing (F57).
        let state = self.msg.read(cx).shape.0.clone();
        let mut root = div().relative().size_full().flex().flex_col().min_h_0().bg(T::bg());
        match state.as_str() {
            "loading" | "error" => return root.child(self.msg.clone().cached(StyleRefinement::default().size_full())).into_any_element(),
            _ => {}
        }
        let th = self.toolbar.read(cx).height;
        root = root.child(self.toolbar.clone().cached(StyleRefinement::default().w_full().h(th).flex_none()));
        if state == "empty" {
            return root.child(self.msg.clone().cached(StyleRefinement::default().w_full().flex_1().min_h_0())).into_any_element();
        }
        // (Placed at no size: its notify is the host's re-layout.)
        root.child(self.msg.clone().cached(StyleRefinement::default().absolute().top_0().left_0().size_0()))
            .child(div().relative().flex_1().min_h_0().overflow_hidden()
                .child(self.dots.clone().cached(StyleRefinement::default().absolute().top_0().left_0().size_full()))
                .child(self.map.clone().cached(StyleRefinement::default().absolute().top_0().left_0().size_full())))
            .into_any_element()
    }
}

/// The Mindmap tab's view: layout only.
pub struct Tab(pub Host);
impl Render for Tab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement { div().id("mindmap-tab").size_full().child(self.0.body(cx)) }
}

/// Loading, an error, no ideas yet; its notify re-lays the host out when
/// the map's state changes.
pub struct Msg { store: Entity<M>, shape: (String, String), _sub: Subscription }

impl Msg {
    fn new(store: Entity<M>, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&store, |this: &mut Self, s, cx| { let x = { let m = s.read(cx); (m.state.clone(), m.error.clone()) }; if x != this.shape { this.shape = x; cx.notify(); } });
        let shape = { let m = store.read(cx); (m.state.clone(), m.error.clone()) };
        Msg { store, shape, _sub: sub }
    }
}

impl Render for Msg {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        rendered("mindmap-msg");
        let _ = &self.store;
        match self.shape.0.as_str() {
            // Loading: nothing yet (the web's panel is empty until the map comes).
            "error" => w::empty(self.shape.1.clone()).into_any_element(),
            "empty" => w::empty("No ideas yet. Add one, or hit \u{201c}Brainstorm\u{201d} on a Kanban card to extend a task here.").into_any_element(),
            _ => div().into_any_element(),
        }
    }
}

// ─── The toolbar and the project bar ───

#[derive(Clone, PartialEq, Default)]
struct Bar { bar: bool, chips: Vec<(String, String, bool)>, hidden: usize }

pub struct Toolbar { store: Entity<M>, bar: Bar, fold: bool, embedded: bool, pub height: Pixels, _sub: Subscription }

impl Toolbar {
    fn new(store: Entity<M>, embedded: bool, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&store, |this: &mut Self, s, cx| { let b = bar_of(s.read(cx)); if b != this.bar { this.bar = b; cx.notify(); } });
        let bar = bar_of(store.read(cx));
        Toolbar { store, bar, fold: false, embedded, height: T::BTN_H + T::SP_6 * 2. + px(1.), _sub: sub }
    }
}

fn bar_of(s: &M) -> Bar { Bar { bar: s.bar, chips: s.chips.iter().map(|c| (c.key.clone(), c.label.clone(), c.off)).collect(), hidden: s.hidden_count } }

impl Render for Toolbar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("mindmap-toolbar");
        let fold = self.fold;
        let s = self.store.read(cx);
        let search = s.ui.as_ref().map(|u| u.search.clone());
        let mut col = div().w_full().flex().flex_col()
            // A press here gives the map the keys (the web's: the map was the last thing pressed),
            // unless it went to the filter box.
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                let st = super::store(cx);
                let (search, canvas) = { let m = st.read(cx); (m.ui.as_ref().map(|u| u.search.clone()), m.ui.as_ref().map(|u| u.canvas.clone())) };
                if !search.is_some_and(|e| e.focus_handle(cx).is_focused(window)) { if let Some(c) = canvas { c.focus(window, cx); } }
            })
            // Its buttons and chips in paint layers (w::layer: the bar is
            // replayed every frame); the filter box is not (its caret).
            .child(w::toolbar()
                .child(w::layer(w::primary("mm-add", "+ Add idea").on_click(|_, window, cx| super::act("add-root", "", window, cx))))
                .child(w::layer(w::button("mm-media", if fold { "" } else { "Media" }).icon(IconName::Images).on_click(|_, window, cx| super::act("media", "", window, cx))))
                .child(div().w(T::SP_48 * 3. + T::SP_16).children(search.as_ref().map(w::input)))
                .child(div().flex_1())
                .when(!self.embedded, |d| d.child(w::layer(w::button("mm-help", if fold { "?" } else { "? Shortcuts" }).tooltip("Mindmap shortcuts").on_click(|_, window, cx| super::act("help", "", window, cx))))));
        // The project bar (.mm-proj-bar): its chips wrap, as the web's.
        if self.bar.bar && s.state != "empty" {
            let mut bar = div().flex().flex_none().flex_wrap().items_center().gap(T::SP_6).px(T::SP_12).py(T::SP_8).bg(T::bg_secondary()).border_b_1().border_color(T::border())
                .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Projects:"));
            for (i, (key, label, off)) in self.bar.chips.iter().enumerate() {
                let k = key.clone();
                bar = bar.child(w::filter_chip(("mm-proj", i), label.clone(), *off, false).on_click(move |_, window, cx| super::act("chip", &k, window, cx)));
            }
            if self.bar.hidden > 0 {
                bar = bar.child(div().flex().items_center().gap(T::SP_4).pl(T::SP_4).text_size(T::TEXT_12).text_color(T::amber())
                        .child(w::icon(IconName::Funnel, T::ICON_SM)).child(format!("filtered ({} hidden)", self.bar.hidden)))
                    .child(w::filter_chip("mm-proj-all", "Show all", false, true).on_click(|_, window, cx| super::act("chip-all", "", window, cx)));
            }
            col = col.child(w::layer(bar));
        }
        // Cached at the height it laid out at; the width folds it (the web's @container 480px).
        let me = cx.entity().downgrade();
        col.relative().child(canvas(move |b, _, cx| {
            let (h, fold) = (b.size.height, b.size.width < T::MINDMAP_FOLD);
            if let Some(me) = me.upgrade() {
                let t = me.read(cx);
                if (t.height - h).abs() > px(0.5) || t.fold != fold { cx.defer(move |cx| me.update(cx, |t, cx| { t.height = h; t.fold = fold; cx.notify(); })); }
            }
        }, |_, _, _, _| {}).absolute().top_0().left_0().size_full())
    }
}

// ─── The canvas ───

/// The canvas's dotted ground (.mindmap-canvas's background): it stays put
/// while the map pans, so it is painted once and replayed.
pub struct Dots;
impl Render for Dots {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        rendered("mindmap-dots");
        canvas(|_, _, _| {}, |b, _, window, _| window.paint_layer(b, |window| {
            let step = f32::from(T::MINDMAP_DOTS);
            let (w_, h_) = (f32::from(b.size.width), f32::from(b.size.height));
            let mut y = 0.;
            while y < h_ {
                let mut x = 0.;
                while x < w_ { window.paint_quad(fill(Bounds::new(point(b.origin.x + px(x), b.origin.y + px(y)), size(T::SP_2, T::SP_2)), T::border_subtle()).corner_radii(px(1.))); x += step; }
                y += step;
            }
        })).size_full()
    }
}

/// The map: the ideas and links that cross the view, panned by its `Pan`.
pub struct MapView {
    store: Entity<M>, pan: Entity<Pan>, edges: Option<Edges>, hot: Option<Hot>, pressed: Option<Hot>,
    /// The ideas at rest, laid out (idea_el.rs), and the ghost's drawing per zoom.
    ideas: super::idea_el::Cache,
    ghost: Option<(u32, Rc<crate::paint::Draw>)>,
    /// The pan and zoom as last drawn.
    last_pan: Option<(f32, f32, f32)>,
    _subs: Vec<Subscription>,
}

/// What the pointer is on: an idea, its fold button, an "add idea" box (its
/// row, its idea).
#[derive(Clone, PartialEq, Debug)]
enum Hot { Node(String), Fold(String), Ghost(String, String) }

impl MapView {
    /// What is under a window point (the map's own hit test: the ideas at
    /// rest have no handlers). The last drawn is on top.
    fn hit(&self, at: Point<Pixels>, cx: &App) -> Option<Hot> {
        let s = self.store.read(cx);
        let p = self.pan.read(cx);
        let z = s.zoom.max(0.01);
        let (ux, uy) = ((f32::from(at.x) - s.canvas_x + p.x) / z, (f32::from(at.y) - s.canvas_y + p.y) / z);
        if f32::from(at.x) < s.canvas_x || f32::from(at.y) < s.canvas_y || f32::from(at.x) > s.canvas_x + s.view_w || f32::from(at.y) > s.canvas_y + s.view_h { return None; }
        let nw = f32::from(T::MINDMAP_NODE);
        for r in s.rows.iter().rev() {
            match r.kind {
                "node" => {
                    let top = (r.y - r.h / 2.).max(f32::from(T::SP_4));
                    if r.kids {
                        let (fx, fs, cy) = (r.x + nw - f32::from(T::SP_10), f32::from(T::SP_16 + T::SP_2), top + r.h / 2.);
                        if ux >= fx && ux <= fx + fs && uy >= cy - fs / 2. && uy <= cy + fs / 2. { return Some(Hot::Fold(r.id.clone())); }
                    }
                    if ux >= r.x && ux <= r.x + nw && uy >= top && uy <= top + r.h { return Some(Hot::Node(r.id.clone())); }
                }
                "ghost" if !r.hidden => {
                    let (top, h) = (r.y - r.h / 2., r.h.max(f32::from(T::SP_40)));
                    if ux >= r.x && ux <= r.x + nw && uy >= top && uy <= top + h { return Some(Hot::Ghost(r.id.clone(), r.parent.clone())); }
                }
                _ => {}
            }
        }
        None
    }

    fn hover_at(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let h = self.hit(at, cx);
        self.set_hot(h, window, cx);
    }

    /// The pointer onto another part: drawn hovered; an idea whose words are
    /// cut opens up over itself (as its hover did).
    fn set_hot(&mut self, h: Option<Hot>, window: &mut Window, cx: &mut Context<Self>) {
        if h == self.hot { return; }
        let idea = |h: &Option<Hot>| match h { Some(Hot::Node(i)) | Some(Hot::Fold(i)) => Some(i.clone()), _ => None };
        let (was, now) = (idea(&self.hot), idea(&h));
        self.hot = h;
        cx.notify();
        if was == now { return; }
        let rows: Vec<Row> = { let s = self.store.read(cx); s.rows.iter().filter(|r| r.kind == "node" && (Some(&r.id) == was.as_ref() || Some(&r.id) == now.as_ref())).cloned().collect() };
        let z = self.store.read(cx).zoom;
        if let Some(r) = was.and_then(|w| rows.iter().find(|r| r.id == w)) {
            // The idea opened up decides when it closes (it lies over this one).
            if super::store(cx).read(cx).hover.as_ref().is_none_or(|h| h.id != r.id) { super::hover(r, false, false, cx); }
        }
        if let Some(r) = now.and_then(|n| rows.iter().find(|r| r.id == n)) { let c = clamped(&r.text, z, window); super::hover(r, true, c, cx); }
    }
}

/// The links as paths at the map's origin, for one zoom, by their end points.
struct Edges { zoom: u32, paths: HashMap<([u32; 4], bool), Path<Pixels>> }

impl MapView {
    fn new(store: Entity<M>, cx: &mut Context<Self>) -> Self {
        let pan = store.read(cx).pan();
        let subs = vec![cx.observe(&store, |_, _, cx| cx.notify()), cx.observe(&pan, |_, _, cx| cx.notify())];
        MapView { store, pan, edges: None, hot: None, pressed: None, ideas: Rc::default(), ghost: None, last_pan: None, _subs: subs }
    }
}

impl Render for MapView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        rendered("mindmap-map");
        let store = self.store.clone();
        let s = store.read(cx);
        let p = self.pan.read(cx);
        let (z, panx, pany) = (s.zoom, p.x, p.y);
        let Some(u) = s.ui.as_ref() else { return div().into_any_element() };
        let (cw, ch) = (s.view_w.max(s.base_w * z), s.view_h.max(s.base_h * z));
        // The view, in the map's own (unscaled) units, with a margin.
        let (vx0, vy0) = (panx / z - f32::from(T::MINDMAP_NODE), pany / z - f32::from(T::SP_48) * 4.);
        let (vx1, vy1) = ((panx + s.view_w.max(1.)) / z + f32::from(T::SP_24), (pany + s.view_h.max(1.)) / z + f32::from(T::SP_48) * 4.);
        let seen = |x: f32, y: f32, h: f32| x + f32::from(T::MINDMAP_NODE) >= vx0 && x <= vx1 && y + h / 2. >= vy0 && y - h / 2. <= vy1;
        // The links in view: each tessellated once (at the map's origin, per
        // zoom, kept by its end points: a new layout reuses the links that
        // did not move), then only moved as the map pans.
        let crosses = |c: &[f32; 4]| c[0].max(c[2]) >= vx0 && c[0].min(c[2]) <= vx1 && c[1].max(c[3]) >= vy0 && c[1].min(c[3]) <= vy1;
        if self.edges.as_ref().is_none_or(|e| e.zoom != z.to_bits() || e.paths.len() > 4000) { self.edges = Some(Edges { zoom: z.to_bits(), paths: HashMap::new() }); }
        let cache = &mut self.edges.as_mut().expect("edges").paths;
        let mut take = |list: &[[f32; 4]], dashed: bool| -> Vec<Path<Pixels>> {
            list.iter().filter(|c| crosses(c)).filter_map(|c| {
                let k = (c.map(f32::to_bits), dashed);
                if !cache.contains_key(&k) {
                    let mut pb = PathBuilder::stroke(T::SP_2 * z);
                    if dashed { let d = (T::SP_4 + px(1.)) * z; pb = pb.dash_array(&[d, d]); }
                    curves(&mut pb, std::slice::from_ref(c), point(px(0.), px(0.)), z);
                    cache.insert(k, pb.build().ok()?);
                }
                cache.get(&k).cloned()
            }).collect()
        };
        let solid = take(&s.lay.curves, false);
        let dashed = take(&s.lay.dash_curves, true);
        let edges = canvas(|_, _, _| {}, move |b, _, window, _| {
            let mut paint = |list: Vec<Path<Pixels>>, color: Hsla| for mut p in list {
                for v in p.vertices.iter_mut() { v.xy_position = v.xy_position + b.origin; }
                p.bounds.origin = p.bounds.origin + b.origin;
                window.paint_path(p, color);
            };
            paint(solid, T::border());
            paint(dashed, T::border().opacity(T::MINDMAP_O_DIM));
        }).absolute().top_0().left_0().w(px(s.base_w * z)).h(px(s.base_h * z));
        let mut content = div().id("mm-content").absolute().left(px(-panx)).top(px(-pany)).w(px(cw)).h(px(ch)).child(edges);
        // What the pointer is on, as this frame places the map (a pan moves
        // the ideas under a still pointer).
        // While it pans, nothing is drawn hovered (the pointer's next move
        // hovers what is under it; a press hits through `hit` regardless).
        let pan_now = (panx, pany, z);
        if self.last_pan != Some(pan_now) { if self.last_pan.is_some() { self.hot = None; } self.last_pan = Some(pan_now); }
        let s = store.read(cx);
        let mut drawn = vec![];
        // The ideas and ghosts at rest: one element paints them from their
        // drawings (idea_el.rs). The one under the pointer, and the ones that
        // must stay divs (a glow, a cut chip), are divs over it.
        let mut rest: Vec<super::idea_el::Item> = vec![];
        let mut divs: Vec<AnyElement> = vec![];
        let ghost_draw = match &self.ghost { Some((zb, d)) if *zb == z.to_bits() => d.clone(), _ => { let d = Rc::new(super::idea_el::lay_ghost(z, window, cx)); self.ghost = Some((z.to_bits(), d.clone())); d } };
        for r in &s.rows {
            if !seen(r.x, r.y, r.h) { continue; }
            match r.kind {
                "node" => {
                    drawn.push(r.id.clone());
                    let (h, f) = match &self.hot { Some(Hot::Node(i)) if *i == r.id => (true, false), Some(Hot::Fold(i)) if *i == r.id => (true, true), _ => (false, false) };
                    if !h && !s.all_div {
                        let k = super::idea_el::key(r, z);
                        let cached = self.ideas.borrow().get(&r.id).filter(|(kk, _)| *kk == k).map(|(_, d)| d.clone());
                        let d = match cached { Some(d) => d, None => { let d = super::idea_el::lay_idea(r, z, window, cx).map(Rc::new); self.ideas.borrow_mut().insert(r.id.clone(), (k, d.clone())); d } };
                        if let Some(d) = d {
                            let top = crate::paint::r(px((r.y * z - r.h * z / 2.).max(f32::from(T::SP_4) * z)));
                            rest.push(super::idea_el::Item { id: r.id.clone(), left: crate::paint::r(px(r.x * z)), top, draw: d });
                            continue;
                        }
                    }
                    divs.push(card(r, Mode::Normal, z, None, h, f))
                }
                "ghost" if !r.hidden => {
                    let h = matches!(&self.hot, Some(Hot::Ghost(i, _)) if *i == r.id);
                    if !h && !s.all_div { rest.push(super::idea_el::Item { id: r.id.clone(), left: crate::paint::r(px(r.x * z)), top: crate::paint::r(px(r.y * z - r.h * z / 2.)), draw: ghost_draw.clone() }); }
                    else { divs.push(ghost(r, z, h)); }
                }
                "drop" => divs.push(drop_box(r, z)),
                _ => {}
            }
        }
        content = content.child(super::idea_el::MapEl { items: rest, z }).children(divs);
        *s.drawn.borrow_mut() = drawn;
        if s.ring_on { content = content.child(ring(s, z)); }
        if let Some(h) = &s.hover { if !(s.edit_on && s.edit_node.as_ref().is_some_and(|e| e.id == h.id)) { content = content.child(card(h, Mode::Open, z, None, true, false)); } }
        if s.edit_on { if let Some(e) = &s.edit_node { content = content.child(card(e, Mode::Editing, z, Some(&u.edit), true, false)); } }
        if s.draft_on { content = content.child(draft(s, z, &u.draft)); }
        let clone = (!s.drag_id.is_empty()).then(|| clone_box(s, z));
        let lh = window.line_height();
        let held = s.press.is_some();
        // The canvas's scrollbars (the web's .mindmap-canvas overflow: auto),
        // over the map in their own layer (a thumb lies over the ideas).
        let (vw, vh) = (s.view_w, s.view_h);
        let canvas_fh = u.canvas.clone();
        let both = cw > vw + 1. && ch > vh + 1.;
        let corner = if both { T::SP_6 } else { px(0.) };
        let vbar = w::scrollbar("mm-vbar", true, vh, ch, pany, corner, window, cx, move |y, _, cx| { let x = super::store(cx).read(cx).pan().read(cx).x; super::set_scroll(x, y, cx); });
        let hbar = w::scrollbar("mm-hbar", false, vw, cw, panx, corner, window, cx, move |x, _, cx| { let y = super::store(cx).read(cx).pan().read(cx).y; super::set_scroll(x, y, cx); });
        // The keys of the map (the web's document keydown while the map was
        // the last thing pressed): a press anywhere in it gives it the keys.
        div().id("mindmap").track_focus(&canvas_fh).relative().size_full().overflow_hidden()
            .key_context(super::MAP_CONTEXT)
            .on_action(|_: &super::MapTab, window, cx| { super::key(&Key { key: "tab", ctrl: false, shift: false, alt: false }, window, cx); })
            .on_action(|_: &super::MapBackTab, window, cx| { super::key(&Key { key: "tab", ctrl: false, shift: true, alt: false }, window, cx); })
            .on_key_down(|e: &KeyDownEvent, window, cx| if super::key(&key_of(&e.keystroke), window, cx) { cx.stop_propagation(); })
            .when(self.hot.is_some(), |d| d.cursor_pointer())
            // The ideas, their fold buttons and the "add idea" boxes are hit
            // here (they are drawn without handlers: a pan frame rebuilds
            // every idea on screen). The boxes open over the map (the idea
            // opened up, the one being edited, the draft) take their own.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                this.hover_at(e.position, window, cx);
                // (A held press's moves are taken window-wide: `held` below.)
                if !super::pressing(cx) { super::pointer_move(f32::from(e.position.x), f32::from(e.position.y), window, cx) }
            }))
            // A press anywhere else lets go of the ring (the web's document pointerdown).
            .on_mouse_down_out(|_, _, cx| super::press_outside(cx))
            .on_hover(cx.listener(|this, on: &bool, window, cx| if !*on { this.set_hot(None, window, cx); }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                let h = this.hit(e.position, cx);
                this.pressed = h.clone();
                match h {
                    Some(Hot::Node(id)) => super::node_down(&id, f32::from(e.position.x), f32::from(e.position.y), cx),
                    Some(_) => {}
                    // The bare canvas: a press lets go of the ring and closes a box.
                    None => super::canvas_press(window, cx),
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, window, cx| {
                let h = this.hit(e.position, cx);
                let pressed = this.pressed.take();
                match h {
                    Some(Hot::Node(id)) => super::node_up(&id, e.click_count, window, cx),
                    Some(Hot::Fold(id)) if pressed == Some(Hot::Fold(id.clone())) => { super::act("toggle", &id, window, cx); super::pointer_up(window, cx) }
                    Some(Hot::Ghost(g, parent)) if pressed == Some(Hot::Ghost(g.clone(), parent.clone())) => { super::act("ghost", &parent, window, cx); super::pointer_up(window, cx) }
                    _ => super::pointer_up(window, cx),
                }
            }))
            // The wheel scrolls the map both ways (Shift: sideways); Ctrl and the wheel zoom about the pointer.
            .on_scroll(move |e: &ScrollEvent, window, cx| {
                let d = e.delta.pixel_delta(lh);
                let st = super::store(cx);
                let (ox, oy) = { let m = st.read(cx); (m.canvas_x, m.canvas_y) };
                if e.modifiers.control { super::wheel_zoom(f32::from(d.y), f32::from(e.position.x) - ox, f32::from(e.position.y) - oy, window, cx); }
                else {
                    let (dx, dy) = if e.modifiers.shift && d.x == px(0.) { (d.y, px(0.)) } else { (d.x, d.y) };
                    super::scroll_by(f32::from(dx), f32::from(dy), cx);
                }
                cx.stop_propagation();
            })
            // A touchpad's pinch zooms about its centre, as Ctrl and the wheel do.
            .on_pinch(|e: &PinchEvent, window, cx| {
                let st = super::store(cx);
                let (ox, oy) = { let m = st.read(cx); (m.canvas_x, m.canvas_y) };
                super::pinch_zoom(e.delta, f32::from(e.position.x) - ox, f32::from(e.position.y) - oy, window, cx);
                cx.stop_propagation();
            })
            .child(canvas(|b, _, cx| super::canvas_measured(b, cx), move |b, _, window, _| {
                // While a press on an idea is held, its moves and its release
                // are taken window-wide: a drag may leave the map, and a
                // release outside it ends the drag (nothing moves there).
                if !held { return; }
                window.on_mouse_event(|e: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble && super::pressing(cx) { super::pointer_move(f32::from(e.position.x), f32::from(e.position.y), window, cx); }
                });
                window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble && e.button == MouseButton::Left && !b.contains(&e.position) && super::pressing(cx) { super::release_outside(window, cx); }
                });
            }).absolute().top_0().left_0().size_full())
            .child(w::layer(content))
            .children(clone)
            .when(vbar.is_some() || hbar.is_some(), |d| d.child(w::layer(div().absolute().top_0().left_0().size_full().children(vbar).children(hbar))))
            .into_any_element()
    }
}
